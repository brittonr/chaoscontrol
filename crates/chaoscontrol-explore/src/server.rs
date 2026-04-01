//! Embedded HTTP server for the live dashboard.
//!
//! Only compiled when the `dashboard` feature is enabled.

use crate::dashboard_types::{DashboardEvent, DashboardState};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use log::{info, warn};
use std::convert::Infallible;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const CHART_JS: &str = include_str!("../assets/chart.umd.min.js");
const ANNOTATION_JS: &str = include_str!("../assets/chartjs-plugin-annotation.min.js");

/// Shared state between the axum handlers and the event receiver.
struct AppState {
    state: RwLock<DashboardState>,
    tx: broadcast::Sender<DashboardEvent>,
}

/// Start the dashboard in live mode.
///
/// Spawns a background thread running a tokio runtime with the HTTP
/// server. Returns a `SyncSender` that the explorer uses to push
/// events, or `None` if the server failed to bind.
pub fn start(port: u16) -> Option<SyncSender<DashboardEvent>> {
    let (event_tx, event_rx) = std::sync::mpsc::sync_channel::<DashboardEvent>(64);
    let (broadcast_tx, _) = broadcast::channel::<DashboardEvent>(256);

    let app_state = Arc::new(AppState {
        state: RwLock::new(DashboardState::empty()),
        tx: broadcast_tx.clone(),
    });

    let app_state_clone = Arc::clone(&app_state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match std::net::TcpListener::bind(addr) {
        Ok(l) => {
            l.set_nonblocking(true).ok();
            l
        }
        Err(e) => {
            warn!("Dashboard: failed to bind port {}: {}", port, e);
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
            let state_for_receiver = Arc::clone(&app_state_clone);
            let broadcast_for_receiver = broadcast_tx;

            tokio::spawn(async move {
                event_receiver_loop(event_rx, state_for_receiver, broadcast_for_receiver).await;
            });

            let app = build_router(app_state_clone);

            let tokio_listener =
                tokio::net::TcpListener::from_std(listener).expect("tokio listener from std");

            info!("Dashboard server listening on http://0.0.0.0:{}", port);

            axum::serve(tokio_listener, app)
                .await
                .expect("dashboard server");
        });
    });

    Some(event_tx)
}

/// Start in standalone mode — blocks the calling thread.
pub fn start_standalone(state: DashboardState, port: u16) -> Result<(), String> {
    let (broadcast_tx, _) = broadcast::channel::<DashboardEvent>(16);
    let app_state = Arc::new(AppState {
        state: RwLock::new(state),
        tx: broadcast_tx,
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {}", e))?;

    rt.block_on(async move {
        let app = build_router(app_state);
        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("bind port {}: {}", port, e))?;

        info!("Dashboard server listening on http://0.0.0.0:{}", port);
        eprintln!("Dashboard: http://localhost:{}", port);

        axum::serve(listener, app)
            .await
            .map_err(|e| format!("server: {}", e))
    })
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/assets/chart.umd.min.js", get(chart_js_handler))
        .route(
            "/assets/chartjs-plugin-annotation.min.js",
            get(annotation_js_handler),
        )
        .route("/api/state", get(state_handler))
        .route("/api/events", get(sse_handler))
        .route("/api/bugs", get(bugs_handler))
        .route("/api/bugs/{id}", get(bug_detail_handler))
        .with_state(state)
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn chart_js_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript")],
        CHART_JS,
    )
}

async fn annotation_js_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript")],
        ANNOTATION_JS,
    )
}

async fn state_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    let json = serde_json::to_string(&*data).unwrap_or_else(|_| "{}".to_string());
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        json,
    )
}

async fn sse_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();

    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        let event_type = match &event {
                            DashboardEvent::Started { .. } => "started",
                            DashboardEvent::RoundComplete { .. } => "round",
                            DashboardEvent::BugFound { .. } => "bug",
                            DashboardEvent::Finished { .. } => "finished",
                        };
                        yield Ok(Event::default().event(event_type).data(json));
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    log::debug!("SSE client lagged {} events", n);
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn bugs_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    let json = serde_json::to_string(&data.bugs).unwrap_or_else(|_| "[]".to_string());
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        json,
    )
}

async fn bug_detail_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<usize>,
) -> impl IntoResponse {
    let data = state.state.read().unwrap();
    if let Some(bug) = data.bugs.get(id) {
        let json = serde_json::to_string(bug).unwrap_or_else(|_| "{}".to_string());
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            json,
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"bug not found"}"#.to_string(),
        )
    }
}

async fn event_receiver_loop(
    rx: Receiver<DashboardEvent>,
    state: Arc<AppState>,
    broadcast_tx: broadcast::Sender<DashboardEvent>,
) {
    tokio::task::spawn_blocking(move || {
        while let Ok(event) = rx.recv() {
            {
                let mut data = state.state.write().unwrap();
                match &event {
                    DashboardEvent::Started {
                        num_vms,
                        seed,
                        branch_factor,
                        ticks_per_branch,
                        max_rounds,
                        mode,
                        kernel_path,
                        catalog_size,
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
                    DashboardEvent::RoundComplete {
                        round,
                        branches_run,
                        new_edges,
                        cumulative_edges,
                        bugs_found,
                        cumulative_bugs,
                        frontier_size,
                        corpus_size,
                        assertion_stats,
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
                    DashboardEvent::BugFound {
                        bug_index,
                        assertion_id,
                        assertion_message,
                        round,
                        tick,
                        schedule_length,
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
                    DashboardEvent::Finished { reason, .. } => {
                        data.running = false;
                        data.finish_reason = reason.clone();
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
