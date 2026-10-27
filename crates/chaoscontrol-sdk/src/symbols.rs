//! Runtime facts for symbolized coverage.
//!
//! Build-produced symbol catalogs bind module content identities and source
//! locations. This module lets an instrumented guest declare the modules it
//! actually loaded, so a consumer can join catalogs and observations exactly.

use core::sync::atomic::{AtomicU32, Ordering};

/// Stable event name for one loaded instrumented module.
pub const MODULE_LOAD_EVENT: &str = "chaoscontrol_coverage_module";
/// Stable module-load record schema.
pub const MODULE_LOAD_SCHEMA: &str = "chaoscontrol.coverage-module-load.v1";
/// Maximum modules one guest may declare.
pub const MAX_DECLARED_MODULES: u32 = 256;
/// Maximum reviewed module path bytes.
pub const MAX_MODULE_PATH_BYTES: usize = 1_024;

static DECLARED_MODULES: AtomicU32 = AtomicU32::new(0);

/// Module declaration failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleLoadError {
    EmptyContentIdentity,
    InvalidPath,
    ModuleBound,
}

impl core::fmt::Display for ModuleLoadError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::EmptyContentIdentity => "module content identity must be nonzero",
            Self::InvalidPath => "module path must be relative, confined, and nonempty",
            Self::ModuleBound => "module declaration count exceeds the bound",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ModuleLoadError {}

/// Declare one loaded instrumented module.
///
/// The declaration is an ordinary structured guest event. It does not grant
/// symbol trust, prove the module bytes, or promote coverage evidence.
#[cfg(feature = "full")]
pub fn register_module(
    content_id: [u8; 32],
    path: &str,
    load_ordinal: u32,
) -> Result<(), ModuleLoadError> {
    validate_module(content_id, path)?;
    let count = DECLARED_MODULES.fetch_add(1, Ordering::Relaxed);
    if count >= MAX_DECLARED_MODULES {
        DECLARED_MODULES.fetch_sub(1, Ordering::Relaxed);
        return Err(ModuleLoadError::ModuleBound);
    }
    crate::lifecycle::send_event(
        MODULE_LOAD_EVENT,
        &serde_json::json!({
            "schema": MODULE_LOAD_SCHEMA,
            "content_id": content_id,
            "path": path,
            "load_ordinal": load_ordinal,
        }),
    );
    Ok(())
}

/// Declare one loaded instrumented module in no-op mode.
#[cfg(not(feature = "full"))]
pub fn register_module(
    content_id: [u8; 32],
    path: &str,
    _load_ordinal: u32,
) -> Result<(), ModuleLoadError> {
    validate_module(content_id, path)
}

fn validate_module(content_id: [u8; 32], path: &str) -> Result<(), ModuleLoadError> {
    if content_id == [0; 32] {
        return Err(ModuleLoadError::EmptyContentIdentity);
    }
    if path.is_empty()
        || path.len() > MAX_MODULE_PATH_BYTES
        || path.starts_with('/')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(ModuleLoadError::InvalidPath);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_module_identity_and_path() {
        assert_eq!(
            validate_module([0; 32], "src/lib.rs").unwrap_err(),
            ModuleLoadError::EmptyContentIdentity
        );
        assert_eq!(
            validate_module([1; 32], "../escape.rs").unwrap_err(),
            ModuleLoadError::InvalidPath
        );
        validate_module([1; 32], "src/lib.rs").unwrap();
    }
}
