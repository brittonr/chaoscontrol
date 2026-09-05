//! Embedded HTTP server for the live dashboard.
//!
//! Only compiled when the `dashboard` feature is enabled.

use axum::response::IntoResponse;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const CHART_JS: &str = include_str!("../assets/chart.umd.min.js");
const ANNOTATION_JS: &str = include_str!("../assets/chartjs-plugin-annotation.min.js");

/// Shared state between the axum handlers and the event receiver.
struct AppState {
    state: std::sync::RwLock<crate::dashboard_types::DashboardState>,
    tx: ::tokio::sync::broadcast::Sender<crate::dashboard_types::DashboardEvent>,
}

/// Default dashboard bind address: loopback only.
///
/// The dashboard has no authentication. Binding to anything beyond
/// loopback exposes run state, kernel paths, and bug details to the
/// network. Pass an explicit host to opt into a wider bind.
pub const DEFAULT_DASHBOARD_HOST: &str = "127.0.0.1";

/// Resolve a dashboard host string into an IP address.
///
/// Pure core: no I/O. Returns `None` for unparseable hosts.
fn parse_dashboard_host(host: &str) -> Option<std::net::IpAddr> {
    host.parse::<std::net::IpAddr>().ok()
}

/// Start the dashboard in live mode.
///
/// Spawns a background thread running a tokio runtime with the HTTP
/// server. Returns a `SyncSender` that the explorer uses to push
/// events, or `None` if the host is invalid or the server failed to bind.
pub fn start(
    host: &str,
    port: u16,
) -> Option<::std::sync::mpsc::SyncSender<crate::dashboard_types::DashboardEvent>> {
    let (event_tx, event_rx) =
        std::sync::mpsc::sync_channel::<crate::dashboard_types::DashboardEvent>(64);
    let (broadcast_tx, _) =
        ::tokio::sync::broadcast::channel::<crate::dashboard_types::DashboardEvent>(256);

    let app_state = std::sync::Arc::new(AppState {
        state: std::sync::RwLock::new(crate::dashboard_types::DashboardState::empty()),
        tx: broadcast_tx.clone(),
    });

    let app_state_clone = std::sync::Arc::clone(&app_state);

    let ip = match parse_dashboard_host(host) {
        Some(ip) => ip,
        None => {
            ::log::warn!(
                "Dashboard: invalid host '{}' (expected an IP address)",
                host
            );
            return None;
        }
    };
    let addr = std::net::SocketAddr::from((ip, port));
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(l) => {
            l.set_nonblocking(true).ok();
            l
        }
        Err(e) => {
            ::log::warn!("Dashboard: failed to bind {}: {}", addr, e);
            return None;
        }
    };

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("dashboard tokio runtime");

        rt.block_on(async move {
            let state_for_receiver = std::sync::Arc::clone(&app_state_clone);
            let broadcast_for_receiver = broadcast_tx;

            tokio::spawn(async move {
                event_receiver_loop(event_rx, state_for_receiver, broadcast_for_receiver).await;
            });

            let app = build_router(app_state_clone);

            let tokio_listener =
                tokio::net::TcpListener::from_std(listener).expect("tokio listener from std");

            ::log::info!("Dashboard server listening on http://{}", addr);

            axum::serve(tokio_listener, app)
                .await
                .expect("dashboard server");
        });
    });

    Some(event_tx)
}

