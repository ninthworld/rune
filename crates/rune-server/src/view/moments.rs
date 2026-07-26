//! Deriving **presentation moments** from the receiver-safe game log (issue #594).
//!
//! This is the whole of the server's moment vocabulary, and it is a *projection of a
//! projection*: the engine's history becomes [`GameLogEntry`]s in [`super::log`], and
//! those entries become [`MomentDraft`]s here. Nothing in this module reads the game
//! state, diffs two boards, or decides what is legal — a moment that cannot be read
//! off an event the engine already stated is a moment this server does not produce.
//! That is the point. The client is forbidden to infer ordering from board diffs
//! (`AGENTS.md`: zero game logic in the client), so a server that manufactured moments
//! by diffing would only be moving the same invention one layer down.
//!
//! **Derived, never a second source of truth.** The log is the authoritative record
//! (ADR 0021); this is a display vocabulary over the same events, which is why the two
//! can never disagree about what happened. Where the vocabularies differ they differ
//! deliberately: a mulligan or a kept hand is log-only (nothing on the board moves),
//! while a zone move is moment-only (the log names the *resolution* that caused it, and
//! the travel is what a client animates).
//!
//! **Counter changes are absent on purpose.** The engine emits no counter-change event
//! and no general zone-change event, so there is nothing here to map. See
//! `docs/protocol.md`; the honest answer is a missing moment, not a guessed one.

use std::collections::BTreeMap;

use rune_protocol::{MomentKind, MomentObject, MomentZone};

use super::*;

/// Where the game was when a moment happened — the position a moment is *labelled*
/// with, which is not where the game *is* by the time the moment is broadcast.
///
/// A settle applies many actions before a single view goes out, so the view's own
/// `turn`/`phase` have already moved on. The log states a position only when it
/// changes (`step_changed`), so the derivation carries one forward: seeded from the
/// state as the batch opened, then advanced by each `step_changed` it walks past.
/// Every moment in between is labelled with the position in force at the time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MomentPosition {
    /// The turn number in force.
    pub(crate) turn: u32,
    /// The step in force.
    pub(crate) phase: Phase,
    /// Whose turn it is — carried only so a turn *boundary* can be recognized when the
    /// turn number does not change (a repeated turn number with a new active player is
    /// still a new turn, and the client is not allowed to work that out itself).
    pub(crate) active_player: String,
}

impl MomentPosition {
    /// The position a state is currently at.
    pub(crate) fn of(state: &GameState) -> Self {
        Self {
            turn: state.turn,
            phase: phase_of(state.step),
            active_player: player_id(state.active_player),
        }
    }
}

/// The **retained public faces** a derivation may attach to its objects, borrowed from
/// the room's presentation trail.
///
/// Both maps are keyed by the same opaque entity id the log uses, and both contain
/// **public zones only** — the room never inserts a hand or library face (see
/// `crate::room::presentation`). This type carries no redaction logic of its own for
/// exactly that reason: redaction happens where the observation is made, so no caller
/// here can leak a face that was never cached.
#[derive(Clone, Copy)]
pub(crate) struct RetainedObjects<'a> {
    /// Retained public card faces by entity id.
    pub(crate) faces: &'a BTreeMap<String, CardView>,
    /// The zone each object was observed in when the current batch **opened** — the
    /// only honest answer to "where did this come from" for a move whose event does
    /// not name its origin (CR 903.9a).
    pub(crate) origins: &'a BTreeMap<String, MomentZone>,
}

/// One moment before it has an identity: what to show and where it happened, with its
/// cause named **by position in this same derivation** rather than by id.
///
/// Ids belong to the room-wide trail, which is the only thing that can allocate them
/// monotonically; a derivation is a pure function that does not know them. So a cause
/// is an index here and the trail resolves it, which also means an aggregated moment's
/// dependents automatically point at the surviving moment.
pub(crate) struct MomentDraft {
    /// What happened.
    pub(crate) kind: MomentKind,
    /// The turn it happened on.
    pub(crate) turn: u32,
    /// The step it happened at.
    pub(crate) phase: Phase,
    /// The index, within this derivation's output, of the draft that caused this one.
    /// `None` for a root moment or one whose cause the server cannot state.
    pub(crate) cause: Option<usize>,
}

