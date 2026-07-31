//! Card, permanent, and zone projection into wire views.

use super::*;

/// Whether this battlefield object **is** somebody's commander (CR 903.3, issue
/// #553) — the marker `Permanent::is_commander` carries.
///
/// Matched on the card **instance**, which is the engine's designation key
/// ([`CommanderState::instance`](sage_engine::CommanderState)): a
/// [`PermanentId`] is minted fresh on every battlefield entry, so a recast
/// commander is a new object but the same instance. Every seat's designation is
/// checked rather than only the controller's, because a commander that has changed
/// control is still its owner's commander.
///
/// This is a *lookup*, not a derivation: nothing about a name, a zone, or a type
/// line participates, which is exactly why a client cannot compute it.
pub(crate) fn is_commander_permanent(state: &GameState, perm: &sage_engine::Permanent) -> bool {
    state.players.iter().any(|player| {
        player
            .commander
            .is_some_and(|c| c.instance == perm.instance)
    })
}

/// Projects a permanent's stored engine counters into the wire [`Counter`] list.
///
/// Ordering follows the permanent's `BTreeMap<CounterKind, _>` iteration, which
/// is sorted by [`CounterKind`] and therefore stable across runs. Absent kinds
/// are simply not emitted, so a permanent with no counters yields an empty
/// `Vec` (the `skip_serializing_if` wire shape stays unchanged).
pub(crate) fn permanent_counters(perm: &sage_engine::Permanent) -> Vec<Counter> {
    perm.counters
        .iter()
        .map(|(&kind, &count)| Counter {
            kind: counter_kind_str(kind).to_owned(),
            count,
        })
        .collect()
}

/// Map the engine's turn [`Step`] onto the protocol [`Phase`]. The two enums are
/// deliberately decoupled (`sage-engine` never depends on `sage-protocol`), so the
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

/// The display name of a battlefield permanent — the card's name, or the token's
/// (CR 111.3: a token's name is whatever the effect that created it gave it).
///
/// Every prompt, label, and stack sentence that names a permanent goes through here
/// rather than through [`card_name`], so a token is named by what it is instead of
/// being reported as an unknown card.
pub(crate) fn permanent_name(perm: &sage_engine::Permanent, db: &CardDatabase) -> String {
    perm.printed.face(db).map_or_else(
        || match perm.printed.card() {
            Some(card) => format!("Unknown card {}", card.0),
            None => "Token".to_string(),
        },
        |face| face.name().to_string(),
    )
}

/// Build the full [`CardView`] for a card the viewer is entitled to see.
pub(crate) fn card_view(entity_id: String, card: CardId, db: &CardDatabase) -> CardView {
    match db.card(card) {
        Some(data) => full_card_view(entity_id, data),
        None => unknown_card_view(entity_id, Some(card)),
    }
}

/// The defensive placeholder view for an object the server cannot resolve: a card
/// handle absent from the database, or — with no handle at all — a token whose
/// characteristics somehow did not come through. Carries no identity and no rules, so
/// a client renders something legible rather than nothing.
fn unknown_card_view(entity_id: String, card: Option<CardId>) -> CardView {
    CardView {
        id: entity_id,
        name: match card {
            Some(card) => format!("Unknown card {}", card.0),
            None => "Token".to_string(),
        },
        type_line: String::new(),
        mana_cost: None,
        rules_text: String::new(),
        functional_id: String::new(),
        token: card.is_none(),
        power: None,
        toughness: None,
        keywords: Vec::new(),
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
        Keyword::Defender => "defender",
        Keyword::Menace => "menace",
        Keyword::FirstStrike => "first_strike",
        Keyword::Trample => "trample",
        Keyword::Deathtouch => "deathtouch",
        Keyword::Lifelink => "lifelink",
        Keyword::DoubleStrike => "double_strike",
        Keyword::Hexproof => "hexproof",
    }
}

