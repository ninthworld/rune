//! Round-trip and compatibility tests for [`GameView`](crate::GameView).
//!
//! Split out of `view.rs` (issues #553/#554): the module had grown to ~75% tests and
//! crossed the file-size ceiling in `docs/coding-standards.md`. Pure code motion —
//! every test is unchanged and moves with the code it exercises.

#![allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests

use std::collections::BTreeMap;

use crate::*;

#[test]
fn issue_264_game_view_stops_and_auto_passed_round_trip_and_elide() {
    // `stops` and `auto_passed` ride the view; both elide from the wire at their
    // defaults (empty / false) and round-trip when present.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::Upkeep,
        turn: 1,
        active_player: "p0".into(),
        priority_player: Some("p0".into()),
        ..Default::default()
    };
    // Defaults elide.
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("stops").is_none());
    assert!(json.get("auto_passed").is_none());

    // Present values round-trip.
    view.stops = vec![Phase::Upkeep, Phase::PostcombatMain];
    view.auto_passed = true;
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json["stops"],
        serde_json::json!(["upkeep", "postcombat_main"])
    );
    assert_eq!(json["auto_passed"], serde_json::json!(true));
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);

    // An older server that omits both still deserializes to the defaults.
    let legacy: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert!(legacy.stops.is_empty());
    assert!(!legacy.auto_passed);
}

#[test]
fn issue_455_own_turn_stops_and_auto_passed_steps_round_trip_and_elide() {
    // The pacing contract's two additive fields: the narrower half of the stop
    // preference, and the path a settle took on this receiver's behalf. Both elide
    // at their empty defaults, so a view from before they existed is unchanged.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::DeclareAttackers,
        turn: 2,
        active_player: "p1".into(),
        ..Default::default()
    };
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("own_turn_stops").is_none());
    assert!(json.get("auto_passed_steps").is_none());

    // The seat defaults to stopping at its own main phases, and this settle ran it
    // through the three idle steps before combat. Every entry carries its own turn.
    view.own_turn_stops = vec![Phase::PrecombatMain, Phase::PostcombatMain];
    view.auto_passed = true;
    view.auto_passed_steps = vec![
        AutoPassedStep {
            phase: Phase::Upkeep,
            turn: 2,
        },
        AutoPassedStep {
            phase: Phase::Draw,
            turn: 2,
        },
        AutoPassedStep {
            phase: Phase::BeginCombat,
            turn: 2,
        },
    ];
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json["own_turn_stops"],
        serde_json::json!(["precombat_main", "postcombat_main"])
    );
    assert_eq!(
        json["auto_passed_steps"],
        serde_json::json!([
            { "phase": "upkeep", "turn": 2 },
            { "phase": "draw", "turn": 2 },
            { "phase": "begin_combat", "turn": 2 },
        ])
    );
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);

    // An older server that omits both still deserializes to the defaults.
    let legacy: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert!(legacy.own_turn_stops.is_empty());
    assert!(legacy.auto_passed_steps.is_empty());
}

#[test]
fn issue_455_the_auto_passed_path_keeps_every_occurrence_and_states_its_turn() {
    // The path property, and the reason each entry carries a turn at all.
    //
    // A repeated step does NOT mean the settle crossed a turn: an extra combat phase
    // (CR 506.1) revisits the combat steps inside one turn, and an extra cleanup
    // (CR 514.3a) revisits cleanup. So "same phase twice" and "new turn" are
    // independent facts, and the wire states both rather than letting a client derive
    // one from the other — which is why the two halves of this test look alike on the
    // phase axis and differ entirely on the turn axis.
    let crossed = vec![
        AutoPassedStep {
            phase: Phase::End,
            turn: 3,
        },
        AutoPassedStep {
            phase: Phase::Upkeep,
            turn: 4,
        },
        AutoPassedStep {
            phase: Phase::End,
            turn: 4,
        },
    ];
    let within = vec![
        AutoPassedStep {
            phase: Phase::DeclareAttackers,
            turn: 4,
        },
        AutoPassedStep {
            phase: Phase::EndCombat,
            turn: 4,
        },
        AutoPassedStep {
            phase: Phase::DeclareAttackers,
            turn: 4,
        },
    ];

    for path in [&crossed, &within] {
        let view = GameView {
            you: "p0".into(),
            phase: Phase::End,
            auto_passed: true,
            auto_passed_steps: path.clone(),
            auto_passed_from: None,
            ..Default::default()
        };
        let back: GameView = serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
        assert_eq!(
            back.auto_passed_steps, *path,
            "every occurrence survives in order"
        );
    }

    // The two paths are told apart by their turns alone — a phase-only reading would
    // call both of them a turn change, and be wrong about one of them.
    let turns = |path: &[AutoPassedStep]| path.iter().map(|s| s.turn).collect::<Vec<_>>();
    assert_eq!(turns(&crossed), vec![3, 4, 4]);
    assert_eq!(turns(&within), vec![4, 4, 4]);
}

