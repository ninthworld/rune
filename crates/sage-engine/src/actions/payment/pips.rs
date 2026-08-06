//! The cost a player still has to pay, unit by unit, and what could pay each unit.
//!
//! The presentation half of paying for a cast, and the reason it lives in the engine
//! rather than above it: *which sources can pay a `{W}`*, *when does the choice between a
//! dual land's two halves matter*, and *how much of the cost does the floating pool
//! already cover* are all rules questions. A client that answered any of them would be
//! computing cost.
//!
//! **One pip, one slot.** That is what lets everything above this show a running cost
//! without arithmetic: the still-to-pay line is the pips of the unfilled slots, and
//! filling or unfilling one adds or removes a symbol. Nothing subtracts a cost from
//! anything, anywhere, which is the only way the rule "no game logic in the client"
//! survives contact with hybrid pips, cost reduction, and restricted mana.
//!
//! The pips come out **most-constrained first** — colored, then `{C}`, then generic — and
//! that order is load-bearing for a one-click gesture. A caller filling the first slot a
//! clicked source can pay will spend a Plains on the `{W}` of `{1}{W}` rather than on its
//! generic half, which is what a player means by clicking it. (Printed costs read the
//! other way round, generic first; that is a *display* order, and a presentation is free
//! to sort the symbols it draws without changing which slot a click fills.)

use crate::card::CardDatabase;
use crate::id::{CardInstance, CardInstanceId};
use crate::mana::{Color, SpendPurpose};
use crate::state::GameState;

use super::super::definition::ManaSource;
use super::super::utilities::cast_cost;
use super::sources::mana_options;

/// One unit of a cost still to be paid, and every way the board could pay it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaymentPip {
    /// The mana symbol this unit demands — `"{W}"`, `"{C}"`, or `"{1}"` for one unit of
    /// a generic requirement. Display text, and the symbol a caller draws in the
    /// still-to-pay line.
    pub pip: String,
    /// Every activation that could pay *this* unit, in board order.
    ///
    /// A permanent appears **once per ability that could pay this pip**, which is the
    /// whole signal a presentation needs in order to know it must ask which: a dual land
    /// listed twice for a generic pip would be a question with one meaningful answer, so
    /// it is listed once there and twice only where the halves differ in what they pay.
    pub candidates: Vec<ManaSource>,
}

/// The discard a cost demands, and the cards that could pay it (CR 601.2b) — a cast's
/// additional cost ([`discard_cost`]) and an activation's
/// ([`activation_discard_cost`](super::activation_discard_cost)) alike, because it is the
/// same question about the same zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardCost {
    /// How many cards must be discarded. Never zero — a card with no such cost has no
    /// [`DiscardCost`] at all.
    pub count: u8,
    /// The cards in the paying player's hand that could pay it.
    ///
    /// **The card being cast is not among them.** It is on its way to the stack, so a hand
    /// of exactly this one card cannot discard to cast it — which is a rule (CR 601.2b,
    /// 601.2h) and therefore answered here rather than by whoever draws the hand. An
    /// activation has nothing to exclude: its source is a permanent, not a card in the hand
    /// paying for itself.
    pub candidates: Vec<CardInstanceId>,
}

/// The sacrifice a cost demands, and the permanents that could pay it
/// (CR 601.2b / 701.17) — a cast's additional cost ([`sacrifice_cost`]) and an
/// activation's ([`activation_sacrifice_cost`](super::activation_sacrifice_cost)) alike,
/// because it is the same question about the same zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SacrificeCost {
    /// How many permanents it takes — a fixed number, or **any number** the payer picks
    /// (CR 601.2b). The open form is the one a caller must pose with a minimum of zero.
    pub count: crate::ability::SacrificeCount,
    /// The permanents the payer controls that could pay it. May be empty in a state the
    /// action was never offered from; the offer gate refuses such an action, so nothing
    /// downstream has to treat an empty list as payable — except for an open count, where
    /// an empty list really is a legal payment of none.
    pub candidates: Vec<crate::id::PermanentId>,
}

/// The exile a cost demands, and the cards in the payer's own graveyard that could pay it
/// (CR 601.2b / 701.19).
///
/// The graveyard counterpart of [`DiscardCost`], and stated the same way: a count, and the
/// only ids an answer may name. Whose graveyard is not a field — every printed cost of
/// this shape says *your graveyard* — so the candidates are always the payer's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExileCost {
    /// How many cards must be exiled. Never zero — a card with no such cost has no
    /// [`ExileCost`] at all.
    pub count: u8,
    /// The cards in the paying player's graveyard that could pay it.
    pub candidates: Vec<CardInstanceId>,
}

