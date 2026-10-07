//! Context Lifecycle Finite State Machine.
//!
//! This module contains exactly one logical thing: the transition table.
//! It is pure — no I/O, no registry access, no side effects.
//! Every caller that changes a context state must pass through here.
//!
//! Full transition map (from Context OS Spec v1.0):
//!
//!   Creating  -> Active      (engine loaded successfully)
//!   Creating  -> Deleted     (creation failed, rollback)
//!   Active    -> Idle        (idle_timeout_secs elapsed)
//!   Active    -> Sleeping    (LRU eviction, snapshot saved)
//!   Active    -> Recovering  (crash detected on load)
//!   Active    -> Deleted     (user delete command)
//!   Idle      -> Active      (new request arrives)
//!   Idle      -> Sleeping    (sleep_timeout_secs elapsed)
//!   Idle      -> Deleted     (user delete command)
//!   Sleeping  -> Active      (clean load, no WAL replay needed)
//!   Sleeping  -> Recovering  (WAL replay needed on load)
//!   Sleeping  -> Deleted     (user delete command)
//!   Recovering-> Active      (WAL replay or migration complete)
//!   Recovering-> Sleeping    (schema mismatch fail-closed: data intact, UUID preserved)
//!   Recovering-> Deleted     (replay failed, unrecoverable)
//!   Archived  -> Sleeping    (re-activate from cold storage)
//!   Deleted   -> (nothing)   TERMINAL STATE

use crate::context::ContextState;

/// Error returned when a state transition is not permitted by the FSM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionError {
    pub from: ContextState,
    pub to:   ContextState,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "E_INVALID_TRANSITION: {} -> {} is not a legal state transition",
            self.from, self.to
        )
    }
}

impl std::error::Error for TransitionError {}

/// The single source of truth for context state transitions.
///
/// Returns `Ok(())` if the transition is legal.
/// Returns `Err(TransitionError)` if the transition is not permitted.
///
/// This function is the ONLY place allowed to decide whether a transition is valid.
pub fn validate(from: ContextState, to: ContextState) -> Result<(), TransitionError> {
    if is_valid(from, to) {
        Ok(())
    } else {
        Err(TransitionError { from, to })
    }
}

/// Pure boolean form — useful for routing guards, assertions, and tests.
pub fn is_valid(from: ContextState, to: ContextState) -> bool {
    use ContextState::*;
    matches!(
        (from, to),
        (Creating,   Active)
        | (Creating,   Deleted)
        | (Active,     Idle)
        | (Active,     Sleeping)
        | (Active,     Recovering)
        | (Active,     Deleted)
        | (Idle,       Active)
        | (Idle,       Sleeping)
        | (Idle,       Deleted)
        | (Sleeping,   Active)
        | (Sleeping,   Recovering)
        | (Sleeping,   Deleted)
        | (Recovering, Active)
        | (Recovering, Sleeping)  // schema mismatch fail-closed: data intact, UUID preserved
        | (Recovering, Deleted)
        | (Archived,   Sleeping)
        // Deleted is terminal: no outgoing transitions
    )
}

/// All states reachable from `state` via a single legal transition.
/// Useful for documentation, debugging, and testing.
pub fn successors(state: ContextState) -> Vec<ContextState> {
    use ContextState::*;
    let all = [Creating, Active, Idle, Sleeping, Recovering, Archived, Deleted];
    all.iter()
        .copied()
        .filter(|&target| is_valid(state, target))
        .collect()
}

/// Returns true if this state can serve user requests (router Sa component).
pub fn is_serviceable(state: ContextState) -> bool {
    matches!(state, ContextState::Active | ContextState::Idle)
}

