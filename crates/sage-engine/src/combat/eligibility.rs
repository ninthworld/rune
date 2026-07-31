use crate::card::{CombatRestriction, Keyword};
use crate::card_type::CardType;
use crate::id::{PermanentId, PlayerId};
use crate::state::GameState;
use crate::CardDatabase;

use super::helpers::{permanent_has_restriction, summoning_sickness_restricts};

/// **What** an attacker was declared to attack (CR 508.1a): a defending player, or a
/// planeswalker one of them controls.
///
/// The one type that carries the CR 508.1a generalization the rest of combat is written
/// against. Before planeswalkers were modeled an attack named a bare
/// [`PlayerId`](crate::PlayerId), and every downstream rule — blocker eligibility,
/// damage routing, the multi-defender declaration order — read that id as *both* "what
/// is being attacked" and "who declares blockers against it". Those are different
/// questions the instant a planeswalker can be attacked: the thing attacked is the
/// planeswalker, and the player who blocks for it is its controller. Splitting them is
/// what [`attacking_defender_of`] (the player) and [`attack_target_of`] (the thing) now
/// answer separately.
///
/// Plain `Copy`/`Eq` data, so a [`Permanent`](crate::Permanent) and an
/// [`Attack`](crate::Attack) both stay values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackTarget {
    /// A defending player (CR 508.1a) — every attack before planeswalkers, and still
    /// the overwhelming majority.
    Player(PlayerId),
    /// A **planeswalker** an opponent controls. The player who declares blockers
    /// against this attacker is that planeswalker's controller, and the attacker's
    /// combat damage removes loyalty rather than costing life (CR 120.3c).
    Planeswalker(PermanentId),
}

impl AttackTarget {
    /// The **defending player** this target belongs to (CR 508.1a): the player
    /// themselves, or the controller of the attacked planeswalker.
    ///
    /// `None` when the attacked planeswalker is no longer on the battlefield — the
    /// state an attacker is left in when its planeswalker dies mid-combat, and exactly
    /// the state that stops it dealing damage anywhere.
    #[must_use]
    pub fn defending_player(self, state: &GameState) -> Option<PlayerId> {
        match self {
            Self::Player(player) => Some(player),
            Self::Planeswalker(id) => state
                .battlefield
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.controller),
        }
    }
}

/// The players an attacker may legally be declared to attack (CR 508.1a): every
/// opponent still in the game — a seat other than the active (attacking) player
/// that has not lost. In seat order, so the enumeration is deterministic.
///
/// The **player half** of [`defender_candidates`], and the frame of reference the
/// planeswalker half is drawn from: only a planeswalker one of *these* players controls
/// may be attacked. Kept separate because several callers want the seats and not the
/// objects — who declares blockers, whether there is a single defending player, whether
/// an attack is possible at all.
#[must_use]
pub fn defending_player_candidates(state: &GameState) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .enumerate()
        .filter(|(seat, player)| PlayerId(*seat) != state.active_player && !player.has_lost)
        .map(|(seat, _)| PlayerId(seat))
        .collect()
}

/// Everything an attacker may legally be declared to attack (CR 508.1a): every opponent
/// still in the game, followed by every planeswalker those opponents control, in seat
/// order then battlefield order so the enumeration is deterministic.
///
/// In a two-player game with no planeswalkers this is exactly the sole opponent, so the
/// only legal assignment for every attacker is that one and combat plays as it always
/// has. A second candidate appears the moment an opponent has a planeswalker — which is
/// why the server's per-attacker defender slot is keyed on *this* list's length rather
/// than on the number of opponents (issue #608): a two-player game with a planeswalker
/// on the other side is a real choice.
///
/// A player may never attack themselves, an eliminated player, or their own
/// planeswalker, so none of those is a candidate.
#[must_use]
pub fn defender_candidates(state: &GameState, db: &CardDatabase) -> Vec<AttackTarget> {
    let players = defending_player_candidates(state);
    let mut candidates: Vec<AttackTarget> =
        players.iter().copied().map(AttackTarget::Player).collect();
    candidates.extend(
        state
            .battlefield
            .iter()
            .filter(|perm| {
                players.contains(&perm.controller)
                    && perm
                        .printed
                        .face(db)
                        .is_some_and(|face| face.has_type(CardType::Planeswalker))
            })
            .map(|perm| AttackTarget::Planeswalker(perm.id)),
    );
    candidates
}