/// Project engine [`CardData`] onto the wire [`CardView`]. Power/toughness become
/// strings so non-numeric values round-trip (`sage-protocol`); an empty mana cost
/// is elided rather than sent as `""`; printed keywords project to their lowercase
/// wire names for display.
///
/// The card's rules text is **generated** here from its ability IR
/// ([`crate::rules_text`], ADR 0008 §7) rather than read from a stored string — the
/// catalog holds no prose — and its authored `functional_id` rides along as the stable
/// presentation identity (ADR 0008 §8). A scripted card's hand-authored text comes from
/// the engine's escape hatch — keyed, like the catalog itself, on the card's authored
/// `functional_id` rather than its build-interned handle (ADR 0008 §3), and guaranteed
/// by the loader to exist whenever the definition declares `scripted: true`.
pub(crate) fn full_card_view(entity_id: String, data: &CardData) -> CardView {
    CardView {
        id: entity_id,
        name: data.name.clone(),
        type_line: data.type_line(),
        mana_cost: (!data.mana_cost.is_empty()).then(|| data.mana_cost.clone()),
        rules_text: rules_text(data, scripted_rules_text(&data.functional_id)),
        functional_id: data.functional_id.to_string(),
        token: false,
        power: data.power.map(|p| p.to_string()),
        toughness: data.toughness.map(|t| t.to_string()),
        keywords: data
            .keywords
            .iter()
            .map(|&kw| keyword_str(kw).to_owned())
            .collect(),
    }
}

