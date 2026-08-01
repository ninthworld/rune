//! Stack-object projection into wire [`StackItem`]s.
//!
//! The stack is where the protocol's "what is happening right now" lives: an
//! object's kind, the targets it named on announcement, and the face to render for
//! it (issue #550). Split out of [`super::cards`] so the stack contract and its
//! tests sit together.

use super::*;
use sage_engine::AbilitySource;

/// Project one engine [`StackObject`] onto its wire [`StackItem`].
///
/// An ability's description is composed by the same formatter that writes a card's
/// rules text ([`crate::rules_text::effects_description`]), so the stack and the card
/// never describe one effect two different ways.
///
/// Beyond that prose (issue #550) the entry carries the structure a client needs to
/// *draw* the object without interpreting the sentence: its [`StackItemKind`], its
/// ordered [`StackTarget`] list, and the card face to render. `description` stays
/// authoritative for text — the targets are additive geometry, never a substitute a
/// client must parse prose to recover (ADR 0001).
pub(crate) fn stack_item(state: &GameState, object: &StackObject, db: &CardDatabase) -> StackItem {
    // The targets recorded on announcement (CR 601.2c), projected verbatim and in
    // order: the engine never rewrites the list, so an entry keeps naming a target
    // that has since become illegal until it resolves or fizzles (CR 608.2b). That
    // is what makes a reconnect mid-resolution rebuild the same relationships.
    let targets = object.targets.iter().map(stack_target).collect();
    match &object.kind {
        StackObjectKind::Spell { card } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: card_name(card.card, db),
            source: None,
            // The physical card being cast (CR 108.1, issue #650). The engine carries
            // the whole `CardInstance` across the stack precisely so this survives, and
            // it is the same id the card had in hand and will have on the battlefield or
            // in a graveyard — while the `stack_` id, like every per-zone id, is this
            // object's alone (CR 400.7).
            physical_card: Some(card_entity_id(card.id)),
            kind: Some(StackItemKind::Spell),
            targets,
            // The face of the card being cast, keyed by the physical instance so it
            // is the same entity id the card carried in hand and will carry on the
            // battlefield.
            card: Some(card_view(card_entity_id(card.id), card.card, db)),
        },
        StackObjectKind::Ability {
            source,
            origin,
            effects,
        } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: effects_description(&source_name(state, *source, db), effects),
            source: source.permanent().map(permanent_entity_id),
            // An ability on the stack (CR 113.3) is an object with no card behind it, so
            // there is no physical card to name (issue #650). `source` names the
            // permanent it came from, which is a different question — and the card face
            // below is that permanent's, keyed by its `perm_` id, which is exactly why
            // this question needs its own field rather than a client reading `card.id`.
            physical_card: None,
            // The engine records which push site put this here (issue #579), so the
            // projection states the finer kind rather than the coarse `ability` it
            // was limited to under #550. Still proof, not inference: the value comes
            // from the engine's own record of the push, never from the description
            // or from when the object appeared.
            kind: Some(ability_kind(*origin)),
            targets,
            // An ability's face is its *source permanent's* current face, so the
            // entry can show a source thumbnail without a battlefield lookup. It
            // degrades exactly as `source_name` does: a source that has already left
            // the battlefield (CR 608.2) has no face left to give.
            card: source_permanent(state, *source).map(|perm| permanent_card_view(state, perm, db)),
        },
    }
}

/// The wire kind for an ability, from the provenance the engine recorded at the push
/// site (issue #579, gap G2).
///
/// Exhaustive by construction, like [`stack_target`]: a new engine origin forces a
/// matching wire value here rather than silently collapsing to the coarse
/// [`StackItemKind::Ability`], which stays reserved for a server that genuinely
/// cannot prove which it was.
fn ability_kind(origin: AbilityOrigin) -> StackItemKind {
    match origin {
        AbilityOrigin::Activated => StackItemKind::Activated,
        AbilityOrigin::Triggered => StackItemKind::Triggered,
    }
}