/// The single defending player of a two-player combat: the one opponent still in
/// the game (CR 508.1). `None` when there is not exactly one eligible defender —
/// on a state with fewer than two seats, or (once multiplayer combat lands) more
/// than one opponent, where there is no *single* defender and callers must consult
/// [`defender_candidates`] / each attacker's own [`crate::state::Permanent::attacking`]
/// target instead. This keeps every two-player code path (blocker declaration flow,
/// server view binding) working unchanged while the multi-defender flow (#344)
/// builds on the per-attacker targets.
#[must_use]
pub fn defending_player(state: &GameState) -> Option<PlayerId> {
    let candidates = defending_player_candidates(state);
    match candidates.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// The permanents the active player may legally declare as attackers right now
/// (CR 508.1a): creatures they control that are untapped and free of summoning
/// sickness (CR 302.6). In stable battlefield order.
///
/// This is the multi-select candidate set for the declare-attackers action — one
/// O(N) scan of the battlefield, never a product over selections. Haste (CR
/// 702.10b) exempts a creature from the summoning-sickness restriction; defender
/// (CR 702.3b) and a [`CombatRestriction::CantAttack`] imposition (CR 506.3a) each
/// remove a creature from the set outright.
///
/// Attack *requirements* — "attacks each combat if able" — are a constraint from the
/// other direction and are not modeled: nothing here can force a creature into the
/// declaration.
#[must_use]
pub fn attacker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let active = state.active_player;
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == active
                && super::helpers::is_creature(perm, db)
                && !perm.tapped
                // CR 302.6, with the CR 702.10b haste exemption: a hasty creature
                // ignores the summoning-sickness attack restriction.
                && !summoning_sickness_restricts(state, perm, db)
                // CR 702.3b: a creature with defender can't attack. Read through the
                // computed keywords (CR 613.1f), so a *granted* defender restricts
                // exactly as a printed one does — and stops doing so the instant the
                // grant ends.
                && !super::helpers::has_keyword(state, perm, Keyword::Defender, db)
                // CR 506.3a: the same prohibition without the keyword, as an Aura or a
                // spell imposes it. Read through the same computed characteristics.
                && !permanent_has_restriction(state, perm.id, CombatRestriction::CantAttack, db)
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents `defender` may legally declare as blockers right now
/// (CR 509.1a): untapped creatures they control (a tapped creature can't block) that
/// are not under a [`CombatRestriction::CantBlock`] imposition (CR 506.3c). In stable
/// battlefield order.
///
/// This is the per-defender blocker candidate set: a player may block only with
/// their own creatures, and (enforced in the declaration's legality check, not
/// here) only against attackers attacking *them* (issue #341). The multi-defender
/// declaration flow (#344) calls this once per attacked player.
///
/// "Can't block" is a fact about the creature rather than about a pairing, so unlike
/// the evasion rules it belongs in the candidate set — a creature that can't block has
/// no attacker it could be assigned to. [`blocker_can_block_attacker`](super::blocker_can_block_attacker)
/// re-derives it anyway, so the two gates cannot disagree.
#[must_use]
pub fn blocker_candidates_for(
    state: &GameState,
    defender: PlayerId,
    db: &CardDatabase,
) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| {
            perm.controller == defender
                && super::helpers::is_creature(perm, db)
                && !perm.tapped
                && !permanent_has_restriction(state, perm.id, CombatRestriction::CantBlock, db)
        })
        .map(|perm| perm.id)
        .collect()
}

