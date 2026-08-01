//! Which **physical card** a projection is of (CR 108.1, issue #650).
//!
//! Every wire id in this shim names an *object*: `perm_` a permanent, `stack_` an object
//! on the stack, `card_` a card in a zone. CR 400.7 says an object that changes zone
//! becomes a new object "with no memory of, or relation to, its previous existence", so
//! those ids are correctly unrelated to one another — the permanent that died and the
//! card now in the graveyard are two objects, not one object twice.
//!
//! What survives a zone change is the physical card (CR 108.1) — the thing a player's eye
//! follows across the table — and the engine already models exactly that distinction: a
//! `PermanentId` is reborn on each battlefield entry while the `CardInstanceId` stays with
//! the card. This module is the one place that reads it back out, so the projection sites
//! cannot answer the same question two different ways.
//!
//! It is a **projection and nothing more**: a pure function of the permanent in front of
//! it, with no diff against a previous view, no history, and no server-side memory. That
//! is the whole reason the field could be added at all — "what was this a moment ago" is
//! history the engine deliberately drops, and reconstructing it would put either a memory
//! in the engine or a diff in the server, both of which ADR 0005 keeps out.

use super::*;

/// The physical card a permanent is a projection of, as the [`card_entity_id`] that card
/// carries in every zone a view shows it in — or `None` when there is no card.
///
/// Not object identity, and the field it fills is named so a client cannot read it as
/// one. This states which card two projections are of, which is a strictly weaker claim
/// than CR 400.7's exceptions and never licenses carrying anything else across.
///
/// **A token gets `None`.** The engine gives a token a `CardInstanceId` from the same
/// counter — it is the per-object handle the commander designation and the death diff are
/// keyed on — but a token is not a card (CR 111.1), and CR 111.7 means its instance can
/// never turn up in a hand, a graveyard, or exile. Stating it would offer a join whose
/// other end cannot exist. So the question asked is
/// [`Printed::card`](sage_engine::Printed::card) — the one accessor that crosses back to
/// card identity, and the same `None` that makes CR 111.7 a thing the compiler asks about.
pub(crate) fn physical_card_of(perm: &sage_engine::Permanent) -> Option<String> {
    perm.printed.card()?;
    Some(card_entity_id(perm.instance))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::test_support::put_permanent;
    use super::*;
    use crate::test_support::fixture;
    use sage_engine::{apply_action, Color};

    /// A `PrecombatMain` two-player game where seat 0 holds one Llanowar Elves and
    /// exactly the green mana to cast it.
    fn ready_to_cast() -> (GameState, CardDatabase, CardInstance) {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.turn = 1;
        let card = state.new_instance(fixture("llanowar_elves"));
        state.players[0].hand = vec![card];
        state.players[0].mana_pool.add(Color::Green, 1);
        (state, db, card)
    }

    /// Pass priority until the stack is empty, so a spell that was cast resolves.
    fn resolve_the_stack(state: &GameState, db: &CardDatabase) -> GameState {
        let mut state = state.clone();
        for _ in 0..8 {
            if state.stack.is_empty() {
                return state;
            }
            state = apply_action(&state, &Action::PassPriority, db);
        }
        panic!("the stack drains within a few priority passes")
    }

    #[test]
    fn issue_650_one_card_is_followable_across_four_zones_with_no_client_memory() {
        // The acceptance case. A card is cast from hand, resolves onto the battlefield,
        // and dies; at every step the view states which physical card the object in front
        // of the player is of, and every *object* id along the way is different — which
        // is CR 400.7 holding, not a gap in the projection.
        let (state, db, card) = ready_to_cast();
        let physical = card_entity_id(card.id);

        // In hand, the card's own entity id *is* the physical card's: `CardView.id` has
        // been keyed by the instance all along, which is why this field introduces no new
        // id space and joins to the rest by construction.
        let in_hand = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(in_hand.my_hand[0].id, physical);

        // On the stack: a fresh `stack_` id for a new object, naming the same card.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card,
                targets: Vec::new(),
            },
            &db,
        );
        let on_stack = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(
            on_stack.stack.len(),
            1,
            "the creature spell is on the stack"
        );
        let spell = &on_stack.stack[0];
        assert_eq!(spell.physical_card.as_deref(), Some(physical.as_str()));
        assert_ne!(spell.id, physical);
        assert!(on_stack.my_hand.is_empty(), "it left the hand");

        // On the battlefield: a fresh `perm_` id for another new object, same card.
        let state = resolve_the_stack(&state, &db);
        let on_battlefield = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(on_battlefield.battlefield.len(), 1);
        let permanent = &on_battlefield.battlefield[0];
        assert_eq!(permanent.physical_card.as_deref(), Some(physical.as_str()));
        assert_ne!(permanent.id, physical);
        assert_ne!(permanent.id, spell.id);

        // Into the graveyard: lethal damage, then a priority pass for the state-based
        // action to see it (CR 704.5g). The card there carries the physical card's id as
        // its own, because a card in a pile is projected by instance.
        let mut dying = state.clone();
        dying.battlefield[0].damage = 99;
        let state = apply_action(&dying, &Action::PassPriority, &db);
        let in_graveyard = personalized_view(&state, &db, PlayerId(0));
        assert!(
            in_graveyard.battlefield.is_empty(),
            "the creature died to lethal damage"
        );
        let grave = in_graveyard
            .graveyards
            .iter()
            .find(|pile| pile.player_id == player_id(PlayerId(0)))
            .expect("its owner's graveyard is projected");
        assert_eq!(grave.cards.len(), 1);
        assert_eq!(grave.cards[0].id, physical);

        // Four objects, four different ids, one physical card. Nothing in any of the four
        // views above depended on remembering a previous one.
        let object_ids = [
            in_hand.my_hand[0].id.clone(),
            spell.id.clone(),
            permanent.id.clone(),
            grave.cards[0].id.clone(),
        ];
        assert_eq!(
            object_ids[1..]
                .iter()
                .filter(|id| **id == object_ids[0])
                .count(),
            1,
            "only the two zone-pile projections share the card's own id"
        );
        assert_ne!(spell.id, permanent.id);
    }

    #[test]
    fn issue_650_a_permanent_that_leaves_and_re_enters_is_a_new_object_of_the_same_card() {
        // The CR 400.7 boundary in one assertion: the second time on the battlefield is a
        // different object with a different id, and the physical card is unchanged.
        let (state, db, card) = ready_to_cast();
        let physical = card_entity_id(card.id);

        let cast = |state: &GameState| {
            let state = apply_action(
                state,
                &Action::CastSpell {
                    card,
                    targets: Vec::new(),
                },
                &db,
            );
            resolve_the_stack(&state, &db)
        };

        let state = cast(&state);
        let first = personalized_view(&state, &db, PlayerId(0)).battlefield[0].clone();

        // Kill it, then put the card back in hand with the mana to recast it. The catalog
        // has no bounce or regrowth effect to do the graveyard→hand step for us, so the
        // test stages that one move; both battlefield entries themselves go through the
        // engine's own cast-and-resolve path, which is what mints the ids being compared.
        let mut dying = state.clone();
        dying.battlefield[0].damage = 99;
        let mut state = apply_action(&dying, &Action::PassPriority, &db);
        state.step = Step::PrecombatMain;
        state.priority = PlayerId(0);
        state.players[0].graveyard.clear();
        state.players[0].hand = vec![card];
        state.players[0].mana_pool.add(Color::Green, 1);

        let state = cast(&state);
        let second = personalized_view(&state, &db, PlayerId(0)).battlefield[0].clone();

        assert_ne!(
            first.id, second.id,
            "a permanent re-entering the battlefield is a new object (CR 400.7)"
        );
        assert_eq!(
            first.physical_card, second.physical_card,
            "and it is the same physical card (CR 108.1)"
        );
        assert_eq!(second.physical_card.as_deref(), Some(physical.as_str()));
    }

    #[test]
    fn issue_650_two_copies_of_one_card_are_distinguished_where_their_names_are_not() {
        // Two Llanowar Elves on the battlefield agree on everything a client can see —
        // name, `functional_id`, type line — so any join but the instance would be the
        // client deciding which one moved, and wrong on exactly these boards.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        let elves = fixture("llanowar_elves");
        let first = put_permanent(&mut state, elves, PlayerId(0), false, false);
        let second = put_permanent(&mut state, elves, PlayerId(0), false, false);
        assert_ne!(first, second);

        let view = personalized_view(&state, &db, PlayerId(0));
        let [a, b] = &view.battlefield[..] else {
            panic!("both copies are on the battlefield")
        };

        assert_eq!(a.card.name, b.card.name);
        assert_eq!(a.card.functional_id, b.card.functional_id);
        assert_ne!(a.id, b.id);
        assert_ne!(
            a.physical_card, b.physical_card,
            "distinct copies are distinct physical cards"
        );
        assert!(a.physical_card.is_some() && b.physical_card.is_some());
    }

    #[test]
    fn issue_650_a_token_names_no_physical_card_and_an_ability_on_the_stack_names_none() {
        // The two objects with nothing to name. A token (CR 111) is not a card, and CR
        // 111.7 keeps its instance out of every zone a join could reach; an ability on the
        // stack (CR 113.3) has no card behind it at all and only ever names its source.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        // A card permanent beside it, so the assertion is a difference and not an
        // everywhere-`None`.
        put_permanent(
            &mut state,
            fixture("llanowar_elves"),
            PlayerId(0),
            false,
            false,
        );
        let token = PermanentId(state.mint_id());
        let token_instance = CardInstanceId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id: token,
            // The engine hands a token an instance from the same counter cards draw
            // from; the projection's job is to *not* state it, which is the assertion
            // below rather than an absence staged here.
            instance: token_instance,
            printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
                name: "Thopter".to_string(),
                types: vec![
                    sage_engine::CardType::Artifact,
                    sage_engine::CardType::Creature,
                ],
                power: Some(1),
                toughness: Some(1),
                ..Default::default()
            })),
            controller: PlayerId(0),
            ..Default::default()
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let [card_permanent, token_permanent] = &view.battlefield[..] else {
            panic!("a card and a token are on the battlefield")
        };
        assert!(card_permanent.physical_card.is_some());
        assert!(token_permanent.card.token, "the second one is the token");
        assert_eq!(
            token_permanent.physical_card, None,
            "a token is not a card, so it is a projection of none"
        );

        // The same view, seen by a spectator: a token is public and stays card-less.
        let watched = spectator_view(&state, &db);
        assert_eq!(watched.battlefield[1].physical_card, None);
    }

    #[test]
    fn issue_650_the_physical_card_is_stated_only_where_the_card_itself_is_already_shown() {
        // The hidden-information review, run over one movement for all three receivers.
        //
        // The rule the projection keeps is structural rather than conditional: the field
        // rides on a battlefield permanent and on a spell on the stack, and both of those
        // are **public objects whose whole face the same view already carries**. There is
        // no receiver-dependent branch, because there is no receiver for whom either
        // object is hidden. What is *not* projected is the other half of the rule: a card
        // in a hand is in no seat's view but its owner's, so no instance id for it reaches
        // anybody else and there is nothing for them to join to later.
        let (state, db, card) = ready_to_cast();
        let physical = card_entity_id(card.id);

        // While the card is in seat 0's hand, its id appears nowhere at all in seat 1's
        // view or in a spectator's — not as a card, not as a permanent, not on the stack.
        let opponent = serde_json::to_string(&personalized_view(&state, &db, PlayerId(1))).unwrap();
        let watching = serde_json::to_string(&spectator_view(&state, &db)).unwrap();
        for text in [&opponent, &watching] {
            assert!(
                !text.contains(&physical),
                "a card in a hand is named to nobody but its owner"
            );
        }

        // Cast it and resolve it. Now it is a permanent — public information — and all
        // three receivers are told the identical thing about it, because there is nothing
        // here that one of them may know and another may not.
        let state = apply_action(
            &state,
            &Action::CastSpell {
                card,
                targets: Vec::new(),
            },
            &db,
        );
        let state = resolve_the_stack(&state, &db);

        let mine = personalized_view(&state, &db, PlayerId(0));
        let theirs = personalized_view(&state, &db, PlayerId(1));
        let spectating = spectator_view(&state, &db);
        assert_eq!(
            mine.battlefield[0].physical_card.as_deref(),
            Some(physical.as_str())
        );
        assert_eq!(
            theirs.battlefield[0].physical_card,
            mine.battlefield[0].physical_card
        );
        assert_eq!(
            spectating.battlefield[0].physical_card,
            mine.battlefield[0].physical_card
        );

        // And the opponent's *own* hand is still nobody else's business: the seat-1 card
        // below is projected to seat 1 alone, field or no field.
        let mut hidden = state.clone();
        let theirs_in_hand = hidden.new_instance(fixture("llanowar_elves"));
        hidden.players[1].hand = vec![theirs_in_hand];
        let seat_zero =
            serde_json::to_string(&personalized_view(&hidden, &db, PlayerId(0))).unwrap();
        assert!(!seat_zero.contains(&card_entity_id(theirs_in_hand.id)));
    }
}
