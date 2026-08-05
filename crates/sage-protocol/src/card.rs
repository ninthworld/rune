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
    /// (ADR 0008 §7) — never a stored or upstream string, and never exact Oracle text.
    /// Written to be semantically complete for play; matching official wording is not
    /// a goal. Empty (and omitted from the wire) for a card with no rules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    /// The card's stable **presentation identity**: the `functional_id` of the card
    /// definition this object is a copy of (ADR 0008 §3, §8).
    ///
    /// Unlike [`CardView::id`], which is a per-game entity handle, this is the same
    /// string for every copy of a card in every game, and it survives a server rebuild.
    /// It exists so a future *client-local* cache can look a card up by identity to
    /// enrich its presentation; the server neither has nor requires such a cache, and a
    /// client that ignores this field renders the card completely from `rules_text`.
    /// Empty only for a card the server cannot resolve (a defensive placeholder).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub functional_id: String,
    /// Whether this object is a **token** (CR 111) rather than a card — a permanent
    /// the game created, with no card behind it.
    ///
    /// The client needs this told to it, not inferred. An absent
    /// [`functional_id`](CardView::functional_id) is the *symptom* a token shares with
    /// a card the server could not resolve, and the two want opposite treatment: a
    /// token is a real object rendered normally and simply has no card identity to
    /// cache or to look presentation up by, while an unresolvable card is a fault.
    /// Additive: omitted (and defaults to `false`) so every existing view is unchanged
    /// on the wire.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub token: bool,
    /// Displayed power (a string so `*` and other non-numeric values round-trip).
    /// Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CardView::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// Displayed **printed starting loyalty** (CR 306.5b), as a string for the same
    /// reason [`CardView::power`] is one. Present only for planeswalkers.
    ///
    /// This is what the card enters the battlefield with, the number in its corner —
    /// **not** how much loyalty a planeswalker on the battlefield has right now, which
    /// is its `loyalty` entry in [`Permanent::counters`]. A client renders this on a
    /// card in hand, on the stack, or in a graveyard, and renders the counter on the
    /// battlefield; showing this one on a battlefield planeswalker would say "4" about
    /// a planeswalker sitting at 1. Additive: omitted (and defaults to `None`) for
    /// every non-planeswalker, so every existing view is unchanged on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<String>,
    /// The card's keyword abilities as lowercase wire names (e.g. `"flying"`,
    /// `"first_strike"`), server-computed for display; the client renders badges
    /// and never derives them. Omitted from the wire when the card has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// The card's types (CR 300), as the structured set
    /// [`type_line`](CardView::type_line) is rendered from.
    ///
    /// The type line is a *sentence* — `"Artifact Creature — Thopter"` — and a client
    /// that needed to know a permanent is a creature could only get there by parsing
    /// it. Parsing is exactly the derivation the client is not allowed to make: it
    /// fails on the cards where the answer matters (an animated land, a permanent
    /// whose types an effect changed) and it makes every consumer re-implement a
    /// grammar. So the set the server already holds is stated, and the sentence stays
    /// what it always was — the thing to *print*.
    ///
    /// Both are projected from the same source, so they can never disagree. Subtypes
    /// are deliberately not here: they are an open set of thousands, they belong to
    /// the sentence, and nothing presentational keys off them.
    ///
    /// Additive: omitted (and defaults to empty) for a card the server could not
    /// resolve, so every existing view is unchanged on the wire and an empty list is
    /// "not stated" rather than "no types".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub card_types: Vec<CardType>,
    /// The card's **colour identity** (CR 903.4): its colours, the colours of the mana
    /// symbols in its cost, and the colours of the mana symbols in its rules text.
    ///
    /// Stated because the one thing a client can read for itself — the printed cost —
    /// is silent on exactly the cards a board is scanned by colour most: a Forest costs
    /// nothing and prints no coloured pip, and rendering it as colourless makes a
    /// mana base unreadable. Colour identity is the answer the server already computes
    /// for deck legality and for a seat's commander gems, so it is the same computation
    /// rather than a second one that could disagree with it.
    ///
    /// It is **not** the card's colour (CR 105) and must not be rendered as one: this
    /// is what a card *belongs to*, which is the question a player asks when scanning a
    /// battlefield. In WUBRG order, deduplicated.
    ///
    /// Additive: omitted (and defaults to empty) for a colourless card and for a card
    /// the server could not resolve, so every existing view is unchanged on the wire.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub color_identity: Vec<crate::Color>,
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
    /// The receiver's maximum hand size (CR 402.2) — how many cards they may still be
    /// holding when the cleanup step ends, or [`MaximumHandSize::Unlimited`] if they
    /// have none (issue #745).
    ///
    /// Stated because the cleanup discard is the one turn-based action a player is asked
    /// to perform on their own hand, and a client that assumed the default seven would
    /// tell a player holding nine cards that they are about to discard two when a land
    /// on the battlefield says otherwise. Server-computed, like every other number here.
    ///
    /// Additive: omitted, it defaults to seven — which is not a guess but exactly what
    /// every game a server predating this field could run actually used.
    #[serde(default, skip_serializing_if = "MaximumHandSize::is_default")]
    pub maximum_hand_size: MaximumHandSize,
}