#[test]
fn issue_345_multiplayer_combat_and_elimination_fields_round_trip_and_elide() {
    // The multiplayer contract fields — a permanent's `attacking_player`, an
    // opponent's `eliminated`, and the view's `seat_order` — round-trip and elide
    // from the wire at their two-player defaults, so an older-shaped view renders
    // exactly as today.
    let card: CardView =
        serde_json::from_str(r#"{"id":"perm_1","name":"Raider","type_line":"Creature — Orc"}"#)
            .unwrap();
    let attacker = Permanent {
        id: "perm_1".into(),
        controller: "p0".into(),
        owner: "p0".into(),
        card,
        physical_card: None,
        tapped: false,
        attacking: true,
        attacking_player: Some("p2".into()),
        attacking_planeswalker: None,
        blocking: None,
        damage: 0,
        attached_to: None,
        is_commander: false,
        counters: vec![],
    };
    let json = serde_json::to_value(&attacker).unwrap();
    assert_eq!(json["attacking_player"], serde_json::json!("p2"));
    assert_eq!(serde_json::from_value::<Permanent>(json).unwrap(), attacker);

    // A not-attacking permanent omits `attacking_player`.
    let idle = Permanent {
        attacking: false,
        attacking_player: None,
        attacking_planeswalker: None,
        ..attacker.clone()
    };
    assert!(serde_json::to_value(&idle)
        .unwrap()
        .get("attacking_player")
        .is_none());

    // `eliminated` rides the opponent and elides when false.
    let out = OpponentView {
        player_id: "p1".into(),
        hand_size: 0,
        life: 0,
        library_size: 0,
        graveyard_size: 0,
        statuses: vec![],
        eliminated: true,
        connected: true,
        ai: false,
    };
    assert_eq!(serde_json::to_value(&out).unwrap()["eliminated"], true);
    let alive = OpponentView {
        eliminated: false,
        ..out.clone()
    };
    assert!(serde_json::to_value(&alive)
        .unwrap()
        .get("eliminated")
        .is_none());

    // An older opponent/permanent that omits the new fields deserializes to the
    // two-player defaults.
    let legacy_perm: Permanent = serde_json::from_str(
        r#"{"id":"perm_1","controller":"p0","owner":"p0","card":{"id":"perm_1","name":"","type_line":""},"attacking":true}"#,
    )
    .unwrap();
    assert!(legacy_perm.attacking_player.is_none());
    let legacy_opp: OpponentView = serde_json::from_str(
        r#"{"player_id":"p1","hand_size":0,"life":0,"library_size":0,"graveyard_size":0}"#,
    )
    .unwrap();
    assert!(!legacy_opp.eliminated);
}

#[test]
fn game_view_round_trips_through_json() {
    let view = GameView {
        you: "p1".into(),
        // An emblem (CR 114, issue #620): public, in no zone, and nothing but its
        // abilities — so the round trip carries one to prove the shape survives.
        emblems: vec![Emblem {
            id: "emblem_41".into(),
            controller: "p1".into(),
            abilities: vec!["Creatures you control get +2/+2.".into()],
        }],
        // Cards from a hidden zone this seat alone is being shown (issue #604); empty
        // in the ordinary case, so it elides from the wire.
        revealed: Vec::new(),
        my_hand: vec![CardView {
            id: "c1".into(),
            name: "Llanowar Elves".into(),
            type_line: "Creature — Elf Druid".into(),
            mana_cost: Some("{G}".into()),
            rules_text: "{T}: Add {G}.".into(),
            functional_id: "llanowar_elves".into(),
            token: false,
            power: Some("1".into()),
            toughness: Some("1".into()),
            loyalty: None,
            keywords: vec![],
            card_types: Vec::new(),
        }],
        me: SelfView {
            life: 18,
            library_size: 52,
            eliminated: false,
            connected: true,
            ai: false,
        },
        opponents: vec![OpponentView {
            player_id: "p2".into(),
            hand_size: 7,
            life: 20,
            library_size: 53,
            graveyard_size: 0,
            statuses: vec!["monarch".into()],
            eliminated: false,
            connected: true,
            ai: false,
        }],
        battlefield: vec![Permanent {
            id: "perm_xyz".into(),
            controller: "p1".into(),
            owner: "p1".into(),
            physical_card: Some("card_77".into()),
            card: CardView {
                id: "perm_xyz".into(),
                name: "Grizzly Bears".into(),
                type_line: "Creature — Bear".into(),
                mana_cost: Some("{1}{G}".into()),
                rules_text: String::new(),
                functional_id: String::new(),
                token: false,
                power: Some("2".into()),
                toughness: Some("2".into()),
                loyalty: None,
                keywords: vec!["flying".into()],
                card_types: Vec::new(),
            },
            tapped: true,
            attacking: false,
            attacking_player: None,
            attacking_planeswalker: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            is_commander: false,
            counters: vec![Counter {
                kind: "+1/+1".into(),
                count: 2,
            }],
        }],
        stack: vec![StackItem {
            id: "s1".into(),
            controller: "p2".into(),
            description: "Lightning Bolt".into(),
            source: None,
            physical_card: Some("card_78".into()),
            kind: Some(StackItemKind::Spell),
            targets: vec![StackTarget::Player {
                player: "p1".into(),
            }],
            card: None,
        }],
        graveyards: vec![ZonePile {
            player_id: "p1".into(),
            cards: vec![],
        }],
        exile: vec![],
        command: vec![],
        phase: Phase::PrecombatMain,
        turn: 3,
        active_player: "p1".into(),
        seat_order: Vec::new(),
        mana_pool: vec!["{G}".into()],
        priority_player: Some("p1".into()),
        valid_actions: vec![ValidAction {
            mana_ability: false,
            id: "a2".into(),
            kind: "activate_ability".into(),
            label: "Tap for mana".into(),
            subject: vec!["perm_xyz".into()],
            requirements: vec![],
            prompts: vec![],
            destinations: vec![],
            token: "h:00ab".into(),
        }],
        action_deadline: Some(12.5),
        result: None,
        log: vec![GameLogEntry {
            sequence: 41,
            event: GameLogEvent::CardsDrawn {
                player: "p1".into(),
                count: 1,
            },
        }],
        stops: Vec::new(),
        // Pacing contract (issue #455): the own-turn half of the stop preference and
        // the steps a settle skipped this receiver at. Both empty on this frame, so
        // their defaults ride the exhaustive round trip too.
        own_turn_stops: Vec::new(),
        auto_passed: false,
        auto_passed_steps: Vec::new(),
        auto_passed_from: None,
        action_rejected: false,
        // Submission acknowledgement (issue #554): absent on an ordinary broadcast.
        action_ack: None,
        player_names: BTreeMap::new(),
        // Commander combat damage (issue #371): a public per-commander tally.
        commander_damage: vec![CommanderDamage {
            commander: "p2".into(),
            damaged: "p1".into(),
            amount: 14,
        }],
        commander_tax: Vec::new(),
        // In-match presentation metadata (issue #553): the format signal and the
        // designation-keyed commander identity, both absent for this non-Commander
        // frame so their defaults ride the exhaustive round trip too.
        format: None,
        commander_identity: Vec::new(),
    };

    let json = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&json).unwrap();
    assert_eq!(back, view);
    // The receiver's own stats survive the round trip (issue #255).
    assert_eq!(back.me.life, 18);
    assert_eq!(back.me.library_size, 52);
    // The commander-damage tally round-trips (issue #371).
    assert_eq!(back.commander_damage[0].amount, 14);
}

