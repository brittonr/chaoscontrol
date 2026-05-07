//! Multi-VM TCP networking demo for ChaosControl.
//!
//! Runs as PID 1 inside a deterministic VM.  Demonstrates inter-VM
//! communication over virtio-net using smoltcp for TCP/IP.
//!
//! - **VM 0**: TCP server — listens on port 8080, echoes received data
//! - **VM 1+**: TCP clients — connect to VM 0, send "ping", expect "pong"
//!
//! # Protocol
//!
//! Client → Server: `"PING:{vm_id}\n"`
//! Server → Client: `"PONG:{vm_id}\n"`
//!
//! # Build
//!
//! ```sh
//! scripts/build-net-guest.sh  # → guest/initrd-net.gz
//! ```

use chaoscontrol_guest_net::{vm_id, GuestNetwork, TcpHandle, TcpState};
use chaoscontrol_sdk::assert::details;
use chaoscontrol_sdk::{assert, coverage, lifecycle, random};
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════
//  Constants
// ═══════════════════════════════════════════════════════════════════════

const SERVER_PORT: u16 = 8080;
const NUM_VMS: usize = 3;
const NET_SNAPSHOT_REPLAY_PROBE_ASSERTION_ID: u32 = 3_141_592_653;

fn cmdline_value(name: &str) -> Option<String> {
    cmdline_value_from(
        &std::fs::read_to_string("/proc/cmdline").unwrap_or_default(),
        name,
    )
}

fn cmdline_value_from(cmdline: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    cmdline
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix).map(str::to_owned))
}

fn snapshot_probe_enabled() -> bool {
    matches!(
        cmdline_value("net_bug").as_deref(),
        Some("snapshot_replay_probe") | Some("snapshot_probe")
    )
}

fn snapshot_probe_fail_after() -> usize {
    cmdline_value("net_snapshot_probe_fail_after")
        .and_then(|value| value.parse().ok())
        .unwrap_or(25)
}

// ═══════════════════════════════════════════════════════════════════════
//  Server (VM 0)
// ═══════════════════════════════════════════════════════════════════════

struct Server {
    /// Active listening socket handles (one per accepted connection).
    listen_handle: TcpHandle,
    /// Number of pings received and responded to.
    pong_count: usize,
    snapshot_probe: bool,
    snapshot_probe_fail_after: usize,
}

impl Server {
    fn new(net: &mut GuestNetwork, snapshot_probe: bool, snapshot_probe_fail_after: usize) -> Self {
        let handle = net.tcp_listen(SERVER_PORT);
        Self {
            listen_handle: handle,
            pong_count: 0,
            snapshot_probe,
            snapshot_probe_fail_after,
        }
    }