/// How many cards a player may hold at the end of their turn (CR 402.2).
///
/// A two-state type rather than a number, because "no maximum" is a different state from
/// a large one: any sentinel would be a number nobody printed, and every reader — the
/// discard gate, the client's hand — would have to know which number meant "none" and
/// get it right. Here the compiler asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaximumHandSize {
    /// This many cards, the ordinary rule.
    Cards(u32),
    /// No maximum at all — the cleanup discard never applies.
    Unlimited,
}

impl MaximumHandSize {
    /// Whether this is the default seven, and so may be left off the wire.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for MaximumHandSize {
    /// Seven (CR 402.2) — the rule in force whenever nothing changes it, and therefore
    /// the only honest reading of a payload that does not mention it.
    fn default() -> Self {
        Self::Cards(7)
    }
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
            maximum_hand_size: MaximumHandSize::default(),
        }
    }
}

/// A permanent on the battlefield with its server-computed characteristics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permanent {
    /// Entity id of this permanent.
    pub id: EntityId,
    /// Player who controls it **right now**, after the CR 613 layer-2 control change —
    /// the row a client draws it in. Stated, never inferred.
    pub controller: PlayerId,
    /// Player who owns it, and whose graveyard, hand, or library it goes to when it
    /// leaves the battlefield (CR 400.7). Equal to [`Self::controller`] on almost every
    /// board; the two differ exactly while a control-changing effect is in force, which
    /// is how a client tells a borrowed permanent from an owned one.
    pub owner: PlayerId,
    /// The permanent's current (computed) card face.
    pub card: CardView,
    /// The **physical card** (CR 108.1) this permanent is a projection of, as the same
    /// entity id that card carries in [`CardView::id`] wherever a view shows it —
    /// in a hand, on the stack, in a graveyard, in exile (issue #650).
    ///
    /// **It is not object identity, and a client must never read it as one.** CR 400.7:
    /// *"An object that moves from one zone to another becomes a new object with no
    /// memory of, or relation to, its previous existence."* The permanent that died and
    /// the card now in the graveyard are two different objects with two different ids,
    /// correctly so. This field says only that both are projections of one physical
    /// card — the thing a player's eye follows across the table. Nothing else carried
    /// over: not counters, not damage, not auras, not control, not targeting, not
    /// anything the rules just discarded. CR 400.7's exceptions (400.7a–400.7m) are the
    /// server's to apply and never become a client's business.
    ///
    /// **Never an addressing scheme.** [`Self::id`] stays the only handle for this
    /// permanent — `valid_actions[].subject`, targets, `attached_to`, and `blocking` all
    /// address objects by their per-zone entity ids, and this field addresses nothing.
    ///
    /// **Absent for a token.** A token (CR 111) is not a card, so there is no physical
    /// card for it to be a projection of. The engine gives a token an instance handle
    /// from the same counter it gives cards, but CR 111.7 means a token can never appear
    /// in a hand, a graveyard, or exile, so such a value could only ever join to itself —
    /// stating it would invite a join that never has a second end. Omitted, so `token:
    /// true` and an absent `physical_card` say the same thing twice, from both ends.
    ///
    /// Additive: omitted by a server that predates the field, and a client that ignores
    /// it renders exactly as it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_card: Option<EntityId>,
    /// Whether the permanent is tapped.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub tapped: bool,
    /// Whether this permanent is currently attacking (CR 508) — declared as an
    /// attacker this combat, or put onto the battlefield attacking by an effect that
    /// said so (CR 506.3c). The board draws both the same way, because in combat they
    /// *are* the same. Server-computed; the client displays it and never derives it.
    /// Omitted from the wire when `false`.
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
    /// The **planeswalker** this permanent is attacking (CR 508.1a), as that
    /// planeswalker's entity id — the second thing an attack may name (issue #608).
    /// `None`/omitted when the attacker is attacking a player instead, or is not
    /// attacking at all.
    ///
    /// It rides *alongside* [`Self::attacking_player`] rather than replacing it, and
    /// both are set when a planeswalker is attacked: this names what is being attacked,
    /// while `attacking_player` names the seat that answers for it — its controller,
    /// who declares blockers and whose sub-combat this belongs to. A client draws the
    /// arrow at whichever one it wants to point at, and needs no rule to work out the
    /// relationship between them. Server-computed; never derived by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacking_planeswalker: Option<EntityId>,
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
    /// Whether **summoning sickness currently restricts this permanent** (CR 302.6):
    /// it is a creature, its controller has not controlled it continuously since the
    /// start of their most recent turn, and it does not have haste (CR 702.10b).
    ///
    /// A player reads this off the board constantly — it is the difference between a
    /// creature that can attack this turn and one that cannot — and no client can
    /// work it out: continuous control since a turn began is stored engine state, and
    /// haste may be granted by an Aura, an anthem, or a pump that is nowhere in the
    /// permanent's printed keywords. Absence of an attack action is not an answer
    /// either, since a creature is offered none outside the declare-attackers step.
    ///
    /// It is a *restriction*, not a property: a permanent that is summoning sick and
    /// has haste reports `false`, because the restriction is what a player is looking
    /// at. Server-computed from the engine's own predicate — the same one that gates
    /// attacking and `{T}` costs — so the board and the action list cannot disagree.
    ///
    /// Additive: omitted (and defaults to `false`), so every existing view is
    /// unchanged on the wire and a client that ignores it renders as it did.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub summoning_sick: bool,
    /// Whether this permanent will **not** untap in its controller's next untap step
    /// (CR 502.4) — what a spell that said so left behind after the spell itself is
    /// gone.
    ///
    /// Stated for the reason [`Self::summoning_sick`] is stated: no client can work it
    /// out. The rules text that imposed it belongs to a spell now in a graveyard, and
    /// the permanent's own printed text says nothing, so without this field a tapped
    /// creature that stays tapped through an untap step is simply inexplicable — the
    /// board would be telling a player a rule it never explained.
    ///
    /// Like summoning sickness it is a *restriction* a player is looking at, not a
    /// mechanism: it reports what will happen, not how many steps are left, because a
    /// card names one untap step and the engine holds one flag.
    ///
    /// Additive: omitted (and defaults to `false`), so every existing view is unchanged
    /// on the wire and a client that ignores it renders as it did.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub skips_next_untap: bool,
    /// The keywords this permanent has that its **printed card does not** — the trample
    /// an *until end of turn* pump gave it, the flying an Aura grants, the vigilance an
    /// anthem hands to a whole team (CR 613 layer 6, CR 613.1f).
    ///
    /// [`CardView::keywords`] already carries a permanent's *current* keywords, and
    /// [`CardView::rules_text`] is the **printed** card's generated text. Between those
    /// two a client cannot say which words are new: subtracting one list from the other
    /// means matching generated prose against keyword names, which is the client reading
    /// rules text to work out a rules fact. So the difference is stated, and the card can
    /// show a granted ability the way it shows a granted `4/4` — as part of what this
    /// permanent *is* right now.
    ///
    /// Carried as the **words a card prints them with** — `"Trample"`, `"First strike"` —
    /// because they are drawn as text beside text, in the same box as
    /// [`CardView::rules_text`], which is likewise server-composed prose (ADR 0008 §7). A
    /// client renders them and nothing more; it never parses them back into keywords.
    ///
    /// Additive: omitted when empty, which is every permanent whose abilities are all
    /// printed, and a client that ignores it renders exactly as it did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub granted_keywords: Vec<String>,
}

