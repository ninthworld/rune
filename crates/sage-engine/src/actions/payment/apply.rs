//! Applying a payment a player assembled, and deciding whether it covers the cost.
//!
//! [`super::solve`] answers what a payment *could* be; this validates the one that
//! actually arrived. They are separate because they are asked at different moments and
//! must be allowed to disagree: the offer is made before the player has chosen anything,
//! and this is where the choice is settled.

use crate::card::CardDatabase;
use crate::id::CardInstance;
use crate::state::GameState;

use super::super::definition::{discards_of, mana_of, sacrifices_of, Action, CostPayment};
use super::super::generation::valid_actions;
use super::super::utilities::cast_cost;
use super::sources::is_plain_mana_source;

/// Apply a payment's **mana half** to `state`, returning whether every source in it was
/// legal.
///
/// Only the mana. The rest of a total cost (CR 601.2b) is paid once the card is on the
/// stack, in [`crate::apply::apply_cast_spell`], because CR 601.2 puts the spell there
/// before costs are paid — which is exactly what stops a spell being discarded to pay for
/// itself. Splitting it here rather than checking for that case is the difference between
/// a rule and a guard.
///
/// **Sequential, and re-validated at each step against the state the previous ones
/// produced.** That is not belt-and-braces: a source named twice is a real submission a
/// client can send, and the second naming has to be illegal for the same reason the
/// second click on a tapped land does nothing. It is also what enforces *one option per
/// permanent* — naming both halves of a dual land is exactly naming it twice, and the
/// second activation finds it tapped.
///
/// Asking [`valid_actions`] each time is the same authority that gates a standalone
/// activation, so a payment can never do anything a sequence of ordinary taps could not.
///
/// The caller applies this to a **scratch copy** when it is deciding legality and to the
/// real state when it is applying the cast. Both are correct because `GameState` is
/// owned and the engine mutates nothing it was handed.
pub(crate) fn apply_payment(
    state: &mut GameState,
    db: &CardDatabase,
    payment: &[CostPayment],
) -> bool {
    for source in mana_of(payment) {
        if !is_plain_mana_source(state, db, source) {
            return false;
        }
        let activation = Action::ActivateAbility {
            permanent: source.permanent,
            index: source.index,
            targets: Vec::new(),
            payment: Vec::new(),
        };
        if !valid_actions(state, db).contains(&activation) {
            return false;
        }
        // A mana ability charges nothing a player picks — [`is_plain_mana_source`] has
        // established it is a `{T}`-and-mana activation — so it is applied with an empty
        // payment of its own.
        crate::apply::apply_activate_ability(state, source.permanent, source.index, &[], &[], db);
    }
    true
}

