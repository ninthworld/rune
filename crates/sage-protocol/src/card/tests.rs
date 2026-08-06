//! Round-trip and compatibility tests for the in-game card, board, and zone views.
//!
//! Split out of `card.rs` for size (issue #711). Pure code motion — every test is
//! unchanged and moves with the code it exercises.

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use crate::*;

#[test]
fn issue_255_self_view_round_trips_and_defaults_when_omitted() {
    // The receiver's own public stats round-trip on their own...
    let me = SelfView {
        life: 15,
        library_size: 40,
        ..Default::default()
    };
    let back: SelfView = serde_json::from_str(&serde_json::to_string(&me).unwrap()).unwrap();
    assert_eq!(back, me);

    // ...and a GameView from an older server that omits `me` still deserializes,
    // defaulting to a zero placeholder rather than failing (the `you`-field pattern).
    let view: GameView = serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
    assert_eq!(view.me, SelfView::default());
    assert_eq!(view.me.life, 0);
}

#[test]
fn permanent_combat_state_round_trips_and_elides_when_absent() {
    // Attack/block state (issue #117) and marked damage (issue #118):
    // `attacking`, `blocking`, and `damage` round-trip when present, and all
    // elide from the wire in the common not-in-combat, undamaged case so the
    // serialized shape is unchanged for non-combat permanents.
    let base = Permanent {
        id: "perm_1".into(),
        controller: "p0".into(),
        owner: "p0".into(),
        physical_card: None,
        card: CardView {
            id: "perm_1".into(),
            name: "Grizzly Bears".into(),
            type_line: "Creature — Bear".into(),
            mana_cost: Some("{1}{G}".into()),
            rules_text: String::new(),
            functional_id: String::new(),
            token: false,
            power: Some("2".into()),
            toughness: Some("2".into()),
            loyalty: None,
            keywords: vec![],
            card_types: Vec::new(),
            color_identity: Vec::new(),
        },
        tapped: false,
        attacking: false,
        attacking_player: None,
        attacking_planeswalker: None,
        blocking: Vec::new(),
        damage: 0,
        attached_to: None,
        chosen_color: None,
        named_card: None,
        is_commander: false,
        counters: vec![],
        summoning_sick: false,
        skips_next_untap: false,
        granted_keywords: Vec::new(),
    };

    // Not in combat and undamaged: all three fields elide from the JSON.
    let json = serde_json::to_value(&base).unwrap();
    assert!(json.get("attacking").is_none());
    assert!(json.get("blocking").is_none());
    assert!(json.get("damage").is_none());

    // The two answers a permanent can carry from its own entry (CR 614.12, issue #738)
    // are additive in exactly the same way: absent for almost every permanent, and a
    // plain value when present. The named card travels as a **name** — the catalog's own
    // name for the card the engine recorded an identity for — so a client neither parses
    // an identity nor is ever shown one the catalog does not contain.
    assert!(json.get("chosen_color").is_none());
    assert!(json.get("named_card").is_none());
    let named = Permanent {
        chosen_color: Some(crate::Color::Red),
        named_card: Some("Highland Lake".into()),
        ..base.clone()
    };
    let named_json = serde_json::to_value(&named).unwrap();
    assert_eq!(
        named_json.get("chosen_color"),
        Some(&serde_json::json!("R"))
    );
    assert_eq!(
        named_json.get("named_card"),
        Some(&serde_json::json!("Highland Lake"))
    );
    assert_eq!(
        serde_json::from_value::<Permanent>(named_json).unwrap(),
        named
    );

    // An attacker and its blocker both round-trip with their state present.
    let attacker = Permanent {
        attacking: true,
        attacking_player: None,
        attacking_planeswalker: None,
        ..base.clone()
    };
    let blocker = Permanent {
        blocking: vec!["perm_1".into()],
        ..base.clone()
    };
    let attacker_json = serde_json::to_value(&attacker).unwrap();
    assert_eq!(
        attacker_json.get("attacking"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        serde_json::from_value::<Permanent>(attacker_json).unwrap(),
        attacker
    );
    let blocker_json = serde_json::to_value(&blocker).unwrap();
    assert_eq!(
        blocker_json.get("blocking"),
        Some(&serde_json::json!(["perm_1"]))
    );
    assert_eq!(
        serde_json::from_value::<Permanent>(blocker_json).unwrap(),
        blocker
    );

    // Marked damage round-trips when non-zero and serializes as a number.
    let damaged = Permanent {
        damage: 2,
        ..base.clone()
    };
    let damaged_json = serde_json::to_value(&damaged).unwrap();
    assert_eq!(damaged_json.get("damage"), Some(&serde_json::json!(2)));
    assert_eq!(
        serde_json::from_value::<Permanent>(damaged_json).unwrap(),
        damaged
    );
}

#[test]
fn permanent_attachment_round_trips_and_elides_when_absent() {
    // Aura attachment (issue #333, CR 303.4): `attached_to` names the host's
    // entity id when the permanent is attached, round-trips through the wire,
    // and elides entirely for an unattached permanent so the common non-Aura
    // shape is unchanged.
    let base = Permanent {
        id: "perm_1".into(),
        controller: "p0".into(),
        owner: "p0".into(),
        physical_card: None,
        card: CardView {
            id: "perm_1".into(),
            name: "Ironbark Aegis".into(),
            type_line: "Enchantment — Aura".into(),
            mana_cost: Some("{1}{G}".into()),
            rules_text: "Enchant creature".into(),
            functional_id: String::new(),
            token: false,
            power: None,
            toughness: None,
            loyalty: None,
            keywords: vec![],
            card_types: Vec::new(),
            color_identity: Vec::new(),
        },
        tapped: false,
        attacking: false,
        attacking_player: None,
        attacking_planeswalker: None,
        blocking: Vec::new(),
        damage: 0,
        attached_to: None,
        chosen_color: None,
        named_card: None,
        is_commander: false,
        counters: vec![],
        summoning_sick: false,
        skips_next_untap: false,
        granted_keywords: Vec::new(),
    };

    // Unattached: the field elides from the JSON.
    let json = serde_json::to_value(&base).unwrap();
    assert!(json.get("attached_to").is_none());

    // Attached: the host id round-trips and serializes as a string.
    let attached = Permanent {
        attached_to: Some("perm_9".into()),
        ..base.clone()
    };
    let attached_json = serde_json::to_value(&attached).unwrap();
    assert_eq!(
        attached_json.get("attached_to"),
        Some(&serde_json::json!("perm_9"))
    );
    assert_eq!(
        serde_json::from_value::<Permanent>(attached_json).unwrap(),
        attached
    );
}

#[test]
fn issue_153_card_keywords_round_trip_and_elide_when_absent() {
    // Keyword abilities (issue #153) surface on a CardView as lowercase wire
    // names for display; the list round-trips when present and elides from the
    // JSON when the card has none, so a keyword-less card keeps its terse shape.
    let base = CardView {
        id: "c1".into(),
        name: "Snapping Drake".into(),
        type_line: "Creature — Drake".into(),
        mana_cost: Some("{3}{U}".into()),
        rules_text: "Flying".into(),
        functional_id: "snapping_drake".into(),
        token: false,
        power: Some("3".into()),
        toughness: Some("2".into()),
        loyalty: None,
        keywords: vec!["flying".into()],
        card_types: Vec::new(),
        color_identity: Vec::new(),
    };
    let json = serde_json::to_value(&base).unwrap();
    assert_eq!(json.get("keywords"), Some(&serde_json::json!(["flying"])));
    assert_eq!(serde_json::from_value::<CardView>(json).unwrap(), base);

    // A card with no keywords omits the field entirely.
    let vanilla = CardView {
        keywords: vec![],
        card_types: Vec::new(),
        ..base.clone()
    };
    let vanilla_json = serde_json::to_value(&vanilla).unwrap();
    assert!(vanilla_json.get("keywords").is_none());
}

#[test]
fn issue_550_stack_item_kind_targets_and_card_elide_when_absent() {
    // The pre-#550 shape: a spell entry with none of the additive fields set
    // serializes to exactly the four keys it always had, so an existing payload
    // (and the canonical fixture's terse entries) is unchanged on the wire.
    let bare = StackItem {
        id: "s1".into(),
        controller: "p2".into(),
        description: "Lightning Bolt".into(),
        source: None,
        physical_card: None,
        kind: None,
        targets: vec![],
        card: None,
    };
    let json = serde_json::to_value(&bare).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "id": "s1",
            "controller": "p2",
            "description": "Lightning Bolt",
        })
    );
    assert_eq!(serde_json::from_value::<StackItem>(json).unwrap(), bare);
}