#[test]
fn issue_372_command_zone_and_tax_round_trip_and_elide_when_empty() {
    // The command zone (CR 903.6) rides the same public `ZonePile` shape as
    // graveyards/exile, and the commander tax (CR 903.8) rides its own additive
    // list; both are omitted from the wire for a non-commander game.
    let tax = CommanderTax {
        commander: "p1".into(),
        casts: 2,
        tax: 4,
    };
    let json = serde_json::to_value(&tax).unwrap();
    assert_eq!(json["commander"], "p1");
    assert_eq!(json["casts"], 2);
    assert_eq!(json["tax"], 4);
    let back: CommanderTax = serde_json::from_value(json).unwrap();
    assert_eq!(back, tax);

    // A minimal view carries neither the command zone nor the tax.
    let view: GameView = serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
    assert!(view.command.is_empty());
    assert!(view.commander_tax.is_empty());
    let round = serde_json::to_value(&view).unwrap();
    assert!(round.get("command").is_none());
    assert!(round.get("commander_tax").is_none());

    // A zero tax elides `casts`/`tax` but the entry (its presence) still marks a
    // commander in play.
    let zero = serde_json::to_value(CommanderTax {
        commander: "p0".into(),
        casts: 0,
        tax: 0,
    })
    .unwrap();
    assert_eq!(zero, serde_json::json!({ "commander": "p0" }));
}

#[test]
fn issue_372_command_zone_pile_round_trips_with_its_commander() {
    // A populated command zone (CR 903.6) carries a public `ZonePile` per player,
    // exactly like graveyards/exile: its commander card round-trips verbatim under
    // the `command` key. (The elide-when-empty case is covered above; this is the
    // populated round-trip the field previously lacked.)
    let mut view: GameView =
        serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
    view.command = vec![ZonePile {
        player_id: "p0".into(),
        cards: vec![CardView {
            id: "c9".into(),
            name: "Lathliss, Dragon Queen".into(),
            type_line: "Legendary Creature — Cat Warrior".into(),
            mana_cost: Some("{4}{G}{G}".into()),
            rules_text: String::new(),
            functional_id: "lathliss_dragon_queen".into(),
            token: false,
            power: Some("5".into()),
            toughness: Some("5".into()),
            loyalty: None,
            keywords: vec![],
            card_types: Vec::new(),
        }],
    }];
    let json = serde_json::to_value(&view).unwrap();
    // The populated zone rides the wire under `command`, one pile per player.
    assert_eq!(json["command"][0]["player_id"], "p0");
    assert_eq!(
        json["command"][0]["cards"][0]["functional_id"],
        "lathliss_dragon_queen"
    );
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back.command, view.command);
    assert_eq!(back, view);
}

#[test]
fn empty_game_view_round_trips() {
    let view = GameView {
        you: "p0".into(),
        phase: Phase::Upkeep,
        ..Default::default()
    };
    let json = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&json).unwrap();
    assert_eq!(back, view);
}

#[test]
fn mana_pool_is_omitted_when_empty_and_round_trips_when_present() {
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::PrecombatMain,
        ..Default::default()
    };
    // Empty pool is elided from the wire.
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("mana_pool").is_none());

    // A non-empty pool round-trips as a list of pip strings.
    view.mana_pool = vec!["{G}".into(), "{G}".into()];
    let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back.mana_pool, vec!["{G}".to_string(), "{G}".to_string()]);
}