/// Derive the moments for every log entry after `after`, advancing `position` as the
/// walk crosses step boundaries.
///
/// The suffix-past-a-cursor shape is what makes moment production idempotent under the
/// room's coalescing broadcasts: the same log may be projected many times, and only the
/// entries the trail has not already turned into moments produce new ones. Entries that
/// have fallen out of the engine's bounded log window are simply never seen — a gap in
/// the moment stream, which the contract already permits (issue #594) and which a client
/// must not try to fill.
pub(crate) fn moments_from_log(
    entries: &[GameLogEntry],
    after: u64,
    position: &mut MomentPosition,
    retained: RetainedObjects<'_>,
) -> Vec<MomentDraft> {
    let mut deriver = Deriver {
        out: Vec::new(),
        resolution: None,
        position,
        retained,
    };
    for entry in entries.iter().filter(|entry| entry.sequence > after) {
        deriver.entry(&entry.event);
    }
    deriver.out
}

/// The walking state of one derivation.
struct Deriver<'a> {
    out: Vec<MomentDraft>,
    /// The index of the most recent [`MomentKind::Resolved`] in this derivation, and
    /// the only causal attribution this module makes. It is not adjacency: the engine
    /// records `spell_resolved` **before** the effects that spell causes (CR 608.2,
    /// and stated in `rune_engine::resolve`), so a death or a damage event that follows
    /// it in the same step is that resolution's consequence by the engine's own
    /// ordering, not by proximity. Cleared at every step boundary, because a resolution
    /// cannot reach across one — combat damage in the combat-damage step is not caused
    /// by a spell that resolved back in declare-blockers.
    resolution: Option<usize>,
    position: &'a mut MomentPosition,
    retained: RetainedObjects<'a>,
}

