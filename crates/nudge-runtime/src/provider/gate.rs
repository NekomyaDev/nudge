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
    /// Enabled via RFC 8628 Device Flow.
    DeviceFlow {
        issuer: &'static str,
        token_endpoint: &'static str,
        client_id: &'static str,
    },
    /// Enabled via OAuth 2.0 Loopback flow (RFC 8252 installed-app flow with PKCE).
    LoopbackOAuth {
        auth_url: &'static str,
        token_endpoint: &'static str,
        client_id: &'static str,
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
        policy: ProviderGatePolicy::LoopbackOAuth {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_endpoint: "https://oauth2.googleapis.com/token",
            client_id: "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
        },
        oauth_enabled: true,
    },
    ProviderGateEntry {
        name: "github_copilot",
        policy: ProviderGatePolicy::DeviceFlow {
            issuer: "https://github.com/login/device/code",
            token_endpoint: "https://github.com/login/oauth/access_token",
            client_id: "Iv1.b507a08c87ecfe81",
        },
        oauth_enabled: true,
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
        // PACK 5 Public Verification Gate — keep antigravity FAIL-CLOSED by default.
        // We found NO official proof that this OAuth client and its model endpoint may
        // be used from an independent third-party client. A mock OAuth test and green CI
        // do NOT prove live-account access or usage permission, so the connection stays
        // disabled until permission is verified. Consumers (e.g. Nudge Studio) must not
        // rely on this provider until then. Explicit, user-asserted opt-in only:
        let opted_in = std::env::var("NUDGE_ENABLE_ANTIGRAVITY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !opted_in {
            return ProviderGatePolicy::Disabled {
                reason: "provider 'antigravity' is fail-closed: official third-party usage permission for this OAuth client/model endpoint has not been proven (a mock OAuth test or green CI does not prove live access). Set NUDGE_ENABLE_ANTIGRAVITY=1 to explicitly opt in at your own risk once permission is verified.",
            };
        }
        let has_first_party_key = std::env::var("NUDGE_API_KEY")
            .or_else(|_| std::env::var("ANTIGRAVITY_API_KEY"))
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false);
        if has_first_party_key {
            return ProviderGatePolicy::ApiKey {
                primary_env: "NUDGE_API_KEY",
                fallback_env: Some("ANTIGRAVITY_API_KEY"),
            };
        } else {
            return ProviderGatePolicy::LoopbackOAuth {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_endpoint: "https://oauth2.googleapis.com/token",
                client_id:
                    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
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
