//! In-game card, board, and zone views — the public and self-visible pieces a
//! [`GameView`](crate::GameView) is assembled from.

use serde::{Deserialize, Serialize};

use crate::{EntityId, PlayerId};

/// A card object, shown only to a player entitled to see it (`my_hand`, public
/// zones, revealed cards). Characteristics are server-computed; the client never
/// derives them. Grows alongside the card database (backlog: engine card loader).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardView {
    /// Entity id of this card instance.
    pub id: EntityId,
    /// Display name.
    pub name: String,
    /// e.g. `"Creature — Elf Warrior"`.
    pub type_line: String,
    /// Displayed mana cost string, e.g. `"{1}{G}"`. `None` for cards without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// The card's rules text, **generated** by the server from the card's ability IR
    /// (ADR 0018 §7) — never a stored or upstream string, and never exact Oracle text.
    /// Written to be semantically complete for play; matching official wording is not
    /// a goal. Empty (and omitted from the wire) for a card with no rules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    /// The card's stable **presentation identity**: the `functional_id` of the card
    /// definition this object is a copy of (ADR 0018 §3, §8).
    ///
    /// Unlike [`CardView::id`], which is a per-game entity handle, this is the same
    /// string for every copy of a card in every game, and it survives a server rebuild.
    /// It exists so a future *client-local* cache can look a card up by identity to
    /// enrich its presentation; the server neither has nor requires such a cache, and a
    /// client that ignores this field renders the card completely from `rules_text`.
    /// Empty only for a card the server cannot resolve (a defensive placeholder).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub functional_id: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip).
    /// Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CardView::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// The card's keyword abilities as lowercase wire names (e.g. `"flying"`,
    /// `"first_strike"`), server-computed for display; the client renders badges
    /// and never derives them. Omitted from the wire when the card has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// What the receiving player is allowed to know about an opponent: hidden zones
/// are reduced to counts, public state is exact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpponentView {
    /// Which opponent this describes.
    pub player_id: PlayerId,
    /// Number of cards in hand (contents hidden).
    pub hand_size: u32,
    /// Current life total.
    pub life: i32,
    /// Number of cards left in library.
    pub library_size: u32,
    /// Number of cards in the graveyard.
    pub graveyard_size: u32,
    /// Free-form status labels (e.g. `"monarch"`, `"hexproof"`) for display only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    /// Whether this opponent has been eliminated — they lost while the game
    /// continued and left it (CR 800.4a, issue #342/#345). Additive: omitted (and
    /// defaults to `false`) so a two-player view is unchanged; the client shows an
    /// eliminated opponent as out of the game. Server-computed from the player's
    /// stored loss state; never derived by the client.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub eliminated: bool,
    /// Whether this seat currently has a live connection (issue #553). The server
    /// **holds a disconnected seat open** — the game does not end, the seat is not
    /// conceded, and play simply waits on whoever must act — so a client needs to
    /// say "waiting on a disconnected player" rather than "not responding".
    ///
    /// The one field on this type whose **absent value is `true`**: a payload that
    /// omits it (an older server, or any connected seat) means *connected*, so it
    /// rides the wire only as `false`. Room state, not engine state.
    #[serde(
        default = "crate::default_true",
        skip_serializing_if = "crate::is_true"
    )]
    pub connected: bool,
    /// Whether this seat is played by a server-side **AI** (issue #415/#553) rather
    /// than a human. Public presentation information — the lobby already shows the
    /// AI kind's name on the seat before the game, and it does not stop being true
    /// once the game starts — so carrying it in-game lets a client mark the seat
    /// instead of losing the fact at the hand-off. Additive: omitted (and defaults
    /// to `false`, i.e. human). Room state, not engine state; nothing about the AI's
    /// decisions or its policy is exposed.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub ai: bool,
}

