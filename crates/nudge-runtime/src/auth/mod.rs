//! Nudge Authentication Subsystem (Pack 5 Provider Auth).
//!
//! Provides first-class account and authorization support:
//! - Isolated credential store (~/.nudge/credentials.json, mode 0600).
//! - Fail-closed dispatch and non-interactive (CI/daemon) gating.
//! - In-process RFC 8628 (OAuth 2.0 Device Authorization Grant) interactive client.
//! - Scope verification and permission enforcement.
//! - Zero foreign token harvesting (no reading of ~/.jcode or ~/.openhands).

pub mod credential;
pub mod device_flow;
pub mod store;

pub use credential::Credential;
pub use device_flow::{is_tty, validate_scope, DeviceFlowClient, DeviceFlowConfig};
pub use store::{AccountEntry, CredentialStore};

#[cfg(test)]
mod tests;
