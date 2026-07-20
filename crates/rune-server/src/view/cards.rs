//! Card, permanent, and zone projection into wire views.

use super::*;

/// Projects a permanent's stored engine counters into the wire [`Counter`] list.
///
/// Ordering follows the permanent's `BTreeMap<CounterKind, _>` iteration, which
/// is sorted by [`CounterKind`] and therefore stable across runs. Absent kinds
/// are simply not emitted, so a permanent with no counters yields an empty
/// `Vec` (the `skip_serializing_if` wire shape stays unchanged).
pub(crate) fn permanent_counters(perm: &rune_engine::Permanent) -> Vec<Counter> {
    perm.counters
        .iter()
        .map(|(&kind, &count)| Counter {
            kind: counter_kind_str(kind).to_owned(),
            count,
        })
        .collect()
}

/// Map the engine's turn [`Step`] onto the protocol [`Phase`]. The two enums are
/// deliberately decoupled (`rune-engine` never depends on `rune-protocol`), so the
/// mapping is written out here.
pub(crate) fn phase_of(step: Step) -> Phase {
    match step {
        Step::Untap => Phase::Untap,
        Step::Upkeep => Phase::Upkeep,
        Step::Draw => Phase::Draw,
        Step::PrecombatMain => Phase::PrecombatMain,
        Step::BeginCombat => Phase::BeginCombat,
        Step::DeclareAttackers => Phase::DeclareAttackers,
        Step::DeclareBlockers => Phase::DeclareBlockers,
        Step::CombatDamage => Phase::CombatDamage,
        Step::EndCombat => Phase::EndCombat,
        Step::PostcombatMain => Phase::PostcombatMain,
        Step::End => Phase::End,
        Step::Cleanup => Phase::Cleanup,
    }
}

/// The display name of a card, or a stable placeholder if the id is unknown.
pub(crate) fn card_name(card: CardId, db: &CardDatabase) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("Unknown card {}", card.0))
}

/// Build the full [`CardView`] for a card the viewer is entitled to see.
pub(crate) fn card_view(entity_id: String, card: CardId, db: &CardDatabase) -> CardView {
    match db.card(card) {
        Some(data) => full_card_view(entity_id, data),
        None => CardView {
            id: entity_id,
            name: format!("Unknown card {}", card.0),
            type_line: String::new(),
            mana_cost: None,
            rules_text: String::new(),
            functional_id: String::new(),
            power: None,
            toughness: None,
            keywords: Vec::new(),
        },
    }
}

/// The wire name for an engine [`Keyword`], as the client expects it in
/// [`CardView::keywords`] (e.g. `"flying"`, `"first_strike"`). Kept exhaustive so
/// a new engine keyword forces a matching wire string here rather than silently
/// going unnamed.
fn keyword_str(keyword: Keyword) -> &'static str {
    match keyword {
        Keyword::Flying => "flying",
        Keyword::Reach => "reach",
        Keyword::Vigilance => "vigilance",
        Keyword::Haste => "haste",
        Keyword::FirstStrike => "first_strike",
        Keyword::Trample => "trample",
        Keyword::Deathtouch => "deathtouch",
        Keyword::Lifelink => "lifelink",
        Keyword::DoubleStrike => "double_strike",
    }
}

/// Project engine [`CardData`] onto the wire [`CardView`]. Power/toughness become
/// strings so non-numeric values round-trip (`rune-protocol`); an empty mana cost
/// is elided rather than sent as `""`; printed keywords project to their lowercase
/// wire names for display.
///
/// The card's rules text is **generated** here from its ability IR
/// ([`crate::rules_text`], ADR 0018 §7) rather than read from a stored string — the
/// catalog holds no prose — and its authored `functional_id` rides along as the stable
/// presentation identity (ADR 0018 §8). A scripted card's hand-authored text comes from
/// the engine's escape hatch — keyed, like the catalog itself, on the card's authored
/// `functional_id` rather than its build-interned handle (ADR 0018 §3), and guaranteed
/// by the loader to exist whenever the definition declares `scripted: true`.
pub(crate) fn full_card_view(entity_id: String, data: &CardData) -> CardView {
    CardView {
        id: entity_id,
        name: data.name.clone(),
        type_line: data.type_line(),
        mana_cost: (!data.mana_cost.is_empty()).then(|| data.mana_cost.clone()),
        rules_text: rules_text(data, scripted_rules_text(&data.functional_id)),
        functional_id: data.functional_id.to_string(),
        power: data.power.map(|p| p.to_string()),
        toughness: data.toughness.map(|t| t.to_string()),
        keywords: data
            .keywords
            .iter()
            .map(|&kw| keyword_str(kw).to_owned())
            .collect(),
    }
}