/// The receiver's own public stats — the self-counterpart of [`OpponentView`].
///
/// A player is entitled to see their own public state, but [`GameView`] historically
/// carried none of it: hand *contents* ride in `my_hand` and unspent mana in
/// `mana_pool`, yet the two public numbers every opponent already sees about this
/// player — life total and library size — had no home, so a player could see everyone's
/// life but their own. This is that home; it exposes no hidden information (a player's
/// own life and library size are public).
///
/// [`Default`] is written by hand rather than derived because
/// [`connected`](Self::connected) defaults to `true`: a [`GameView`] from an older
/// server omits `me` entirely and falls back to `SelfView::default()`, which must
/// agree with what deserializing `{}` produces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfView {
    /// The receiver's current life total.
    pub life: i32,
    /// Number of cards left in the receiver's library.
    pub library_size: u32,
    /// Whether the receiver has been eliminated — they lost while two or more
    /// players remained, so the game continues without them (CR 800.4a, issue
    /// #553). The self-counterpart of [`OpponentView::eliminated`], and the only
    /// authoritative source for a *local* elimination: `result` arrives at game
    /// over, which in a multiplayer game can be many turns later, and the bounded
    /// `log` window is not reconstructable, so neither may stand in for this.
    /// Additive: omitted (and defaults to `false`) so a two-player view is
    /// unchanged. Server-computed; the client never infers its own loss.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub eliminated: bool,
    /// Whether the receiver's own seat has a live connection (issue #553) — see
    /// [`OpponentView::connected`]. Trivially `true` for the connection reading it,
    /// so it exists for symmetry: a surface that renders a seat cluster reads the
    /// same field for every seat rather than special-casing itself. Absent ⇒
    /// connected; rides the wire only as `false`.
    #[serde(
        default = "crate::default_true",
        skip_serializing_if = "crate::is_true"
    )]
    pub connected: bool,
    /// Whether the receiver's own seat is AI-controlled (issue #553) — see
    /// [`OpponentView::ai`]. True only for the server's in-process AI driver, which
    /// receives the same `GameView` a human would; present for the same symmetry
    /// reason as [`Self::connected`]. Omitted (and defaults to `false`).
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub ai: bool,
}

impl Default for SelfView {
    /// A zero placeholder for a view that carries no `me` — life `0`, empty
    /// library, still in the game, **connected**, human.
    fn default() -> Self {
        Self {
            life: 0,
            library_size: 0,
            eliminated: false,
            connected: true,
            ai: false,
        }
    }
}

/// A permanent on the battlefield with its server-computed characteristics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permanent {
    /// Entity id of this permanent.
    pub id: EntityId,
    /// Player who currently controls it.
    pub controller: PlayerId,
    /// Player who owns it (matters when control changes).
    pub owner: PlayerId,
    /// The permanent's current (computed) card face.
    pub card: CardView,
    /// Whether the permanent is tapped.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub tapped: bool,
    /// Whether this permanent is currently attacking — declared as an attacker
    /// this combat (CR 508). Server-computed; the client displays it and never
    /// derives it. Omitted from the wire when `false`.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub attacking: bool,
    /// The defending player this permanent is attacking (CR 508.1a), as their
    /// entity id — the multiplayer generalization of [`Self::attacking`] (issue
    /// #341/#345). Additive: omitted (and defaults to `None`) when the permanent is
    /// not attacking, and in a two-player game a client may ignore it since the sole
    /// opponent is the only possible defender; with more seats it names *whom* the
    /// attacker attacks so the board can render split attacks. Follows the
    /// `blocking`/`attached_to` precedent of projecting one object's reference to
    /// another. Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacking_player: Option<EntityId>,
    /// The permanent this one is blocking, if it was declared as a blocker this
    /// combat (CR 509): the attacker's entity id. `None`/omitted when it is not
    /// blocking. Several blockers may name the same attacker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocking: Option<EntityId>,
    /// Damage marked on this permanent this turn (CR 120.3), the value the
    /// lethal-damage state-based action compares against toughness (CR 704.5g).
    /// Server-computed; the client displays it and never derives it. Cleared at
    /// cleanup (CR 514.2). `0`/omitted when no damage is marked.
    #[serde(default, skip_serializing_if = "crate::is_zero")]
    pub damage: u32,
    /// The permanent this one is attached to, if any (CR 303.4): an Aura on the
    /// battlefield names the object it enchants, as that host's entity id — the
    /// same `PermanentId`→`EntityId` projection [`blocking`](Self::blocking) uses.
    /// `None`/omitted for an unattached permanent. Server-computed; the client
    /// clusters the attachment with its host and derives no rules from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<EntityId>,
    /// Whether this permanent **is** its controller's commander (CR 903.3, issue
    /// #553): the server-computed marker that says "this object is the commander",
    /// so a client can put the commander crown on the card without inferring it.
    ///
    /// The inference it replaces was never sound. A commander on the battlefield is
    /// an ordinary permanent: its name is not distinguished, its zone is shared with
    /// every other permanent, and "legendary creature" is neither necessary
    /// (a commander may be a planeswalker) nor sufficient (most legends are not
    /// commanders). Only the server holds the designation, which is keyed to the
    /// card *instance* and so survives every zone change and recast.
    ///
    /// Public information (a commander is announced before the game). Additive:
    /// omitted (and defaults to `false`) for every non-commander permanent and in
    /// every non-Commander game.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub is_commander: bool,
    /// Named counters and their quantities, e.g. `{"+1/+1": 2}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counters: Vec<Counter>,
}