#[test]
fn issue_550_an_older_payload_parses_with_the_new_fields_defaulted() {
    // Backward compatibility: a stack entry from a server predating #550 carries
    // no `kind`, `targets`, or `card`. It must still deserialize, defaulting to
    // "unclassified, targetless, no face" rather than failing the whole message —
    // and, crucially, never defaulting `kind` to a *guess*.
    let older = r#"{"id":"s2","controller":"p1","description":"Add {G}.","source":"perm_bear"}"#;
    let item: StackItem = serde_json::from_str(older).unwrap();
    assert_eq!(item.kind, None, "an absent kind is unknown, never guessed");
    assert!(item.targets.is_empty());
    assert_eq!(item.card, None);
    assert_eq!(item.source.as_deref(), Some("perm_bear"));
}

#[test]
fn issue_579_every_stack_item_kind_round_trips_as_its_documented_snake_case_value() {
    // The union after #579's widening. Each value is the exact string
    // `docs/protocol.md` and the TypeScript mirror's `STACK_ITEM_KINDS` list, so a
    // rename here fails cross-language rather than silently drifting.
    let cases = [
        (StackItemKind::Spell, "spell"),
        (StackItemKind::Ability, "ability"),
        (StackItemKind::Activated, "activated"),
        (StackItemKind::Triggered, "triggered"),
    ];
    for (kind, wire) in cases {
        let json = serde_json::to_value(kind).unwrap();
        assert_eq!(json, serde_json::json!(wire));
        assert_eq!(serde_json::from_value::<StackItemKind>(json).unwrap(), kind);
    }
}