/// Build the [`CardView`] for a battlefield permanent, projecting its **current**
/// power/toughness (CR 613 layer 7c) and keywords (CR 613.1f, layer 6) from the
/// engine's computed [`characteristics`] rather than the printed card. This is what
/// makes counters, until-end-of-turn pumps, and an attached Aura's P/T grant
/// (CR 303.4) visible on the wire — a Boar enchanted with a `+2/+2` Aura projects as
/// a 5/4 — and, equally, what makes a granted keyword show up like a printed one: a
/// creature enchanted with an Aura granting flying projects with `flying`. Every
/// other field is the printed projection ([`card_view`]); a non-creature keeps its
/// absent P/T.
pub(crate) fn permanent_card_view(
    state: &GameState,
    perm: &rune_engine::Permanent,
    db: &CardDatabase,
) -> CardView {
    let mut view = card_view(permanent_entity_id(perm.id), perm.card, db);
    let current = characteristics(state, perm.id, db);
    view.power = current.power.map(|p| p.to_string());
    view.toughness = current.toughness.map(|t| t.to_string());
    // CR 613 layer 6 (CR 613.1f): project the *current* keywords, so a keyword
    // granted by an Aura, an anthem, or an until-end-of-turn pump appears on the wire
    // exactly like a printed one.
    view.keywords = current
        .keywords
        .iter()
        .map(|&kw| keyword_str(kw).to_owned())
        .collect();
    view
}

/// Project one engine [`StackObject`] onto its wire [`StackItem`].
///
/// An ability's description is composed by the same formatter that writes a card's
/// rules text ([`crate::rules_text::effects_description`]), so the stack and the card
/// never describe one effect two different ways.
pub(crate) fn stack_item(state: &GameState, object: &StackObject, db: &CardDatabase) -> StackItem {
    match &object.kind {
        StackObjectKind::Spell { card } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: card_name(card.card, db),
            source: None,
        },
        StackObjectKind::Ability { source, effects } => StackItem {
            id: stack_entity_id(object.id),
            controller: player_id(object.controller),
            description: effects_description(&source_name(state, *source, db), effects),
            source: Some(permanent_entity_id(*source)),
        },
    }
}

/// The name of the permanent an ability on the stack came from — what its sentences
/// call themselves. A permanent that has already left the battlefield (its ability
/// outlives it on the stack, CR 608.2) has no name left to give.
fn source_name(state: &GameState, source: PermanentId, db: &CardDatabase) -> String {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == source)
        .map_or_else(
            || "This ability's source".to_string(),
            |perm| card_name(perm.card, db),
        )
}