#[test]
fn issue_627_board_fixture_round_trips_with_every_relationship_a_view_projects() {
    // The cross-language stress fixture for issue #627: one frame carrying every
    // relationship a client has to be able to draw at once — a deep stack, all four
    // target tags, two creatures blocking one attacker, an attack on a planeswalker,
    // an Aura and an Equipment, counters, marked damage, tokens, an emblem, and a
    // public pile long enough to need its own browser.
    //
    // Consumed verbatim by the web client's mirror-parity suite and replayed by its
    // browser view tier, so a field renamed here and not there fails on one side or
    // the other rather than drifting quietly.
    let json = include_str!("../../fixtures/gameview-board.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    let permanent = |id: &str| {
        view.battlefield
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} is on the battlefield"))
    };

    // Restricted mana (CR 106.6) rides the pool as a pip suffixed `*`, beside an
    // ordinary one. The suffix is the whole of what the wire says — that this mana is
    // restricted, never what to — so a client may report the restriction and must not
    // invent its condition.
    assert_eq!(view.mana_pool, vec!["{R}".to_string(), "{G}*".to_string()]);

    // An attack on a planeswalker states both: the seat that answers for it, and the
    // planeswalker the damage is going to. A client that had only one of them would
    // draw the arrow at the wrong object.
    let ogre = permanent("perm_ogre");
    assert!(ogre.attacking);
    assert_eq!(ogre.attacking_player.as_deref(), Some("p2"));
    assert_eq!(ogre.attacking_planeswalker.as_deref(), Some("perm_vivien"));

    // Two blockers on one attacker. Nothing on the attacker says so — the relationship
    // rides on each blocker, which is why a client has to index it from both ends.
    let blockers: Vec<&str> = view
        .battlefield
        .iter()
        .filter(|p| p.blocking.as_deref() == Some("perm_ogre"))
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(blockers, vec!["perm_dreadmaw", "perm_zombie"]);

    // Both kinds of attachment, one of them onto a permanent the attacher's controller
    // does not control.
    assert_eq!(
        permanent("perm_axe").attached_to.as_deref(),
        Some("perm_serra")
    );
    assert_eq!(
        permanent("perm_pacifism").attached_to.as_deref(),
        Some("perm_gearsmith")
    );
    assert_eq!(permanent("perm_pacifism").controller, "p1");
    assert_eq!(permanent("perm_gearsmith").controller, "p2");

    // A deep stack, listed bottom first, carrying every target tag between them plus a
    // single object with more than one target.
    assert_eq!(view.stack.len(), 7);
    assert_eq!(view.stack[0].id, "s1");
    let tags: Vec<&str> = view
        .stack
        .iter()
        .flat_map(|item| &item.targets)
        .map(|target| match target {
            StackTarget::Player { .. } => "player",
            StackTarget::Permanent { .. } => "permanent",
            StackTarget::Card { .. } => "card",
            StackTarget::Stack { .. } => "stack",
        })
        .collect();
    for tag in ["player", "permanent", "card", "stack"] {
        assert!(tags.contains(&tag), "the fixture exercises a {tag} target");
    }
    let two = view
        .stack
        .iter()
        .find(|item| item.targets.len() > 1)
        .unwrap_or_else(|| panic!("one object names more than one target"));
    assert_eq!(two.id, "s7");

    // An ability on the stack names the permanent it came from, which is the only link
    // back to a source that has no card of its own on the stack.
    let trigger = view
        .stack
        .iter()
        .find(|item| item.id == "s3")
        .unwrap_or_else(|| panic!("the Gravedigger trigger is on the stack"));
    assert_eq!(trigger.source.as_deref(), Some("perm_gravedigger"));
    assert!(trigger.card.is_none());

    // A public pile long enough that a panel has to browse it rather than list it, and
    // an opponent whose graveyard the view summarised instead of itemizing — both
    // shapes reach a client and they render differently.
    let mine = view
        .graveyards
        .iter()
        .find(|pile| pile.player_id == "p1")
        .unwrap_or_else(|| panic!("your graveyard is itemized"));
    assert_eq!(mine.cards.len(), 10);
    assert!(!view.graveyards.iter().any(|pile| pile.player_id == "p2"));
    assert_eq!(view.opponents[0].graveyard_size, 3);
}

#[test]
fn issue_650_the_board_fixture_names_the_physical_card_wherever_there_is_one() {
    // The same cross-language fixture, read for the #650 projection: which physical card
    // (CR 108.1) each battlefield and stack object is of. It is carried on the two
    // projections that hold an *object* id and is absent from the two that have no card
    // at all, which together are the whole of the rule.
    let json = include_str!("../../fixtures/gameview-board.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    let permanent = |id: &str| {
        view.battlefield
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} is on the battlefield"))
    };
    let stack = |id: &str| {
        view.stack
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("{id} is on the stack"))
    };

    // A permanent that is a card names it, and the name is a *different* id from its own:
    // CR 400.7 makes the permanent and the card it becomes elsewhere two objects.
    let ogre = permanent("perm_ogre");
    assert_eq!(ogre.physical_card.as_deref(), Some("card_ogre"));
    assert_ne!(ogre.physical_card.as_deref(), Some(ogre.id.as_str()));

    // A token (CR 111) is not a card and names none, from both ends: `token` on its face
    // and an absent `physical_card` say the same thing.
    let thopter = permanent("perm_thopter");
    assert!(thopter.card.token);
    assert_eq!(thopter.physical_card, None);

    // A spell names the card being cast; an ability on the stack has no card to name, and
    // its `source` — a permanent id — is deliberately a different question.
    assert_eq!(stack("s1").physical_card.as_deref(), Some("card_verdict"));
    let trigger = stack("s3");
    assert_eq!(trigger.physical_card, None);
    assert_eq!(trigger.source.as_deref(), Some("perm_gravedigger"));

    // Two copies of one card, told apart. The fixture holds a Lightning Strike in hand and
    // another on the stack: identical name, identical `functional_id`, and distinguishable
    // only by which physical card each is — which is exactly why a client may never join
    // on either of the other two.
    let hand = &view.my_hand[0];
    let cast = stack("s6");
    assert_eq!(hand.name, "Lightning Strike");
    assert_eq!(
        cast.card.as_ref().map(|c| c.functional_id.as_str()),
        Some(hand.functional_id.as_str())
    );
    assert_ne!(cast.physical_card.as_deref(), Some(hand.id.as_str()));
}

