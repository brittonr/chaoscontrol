//! Deterministic simulated network device for the ChaosControl hypervisor.
//!
//! Provides a fully in-memory NIC with explicit RX/TX queues, replacing
//! `virtio-net` so that all network I/O is controllable and reproducible.
//! The test harness injects packets into the RX queue and drains the TX
//! queue to observe what the guest sends.

use super::virtio_types::{VirtioLimits, DEFAULT_MAX_NET_TX_BYTES, DEFAULT_MAX_NET_TX_PACKETS};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetQueueError {
    PacketLimit { requested: usize, maximum: usize },
    ByteLimit { requested: u64, maximum: u64 },
    Arithmetic,
    Allocation,
    PostCommit,
}

/// Per-direction packet and byte counters.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetStats {
    /// Packets received by the guest (injected by the harness).
    pub rx_packets: u64,
    /// Packets transmitted by the guest.
    pub tx_packets: u64,
    /// Total bytes received by the guest.
    pub rx_bytes: u64,
    /// Total bytes transmitted by the guest.
    pub tx_bytes: u64,
}

/// Snapshot of a [`DeterministicNet`], capturing queues, MAC, and stats.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NetSnapshot {
    mac: [u8; 6],
    rx_queue: VecDeque<Vec<u8>>,
    tx_queue: VecDeque<Vec<u8>>,
    stats: NetStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetSnapshotQueue {
    Receive,
    Transmit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetSnapshotValidationError {
    MacMismatch {
        expected: [u8; 6],
        actual: [u8; 6],
    },
    PacketLimit {
        queue: NetSnapshotQueue,
        requested: usize,
        maximum: usize,
    },
    FrameLimit {
        queue: NetSnapshotQueue,
        requested: u64,
        maximum: u64,
    },
    ByteLimit {
        queue: NetSnapshotQueue,
        requested: u64,
        maximum: u64,
    },
    StatsUnderflow {
        queue: NetSnapshotQueue,
    },
    Arithmetic,
}

impl NetSnapshot {
    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    pub fn validate_mac(&self, expected: [u8; 6]) -> Result<(), NetSnapshotValidationError> {
        let actual = self.mac();
        if actual != expected {
            return Err(NetSnapshotValidationError::MacMismatch { expected, actual });
        }
        Ok(())
    }

    /// Validate retained queue and counter facts before backend mutation.
    pub fn validate_structure(
        &self,
        limits: VirtioLimits,
    ) -> Result<(), NetSnapshotValidationError> {
        if self.tx_queue.len() > limits.max_net_tx_packets {
            return Err(NetSnapshotValidationError::PacketLimit {
                queue: NetSnapshotQueue::Transmit,
                requested: self.tx_queue.len(),
                maximum: limits.max_net_tx_packets,
            });
        }
        let rx_bytes = validate_snapshot_packets(
            NetSnapshotQueue::Receive,
            &self.rx_queue,
            limits.max_net_frame_bytes,
        )?;
        let tx_bytes = validate_snapshot_packets(
            NetSnapshotQueue::Transmit,
            &self.tx_queue,
            limits.max_net_frame_bytes,
        )?;
        if tx_bytes > limits.max_net_tx_bytes {
            return Err(NetSnapshotValidationError::ByteLimit {
                queue: NetSnapshotQueue::Transmit,
                requested: tx_bytes,
                maximum: limits.max_net_tx_bytes,
            });
        }
        let rx_packets = u64::try_from(self.rx_queue.len())
            .map_err(|_| NetSnapshotValidationError::Arithmetic)?;
        let tx_packets = u64::try_from(self.tx_queue.len())
            .map_err(|_| NetSnapshotValidationError::Arithmetic)?;
        if self.stats.rx_packets < rx_packets || self.stats.rx_bytes < rx_bytes {
            return Err(NetSnapshotValidationError::StatsUnderflow {
                queue: NetSnapshotQueue::Receive,
            });
        }
        if self.stats.tx_packets < tx_packets || self.stats.tx_bytes < tx_bytes {
            return Err(NetSnapshotValidationError::StatsUnderflow {
                queue: NetSnapshotQueue::Transmit,
            });
        }
        Ok(())
    }
}

fn validate_snapshot_packets(
    queue: NetSnapshotQueue,
    packets: &VecDeque<Vec<u8>>,
    max_frame_bytes: u64,
) -> Result<u64, NetSnapshotValidationError> {
    let mut retained_bytes = 0u64;
    for packet in packets {
        let packet_bytes =
            u64::try_from(packet.len()).map_err(|_| NetSnapshotValidationError::Arithmetic)?;
        if packet_bytes > max_frame_bytes {
            return Err(NetSnapshotValidationError::FrameLimit {
                queue,
                requested: packet_bytes,
                maximum: max_frame_bytes,
            });
        }
        retained_bytes = retained_bytes
            .checked_add(packet_bytes)
            .ok_or(NetSnapshotValidationError::Arithmetic)?;
    }
    Ok(retained_bytes)
}

/// A simulated network device with explicit RX/TX queues.
///
/// # Examples
///
/// ```
/// use chaoscontrol_vmm::devices::net::DeterministicNet;
///
/// let mut net = DeterministicNet::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
///
/// // Harness injects a packet for the guest to receive
/// net.inject_packet(vec![0xFF; 64]);
/// assert!(net.has_rx_data());
///
/// let pkt = net.pop_rx().unwrap();
/// assert_eq!(pkt.len(), 64);
/// ```
#[derive(Clone, Debug)]
pub struct DeterministicNet {
    /// MAC address of the virtual NIC.
    mac: [u8; 6],
    /// Packets waiting to be delivered to the guest.
    rx_queue: VecDeque<Vec<u8>>,
    /// Packets transmitted by the guest.
    tx_queue: VecDeque<Vec<u8>>,
    /// Bytes retained in `tx_queue` and not yet drained by the host.
    tx_queued_bytes: u64,
    /// Cumulative statistics.
    stats: NetStats,
    /// Live-only injection for a failure after queue processing starts.
    fail_after_next_tx_enqueue: bool,
}

impl DeterministicNet {
    /// Create a new simulated NIC with the given MAC address.
    pub fn new(mac: [u8; 6]) -> Self {
        Self {
            mac,
            rx_queue: VecDeque::new(),
            tx_queue: VecDeque::new(),
            tx_queued_bytes: 0,
            stats: NetStats::default(),
            fail_after_next_tx_enqueue: false,
        }
    }

    /// The MAC address assigned to this virtual NIC.
    pub fn mac(&self) -> &[u8; 6] {
        &self.mac
    }

    /// Inject a packet into the RX queue (harness → guest).
    ///
    /// The packet will be available via [`pop_rx`](Self::pop_rx) in FIFO
    /// order.
    pub fn inject_packet(&mut self, data: Vec<u8>) {
        self.stats.rx_bytes += data.len() as u64;
        self.stats.rx_packets += 1;
        self.rx_queue.push_back(data);
    }

    /// Check whether there are packets waiting in the RX queue.
    pub fn has_rx_data(&self) -> bool {
        !self.rx_queue.is_empty()
    }

    /// Borrow the next RX packet without consuming it.
    pub fn peek_rx(&self) -> Option<&[u8]> {
        self.rx_queue.front().map(Vec::as_slice)
    }

    /// Dequeue the next packet from the RX queue (oldest first).
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop_rx(&mut self) -> Option<Vec<u8>> {
        self.rx_queue.pop_front()
    }

    pub fn validate_tx_retention(
        &self,
        packet_bytes: usize,
        max_packets: usize,
        max_bytes: u64,
    ) -> Result<(), NetQueueError> {
        self.tx_retention_values(packet_bytes, max_packets, max_bytes)
            .map(|_| ())
    }

    /// Record a packet only when the retained queue stays within both limits.
    pub fn try_enqueue_tx_bounded(
        &mut self,
        data: Vec<u8>,
        max_packets: usize,
        max_bytes: u64,
    ) -> Result<(), NetQueueError> {
        let (requested_bytes, tx_bytes, tx_packets) =
            self.tx_retention_values(data.len(), max_packets, max_bytes)?;
        if self.tx_queue.try_reserve(1).is_err() {
            return Err(NetQueueError::Allocation);
        }
        self.stats.tx_bytes = tx_bytes;
        self.stats.tx_packets = tx_packets;
        self.tx_queued_bytes = requested_bytes;
        self.tx_queue.push_back(data);
        if std::mem::take(&mut self.fail_after_next_tx_enqueue) {
            return Err(NetQueueError::PostCommit);
        }
        Ok(())
    }

    fn tx_retention_values(
        &self,
        packet_bytes: usize,
        max_packets: usize,
        max_bytes: u64,
    ) -> Result<(u64, u64, u64), NetQueueError> {
        let requested_packets = self
            .tx_queue
            .len()
            .checked_add(1)
            .ok_or(NetQueueError::Arithmetic)?;
        if requested_packets > max_packets {
            return Err(NetQueueError::PacketLimit {
                requested: requested_packets,
                maximum: max_packets,
            });
        }
        let length = u64::try_from(packet_bytes).map_err(|_| NetQueueError::Arithmetic)?;
        let requested_bytes = self
            .tx_queued_bytes
            .checked_add(length)
            .ok_or(NetQueueError::Arithmetic)?;
        if requested_bytes > max_bytes {
            return Err(NetQueueError::ByteLimit {
                requested: requested_bytes,
                maximum: max_bytes,
            });
        }
        let tx_bytes = self
            .stats
            .tx_bytes
            .checked_add(length)
            .ok_or(NetQueueError::Arithmetic)?;
        let tx_packets = self
            .stats
            .tx_packets
            .checked_add(1)
            .ok_or(NetQueueError::Arithmetic)?;
        Ok((requested_bytes, tx_bytes, tx_packets))
    }

    pub fn inject_failure_after_next_tx_enqueue(&mut self) {
        self.fail_after_next_tx_enqueue = true;
    }

    /// Record a host-controlled packet with the default retained limits.
    pub fn enqueue_tx(&mut self, data: Vec<u8>) -> Result<(), NetQueueError> {
        self.try_enqueue_tx_bounded(data, DEFAULT_MAX_NET_TX_PACKETS, DEFAULT_MAX_NET_TX_BYTES)
    }

    pub fn tx_queued_packets(&self) -> usize {
        self.tx_queue.len()
    }

    pub fn tx_queued_bytes(&self) -> u64 {
        self.tx_queued_bytes
    }

    /// Drain all packets transmitted by the guest, returning them in order.
    pub fn drain_tx(&mut self) -> Vec<Vec<u8>> {
        self.tx_queued_bytes = 0;
        self.tx_queue.drain(..).collect()
    }

    /// Current network statistics.
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }

    /// Capture a snapshot of the full device state.
    pub fn snapshot(&self) -> NetSnapshot {
        NetSnapshot {
            mac: self.mac,
            rx_queue: self.rx_queue.clone(),
            tx_queue: self.tx_queue.clone(),
            stats: self.stats.clone(),
        }
    }

    /// Restore a device from a previously captured snapshot.
    pub fn restore(snapshot: &NetSnapshot) -> Self {
        Self {
            mac: snapshot.mac,
            rx_queue: snapshot.rx_queue.clone(),
            tx_queue: snapshot.tx_queue.clone(),
            tx_queued_bytes: retained_bytes(&snapshot.tx_queue),
            stats: snapshot.stats.clone(),
            fail_after_next_tx_enqueue: false,
        }
    }
}

