// cmhm_core/src/authority.rs
//
// Phase 2 — Provenance-class authority policy.
//
// `clamp_authority(authority, provenance)` enforces per-class bounds so a caller
// cannot assert a higher trust level than their declared provenance class permits.
// This prevents authority forgery (e.g., `authority=1.0, provenance=""` is clamped
// to the neutral ceiling 0.70).

/// Caller role in the ingestion context.
///
/// Enforces source-trust boundaries: ordinary agent/tool callers cannot forge
/// high-trust provenance (`"user_explicit"`, `"corrected"`) or bypass authority ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallerRole {
    /// Direct user interaction or authenticated administrator.
    /// Authorized to assign any provenance class and authority up to 1.00.
    User,
    /// Automated system/pipeline with verified credentials.
    System,
    /// Standard AI agent, tool executor, or unauthenticated caller.
    /// Cannot claim `"user_explicit"` or `"corrected"`. Claims are downgraded to `"inferred"`.
    Agent,
}

impl Default for CallerRole {
    fn default() -> Self {
        Self::Agent
    }
}

impl CallerRole {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "user" | "admin" => Self::User,
            "system" => Self::System,
            _ => Self::Agent,
        }
    }

    /// True if caller possesses credentials to submit high-trust claims.
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::User | Self::System)
    }
}

/// Returns the allowed `[min, max]` authority range for a provenance class string.
///
/// | provenance            | range        | semantics                                |
/// |-----------------------|--------------|------------------------------------------|
/// | `""`  (empty/unknown) | [0.30, 0.70] | neutral band — source unspecified        |
/// | `"inferred"`          | [0.20, 0.60] | agent-generated inference                |
/// | `"external_feed"`     | [0.30, 0.70] | external data source, unverified         |
/// | `"user_explicit"`     | [0.00, 1.00] | direct user instruction, any level       |
/// | `"corrected"`         | [0.70, 1.00] | explicit user correction, floor 0.70     |
/// | `"contradicted"`      | [0.00, 0.30] | explicitly marked wrong, ceiling 0.30    |
///
/// Unknown strings (not in the table above) are treated as `""` → neutral band.
pub fn authority_range(provenance: &str) -> (f32, f32) {
    match provenance {
        ""              => (0.30, 0.70),
        "inferred"      => (0.20, 0.60),
        "external_feed" => (0.30, 0.70),
        "user_explicit" => (0.00, 1.00),
        "corrected"     => (0.70, 1.00),
        "contradicted"  => (0.00, 0.30),
        _               => (0.30, 0.70), // unknown class → neutral band
    }
}

/// Clamp `authority` ∈ [0, 1] to the allowed range for the given provenance class.
pub fn clamp_authority(authority: f32, provenance: &str) -> f32 {
    let (min, max) = authority_range(provenance);
    authority.clamp(min, max)
}