/// A named counter on a permanent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counter {
    /// Counter name, e.g. `"+1/+1"` or `"loyalty"`.
    pub kind: String,
    /// How many of this counter are present.
    pub count: u32,
}

/// What an object on the stack **is** (issue #550): the server-stated
/// discriminator a client renders from, instead of inferring a kind from which
/// other fields happen to be present.
///
/// Deliberately only as fine-grained as the engine can prove, and it **widens
/// additively** as the engine proves more. Issue #579 landed the first widening:
/// the engine now records an ability's provenance (`AbilityOrigin` on its stack
/// object), so a server states
/// [`Activated`](Self::Activated) or [`Triggered`](Self::Triggered) where it
/// previously could only say [`Ability`](Self::Ability). A `copy` value arrives with
/// a copy mechanic (gap G3).
///
/// Two compatibility rules follow, and both are load-bearing:
///
/// - **A server that states only `ability` stays valid.** [`Ability`](Self::Ability)
///   is the coarse value — what a server predating #579 sends, and what any server
///   sends for an ability whose provenance it cannot prove. It never stops
///   deserializing and never means "neither activated nor triggered".
/// - **A client that knows only `ability` renders generically.** Treat an
///   unrecognized value as *unclassified* and fall back to
///   [`StackItem::description`] — never coerce it into a known variant, and never
///   invent the activated/triggered distinction from prose or timing. That
///   reconstruction is rules interpretation, which ADR 0002 puts on the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StackItemKind {
    /// A spell: a card cast onto the stack (CR 601). Its [`StackItem::card`] is the
    /// card being cast.
    Spell,
    /// An ability on the stack (CR 113.3) whose provenance the server does not
    /// state — the coarse value a pre-#579 server sends. Its [`StackItem::source`]
    /// names the permanent the ability came from.
    Ability,
    /// An **activated** ability (CR 602.2): a player chose it and paid its costs.
    /// Carries the same [`StackItem::source`] and face as [`Ability`](Self::Ability).
    Activated,
    /// A **triggered** ability (CR 603.3): a condition was met and the game put it on
    /// the stack — no player activated it. This is the value §2.3's trigger caret
    /// reads. Carries the same [`StackItem::source`] and face as
    /// [`Ability`](Self::Ability).
    Triggered,
}

/// One target chosen for an object on the stack (CR 601.2c), **typed at the source**
/// (issue #550, gaps G1/G6).
///
/// The variant states what kind of thing is targeted, so a client never classifies a
/// target by testing which collection its id appears in — that classification is
/// rules interpretation and belongs to the server (ADR 0002). Serialized internally
/// tagged as `{"kind": "...", ...}`; the id field is named per variant so a `player`
/// carries a [`PlayerId`] (the same id `controller`, `seat_order`, and
/// `player_names` are keyed by) while every other variant carries an [`EntityId`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StackTarget {
    /// A player (CR 115.1).
    Player {
        /// The targeted player's id — the seat key, not an entity id.
        player: PlayerId,
    },
    /// A permanent on the battlefield.
    Permanent {
        /// The targeted permanent's entity id, as it appears in `battlefield[].id`.
        id: EntityId,
    },
    /// A physical card in a zone other than the battlefield or the stack (e.g. a
    /// card in a graveyard).
    Card {
        /// The targeted card's entity id, as it appears in a [`ZonePile`].
        id: EntityId,
    },
    /// Another object on the stack — what a counterspell names (CR 701.5).
    Stack {
        /// The targeted stack object's entity id, as it appears in `stack[].id`.
        id: EntityId,
    },
}