/// The discard `card`'s additional cost demands, or `None` when it has none — which is
/// almost every card.
#[must_use]
pub fn discard_cost(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Option<DiscardCost> {
    let count = db.card(card.card)?.additional_cost?.discard_count();
    if count == 0 {
        return None;
    }
    Some(DiscardCost {
        count,
        candidates: state
            .players
            .get(state.priority.0)?
            .hand
            .iter()
            .filter(|held| held.id != card.id)
            .map(|held| held.id)
            .collect(),
    })
}

/// The permanents the caster may sacrifice to pay `card`'s additional cost, and how many
/// of them the cost takes (CR 601.2b / 701.17).
///
/// The permanent counterpart of [`discard_cost`], and stated for the same reason: the
/// server poses the choice as a slot over an enumerated candidate set, so the client
/// picks from a list it was handed rather than working out what may be sacrificed.
///
/// `None` for a card with no sacrifice cost. The candidate list is every permanent of the
/// named type the caster controls (CR 701.17b — you may sacrifice only your own), which
/// can never include the card being cast: that one is in hand, on its way to the stack.
#[must_use]
pub fn sacrifice_cost(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Option<SacrificeCost> {
    let cost = db.card(card.card)?.additional_cost?;
    let card_type = cost.sacrifice_type()?;
    Some(SacrificeCost {
        count: cost.sacrifice_count().unwrap_or_default(),
        candidates: state.sacrifice_candidates_for_cast(state.priority, card_type, db),
    })
}

/// What still has to be paid to cast `card`, one pip at a time.
///
/// Empty when the pool already covers the cost (CR 605.3 — mana floated first), which is
/// exactly the case where there is nothing left to choose. `None` for a card the database
/// does not hold.
///
/// `x` is the announced value of X (CR 601.2b), which is part of the cost and therefore
/// part of the pips: `{X}{R}` at X = 3 owes four of them. A caller asking before the
/// value is chosen passes `None` and is answered for the base cost, which is what X = 0
/// costs anyway.
#[must_use]
pub fn payment_pips(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
) -> Option<Vec<PaymentPip>> {
    let (cost, subtypes) = cast_cost(state, db, card, x)?;
    let purpose = SpendPurpose::CastingSpell {
        subtypes: &subtypes,
    };
    let pool = state.players.get(state.priority.0)?.mana_pool.clone();
    // Only the part the floating pool does not already cover: a seat holding {W} against
    // a {1}{W} cast is asked for one more mana, not two.
    let owed = pool.remaining_cost(&cost, purpose);
    let options = mana_options(state, db, state.priority);

    let mut pips = Vec::new();
    for (count, color) in [
        (owed.white, Color::White),
        (owed.blue, Color::Blue),
        (owed.black, Color::Black),
        (owed.red, Color::Red),
        (owed.green, Color::Green),
    ] {
        for _ in 0..count {
            pips.push(PaymentPip {
                pip: color.pip().to_string(),
                // A colored pip takes that color and nothing else, restricted mana
                // included only where its restriction allows this spell (CR 106.6).
                candidates: sources_matching(&options, |adds| {
                    adds.color_amount(color) > 0
                        || adds
                            .restricted
                            .iter()
                            .any(|r| r.color == color && r.restriction.allows(purpose))
                }),
            });
        }
    }
    for _ in 0..owed.colorless {
        pips.push(PaymentPip {
            // CR 107.4c: `{C}` is paid with colorless mana specifically — no colored
            // mana pays it, which is what makes it its own pip rather than a generic one.
            pip: "{C}".into(),
            candidates: sources_matching(&options, |adds| adds.colorless > 0),
        });
    }
    for _ in 0..owed.generic {
        pips.push(PaymentPip {
            pip: "{1}".into(),
            // Anything pays a generic pip, so every source is listed — and listed
            // **once**, because which half of a dual land pays it cannot matter and
            // asking would be a question with one meaningful answer.
            candidates: options
                .iter()
                .filter_map(|source| {
                    source.options.first().map(|option| ManaSource {
                        permanent: source.permanent,
                        index: option.index,
                    })
                })
                .collect(),
        });
    }
    Some(pips)
}

/// Every activation whose yield satisfies `wanted`, one entry per matching ability.
fn sources_matching(
    options: &[super::sources::SourceOptions],
    wanted: impl Fn(&crate::mana::ManaPool) -> bool,
) -> Vec<ManaSource> {
    options
        .iter()
        .flat_map(|source| {
            source
                .options
                .iter()
                .filter(|option| wanted(&option.adds))
                .map(|option| ManaSource {
                    permanent: source.permanent,
                    index: option.index,
                })
        })
        .collect()
}

/// The unpaid remainder of `cost` as a printed cost string — `"{1}{W}"` — for a caller
/// that wants the whole line rather than the pips.
#[must_use]
pub fn remaining_cost_pips(pips: &[PaymentPip]) -> String {
    // Printed order, which is the other one: generic reads first on a real card even
    // though the colored pips are the ones a click should fill first.
    let mut generic = 0usize;
    let mut colored = String::new();
    for pip in pips {
        if pip.pip == "{1}" {
            generic += 1;
        } else {
            colored.push_str(&pip.pip);
        }
    }
    if generic > 0 {
        format!("{{{generic}}}{colored}")
    } else {
        colored
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
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

    fn board(
        lands: &[&str],
        spell: &str,
    ) -> (GameState, CardDatabase, Vec<PermanentId>, CardInstance) {
        let database = db();
        let mut state = main_phase();
        let placed = lands
            .iter()
            .map(|slug| place(&mut state, card(&database, slug)))
            .collect();
        let held = state.new_instance(card(&database, spell));
        state.players[0].hand.push(held);
        (state, database, placed, held)
    }

    /// The scenario this whole path exists for: four Plains, a {1}{W} creature in hand,
    /// and nothing floating. Two pips, colored first so a click on a Plains pays the {W}.
    #[test]
    fn a_cost_is_posed_one_pip_at_a_time_most_constrained_first() {
        let (state, database, lands, spell) = board(
            &["plains", "plains", "plains", "plains"],
            "ajani_s_pridemate",
        );
        let pips = payment_pips(&state, &database, spell, None).unwrap();

        assert_eq!(pips.len(), 2, "{{1}}{{W}} is two pips");
        assert_eq!(pips[0].pip, "{W}", "the colored pip comes first");
        assert_eq!(pips[1].pip, "{1}");
        assert_eq!(
            remaining_cost_pips(&pips),
            "{1}{W}",
            "…and reads as printed"
        );
        for pip in &pips {
            assert_eq!(
                pip.candidates.len(),
                4,
                "any of the four Plains pays either"
            );
            assert!(pip
                .candidates
                .iter()
                .all(|source| lands.contains(&source.permanent)));
        }
    }

    /// Mana already floating is cost already paid: it is subtracted from what is posed,
    /// so a seat holding {W} against {1}{W} is asked for one more mana and not two.
    #[test]
    fn floating_mana_is_not_asked_for_again() {
        let (mut state, database, _lands, spell) =
            board(&["plains", "plains"], "ajani_s_pridemate");
        state.players[0].mana_pool.add(Color::White, 1);

        let pips = payment_pips(&state, &database, spell, None).unwrap();
        assert_eq!(pips.len(), 1);
        assert_eq!(pips[0].pip, "{1}");
        assert_eq!(remaining_cost_pips(&pips), "{1}");
    }

    /// A cast the pool already covers poses nothing at all — there is no choice left to
    /// make, which is the CR 605.3 float-first path arriving where it always did.
    #[test]
    fn a_cost_already_covered_poses_no_pips() {
        let (mut state, database, _lands, spell) = board(&[], "ajani_s_pridemate");
        state.players[0].mana_pool.add(Color::White, 1);
        state.players[0].mana_pool.add_colorless(1);
        assert!(payment_pips(&state, &database, spell, None)
            .unwrap()
            .is_empty());
    }

    /// **The dual-land rule.** A dual land is offered twice for a colored pip — the two
    /// halves pay different things, so which one is a real question — and once for a
    /// generic pip, where it is not.
    #[test]
    fn a_dual_land_is_asked_about_only_where_the_answer_matters() {
        let (state, database, lands, spell) =
            board(&["meandering_river", "plains"], "ajani_s_pridemate");
        let pips = payment_pips(&state, &database, spell, None).unwrap();

        let white = &pips[0];
        assert_eq!(white.pip, "{W}");
        let river_ways = white
            .candidates
            .iter()
            .filter(|source| source.permanent == lands[0])
            .count();
        assert_eq!(river_ways, 1, "only the River's white half pays a {{W}}");

        let generic = &pips[1];
        assert_eq!(generic.pip, "{1}");
        let river_ways = generic
            .candidates
            .iter()
            .filter(|source| source.permanent == lands[0])
            .count();
        assert_eq!(
            river_ways, 1,
            "either half pays a generic pip, so it is offered once and nobody is asked"
        );
    }

    /// A source that cannot pay a pip is not offered for it: an Island pays no {W}.
    #[test]
    fn a_source_of_the_wrong_color_is_not_offered_for_a_colored_pip() {
        let (state, database, lands, spell) = board(&["island", "plains"], "ajani_s_pridemate");
        let pips = payment_pips(&state, &database, spell, None).unwrap();

        assert_eq!(pips[0].pip, "{W}");
        assert_eq!(pips[0].candidates.len(), 1);
        assert_eq!(pips[0].candidates[0].permanent, lands[1], "the Plains");
        assert_eq!(pips[1].candidates.len(), 2, "either pays the generic pip");
    }
}
