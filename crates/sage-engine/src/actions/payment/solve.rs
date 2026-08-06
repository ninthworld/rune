//! Finding a payment for a cost, given what the board can tap for.
//!
//! Two callers, one answer. The generator asks *does a payment exist* before it will
//! announce a cast, and the server asks *what is a payment* when it taps for a player
//! (ADR 0010 — the engine answers what is legal, the caller decides whether to use it).
//! Both go through [`ManaOptions`], so "announced" and "payable" cannot come apart. They
//! did once: the offer was gated on an estimate that erred high, and a cast announced
//! against mana no payment could produce is a cast an automated player takes, has
//! refused as a no-op, and takes again for ever.
//!
//! ## Why this is a search and not a sum
//!
//! Adding mana to a pool never makes a cost *less* payable — [`crate::ManaPool::pay_for`]
//! pays each colored requirement from its own color and the generic remainder from
//! anything — so if every source produced one fixed thing, the answer would be a sum:
//! tap everything, ask once. That is what the first version did, and it is wrong for the
//! only interesting sources on the board. A dual land taps for `{W}` **or** `{U}`, and:
//!
//! - summing both halves credits the seat mana it cannot make, which is the over-offer
//!   that livelocks an automated player;
//! - spending both halves is not a payment at all, since the second activation finds the
//!   permanent already tapped — which is why a prefix walk over flattened activations
//!   could not pay with a dual land *at all*, not even for a cost it plainly covers.
//!
//! So the choice per permanent is real and the search is over choices. It stays cheap
//! because of what is being searched: permanents offering a single option are not
//! choices at all (taking them is always safe, by the monotonicity above), and the
//! remaining branching collapses under memoization — ten identical dual lands reach the
//! same handful of pools no matter which order they are considered in.

use std::collections::HashSet;

use crate::card::CardDatabase;
use crate::id::{CardInstance, PlayerId};
use crate::mana::{ManaCost, ManaPool, SpendPurpose};
use crate::state::GameState;

use super::super::definition::{CostPayment, ManaSource};
use super::super::utilities::cast_cost;
use super::sources::{mana_options, SourceOptions};

/// What one seat can tap for, arranged for asking *can this cost be paid* many times.
///
/// Built once per [`crate::valid_actions`] and asked once per castable card, which is
/// the reason it is a type rather than a function: enumerating the board's mana sources
/// is the expensive half and the question is the cheap half.
pub(crate) struct ManaOptions {
    /// The mana already floating. A payment is only ever *additional* to this.
    base: ManaPool,
    /// One entry per permanent that could be tapped, each with the options it offers.
    sources: Vec<SourceOptions>,
    /// `capacity[i]` is the most mana `sources[i..]` could add between them — an
    /// admissible (never-under) bound used to prune the search.
    capacity: Vec<u16>,
}

impl ManaOptions {
    /// Enumerate what `player` could tap for in `state`.
    pub(crate) fn of(state: &GameState, db: &CardDatabase, player: PlayerId) -> Self {
        let base = state
            .players
            .get(player.0)
            .map(|p| p.mana_pool.clone())
            .unwrap_or_default();
        let sources = mana_options(state, db, player);
        let mut capacity = vec![0u16; sources.len() + 1];
        for i in (0..sources.len()).rev() {
            capacity[i] = capacity[i + 1].saturating_add(sources[i].best_yield());
        }
        Self {
            base,
            sources,
            capacity,
        }
    }

    /// Whether some payment covers `cost` — the gate the generator announces a cast
    /// against.
    ///
    /// This is the *same* predicate [`Self::solve`] answers, deliberately and
    /// permanently: an offer gated on anything else is an offer that can disagree with
    /// what a payment can do.
    pub(crate) fn covers(&self, cost: &ManaCost, purpose: SpendPurpose<'_>) -> bool {
        self.solve(cost, purpose).is_some()
    }