#[test]
fn issue_620_emblem_fixture_round_trips_with_its_emblem_and_optional_target_slots() {
    // The cross-language contract fixture for the two shapes issue #620 added: an
    // **emblem** beside the battlefield (CR 114 — public, in no zone, nothing but its
    // abilities), and a target requirement whose slots are **optional**, which is how
    // "up to two target creatures" reaches a client that knows no rules.
    //
    // Consumed verbatim by the web client's mirror-parity suite, so a field renamed here
    // and not there fails on one side or the other rather than drifting quietly.
    let json = include_str!("../../fixtures/gameview-emblem.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    assert_eq!(view.emblems.len(), 1);
    assert_eq!(view.emblems[0].id, "emblem_41");
    assert_eq!(view.emblems[0].controller, "p0");
    assert_eq!(view.emblems[0].abilities.len(), 4);
    // An emblem carries no card, no zone, and no counters — there are no such fields to
    // carry, which is the shape rather than an omission.

    let Some(ability) = view
        .valid_actions
        .iter()
        .find(|action| action.kind == "activate_ability")
    else {
        panic!("the loyalty ability is offered")
    };
    assert_eq!(
        ability.requirements.len(),
        2,
        "up to two targets, two slots"
    );
    assert!(
        ability.requirements.iter().all(|slot| slot.optional),
        "both slots of an 'up to' group may be left empty"
    );

    // The flag elides when false, so an ordinary mandatory slot is byte-for-byte what it
    // was before the field existed.
    let mandatory = serde_json::to_value(TargetRequirement {
        slot: "t0".to_string(),
        prompt: "Choose target creature".to_string(),
        optional: false,
        candidates: vec!["perm_ogre".to_string()],
    })
    .unwrap();
    assert!(mandatory.get("optional").is_none());
}

#[test]
fn canonical_fixture_round_trips_and_matches_typed_fields() {
    // Single-sourced cross-language contract fixture (issue #56): this exact
    // JSON is also consumed by the web client's `wire.test.ts`. A field
    // renamed, retyped, or removed in this crate without updating the fixture
    // fails to deserialize (or mismatches an assertion) here — the same drift
    // the same-PR discipline used to catch by convention alone.
    let json = include_str!("../../fixtures/gameview.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    // Round-trips through serde JSON without loss.
    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    // Load-bearing typed fields: a rename/retype in the structs breaks one of
    // these (or the deserialize above) rather than passing silently.
    assert_eq!(view.you, "p1");
    assert_eq!(view.phase, Phase::PrecombatMain);
    assert_eq!(view.turn, 3);
    assert_eq!(view.active_player, "p1");
    assert_eq!(view.mana_pool, vec!["{G}".to_string(), "{G}".to_string()]);
    assert_eq!(view.priority_player.as_deref(), Some("p1"));
    assert_eq!(view.action_deadline, Some(12.5));

    // Populated hand: creature carries P/T, the land omits them.
    assert_eq!(
        view.my_hand
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        ["c1", "c2", "c3"]
    );
    assert_eq!(view.my_hand[0].power.as_deref(), Some("1"));
    assert_eq!(view.my_hand[1].power, None);

    // Opponent view redacts hidden zones to counts and carries statuses.
    assert_eq!(view.opponents[0].hand_size, 7);
    assert_eq!(view.opponents[0].statuses, vec!["monarch".to_string()]);

    // Battlefield: a tapped permanent with a `+1/+1` counter and a
    // planeswalker with a `loyalty` counter — exercising `Counter {kind, count}`.
    let bear = &view.battlefield[0];
    assert!(bear.tapped);
    assert_eq!(
        bear.counters,
        vec![Counter {
            kind: "+1/+1".into(),
            count: 2,
        }]
    );
    // Marked damage (CR 120.3) is its own channel, separate from both counters and the
    // computed toughness above it — a client that folded it into either would show a
    // creature's remaining toughness as a number the server never sent.
    assert_eq!(bear.damage, 1);
    assert_eq!(view.battlefield[1].counters[0].kind, "loyalty");
    assert_eq!(view.battlefield[1].counters[0].count, 5);
    assert!(!view.battlefield[1].tapped);
    // Printed starting loyalty (CR 306.5b) rides the card face, while *current*
    // loyalty is the counter above. Both are present here, and they are different
    // channels answering different questions — this fixture pins that apart.
    assert_eq!(view.battlefield[1].card.loyalty.as_deref(), Some("5"));
    assert_eq!(view.battlefield[0].card.loyalty, None);

    // A **token** (CR 111, issue #605): a full permanent with computed
    // characteristics and no card identity behind it. `token` is what says so — an
    // empty `functional_id` alone would be indistinguishable from a card the server
    // could not resolve.
    let thopter = &view.battlefield[2].card;
    assert!(thopter.token);
    assert!(thopter.functional_id.is_empty());
    assert!(thopter.mana_cost.is_none());
    assert_eq!(thopter.power.as_deref(), Some("1"));
    assert_eq!(thopter.keywords, vec!["flying".to_string()]);

    // Stack: an ability carries its `source`; a spell does not.
    assert_eq!(view.stack[0].source, None);
    assert_eq!(view.stack[1].source.as_deref(), Some("perm_bear"));

    // Stack structure (issue #550): the kind is server-stated, the card face
    // rides along, and the target list is typed and ordered.
    assert_eq!(view.stack[0].kind, Some(StackItemKind::Spell));
    assert_eq!(
        view.stack[0].card.as_ref().map(|c| c.name.as_str()),
        Some("Lightning Bolt")
    );
    assert_eq!(
        view.stack[0].targets,
        vec![StackTarget::Permanent {
            id: "perm_bear".into()
        }]
    );
    // The terse ability entry keeps the pre-#550 body: an entry with no face and
    // no targets is not an error, and its kind is still stated. `ability` is also
    // the coarse value a pre-#579 server sends, and it must keep deserializing.
    assert_eq!(view.stack[1].kind, Some(StackItemKind::Ability));
    assert_eq!(view.stack[1].card, None);
    assert!(view.stack[1].targets.is_empty());

    // A multi-target spell reconstructs its full relationship set from this one
    // view: two targets, typed differently, in the order the client numbers them.
    assert_eq!(
        view.stack[2].targets,
        vec![
            StackTarget::Permanent {
                id: "perm_nissa".into()
            },
            StackTarget::Player {
                player: "p2".into()
            },
        ]
    );
    // ...and a stack object may itself be a target (CR 701.5).
    assert_eq!(
        view.stack[3].targets,
        vec![StackTarget::Stack { id: "s3".into() }]
    );
    // The finer ability kinds (issue #579): two entries alike in every other
    // field — same source, same description — separated only by their kind, which
    // is exactly why a client may not reconstruct the distinction from prose.
    assert_eq!(view.stack[4].kind, Some(StackItemKind::Activated));
    assert_eq!(view.stack[5].kind, Some(StackItemKind::Triggered));
    assert_eq!(view.stack[4].description, view.stack[5].description);
    assert_eq!(view.stack[4].source, view.stack[5].source);

    // Public piles round-trip populated.
    assert_eq!(view.graveyards[0].cards[0].id, "g1");
    assert_eq!(view.exile[0].cards[0].id, "x1");

    // Every valid-action kind emitted today is represented, in order.
    assert_eq!(
        view.valid_actions
            .iter()
            .map(|a| a.kind.as_str())
            .collect::<Vec<_>>(),
        [
            "pass_priority",
            "play_land",
            "cast_spell",
            "activate_ability"
        ]
    );
    // `pass_priority` is subject-less; the ability action names its permanent.
    assert!(view.valid_actions[0].subject.is_empty());
    assert_eq!(view.valid_actions[3].subject, vec!["perm_bear".to_string()]);
}

#[test]
fn issue_553_commander_fixture_renders_without_a_populated_command_zone() {
    // Cross-language contract fixture (issue #553): a three-seat Commander game
    // **mid-game**, in which every commander has left the command zone — one is on
    // the battlefield, one is in a graveyard, one belongs to an eliminated seat.
    // The frame therefore carries no `command` key at all, which is exactly the
    // state a client used to have to guess at. The web client's `wire.test.ts`
    // consumes these same bytes.
    let json = include_str!("../../fixtures/gameview-commander.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    // Round-trips through serde JSON without loss.
    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    // The format signal is independent of zone contents: no command zone, and the
    // frame still says, authoritatively, that this is Commander.
    assert!(view.command.is_empty(), "every commander is elsewhere");
    let Some(format) = view.format.as_ref() else {
        panic!("the format signal rides the view");
    };
    assert_eq!(format.id, "commander");
    assert!(format.commander);

    // Commander identity is keyed to the designation, so it is present for all
    // three seats regardless of where each commander currently sits — including
    // the eliminated seat, whose commander is in no visible zone at all.
    let identity = |seat: &str| {
        view.commander_identity
            .iter()
            .find(|c| c.commander == seat)
            .unwrap_or_else(|| panic!("seat {seat} has a commander identity"))
    };
    assert_eq!(identity("p1").name, "Lathliss, Dragon Queen");
    assert_eq!(identity("p1").color_identity, vec![Color::Red]);
    assert_eq!(
        identity("p2").color_identity,
        vec![Color::Blue, Color::Black, Color::Red]
    );
    // A colorless commander's identity is empty, which is a value, not a gap.
    assert_eq!(identity("p0").name, "Karn, Silver Golem");
    assert!(identity("p0").color_identity.is_empty());

    // The commander on the battlefield is marked by the server; the ordinary
    // creature beside it is not — the client never infers this from the type line.
    let perm = |id: &str| {
        view.battlefield
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id} is on the battlefield"))
    };
    assert!(perm("perm_lathliss").is_commander);
    assert!(!perm("perm_bear").is_commander);

    // Local elimination while the game continues: the receiver is out, and says so
    // on its own `me` — `result` is still absent because two seats remain.
    assert!(view.me.eliminated);
    assert!(view.result.is_none());
    assert!(!view.opponents[0].eliminated);

    // Per-seat connection and AI state, both public presentation facts.
    assert!(
        !view.opponents[0].connected,
        "p1 is held open, disconnected"
    );
    assert!(!view.opponents[0].ai);
    assert!(view.opponents[1].connected);
    assert!(view.opponents[1].ai, "p2 is a server-side AI seat");
}

#[test]
fn issue_554_action_contract_fixture_round_trips_with_labels_ack_number_and_destinations() {
    // Cross-language contract fixture (issue #554): a mid-turn frame carrying all
    // four halves of the completed action contract at once — a contextual pass
    // label, an acknowledgement correlated to a submission, a numeric prompt, and
    // server-authoritative destinations (including an action that names none).
    // The web client's `wire.test.ts` consumes these same bytes.
    let json = include_str!("../../fixtures/gameview-actions.json");
    let view: GameView = serde_json::from_str(json).unwrap();
    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    let action = |id: &str| {
        view.valid_actions
            .iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("action {id} is offered"))
    };

    // A contextual label: something is on the stack, so passing resolves it, and
    // the server says so. The client renders the string verbatim.
    assert!(!view.stack.is_empty());
    assert_eq!(action("a0").kind, "pass_priority");
    assert_eq!(action("a0").label, "Resolve");

    // Acknowledgement, tied to the exact submission the client sent.
    let Some(ack) = view.action_ack.as_ref() else {
        panic!("this view answers a correlated submission");
    };
    assert_eq!(ack.submission, "s:17");
    assert!(ack.accepted);

    // A numeric prompt with the server's own bounds, alongside a target slot on
    // the same action — one atomic answer fills both.
    let Prompt::Number { slot, min, max, .. } = &action("a2").prompts[0] else {
        panic!("the cast poses a numeric slot");
    };
    assert_eq!(slot, "x");
    assert_eq!((*min, *max), (0, 3));
    assert_eq!(action("a2").requirements[0].slot, "t0");

    // Destinations: a land goes to the battlefield, a spell to the stack…
    assert_eq!(action("a1").destinations[0].kind, "zone");
    assert_eq!(action("a1").destinations[0].id, "battlefield");
    assert_eq!(action("a2").destinations[0].id, "stack");
    // …and the actions that are a click, not a drag, name none — so a client that
    // derives drop regions from this list alone offers no drop target for them.
    assert!(action("a0").destinations.is_empty(), "a pass goes nowhere");
    assert!(
        action("a3").destinations.is_empty(),
        "a mana ability never uses the stack (CR 605.1a)"
    );
}

#[test]
fn issue_554_action_ack_is_absent_on_an_ordinary_broadcast() {
    // The ack's *presence* is the signal, so an ordinary view — and an older
    // server's view — must carry none rather than an empty placeholder.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::Upkeep,
        ..Default::default()
    };
    assert!(serde_json::to_value(&view)
        .unwrap()
        .get("action_ack")
        .is_none());
    let legacy: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert!(legacy.action_ack.is_none());

    // When present it round-trips whole, verdict included.
    view.action_ack = Some(ActionAck {
        submission: "s:4".into(),
        accepted: false,
    });
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json["action_ack"],
        serde_json::json!({ "submission": "s:4", "accepted": false })
    );
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);
}