fn retained_bytes(queue: &VecDeque<Vec<u8>>) -> u64 {
    queue.iter().fold(0u64, |total, packet| {
        let length = u64::try_from(packet.len()).unwrap_or(u64::MAX);
        total.saturating_add(length)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
    const OTHER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];

    #[test]
    fn new_device_has_empty_queues() {
        let mut net = DeterministicNet::new(TEST_MAC);
        assert!(!net.has_rx_data());
        assert!(net.drain_tx().is_empty());
        assert_eq!(*net.mac(), TEST_MAC);
    }

    #[test]
    fn inject_and_pop_rx() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.inject_packet(vec![1, 2, 3]);
        net.inject_packet(vec![4, 5]);

        assert!(net.has_rx_data());
        assert_eq!(net.pop_rx(), Some(vec![1, 2, 3]));
        assert_eq!(net.pop_rx(), Some(vec![4, 5]));
        assert_eq!(net.pop_rx(), None);
        assert!(!net.has_rx_data());
    }

    #[test]
    fn enqueue_and_drain_tx() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.enqueue_tx(vec![10, 20]).expect("first TX packet");
        net.enqueue_tx(vec![30]).expect("second TX packet");

        let packets = net.drain_tx();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], vec![10, 20]);
        assert_eq!(packets[1], vec![30]);

        // Queue is empty after drain
        assert!(net.drain_tx().is_empty());
    }

    #[test]
    fn enqueue_tx_reports_default_packet_limit() {
        let mut net = DeterministicNet::new(TEST_MAC);
        for _ in 0..DEFAULT_MAX_NET_TX_PACKETS {
            net.enqueue_tx(Vec::new()).expect("packet within limit");
        }
        assert_eq!(
            net.enqueue_tx(Vec::new()),
            Err(NetQueueError::PacketLimit {
                requested: DEFAULT_MAX_NET_TX_PACKETS + 1,
                maximum: DEFAULT_MAX_NET_TX_PACKETS,
            })
        );
    }

    #[test]
    fn stats_tracking() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.inject_packet(vec![0; 100]);
        net.inject_packet(vec![0; 50]);
        net.enqueue_tx(vec![0; 200]).expect("TX packet");

        let s = net.stats();
        assert_eq!(s.rx_packets, 2);
        assert_eq!(s.rx_bytes, 150);
        assert_eq!(s.tx_packets, 1);
        assert_eq!(s.tx_bytes, 200);
    }

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.inject_packet(vec![1, 2, 3]);
        net.enqueue_tx(vec![4, 5, 6]).expect("TX packet");

        let snap = net.snapshot();

        // Mutate the original
        net.pop_rx();
        net.drain_tx();
        net.inject_packet(vec![99]);

        // Restore from snapshot
        let mut restored = DeterministicNet::restore(&snap);
        assert_eq!(restored.pop_rx(), Some(vec![1, 2, 3]));
        let tx = restored.drain_tx();
        assert_eq!(tx, vec![vec![4, 5, 6]]);
        assert_eq!(*restored.mac(), TEST_MAC);
        assert_eq!(restored.stats().rx_packets, snap.stats.rx_packets);
    }

    #[test]
    fn snapshot_structure_accepts_retained_queues() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.inject_packet(vec![1, 2, 3]);
        net.enqueue_tx(vec![4, 5, 6]).expect("TX packet");

        assert_eq!(
            net.snapshot().validate_structure(VirtioLimits::default()),
            Ok(())
        );
    }

    #[test]
    fn snapshot_structure_rejects_oversized_packets_and_forged_stats() {
        let limits = VirtioLimits::default();
        let oversized_bytes = usize::try_from(limits.max_net_frame_bytes)
            .expect("frame limit fits usize")
            .saturating_add(1);
        let mut oversized = DeterministicNet::new(TEST_MAC).snapshot();
        oversized.rx_queue.push_back(vec![0; oversized_bytes]);
        assert!(matches!(
            oversized.validate_structure(limits),
            Err(NetSnapshotValidationError::FrameLimit {
                queue: NetSnapshotQueue::Receive,
                ..
            })
        ));

        let mut forged_stats = DeterministicNet::new(TEST_MAC);
        forged_stats
            .enqueue_tx(vec![1])
            .expect("retained TX packet");
        let mut forged_stats = forged_stats.snapshot();
        forged_stats.stats.tx_packets = 0;
        assert_eq!(
            forged_stats.validate_structure(limits),
            Err(NetSnapshotValidationError::StatsUnderflow {
                queue: NetSnapshotQueue::Transmit,
            })
        );

        let wrong_mac = DeterministicNet::new(TEST_MAC).snapshot();
        assert_eq!(
            wrong_mac.validate_mac(OTHER_MAC),
            Err(NetSnapshotValidationError::MacMismatch {
                expected: OTHER_MAC,
                actual: TEST_MAC,
            })
        );
    }

    #[test]
    fn fifo_ordering() {
        let mut net = DeterministicNet::new(TEST_MAC);
        for i in 0..5u8 {
            net.inject_packet(vec![i]);
        }
        for i in 0..5u8 {
            assert_eq!(net.pop_rx(), Some(vec![i]));
        }
    }

    #[test]
    fn mac_address() {
        let mac = [0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01];
        let net = DeterministicNet::new(mac);
        assert_eq!(*net.mac(), mac);
    }

    #[test]
    fn default_stats() {
        let net = DeterministicNet::new(TEST_MAC);
        let s = net.stats();
        assert_eq!(
            *s,
            NetStats {
                rx_packets: 0,
                tx_packets: 0,
                rx_bytes: 0,
                tx_bytes: 0,
            }
        );
    }
}
