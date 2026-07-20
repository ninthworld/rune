//! The personalized in-game [`GameView`] the server pushes after every change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    CardView, CommanderDamage, CommanderTax, GameLogEntry, GameResult, OpponentView, Permanent,
    Phase, PlayerId, SelfView, StackItem, ValidAction, ZonePile,
};

/// The personalized state the server sends after every change (docs/protocol.md).
/// Hidden information is redacted server-side before this is built. A client must
/// be able to fully reconstruct its UI from a single `GameView` — no client state
/// is load-bearing across messages.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameView {
    /// The receiver's own seat entity id (the `p{N}` form used for players
    /// throughout the view). Lets a client identify itself directly instead of
    /// inferring it from which id is not an opponent. `#[serde(default)]` so a
    /// payload from an older server that omits it still deserializes (to `""`).
    #[serde(default)]
    pub you: PlayerId,
    /// Full card objects for the receiving player only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub my_hand: Vec<CardView>,
    /// The receiver's own public stats (life total, library size) — see [`SelfView`].
    /// `#[serde(default)]` so a payload from an older server that omits it still
    /// deserializes (to a zero placeholder).
    #[serde(default)]
    pub me: SelfView,
    /// Redacted views of every other player.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub opponents: Vec<OpponentView>,
    /// All permanents in play.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's graveyard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// Each player's **command zone** (CR 903.6, issue #372): the public pile
    /// holding their commander while it is there. **Public information** — every
    /// seat sees every command zone. One [`ZonePile`] per player that has any card
    /// in their command zone; empty (and omitted from the wire) for a non-commander
    /// game or while every commander is elsewhere. Additive, like [`Self::exile`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The current turn number (1-based; `0` only in an empty/default state). The
    /// server owns turn counting — the client never counts turns itself, it renders
    /// this. `#[serde(default)]` so a payload from an older server that omits it
    /// still deserializes (to `0`).
    #[serde(default)]
    pub turn: u32,
    /// The player whose turn it is (the *active player*), as the `p{N}` id used
    /// throughout the view. Distinct from [`Self::priority_player`]: the active
    /// player owns the turn even while priority sits with an opponent (e.g. during
    /// their response). `#[serde(default)]` so an older payload that omits it
    /// deserializes to `""` (unknown), and it is elided from the wire when empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_player: PlayerId,
    /// The table's seat order: every player's id (`p0`, `p1`, …) in seat order,
    /// including the receiver and any eliminated players (issue #345). The explicit
    /// promise the multiplayer table layout relies on to place opponents in a stable
    /// arrangement around the receiver — opponents were only ever *happened* to be
    /// projected in seat order before, which no client could rely on. Additive:
    /// omitted (and defaults to empty) so a client that ignores it sees no change;
    /// a two-player client can continue to infer the arrangement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_order: Vec<PlayerId>,
    /// The receiving player's unspent mana, as pip strings (e.g. `["{G}", "{G}"]`).
    /// Server-computed; the client only displays it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_pool: Vec<String>,
    /// The player who currently holds priority, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The only source of interactivity: what the receiving player may do now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_actions: Vec<ValidAction>,
    /// Seconds remaining for the pending decision, if a clock is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_deadline: Option<f64>,
    /// The terminal result once the game is over (winner/losers/reason, CR 104.2a).
    /// Omitted while the game is live (the empty-optional convention), so its
    /// presence alone tells a client the game has ended; when present,
    /// `valid_actions` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    /// A bounded, sequence-numbered window of structured public game history.
    /// It is carried in every full view so reconnecting clients need no accumulated
    /// local log state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<GameLogEntry>,
    /// The receiver's own current **priority-stop preferences** (issue #264): the
    /// steps at which they want to receive priority even when the engine reports
    /// they have no meaningful action, so basic auto-pass (ADR 0020) does not skip
    /// them there. Carried on the view so the per-phase stops UI is reconstructable
    /// from a single message and survives reconnect (the preferences live on the
    /// room, like `player_names`, not in client memory). Per-viewer, not secret;
    /// the client renders toggles from this and answers with the `set_stops`
    /// message. Omitted from the wire when empty (stop nowhere — the default); a
    /// client treats a missing field as "no stops".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<Phase>,
    /// Whether reaching this state **auto-passed** priority on the receiver's behalf
    /// (issue #264, ADR 0020): set on the broadcast that follows a settle in which
    /// the room passed priority for this seat, so the client can show a display-only
    /// "passed for you" indicator. Advisory and transient — the UI reconstructs
    /// fully without it, and a reconnect re-send need not preserve it. Omitted from
    /// the wire when `false`.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub auto_passed: bool,
    /// Whether this view was pushed **because the receiver's last in-game action was
    /// rejected** (issue #265): a stale-view race meant the chosen action was no longer
    /// on offer (unknown id, mismatched [`ValidAction::token`], or a now-illegal target),
    /// so the server re-sent the current state unchanged rather than mutating the game.
    /// Purely advisory and transient — like [`Self::auto_passed`], the UI reconstructs
    /// fully without it and a reconnect re-send need not preserve it — so a client shows a
    /// brief, non-blaming "the game moved on" notice and nothing more. It is never load
    /// bearing: `valid_actions` already reflects the true current legal set. Set only on
    /// the one re-send that answers a rejection; omitted from the wire (treated as `false`)
    /// on every other broadcast.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub action_rejected: bool,
    /// Public display names, keyed by [`PlayerId`] (issue #294): every player who has
    /// chosen a name maps to it, so any in-game surface — the turn indicator, player
    /// tiles, zone-browser titles, the game-over verdict — can label any player
    /// (`you`, an opponent, the active/priority player, a winner) without a lobby
    /// round-trip. Names are public information (no redaction beyond validation), the
    /// display name never replaces the `p{N}` id an action echoes back, and a player
    /// with no name simply has no entry here. Omitted from the wire when empty; a
    /// client treats a missing key as "unnamed" and falls back to a seat-derived
    /// label, so an older server that never sends names keeps working.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub player_names: BTreeMap<PlayerId, String>,
    /// Cumulative **combat** damage each commander has dealt each player this game
    /// (CR 903.10a, issue #371), one entry per `(commander, damaged)` pair that has
    /// taken any — see [`CommanderDamage`]. **Public information**, so it is the
    /// same for every receiver. A player who has taken 21+ from one commander has
    /// lost (that shows in [`Self::result`] with
    /// [`GameOverReason::CommanderDamage`]); the running tally lets a client warn
    /// before then. Additive: omitted (and defaults to empty) so a non-commander
    /// game — and an older client — is unchanged. Server-computed; never derived by
    /// the client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_damage: Vec<CommanderDamage>,
    /// The **commander tax** owed on each designated commander (CR 903.8, issue
    /// #372), one entry per player with a commander — see [`CommanderTax`]. **Public
    /// information**: the tax is `{2}` per prior cast from the command zone, so every
    /// seat sees how much a recast costs. Additive: omitted (and defaults to empty)
    /// for a non-commander game. Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_tax: Vec<CommanderTax>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use std::collections::BTreeMap;

    use crate::*;

    #[test]
    fn issue_264_game_view_stops_and_auto_passed_round_trip_and_elide() {
        // `stops` and `auto_passed` ride the view; both elide from the wire at their
        // defaults (empty / false) and round-trip when present.
        let mut view = GameView {
            you: "p0".into(),
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::Upkeep,
            turn: 1,
            active_player: "p0".into(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: Some("p0".into()),
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: vec![],
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
            tapped: false,
            attacking: true,
            attacking_player: Some("p2".into()),
            blocking: None,
            damage: 0,
            attached_to: None,
            counters: vec![],
        };
        let json = serde_json::to_value(&attacker).unwrap();
        assert_eq!(json["attacking_player"], serde_json::json!("p2"));
        assert_eq!(serde_json::from_value::<Permanent>(json).unwrap(), attacker);

        // A not-attacking permanent omits `attacking_player`.
        let idle = Permanent {
            attacking: false,
            attacking_player: None,
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
            my_hand: vec![CardView {
                id: "c1".into(),
                name: "Llanowar Elves".into(),
                type_line: "Creature — Elf Druid".into(),
                mana_cost: Some("{G}".into()),
                rules_text: "{T}: Add {G}.".into(),
                functional_id: "llanowar_elves".into(),
                power: Some("1".into()),
                toughness: Some("1".into()),
                keywords: vec![],
            }],
            me: SelfView {
                life: 18,
                library_size: 52,
            },
            opponents: vec![OpponentView {
                player_id: "p2".into(),
                hand_size: 7,
                life: 20,
                library_size: 53,
                graveyard_size: 0,
                statuses: vec!["monarch".into()],
                eliminated: false,
            }],
            battlefield: vec![Permanent {
                id: "perm_xyz".into(),
                controller: "p1".into(),
                owner: "p1".into(),
                card: CardView {
                    id: "perm_xyz".into(),
                    name: "Grizzly Bears".into(),
                    type_line: "Creature — Bear".into(),
                    mana_cost: Some("{1}{G}".into()),
                    rules_text: String::new(),
                    functional_id: String::new(),
                    power: Some("2".into()),
                    toughness: Some("2".into()),
                    keywords: vec!["flying".into()],
                },
                tapped: true,
                attacking: false,
                attacking_player: None,
                blocking: None,
                damage: 0,
                attached_to: None,
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
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            // Commander combat damage (issue #371): a public per-commander tally.
            commander_damage: vec![CommanderDamage {
                commander: "p2".into(),
                damaged: "p1".into(),
                amount: 14,
            }],
            commander_tax: Vec::new(),
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
        let view: GameView =
            serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
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
                name: "Jedit Ojanen".into(),
                type_line: "Legendary Creature — Cat Warrior".into(),
                mana_cost: Some("{4}{G}{G}".into()),
                rules_text: String::new(),
                functional_id: "jedit_ojanen".into(),
                power: Some("5".into()),
                toughness: Some("5".into()),
                keywords: vec![],
            }],
        }];
        let json = serde_json::to_value(&view).unwrap();
        // The populated zone rides the wire under `command`, one pile per player.
        assert_eq!(json["command"][0]["player_id"], "p0");
        assert_eq!(
            json["command"][0]["cards"][0]["functional_id"],
            "jedit_ojanen"
        );
        let back: GameView = serde_json::from_value(json).unwrap();
        assert_eq!(back.command, view.command);
        assert_eq!(back, view);
    }

    #[test]
    fn empty_game_view_round_trips() {
        let view = GameView {
            you: "p0".into(),
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::Upkeep,
            turn: 0,
            active_player: String::new(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn mana_pool_is_omitted_when_empty_and_round_trips_when_present() {
        let mut view = GameView {
            you: "p0".into(),
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::PrecombatMain,
            turn: 0,
            active_player: String::new(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
    fn canonical_fixture_round_trips_and_matches_typed_fields() {
        // Single-sourced cross-language contract fixture (issue #56): this exact
        // JSON is also consumed by the web client's `wire.test.ts`. A field
        // renamed, retyped, or removed in this crate without updating the fixture
        // fails to deserialize (or mismatches an assertion) here — the same drift
        // the same-PR discipline used to catch by convention alone.
        let json = include_str!("../fixtures/gameview.json");
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
        assert_eq!(view.battlefield[1].counters[0].kind, "loyalty");
        assert_eq!(view.battlefield[1].counters[0].count, 5);
        assert!(!view.battlefield[1].tapped);

        // Stack: an ability carries its `source`; a spell does not.
        assert_eq!(view.stack[0].source, None);
        assert_eq!(view.stack[1].source.as_deref(), Some("perm_bear"));

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
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::End,
            turn: 0,
            active_player: String::new(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::Upkeep,
            turn: 0,
            active_player: String::new(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::Upkeep,
            turn: 1,
            active_player: "p1".into(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
            my_hand: vec![],
            me: SelfView::default(),
            opponents: vec![],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            command: vec![],
            phase: Phase::Upkeep,
            turn: 1,
            active_player: "p1".into(),
            seat_order: Vec::new(),
            mana_pool: vec![],
            priority_player: None,
            valid_actions: vec![],
            action_deadline: None,
            result: None,
            log: vec![],
            stops: Vec::new(),
            auto_passed: false,
            action_rejected: false,
            player_names: BTreeMap::new(),
            commander_damage: Vec::new(),
            commander_tax: Vec::new(),
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
}