#[test]
fn issue_579_the_coarse_ability_kind_survives_the_widening() {
    // The compatibility rule the widening rests on: a server that predates #579
    // states only `ability`, and that payload must keep deserializing to the coarse
    // variant — not become a parse error, and not be refined into a guess.
    let legacy = r#"{"id":"s2","controller":"p1","description":"Add {G}.",
                     "source":"perm_bear","kind":"ability"}"#;
    let item: StackItem = serde_json::from_str(legacy).unwrap();
    assert_eq!(item.kind, Some(StackItemKind::Ability));
    assert_ne!(item.kind, Some(StackItemKind::Activated));
    assert_ne!(item.kind, Some(StackItemKind::Triggered));
}

#[test]
fn issue_650_the_physical_card_round_trips_and_elides_on_both_projections() {
    // The projection of the physical card a permanent and a spell are of (CR 108.1).
    // It rides the wire only when there is a card to name, so a token permanent and
    // an ability on the stack are byte-for-byte what they were before the field
    // existed — and a client that ignores it renders exactly as it did.
    let face = CardView {
        id: "perm_9".into(),
        name: "Grizzly Bears".into(),
        type_line: "Creature — Bear".into(),
        mana_cost: Some("{1}{G}".into()),
        rules_text: String::new(),
        functional_id: "grizzly_bears".into(),
        token: false,
        power: Some("2".into()),
        toughness: Some("2".into()),
        loyalty: None,
        keywords: vec![],
        card_types: Vec::new(),
        color_identity: Vec::new(),
    };
    let permanent = Permanent {
        id: "perm_9".into(),
        controller: "p0".into(),
        owner: "p0".into(),
        card: face.clone(),
        physical_card: Some("card_5".into()),
        tapped: false,
        attacking: false,
        attacking_player: None,
        attacking_planeswalker: None,
        blocking: Vec::new(),
        damage: 0,
        attached_to: None,
        chosen_color: None,
        named_card: None,
        is_commander: false,
        counters: vec![],
        summoning_sick: false,
        skips_next_untap: false,
        granted_keywords: Vec::new(),
    };
    let json = serde_json::to_value(&permanent).unwrap();
    assert_eq!(
        json.get("physical_card"),
        Some(&serde_json::json!("card_5"))
    );
    // The per-zone id and the physical card are *different* ids and stay so: CR 400.7
    // makes the permanent and the card it becomes elsewhere two different objects.
    assert_ne!(json.get("physical_card"), json.get("id"));
    assert_eq!(
        serde_json::from_value::<Permanent>(json).unwrap(),
        permanent
    );

    // A token (CR 111) is not a card, so it names none — and CR 111.7 means the join
    // it would offer could never have a second end.
    let token = Permanent {
        physical_card: None,
        ..permanent.clone()
    };
    assert!(serde_json::to_value(&token)
        .unwrap()
        .get("physical_card")
        .is_none());

    // A spell names the card being cast; an ability, having no card, names nothing.
    let spell = StackItem {
        id: "stack_3".into(),
        controller: "p0".into(),
        description: "Grizzly Bears".into(),
        source: None,
        physical_card: Some("card_5".into()),
        kind: Some(StackItemKind::Spell),
        targets: vec![],
        card: Some(face),
    };
    let spell_json = serde_json::to_value(&spell).unwrap();
    assert_eq!(
        spell_json.get("physical_card"),
        Some(&serde_json::json!("card_5"))
    );
    assert_eq!(
        serde_json::from_value::<StackItem>(spell_json).unwrap(),
        spell
    );

    let ability = StackItem {
        id: "stack_4".into(),
        controller: "p0".into(),
        description: "Add {G}.".into(),
        source: Some("perm_9".into()),
        physical_card: None,
        kind: Some(StackItemKind::Activated),
        targets: vec![],
        card: None,
    };
    assert!(serde_json::to_value(&ability)
        .unwrap()
        .get("physical_card")
        .is_none());
}