/// Project a **permanent's printed face** onto the wire, whether it is a card or a
/// token (CR 111).
///
/// A card defers to [`full_card_view`], so nothing about an ordinary permanent's
/// projection changes. A token differs in exactly the two ways it differs in the
/// engine: it carries **no `functional_id`** — there is no card identity behind it, so
/// the field a client would cache or look art up by is empty, and `token` says why
/// rather than leaving the client to infer it from an absence — and its rules text is
/// generated from the abilities the creating effect gave it, through the same
/// formatter a card's text comes from.
fn face_card_view(entity_id: String, face: PrintedFace<'_>) -> CardView {
    match face {
        PrintedFace::Card(data) => full_card_view(entity_id, data),
        PrintedFace::Token(token) => CardView {
            id: entity_id,
            name: token.name.clone(),
            type_line: face.type_line(),
            // CR 111.3: a token has no mana cost, so the field is elided entirely.
            mana_cost: None,
            rules_text: token_rules_text(token),
            // A token is not a card and has no authored identity (ADR 0008 §3).
            functional_id: String::new(),
            token: true,
            power: token.power.map(|p| p.to_string()),
            toughness: token.toughness.map(|t| t.to_string()),
            keywords: token
                .keywords
                .iter()
                .map(|&kw| keyword_str(kw).to_owned())
                .collect(),
        },
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
    perm: &sage_engine::Permanent,
    db: &CardDatabase,
) -> CardView {
    let mut view = match perm.printed.face(db) {
        Some(face) => face_card_view(permanent_entity_id(perm.id), face),
        None => unknown_card_view(permanent_entity_id(perm.id), perm.printed.card()),
    };
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

    /// A **token** projects onto the wire as a complete, playable object with no card
    /// identity behind it (issue #605): its characteristics come from the token itself,
    /// its `functional_id` is empty because there is no card to name, and `token: true`
    /// says so outright rather than leaving a client to infer it from that absence.
    #[test]
    fn issue_605_a_token_projects_with_characteristics_and_no_card_identity() {
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();

        let id = PermanentId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id,
            instance: CardInstanceId(0),
            printed: sage_engine::Printed::Token(Box::new(sage_engine::TokenData {
                name: "Thopter".to_string(),
                types: vec![
                    sage_engine::CardType::Artifact,
                    sage_engine::CardType::Creature,
                ],
                subtypes: vec!["Thopter".to_string()],
                colors: Vec::new(),
                power: Some(1),
                toughness: Some(1),
                keywords: vec![Keyword::Flying],
                ..Default::default()
            })),
            controller: PlayerId(0),
            ..Default::default()
        });
        // A +1/+1 counter, to prove the projection is the *computed* face and not a
        // second, token-only read path.
        state.battlefield[0]
            .counters
            .insert(sage_engine::CounterKind::PlusOnePlusOne, 1);

        let view = personalized_view(&state, &db, PlayerId(0));
        let permanent = view
            .battlefield
            .iter()
            .find(|p| p.id == permanent_entity_id(id))
            .expect("the token appears on the battlefield");
        let card = &permanent.card;

        assert_eq!(card.name, "Thopter");
        assert_eq!(card.type_line, "Artifact Creature — Thopter");
        assert_eq!(card.power.as_deref(), Some("2"), "1/1 plus a +1/+1 counter");
        assert_eq!(card.toughness.as_deref(), Some("2"));
        assert_eq!(card.keywords, vec!["flying".to_string()]);
        assert_eq!(card.rules_text, "Flying");
        assert!(card.token, "the client is told it is a token");
        assert!(
            card.functional_id.is_empty(),
            "a token has no card identity to cache or look presentation up by"
        );
        assert!(card.mana_cost.is_none(), "a token has no mana cost");
    }

    /// A battlefield permanent enchanted with an Aura projects its **current**
    /// (computed) power/toughness on the wire, so the host's P/T reflects the Aura's
    /// layer-7c grant (CR 303.4 / 613.7c, issue #152) rather than the printed value.
    #[test]
    fn issue_152_aura_boosted_host_projects_current_pt() {
        // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0009):
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
        state.battlefield.push(sage_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            printed: id_in(&db, "test_scout").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: aura,
            instance: CardInstanceId(1),
            printed: id_in(&db, "test_aegis").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: with_counters,
            instance: CardInstanceId(0),
            printed: fixture("forest").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: without_counters,
            instance: CardInstanceId(1),
            printed: fixture("forest").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: attacker,
            instance: CardInstanceId(0),
            printed: fixture("walking_corpse").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: blocker,
            instance: CardInstanceId(1),
            printed: fixture("walking_corpse").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: damaged,
            instance: CardInstanceId(0),
            printed: fixture("onakke_ogre").into(),
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

        // P/T Auras have no clean M19 card, so this is exercised inline (ADR 0009).
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
        state.battlefield.push(sage_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            printed: id_in(&db, "test_scout").into(),
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
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);
        let state = sage_engine::apply_action(&state, &Action::PassPriority, &db);

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
        state.battlefield.push(sage_engine::Permanent {
            id: flyer,
            instance: CardInstanceId(0),
            printed: fixture("snapping_drake").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: vanilla,
            instance: CardInstanceId(1),
            printed: fixture("onakke_ogre").into(),
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
    /// permanent's card view exactly like a printed one: a plain body (no printed
    /// keyword) enchanted with an Aura granting flying shows `flying` on the wire,
    /// and a second, unenchanted body shows none.
    #[test]
    fn issue_374_granted_keyword_projects_onto_the_card_view() {
        let db = CardDatabase::from_json(
            r#"[
                {"schema_version":1,"functional_id":"test_flight","name":"Test Flight",
                 "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{U}","colors":["blue"],
                 "aura":{"enchant":"any_creature","keywords":["flying"]}},
                {"schema_version":1,"functional_id":"test_ogre","name":"Test Ogre",
                 "types":["creature"],"subtypes":["Ogre"],"mana_cost":"{2}{R}","colors":["red"],
                 "power":4,"toughness":2}
            ]"#,
        )
        .unwrap();
        let mut state = GameState::new_two_player();

        let host = PermanentId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id: host,
            instance: CardInstanceId(0),
            printed: id_in(&db, "test_ogre").into(),
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
        state.battlefield.push(sage_engine::Permanent {
            id: bystander,
            instance: CardInstanceId(1),
            printed: id_in(&db, "test_ogre").into(),
            controller: PlayerId(0),
            tapped: false,
            entered_turn: 0,
            attacking: None,
            blocking: None,
            damage: 0,
            counters: std::collections::BTreeMap::new(),
            attached_to: None,
        });
        // A keyword-only Aura granting flying, attached to the host. M19 prints no
        // such Aura (Prodigious Growth grants trample alongside +7/+7), so the shape
        // is exercised by an inline definition rather than a shipped card (ADR 0009).
        let aura = PermanentId(state.mint_id());
        state.battlefield.push(sage_engine::Permanent {
            id: aura,
            instance: CardInstanceId(2),
            printed: id_in(&db, "test_flight").into(),
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

    /// The ability-target `requirements` projection (ADR 0004 deferral #73, folded
    /// into issue #140): a `{T}: Tap target creature` activation advertises its one
    /// target slot with the legal creature candidates, and a returned target
    /// resolves to an `ActivateAbility` carrying exactly that chosen target.
    #[test]
    fn issue_194_cards_project_generated_rules_text_and_their_stable_identity() {
        // ADR 0008 §7-§8: the catalog stores no prose, so what the player reads is
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
