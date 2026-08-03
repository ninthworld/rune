//! Paying for a cast as part of casting it (CR 601.2).
//!
//! CR 601.2 walks the casting process in order: announce the spell, choose modes and
//! targets, determine the total cost, **activate mana abilities**, pay. It is one
//! process, and if it cannot be completed the game returns to where it was before the
//! process started — so the taps that paid for a spell that never got cast are undone
//! by the rules themselves, not by anybody's undo feature.
//!
//! The engine used to model only the *other* thing: floating mana with a standalone
//! activation, then casting out of a pool that already covered the cost. That is real
//! and stays (CR 605.3 — a player may activate a mana ability whenever they have
//! priority, and holding mana across a cast needs it), but it is the special case. This
//! module is the general one, and everything about it follows from being **one action**:
//!
//! - a payment is applied on the way into the cast, in the same `apply_action` call, so
//!   an insufficient one is a no-op and the state comes back untouched;
//! - a player assembling a payment has therefore sent nothing at all, which is what
//!   makes taking a source back out free — no client draft to reconcile and no server
//!   draft to invalidate;
//! - and the ordering questions that a two-step model has to invent answers for — what
//!   happens if the board moves between the tap and the cast, what an opponent sees
//!   mid-payment — do not arise, because there is no between.
//!
//! **Nothing here decides what a card produces.** A [`ManaSource`] names an activation
//! and the card's own ability says what it adds; the pool that results is the ordinary
//! one, and whether it covers the cost is [`ManaPool::pay_for`]'s answer, the same one
//! the pool-first path has always used.
//!
//! ## What this does not yet do
//!
//! A mana ability that **asks a question** — `Add one mana of any color`, which CR
//! 605.3b expressly permits — is refused as a payment source rather than answered. Its
//! activation suspends into a pending choice, and a choice posed *inside* a casting
//! process is a second suspension point the process does not have anywhere to put yet.
//! Such a source is still activatable on its own, so the pool-first path pays with it;
//! it simply cannot ride inside the cast. That is the honest boundary of this change and
//! it is checked rather than assumed ([`is_plain_mana_source`]) — and, importantly, it is
//! subtracted from what the generator announces against ([`realisable_mana_pool`]), so the
//! boundary costs a player a convenience and never leaves them an offer they cannot take.
use crate::ability::{Ability, Effect};
use crate::card::abilities_of_permanent;
use crate::card::CardDatabase;
use crate::id::CardInstance;
use crate::state::GameState;

use super::definition::{Action, ManaSource};
use super::generation::valid_actions;
use super::utilities::cast_cost;

/// Whether this permanent's ability may ride inside a cast as a payment source.
///
/// Two conditions, and the second is the one worth stating: it must be a mana ability
/// (CR 605.1a — no stack, no priority, so it can happen inside another process), and it
/// must not *pose a question*. `Add one mana of any color` is a mana ability that asks
/// which colour, and answering that mid-cast needs a suspension point the casting
/// process has not got. Refused here rather than half-applied.
#[must_use]
pub fn is_plain_mana_source(state: &GameState, db: &CardDatabase, source: ManaSource) -> bool {
    let Some(perm) = state.battlefield.iter().find(|p| p.id == source.permanent) else {
        return false;
    };
    let Some(ability) = abilities_of_permanent(db, perm)
        .into_iter()
        .nth(source.index)
    else {
        return false;
    };
    if !crate::ability::is_mana_ability(&ability) {
        return false;
    }
    let Ability::Activated { effects, .. } = ability else {
        return false;
    };
    !effects
        .iter()
        .any(|effect| matches!(effect, Effect::AddManaAnyColor { .. }))
}

