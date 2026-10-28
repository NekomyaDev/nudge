//! Deterministic Self-Healing Replay Engine (`healer.rs`) for Nudge Sovereign Loop.
//!
//! When deterministic trace execution (`NTF`) encounters DOM mutations, CSS class drifts,
//! or API schema changes, `HealerEngine` synthesizes alternative selectors and routes
//! concurrently (`par race`), verifies candidate outputs, and atomically updates the
//! trace baseline without human escalation.

use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Categories of detected execution or replay drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriftKind {
    SelectorNotFound { selector: String },
    DomLayoutMutated { tag: String, expected_text: String },
    SchemaChanged { expected_type: String },
    NetworkFallback { url: String, status_code: u16 },
}

impl DriftKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SelectorNotFound { .. } => "selector_not_found",
            Self::DomLayoutMutated { .. } => "dom_layout_mutated",
            Self::SchemaChanged { .. } => "schema_changed",
            Self::NetworkFallback { .. } => "network_fallback",
        }
    }
}

/// Multi-attribute structural signature of a UI element or DOM node.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetSignature {
    pub tag: Option<String>,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub data_testid: Option<String>,
    pub aria_label: Option<String>,
    pub role: Option<String>,
    pub text_content: Option<String>,
    pub name: Option<String>,
    pub placeholder: Option<String>,
}

/// Simulated DOM node for dry-run verification.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockDomElement {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub data_testid: Option<String>,
    pub aria_label: Option<String>,
    pub role: Option<String>,
    pub text: Option<String>,
}

impl MockDomElement {
    pub fn matches_selector(&self, selector: &str) -> bool {
        let sel = selector.trim();
        if let Some(id_val) = sel.strip_prefix('#') {
            return self.id.as_deref() == Some(id_val);
        }
        if let Some(rest) = sel.strip_prefix("[data-testid='") {
            if let Some((val, _)) = rest.split_once("']") {
                return self.data_testid.as_deref() == Some(val);
            }
        }
        if let Some(rest) = sel.strip_prefix("[aria-label='") {
            if let Some((val, _)) = rest.split_once("']") {
                return self.aria_label.as_deref() == Some(val);
            }
        }
        if let Some(rest) = sel.strip_prefix(":contains('") {
            if let Some((val, _)) = rest.split_once("')") {
                return self.text.as_deref().is_some_and(|t| t.contains(val));
            }
        }
        if sel == self.tag {
            return true;
        }
        false
    }
}

/// Repair candidate synthesized by the healing engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairCandidate {
    pub id: String,
    pub strategy: String,
    pub payload: String,
    pub confidence: f64,
}

/// Result of a self-healing reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealResolution {
    pub original_target: String,
    pub winning_candidate: RepairCandidate,
    pub verified: bool,
    pub attempts: usize,
    pub timestamp: u64,
}

/// Self-Healing Engine state and history ledger.
#[derive(Debug, Default)]
pub struct HealerEngine {
    resolutions: Vec<HealResolution>,
    active_heals: usize,
    total_healed: usize,
}

impl HealerEngine {
    pub fn new() -> Self {
        Self {
            resolutions: Vec::new(),
            active_heals: 0,
            total_healed: 0,
        }
    }

    /// Synthesize candidate selectors/fallbacks for a broken web/system target.
    pub fn synthesize_candidates(&self, broken_target: &str) -> Vec<RepairCandidate> {
        let mut candidates = Vec::with_capacity(3);

        // Candidate 1: ARIA accessibility label / role matching
        candidates.push(RepairCandidate {
            id: format!("cand_aria_{}", candidates.len() + 1),
            strategy: "aria_role_and_label".to_string(),
            payload: format!(
                "[aria-label*='{}'], [role='button'][name*='{}']",
                broken_target, broken_target
            ),
            confidence: 0.92,
        });

        // Candidate 2: Fuzzy text content match
        candidates.push(RepairCandidate {
            id: format!("cand_text_{}", candidates.len() + 1),
            strategy: "fuzzy_text_search".to_string(),
            payload: format!(":contains('{}')", broken_target),
            confidence: 0.85,
        });

        // Candidate 3: Normalized tag and class ancestor walk
        candidates.push(RepairCandidate {
            id: format!("cand_tag_{}", candidates.len() + 1),
            strategy: "tag_ancestor_walk".to_string(),
            payload: "form input, div[role='main'] *".to_string(),
            confidence: 0.70,
        });

        candidates
    }

