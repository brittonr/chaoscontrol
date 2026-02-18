//! Deterministic simulated network device for the ChaosControl hypervisor.
//!
//! Provides a fully in-memory NIC with explicit RX/TX queues, replacing
//! `virtio-net` so that all network I/O is controllable and reproducible.
//! The test harness injects packets into the RX queue and drains the TX
//! queue to observe what the guest sends.

use std::collections::VecDeque;

/// Per-direction packet and byte counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
pub struct NetSnapshot {
    mac: [u8; 6],
    rx_queue: VecDeque<Vec<u8>>,
    tx_queue: VecDeque<Vec<u8>>,
    stats: NetStats,
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
    /// Cumulative statistics.
    stats: NetStats,
}

impl DeterministicNet {
    /// Create a new simulated NIC with the given MAC address.
    pub fn new(mac: [u8; 6]) -> Self {
        Self {
            mac,
            rx_queue: VecDeque::new(),
            tx_queue: VecDeque::new(),
            stats: NetStats::default(),
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

    /// Dequeue the next packet from the RX queue (oldest first).
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop_rx(&mut self) -> Option<Vec<u8>> {
        self.rx_queue.pop_front()
    }

    /// Record a packet transmitted by the guest (guest → harness).
    pub fn enqueue_tx(&mut self, data: Vec<u8>) {
        self.stats.tx_bytes += data.len() as u64;
        self.stats.tx_packets += 1;
        self.tx_queue.push_back(data);
    }

    /// Drain all packets transmitted by the guest, returning them in order.
    pub fn drain_tx(&mut self) -> Vec<Vec<u8>> {
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
            stats: snapshot.stats.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

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
        net.enqueue_tx(vec![10, 20]);
        net.enqueue_tx(vec![30]);

        let packets = net.drain_tx();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0], vec![10, 20]);
        assert_eq!(packets[1], vec![30]);

        // Queue is empty after drain
        assert!(net.drain_tx().is_empty());
    }

    #[test]
    fn stats_tracking() {
        let mut net = DeterministicNet::new(TEST_MAC);
        net.inject_packet(vec![0; 100]);
        net.inject_packet(vec![0; 50]);
        net.enqueue_tx(vec![0; 200]);

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
        net.enqueue_tx(vec![4, 5, 6]);

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