    /// A payment that covers `cost`, or `None` if the board cannot pay it.
    ///
    /// The payment is **minimal**: every source in it is one whose removal would leave
    /// the cost unpaid. That matters because this is what the server taps for a player
    /// with — a search that stopped at the first covering assignment would happily tap
    /// six lands to pay for a two-drop.
    ///
    /// It is not necessarily the payment a *person* would choose — it may spend a dual
    /// land where a basic would have done — which is precisely why choosing manually
    /// stays the other path rather than this one becoming the only one.
    pub(crate) fn solve(
        &self,
        cost: &ManaCost,
        purpose: SpendPurpose<'_>,
    ) -> Option<Vec<ManaSource>> {
        if self.base.can_pay_for(cost, purpose) {
            return Some(Vec::new());
        }
        let mut picks = Vec::new();
        let mut exhausted = HashSet::new();
        self.search(&self.base, 0, cost, purpose, &mut picks, &mut exhausted)
            .then(|| self.trim(picks, cost, purpose))
    }

    /// Depth-first over permanents, taking at most one option from each.
    ///
    /// Returns with `picks` holding a covering assignment. Three things keep it cheap,
    /// and each is load-bearing rather than an optimization:
    ///
    /// - it succeeds the moment the pool covers the cost, so it stops at a *prefix* of
    ///   the board rather than walking all of it;
    /// - it prunes on `capacity`, an over-estimate of what the untouched remainder could
    ///   add, so a hopeless branch dies at its root;
    /// - it memoizes on (depth, pool), which is what collapses interchangeable sources.
    ///   Ten Forests reach one pool per count, not 2^10 paths.
    ///
    /// **There is no "skip this permanent" branch, and that is a claim rather than an
    /// omission**: taking a source can never make a cost less payable, so any assignment
    /// that skips a permanent is dominated by one that does not. Minimality is restored
    /// afterwards by [`Self::trim`], where it costs nothing.
    fn search(
        &self,
        pool: &ManaPool,
        depth: usize,
        cost: &ManaCost,
        purpose: SpendPurpose<'_>,
        picks: &mut Vec<ManaSource>,
        exhausted: &mut HashSet<(usize, ManaPool)>,
    ) -> bool {
        if pool.can_pay_for(cost, purpose) {
            return true;
        }
        let Some(source) = self.sources.get(depth) else {
            return false;
        };
        if pool.total().saturating_add(self.capacity[depth]) < total_cost(cost) {
            return false;
        }
        if exhausted.contains(&(depth, pool.clone())) {
            return false;
        }
        for option in &source.options {
            let mut next = pool.clone();
            absorb(&mut next, &option.adds);
            picks.push(ManaSource {
                permanent: source.permanent,
                index: option.index,
            });
            if self.search(&next, depth + 1, cost, purpose, picks, exhausted) {
                return true;
            }
            picks.pop();
        }
        exhausted.insert((depth, pool.clone()));
        false
    }

    /// Drop every source the payment does not need, in reverse order so the cheap tail
    /// of an over-eager assignment goes first.
    ///
    /// A covering assignment is not automatically a minimal one: the search takes each
    /// permanent in board order, so paying `{1}{G}` off a board of Plains and one Forest
    /// picks up Plains on the way to the Forest it actually needed.
    fn trim(
        &self,
        picks: Vec<ManaSource>,
        cost: &ManaCost,
        purpose: SpendPurpose<'_>,
    ) -> Vec<ManaSource> {
        let mut kept = picks;
        let mut index = kept.len();
        while index > 0 {
            index -= 1;
            let mut without = kept.clone();
            without.remove(index);
            if self.pool_of(&without).can_pay_for(cost, purpose) {
                kept = without;
            }
        }
        kept
    }

    /// The pool `payment` would produce, on top of what is already floating.
    fn pool_of(&self, payment: &[ManaSource]) -> ManaPool {
        let mut pool = self.base.clone();
        for source in payment {
            let Some(options) = self
                .sources
                .iter()
                .find(|candidate| candidate.permanent == source.permanent)
            else {
                continue;
            };
            if let Some(option) = options.option(source.index) {
                absorb(&mut pool, &option.adds);
            }
        }
        pool
    }
}