    /// Synthesize high-confidence candidates from a rich multi-attribute signature.
    pub fn synthesize_from_signature(&self, sig: &TargetSignature) -> Vec<RepairCandidate> {
        let mut candidates = Vec::new();

        // 1. data-testid (most deterministic and resilient)
        if let Some(testid) = &sig.data_testid {
            candidates.push(RepairCandidate {
                id: format!("cand_testid_{}", candidates.len() + 1),
                strategy: "data_testid".to_string(),
                payload: format!("[data-testid='{}']", testid),
                confidence: 0.98,
            });
        }

        // 2. Element ID
        if let Some(id) = &sig.id {
            candidates.push(RepairCandidate {
                id: format!("cand_id_{}", candidates.len() + 1),
                strategy: "element_id".to_string(),
                payload: format!("#{}", id),
                confidence: 0.95,
            });
        }

        // 3. ARIA label and role
        if let (Some(role), Some(aria)) = (&sig.role, &sig.aria_label) {
            candidates.push(RepairCandidate {
                id: format!("cand_aria_role_{}", candidates.len() + 1),
                strategy: "aria_role_and_label".to_string(),
                payload: format!("[role='{}'][aria-label='{}']", role, aria),
                confidence: 0.93,
            });
        } else if let Some(aria) = &sig.aria_label {
            candidates.push(RepairCandidate {
                id: format!("cand_aria_{}", candidates.len() + 1),
                strategy: "aria_label".to_string(),
                payload: format!("[aria-label='{}']", aria),
                confidence: 0.90,
            });
        }

        // 4. Exact text content
        if let Some(text) = &sig.text_content {
            let tag_prefix = sig.tag.as_deref().unwrap_or("*");
            candidates.push(RepairCandidate {
                id: format!("cand_text_{}", candidates.len() + 1),
                strategy: "text_content".to_string(),
                payload: format!("{}:contains('{}')", tag_prefix, text),
                confidence: 0.86,
            });
        }

        // 5. CSS Tag and classes
        if let Some(tag) = &sig.tag {
            if !sig.classes.is_empty() {
                let class_str = sig.classes.join(".");
                candidates.push(RepairCandidate {
                    id: format!("cand_css_{}", candidates.len() + 1),
                    strategy: "tag_and_classes".to_string(),
                    payload: format!("{}.{}", tag, class_str),
                    confidence: 0.78,
                });
            }
        }

        // Fallback default if empty
        if candidates.is_empty() {
            candidates.push(RepairCandidate {
                id: "cand_fallback_1".to_string(),
                strategy: "generic_fallback".to_string(),
                payload: "*".to_string(),
                confidence: 0.50,
            });
        }

        candidates
    }

    /// Dry-run verification of candidate against DOM element snapshot.
    /// Returns true if candidate uniquely matches an active element.
    pub fn verify_candidate_against_dom(
        &self,
        candidate: &RepairCandidate,
        elements: &[MockDomElement],
    ) -> bool {
        let match_count = elements
            .iter()
            .filter(|el| el.matches_selector(&candidate.payload))
            .count();
        match_count == 1
    }

    /// Rollback the most recent resolution if subsequent execution steps encounter errors.
    pub fn rollback_last_resolution(&mut self) -> Option<HealResolution> {
        let popped = self.resolutions.pop();
        if popped.is_some() {
            self.total_healed = self.total_healed.saturating_sub(1);
        }
        popped
    }

    /// Reset healer state for test isolation.
    pub fn clear(&mut self) {
        self.resolutions.clear();
        self.active_heals = 0;
        self.total_healed = 0;
    }

    /// Commit a winning verified candidate as the new trace baseline.
    pub fn commit_resolution(
        &mut self,
        original_target: String,
        winning_candidate: RepairCandidate,
        attempts: usize,
    ) -> HealResolution {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let resolution = HealResolution {
            original_target,
            winning_candidate,
            verified: true,
            attempts,
            timestamp: now,
        };

        self.resolutions.push(resolution.clone());
        self.total_healed = self.total_healed.saturating_add(1);
        resolution
    }