/// Authenticate caller credentials and clamp authority to authorized class bounds.
///
/// Authenticate caller credentials and clamp authority to authorized class bounds.
///
/// Security Invariants:
/// - Unauthenticated / ordinary callers (`CallerRole::Agent`) CANNOT exceed authority ceiling `0.60`.
/// - For `CallerRole::Agent`, any claim other than `"contradicted"` (e.g. `"user_explicit"`, `"corrected"`,
///   `"external_feed"`, unknown strings, or casing/whitespace variants) normalizes to `"inferred"`
///   with authority strictly clamped to `[0.20, 0.60]`.
/// - If `CallerRole::Agent` claims `"contradicted"`, authority is clamped to `[0.00, 0.30]`.
/// - Privileged callers (`CallerRole::User`, `CallerRole::System`) can utilize the full range
///   authorized for their requested provenance class.
pub fn authenticate_and_clamp(
    authority: f32,
    provenance: &str,
    caller_role: CallerRole,
) -> (f32, String) {
    let norm = provenance.trim().to_ascii_lowercase();

    let (effective_prov, clamped_auth) = match caller_role {
        CallerRole::Agent => {
            // Unauthenticated agent: strictly enforce agent ceiling <= 0.60
            match norm.as_str() {
                "contradicted" => {
                    let clamped = authority.clamp(0.00, 0.30);
                    ("contradicted".to_string(), clamped)
                }
                // Any other claim (user_explicit, corrected, external_feed, unknown, whitespace)
                // normalizes to "inferred" with authority strictly capped to [0.20, 0.60]
                _ => {
                    let clamped = authority.clamp(0.20, 0.60);
                    ("inferred".to_string(), clamped)
                }
            }
        }
        CallerRole::User | CallerRole::System => {
            let prov = match norm.as_str() {
                "user_explicit" => "user_explicit",
                "corrected"     => "corrected",
                "contradicted"  => "contradicted",
                "inferred"      => "inferred",
                "external_feed" => "external_feed",
                _               => "",
            };
            let (floor, ceiling) = authority_range(prov);
            let clamped = authority.clamp(floor, ceiling);
            (prov.to_string(), clamped)
        }
    };

    (clamped_auth, effective_prov)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_provenance_clamps_to_neutral_band() {
        assert_eq!(clamp_authority(1.0, ""), 0.70);
        assert_eq!(clamp_authority(0.0, ""), 0.30);
        assert!((clamp_authority(0.5, "") - 0.5).abs() < 1e-6);
    }

    #[test]
    fn user_explicit_allows_full_range() {
        assert!((clamp_authority(1.0, "user_explicit") - 1.0).abs() < 1e-6);
        assert!((clamp_authority(0.0, "user_explicit") - 0.0).abs() < 1e-6);
    }

    #[test]
    fn corrected_has_floor() {
        assert!((clamp_authority(0.0, "corrected") - 0.70).abs() < 1e-6);
        assert!((clamp_authority(1.0, "corrected") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn contradicted_has_ceiling() {
        assert!((clamp_authority(1.0, "contradicted") - 0.30).abs() < 1e-6);
        assert!((clamp_authority(0.0, "contradicted") - 0.0).abs() < 1e-6);
    }

    #[test]
    fn unknown_provenance_treated_as_empty() {
        assert_eq!(clamp_authority(1.0, "forge_class"), 0.70);
        assert_eq!(clamp_authority(1.0, "unknown_xyz"), 0.70);
    }

    #[test]
    fn inferred_is_capped_at_0_60() {
        assert!((clamp_authority(1.0, "inferred") - 0.60).abs() < 1e-6);
        assert!((clamp_authority(0.0, "inferred") - 0.20).abs() < 1e-6);
    }

    #[test]
    fn range_symmetric_around_0_5_for_neutral() {
        let (min, max) = authority_range("");
        assert!((max - min - 0.40).abs() < 1e-6, "neutral band should be 0.40 wide");
    }

    #[test]
    fn untrusted_agent_caller_claiming_user_explicit_is_downgraded() {
        let (auth, prov) = authenticate_and_clamp(1.0, "user_explicit", CallerRole::Agent);
        assert_eq!(prov, "inferred", "Agent caller must be downgraded to 'inferred'");
        assert_eq!(auth, 0.60, "Agent caller claiming 1.0 must be clamped to inferred ceiling 0.60");
    }

    #[test]
    fn untrusted_agent_caller_claiming_corrected_is_downgraded() {
        let (auth, prov) = authenticate_and_clamp(0.95, "corrected", CallerRole::Agent);
        assert_eq!(prov, "inferred", "Agent caller must be downgraded to 'inferred'");
        assert_eq!(auth, 0.60, "Authority must be clamped to 0.60");
    }

    #[test]
    fn untrusted_agent_caller_claiming_external_feed_is_capped_to_0_60() {
        let (auth, prov) = authenticate_and_clamp(0.90, "external_feed", CallerRole::Agent);
        assert_eq!(prov, "inferred", "Agent caller claiming external_feed must normalize to 'inferred'");
        assert_eq!(auth, 0.60, "Agent claiming external_feed must not exceed 0.60 ceiling");
    }

    #[test]
    fn untrusted_agent_caller_whitespace_casing_variants_are_normalized_and_capped() {
        let (auth, prov) = authenticate_and_clamp(1.0, "  USER_EXPLICIT  \n", CallerRole::Agent);
        assert_eq!(prov, "inferred");
        assert_eq!(auth, 0.60);

        let (auth2, prov2) = authenticate_and_clamp(0.85, "  CoRReCTed\t", CallerRole::Agent);
        assert_eq!(prov2, "inferred");
        assert_eq!(auth2, 0.60);
    }

    #[test]
    fn untrusted_agent_caller_unknown_strings_fail_closed_to_inferred_0_60() {
        let (auth, prov) = authenticate_and_clamp(0.99, "arbitrary_attacker_class", CallerRole::Agent);
        assert_eq!(prov, "inferred");
        assert_eq!(auth, 0.60);
    }

    #[test]
    fn untrusted_agent_caller_contradicted_is_capped_at_0_30() {
        let (auth, prov) = authenticate_and_clamp(0.80, "contradicted", CallerRole::Agent);
        assert_eq!(prov, "contradicted");
        assert_eq!(auth, 0.30);
    }

    #[test]
    fn trusted_user_caller_preserves_user_explicit_and_full_authority() {
        let (auth, prov) = authenticate_and_clamp(1.0, "user_explicit", CallerRole::User);
        assert_eq!(prov, "user_explicit");
        assert_eq!(auth, 1.00);
    }

    #[test]
    fn trusted_user_caller_preserves_corrected() {
        let (auth, prov) = authenticate_and_clamp(0.85, "corrected", CallerRole::User);
        assert_eq!(prov, "corrected");
        assert_eq!(auth, 0.85);
    }
}


