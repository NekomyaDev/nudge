//! Provider authorization gate and policy table.
//!
//! Enforces universal hard security boundaries:
//! 1. Zero foreign token harvesting: runtime never snoops on ~/.jcode, ~/.openhands, etc.
//! 2. Fail-closed execution: if a live provider's credentials are missing/invalid,
//!    execution halts with a clear error diagnostic (e.g. error[E-auth-0401] or error[E-auth-0404])
//!    and NEVER silently substitutes FakeProvider.
//! 3. Per-provider authorization gate: providers without a proven official credential
//!    contract have their live adapters disabled.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderGatePolicy {
    /// Fully enabled without external credentials (e.g. fake mock provider).
    Enabled,
    /// Enabled via explicit first-party API key environment variable.
    ApiKey {
        primary_env: &'static str,
        fallback_env: Option<&'static str>,
    },
    /// Adapter disabled because no proven official public OAuth contract exists,
    /// or explicitly disabled via configuration.
    Disabled { reason: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderGateEntry {
    pub name: &'static str,
    pub policy: ProviderGatePolicy,
    pub oauth_enabled: bool,
}

pub const PROVIDER_GATE_TABLE: &[ProviderGateEntry] = &[
    ProviderGateEntry {
        name: "fake",
        policy: ProviderGatePolicy::Enabled,
        oauth_enabled: false,
    },
    ProviderGateEntry {
        name: "openai",
        policy: ProviderGatePolicy::ApiKey {
            primary_env: "OPENAI_API_KEY",
            fallback_env: None,
        },
        oauth_enabled: false,
    },
    ProviderGateEntry {
        name: "claude",
        policy: ProviderGatePolicy::ApiKey {
            primary_env: "ANTHROPIC_API_KEY",
            fallback_env: None,
        },
        oauth_enabled: false,
    },
    ProviderGateEntry {
        name: "antigravity",
        policy: ProviderGatePolicy::Disabled {
            reason: "provider adapter 'antigravity' is disabled because no official public OAuth contract exists",
        },
        oauth_enabled: false,
    },
];

/// Resolve the authorization policy for the specified provider name.
pub fn get_provider_gate_policy(name: &str) -> ProviderGatePolicy {
    // Check if the provider is explicitly disabled via NUDGE_DISABLE_PROVIDER_<NAME>
    let disable_env = format!(
        "NUDGE_DISABLE_PROVIDER_{}",
        name.to_ascii_uppercase().replace('-', "_")
    );
    if std::env::var(&disable_env)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return ProviderGatePolicy::Disabled {
            reason: "provider adapter is explicitly disabled by configuration",
        };
    }

    if name == "antigravity" {
        let has_first_party_key = std::env::var("NUDGE_API_KEY")
            .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);
        if !has_first_party_key {
            return ProviderGatePolicy::Disabled {
                reason: "provider adapter 'antigravity' is disabled because no official public OAuth contract exists",
            };
        } else {
            return ProviderGatePolicy::ApiKey {
                primary_env: "NUDGE_API_KEY",
                fallback_env: Some("ANTIGRAVITY_API_KEY"),
            };
        }
    }

    for entry in PROVIDER_GATE_TABLE {
        if entry.name == name {
            return entry.policy.clone();
        }
    }

    ProviderGatePolicy::Disabled {
        reason: "provider adapter is unknown or disabled",
    }
}