/// Project one engine [`Target`] onto its wire [`StackTarget`], typed at the source.
///
/// Exhaustive by construction: a new engine target variant forces a matching wire
/// variant here rather than silently degrading to an untyped id the client would
/// have to classify itself (issue #550, gap G6). The player arm projects to a
/// [`sage_protocol::PlayerId`] — the seat key `controller`, `seat_order`, and
/// `player_names` share — while every other arm projects to an entity id.
fn stack_target(target: &Target) -> StackTarget {
    match *target {
        Target::Player(seat) => StackTarget::Player {
            player: player_id(seat),
        },
        Target::Permanent(id) => StackTarget::Permanent {
            id: permanent_entity_id(id),
        },
        Target::Card(id) => StackTarget::Card {
            id: card_entity_id(id),
        },
        Target::Spell(id) => StackTarget::Stack {
            id: stack_entity_id(id),
        },
    }
}

/// The permanent an ability on the stack came from, while it is still on the
/// battlefield. `None` once it has left — its ability outlives it there (CR 608.2).
fn source_permanent(state: &GameState, source: AbilitySource) -> Option<&sage_engine::Permanent> {
    let id = source.permanent()?;
    state.battlefield.iter().find(|perm| perm.id == id)
}

/// The name of the permanent an ability on the stack came from — what its sentences
/// call themselves. A permanent that has already left the battlefield (its ability
/// outlives it on the stack, CR 608.2) has no name left to give.
fn source_name(state: &GameState, source: AbilitySource, db: &CardDatabase) -> String {
    source_permanent(state, source).map_or_else(
        || match source {
            // An emblem (CR 114) has no name of its own — it has only its abilities —
            // so its sentences say what it is rather than inventing a title for it.
            AbilitySource::Emblem(_) => "An emblem".to_string(),
            AbilitySource::Permanent(_) => "This ability's source".to_string(),
        },
        |perm| permanent_name(perm, db),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::{fixture, id_in};
    use crate::view::test_support::put_permanent;
    use sage_engine::{DamageSubject, Effect, TargetSpec};

    /// A two-target damage spell, which no bundled M19 card provides (ADR 0009:
    /// exercise an unrepresented shape from an inline catalog rather than bending a
    /// real card). Its two `deal_damage` effects consume two targets in order.
    fn twin_bolt_db() -> CardDatabase {
        let json = r#"[
            {"schema_version":1,"functional_id":"test_twin_bolt","name":"Test Twin Bolt",
             "types":["instant"],"mana_cost":"{1}{R}","colors":["red"],
             "spell_effects":[
               {"kind":"deal_damage","target":"any_target","amount":1},
               {"kind":"deal_damage","target":"any_target","amount":1}]},
            {"schema_version":1,"functional_id":"test_bear","name":"Test Bear",
             "types":["creature"],"subtypes":["Bear"],"mana_cost":"{1}{G}","colors":["green"],
             "power":2,"toughness":2}
        ]"#;
        CardDatabase::from_json(json).unwrap()
    }

    /// Push one object onto the stack of `state`, minting its stack id.
    fn push(
        state: &mut GameState,
        controller: PlayerId,
        kind: StackObjectKind,
        targets: Vec<Target>,
    ) -> StackId {
        let id = StackId(state.mint_id());
        state.stack.push(StackObject {
            id,
            controller,
            kind,
            targets,
        });
        id
    }

    /// A spell on the stack projects its kind, the face of the card being cast, and
    /// the target it named — and an ability projects its kind, its source, and the
    /// **current** face of that source permanent (issue #550, gaps G1/G4).
    #[test]
    fn issue_550_a_spell_and_an_ability_project_kind_targets_and_a_card_face() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // A creature to aim at, and a Llanowar Elves whose ability shares the stack
        // with a Shock aimed at that creature.
        let ogre = put_permanent(
            &mut state,
            fixture("onakke_ogre"),
            PlayerId(1),
            false,
            false,
        );
        let elves = put_permanent(
            &mut state,
            fixture("llanowar_elves"),
            PlayerId(0),
            true,
            false,
        );

        let shock = state.new_instance(fixture("shock"));
        let spell = push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Spell { card: shock },
            vec![Target::Permanent(ogre)],
        );
        let ability = push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Ability {
                source: sage_engine::AbilitySource::Permanent(elves),
                origin: AbilityOrigin::Activated,
                effects: vec![Effect::DrawCard { count: 1 }],
            },
            Vec::new(),
        );

        let view = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(view.stack.len(), 2);

        let spell_view = &view.stack[0];
        assert_eq!(spell_view.id, stack_entity_id(spell));
        assert_eq!(spell_view.kind, Some(StackItemKind::Spell));
        assert_eq!(spell_view.source, None);
        assert_eq!(
            spell_view.card.as_ref().map(|c| c.name.as_str()),
            Some("Shock"),
            "a spell renders the face of the card being cast",
        );
        assert_eq!(
            spell_view.card.as_ref().map(|c| c.id.as_str()),
            Some(card_entity_id(shock.id).as_str()),
            "keyed by the physical instance — the id it already carried in hand",
        );
        assert_eq!(
            spell_view.targets,
            vec![StackTarget::Permanent {
                id: permanent_entity_id(ogre)
            }],
        );

        let ability_view = &view.stack[1];
        assert_eq!(ability_view.id, stack_entity_id(ability));
        assert_eq!(
            ability_view.kind,
            Some(StackItemKind::Activated),
            "an activation states the finer kind (issue #579)",
        );
        assert_eq!(
            ability_view.source.as_deref(),
            Some(permanent_entity_id(elves).as_str())
        );
        assert_eq!(
            ability_view.card.as_ref().map(|c| c.name.as_str()),
            Some("Llanowar Elves"),
            "an ability renders its source permanent's face as the thumbnail",
        );
        assert!(
            ability_view.targets.is_empty(),
            "a targetless entry is not an error — the list is simply empty",
        );
        // An empty target list elides from the wire entirely.
        let json = serde_json::to_value(ability_view).unwrap();
        assert!(json.get("targets").is_none());

        // The spectator sees exactly the same public stack: an object on
        // the stack is public, so nothing is redacted and nothing extra is added.
        let spectated = spectator_view(&state, &db);
        assert_eq!(spectated.stack, view.stack);
    }

    /// A multi-target spell projects **every** target, typed and in the order its
    /// effects consume them — the ordering channel the client's ①②③ numerals come
    /// from (issue #550, gap G1). A stack object may itself be a target (CR 701.5),
    /// and a player target carries the seat id rather than an entity id (gap G6).
    #[test]
    fn issue_550_a_multi_target_spell_projects_its_targets_typed_and_in_order() {
        let db = twin_bolt_db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        let bear = put_permanent(
            &mut state,
            id_in(&db, "test_bear"),
            PlayerId(1),
            false,
            false,
        );
        let twin = state.new_instance(id_in(&db, "test_twin_bolt"));
        let bolt = push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Spell { card: twin },
            // One damage to the creature, one to its controller.
            vec![Target::Permanent(bear), Target::Player(PlayerId(1))],
        );
        // ...and a spell aimed at that spell.
        let counter = state.new_instance(id_in(&db, "test_twin_bolt"));
        push(
            &mut state,
            PlayerId(1),
            StackObjectKind::Spell { card: counter },
            vec![Target::Spell(bolt)],
        );

        let view = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(
            view.stack[0].targets,
            vec![
                StackTarget::Permanent {
                    id: permanent_entity_id(bear)
                },
                StackTarget::Player {
                    player: player_id(PlayerId(1))
                },
            ],
            "both targets, typed at the source and in announcement order",
        );
        assert_eq!(
            view.stack[1].targets,
            vec![StackTarget::Stack {
                id: stack_entity_id(bolt)
            }],
        );
    }

    /// An ability whose source has left the battlefield (CR 608.2) keeps resolving,
    /// but has no face left to give: the card view degrades to `None` exactly where
    /// the description degrades to "This ability's source" (issue #550, conflict C5).
    #[test]
    fn issue_550_an_ability_whose_source_left_the_battlefield_carries_no_face() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // A source id that is not on the battlefield — the ability outlived it. Its
        // one effect names its source, so the description shows the same degradation.
        let ghost = PermanentId(state.mint_id());
        push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Ability {
                source: sage_engine::AbilitySource::Permanent(ghost),
                origin: AbilityOrigin::Triggered,
                effects: vec![Effect::DealDamage {
                    subject: DamageSubject::Target(TargetSpec::AnyTarget),
                    amount: 1,
                }],
            },
            vec![Target::Player(PlayerId(1))],
        );

        let view = personalized_view(&state, &db, PlayerId(0));
        let entry = &view.stack[0];
        assert_eq!(
            entry.kind,
            Some(StackItemKind::Triggered),
            "provenance survives the source leaving play — it is recorded on the push",
        );
        assert_eq!(entry.card, None, "no source permanent, no face");
        assert_eq!(
            entry.source.as_deref(),
            Some(permanent_entity_id(ghost).as_str()),
            "the reference is still stated; only the face is missing",
        );
        assert!(
            entry.description.contains("This ability's source"),
            "the description degrades the same way: {}",
            entry.description,
        );
        let json = serde_json::to_value(entry).unwrap();
        assert!(
            json.get("card").is_none(),
            "an absent face elides from the wire"
        );
    }

    /// A client that reconnects mid-resolution rebuilds the stack **and its
    /// relationship paths** from the one re-sent view: the projection is a pure
    /// function of state, so a re-send after a target has become illegal still names
    /// that target (CR 608.2b keeps it on the object until it resolves or fizzles).
    #[test]
    fn issue_550_a_reconnect_mid_resolution_rebuilds_the_stack_and_its_paths() {
        let db = twin_bolt_db();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        let bear = put_permanent(
            &mut state,
            id_in(&db, "test_bear"),
            PlayerId(1),
            false,
            false,
        );
        let twin = state.new_instance(id_in(&db, "test_twin_bolt"));
        push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Spell { card: twin },
            vec![Target::Permanent(bear), Target::Player(PlayerId(1))],
        );

        // The frame a reconnecting client receives is the same projection, serialized
        // and parsed back — nothing about the stack is carried across messages.
        let first = personalized_view(&state, &db, PlayerId(0));
        let resent = personalized_view(&state, &db, PlayerId(0));
        let wire = serde_json::to_string(&resent).unwrap();
        let rebuilt: sage_protocol::GameView = serde_json::from_str(&wire).unwrap();
        assert_eq!(
            rebuilt.stack, first.stack,
            "a resync rebuilds the stack verbatim"
        );

        // Now the creature leaves the battlefield: the chosen target is illegal, but
        // the object still names it, so the reconnecting client draws the same two
        // paths (and the renderer, not the server, decides an unresolvable endpoint).
        state.battlefield.retain(|perm| perm.id != bear);
        let after = personalized_view(&state, &db, PlayerId(0));
        let wire = serde_json::to_string(&after).unwrap();
        let rebuilt: sage_protocol::GameView = serde_json::from_str(&wire).unwrap();
        assert_eq!(rebuilt.stack[0].targets.len(), 2);
        assert_eq!(
            rebuilt.stack[0].targets[0],
            StackTarget::Permanent {
                id: permanent_entity_id(bear)
            },
            "an illegal target stays named until the object resolves (CR 608.2b)",
        );
        assert_eq!(rebuilt.stack, after.stack);

        // A spectator reconnecting mid-resolution rebuilds the identical stack.
        let spectated = spectator_view(&state, &db);
        let rebuilt_spectator: sage_protocol::SpectatorView =
            serde_json::from_str(&serde_json::to_string(&spectated).unwrap()).unwrap();
        assert_eq!(rebuilt_spectator.stack, after.stack);
    }

    /// An activated and a triggered ability sharing the stack project **different**
    /// kinds, and the distinction is the same in every view an audience can hold —
    /// the controller's, the opponent's, and a spectator's (issue #579, gap G2).
    ///
    /// Nothing else on the two entries separates them: same source permanent, same
    /// effect, therefore the same composed `description` and the same face. That is
    /// the point — only the engine's recorded provenance can tell them apart, which
    /// is why a client inferring it from prose would be guessing.
    #[test]
    fn issue_579_an_activation_and_a_trigger_project_distinct_kinds_in_every_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        let elves = put_permanent(
            &mut state,
            fixture("llanowar_elves"),
            PlayerId(0),
            false,
            false,
        );
        let effects = vec![Effect::DrawCard { count: 1 }];
        let activated = push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Ability {
                source: sage_engine::AbilitySource::Permanent(elves),
                origin: AbilityOrigin::Activated,
                effects: effects.clone(),
            },
            Vec::new(),
        );
        let triggered = push(
            &mut state,
            PlayerId(0),
            StackObjectKind::Ability {
                source: sage_engine::AbilitySource::Permanent(elves),
                origin: AbilityOrigin::Triggered,
                effects,
            },
            Vec::new(),
        );

        let mine = personalized_view(&state, &db, PlayerId(0));
        assert_eq!(mine.stack[0].id, stack_entity_id(activated));
        assert_eq!(mine.stack[0].kind, Some(StackItemKind::Activated));
        assert_eq!(mine.stack[1].id, stack_entity_id(triggered));
        assert_eq!(mine.stack[1].kind, Some(StackItemKind::Triggered));
        assert_eq!(
            mine.stack[0].description, mine.stack[1].description,
            "the prose is identical — the kind is the only channel that separates them",
        );

        // The stack is public, so the opponent's and a spectator's copies
        // are the same entries, finer kind included. Nothing here is personalized.
        let theirs = personalized_view(&state, &db, PlayerId(1));
        assert_eq!(theirs.stack, mine.stack);
        assert_eq!(spectator_view(&state, &db).stack, mine.stack);

        // Cross-language shape: the finer kinds are the documented snake_case values
        // the TypeScript mirror validates against, and they survive a round trip.
        let json = serde_json::to_value(&mine.stack).unwrap();
        assert_eq!(json[0]["kind"], "activated");
        assert_eq!(json[1]["kind"], "triggered");
        let rebuilt: Vec<StackItem> = serde_json::from_value(json).unwrap();
        assert_eq!(rebuilt, mine.stack);
    }

    /// The coarse `ability` value keeps deserializing: a payload from a server that
    /// predates issue #579 states only that an ability is on the stack, and that must
    /// remain a legal frame rather than becoming a parse error (issue #579).
    #[test]
    fn issue_579_the_coarse_ability_kind_still_deserializes() {
        let legacy = r#"{"id":"s1","controller":"p1","description":"Add {G}.",
                         "source":"perm_1","kind":"ability"}"#;
        let item: StackItem = serde_json::from_str(legacy).unwrap();
        assert_eq!(item.kind, Some(StackItemKind::Ability));

        // …as does a payload predating #550, which states no kind at all. Absent is
        // unclassified, never a guess.
        let older = r#"{"id":"s1","controller":"p1","description":"Add {G}."}"#;
        let item: StackItem = serde_json::from_str(older).unwrap();
        assert_eq!(item.kind, None);
    }
}
