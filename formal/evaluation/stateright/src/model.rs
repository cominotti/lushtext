// SPDX-License-Identifier: GPL-3.0-or-later

//! The `stateright` [`Model`] over [`env::Journal`].
//!
//! - one process = Kani's `journal_invariants_hold_under_crashes`: every
//!   previous session, `Startup` with either fault, then up to `max_steps`
//!   actions;
//! - two processes = Kani's `a_second_writer_breaks_the_journal_invariants`:
//!   every previous session, fault-free `Startup` of A then B, then up to
//!   `max_steps` actions by either actor.
//!
//! An action that leaves the journal unchanged (a no-op step in the harness)
//! returns `None`: the set of reachable journal states within `max_steps`
//! steps is the same, since a no-op only consumes budget.

use lushtext_core::services::draft_service::journal_core::RestoreEnding;
use stateright::{Model, Property};

use crate::env::{self, Action, IDS, Journal, PreviousId};

#[derive(Clone, Debug)]
pub struct State {
    pub journal: Journal,
    /// Actions taken after the startup(s).
    pub n: usize,
    /// Whether `n` is part of the state's identity (see
    /// [`JournalModel::count_steps`]); constant within one run.
    pub n_in_identity: bool,
}

impl State {
    fn identity(&self) -> (&Journal, Option<usize>) {
        (&self.journal, self.n_in_identity.then_some(self.n))
    }
}

impl core::hash::Hash for State {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.identity().hash(state);
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}

impl Eq for State {}

// `RestoreEnding` does not derive `Hash`; stateright does not need it here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// `false` = process A, `true` = process B.
    pub actor: bool,
    pub action: Action,
    pub id: usize,
    pub fault: bool,
    /// The `kani::any()` ending chosen inside `RestoreApply`, when reached.
    pub ending: Option<RestoreEnding>,
}

#[derive(Clone, Debug)]
pub struct JournalModel {
    pub max_steps: usize,
    pub two_processes: bool,
    /// `true` (the default): the step counter `n` is part of the state's
    /// identity. `false` (`--depth-bound`): `n` still bounds the actions but is
    /// left out of `Hash`/`Eq`, so a journal reached at two depths is one
    /// state carrying the depth of its first visit. That is sound only under
    /// strict BFS (`--threads 1`), where the first visit is at minimal depth.
    pub count_steps: bool,
}

impl JournalModel {
    /// Every previous-session disk, in a deterministic order.
    pub fn previous_sessions() -> Vec<[PreviousId; IDS]> {
        let all: Vec<PreviousId> = PreviousId::all().collect();
        let mut out = Vec::new();
        for &a in &all {
            for &b in &all {
                for &c in &all {
                    out.push([a, b, c]);
                }
            }
        }
        out
    }

    /// The state right after startup, as the harness has it before its loop.
    pub fn start(&self, previous: [PreviousId; IDS], startup_fault: bool) -> Journal {
        let mut journal = Journal::previous_session(previous);
        if self.two_processes {
            journal = journal.with_second_process();
            journal.step_as(false, Action::Startup, 0, false, None);
            journal.step_as(true, Action::Startup, 0, false, None);
        } else {
            journal.step(Action::Startup, 0, startup_fault, None);
        }
        journal
    }
}

impl Model for JournalModel {
    type State = State;
    type Action = Step;

    fn init_states(&self) -> Vec<State> {
        let faults: &[bool] = if self.two_processes {
            &[false]
        } else {
            &[false, true]
        };
        let mut states = Vec::new();
        for previous in Self::previous_sessions() {
            for &fault in faults {
                let journal = self.start(previous, fault);
                let state = State {
                    journal,
                    n: 0,
                    n_in_identity: self.count_steps,
                };
                if !states.contains(&state) {
                    states.push(state);
                }
            }
        }
        states
    }

    fn actions(&self, state: &State, actions: &mut Vec<Step>) {
        if state.n >= self.max_steps {
            return;
        }
        let actors: &[bool] = if self.two_processes {
            &[false, true]
        } else {
            &[false]
        };
        for &actor in actors {
            for action in Action::ALL {
                for id in 0..IDS {
                    for fault in [false, true] {
                        if action == Action::RestoreApply
                            && state.journal.restore_choice_needed(actor, id)
                        {
                            for ending in env::CHOSEN_ENDINGS {
                                actions.push(Step {
                                    actor,
                                    action,
                                    id,
                                    fault,
                                    ending: Some(ending),
                                });
                            }
                        } else {
                            actions.push(Step {
                                actor,
                                action,
                                id,
                                fault,
                                ending: None,
                            });
                        }
                    }
                }
            }
        }
    }

    fn next_state(&self, last: &State, step: Step) -> Option<State> {
        let mut journal = last.journal.clone();
        journal.step_as(step.actor, step.action, step.id, step.fault, step.ending);
        if journal == last.journal {
            return None;
        }
        Some(State {
            journal,
            n: last.n + 1,
            n_in_identity: self.count_steps,
        })
    }

    fn properties(&self) -> Vec<Property<Self>> {
        vec![
            Property::always("S1", |_, s: &State| s.journal.acceptance_durability_holds()),
            Property::always("S2", |_, s: &State| !s.journal.cleanup_unsafe),
            Property::always("S3", |_, s: &State| s.journal.delete_ordering_holds()),
            Property::always("S4", |_, s: &State| s.journal.trust_holds()),
            Property::always("NoBodyWithoutEntry", |_, s: &State| {
                !s.journal.body_without_entry
            }),
        ]
    }
}