/// Whether this payment pays **the whole cost** of casting `card` — the mana it produces
/// covers the mana cost, and the cards it names satisfy any additional cost exactly.
///
/// The one question the two halves of a cast have to agree on, asked in the one place
/// both of them read. Legality asks it before anything is applied;
/// [`crate::apply_action`] re-applies the payment for real, so the offer and the charge
/// cannot disagree about what a payment was worth.
///
/// A payment with no mana entries reduces the mana half to exactly the old question —
/// does the pool as it stands cover the cost — which is why a cast paid out of floating
/// mana is unchanged.
#[must_use]
pub(crate) fn payment_covers_cast(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
    payment: &[CostPayment],
) -> bool {
    let Some((cost, purpose_subtypes)) = cast_cost(state, db, card, x) else {
        return false;
    };
    if !discards_pay_the_additional_cost(state, db, card, &discards_of(payment)) {
        return false;
    }
    if !sacrifices_pay_the_additional_cost(state, db, card, &sacrifices_of(payment)) {
        return false;
    }
    // No additional *cast* cost exiles anything — that shape exists only on an activation
    // (CR 601.2b) — so an exile entry here pays nothing and is refused rather than
    // dropped, for the reason a mana entry on an activation is.
    if !super::super::definition::exiles_of(payment).is_empty() {
        return false;
    }
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

/// Whether `discards` is exactly what `card`'s additional cost demands (CR 601.2b).
///
/// **Exactly**, in both directions: a card with no additional cost accepts no discards at
/// all, and a card with one is not paid by fewer — or by more, since over-paying a cost
/// is not a thing a player may choose to do. Each named card must be a distinct card in
/// the caster's hand, and none of them may be the card being cast: it is on its way to
/// the stack, so a hand of exactly this one card cannot discard to cast it.
fn discards_pay_the_additional_cost(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    discards: &[crate::id::CardInstanceId],
) -> bool {
    let owed = db
        .card(card.card)
        .and_then(|data| data.additional_cost)
        .map_or(0, crate::AdditionalCost::discard_count);
    if discards.len() != usize::from(owed) {
        return false;
    }
    let Some(player) = state.players.get(state.priority.0) else {
        return false;
    };
    discards.iter().enumerate().all(|(i, &named)| {
        named != card.id
            && !discards[..i].contains(&named)
            && player.hand.iter().any(|held| held.id == named)
    })
}

/// Whether `sacrifices` is exactly what `card`'s additional cost demands (CR 601.2b).
///
/// The permanent counterpart of [`discards_pay_the_additional_cost`], exact in the same
/// two directions: a card with no sacrifice cost accepts no sacrifices at all, and one
/// with a fixed sacrifice cost is paid by exactly that many permanents — never fewer, and
/// never more, since over-paying a cost is not something a player may choose to do.
///
/// A cost taking **any number** is the one that bends, and only in the direction the card
/// prints: anything from none up to every candidate the board holds is a legal payment,
/// and there is nothing above that to over-pay with.
///
/// Each named permanent must be **on the battlefield, controlled by the caster**
/// (CR 701.17b), of the type the cost names, and named only once. Unlike the discard
/// check there is no card to exclude: the spell being cast is in hand, on its way to the
/// stack, and was never a permanent anyone could sacrifice.
fn sacrifices_pay_the_additional_cost(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    sacrifices: &[crate::id::PermanentId],
) -> bool {
    let owed = db.card(card.card).and_then(|data| data.additional_cost);
    let Some((card_type, count)) = owed.and_then(|cost| {
        cost.sacrifice_type()
            .zip(Some(cost.sacrifice_count().unwrap_or_default()))
    }) else {
        return sacrifices.is_empty();
    };
    let candidates = state.sacrifice_candidates_for_cast(state.priority, card_type, db);
    count.is_paid_by(sacrifices.len(), candidates.len())
        && sacrifices
            .iter()
            .enumerate()
            .all(|(i, named)| !sacrifices[..i].contains(named) && candidates.contains(named))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use crate::card::CardDatabase;
    use crate::id::{CardInstance, PermanentId, PlayerId};
    use crate::phase::Step;
    use crate::state::{GameState, Permanent};
    use crate::{apply_action, valid_actions, Action, CardId, CostPayment, ManaSource};

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    fn card(db: &CardDatabase, slug: &str) -> CardId {
        db.card_id(&slug.to_string().try_into().unwrap()).unwrap()
    }

    fn main_phase() -> GameState {
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.priority = state.active_player;
        state.turn = 3;
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
            blocking: Vec::new(),
            skips_untap: false,
            dealt_damage: false,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
            named_card: None,
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

    fn tap_all(lands: &[PermanentId]) -> Vec<CostPayment> {
        lands
            .iter()
            .map(|&permanent| {
                CostPayment::Mana(ManaSource {
                    permanent,
                    index: 0,
                })
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
            mode: None,
            x: None,
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
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: vec![CostPayment::Mana(ManaSource {
                permanent: lands[0],
                index: 0,
            })],
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
            mode: None,
            x: None,
            targets: Vec::new(),
            payment: vec![
                CostPayment::Mana(ManaSource {
                    permanent: lands[0],
                    index: 0,
                }),
                CostPayment::Mana(ManaSource {
                    permanent: lands[0],
                    index: 0,
                }),
            ],
        };

        let after = apply_action(&state, &doubled, &database);
        assert!(after.stack.is_empty());
        assert!(after.battlefield.iter().all(|p| !p.tapped));
    }

    /// **Both halves of a dual land is the same illegal submission as naming one twice.**
    /// Tapping it for `{W}` retires the `{U}` its other ability would have made, and the
    /// sequential validation is what says so.
    #[test]
    fn both_halves_of_a_dual_land_cannot_be_spent() {
        let database = db();
        let mut state = main_phase();
        let river = place(&mut state, card(&database, "meandering_river"));
        let spell = state.new_instance(card(&database, "daybreak_chaplain"));
        state.players[0].hand.push(spell);

        // The two mana abilities of a River are at indices 1 and 2 — index 0 is
        // `enters_tapped`. Take them from the offer rather than assuming.
        let indices: Vec<usize> = valid_actions(&state, &database)
            .into_iter()
            .filter_map(|action| match action {
                Action::ActivateAbility {
                    permanent, index, ..
                } if permanent == river => Some(index),
                _ => None,
            })
            .collect();
        assert_eq!(indices.len(), 2, "a River offers two mana abilities");

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: indices
                    .into_iter()
                    .map(|index| {
                        CostPayment::Mana(ManaSource {
                            permanent: river,
                            index,
                        })
                    })
                    .collect(),
            },
            &database,
        );
        assert!(after.stack.is_empty(), "the cast was refused");
        assert!(
            after.battlefield.iter().all(|p| !p.tapped),
            "and nothing was spent trying"
        );
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
                mode: None,
                x: None,
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
                mode: None,
                x: None,
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

        assert!(
            valid_actions(&state, &database).contains(&Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            }),
            "the cast is announceable while the mana is still in the lands"
        );
    }

    /// …but announcing it is not casting it. A cast sent with no payment and nothing
    /// floating is still illegal and still a no-op.
    #[test]
    fn the_widened_offer_never_widens_what_is_legal() {
        let (state, database, _lands, spell) = board(2);
        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            },
            &database,
        );
        assert!(after.stack.is_empty(), "an unpaid cast is refused");
        assert!(after.battlefield.iter().all(|p| !p.tapped));
    }
}