#[test]
fn issue_553_absent_presentation_metadata_defaults_to_the_pre_553_reading() {
    // The compatibility contract in one place: a payload from a server that
    // predates this shape omits every new field, and each must read as the
    // status quo ante — connected, not eliminated, human, non-Commander, no
    // marker — rather than as its `bool::default()`.
    let legacy: GameView = serde_json::from_str(
        r#"{"you":"p0","phase":"upkeep","me":{"life":20,"library_size":53},
            "opponents":[{"player_id":"p1","hand_size":7,"life":20,
                          "library_size":53,"graveyard_size":0}],
            "battlefield":[{"id":"perm_1","controller":"p1","owner":"p1",
                            "card":{"id":"perm_1","name":"Bear","type_line":"Creature"}}]}"#,
    )
    .unwrap();
    assert!(legacy.format.is_none(), "no format ⇒ not a Commander game");
    assert!(legacy.commander_identity.is_empty());
    assert!(legacy.me.connected, "absent ⇒ connected, not disconnected");
    assert!(!legacy.me.eliminated);
    assert!(!legacy.me.ai);
    assert!(legacy.opponents[0].connected);
    assert!(!legacy.opponents[0].ai);
    assert!(!legacy.battlefield[0].is_commander);

    // A view that omits `me` entirely falls back to `SelfView::default()`, which
    // must agree with the wire default rather than reading as "disconnected".
    let nameless: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert_eq!(nameless.me, SelfView::default());
    assert!(nameless.me.connected);

    // And the defaults all elide again on the way out, so a non-Commander frame
    // serializes exactly as it did before issue #553.
    let round = serde_json::to_value(&nameless).unwrap();
    for absent in ["format", "commander_identity"] {
        assert!(round.get(absent).is_none(), "`{absent}` elides at default");
    }
}