/// One object on the stack — a spell or an ability. Ability entries carry their
/// source permanent so the client can point back at it.
///
/// [`description`](Self::description) stays the authoritative human-readable text;
/// [`kind`](Self::kind), [`targets`](Self::targets), and [`card`](Self::card) are
/// additive structure for *presentation geometry* (issue #550), never a replacement
/// a client must reassemble prose from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackItem {
    /// Entity id of this stack object.
    pub id: EntityId,
    /// Player who controls it (chooses targets/resolution).
    pub controller: PlayerId,
    /// Spell name or ability text as it should be displayed.
    pub description: String,
    /// Source permanent for an ability; `None` for a spell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EntityId>,
    /// What this object is (issue #550), as finely as the server can prove it —
    /// including whether an ability was activated or triggered (issue #579).
    /// Server-stated; a client never derives it from the presence of
    /// [`source`](Self::source). `None` only when the payload came from a server
    /// predating the field — an unclassified entry, which a client renders
    /// generically rather than guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<StackItemKind>,
    /// The targets chosen for this object, **in the order the object's own effects
    /// consume them** (CR 601.2c) — the ordering channel the client's target
    /// numerals (①②③) come from. Empty (and omitted) for a targetless object; an
    /// object that targets nothing is not an error.
    ///
    /// This is the list **as it currently stands**: targets are locked in on
    /// announcement and a target that has since become illegal stays named here
    /// until the object resolves or fizzles (CR 608.2b), so a client that
    /// reconnects mid-resolution rebuilds exactly the relationships the game holds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<StackTarget>,
    /// The card face to render for this entry (issue #550): for a
    /// [`StackItemKind::Spell`] the card being cast; for an ability of any kind the
    /// **current** face of its source permanent, so the entry can show a source
    /// thumbnail without a battlefield lookup.
    ///
    /// `None` when there is no face to show — notably an ability whose source has
    /// already left the battlefield (CR 608.2), which is the same degradation
    /// [`description`](Self::description) makes when it can no longer name its
    /// source. Public information only: an object on the stack is visible to every
    /// player, so this leaks nothing a spectator may not see.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<CardView>,
}

/// A public, ordered pile owned by one player (graveyard or exile).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZonePile {
    /// Player who owns the pile.
    pub player_id: PlayerId,
    /// Cards in zone order (top last).
    pub cards: Vec<CardView>,
}

/// The current turn step. The full sequence lives in the engine's phase FSM
/// (backlog); the protocol carries the current step for overview/focus rendering.
///
/// [`Default`] is the untap step — the first step of a turn, and the zero value
/// [`GameView::default()`](crate::GameView) needs. It is a *placeholder*, not a
/// claim about any real game: `phase` is the one mandatory field on the wire, so a
/// defaulted phase never reaches a client.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Untap step.
    #[default]
    Untap,
    /// Upkeep step.
    Upkeep,
    /// Draw step.
    Draw,
    /// Precombat main phase.
    PrecombatMain,
    /// Beginning of combat step.
    BeginCombat,
    /// Declare attackers step.
    DeclareAttackers,
    /// Declare blockers step.
    DeclareBlockers,
    /// Combat damage step.
    CombatDamage,
    /// End of combat step.
    EndCombat,
    /// Postcombat main phase.
    PostcombatMain,
    /// End step.
    End,
    /// Cleanup step.
    Cleanup,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
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
        let view: GameView =
            serde_json::from_str(r#"{"you":"p0","phase":"precombat_main"}"#).unwrap();
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
            card: CardView {
                id: "perm_1".into(),
                name: "Grizzly Bears".into(),
                type_line: "Creature — Bear".into(),
                mana_cost: Some("{1}{G}".into()),
                rules_text: String::new(),
                functional_id: String::new(),
                power: Some("2".into()),
                toughness: Some("2".into()),
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            is_commander: false,
            counters: vec![],
        };

        // Not in combat and undamaged: all three fields elide from the JSON.
        let json = serde_json::to_value(&base).unwrap();
        assert!(json.get("attacking").is_none());
        assert!(json.get("blocking").is_none());
        assert!(json.get("damage").is_none());

        // An attacker and its blocker both round-trip with their state present.
        let attacker = Permanent {
            attacking: true,
            attacking_player: None,
            ..base.clone()
        };
        let blocker = Permanent {
            blocking: Some("perm_1".into()),
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
            Some(&serde_json::json!("perm_1"))
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
            card: CardView {
                id: "perm_1".into(),
                name: "Ironbark Aegis".into(),
                type_line: "Enchantment — Aura".into(),
                mana_cost: Some("{1}{G}".into()),
                rules_text: "Enchant creature".into(),
                functional_id: String::new(),
                power: None,
                toughness: None,
                keywords: vec![],
            },
            tapped: false,
            attacking: false,
            attacking_player: None,
            blocking: None,
            damage: 0,
            attached_to: None,
            is_commander: false,
            counters: vec![],
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
            power: Some("3".into()),
            toughness: Some("2".into()),
            keywords: vec!["flying".into()],
        };
        let json = serde_json::to_value(&base).unwrap();
        assert_eq!(json.get("keywords"), Some(&serde_json::json!(["flying"])));
        assert_eq!(serde_json::from_value::<CardView>(json).unwrap(), base);

        // A card with no keywords omits the field entirely.
        let vanilla = CardView {
            keywords: vec![],
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
        let older =
            r#"{"id":"s2","controller":"p1","description":"Add {G}.","source":"perm_bear"}"#;
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
                power: None,
                toughness: None,
                keywords: vec![],
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
}