#[test]
fn issue_650_an_older_payload_parses_with_no_physical_card_claimed() {
    // Backward compatibility, and the shape of the absence: a payload from a server
    // predating #650 carries no `physical_card` on either projection. It must
    // deserialize to "not stated" — never to a guess, and above all never to the
    // object's own id, which would assert exactly the identity CR 400.7 denies.
    let permanent: Permanent = serde_json::from_str(
        r#"{"id":"perm_9","controller":"p0","owner":"p0",
            "card":{"id":"perm_9","name":"Grizzly Bears","type_line":"Creature — Bear"}}"#,
    )
    .unwrap();
    assert_eq!(permanent.physical_card, None);

    let item: StackItem = serde_json::from_str(
        r#"{"id":"stack_3","controller":"p0","description":"Grizzly Bears","kind":"spell"}"#,
    )
    .unwrap();
    assert_eq!(item.physical_card, None);
}

#[test]
fn issue_650_two_copies_of_one_card_are_told_apart_by_the_physical_card_alone() {
    // The case the whole field exists for. Two Forests differ in nothing a client can
    // see — same name, same `functional_id`, same type line — so a join by either
    // would be the client *deciding* which one moved, and wrong half the time.
    let forest = |permanent: &str, card: &str| Permanent {
        id: permanent.into(),
        controller: "p0".into(),
        owner: "p0".into(),
        card: CardView {
            id: permanent.into(),
            name: "Forest".into(),
            type_line: "Basic Land — Forest".into(),
            mana_cost: None,
            rules_text: String::new(),
            functional_id: "forest".into(),
            token: false,
            power: None,
            toughness: None,
            loyalty: None,
            keywords: vec![],
            card_types: vec![CardType::Land],
            color_identity: Vec::new(),
        },
        physical_card: Some(card.into()),
        tapped: false,
        attacking: false,
        attacking_player: None,
        attacking_planeswalker: None,
        blocking: Vec::new(),
        damage: 0,
        attached_to: None,
        chosen_color: None,
        named_card: None,
        is_commander: false,
        counters: vec![],
        summoning_sick: false,
        skips_next_untap: false,
        granted_keywords: Vec::new(),
    };
    let first = forest("perm_9", "card_5");
    let second = forest("perm_10", "card_6");

    assert_eq!(first.card.name, second.card.name);
    assert_eq!(first.card.functional_id, second.card.functional_id);
    assert_ne!(first.physical_card, second.physical_card);
}