#[test]
fn issue_553_commander_identity_survives_a_zone_change() {
    // The property the `command` pile could never provide: moving a commander out
    // of the command zone changes nothing about the seat's identity. Same view,
    // command zone emptied, identity untouched.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::PrecombatMain,
        command: vec![ZonePile {
            player_id: "p0".into(),
            cards: vec![],
        }],
        format: Some(MatchFormat {
            id: "commander".into(),
            commander: true,
        }),
        commander_identity: vec![CommanderIdentity {
            commander: "p0".into(),
            name: "Lathliss, Dragon Queen".into(),
            color_identity: vec![Color::Red],
        }],
        ..Default::default()
    };
    let before = view.commander_identity.clone();
    // The commander is cast: its pile goes away entirely.
    view.command.clear();
    assert_eq!(view.commander_identity, before);
    assert!(view.format.as_ref().is_some_and(|f| f.commander));
    let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back.commander_identity, before);
}

#[test]
fn unknown_fields_are_ignored() {
    // Forward-compat invariant (docs/protocol.md): a newer server may add
    // fields; older clients must still deserialize the message.
    let json = r#"{ "phase": "draw", "some_future_field": 42 }"#;
    let view: GameView = serde_json::from_str(json).unwrap();
    assert_eq!(view.phase, Phase::Draw);
    assert!(view.my_hand.is_empty());
}

#[test]
fn you_defaults_to_empty_when_absent() {
    // Backward-compat: a payload from an older server omits `you`; it must
    // still deserialize, defaulting the seat id to an empty string rather
    // than failing the whole message.
    let json = r#"{ "phase": "draw" }"#;
    let view: GameView = serde_json::from_str(json).unwrap();
    assert_eq!(view.you, "");
}

#[test]
fn game_view_result_is_omitted_while_live_and_round_trips_when_over() {
    // Empty-optional convention: `result` is absent from the wire while the
    // game is live, and round-trips (winner/losers/reason) once it is over.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::End,
        ..Default::default()
    };
    // Live game: the field elides entirely.
    let json = serde_json::to_value(&view).unwrap();
    assert!(json.get("result").is_none());

    // Game over: winner p0, loser p1, decked. Round-trips losslessly.
    view.result = Some(GameResult {
        winner: Some("p0".into()),
        losers: vec!["p1".into()],
        reason: GameOverReason::Decked,
    });
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json.get("result").unwrap(),
        &serde_json::json!({
            "winner": "p0",
            "losers": ["p1"],
            "reason": "decked"
        })
    );
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);

    // A draw omits the winner but still round-trips.
    view.result = Some(GameResult {
        winner: None,
        losers: vec!["p0".into(), "p1".into()],
        reason: GameOverReason::LifeZero,
    });
    let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);
    assert!(back.result.unwrap().winner.is_none());
}

#[test]
fn game_view_serializes_you_on_the_wire() {
    let view = GameView {
        you: "p1".into(),
        // An emblem (CR 114, issue #620): public, in no zone, and nothing but its
        // abilities — so the round trip carries one to prove the shape survives.
        emblems: vec![Emblem {
            id: "emblem_41".into(),
            controller: "p1".into(),
            abilities: vec!["Creatures you control get +2/+2.".into()],
        }],
        phase: Phase::Upkeep,
        ..Default::default()
    };
    let json = serde_json::to_value(&view).unwrap();
    // The receiver's own seat id is always present on the wire (like `phase`),
    // not elided the way empty collections are.
    assert_eq!(json.get("you"), Some(&serde_json::json!("p1")));
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back.you, "p1");
}

#[test]
fn game_view_player_names_round_trip_and_elide_when_empty() {
    // Issue #294: the per-player name map lets any in-game surface label a player;
    // it round-trips as a JSON object and elides from the wire when empty. An older
    // server that omits it deserializes to an empty map (backward compatibility).
    let mut view = GameView {
        you: "p1".into(),
        phase: Phase::Upkeep,
        turn: 1,
        active_player: "p1".into(),
        ..Default::default()
    };
    // Empty map elides from the wire.
    assert!(serde_json::to_value(&view)
        .unwrap()
        .get("player_names")
        .is_none());

    // Populated: names keyed by player id survive the round trip.
    view.player_names.insert("p1".into(), "Alice".into());
    view.player_names.insert("p2".into(), "Bob".into());
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json.get("player_names"),
        Some(&serde_json::json!({ "p1": "Alice", "p2": "Bob" }))
    );
    let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);

    // A payload from an older server that omits the field defaults to an empty map.
    let legacy: GameView = serde_json::from_str(r#"{"you":"p1","phase":"upkeep"}"#).unwrap();
    assert!(legacy.player_names.is_empty());
}