impl Deriver<'_> {
    /// Derive the moments for one log event.
    fn entry(&mut self, event: &GameLogEvent) {
        match event {
            GameLogEvent::SpellCast { player, card } => {
                let object = self.object(card);
                self.push(
                    MomentKind::Cast {
                        player: player.clone(),
                        object,
                    },
                    None,
                );
            }
            GameLogEvent::SpellResolved { player, card } => {
                let object = self.object(card);
                let at = self.push(
                    MomentKind::Resolved {
                        player: player.clone(),
                        object,
                    },
                    None,
                );
                self.resolution = Some(at);
            }
            // CR 701.5a / 608.2b: both leave the stack for a graveyard, and both are a
            // *travel* a client animates, so each states the removal and the move it
            // caused. Countered and fizzled stay distinct kinds — conflating them would
            // credit an opponent with an answer they never had.
            GameLogEvent::SpellCountered { player, card } => {
                let object = self.object(card);
                let at = self.push(
                    MomentKind::Countered {
                        player: player.clone(),
                        object: object.clone(),
                    },
                    None,
                );
                self.push(
                    MomentKind::ZoneMove {
                        object,
                        from: MomentZone::Stack,
                        to: MomentZone::Graveyard,
                    },
                    Some(at),
                );
            }
            GameLogEvent::SpellFizzled { player, card } => {
                let object = self.object(card);
                let at = self.push(
                    MomentKind::Fizzled {
                        player: player.clone(),
                        object: object.clone(),
                    },
                    None,
                );
                self.push(
                    MomentKind::ZoneMove {
                        object,
                        from: MomentZone::Stack,
                        to: MomentZone::Graveyard,
                    },
                    Some(at),
                );
            }
            // CR 508.1 / 509.1: a declaration is one beat, not one per creature.
            GameLogEvent::AttackersDeclared { player, attackers } => {
                let attackers = attackers.iter().map(|entity| self.object(entity)).collect();
                self.push(
                    MomentKind::Attacked {
                        player: player.clone(),
                        attackers,
                    },
                    None,
                );
            }
            GameLogEvent::BlockersDeclared { player, blocks } => {
                self.push(
                    MomentKind::Blocked {
                        player: player.clone(),
                        blocks: blocks.clone(),
                    },
                    None,
                );
            }
            // Log-only: nothing on the board moves, so there is nothing to give a beat
            // of screen time to (issue #594).
            GameLogEvent::Mulligan { .. } | GameLogEvent::HandKept { .. } => {}
            GameLogEvent::LifeChanged { player, amount } => {
                self.push(
                    MomentKind::Life {
                        player: player.clone(),
                        amount: *amount,
                    },
                    None,
                );
            }
            GameLogEvent::DamageDealt { target, amount } => {
                let cause = self.resolution;
                self.push(
                    MomentKind::Damage {
                        target: target.clone(),
                        amount: *amount,
                    },
                    cause,
                );
            }
            GameLogEvent::CardsDrawn { player, count } => {
                self.push(
                    MomentKind::Drew {
                        player: player.clone(),
                        count: *count,
                    },
                    None,
                );
            }
            // CR 700.4: a creature died. The death and the battlefield→graveyard travel
            // are two beats — one says what happened, one says what to animate.
            GameLogEvent::PermanentDied { permanent } => {
                let object = self.object(permanent);
                let cause = self.resolution;
                let at = self.push(
                    MomentKind::Died {
                        object: object.clone(),
                    },
                    cause,
                );
                self.push(
                    MomentKind::ZoneMove {
                        object,
                        from: MomentZone::Battlefield,
                        to: MomentZone::Graveyard,
                    },
                    Some(at),
                );
            }
            GameLogEvent::StepChanged {
                turn,
                active_player,
                phase,
            } => self.step_changed(*turn, active_player, *phase),
            GameLogEvent::PlayerEliminated { player, reason } => {
                self.push(
                    MomentKind::Eliminated {
                        player: player.clone(),
                        reason: *reason,
                    },
                    None,
                );
            }
            // CR 903.9a: the commander moved to the command zone from *either* a
            // graveyard or exile, and the event does not say which. The origin is the
            // zone the room observed the card in when this batch opened — proof, not a
            // guess — and with no observation there is no honest `from`, so no moment is
            // produced rather than a fabricated one.
            GameLogEvent::CommanderReturnedToCommandZone { card, .. } => {
                let from = self.retained.origins.get(&card.id).copied();
                let Some(from @ (MomentZone::Graveyard | MomentZone::Exile)) = from else {
                    return;
                };
                let object = self.object(card);
                self.push(
                    MomentKind::ZoneMove {
                        object,
                        from,
                        to: MomentZone::Command,
                    },
                    None,
                );
            }
            GameLogEvent::GameOver { result } => {
                self.push(
                    MomentKind::GameOver {
                        result: result.clone(),
                    },
                    None,
                );
            }
        }
    }

    /// A step boundary (CR 500.1): the position advances, the resolution attribution
    /// window closes, and a *turn* boundary gets its own moment on top of the phase one.
    ///
    /// A turn change is stated by the server precisely because a client cannot read it
    /// off the sequence: a repeated step means an extra combat phase (CR 506.1) or an
    /// extra cleanup (CR 514.3a) at least as often as it means a new turn, and a
    /// repeated *turn number* with a different active player is still a new turn.
    fn step_changed(&mut self, turn: u32, active_player: &str, phase: Phase) {
        let turned = turn != self.position.turn || active_player != self.position.active_player;
        self.position.turn = turn;
        self.position.phase = phase;
        self.position.active_player = active_player.to_string();
        self.resolution = None;
        if turned {
            self.push(
                MomentKind::TurnChange {
                    turn,
                    active_player: active_player.to_string(),
                },
                None,
            );
        }
        self.push(MomentKind::PhaseChange { phase }, None);
    }

    /// Record a draft at the current position, returning its index so a consequence can
    /// name it as its cause.
    fn push(&mut self, kind: MomentKind, cause: Option<usize>) -> usize {
        self.out.push(MomentDraft {
            kind,
            turn: self.position.turn,
            phase: self.position.phase,
            cause,
        });
        self.out.len() - 1
    }

    /// The retained snapshot of a logged entity: the id and the name **the event
    /// recorded**, plus the public face the room had cached for it, if any.
    ///
    /// The name is never re-resolved against the current board — the same promise
    /// [`LogEntity`] makes — because a moment is shown after the object is gone. A
    /// missing face means "no public face was known", never "hidden from you": a
    /// private face is never cached in the first place, so absence carries no
    /// information a receiver could mine.
    fn object(&self, entity: &LogEntity) -> MomentObject {
        MomentObject {
            id: entity.id.clone(),
            name: entity.name.clone(),
            card: self.retained.faces.get(&entity.id).cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn position() -> MomentPosition {
        MomentPosition {
            turn: 1,
            phase: Phase::Upkeep,
            active_player: "p0".into(),
        }
    }

    fn entity(id: &str, name: &str) -> LogEntity {
        LogEntity {
            id: id.into(),
            name: name.into(),
        }
    }

    fn derive(events: Vec<GameLogEvent>, at: &mut MomentPosition) -> Vec<MomentDraft> {
        let entries: Vec<GameLogEntry> = events
            .into_iter()
            .enumerate()
            .map(|(i, event)| GameLogEntry {
                sequence: i as u64 + 1,
                event,
            })
            .collect();
        let faces = BTreeMap::new();
        let origins = BTreeMap::new();
        moments_from_log(
            &entries,
            0,
            at,
            RetainedObjects {
                faces: &faces,
                origins: &origins,
            },
        )
    }

    #[test]
    fn issue_594_a_death_names_the_resolution_that_caused_it() {
        // The engine records `spell_resolved` before the effects it causes, so the
        // death that follows inside the same step is attributed to it — and the
        // battlefield→graveyard travel is attributed to the death.
        let mut at = position();
        let drafts = derive(
            vec![
                GameLogEvent::SpellResolved {
                    player: "p1".into(),
                    card: entity("card_3", "Shock"),
                },
                GameLogEvent::PermanentDied {
                    permanent: entity("perm_7", "Llanowar Elves"),
                },
            ],
            &mut at,
        );
        assert_eq!(drafts.len(), 3, "resolved, died, zone_move");
        assert!(matches!(drafts[0].kind, MomentKind::Resolved { .. }));
        assert!(matches!(drafts[1].kind, MomentKind::Died { .. }));
        assert_eq!(drafts[1].cause, Some(0), "the death names the resolution");
        assert_eq!(drafts[2].cause, Some(1), "the travel names the death");
        assert!(matches!(
            drafts[2].kind,
            MomentKind::ZoneMove {
                from: MomentZone::Battlefield,
                to: MomentZone::Graveyard,
                ..
            }
        ));
    }

    #[test]
    fn issue_594_a_step_boundary_closes_the_attribution_window() {
        // A resolution cannot reach across a step: combat damage in the combat-damage
        // step is not the consequence of a spell that resolved in declare-blockers.
        let mut at = position();
        let drafts = derive(
            vec![
                GameLogEvent::SpellResolved {
                    player: "p0".into(),
                    card: entity("card_1", "Giant Growth"),
                },
                GameLogEvent::StepChanged {
                    turn: 1,
                    active_player: "p0".into(),
                    phase: Phase::CombatDamage,
                },
                GameLogEvent::PermanentDied {
                    permanent: entity("perm_2", "Walking Corpse"),
                },
            ],
            &mut at,
        );
        let died = drafts
            .iter()
            .find(|d| matches!(d.kind, MomentKind::Died { .. }))
            .unwrap();
        assert_eq!(died.cause, None, "no resolution is in force after the step");
        assert_eq!(
            died.phase,
            Phase::CombatDamage,
            "labelled where it happened"
        );
    }

    #[test]
    fn issue_594_a_turn_boundary_is_stated_not_inferred() {
        // Same turn number, new active player is still a new turn; a step change inside
        // one turn is not. Both are the server's judgment, never the client's.
        let mut at = position();
        let drafts = derive(
            vec![
                GameLogEvent::StepChanged {
                    turn: 1,
                    active_player: "p0".into(),
                    phase: Phase::Draw,
                },
                GameLogEvent::StepChanged {
                    turn: 2,
                    active_player: "p1".into(),
                    phase: Phase::Untap,
                },
            ],
            &mut at,
        );
        let kinds: Vec<&MomentKind> = drafts.iter().map(|d| &d.kind).collect();
        assert_eq!(kinds.len(), 3, "phase, then turn + phase: {kinds:?}");
        assert!(matches!(kinds[0], MomentKind::PhaseChange { .. }));
        assert!(matches!(kinds[1], MomentKind::TurnChange { turn: 2, .. }));
        assert!(matches!(kinds[2], MomentKind::PhaseChange { .. }));
        assert_eq!(at.turn, 2, "the walk carries the position forward");
    }

    #[test]
    fn issue_594_a_commander_return_without_a_known_origin_produces_no_moment() {
        // CR 903.9a permits a graveyard or exile origin and the event names neither, so
        // an unobserved commander yields no moment at all rather than a guessed one.
        let mut at = position();
        let drafts = derive(
            vec![GameLogEvent::CommanderReturnedToCommandZone {
                player: "p0".into(),
                card: entity("card_9", "Jedit Ojanen"),
            }],
            &mut at,
        );
        assert!(drafts.is_empty());

        // Observed in a graveyard when the batch opened, the same event states both
        // endpoints.
        let faces = BTreeMap::new();
        let mut origins = BTreeMap::new();
        origins.insert("card_9".to_string(), MomentZone::Graveyard);
        let entries = vec![GameLogEntry {
            sequence: 1,
            event: GameLogEvent::CommanderReturnedToCommandZone {
                player: "p0".into(),
                card: entity("card_9", "Jedit Ojanen"),
            },
        }];
        let drafts = moments_from_log(
            &entries,
            0,
            &mut at,
            RetainedObjects {
                faces: &faces,
                origins: &origins,
            },
        );
        assert!(matches!(
            drafts[0].kind,
            MomentKind::ZoneMove {
                from: MomentZone::Graveyard,
                to: MomentZone::Command,
                ..
            }
        ));
    }

    #[test]
    fn issue_594_only_entries_past_the_cursor_become_moments() {
        // Idempotence under coalescing: the same log is projected on every settle, and
        // only the unconsumed suffix produces moments.
        let entries: Vec<GameLogEntry> = (1..=3)
            .map(|sequence| GameLogEntry {
                sequence,
                event: GameLogEvent::CardsDrawn {
                    player: "p0".into(),
                    count: 1,
                },
            })
            .collect();
        let faces = BTreeMap::new();
        let origins = BTreeMap::new();
        let retained = RetainedObjects {
            faces: &faces,
            origins: &origins,
        };
        let mut at = position();
        assert_eq!(moments_from_log(&entries, 0, &mut at, retained).len(), 3);
        assert_eq!(moments_from_log(&entries, 2, &mut at, retained).len(), 1);
        assert_eq!(moments_from_log(&entries, 3, &mut at, retained).len(), 0);
    }

    #[test]
    fn issue_594_a_log_only_event_produces_no_moment() {
        // The vocabularies differ on purpose: nothing on the board moves for a
        // mulligan or a kept hand, so neither is worth a beat of screen time.
        let mut at = position();
        let drafts = derive(
            vec![
                GameLogEvent::Mulligan {
                    player: "p0".into(),
                },
                GameLogEvent::HandKept {
                    player: "p1".into(),
                },
            ],
            &mut at,
        );
        assert!(drafts.is_empty());
    }
}