/// The permanents the sole defending player of a two-player combat may legally
/// declare as blockers (CR 509.1a). Empty when there is no single defender (see
/// [`defending_player`]). A convenience over [`blocker_candidates_for`] for the
/// two-player declaration flow and server view binding; the multi-defender flow
/// (#344) uses [`blocker_candidates_for`] per attacked player.
#[must_use]
pub fn blocker_candidates(state: &GameState, db: &CardDatabase) -> Vec<PermanentId> {
    let Some(defender) = defending_player(state) else {
        return Vec::new();
    };
    blocker_candidates_for(state, defender, db)
}

/// The permanents currently declared as attackers, in stable battlefield order —
/// the legal set of creatures a blocker may be assigned to block (CR 509.1a).
#[must_use]
pub fn declared_attackers(state: &GameState) -> Vec<PermanentId> {
    state
        .battlefield
        .iter()
        .filter(|perm| perm.attacking.is_some())
        .map(|perm| perm.id)
        .collect()
}

/// **What** the permanent `attacker` is attacking this combat (CR 508.1a) — a player or
/// a planeswalker — or `None` if it is not on the battlefield or is not an attacker.
/// This is where its combat damage goes.
#[must_use]
pub fn attack_target_of(state: &GameState, attacker: PermanentId) -> Option<AttackTarget> {
    state
        .battlefield
        .iter()
        .find(|p| p.id == attacker)
        .and_then(|p| p.attacking)
}