// =============================================================================
// Unit tests — exhaustive, zero I/O, microsecond-fast
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use ContextState::*;

    // --- Valid transitions ---------------------------------------------------

    #[test] fn creating_to_active()      { assert!(is_valid(Creating,   Active));     }
    #[test] fn creating_to_deleted()     { assert!(is_valid(Creating,   Deleted));    }
    #[test] fn active_to_idle()          { assert!(is_valid(Active,      Idle));       }
    #[test] fn active_to_sleeping()      { assert!(is_valid(Active,      Sleeping));  }
    #[test] fn active_to_recovering()    { assert!(is_valid(Active,      Recovering));}
    #[test] fn active_to_deleted()       { assert!(is_valid(Active,      Deleted));   }
    #[test] fn idle_to_active()          { assert!(is_valid(Idle,        Active));     }
    #[test] fn idle_to_sleeping()        { assert!(is_valid(Idle,        Sleeping));  }
    #[test] fn idle_to_deleted()         { assert!(is_valid(Idle,        Deleted));   }
    #[test] fn sleeping_to_active()      { assert!(is_valid(Sleeping,    Active));     }
    #[test] fn sleeping_to_recovering()  { assert!(is_valid(Sleeping,    Recovering));}
    #[test] fn sleeping_to_deleted()     { assert!(is_valid(Sleeping,    Deleted));   }
    #[test] fn recovering_to_active()    { assert!(is_valid(Recovering,  Active));     }
    #[test] fn recovering_to_deleted()   { assert!(is_valid(Recovering,  Deleted));   }
    #[test] fn archived_to_sleeping()    { assert!(is_valid(Archived,    Sleeping));  }

    // --- Invalid transitions ------------------------------------------------

    #[test] fn creating_to_idle()        { assert!(!is_valid(Creating,   Idle));       }
    #[test] fn creating_to_sleeping()    { assert!(!is_valid(Creating,   Sleeping));  }
    #[test] fn creating_to_recovering()  { assert!(!is_valid(Creating,   Recovering));}
    #[test] fn creating_to_archived()    { assert!(!is_valid(Creating,   Archived));  }

    #[test] fn active_to_creating()      { assert!(!is_valid(Active,     Creating));  }
    #[test] fn active_to_archived()      { assert!(!is_valid(Active,     Archived));  }

    #[test] fn idle_to_creating()        { assert!(!is_valid(Idle,       Creating));  }
    #[test] fn idle_to_recovering()      { assert!(!is_valid(Idle,       Recovering));}
    #[test] fn idle_to_archived()        { assert!(!is_valid(Idle,       Archived));  }

    #[test] fn sleeping_to_idle()        { assert!(!is_valid(Sleeping,   Idle));       }
    #[test] fn sleeping_to_creating()    { assert!(!is_valid(Sleeping,   Creating));  }
    #[test] fn sleeping_to_archived()    { assert!(!is_valid(Sleeping,   Archived));  }

    #[test] fn recovering_to_idle()      { assert!(!is_valid(Recovering, Idle));       }
    #[test] fn recovering_to_sleeping()  { assert!( is_valid(Recovering, Sleeping));  } // schema mismatch rollback
    #[test] fn recovering_to_archived()  { assert!(!is_valid(Recovering, Archived));  }

    #[test] fn archived_to_active()      { assert!(!is_valid(Archived,   Active));    }
    #[test] fn archived_to_deleted()     { assert!(!is_valid(Archived,   Deleted));   }

    // --- Terminal: Deleted has NO outgoing transitions ----------------------

    #[test] fn deleted_to_creating()     { assert!(!is_valid(Deleted,    Creating));  }
    #[test] fn deleted_to_active()       { assert!(!is_valid(Deleted,    Active));    }
    #[test] fn deleted_to_idle()         { assert!(!is_valid(Deleted,    Idle));       }
    #[test] fn deleted_to_sleeping()     { assert!(!is_valid(Deleted,    Sleeping));  }
    #[test] fn deleted_to_recovering()   { assert!(!is_valid(Deleted,    Recovering));}
    #[test] fn deleted_to_archived()     { assert!(!is_valid(Deleted,    Archived));  }
    #[test] fn deleted_to_deleted()      { assert!(!is_valid(Deleted,    Deleted));   }

    // --- Self-transitions must all be invalid --------------------------------

    #[test] fn no_self_creating()        { assert!(!is_valid(Creating,   Creating));  }
    #[test] fn no_self_active()          { assert!(!is_valid(Active,     Active));    }
    #[test] fn no_self_idle()            { assert!(!is_valid(Idle,       Idle));       }
    #[test] fn no_self_sleeping()        { assert!(!is_valid(Sleeping,   Sleeping));  }
    #[test] fn no_self_recovering()      { assert!(!is_valid(Recovering, Recovering));}
    #[test] fn no_self_archived()        { assert!(!is_valid(Archived,   Archived));  }
    #[test] fn no_self_deleted()         { assert!(!is_valid(Deleted,    Deleted));   }

    // --- Successor sets -----------------------------------------------------

    #[test]
    fn creating_has_exactly_two_successors() {
        let s = successors(Creating);
        assert_eq!(s.len(), 2);
        assert!(s.contains(&Active));
        assert!(s.contains(&Deleted));
    }

    #[test]
    fn deleted_has_zero_successors() {
        assert!(successors(Deleted).is_empty());
    }

    #[test]
    fn active_has_four_successors() {
        let s = successors(Active);
        assert_eq!(s.len(), 4);
    }

    // --- Serviceability -----------------------------------------------------

    #[test] fn active_is_serviceable()     { assert!(is_serviceable(Active));    }
    #[test] fn idle_is_serviceable()       { assert!(is_serviceable(Idle));      }
    #[test] fn sleeping_not_serviceable()  { assert!(!is_serviceable(Sleeping)); }
    #[test] fn creating_not_serviceable()  { assert!(!is_serviceable(Creating)); }
    #[test] fn deleted_not_serviceable()   { assert!(!is_serviceable(Deleted));  }
    #[test] fn recovering_not_serviceable(){ assert!(!is_serviceable(Recovering));}
    #[test] fn archived_cannot_delete_directly() { assert!(!is_valid(Archived, Deleted)); }

}