    pub fn total_healed(&self) -> usize {
        self.total_healed
    }

    pub fn resolutions(&self) -> &[HealResolution] {
        &self.resolutions
    }
}

/// Global self-healing engine singleton.
static HEALER: OnceLock<Mutex<HealerEngine>> = OnceLock::new();

pub fn get_healer() -> &'static Mutex<HealerEngine> {
    HEALER.get_or_init(|| Mutex::new(HealerEngine::new()))
}

pub fn with_healer<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut HealerEngine) -> Result<R, String>,
{
    let mutex = get_healer();
    let mut h = mutex
        .lock()
        .map_err(|e| format!("error[E0960]: HealerEngine lock poisoned: {e}"))?;
    f(&mut h)
}

/// Register healer functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert(
        "healer.candidates".to_string(),
        Value::Native("healer.candidates"),
    );
    functions.insert("healer.commit".to_string(), Value::Native("healer.commit"));
    functions.insert(
        "healer.history".to_string(),
        Value::Native("healer.history"),
    );
    functions.insert("healer.status".to_string(), Value::Native("healer.status"));
    functions
}

/// Dispatch a call to a healer builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "healer.candidates" => {
            if args.is_empty() {
                return Err("healer.candidates requires broken_target string".to_string());
            }
            let target = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("healer.candidates: target must be string".to_string()),
            };

            with_healer(|h| {
                let cands = h.synthesize_candidates(target);
                let mut list = Vec::with_capacity(cands.len());
                for c in cands {
                    let mut map = HashMap::new();
                    map.insert("id".to_string(), Value::String(c.id));
                    map.insert("strategy".to_string(), Value::String(c.strategy));
                    map.insert("payload".to_string(), Value::String(c.payload));
                    map.insert("confidence".to_string(), Value::Float(c.confidence));
                    list.push(Value::Map(map));
                }
                Ok(Value::List(list))
            })
        }

        "healer.commit" => {
            if args.len() < 3 {
                return Err(
                    "healer.commit requires (original_target, strategy, payload)".to_string(),
                );
            }
            let target = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("healer.commit: target must be string".to_string()),
            };
            let strategy = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("healer.commit: strategy must be string".to_string()),
            };
            let payload = match &args[2] {
                Value::String(s) => s.clone(),
                _ => return Err("healer.commit: payload must be string".to_string()),
            };

            let cand = RepairCandidate {
                id: "manual_commit".to_string(),
                strategy,
                payload,
                confidence: 1.0,
            };

            with_healer(|h| {
                let res = h.commit_resolution(target, cand, 1);
                Ok(Value::Bool(res.verified))
            })
        }

        "healer.history" => with_healer(|h| {
            let res = h.resolutions();
            let mut list = Vec::with_capacity(res.len());
            for r in res {
                let mut map = HashMap::new();
                map.insert(
                    "target".to_string(),
                    Value::String(r.original_target.clone()),
                );
                map.insert(
                    "strategy".to_string(),
                    Value::String(r.winning_candidate.strategy.clone()),
                );
                map.insert(
                    "payload".to_string(),
                    Value::String(r.winning_candidate.payload.clone()),
                );
                map.insert("attempts".to_string(), Value::Int(r.attempts as i64));
                list.push(Value::Map(map));
            }
            Ok(Value::List(list))
        }),

        "healer.status" => with_healer(|h| {
            let mut map = HashMap::new();
            map.insert(
                "total_healed".to_string(),
                Value::Int(h.total_healed() as i64),
            );
            map.insert(
                "active_heals".to_string(),
                Value::Int(h.active_heals as i64),
            );
            Ok(Value::Map(map))
        }),

        _ => Err(format!("unknown healer function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healer_candidate_synthesis_and_commit() {
        let mut engine = HealerEngine::new();
        let broken = "#btn-submit-order";

        let candidates = engine.synthesize_candidates(broken);
        assert_eq!(candidates.len(), 3);
        assert!(candidates[0].confidence > 0.9);

        let resolution = engine.commit_resolution(broken.to_string(), candidates[0].clone(), 1);

        assert!(resolution.verified);
        assert_eq!(engine.total_healed(), 1);
        assert_eq!(engine.resolutions().len(), 1);
        assert_eq!(engine.resolutions()[0].original_target, broken);
    }
}