/// An **emblem** in the game (CR 114, issue #620): a marker one player has, whose only
/// characteristics are its abilities, and which is in no zone and never leaves.
///
/// It is projected alongside the battlefield rather than inside it because it is not a
/// permanent: it cannot be tapped, attacked, blocked, damaged, destroyed, or targeted,
/// and none of [`Permanent`]'s fields would mean anything on it. A client renders it as a
/// small persistent marker beside its controller, and derives nothing from it — the
/// abilities arrive as the same server-composed rules sentences a card's do.
///
/// **Public information.** Every seat and every spectator sees every emblem, so this list
/// is identical in each view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emblem {
    /// Entity id of this emblem, stable for the rest of the game. Opaque; the client
    /// never parses it, and it never collides with a permanent's or a card's.
    pub id: EntityId,
    /// The player who has it (CR 114.2). Control never changes.
    pub controller: PlayerId,
    /// Its abilities, as server-composed rules sentences — one per ability, in the order
    /// the emblem carries them. This is the whole of what an emblem *is* (CR 114.1), so a
    /// client that renders these has rendered the object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abilities: Vec<String>,
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
///   reconstruction is rules interpretation, which ADR 0001 puts on the server.
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
/// rules interpretation and belongs to the server (ADR 0001). Serialized internally
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
    /// The **physical card** (CR 108.1) this stack object is a projection of — the card
    /// being cast — as the same entity id it carries in [`CardView::id`] in a hand, on the
    /// battlefield, or in a graveyard (issue #650). See
    /// [`Permanent::physical_card`], whose rules are these rules: it is *not* object
    /// identity (CR 400.7), it addresses nothing, and it says only which card the two
    /// projections are of.
    ///
    /// **Absent for an ability.** An ability on the stack (CR 113.3) is an object with no
    /// card behind it at all — activated or triggered — so there is nothing here to name.
    /// [`Self::source`] names the permanent it came from, which is a different question
    /// and stays the only link an ability has.
    ///
    /// Distinct from [`Self::card`], which is a *face to render* and for an ability is the
    /// source permanent's face, keyed by that permanent's id. Joining on `card.id` would
    /// therefore mix a permanent id into a card-id join on exactly the entries where the
    /// answer is "there is no card"; this field is the one that answers the question asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physical_card: Option<EntityId>,
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

/// A card type (CR 300), as [`CardView::card_types`] states it.
///
/// A closed set, mirroring the engine's own — which is why it is an enum here and
/// subtypes are not. The wire names are lowercase (`"creature"`, `"planeswalker"`).
/// A consumer that meets a variant it does not know should render the card rather
/// than drop it; the type line still says what the card is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    /// Land.
    Land,
    /// Creature.
    Creature,
    /// Artifact.
    Artifact,
    /// Enchantment.
    Enchantment,
    /// Instant.
    Instant,
    /// Sorcery.
    Sorcery,
    /// Planeswalker.
    Planeswalker,
    /// Battle.
    Battle,
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
mod tests;