/// Apply a payment to `state`, returning whether every source in it was legal.
///
/// **Sequential, and re-validated at each step against the state the previous ones
/// produced.** That is not belt-and-braces: a source named twice is a real submission a
/// client can send, and the second naming has to be illegal for the same reason the
/// second click on a tapped land does nothing. Asking [`valid_actions`] each time is the
/// same authority that gates a standalone activation, so a payment can never do anything
/// a sequence of ordinary taps could not.
///
/// The caller applies this to a **scratch copy** when it is deciding legality and to the
/// real state when it is applying the cast. Both are correct because `GameState` is
/// owned and the engine mutates nothing it was handed.
pub(crate) fn apply_payment(
    state: &mut GameState,
    db: &CardDatabase,
    payment: &[ManaSource],
) -> bool {
    for &source in payment {
        if !is_plain_mana_source(state, db, source) {
            return false;
        }
        let activation = Action::ActivateAbility {
            permanent: source.permanent,
            index: source.index,
            targets: Vec::new(),
        };
        if !valid_actions(state, db).contains(&activation) {
            return false;
        }
        crate::apply::apply_activate_ability(state, source.permanent, source.index, &[], db);
    }
    true
}

/// Whether this payment, applied in order, leaves a pool that covers the cast's cost.
///
/// The one question the two halves of a cast have to agree on, asked in the one place
/// both of them read. Legality asks it before anything is applied; [`crate::apply_action`]
/// re-applies the payment for real and lets [`crate::ManaPool::pay_for`] charge it, so
/// the offer and the charge cannot disagree about what a payment was worth.
///
/// An empty payment reduces to exactly the old question — does the pool as it stands
/// cover the cost — which is why a cast paid out of floating mana is unchanged.
#[must_use]
pub(crate) fn payment_covers_cast(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    payment: &[ManaSource],
) -> bool {
    let Some((cost, purpose_subtypes)) = cast_cost(state, db, card) else {
        return false;
    };
    let mut scratch = state.clone();
    if !apply_payment(&mut scratch, db, payment) {
        return false;
    }
    scratch
        .players
        .get(scratch.priority.0)
        .is_some_and(|player| {
            player.mana_pool.can_pay_for(
                &cost,
                crate::mana::SpendPurpose::CastingSpell {
                    subtypes: &purpose_subtypes,
                },
            )
        })
}

/// Every mana source the priority holder could name in a payment right now.
///
/// The candidate set a presentation offers, enumerated the way every other candidate set
/// is (ADR 0004): O(permanents × abilities), never a search over combinations. It says
/// which sources are *available*, and deliberately not which combination would cover a
/// given cost — that is the question [`payment_covers_cast`] answers about a payment the
/// player actually assembled, and enumerating winning combinations would be the
/// cartesian product this engine does not build anywhere else.
#[must_use]
pub fn payment_sources(state: &GameState, db: &CardDatabase) -> Vec<ManaSource> {
    let mut sources = Vec::new();
    for perm in &state.battlefield {
        if perm.controller != state.priority {
            continue;
        }
        for index in 0..abilities_of_permanent(db, perm).len() {
            let source = ManaSource {
                permanent: perm.id,
                index,
            };
            if !is_plain_mana_source(state, db, source) {
                continue;
            }
            // Offered only while it is genuinely activatable — untapped, not summoning
            // sick (CR 302.6), and whatever else the generator weighs. One authority.
            let activation = Action::ActivateAbility {
                permanent: perm.id,
                index,
                targets: Vec::new(),
            };
            if valid_actions(state, db).contains(&activation) {
                sources.push(source);
            }
        }
    }
    sources
}

