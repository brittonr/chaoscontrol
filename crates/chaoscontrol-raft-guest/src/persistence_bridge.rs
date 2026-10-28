//! Bind in-process Raft protocol state to one persistent deterministic disk.
//!
//! Only declared protocol facts cross the boundary; the adapter performs no
//! host I/O and grants no voting or repair authority.
//!
//! r[impl chaoscontrol.smr_storage_recovery.persistent_nodes]

use chaoscontrol_smr::persistence::{
    PersistentLogEntry, PersistentNodeCohort, PersistentNodeState, SmrDisk, StoragePublication,
    MAX_STORAGE_LOG_ENTRIES,
};
use chaoscontrol_smr::SmrError;

use crate::{LogEntry, Node};

fn deny(class: &'static str, detail: impl Into<String>) -> SmrError {
    SmrError {
        class,
        detail: detail.into(),
    }
}

/// Encode the node's durable protocol facts for one disk record.
///
/// The applied prefix is retained as opaque application bytes; the log is
/// retained in full because this workload does not model snapshot pruning.
pub fn node_storage_state(node: &Node) -> Result<PersistentNodeState, SmrError> {
    if node.log.len() > MAX_STORAGE_LOG_ENTRIES {
        return Err(deny(
            "raft-storage-log",
            format!(
                "log of {} entries exceeds {MAX_STORAGE_LOG_ENTRIES}",
                node.log.len()
            ),
        ));
    }
    let mut log = Vec::with_capacity(node.log.len());
    for (offset, entry) in node.log.iter().enumerate() {
        log.push(PersistentLogEntry {
            index: offset as u64 + 1,
            term: entry.term,
            command: entry.value.to_le_bytes().to_vec(),
        });
    }
    let applied = node.retained_applied_index_value();
    let mut application = Vec::with_capacity((applied + 1) * 8);
    application.extend_from_slice(&applied.to_le_bytes());
    for entry in node.log.iter().take(applied) {
        application.extend_from_slice(&entry.value.to_le_bytes());
    }
    Ok(PersistentNodeState {
        term: node.current_term,
        voted_for: node.voted_for.map(|vote| vote as u64),
        snapshot: None,
        log,
        application,
    })
}

/// Restore durable protocol facts into a restarted node.
///
/// Returns `false` when the disk never published a record.
pub fn restore_node(cohort: &PersistentNodeCohort, node: &mut Node) -> Result<bool, SmrError> {
    let Some(state) = cohort.node(node.id)?.load()? else {
        return Ok(false);
    };
    if state.log.len() > MAX_STORAGE_LOG_ENTRIES {
        return Err(deny(
            "raft-storage-log",
            "stored log exceeds the admitted bound",
        ));
    }
    let mut log = Vec::with_capacity(state.log.len());
    for (offset, entry) in state.log.iter().enumerate() {
        if entry.index != offset as u64 + 1 {
            return Err(deny(
                "raft-storage-log",
                "stored log is not a contiguous prefix",
            ));
        }
        let bytes: [u8; 8] = entry
            .command
            .as_slice()
            .try_into()
            .map_err(|_| deny("raft-storage-log", "stored command is not one u64 value"))?;
        log.push(LogEntry {
            term: entry.term,
            value: u64::from_le_bytes(bytes),
        });
    }
    if state.application.len() < 8 {
        return Err(deny(
            "raft-storage-application",
            "stored application is truncated",
        ));
    }
    let mut applied = [0_u8; 8];
    applied.copy_from_slice(&state.application[..8]);
    let applied = usize::try_from(u64::from_le_bytes(applied)).map_err(|_| {
        deny(
            "raft-storage-application",
            "applied prefix does not fit the host",
        )
    })?;
    if applied > log.len() || state.application.len() != 8 + applied * 8 {
        return Err(deny(
            "raft-storage-application",
            "stored application prefix does not match the log",
        ));
    }
    node.current_term = state.term;
    node.voted_for = state
        .voted_for
        .map(|vote| {
            usize::try_from(vote)
                .map_err(|_| deny("raft-storage-vote", "vote does not fit the host"))
        })
        .transpose()?;
    node.log = log;
    node.retain_applied_prefix(applied);
    Ok(true)
}

/// Persist one node's state to its own disk.
pub fn persist_node(node: &Node, disk: &mut SmrDisk) -> Result<StoragePublication, SmrError> {
    disk.write(&node_storage_state(node)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with_log() -> Node {
        let mut node = Node::new_with_config(0, 3, crate::BugMode::None);
        node.current_term = 3;
        node.voted_for = Some(1);
        node.log = vec![
            LogEntry { term: 1, value: 10 },
            LogEntry { term: 2, value: 20 },
            LogEntry { term: 3, value: 30 },
        ];
        node.commit_index = 2;
        node.retain_applied_prefix(2);
        node
    }

    #[test]
    fn restart_restores_term_vote_log_and_applied_prefix() {
        let node = node_with_log();
        let mut cohort = PersistentNodeCohort::new(3, 4096).expect("cohort");
        persist_node(&node, cohort.node_mut(0).expect("node")).expect("persist");

        let mut restarted = Node::new_with_config(0, 3, crate::BugMode::None);
        restarted.restart();
        assert!(restore_node(&cohort, &mut restarted).expect("restore"));

        assert_eq!(restarted.current_term, 3);
        assert_eq!(restarted.voted_for, Some(1));
        assert_eq!(restarted.log, node.log);
        assert_eq!(restarted.retained_applied_index_value(), 2);
        assert_eq!(restarted.commit_index, 0);
    }

    #[test]
    fn crash_before_flush_preserves_the_previous_record() {
        let mut disk = SmrDisk::new(4096).expect("disk");
        let mut node = node_with_log();
        persist_node(&node, &mut disk).expect("first persist");
        let published = disk.published_identity();

        node.current_term = 9;
        node.voted_for = Some(2);
        node.log.push(LogEntry { term: 9, value: 90 });
        disk.fail_next_flush();
        disk.stage(&node_storage_state(&node).expect("state"))
            .expect("stage");
        assert!(disk.flush().is_err());
        assert_eq!(disk.published_identity(), published);
        let restored = disk.load().expect("load").expect("published");
        assert_eq!(restored.term, 3);
        assert_eq!(restored.log.len(), 3);
    }

    #[test]
    fn unrecovered_disk_reports_no_published_state() {
        let cohort = PersistentNodeCohort::new(3, 4096).expect("cohort");
        let mut node = Node::new_with_config(1, 3, crate::BugMode::None);
        assert!(!restore_node(&cohort, &mut node).expect("restore"));
        assert_eq!(node.current_term, 0);
        assert!(node.log.is_empty());
    }
}