/// Start in standalone mode — blocks the calling thread.
pub fn start_standalone(
    state: crate::dashboard_types::DashboardState,
    host: &str,
    port: u16,
) -> Result<(), String> {
    let ip = parse_dashboard_host(host)
        .ok_or_else(|| format!("invalid host '{}' (expected an IP address)", host))?;
    let (broadcast_tx, _) =
        ::tokio::sync::broadcast::channel::<crate::dashboard_types::DashboardEvent>(16);
    let app_state = std::sync::Arc::new(AppState {
        state: std::sync::RwLock::new(state),
        tx: broadcast_tx,
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {}", e))?;

    rt.block_on(async move {
        let app = build_router(app_state);
        let addr = std::net::SocketAddr::from((ip, port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("bind {}: {}", addr, e))?;

        ::log::info!("Dashboard server listening on http://{}", addr);
        eprintln!("Dashboard: http://{}", addr);

        axum::serve(listener, app)
            .await
            .map_err(|e| format!("server: {}", e))
    })
}

fn build_router(state: std::sync::Arc<AppState>) -> ::axum::Router {
    ::axum::Router::new()
        .route("/", ::axum::routing::get(index_handler))
        .route(
            "/assets/chart.umd.min.js",
            ::axum::routing::get(chart_js_handler),
        )
        .route(
            "/assets/chartjs-plugin-annotation.min.js",
            ::axum::routing::get(annotation_js_handler),
        )
        .route("/api/state", ::axum::routing::get(state_handler))
        .route("/api/events", ::axum::routing::get(sse_handler))
        .route("/api/bugs", ::axum::routing::get(bugs_handler))
        .route("/api/bugs/{id}", ::axum::routing::get(bug_detail_handler))
        .with_state(state)
}

async fn index_handler() -> ::axum::response::Html<&'static str> {
    ::axum::response::Html(INDEX_HTML)
}

async fn chart_js_handler() -> impl IntoResponse {
    (
        ::axum::http::StatusCode::OK,
        [(::axum::http::header::CONTENT_TYPE, "application/javascript")],
        CHART_JS,
    )
}

async fn annotation_js_handler() -> impl IntoResponse {
    (
        ::axum::http::StatusCode::OK,
        [(::axum::http::header::CONTENT_TYPE, "application/javascript")],
        ANNOTATION_JS,
    )
}

async fn state_handler(
    ::axum::extract::State(state): ::axum::extract::State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    let json = serde_json::to_string(&*data).unwrap_or_else(|_| "{}".to_string());
    (
        ::axum::http::StatusCode::OK,
        [(::axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
}

async fn sse_handler(
    ::axum::extract::State(state): ::axum::extract::State<std::sync::Arc<AppState>>,
) -> ::axum::response::sse::Sse<
    impl futures_core::Stream<Item = Result<::axum::response::sse::Event, ::std::convert::Infallible>>,
> {
    let rx = state.tx.subscribe();

    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let event_type = match &event {
                            crate::dashboard_types::DashboardEvent::Started { .. } => "started",
                            crate::dashboard_types::DashboardEvent::RoundComplete { .. } => "round",
                            crate::dashboard_types::DashboardEvent::BugFound { .. } => "bug",
                            crate::dashboard_types::DashboardEvent::Finished { .. } => "finished",
                            crate::dashboard_types::DashboardEvent::CampaignStarted { .. } => "campaign_started",
                            crate::dashboard_types::DashboardEvent::SeedComplete { .. } => "seed_complete",
                            crate::dashboard_types::DashboardEvent::CampaignFinished { .. } => "campaign_finished",
                        };
                        yield Ok(::axum::response::sse::Event::default().event(event_type).data(json));
                    }
                }
                Err(::tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::debug!("SSE client lagged {} events", n);
                    continue;
                }
                Err(::tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    ::axum::response::sse::Sse::new(stream).keep_alive(::axum::response::sse::KeepAlive::default())
}

async fn bugs_handler(
    ::axum::extract::State(state): ::axum::extract::State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    let json = serde_json::to_string(&data.bugs).unwrap_or_else(|_| "[]".to_string());
    (
        ::axum::http::StatusCode::OK,
        [(::axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
}

async fn bug_detail_handler(
    ::axum::extract::State(state): ::axum::extract::State<std::sync::Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<usize>,
) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    if let Some(bug) = data.bugs.get(id) {
        let json = serde_json::to_string(bug).unwrap_or_else(|_| "{}".to_string());
        (
            ::axum::http::StatusCode::OK,
            [(::axum::http::header::CONTENT_TYPE, "application/json")],
            json,
        )
    } else {
        (
            ::axum::http::StatusCode::NOT_FOUND,
            [(::axum::http::header::CONTENT_TYPE, "application/json")],
            r#"{"error":"bug not found"}"#.to_string(),
        )
    }
}

async fn event_receiver_loop(
    rx: ::std::sync::mpsc::Receiver<crate::dashboard_types::DashboardEvent>,
    state: std::sync::Arc<AppState>,
    broadcast_tx: ::tokio::sync::broadcast::Sender<crate::dashboard_types::DashboardEvent>,
) {
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = rx.recv() {
            {
                let mut data = state.state.write().unwrap();
                match &event {
                    crate::dashboard_types::DashboardEvent::Started {
                        num_vms,
                        seed,
                        branch_factor,
                        ticks_per_branch,
                        max_rounds,
                        mode,
                        kernel_path,
                        catalog_size,
                        ..
                    } => {
                        data.running = true;
                        data.config = crate::dashboard_types::DashboardConfig {
                            num_vms: *num_vms,
                            seed: *seed,
                            branch_factor: *branch_factor,
                            ticks_per_branch: *ticks_per_branch,
                            max_rounds: *max_rounds,
                            mode: mode.clone(),
                            kernel_path: kernel_path.clone(),
                        };
                        data.assertion_stats.catalog_size = *catalog_size;
                    }
                    crate::dashboard_types::DashboardEvent::RoundComplete {
                        round,
                        branches_run,
                        new_edges,
                        cumulative_edges,
                        bugs_found,
                        cumulative_bugs,
                        frontier_size,
                        corpus_size,
                        assertion_stats,
                        ..
                    } => {
                        data.apply_round_complete(
                            *round,
                            *branches_run,
                            *new_edges,
                            *cumulative_edges,
                            *bugs_found,
                            *cumulative_bugs,
                            *frontier_size,
                            *corpus_size,
                            assertion_stats,
                        );
                        data.total_branches += *branches_run as u64;
                    }
                    crate::dashboard_types::DashboardEvent::BugFound {
                        bug_index,
                        assertion_id,
                        assertion_message,
                        round,
                        tick,
                        schedule_length,
                        ..
                    } => {
                        data.apply_bug_found(crate::dashboard_types::DashboardBug {
                            bug_id: *bug_index as u64,
                            assertion_id: *assertion_id,
                            assertion_message: assertion_message.clone(),
                            round: *round,
                            tick: *tick,
                            schedule_length: *schedule_length,
                        });
                    }
                    crate::dashboard_types::DashboardEvent::Finished { ref reason, .. } => {
                        data.running = false;
                        data.finish_reason = reason.clone();
                    }
                    crate::dashboard_types::DashboardEvent::CampaignStarted {
                        seeds,
                        seeds_total,
                    } => {
                        data.running = true;
                        data.mode = "campaign".to_string();
                        data.seeds_total = *seeds_total;
                        let _ = seeds; // seeds list available on /api/state
                    }
                    crate::dashboard_types::DashboardEvent::SeedComplete { summary, .. } => {
                        data.seeds_completed += 1;
                        data.seed_summaries.push(summary.clone());
                    }
                    crate::dashboard_types::DashboardEvent::CampaignFinished { .. } => {
                        data.running = false;
                        data.finish_reason = "campaign_complete".to_string();
                    }
                }
            }
            let _ = broadcast_tx.send(event);
        }

        let mut data = state.state.write().unwrap();
        if data.running {
            data.running = false;
            if data.finish_reason.is_empty() {
                data.finish_reason = "channel_closed".to_string();
            }
        }
    })
    .await
    .ok();
}