/// Add everything in `adds` to `pool`.
fn absorb(pool: &mut ManaPool, adds: &ManaPool) {
    pool.add(crate::mana::Color::White, adds.white);
    pool.add(crate::mana::Color::Blue, adds.blue);
    pool.add(crate::mana::Color::Black, adds.black);
    pool.add(crate::mana::Color::Red, adds.red);
    pool.add(crate::mana::Color::Green, adds.green);
    pool.add_colorless(adds.colorless);
    for entry in &adds.restricted {
        pool.add_restricted(entry.color, entry.amount, entry.restriction.clone());
    }
}

/// Every unit of mana `cost` demands, generic included.
fn total_cost(cost: &ManaCost) -> u16 {
    cost.colored_total() + u16::from(cost.generic)
}

/// A payment that pays the whole cost of casting `card` right now, or `None` if the
/// board cannot pay for it.
///
/// A **rules** question, not a policy one, and the distinction is the seam: this answers
/// *what would be a legal payment*, exactly as [`crate::legal_targets_for_spec`] answers
/// what would be a legal target. Whether to use the answer — tap and discard for the
/// player, or ask them to pick — is a judgment about a person, and it belongs to the
/// caller (ADR 0010).
///
/// `x` is the value announced for X (CR 601.2b), and it is part of what has to be paid:
/// a payment solved for the wrong X is not a cheaper payment, it is not a payment.
///
/// It is emphatically **a** legal payment and not a good one. The discards in particular
/// are taken in hand order, which is a rule about a list rather than a decision about a
/// game; a caller that hands them to a person without asking has made a choice on their
/// behalf, and should know that it did.
#[must_use]
pub fn auto_payment(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    x: Option<u32>,
) -> Option<Vec<CostPayment>> {
    let (cost, subtypes) = cast_cost(state, db, card, x)?;
    let mana = ManaOptions::of(state, db, state.priority).solve(
        &cost,
        SpendPurpose::CastingSpell {
            subtypes: &subtypes,
        },
    )?;
    let mut payment: Vec<CostPayment> = mana.into_iter().map(CostPayment::Mana).collect();
    payment.extend(auto_discards(state, db, card)?);
    payment.extend(auto_sacrifices(state, db, card)?);
    Some(payment)
}