#[test]
fn issue_265_action_rejected_flag_round_trips_and_elides_when_false() {
    // The rejected-action feedback flag is a transient, per-receiver advisory
    // (like `auto_passed`): it appears on the wire only on the one view answering a
    // rejection, and an older server that never sends it deserializes to `false`.
    let mut view = GameView {
        you: "p1".into(),
        phase: Phase::Upkeep,
        turn: 1,
        active_player: "p1".into(),
        ..Default::default()
    };
    // Not rejected: the field elides from the wire (the common case).
    assert!(serde_json::to_value(&view)
        .unwrap()
        .get("action_rejected")
        .is_none());

    // Rejected: the flag serializes and survives the round trip.
    view.action_rejected = true;
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json.get("action_rejected"), Some(&serde_json::json!(true)));
    let back: GameView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
    assert_eq!(back, view);
    assert!(back.action_rejected);

    // A payload from an older server that omits the field defaults to `false`.
    let legacy: GameView = serde_json::from_str(r#"{"you":"p1","phase":"upkeep"}"#).unwrap();
    assert!(!legacy.action_rejected);
}

#[test]
fn issue_604_revealed_cards_ride_the_view_only_while_something_is_showing_them() {
    // The hidden-zone rendering channel: absent from the wire in every ordinary view,
    // present only while a choice is asking this receiver about cards no one else may
    // see, and structurally impossible on a spectator view.
    let mut view = GameView {
        you: "p0".into(),
        phase: Phase::PrecombatMain,
        turn: 3,
        active_player: "p0".into(),
        ..Default::default()
    };
    assert!(
        serde_json::to_value(&view)
            .unwrap()
            .get("revealed")
            .is_none(),
        "nothing is being revealed, so the field elides",
    );

    view.revealed = vec![CardView {
        id: "card_9".into(),
        name: "Forest".into(),
        type_line: "Basic Land — Forest".into(),
        mana_cost: None,
        rules_text: String::new(),
        functional_id: "forest".into(),
        token: false,
        power: None,
        toughness: None,
        loyalty: None,
        keywords: Vec::new(),
        card_types: Vec::new(),
    }];
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["revealed"][0]["id"], serde_json::json!("card_9"));
    let back: GameView = serde_json::from_value(json).unwrap();
    assert_eq!(back, view);

    // A spectator view has no such field at all — redaction by type, not by care.
    assert!(
        !std::any::type_name::<SpectatorView>().is_empty()
            && serde_json::to_value(SpectatorView {
                players: Vec::new(),
                battlefield: Vec::new(),
                emblems: Vec::new(),
                stack: Vec::new(),
                graveyards: Vec::new(),
                exile: Vec::new(),
                command: Vec::new(),
                phase: Phase::Upkeep,
                turn: 1,
                active_player: "p0".into(),
                seat_order: Vec::new(),
                priority_player: None,
                result: None,
                log: Vec::new(),
                player_names: std::collections::BTreeMap::new(),
                commander_damage: Vec::new(),
                commander_tax: Vec::new(),
                format: None,
                commander_identity: Vec::new(),
            })
            .unwrap()
            .get("revealed")
            .is_none(),
    );

    // A payload from a server that predates the field decodes to nothing revealed.
    let legacy: GameView = serde_json::from_str(r#"{"you":"p0","phase":"upkeep"}"#).unwrap();
    assert!(legacy.revealed.is_empty());
}

#[test]
fn issue_628_turn_flow_fixture_round_trips_with_its_stops_and_settle_path() {
    // The cross-language contract fixture for turn flow: the two stop lists as the server
    // reflects them, a settle path that **crosses a turn boundary**, and a decision clock.
    //
    // Consumed verbatim by the web client's mirror-parity suite and by its browser tier, so a
    // field renamed here and not there fails on one side or the other rather than drifting.
    let json = include_str!("../../fixtures/gameview-turn.json");
    let view: GameView = serde_json::from_str(json).unwrap();

    let reencoded = serde_json::to_string(&view).unwrap();
    let back: GameView = serde_json::from_str(&reencoded).unwrap();
    assert_eq!(back, view);

    // A step is never on both lists: `stops` is the wider claim and wins outright.
    assert_eq!(view.stops, vec![Phase::End]);
    assert_eq!(
        view.own_turn_stops,
        vec![Phase::PrecombatMain, Phase::PostcombatMain]
    );
    assert!(view.stops.iter().all(|p| !view.own_turn_stops.contains(p)));

    // The path is ordered and each entry carries its own turn, because it crosses one: a
    // consumer that inferred the turn from a repeated phase would put turn 3's end step in
    // turn 4. `auto_passed` is exactly the path being non-empty.
    assert!(view.auto_passed);
    let path: Vec<(u32, Phase)> = view
        .auto_passed_steps
        .iter()
        .map(|step| (step.turn, step.phase))
        .collect();
    assert_eq!(
        path,
        vec![(3, Phase::End), (3, Phase::Cleanup), (4, Phase::Untap),]
    );
    assert_eq!(view.turn, 4, "the strip's own turn is the later one");

    // The clock rides only the deciding seat's view, and the concede that ends a match is an
    // ordinary subject-less action — nothing about either is special-cased on the wire.
    assert_eq!(view.action_deadline, Some(24.0));
    let Some(concede) = view
        .valid_actions
        .iter()
        .find(|action| action.kind == "concede")
    else {
        panic!("the acting seat is always offered a concession")
    };
    assert!(concede.subject.is_empty());

    // A disconnected seat is a flag on the seat, not an absence of it: the board still shows
    // them, their totals, and their piles.
    assert!(!view.opponents[0].connected);
}