/// Build the [`ZonePile`]s for a public per-player pile (graveyard or exile),
/// skipping empty piles so the wire stays terse.
pub(crate) fn zone_piles(
    state: &GameState,
    pick: impl Fn(&Player) -> &Vec<CardInstance>,
    db: &CardDatabase,
) -> Vec<ZonePile> {
    state
        .players
        .iter()
        .enumerate()
        .filter_map(|(seat, player)| {
            let cards = pick(player);
            if cards.is_empty() {
                return None;
            }
            Some(ZonePile {
                player_id: player_id(PlayerId(seat)),
                cards: cards
                    .iter()
                    .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                    .collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::{fixture, id_in};
    use crate::view::test_support::put_permanent;

    /// A battlefield permanent enchanted with an Aura projects its **current**
    /// (computed) power/toughness on the wire, so the host's P/T reflects the Aura's
    /// layer-7c grant (CR 303.4 / 613.7c, issue #152) rather than the printed value.
    #[test]
    fn issue_152_aura_boosted_host_projects_current_pt() {
        // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0026):
        // a 1/1 host enchanted with a +2/+2 Aura.
        let json = r#"[
            {"schema_version":1,"functional_id":"test_scout","name":"Test Scout",
             "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
             "power":1,"toughness":1},
            {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
             "aura":{"enchant":"any_creature","power":2,"toughness":2}}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();

        let host = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            card: id_in(&db, "test_scout"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        let aura = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: aura,
            instance: CardInstanceId(1),
            card: id_in(&db, "test_aegis"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: Some(host),
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let host_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(host))
            .expect("the enchanted host must appear in the view");
        assert_eq!(
            host_view.card.power.as_deref(),
            Some("3"),
            "printed 1 + Aura's +2 projects as current power 3"
        );
        assert_eq!(host_view.card.toughness.as_deref(), Some("3"));
    }

    /// A battlefield permanent projects its stored engine counters into
    /// [`PermanentView::counters`] as `{ kind, count }` wire entries, in a
    /// deterministic order (sorted by [`CounterKind`], the map's key order), and
    /// a permanent with no counters projects to an empty list — which
    /// `skip_serializing_if` then drops from the JSON entirely (issue #68).
    #[test]
    fn issue_68_permanent_counters_project_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        // Seat 0 holds priority so the state is a valid, viewable snapshot.
        let with_counters = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: with_counters,
            instance: CardInstanceId(0),
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            // Insertion order is deliberately reversed from the expected wire
            // order to prove the projection sorts by kind, not by insertion.
            counters: [
                (CounterKind::MinusOneMinusOne, 1),
                (CounterKind::PlusOnePlusOne, 2),
            ]
            .into_iter()
            .collect(),
            attached_to: None,
        });
        let without_counters = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: without_counters,
            instance: CardInstanceId(1),
            card: fixture("forest"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });

        let view = personalized_view(&state, &db, PlayerId(0));

        let counted = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(with_counters))
            .expect("permanent with counters must appear in the view");
        assert_eq!(
            counted.counters,
            vec![
                Counter {
                    kind: "+1/+1".into(),
                    count: 2,
                },
                Counter {
                    kind: "-1/-1".into(),
                    count: 1,
                },
            ],
            "counters must be sorted by kind (+1/+1 before -1/-1), not by insertion order",
        );

        let bare = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(without_counters))
            .expect("permanent without counters must appear in the view");
        assert!(
            bare.counters.is_empty(),
            "a permanent with no counters projects to an empty list",
        );

        // The empty list is dropped from the wire via `skip_serializing_if`, so
        // the serialized shape is unchanged from the always-empty placeholder.
        let json = serde_json::to_value(bare).unwrap();
        assert!(
            json.get("counters").is_none(),
            "empty counters must not be serialized (skip_serializing_if wire shape)",
        );
        let counted_json = serde_json::to_value(counted).unwrap();
        assert!(
            counted_json.get("counters").is_some(),
            "non-empty counters must be serialized",
        );
    }

    /// Combat declaration state is visible in the projected view (issue #117): an
    /// attacking permanent reports `attacking: true`, and a blocker reports the
    /// entity id of the attacker it is blocking. A permanent not in combat reports
    /// neither.
    #[test]
    fn issue_117_attack_and_block_state_project_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let attacker = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: attacker,
            instance: CardInstanceId(0),
            card: fixture("walking_corpse"),
            controller: PlayerId(0),
            tapped: true,
            entered_turn: 0,
            attacking: Some(PlayerId(1)),
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        let blocker = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: blocker,
            instance: CardInstanceId(1),
            card: fixture("walking_corpse"),
            controller: PlayerId(1),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: Some(attacker),
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let attacker_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(attacker))
            .expect("attacker in view");
        assert!(attacker_view.attacking);
        assert_eq!(attacker_view.blocking, None);

        let blocker_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(blocker))
            .expect("blocker in view");
        assert!(!blocker_view.attacking);
        assert_eq!(
            blocker_view.blocking.as_deref(),
            Some(permanent_entity_id(attacker).as_str())
        );
    }

    /// Marked combat damage (issue #118) projects onto [`PermanentView::damage`]:
    /// a damaged permanent reports its marked damage, and an undamaged one reports
    /// `0`, which `skip_serializing_if` then drops from the wire.
    #[test]
    fn issue_118_marked_damage_projects_into_the_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let damaged = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: damaged,
            instance: CardInstanceId(0),
            card: fixture("onakke_ogre"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 2,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let projected = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(damaged))
            .expect("damaged permanent in view");
        assert_eq!(projected.damage, 2);

        // Zero marked damage elides from the JSON (skip_serializing_if wire shape).
        let mut undamaged = projected.clone();
        undamaged.damage = 0;
        let json = serde_json::to_value(&undamaged).unwrap();
        assert!(json.get("damage").is_none());
    }

    /// Aura attachment (issue #333) projects onto [`PermanentView::attached_to`]: an
    /// Aura resolved onto the battlefield through the real engine path reports the
    /// entity id of the host it enchants, while its host (and any unattached
    /// permanent) reports no attachment and elides the field from the wire.
    #[test]
    fn issue_333_aura_attachment_projects_into_the_view() {
        use std::collections::BTreeMap;

        // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0026).
        let json = r#"[
            {"schema_version":1,"functional_id":"test_scout","name":"Test Scout",
             "types":["creature"],"subtypes":["Elf"],"mana_cost":"{G}","colors":["green"],
             "power":1,"toughness":1},
            {"schema_version":1,"functional_id":"test_aegis","name":"Test Aegis",
             "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
             "aura":{"enchant":"any_creature","power":2,"toughness":2}}
        ]"#;
        let db = CardDatabase::from_json(json).unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // A host creature already on the battlefield.
        let host = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            card: id_in(&db, "test_scout"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: BTreeMap::new(),
            attached_to: None,
        });

        // The Aura spell resolves off the stack attached to the host (CR 303.4d),
        // exactly as the engine's aura-resolution path produces it — no shortcut of
        // hand-populating `attached_to`.
        let aura = state.new_instance(id_in(&db, "test_aegis"));
        let sid = state.mint_id();
        state.stack.push(StackObject {
            id: StackId(sid),
            controller: PlayerId(0),
            kind: StackObjectKind::Spell { card: aura },
            targets: vec![Target::Permanent(host)],
        });
        let state = rune_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = rune_engine::apply_action(&state, &Action::PassPriority, &db);

        let view = personalized_view(&state, &db, PlayerId(0));

        // The Aura's view entry names its host as an entity id.
        let aura_view = view
            .battlefield
            .iter()
            .find(|p| p.attached_to.is_some())
            .expect("the resolved Aura must appear in the view, attached");
        assert_eq!(
            aura_view.attached_to.as_deref(),
            Some(permanent_entity_id(host).as_str()),
            "the Aura names the host it enchants (CR 303.4)",
        );

        // The host itself carries no attachment, and the empty field elides.
        let host_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(host))
            .expect("host in view");
        assert_eq!(host_view.attached_to, None);
        let json = serde_json::to_value(host_view).unwrap();
        assert!(json.get("attached_to").is_none());
    }

    /// A permanent's printed keywords (issue #153) project onto its card view as
    /// lowercase wire names for the client to render, and a keyword-less card omits
    /// the field. Snapping Drake has flying; Onakke Ogre has none.
    #[test]
    fn issue_153_keywords_project_onto_the_card_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let flyer = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: flyer,
            instance: CardInstanceId(0),
            card: fixture("snapping_drake"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        let vanilla = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: vanilla,
            instance: CardInstanceId(1),
            card: fixture("onakke_ogre"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let flyer_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(flyer))
            .expect("flyer in view");
        assert_eq!(flyer_view.card.keywords, vec!["flying".to_string()]);

        let vanilla_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(vanilla))
            .expect("vanilla in view");
        assert!(vanilla_view.card.keywords.is_empty());
        // The empty list elides from the JSON (skip_serializing_if wire shape).
        let json = serde_json::to_value(&vanilla_view.card).unwrap();
        assert!(json.get("keywords").is_none());
    }

    /// A keyword granted by continuous effect (issue #374) projects onto the
    /// permanent's card view exactly like a printed one: an Onakke Ogre (no printed
    /// keyword) enchanted with Flight (an Aura granting flying) shows `flying` on the
    /// wire, and a second, unenchanted Ogre shows none.
    #[test]
    fn issue_374_granted_keyword_projects_onto_the_card_view() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let host = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            card: fixture("onakke_ogre"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        let bystander = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: bystander,
            instance: CardInstanceId(1),
            card: fixture("onakke_ogre"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        // Flight, an Aura granting flying, attached to the host.
        let aura = PermanentId(state.mint_id());
        state.battlefield.push(rune_engine::Permanent {
            id: aura,
            instance: CardInstanceId(2),
            card: fixture("flight"),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: Some(host),
        });

        let view = personalized_view(&state, &db, PlayerId(0));
        let host_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(host))
            .expect("host in view");
        assert_eq!(host_view.card.keywords, vec!["flying".to_string()]);

        let bystander_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(bystander))
            .expect("bystander in view");
        assert!(bystander_view.card.keywords.is_empty());
    }

    /// The ability-target `requirements` projection (ADR 0009 deferral #73, folded
    /// into issue #140): a `{T}: Tap target creature` activation advertises its one
    /// target slot with the legal creature candidates, and a returned target
    /// resolves to an `ActivateAbility` carrying exactly that chosen target.
    #[test]
    fn issue_194_cards_project_generated_rules_text_and_their_stable_identity() {
        // ADR 0018 §7-§8: the catalog stores no prose, so what the player reads is
        // composed from the card's IR at projection time — and rides the same view as
        // the card's authored identity, which a future client-local cache could key on.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Skyscanner (a flying ETB-draw creature) in hand, a Forest on the battlefield.
        let scout = state.new_instance(fixture("skyscanner"));
        state.players[0].hand = vec![scout];
        let forest = put_permanent(&mut state, fixture("forest"), PlayerId(0), false, false);

        let view = personalized_view(&state, &db, PlayerId(0));

        let scout_view = view
            .my_hand
            .iter()
            .find(|c| c.name == "Skyscanner")
            .expect("the skyscanner is in hand");
        assert_eq!(
            scout_view.rules_text, "Flying\nWhen Skyscanner enters the battlefield, draw a card.",
            "the keyword and trigger words are generated from its IR, not stored"
        );
        assert_eq!(scout_view.functional_id, "skyscanner");

        let forest_view = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(forest))
            .map(|p| &p.card)
            .expect("the forest is on the battlefield");
        assert_eq!(forest_view.rules_text, "{T}: Add {G}.");
        assert_eq!(forest_view.functional_id, "forest");

        // A vanilla card claims no rules — and the field is omitted from the wire
        // rather than sent as an empty string.
        let boar = full_card_view("c9".to_string(), db.card(fixture("onakke_ogre")).unwrap());
        assert_eq!(boar.rules_text, "");
        let json = serde_json::to_string(&boar).expect("a card view serializes");
        assert!(!json.contains("rules_text"), "{json}");
        assert!(json.contains(r#""functional_id":"onakke_ogre""#), "{json}");
    }

    #[test]
    fn issue_194_an_unresolvable_card_projects_no_text_and_no_identity() {
        // The defensive placeholder: an id the catalog does not hold has nothing to
        // generate from and no authored identity to claim — it must not invent either.
        let db = CardDatabase::bundled().unwrap();
        let view = card_view("c1".to_string(), CardId(9999), &db);
        assert_eq!(view.name, "Unknown card 9999");
        assert_eq!(view.rules_text, "");
        assert_eq!(view.functional_id, "");
    }
}