/// The mana this seat could **actually make with a payment**, right now.
///
/// The gate the generator announces casts against, and it has to be this rather than
/// [`potential_mana_pool`](super::utilities::potential_mana_pool) — which is an estimate
/// that deliberately errs high. The estimate credits a summoning-sick creature's tap
/// ability and credits `Add one mana of any color` as one mana of *every* colour, and
/// both of those are mana a payment cannot realise: the first is not activatable
/// (CR 302.6) and the second is refused as a payment source (see the module note).
///
/// Erring high is right for the callers that estimate is written for — it only ever
/// keeps a seat non-idle or offers a choice that turns out unaffordable. It is **wrong
/// here**, and the failure is not cosmetic: a cast announced against mana no payment can
/// produce is a cast an automated player takes, has refused as a no-op, and takes again
/// for ever. So this credits exactly what [`payment_sources`] would let a player spend,
/// and "the offer" and "a payment exists" are one answer.
///
/// It deliberately does not call [`valid_actions`]: the generator calls this, so it would
/// be asking itself. The activation conditions it re-derives are the two that bear on a
/// mana ability — untapped, and not summoning sick — which is why they are named here
/// rather than borrowed.
#[must_use]
pub(crate) fn realisable_mana_pool(
    state: &GameState,
    player: crate::id::PlayerId,
    db: &CardDatabase,
) -> crate::mana::ManaPool {
    let mut pool = state
        .players
        .get(player.0)
        .map(|p| p.mana_pool.clone())
        .unwrap_or_default();
    for perm in &state.battlefield {
        if perm.controller != player || perm.tapped {
            continue;
        }
        for (index, ability) in abilities_of_permanent(db, perm).into_iter().enumerate() {
            let source = ManaSource {
                permanent: perm.id,
                index,
            };
            if !is_plain_mana_source(state, db, source) {
                continue;
            }
            let Ability::Activated { cost, effects, .. } = ability else {
                continue;
            };
            // CR 302.6 exempts nothing, mana abilities included: a `{T}` cost on a
            // creature that entered this turn is not activatable, so its mana is not
            // mana this seat can make.
            if super::utilities::tap_cost_is_summoning_sick(state, perm, &cost, db) {
                continue;
            }
            for effect in &effects {
                match effect {
                    Effect::AddMana { color, amount } => pool.add(*color, *amount),
                    Effect::AddColorlessMana { amount } => pool.add_colorless(*amount),
                    Effect::AddRestrictedMana {
                        color,
                        amount,
                        restriction,
                    } => pool.add_restricted(*color, *amount, restriction.clone()),
                    _ => {}
                }
            }
        }
    }
    pool
}

