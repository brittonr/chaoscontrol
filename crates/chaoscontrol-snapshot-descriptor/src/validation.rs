#![allow(
    non_trait_imports,
    reason = "the validation facade exports the closed descriptor validators and one shared diagnostic type"
)]

mod descriptor;
mod observations;
mod preflight;

use std::fmt::{Display, Formatter};

pub use descriptor::{expected_state_owners, validate_descriptor, validate_payload_closure};
pub use observations::{
    validate_consumer_reference, validate_locator_sidecar, validate_restore_receipt,
};
pub use preflight::preflight;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorError {
    code: &'static str,
    detail: String,
}

impl DescriptorError {
    pub fn new(code: &'static str, detail: String) -> Self {
        Self { code, detail }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Display for DescriptorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for DescriptorError {}