#[test]
fn issue_550_every_stack_target_variant_round_trips_tagged_by_kind() {
    // Targets are typed at the source (gap G6): each variant states what it names,
    // so a client never classifies a target by testing which collection its id is
    // in. The `player` variant deliberately carries a `PlayerId` under its own key.
    let cases = [
        (
            StackTarget::Player {
                player: "p2".into(),
            },
            serde_json::json!({"kind": "player", "player": "p2"}),
        ),
        (
            StackTarget::Permanent {
                id: "perm_bear".into(),
            },
            serde_json::json!({"kind": "permanent", "id": "perm_bear"}),
        ),
        (
            StackTarget::Card {
                id: "card_7".into(),
            },
            serde_json::json!({"kind": "card", "id": "card_7"}),
        ),
        (
            StackTarget::Stack { id: "s1".into() },
            serde_json::json!({"kind": "stack", "id": "s1"}),
        ),
    ];
    for (target, expected) in cases {
        let json = serde_json::to_value(&target).unwrap();
        assert_eq!(json, expected);
        assert_eq!(serde_json::from_value::<StackTarget>(json).unwrap(), target);
    }
}

#[test]
fn issue_550_a_multi_target_spell_entry_round_trips_in_order() {
    // The ordered, server-authored target list is the client's numbering channel
    // (①②③): order must survive the wire exactly as sent, and the whole entry —
    // kind, card face, and targets — must round-trip so a reconnecting client
    // rebuilds every relationship from one message.
    let item = StackItem {
        id: "s3".into(),
        controller: "p1".into(),
        description: "Twin Bolt deals 1 damage to each of two targets.".into(),
        source: None,
        physical_card: Some("card_31".into()),
        kind: Some(StackItemKind::Spell),
        targets: vec![
            StackTarget::Permanent {
                id: "perm_bear".into(),
            },
            StackTarget::Player {
                player: "p2".into(),
            },
        ],
        card: Some(CardView {
            id: "card_31".into(),
            name: "Twin Bolt".into(),
            type_line: "Instant".into(),
            mana_cost: Some("{1}{R}".into()),
            rules_text: "Twin Bolt deals 1 damage to each of two targets.".into(),
            functional_id: "twin_bolt".into(),
            token: false,
            power: None,
            toughness: None,
            loyalty: None,
            keywords: vec![],
            card_types: Vec::new(),
            color_identity: Vec::new(),
        }),
    };
    let json = serde_json::to_string(&item).unwrap();
    let back: StackItem = serde_json::from_str(&json).unwrap();
    assert_eq!(back, item);
    assert_eq!(
        back.targets[0],
        StackTarget::Permanent {
            id: "perm_bear".into()
        },
        "the first target stays first — numbering comes from this order"
    );
    assert_eq!(back.kind, Some(StackItemKind::Spell));
    assert_eq!(back.card.map(|c| c.name).as_deref(), Some("Twin Bolt"));
}