/// The **defending player** the permanent `attacker` is attacking this combat
/// (CR 508.1a): the attacked player, or the controller of the attacked planeswalker.
/// This is the player whose creatures may block it.
///
/// `None` if `attacker` is not attacking — and also once the planeswalker it was
/// attacking has left the battlefield, since there is then no seat that answers for it.
/// That second case is why the blocker and damage paths can stay written against a
/// defending *player* without either of them re-deriving the rule.
#[must_use]
pub fn attacking_defender_of(state: &GameState, attacker: PermanentId) -> Option<PlayerId> {
    attack_target_of(state, attacker).and_then(|target| target.defending_player(state))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::CombatRestriction;
    use crate::fixtures::fixture;
    use crate::state::Permanent;

    /// Put a creature (Walking Corpse, a vanilla 2/2 with no combat keyword) on the
    /// battlefield under `controller` with the given tapped state, having entered on
    /// turn `entered_turn`.
    fn creature(
        state: &mut GameState,
        controller: PlayerId,
        tapped: bool,
        entered_turn: u32,
    ) -> PermanentId {
        let inst = state.new_instance(fixture("walking_corpse"));
        let id = PermanentId(state.mint_id());
        state.battlefield.push(Permanent {
            id,
            instance: inst.id,
            printed: fixture("walking_corpse").into(),
            controller,
            tapped,
            entered_turn,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: Default::default(),
            attached_to: None,
        });
        id
    }

    /// The bundled card database, for tests that need oracle data.
    fn db() -> CardDatabase {
        CardDatabase::bundled().unwrap()
    }

    #[test]
    fn attacker_candidates_exclude_sick_and_tapped_creatures_cr_508_1a() {
        // CR 508.1a / 302.6: only the active player's untapped, non-sick creatures
        // are eligible attackers.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(0), false, 1);
        let _sick = creature(&mut state, PlayerId(0), false, 2);
        let _tapped = creature(&mut state, PlayerId(0), true, 1);
        let _opponents = creature(&mut state, PlayerId(1), false, 1);

        assert_eq!(attacker_candidates(&state, &db()), vec![eligible]);
    }

    #[test]
    fn blocker_candidates_exclude_tapped_creatures_cr_509_1a() {
        // CR 509.1a: a tapped creature can't block. Only the defender's untapped
        // creatures are eligible; summoning sickness does not stop blocking.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let eligible = creature(&mut state, PlayerId(1), false, 2); // sick but can block
        let _tapped = creature(&mut state, PlayerId(1), true, 1);
        let _attackers_creature = creature(&mut state, PlayerId(0), false, 1);

        assert_eq!(blocker_candidates(&state, &db()), vec![eligible]);
    }

    /// Impose `restriction` on `id` until end of turn, the way an Aura or a spell
    /// does, so a test can assert the *granted* form behaves as a printed one.
    fn impose(state: &mut GameState, id: PermanentId, restriction: CombatRestriction) {
        let source = state.mint_id();
        state.static_effects.push(crate::state::StaticEffect {
            source,
            affects: crate::state::EffectAffects::SpecificPermanent(id),
            modification: crate::state::Modification::GrantRestriction(restriction),
            duration: crate::state::Duration::UntilEndOfTurn,
        });
    }

    #[test]
    fn issue_606_a_creature_that_cant_attack_leaves_the_attacker_candidates() {
        // CR 506.3a, enforced in exactly one place — the same place defender is. The
        // neighbour that shares every other characteristic stays a candidate, so this
        // is the restriction talking and not the fixture.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let bound = creature(&mut state, PlayerId(0), false, 1);
        let free = creature(&mut state, PlayerId(0), false, 1);

        assert_eq!(attacker_candidates(&state, &db()), vec![bound, free]);
        impose(&mut state, bound, CombatRestriction::CantAttack);
        assert_eq!(
            attacker_candidates(&state, &db()),
            vec![free],
            "the restricted creature is no longer offered"
        );
    }

    #[test]
    fn issue_606_a_creature_that_cant_block_leaves_the_blocker_candidates() {
        // CR 506.3c, the blocking counterpart. Unlike the evasion rules this *is* a
        // candidate-set fact: a creature that can't block has no attacker it could be
        // assigned to.
        let mut state = GameState::new_two_player();
        state.turn = 2;
        let silenced = creature(&mut state, PlayerId(1), false, 1);
        let free = creature(&mut state, PlayerId(1), false, 1);

        assert_eq!(blocker_candidates(&state, &db()), vec![silenced, free]);
        impose(&mut state, silenced, CombatRestriction::CantBlock);
        assert_eq!(blocker_candidates(&state, &db()), vec![free]);
    }

    #[test]
    fn defender_is_the_sole_opponent() {
        let state = GameState::new_two_player();
        assert_eq!(defending_player(&state), Some(PlayerId(1)));
        assert_eq!(defending_player(&GameState::default()), None);
    }

    #[test]
    fn issue_341_defender_candidates_are_every_living_opponent_cr_508_1a() {
        // CR 508.1a: an attacker may be declared to attack any opponent still in the
        // game — never the active player, never an eliminated one.
        let mut state = GameState::new_multiplayer(3);
        state.active_player = PlayerId(0);
        assert_eq!(
            defender_candidates(&state, &db()),
            vec![
                AttackTarget::Player(PlayerId(1)),
                AttackTarget::Player(PlayerId(2))
            ],
            "both opponents of the active player are candidates"
        );
        // A two-player game with no planeswalker has exactly one defender candidate —
        // the sole opponent — so `defending_player` resolves and combat plays as it
        // always has.
        let two = GameState::new_two_player();
        assert_eq!(
            defender_candidates(&two, &db()),
            vec![AttackTarget::Player(PlayerId(1))]
        );
        assert_eq!(defending_player(&two), Some(PlayerId(1)));
        // With more than one opponent there is no single defender.
        assert_eq!(defending_player(&state), None);

        // An eliminated opponent drops out of the candidate set.
        state.players[1].has_lost = true;
        assert_eq!(
            defender_candidates(&state, &db()),
            vec![AttackTarget::Player(PlayerId(2))]
        );
    }

    #[test]
    fn issue_341_blocker_candidates_are_per_defender() {
        // Blocker candidates for a defending player include only that player's own
        // untapped creatures (issue #341); the per-attacker scoping is enforced in
        // the declaration's legality check.
        let db = db();
        let mut state = GameState::new_multiplayer(3);
        let seat1_creature = creature(&mut state, PlayerId(1), false, 0);
        let seat2_creature = creature(&mut state, PlayerId(2), false, 0);

        assert_eq!(
            blocker_candidates_for(&state, PlayerId(1), &db),
            vec![seat1_creature]
        );
        assert_eq!(
            blocker_candidates_for(&state, PlayerId(2), &db),
            vec![seat2_creature]
        );
    }
}