/// A payment that covers this cast, or `None` if no available set of sources does.
///
/// A **rules** question, not a policy one, and the distinction is the seam: this answers
/// *what would be a legal payment*, exactly as [`crate::legal_targets_for_spec`] answers
/// what would be a legal target. Whether to use the answer — auto-tap for the player, or
/// ask them to pick — is a judgment about a person, and it belongs to the caller (ADR
/// 0010).
///
/// The search is a prefix rather than a subset search, and that is sound rather than
/// merely cheap: mana is **monotone** — [`crate::ManaPool::pay_for`] pays each coloured
/// requirement from its own colour and the generic remainder from anything, so adding
/// mana to a pool can never make a cost less payable. If any set of the available
/// sources covers the cost then the whole available set does, and the shortest prefix
/// that covers it is a legal payment. It is not necessarily the payment a *person* would
/// choose — it may spend a dual land where a basic would have done — which is precisely
/// why choosing manually stays the other path rather than this one becoming the only one.
#[must_use]
pub fn auto_payment(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Option<Vec<ManaSource>> {
    if payment_covers_cast(state, db, card, &[]) {
        return Some(Vec::new());
    }
    let available = payment_sources(state, db);
    let mut taken: Vec<ManaSource> = Vec::new();
    for source in available {
        taken.push(source);
        if payment_covers_cast(state, db, card, &taken) {
            return Some(taken);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::apply_action;
    use crate::id::{PermanentId, PlayerId};
    use crate::phase::Step;
    use crate::state::Permanent;
    use crate::CardId;

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    fn card(db: &CardDatabase, slug: &str) -> CardId {
        db.card_id(&slug.to_string().try_into().unwrap()).unwrap()
    }

    /// A two-player precombat main on turn 1 with player 0 holding priority.
    fn main_phase() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.priority = state.active_player;
        state
    }

    fn place(state: &mut GameState, c: CardId) -> PermanentId {
        let inst = state.new_instance(c);
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            printed: c.into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    /// A board with `forests` untapped Forests and one {1}{G} creature in hand.
    fn board(forests: usize) -> (GameState, CardDatabase, Vec<PermanentId>, CardInstance) {
        let database = db();
        let mut state = main_phase();
        let lands = (0..forests)
            .map(|_| place(&mut state, card(&database, "forest")))
            .collect();
        let spell = state.new_instance(card(&database, "highland_game"));
        state.players[0].hand.push(spell);
        (state, database, lands, spell)
    }

    fn tap_all(lands: &[PermanentId]) -> Vec<ManaSource> {
        lands
            .iter()
            .map(|&permanent| ManaSource {
                permanent,
                index: 0,
            })
            .collect()
    }

    /// The whole point, in one action: the sources are tapped, the cost is paid, and the
    /// spell is on the stack, without the player ever having floated mana first.
    #[test]
    fn a_cast_carrying_its_payment_taps_pays_and_goes_on_the_stack_in_one_action() {
        let (state, database, lands, spell) = board(2);
        let cast = Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment: tap_all(&lands),
        };

        let after = apply_action(&state, &cast, &database);

        assert_eq!(after.stack.len(), 1, "the spell was cast");
        assert!(
            after.battlefield.iter().all(|p| p.tapped),
            "both sources paid for it"
        );
        // Nothing floats afterwards: two green in, {1}{G} out.
        assert_eq!(after.players[0].mana_pool.green, 0);
        assert!(after.players[0].hand.is_empty(), "the card left the hand");
    }

    /// The rewind of CR 601.2, which is the reason a player may take a source back out:
    /// a payment that does not cover the cost leaves **no** tapped land behind, because
    /// the process never completed and the whole action is a no-op.
    #[test]
    fn a_payment_that_does_not_cover_the_cost_leaves_nothing_tapped() {
        let (state, database, lands, spell) = board(2);
        // One Forest against {1}{G}: short by one.
        let short = Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment: vec![ManaSource {
                permanent: lands[0],
                index: 0,
            }],
        };

        let after = apply_action(&state, &short, &database);

        assert!(after.stack.is_empty(), "no spell was cast");
        assert!(
            after.battlefield.iter().all(|p| !p.tapped),
            "and no source was spent trying"
        );
        assert_eq!(
            after.players[0].mana_pool.green, 0,
            "no mana was left floating"
        );
        assert_eq!(after.players[0].hand.len(), 1, "the card is still in hand");
    }

    /// A source named twice is tapped once and then illegal — the same answer a second
    /// click on a tapped land gets, because a payment is validated as the sequence of
    /// activations it is rather than as a set.
    #[test]
    fn a_source_named_twice_is_rejected_rather_than_counted_twice() {
        let (state, database, lands, spell) = board(2);
        let doubled = Action::CastSpell {
            card: spell,
            targets: Vec::new(),
            payment: vec![
                ManaSource {
                    permanent: lands[0],
                    index: 0,
                },
                ManaSource {
                    permanent: lands[0],
                    index: 0,
                },
            ],
        };

        let after = apply_action(&state, &doubled, &database);
        assert!(after.stack.is_empty());
        assert!(after.battlefield.iter().all(|p| !p.tapped));
    }

    /// A source that is already tapped cannot pay, and takes the whole cast down with
    /// it rather than being silently skipped — a payment the player did not assemble is
    /// not one the engine may substitute.
    #[test]
    fn an_unavailable_source_makes_the_whole_cast_illegal() {
        let (mut state, database, lands, spell) = board(2);
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == lands[1])
            .unwrap()
            .tapped = true;

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                targets: Vec::new(),
                payment: tap_all(&lands),
            },
            &database,
        );
        assert!(after.stack.is_empty());
        assert!(
            !after
                .battlefield
                .iter()
                .any(|p| p.id == lands[0] && p.tapped),
            "the first source was not spent on a cast that could not complete"
        );
    }

    /// Floating first still works, unchanged: an empty payment is the old path, and the
    /// old path is the one a player holding mana across a cast needs (CR 605.3).
    #[test]
    fn casting_out_of_mana_already_floating_is_unchanged() {
        let (mut state, database, _lands, spell) = board(0);
        state.players[0].mana_pool.add(crate::mana::Color::Green, 1);
        state.players[0].mana_pool.add_colorless(1);

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &database,
        );
        assert_eq!(after.stack.len(), 1);
    }

    /// The generator now announces a cast a player *could* pay for, not only one they
    /// have already paid for — which is what lets a player pick the card first.
    #[test]
    fn a_cast_is_offered_before_its_mana_exists() {
        let (state, database, _lands, spell) = board(2);
        assert_eq!(state.players[0].mana_pool.green, 0, "nothing is floating");

        let offered = valid_actions(&state, &database);
        assert!(
            offered.contains(&Action::CastSpell {
                card: spell,
                targets: Vec::new(),
                payment: Vec::new(),
            }),
            "the cast is announceable while the mana is still in the lands"
        );
    }

    /// …but announcing it is not casting it. The widened offer is an over-estimate, and
    /// this is where it is settled: a cast sent with no payment and nothing floating is
    /// still illegal and still a no-op.
    #[test]
    fn the_widened_offer_never_widens_what_is_legal() {
        let (state, database, _lands, spell) = board(2);
        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &database,
        );
        assert!(after.stack.is_empty(), "an unpaid cast is refused");
        assert!(after.battlefield.iter().all(|p| !p.tapped));
    }

    /// A payment found for a player is a payment they could have assembled by hand: it
    /// covers the cost, and every source in it is one the board actually offered.
    #[test]
    fn auto_payment_finds_a_payment_that_the_engine_then_accepts() {
        let (state, database, lands, spell) = board(3);
        let found = auto_payment(&state, &database, spell).expect("three Forests pay {1}{G}");
        assert_eq!(found.len(), 2, "it stops as soon as the cost is covered");
        assert!(found.iter().all(|s| lands.contains(&s.permanent)));

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                targets: Vec::new(),
                payment: found,
            },
            &database,
        );
        assert_eq!(after.stack.len(), 1);
    }

    /// And it answers honestly when no payment exists, rather than returning a partial
    /// one the caller would send and have rejected.
    #[test]
    fn auto_payment_finds_nothing_when_the_board_cannot_pay() {
        let (state, database, _lands, spell) = board(1);
        assert!(auto_payment(&state, &database, spell).is_none());
    }

    /// **The regression that matters most here**, and the one this spike actually hit:
    /// every cast the generator announces must have a payment that exists.
    ///
    /// The first version gated the offer on `potential_mana_pool`, which deliberately
    /// errs high — it credits a summoning-sick creature's tap ability. That announced a
    /// cast no payment could pay for, and an automated player took it, had it refused as
    /// a no-op, and took it again, for ever: a rule-based agent game stopped terminating.
    /// So the offer is gated on what a payment can *realise*, and this pins the two
    /// together over exactly the board that came apart.
    #[test]
    fn every_announced_cast_has_a_payment_that_exists() {
        let database = db();
        let mut state = main_phase();
        state.turn = 1;
        // A mana creature that entered this turn: it taps for {G} on paper and cannot be
        // tapped at all today (CR 302.6), which is precisely the gap.
        let elves = place(&mut state, card(&database, "llanowar_elves"));
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == elves)
            .unwrap()
            .entered_turn = state.turn;
        let spell = state.new_instance(card(&database, "llanowar_elves"));
        state.players[0].hand.push(spell);

        for action in valid_actions(&state, &database) {
            let Action::CastSpell { card, .. } = action else {
                continue;
            };
            assert!(
                auto_payment(&state, &database, card).is_some(),
                "a cast is announced that no payment can pay for — an automated \
                 player takes it, is refused, and takes it again for ever"
            );
        }
    }

    /// The candidate set a presentation offers: available sources, and nothing about
    /// which combination would cover a cost.
    #[test]
    fn payment_sources_lists_what_is_available_and_not_what_would_be_enough() {
        let (mut state, database, lands, _spell) = board(3);
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == lands[2])
            .unwrap()
            .tapped = true;

        let sources = payment_sources(&state, &database);
        assert_eq!(sources.len(), 2, "the tapped Forest is not a source");
        assert!(sources.iter().all(|s| s.permanent != lands[2]));
    }
}
