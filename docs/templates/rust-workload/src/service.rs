// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tiny downstream-shaped service used by the copyable template.
//!
//! The service can run without ChaosControl instrumentation by default. Enable
//! `--features chaoscontrol-in-process` to place selected SDK assertions inside
//! service code when invariants are not visible from the external workload.

#[cfg(feature = "chaoscontrol-in-process")]
use serde_json::json;

#[derive(Debug, Default)]
pub struct KeyValueService {
    committed_writes: usize,
    readable: bool,
}

impl KeyValueService {
    pub fn start() -> Self {
        Self {
            committed_writes: 0,
            readable: true,
        }
    }

    pub fn write(&mut self, key: &str, value: &str) -> bool {
        let accepted = !key.is_empty() && !value.is_empty();
        if accepted {
            self.committed_writes += 1;
        }
        self.assert_internal_consistency("write");
        accepted
    }

    pub fn read_after_restart(&mut self) -> bool {
        self.readable = self.committed_writes > 0;
        self.assert_internal_consistency("read_after_restart");
        self.readable
    }

    pub fn committed_writes(&self) -> usize {
        self.committed_writes
    }

    #[cfg(feature = "chaoscontrol-in-process")]
    fn assert_internal_consistency(&self, operation: &str) {
        let details = json!({
            "category": "service-invariant",
            "adoption_track": "in-process-service",
            "operation": operation,
            "committed_writes": self.committed_writes,
        });
        chaoscontrol_sdk::cc_assert_always_category!(
            "my-service",
            "service-invariant",
            self.committed_writes <= 1_000_000,
            "service write count remains bounded",
            &details,
        );
    }

    #[cfg(not(feature = "chaoscontrol-in-process"))]
    fn assert_internal_consistency(&self, _operation: &str) {}
}