    fn tick(&mut self, net: &mut GuestNetwork) {
        let state = net.tcp_state(self.listen_handle);

        match state {
            TcpState::Listen => {
                // Waiting for connection
            }
            TcpState::Established => {
                // Connection active — try to receive data
                let mut buf = [0u8; 256];
                let n = net.tcp_recv(self.listen_handle, &mut buf);
                if n > 0 {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    println!("[server] Received: {}", msg.trim());

                    // Parse "PING:{vm_id}" and respond with "PONG:{vm_id}"
                    if let Some(client_id) = msg.trim().strip_prefix("PING:") {
                        let response = format!("PONG:{}\n", client_id);
                        let sent = net.tcp_send(self.listen_handle, response.as_bytes());
                        if sent > 0 {
                            self.pong_count += 1;
                            println!(
                                "[server] Sent PONG to client {} (total: {})",
                                client_id, self.pong_count
                            );
                            coverage::record_edge(self.pong_count);

                            assert::always(
                                true,
                                "server responds to ping",
                                &details::merge(
                                    &details::network(client_id.parse().unwrap_or(0), 0, true),
                                    &json!({"pong_count": self.pong_count}),
                                ),
                            );
                            assert::sometimes(
                                self.pong_count >= 2,
                                "server handles multiple pings",
                                &json!({"pong_count": self.pong_count}),
                            );
                            if self.snapshot_probe {
                                assert::always_with_id(
                                    self.pong_count < self.snapshot_probe_fail_after,
                                    NET_SNAPSHOT_REPLAY_PROBE_ASSERTION_ID,
                                    "net snapshot replay probe trips only after restored parent context",
                                    &json!({
                                        "pong_count": self.pong_count,
                                        "fail_after": self.snapshot_probe_fail_after,
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            TcpState::CloseWait | TcpState::LastAck | TcpState::Closed | TcpState::TimeWait => {
                // Connection closed — re-listen
                println!("[server] Connection closed, re-listening...");
                self.listen_handle = net.tcp_re_listen(self.listen_handle, SERVER_PORT);
            }
            _ => {
                // Transitional state — wait
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Client (VM 1+)
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, PartialEq)]
enum ClientState {
    Connecting,
    SendPing,
    WaitPong,
    GotPong,
    Cooldown { remaining: usize },
}

struct Client {
    my_id: usize,
    handle: TcpHandle,
    state: ClientState,
    pings_sent: usize,
    pongs_received: usize,
}

impl Client {
    fn new(net: &mut GuestNetwork, vm_id: usize) -> Self {
        let server_ip = net.peer_ip(0);
        let handle = net.tcp_connect(server_ip, SERVER_PORT);
        Self {
            my_id: vm_id,
            handle,
            state: ClientState::Connecting,
            pings_sent: 0,
            pongs_received: 0,
        }
    }

    fn tick(&mut self, net: &mut GuestNetwork) {
        let tcp_state = net.tcp_state(self.handle);

        match &self.state {
            ClientState::Connecting => {
                if tcp_state == TcpState::Established {
                    println!("[client {}] Connected to server", self.my_id);
                    self.state = ClientState::SendPing;
                }
            }
            ClientState::SendPing => {
                if net.tcp_can_send(self.handle) {
                    let msg = format!("PING:{}\n", self.my_id);
                    let sent = net.tcp_send(self.handle, msg.as_bytes());
                    if sent > 0 {
                        self.pings_sent += 1;
                        println!("[client {}] Sent PING #{}", self.my_id, self.pings_sent);
                        coverage::record_edge(0x1000 + self.pings_sent);
                        self.state = ClientState::WaitPong;
                    }
                }
            }
            ClientState::WaitPong => {
                let mut buf = [0u8; 256];
                let n = net.tcp_recv(self.handle, &mut buf);
                if n > 0 {
                    let msg = String::from_utf8_lossy(&buf[..n]);
                    println!("[client {}] Received: {}", self.my_id, msg.trim());

                    let expected = format!("PONG:{}", self.my_id);
                    if msg.trim() == expected {
                        self.pongs_received += 1;
                        println!(
                            "[client {}] PONG #{} verified!",
                            self.my_id, self.pongs_received
                        );

                        assert::always(
                            true,
                            "client receives correct pong",
                            &details::merge(
                                &details::network(0, self.my_id, true),
                                &json!({"pongs": self.pongs_received}),
                            ),
                        );
                        assert::sometimes(
                            self.pongs_received >= 3,
                            "client gets 3+ pongs",
                            &details::merge(
                                &details::network(0, self.my_id, true),
                                &json!({"pongs": self.pongs_received}),
                            ),
                        );
                        self.state = ClientState::GotPong;
                    }
                }

                // Handle connection loss
                if tcp_state == TcpState::CloseWait || tcp_state == TcpState::Closed {
                    println!(
                        "[client {}] Connection lost while waiting for pong",
                        self.my_id
                    );
                    self.reconnect(net);
                }
            }
            ClientState::GotPong => {
                // Random cooldown before next ping (deterministic via SDK)
                let cooldown = random::random_choice(5) + 1; // 1-5 ticks
                coverage::record_edge(0x2000 + cooldown);
                self.state = ClientState::Cooldown {
                    remaining: cooldown,
                };
            }
            ClientState::Cooldown { remaining } => {
                let r = *remaining;
                if r <= 1 {
                    self.state = ClientState::SendPing;
                } else {
                    self.state = ClientState::Cooldown { remaining: r - 1 };
                }
            }
        }
    }

    fn reconnect(&mut self, net: &mut GuestNetwork) {
        println!("[client {}] Reconnecting...", self.my_id);
        net.tcp_close(self.handle);
        let server_ip = net.peer_ip(0);
        // Need a new socket since the old one is in closing state
        self.handle = net.tcp_re_listen(self.handle, 0); // remove old
        self.handle = net.tcp_connect(server_ip, SERVER_PORT);
        self.state = ClientState::Connecting;
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════════════════════════════════

fn main() {
    chaoscontrol_sdk::runtime::guest_init();
    println!("chaoscontrol-net-guest starting...");
    let my_id = vm_id();
    let num_vms = chaoscontrol_guest_net::parse_cmdline_param("num_vms").unwrap_or(NUM_VMS);

    println!("VM{}: initializing network ({} VMs total)", my_id, num_vms);

    let mut net = GuestNetwork::init(my_id, num_vms);

    let snapshot_probe = snapshot_probe_enabled();
    let snapshot_probe_fail_after = snapshot_probe_fail_after();
    if snapshot_probe {
        println!(
            "VM{}: net snapshot replay probe enabled (fail_after={})",
            my_id, snapshot_probe_fail_after
        );
    }

    lifecycle::setup_complete(&json!({
        "role": if my_id == 0 { "server" } else { "client" },
        "vm_id": my_id,
        "num_vms": num_vms,
        "snapshot_probe": snapshot_probe,
        "snapshot_probe_fail_after": snapshot_probe_fail_after,
    }));

    println!("VM{}: setup complete, entering main loop", my_id);

    if my_id == 0 {
        let mut server = Server::new(&mut net, snapshot_probe, snapshot_probe_fail_after);
        loop {
            net.poll();
            server.tick(&mut net);
        }
    } else {
        let mut client = Client::new(&mut net, my_id);
        loop {
            net.poll();
            client.tick(&mut net);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cmdline_value() {
        let cmdline = "console=ttyS0 net_bug=snapshot_replay_probe net_snapshot_probe_fail_after=17 num_vms=3";
        assert_eq!(
            cmdline_value_from(cmdline, "net_bug"),
            Some("snapshot_replay_probe".to_string())
        );
        assert_eq!(
            cmdline_value_from(cmdline, "net_snapshot_probe_fail_after"),
            Some("17".to_string())
        );
        assert_eq!(cmdline_value_from(cmdline, "missing"), None);
    }
}