/// A permanent to sacrifice to `card`'s additional cost (CR 601.2b / 701.17), or `None`
/// if the board cannot pay it. Empty for the overwhelming majority of cards.
///
/// The choice is **the first candidate in battlefield order**, which is a policy and not
/// a judgement: this exists so a client that skips the slot still submits a legal action
/// (ADR 0010 — the engine says what a legal payment is, the server decides whether to pay
/// for the player). A seat that cares which of its creatures dies answers the slot.
fn auto_sacrifices(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Option<Vec<CostPayment>> {
    let Some(card_type) = db
        .card(card.card)
        .and_then(|data| data.additional_cost)
        .and_then(crate::AdditionalCost::sacrifice_type)
    else {
        return Some(Vec::new());
    };
    let chosen = state
        .battlefield
        .iter()
        .find(|perm| {
            crate::characteristics::controller_of(state, perm) == state.priority
                && perm
                    .printed
                    .face(db)
                    .is_some_and(|face| face.has_type(card_type))
        })
        .map(|perm| CostPayment::Sacrifice(perm.id))?;
    Some(vec![chosen])
}

/// Cards to discard to `card`'s additional cost (CR 601.2b), or `None` if the hand
/// cannot pay it. Empty for the overwhelming majority of cards, which have no such cost.
fn auto_discards(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
) -> Option<Vec<CostPayment>> {
    let owed = db
        .card(card.card)
        .and_then(|data| data.additional_cost)
        .map_or(0, crate::AdditionalCost::discard_count);
    if owed == 0 {
        return Some(Vec::new());
    }
    // The card being cast is on its way to the stack and cannot pay for itself.
    let chosen: Vec<CostPayment> = state
        .players
        .get(state.priority.0)?
        .hand
        .iter()
        .filter(|held| held.id != card.id)
        .take(usize::from(owed))
        .map(|held| CostPayment::Discard(held.id))
        .collect();
    (chosen.len() == usize::from(owed)).then_some(chosen)
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::actions::payment::payment_sources;
    use crate::apply_action;
    use crate::id::PermanentId;
    use crate::phase::Step;
    use crate::state::Permanent;
    use crate::{valid_actions, Action, CardId};

    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    fn card(db: &CardDatabase, slug: &str) -> CardId {
        db.card_id(&slug.to_string().try_into().unwrap()).unwrap()
    }

    /// A two-player main phase with player 0 holding priority, past summoning sickness.
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
            damage: 0,
            counters: Default::default(),
            attached_to: None,
            chosen_color: None,
        });
        id
    }

    /// Put `lands` on the battlefield and `spell` in hand.
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

    /// **The bug this solver exists for.** A dual land taps for one of two colors, and a
    /// payment has to be able to pick which. The flat-activation model could not pay with
    /// one at all: it tried to spend both halves, found the permanent already tapped, and
    /// reported that no payment existed.
    #[test]
    fn a_dual_land_can_pay_for_a_spell() {
        // Meandering River ({T}: Add {W} / {T}: Add {U}) plus a Plains, casting {1}{W}.
        let (state, database, _lands, spell) =
            board(&["meandering_river", "plains"], "daybreak_chaplain");
        let payment = auto_payment(&state, &database, spell, None)
            .expect("Plains for {W} and the River for the generic pays {1}{W}");

        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment,
            },
            &database,
        );
        assert_eq!(after.stack.len(), 1, "the spell was cast");
    }

    /// And two duals pay a two-colored-in-effect cost between them, one taking each half.
    #[test]
    fn two_dual_lands_split_between_the_pips() {
        let (state, database, _lands, spell) = board(
            &["meandering_river", "meandering_river"],
            "daybreak_chaplain",
        );
        let payment = auto_payment(&state, &database, spell, None).expect("two Rivers pay {1}{W}");
        assert_eq!(payment.len(), 2);
        let after = apply_action(
            &state,
            &Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment,
            },
            &database,
        );
        assert_eq!(after.stack.len(), 1);
    }

    /// One dual land is one mana, not two. Crediting both halves is what announced a cast
    /// no payment could pay for.
    #[test]
    fn one_dual_land_is_one_mana_and_not_two() {
        let (state, database, _lands, spell) = board(&["meandering_river"], "daybreak_chaplain");
        assert!(
            auto_payment(&state, &database, spell, None).is_none(),
            "{{1}}{{W}} needs two mana and a lone dual land makes one"
        );
        assert!(
            !valid_actions(&state, &database).contains(&Action::CastSpell {
                card: spell,
                mode: None,
                x: None,
                targets: Vec::new(),
                payment: Vec::new(),
            }),
            "and the cast is therefore not announced"
        );
    }

    /// The payment the server taps for a player is the one they would have assembled:
    /// no source in it is spare.
    #[test]
    fn an_auto_payment_taps_no_more_than_it_has_to() {
        let (state, database, _lands, spell) =
            board(&["forest", "forest", "forest", "forest"], "highland_game");
        let payment =
            auto_payment(&state, &database, spell, None).expect("four Forests pay {1}{G}");
        assert_eq!(payment.len(), 2, "{{1}}{{G}} is two mana, so two Forests");
    }

    /// A colored pip is paid in kind even when the board is mostly the wrong color: the
    /// one Forest has to be in the payment, because nothing else can pay `{G}`.
    ///
    /// What pays the *generic* half is deliberately not asserted. A Plains is as good as
    /// a second Forest there, and picking between them is a judgment about a person's
    /// plans rather than a rule — which is the whole reason choosing manually stays the
    /// other path.
    #[test]
    fn a_colored_pip_is_paid_by_the_only_source_of_that_color() {
        let (state, database, lands, spell) =
            board(&["plains", "plains", "forest"], "highland_game");
        let payment =
            auto_payment(&state, &database, spell, None).expect("a Forest and any other pays");
        assert_eq!(payment.len(), 2, "{{1}}{{G}} is two mana");
        assert!(
            payment
                .iter()
                .filter_map(|entry| entry.mana())
                .any(|source| source.permanent == lands[2]),
            "the Forest is the only thing on the board that can pay the {{G}}"
        );
    }

    /// **The invariant, over the whole catalog**: every cast the generator announces has
    /// a payment that exists. When these two answers come apart, an automated player
    /// takes the offer, has it refused as a no-op, and takes it again for ever.
    #[test]
    fn every_announced_cast_has_a_payment_that_exists() {
        let database = db();
        // Every land in the catalog that taps for mana, one of each, plus a
        // summoning-sick mana creature — the board that broke the first version.
        let lands = [
            "plains",
            "island",
            "swamp",
            "mountain",
            "forest",
            "meandering_river",
            "timber_gorge",
            "cinder_barrens",
            "submerged_boneyard",
            "tranquil_expanse",
        ];
        for count in 0..=3 {
            let mut state = main_phase();
            for slug in lands.iter().take(count) {
                place(&mut state, card(&database, slug));
            }
            let sick = place(&mut state, card(&database, "llanowar_elves"));
            state
                .battlefield
                .iter_mut()
                .find(|p| p.id == sick)
                .unwrap()
                .entered_turn = state.turn;
            for slug in [
                "daybreak_chaplain",
                "highland_game",
                "llanowar_elves",
                "omenspeaker",
                "colossal_dreadmaw",
            ] {
                let held = state.new_instance(card(&database, slug));
                state.players[0].hand.push(held);
            }

            for action in valid_actions(&state, &database) {
                let Action::CastSpell { card: held, .. } = action else {
                    continue;
                };
                assert!(
                    auto_payment(&state, &database, held, None).is_some(),
                    "with {count} lands, a cast is announced that no payment can pay for"
                );
            }
        }
    }

    /// The offer and the payment are derived from one enumeration, and this pins the
    /// other half of that: every source the enumeration offers is one `valid_actions`
    /// would also let a player activate on its own. The two must not drift, because
    /// `apply_payment` validates a submitted payment against `valid_actions`.
    #[test]
    fn payment_sources_are_all_activatable() {
        let database = db();
        let mut state = main_phase();
        for slug in ["forest", "meandering_river", "llanowar_elves"] {
            place(&mut state, card(&database, slug));
        }
        // One summoning-sick creature, which offers nothing.
        let sick = place(&mut state, card(&database, "llanowar_elves"));
        state
            .battlefield
            .iter_mut()
            .find(|p| p.id == sick)
            .unwrap()
            .entered_turn = state.turn;

        let offered = valid_actions(&state, &database);
        let sources = payment_sources(&state, &database);
        assert!(!sources.is_empty());
        for source in sources {
            assert!(
                offered.contains(&Action::ActivateAbility {
                    permanent: source.permanent,
                    index: source.index,
                    targets: Vec::new(),
                    payment: Vec::new(),
                }),
                "{source:?} is offered as a payment source but not as an activation"
            );
            assert!(
                source.permanent != sick,
                "a summoning-sick creature is not a payment source (CR 302.6)"
            );
        }
    }

    /// A permanent appears once per option, which is what lets a client ask *which
    /// color* when a player clicks a dual land.
    #[test]
    fn a_dual_land_offers_one_source_per_color() {
        let (state, database, lands, _spell) = board(&["meandering_river"], "daybreak_chaplain");
        let sources = payment_sources(&state, &database);
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|source| source.permanent == lands[0]));
    }
}
