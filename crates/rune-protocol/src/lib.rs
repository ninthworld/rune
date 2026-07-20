//! RUNE protocol — the entire client/server contract.
//!
//! Two *in-game* message types (docs/protocol.md):
//! - Server -> client: a personalized [`GameView`]
//! - Client -> server: a [`ClientMessage`] (only variant: [`ChooseAction`])
//!
//! These are flanked by a small **lobby** message set that governs the pre-game
//! phase and hands off to the in-game contract once a game is constructed
//! (docs/decisions/0012-lobby-protocol.md):
//! - Server -> client: a [`LobbyView`] (full pre-game state, `GameView`-style)
//! - Client -> server: a [`LobbyCommand`] (`hello`, `create_room`, `join_room`,
//!   `submit_deck`, `ready`, `leave`)
//!
//! Everything here serializes to the JSON documented in `docs/protocol.md`. Any
//! change to these shapes must update that document in the same PR. Clients and
//! server tolerate unknown fields (serde ignores them) so the wire format can
//! grow without breaking older clients — see the forward-compat test below.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Opaque player identity (server-assigned).
pub type PlayerId = String;

/// Opaque per-game entity id: a card, permanent, or stack object.
pub type EntityId = String;

/// One structured, receiver-safe entry in the authoritative recent game history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLogEntry {
    /// Monotonically increasing sequence number. A bounded window may start after
    /// sequence one; clients must render the entries it carries without filling gaps.
    pub sequence: u64,
    /// The event to render as local prose.
    pub event: GameLogEvent,
}

/// A structured game-log event. Entity ids are opaque references for presentation
/// only; a client may highlight one but never infer legality from it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameLogEvent {
    /// A player cast a publicly identified spell.
    SpellCast {
        /// The caster.
        player: PlayerId,
        /// The cast card.
        card: LogEntity,
    },
    /// A spell finished resolving (it was neither countered nor fizzled).
    SpellResolved {
        /// The spell's controller.
        player: PlayerId,
        /// The card that resolved.
        card: LogEntity,
    },
    /// A spell was countered and put into its owner's graveyard.
    SpellCountered {
        /// The countered spell's controller.
        player: PlayerId,
        /// The card that was countered.
        card: LogEntity,
    },
    /// A spell left the stack without resolving because all of its targets became
    /// illegal (a "fizzle").
    SpellFizzled {
        /// The fizzled spell's controller.
        player: PlayerId,
        /// The card that fizzled.
        card: LogEntity,
    },
    /// A player declared attackers (possibly none).
    AttackersDeclared {
        /// The attacking player.
        player: PlayerId,
        /// The declared attackers.
        attackers: Vec<LogEntity>,
    },
    /// A player declared blocker-to-attacker assignments.
    BlockersDeclared {
        /// The defending player.
        player: PlayerId,
        /// The assignments they declared.
        blocks: Vec<LogBlock>,
    },
    /// A player took a London mulligan.
    Mulligan {
        /// The player taking the mulligan.
        player: PlayerId,
    },
    /// A player kept their opening hand, ending their mulligan decisions.
    HandKept {
        /// The player who kept.
        player: PlayerId,
    },
    /// A player's life total changed by this signed amount from a non-damage source
    /// (life gain, or life paid/lost). Damage is reported as [`Self::DamageDealt`].
    LifeChanged {
        /// The affected player.
        player: PlayerId,
        /// Signed life-total delta.
        amount: i32,
    },
    /// A source dealt damage to a player or permanent (including nonlethal damage).
    DamageDealt {
        /// What the damage was dealt to.
        target: LogDamageTarget,
        /// How much damage.
        amount: u32,
    },
    /// A player drew cards. Card identities are intentionally absent.
    CardsDrawn {
        /// The player who drew.
        player: PlayerId,
        /// Number of cards drawn.
        count: u32,
    },
    /// A creature died; it may no longer be present on the battlefield.
    PermanentDied {
        /// The permanent that died.
        permanent: LogEntity,
    },
    /// The game reached this turn/step.
    StepChanged {
        /// New turn number.
        turn: u32,
        /// Player taking that turn.
        active_player: PlayerId,
        /// Entered phase.
        phase: Phase,
    },
    /// A player left the game under CR 800.4a — they lost while two or more players
    /// remained, so play continues without them and their objects are removed. This
    /// is the mid-game "leaves the game" event, distinct from [`Self::GameOver`],
    /// which fires only once one player is left. A two-player loss produces
    /// `GameOver`, never this.
    PlayerEliminated {
        /// The player who left the game.
        player: PlayerId,
        /// Why they lost (CR 104.3 / 704.5).
        reason: GameOverReason,
    },
    /// The game ended with this already-decided result.
    GameOver {
        /// The terminal result.
        result: GameResult,
    },
}

/// A clickable named entity reference in a game log event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntity {
    /// Opaque object or player id.
    pub id: EntityId,
    /// Server-supplied display name; clients do not look it up from hidden state.
    pub name: String,
}

/// What a [`GameLogEvent::DamageDealt`] was dealt to: a player or a permanent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogDamageTarget {
    /// Damage dealt to a player.
    Player {
        /// The player who took the damage.
        player: PlayerId,
    },
    /// Damage marked on a permanent.
    Permanent {
        /// The permanent the damage was dealt to.
        permanent: LogEntity,
    },
}

/// One blocker assignment in a declaration event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogBlock {
    /// The declared blocker.
    pub blocker: LogEntity,
    /// The attacker it blocks.
    pub attacker: LogEntity,
}

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
    #[serde(default, skip_serializing_if = "is_false")]
    pub eliminated: bool,
}

/// The receiver's own public stats — the self-counterpart of [`OpponentView`].
///
/// A player is entitled to see their own public state, but [`GameView`] historically
/// carried none of it: hand *contents* ride in `my_hand` and unspent mana in
/// `mana_pool`, yet the two public numbers every opponent already sees about this
/// player — life total and library size — had no home, so a player could see everyone's
/// life but their own. This is that home; it exposes no hidden information (a player's
/// own life and library size are public).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfView {
    /// The receiver's current life total.
    pub life: i32,
    /// Number of cards left in the receiver's library.
    pub library_size: u32,
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
    #[serde(default, skip_serializing_if = "is_false")]
    pub tapped: bool,
    /// Whether this permanent is currently attacking — declared as an attacker
    /// this combat (CR 508). Server-computed; the client displays it and never
    /// derives it. Omitted from the wire when `false`.
    #[serde(default, skip_serializing_if = "is_false")]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub damage: u32,
    /// The permanent this one is attached to, if any (CR 303.4): an Aura on the
    /// battlefield names the object it enchants, as that host's entity id — the
    /// same `PermanentId`→`EntityId` projection [`blocking`](Self::blocking) uses.
    /// `None`/omitted for an unattached permanent. Server-computed; the client
    /// clusters the attachment with its host and derives no rules from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<EntityId>,
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

/// One object on the stack — a spell or an ability. Ability entries carry their
/// source permanent so the client can point back at it.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Untap step.
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

/// One entry of [`GameView::valid_actions`]. The client renders these; it never
/// invents its own. `subject` names the entities this action belongs to so the
/// client can put the action ON the card rather than in a global bar
/// (docs/decisions/0004-subject-owned-actions.md).
///
/// A multi-step action (a targeted spell, and later a mode/X choice) additionally
/// carries an ordered [`requirements`](ValidAction::requirements) list the client
/// walks as a prompt queue, and/or a [`prompts`](ValidAction::prompts) list of the
/// non-target choice shapes ([`Prompt`]), plus a content-binding
/// [`token`](ValidAction::token) the client echoes verbatim in [`ChooseAction`].
/// Both are decided in docs/decisions/0009-targeting-model.md (§Protocol).
///
/// `Default` yields an empty, unbound action (no subject, no requirements, empty
/// token); it exists so callers that build an action field-by-field need not
/// restate the newer fields, not because an empty action is meaningful.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidAction {
    /// Opaque id the client echoes back in [`ChooseAction`] to take this action.
    pub id: String,
    /// Coarse action category (e.g. `"pass_priority"`, `"activate_ability"`).
    /// A free-form string, not an enum, so new action kinds do not break older
    /// clients that only key off `subject` and `label`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Human-readable label to render for this action.
    pub label: String,
    /// Entity ids this action belongs to; empty for global actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject: Vec<String>,
    /// Whether this action activates a **mana ability** (CR 605): it targets
    /// nothing, does not use the stack, and only produces mana. Server-computed
    /// so a client may offer a lighter gesture — one-click tap-for-mana — for
    /// exactly these actions without ever classifying abilities itself
    /// (ADR 0025). Omitted from the wire when `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mana_ability: bool,
    /// Ordered choice steps this action requires before it can be taken — one per
    /// target slot (modes/X ride the same mechanism later). The client walks them
    /// as a prompt queue and answers every slot **atomically** in a single
    /// [`ChooseAction`], never a stateful multi-message handshake
    /// (docs/protocol.md, two-message philosophy). Empty for a plain action that
    /// needs no sub-choice; omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<TargetRequirement>,
    /// Non-target choice slots this action poses, generalizing the
    /// [`requirements`](ValidAction::requirements) slot pattern to the three richer
    /// prompt shapes — [`Prompt::Option`], [`Prompt::SelectFromZone`], and
    /// [`Prompt::Order`]. Like `requirements`, the client walks these as part of the
    /// same prompt queue and answers every slot **atomically** in a single
    /// [`ChooseAction`] (each slot keyed by its `slot`), never a stateful
    /// multi-message handshake. A slot's answer is one [`TargetChoice`] whose
    /// `chosen` carries the picked ids (an option id, the selected zone ids, or the
    /// full ordering). Both `requirements` and `prompts` are bound by the same
    /// content-binding [`token`](ValidAction::token) (reject-stale, ADR 0009).
    /// Omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<Prompt>,
    /// Content-binding token: a server-issued value bound to this action's exact
    /// content (kind + subject + requirements + prompts). The client echoes it verbatim in
    /// [`ChooseAction::token`]; the server recomputes it from the freshly
    /// regenerated action and rejects a mismatch, so a stale positional `id` can
    /// never rebind to a *different* action. Opaque — the client never parses or
    /// derives it. Omitted only for legacy/unbound actions, where it deserializes
    /// to `""` (which no real token matches, so such an answer is safely rejected).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
}

/// One choice step of a multi-step [`ValidAction`]: a single target slot the
/// player must fill, listing exactly the legal candidates the server computed.
/// The client renders the prompt, highlights the candidates, and computes no
/// legality of its own (docs/decisions/0009-targeting-model.md §Client).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetRequirement {
    /// Stable slot id the client echoes back as [`TargetChoice::slot`] to key its
    /// answer to this step. Opaque; the client never parses it.
    pub slot: String,
    /// Human-readable prompt describing what to choose, e.g. `"target creature"`.
    pub prompt: String,
    /// The legal candidate entity ids for this slot — the **only** choices the
    /// client may offer. Enumerated O(N) per slot, never the cartesian product of
    /// combinations across slots (docs/decisions/0009-targeting-model.md
    /// §Enumeration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<EntityId>,
}

/// One named choice of a [`Prompt::Option`] slot: an opaque `id` the client echoes
/// back and a human-readable `label` to render. The client displays the label and
/// answers the slot with the chosen option's `id` (in the slot's
/// [`TargetChoice::chosen`]); it computes no legality of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptOption {
    /// Opaque id the client echoes back as the slot's chosen value. Never parsed.
    pub id: String,
    /// Human-readable label to render for this choice.
    pub label: String,
}

/// A non-target choice slot a [`ValidAction`] may pose, a **generalization of the
/// [`TargetRequirement`] slot pattern** (slot + prompt + candidates, bound by the
/// action's content [`token`](ValidAction::token), ADR 0009) to three further
/// shapes the engine already needs to pose:
///
/// - [`Prompt::Option`] — pick exactly one of N named choices (also the clean
///   shape for a yes/no such as the mulligan keep/take-another decision).
/// - [`Prompt::SelectFromZone`] — pick `count` cards from a zone (cleanup
///   discard-to-max, mulligan bottoming, future tutors).
/// - [`Prompt::Order`] — arrange N items into an order (ordering simultaneous
///   triggers, scry).
///
/// Every shape shares the same discipline as a target requirement: the server
/// enumerates the only legal choices, the client renders them and derives nothing,
/// and the answer is one [`TargetChoice`] keyed by `slot` and submitted
/// **atomically** in a single [`ChooseAction`]. The action's content-binding
/// `token` folds in every prompt, so a stale/redirected answer whose prompt content
/// has changed is rejected (ADR 0009 stale-view protection). The `kind` tag
/// discriminates the shape on the wire (`{"kind":"option", ...}`); clients tolerate
/// an unknown future `kind`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prompt {
    /// Pick exactly one of N named [`options`](Prompt::Option::options). The slot is
    /// answered with the chosen [`PromptOption::id`] as its single `chosen` entry.
    Option {
        /// Stable slot id the client echoes back as [`TargetChoice::slot`].
        slot: String,
        /// Human-readable prompt describing the decision.
        prompt: String,
        /// The named choices to offer — the **only** answers the client may submit.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        options: Vec<PromptOption>,
    },
    /// Pick [`count`](Prompt::SelectFromZone::count) entity ids from a zone. The slot
    /// is answered with the selected ids in [`TargetChoice::chosen`]; each must be one
    /// of [`candidates`](Prompt::SelectFromZone::candidates).
    SelectFromZone {
        /// Stable slot id the client echoes back as [`TargetChoice::slot`].
        slot: String,
        /// Human-readable prompt describing what to select.
        prompt: String,
        /// The zone the cards are selected from, e.g. `"hand"` — display context for
        /// the client; a free-form string so new zones do not break older clients.
        zone: String,
        /// The player who owns the zone (whose cards are being selected).
        owner: PlayerId,
        /// Exactly how many ids must be chosen.
        count: u32,
        /// The legal candidate entity ids — the **only** ids the client may pick.
        /// Enumerated O(N) by the server; the client never derives or filters them.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        candidates: Vec<EntityId>,
    },
    /// Arrange the given [`items`](Prompt::Order::items) into an order. The slot is
    /// answered with **all** of the items in the chosen order in
    /// [`TargetChoice::chosen`] (a permutation of `items`).
    Order {
        /// Stable slot id the client echoes back as [`TargetChoice::slot`].
        slot: String,
        /// Human-readable prompt describing what to order.
        prompt: String,
        /// The items to arrange, in their initial order. The answer is a permutation
        /// of exactly these ids.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        items: Vec<EntityId>,
    },
}

/// Why a game ended, as carried in [`GameResult::reason`]. A closed, snake_case
/// enum mirroring the engine's losing conditions (CR 104.3 / CR 704.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameOverReason {
    /// A player was reduced to 0 or less life (CR 704.5a).
    LifeZero,
    /// A player attempted to draw from an empty library (CR 704.5c).
    Decked,
    /// A player conceded (CR 104.3a).
    Concede,
}

/// The terminal outcome of a game (CR 104.2a), present on a [`GameView`] only once
/// the game is over. While the game is live the field is omitted entirely (the
/// empty-optional convention), so its mere presence signals game over to a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameResult {
    /// The winning player (CR 104.2a), or omitted for a draw where every remaining
    /// player lost at once (CR 104.4a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<PlayerId>,
    /// The players who lost, in seat order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub losers: Vec<PlayerId>,
    /// Why the game ended.
    pub reason: GameOverReason,
}

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
    #[serde(default, skip_serializing_if = "is_false")]
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
    #[serde(default, skip_serializing_if = "is_false")]
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
}

/// The state a **spectator** connection receives (ADR 0022, issue #351): a
/// non-seated observer watching a live game with all hidden information redacted
/// **by construction**. It shares [`GameView`]'s public component types verbatim —
/// [`OpponentView`], [`Permanent`], [`StackItem`], [`ZonePile`], [`GameLogEntry`],
/// [`Phase`], [`PlayerId`], [`GameResult`] — but carries **no receiver fields**:
/// there is no `you`, `me`, `my_hand`, `mana_pool`, `valid_actions`, `action_deadline`,
/// or per-seat prompt, because those fields simply do not exist on the type. A
/// projection therefore *cannot* leak a hand, a library's contents, or a decision
/// surface to a spectator — the worst case is a missing public fact, never a leaked
/// private one (ADR 0022 §Consequences).
///
/// Every seat appears as the public [`OpponentView`] shape (life, hand *size*, library
/// *size*, graveyard *size*, public statuses, and the eliminated flag); there is no
/// privileged "self". A spectator reconstructs the whole public board from a single
/// `SpectatorView` with no history (the complete-view principle), so it may join
/// mid-game.
///
/// The client distinguishes this from a seated [`GameView`] structurally: a
/// `SpectatorView` carries no `you` field, whereas a `GameView` always serializes one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectatorView {
    /// Every player at the table as the public [`OpponentView`] shape — no seat is
    /// "self". In seat order (see [`Self::seat_order`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<OpponentView>,
    /// All permanents in play (the same public projection seated views share).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battlefield: Vec<Permanent>,
    /// The stack, bottom first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackItem>,
    /// Each player's public graveyard pile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graveyards: Vec<ZonePile>,
    /// Each player's public exile zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exile: Vec<ZonePile>,
    /// The current turn step.
    pub phase: Phase,
    /// The current turn number (1-based).
    #[serde(default)]
    pub turn: u32,
    /// The player whose turn it is (the active player).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_player: PlayerId,
    /// The table's seat order: every player's id in seat order, including eliminated
    /// players — the same public promise seated views carry (issue #345).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seat_order: Vec<PlayerId>,
    /// Which player currently holds priority, if any. Public, decision-free
    /// information — a spectator sees *whose* turn it is to act but never the actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    /// The terminal outcome once the game is over; omitted while it is live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<GameResult>,
    /// The bounded, sequence-numbered window of **public** game history (ADR 0021's
    /// per-viewer redaction gives a spectator the public log for free).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<GameLogEntry>,
    /// Public display names, keyed by [`PlayerId`] (issue #294) — the same public map
    /// seated views carry, so a spectator labels every player without a lobby round-trip.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub player_names: BTreeMap<PlayerId, String>,
}

/// The client's chosen action, answered atomically: the `id` of one issued
/// [`ValidAction`], its content-binding [`token`](ChooseAction::token), and the
/// full set of [`targets`](ChooseAction::targets) filling that action's
/// requirement slots. The server validates the id, verifies the token against the
/// action it currently offers, and checks each chosen target against that slot's
/// freshly computed legal set; anything else is rejected and the current
/// `GameView` is re-sent (docs/decisions/0009-targeting-model.md §Protocol).
///
/// `Default` yields the minimal no-choice answer (empty token and targets), so a
/// caller answering a plain action can set only `action_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseAction {
    /// The `id` of the chosen [`ValidAction`].
    pub action_id: String,
    /// The chosen action's [`ValidAction::token`], echoed verbatim. Binds this
    /// answer to the exact action content the client saw, closing the stale-`id`
    /// rebinding hole. Omitted (`""`) only for a legacy unbound action; a real
    /// server rejects an answer whose token does not match.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// One entry per [`ValidAction::requirements`] slot, carrying the entity ids
    /// the player selected. Submitted all at once (never a multi-message
    /// handshake); empty for an action with no requirements.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetChoice>,
}

/// The player's answer to one choice slot — a [`TargetRequirement`] **or** a
/// [`Prompt`] — keyed back to the slot by `slot`. The same shape answers every slot
/// kind: `chosen` carries the selected ids (a target id, a [`PromptOption::id`], the
/// picked zone ids, or a full ordering). Each id must be one of that slot's
/// advertised candidates/options/items, or the server treats the action as a no-op.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetChoice {
    /// The [`TargetRequirement::slot`] or [`Prompt`] slot this answers.
    pub slot: String,
    /// The entity ids chosen for this slot: one for a single-target slot; the
    /// chosen [`PromptOption::id`] for an [`Prompt::Option`]; the selected ids for a
    /// [`Prompt::SelectFromZone`]; or the full ordering for a [`Prompt::Order`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chosen: Vec<EntityId>,
}

/// Set (or replace) this connection's **priority-stop preferences** (issue #264,
/// ADR 0020): the steps at which the seat wants priority even when it has no
/// meaningful action, so basic auto-pass does not skip it there. Server-authoritative
/// and reconnect-durable — the room stores the set per seat (like a display name) and
/// reflects it back in [`GameView::stops`]. An unparseable message is ignored and the
/// current view re-sent (the non-fatal pattern); the empty set means "stop nowhere".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetStops {
    /// The steps to stop at, as [`Phase`] values. Replaces the seat's current set
    /// wholesale (not additive). Empty (and omitted from the wire) to clear all stops.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stops: Vec<Phase>,
}

/// Everything a client can send about the game. Serializes with a `type`
/// discriminator (`{"type":"choose_action", ...}`) so the wire stays
/// self-describing and open to future message types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// The player chose one of the issued valid actions.
    ChooseAction(ChooseAction),
    /// The player set their priority-stop preferences (issue #264).
    SetStops(SetStops),
}

// ---------------------------------------------------------------------------
// Lobby protocol (docs/decisions/0012-lobby-protocol.md)
//
// The pre-game analogue of the in-game two-message contract: a full-state
// `LobbyView` pushed on every change (mirroring `GameView`) and a tagged
// `LobbyCommand` the client sends to act (mirroring `ChooseAction`). The client
// reconstructs its entire pre-game UI from one `LobbyView` and computes no
// legality of its own. Once a game is constructed the connection switches to the
// in-game `GameView`/`ClientMessage` contract for the life of that game.
// ---------------------------------------------------------------------------

/// Server-issued opaque session/reconnect token. The client stores it and echoes
/// it verbatim on a later [`Hello`] (after a refresh or dropped socket) to prove
/// it is the same connection and be reunited with a held-open seat. Opaque — the
/// client never parses it. This is an *identity* handle, not authentication of a
/// human (ADR 0012, Out of scope).
pub type SessionToken = String;

/// Opaque room identifier, issued by the server on [`CreateRoom`] and shared
/// out-of-band so a second player can [`JoinRoom`]. The client never parses it.
pub type RoomId = String;

/// Opaque game-setup identifier carried in a [`RoomConfig`]. It names which setup
/// (players, starting life, hand size, …) the room builds its game from. The
/// catalogue of setups and their internal shape are owned by ADR 0013; this crate
/// treats the id as an opaque value the server validates.
pub type GameSetupId = String;

/// Opaque card-identity handle used in a submitted [`SubmitDeck`] decklist. The
/// identity-vs-printing model is owned by ADR 0013 — these are card *identities*,
/// never printings or images. The server validates each against its card
/// database; the client never parses them.
///
/// Concretely, an identity is a card's authored `functional_id` (ADR 0018 §3): a
/// lowercase `snake_case` slug such as `llanowar_elves`. That is the only card identity
/// stable across builds — the engine's `CardId` is interned from the catalog's sort
/// order, so it shifts whenever a card is authored ahead of it. Clients still treat this
/// as an opaque string; the note is here so nobody reintroduces an integer.
pub type CardIdentity = String;

/// Configuration for a room, supplied by the creator in [`CreateRoom`] and echoed
/// back in every [`RoomView`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomConfig {
    /// Number of seats in the room. Validated server-side into the inclusive
    /// range `2..=8`; the lobby supports 2–8 seats even while the engine remains
    /// two-player (ADR 0012).
    pub seats: u8,
    /// Which game setup the room will build its game from (opaque; see
    /// [`GameSetupId`]).
    pub game_setup: GameSetupId,
}

/// One seat in a room's roster, as seen by any connection. Hidden information
/// stays redacted: a seat's decklist contents are never exposed, only the fact
/// that the seat is decked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatView {
    /// Zero-based seat index within the room.
    pub seat: u8,
    /// The player occupying this seat, or `None` if it is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occupied_by: Option<PlayerId>,
    /// The occupant's chosen human-readable display name (issue #294), if they set
    /// one. Public, display-only information — the seat's identity remains its
    /// [`occupied_by`](SeatView::occupied_by) [`PlayerId`]. `None`/omitted for an
    /// empty seat or an occupant who has not named themselves, in which case a client
    /// falls back to a seat-derived label (e.g. `"Player 2"`), so an older server that
    /// never sends names keeps working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether this seat has submitted a server-validated deck.
    #[serde(default, skip_serializing_if = "is_false")]
    pub decked: bool,
    /// Whether this seat has declared itself ready.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ready: bool,
}

/// The room a connection is currently in, with its config and full seat roster.
/// Absent from a [`LobbyView`] when the connection is not in a room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    /// The room's opaque id, shared to invite a second player.
    pub room_id: RoomId,
    /// The room's configuration.
    pub config: RoomConfig,
    /// Every seat in the room, in seat order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SeatView>,
}

/// The lifecycle state of a room in the lobby's [`directory`](LobbyView::directory)
/// (issue #280). A room appears in the directory while it is one of these two states;
/// a finished or emptied room simply leaves the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomState {
    /// Pre-game: the room is still filling seats, taking decks, and readying up. A
    /// `gathering` room with an open seat can be joined straight from the directory.
    Gathering,
    /// The room's game has started. Its seats are no longer joinable, but it can be
    /// **spectated**: an observer joins with [`SpectateRoom`] and watches live with
    /// full redaction (ADR 0022, issue #351). The directory advertises its spectator
    /// count in [`RoomSummary::spectators`].
    InProgress,
}

/// One room as it appears in the lobby's public **room directory** (issue #280):
/// exactly enough to browse and join an open game without an out-of-band id, and no
/// more. It carries no seat roster and no player-identifying information beyond the
/// occupancy count, and never any game state — a room browser, not a spectator feed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomSummary {
    /// The room's opaque id — the same id a [`JoinRoom`] command carries, so a client
    /// can join directly from the listing.
    pub room_id: RoomId,
    /// The room's configuration (seat count and game setup): the config summary the
    /// browser renders.
    pub config: RoomConfig,
    /// How many of the room's seats are currently occupied. The total is
    /// [`RoomConfig::seats`]; a [`RoomState::Gathering`] room with `filled` below that
    /// total has an open seat to join.
    pub filled: u8,
    /// How many **spectators** are currently watching the room (ADR 0022, issue #351).
    /// Spectators do not consume seats, so this is independent of [`Self::filled`]; a
    /// room may be spectated at any state, including [`RoomState::InProgress`]. Only a
    /// count is advertised — never a spectator's identity (no social layer in M5).
    /// Omitted from the wire when zero; a client treats a missing field as `0`.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub spectators: u8,
    /// The room's lifecycle state (`gathering` or `in_progress`).
    pub state: RoomState,
}

/// The full pre-game state for one connection, pushed on every change — the
/// pre-game analogue of [`GameView`]. The client rebuilds its entire pre-game UI
/// from a single `LobbyView` (reconnect-safe by construction) and derives no
/// legality: [`valid_commands`](LobbyView::valid_commands) is the only source of
/// interactivity, exactly as `valid_actions` is in `GameView`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyView {
    /// The connection's session/reconnect token. The client stores it and echoes
    /// it on a later [`Hello`]. Always present on the wire (like `GameView::you`).
    #[serde(default)]
    pub session: SessionToken,
    /// The connection's public player identity, used to match itself against a
    /// [`SeatView::occupied_by`]. Distinct from the secret [`session`](LobbyView::session)
    /// token, which is never shown as a seat occupant. Defaults to `""` for a
    /// payload that omits it.
    #[serde(default)]
    pub you: PlayerId,
    /// The connection's own chosen display name (issue #294), if it has set one via
    /// [`SetName`]. Lets the pre-game UI show the local player's name before a seat
    /// exists (and confirm an accepted name); once seated, the same name also rides in
    /// the matching [`SeatView::name`] of the roster. `None`/omitted when unset, in
    /// which case the client falls back to a default presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The room the connection is in, if any, with its config and seat roster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomView>,
    /// The public **room directory** (issue #280): every browsable room in the lobby,
    /// so a player can discover and join an open game without being handed a room id
    /// out-of-band. Each entry is a [`RoomSummary`] (id, config, occupancy count,
    /// lifecycle state); no seat roster or player-identifying info rides here, and no
    /// game state. Re-projected and pushed on every room lifecycle change, exactly
    /// like the rest of the view. Omitted from the wire when empty (no rooms); a client
    /// treats a missing field as an empty list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directory: Vec<RoomSummary>,
    /// The lobby command kinds currently legal for this connection (e.g.
    /// `"create_room"`, `"join_room"`, `"submit_deck"`, `"ready"`, `"unready"`,
    /// `"leave"`). Free-form strings so new command kinds do not break older
    /// clients; the client renders exactly these and computes no legality.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub valid_commands: Vec<String>,
}

/// First-contact / reconnect command. Carries a previously issued
/// [`SessionToken`] when reconnecting; omitted (`None`) on a fresh connection, in
/// which case the server issues a new identity.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// A previously issued session token to reclaim a held-open seat, echoed
    /// verbatim. Omitted on first contact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<SessionToken>,
}

/// Create a new room with the given [`RoomConfig`]. The server replies with a
/// [`LobbyView`] whose [`RoomView`] carries the freshly issued room id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRoom {
    /// The configuration for the new room.
    pub config: RoomConfig,
}

/// Join an existing room by its id. There is no matchmaking or discovery — the id
/// must have been shared out-of-band by the room's creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRoom {
    /// The opaque id of the room to join.
    pub room_id: RoomId,
}

/// Join an existing room as a **spectator** (ADR 0022, issue #351): a non-seated
/// observer watching the game live with all hidden information redacted. Unlike
/// [`JoinRoom`], a spectator does **not** consume a seat, so it may join a room whose
/// seats are full — including a room whose game is already **in progress**
/// ([`RoomState::InProgress`]); the spectator reconstructs the whole public board from
/// its first [`SpectatorView`]. The room advertises its spectator count in
/// [`RoomSummary::spectators`] but never a spectator's identity to the seated players.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectateRoom {
    /// The opaque id of the room to spectate.
    pub room_id: RoomId,
}

/// Submit a decklist for this connection's seat. The list is a flat sequence of
/// [`CardIdentity`] handles (a card appearing multiple times is repeated). The
/// server validates it authoritatively against its card database and reflects
/// only *decked: yes/no* to other seats, never the contents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitDeck {
    /// The card identities that make up the deck, duplicates repeated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CardIdentity>,
}

/// Declare (or retract) readiness for this connection's seat. A seat may ready
/// only once it is occupied and has a validated deck; the game is constructed the
/// instant every seat is simultaneously filled, decked, and ready.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    /// `true` to ready up, `false` to un-ready.
    pub ready: bool,
}

/// Set (or change) this connection's public display name (issue #294). The name is
/// how other players read this one — it appears in the lobby roster
/// ([`SeatView::name`]) and, once a game starts, in every in-game view
/// ([`GameView::player_names`]). The server validates it (length bounds, printable
/// characters) and rejects an invalid value with the lobby's non-fatal error
/// pattern — the current [`LobbyView`] is re-sent unchanged. The name is bound to
/// the *session*, so it survives a per-tab reconnect. It is a display label only,
/// never an identity or authentication handle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetName {
    /// The requested display name. The server trims and validates it before storing.
    pub name: String,
}

/// Everything a client can send in the lobby phase. Serializes with a `type`
/// discriminator (`{"type":"create_room", ...}`), structurally parallel to
/// [`ClientMessage`], so the wire stays self-describing and open to future
/// commands. The server validates every command against authoritative state and
/// answers with a fresh [`LobbyView`]; an invalid command is rejected and the
/// current `LobbyView` re-sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyCommand {
    /// First contact or reconnect; optionally carries a prior session token.
    Hello(Hello),
    /// Create a new room with a config.
    CreateRoom(CreateRoom),
    /// Join an existing room by id.
    JoinRoom(JoinRoom),
    /// Submit a decklist for this connection's seat.
    SubmitDeck(SubmitDeck),
    /// Declare or retract readiness.
    Ready(Ready),
    /// Set or change this connection's public display name (issue #294).
    SetName(SetName),
    /// Join an existing room as a spectator (ADR 0022, issue #351) — no seat consumed.
    SpectateRoom(SpectateRoom),
    /// Request the public card catalog and per-format deck rules (issue #367). The
    /// server answers with a one-shot [`CatalogView`] and changes no lobby state, so a
    /// connection can browse the supported card pool and format rules without joining
    /// or starting a game. Serializes as the bare tag `{"type":"request_catalog"}`.
    RequestCatalog,
    /// Leave the current room (vacating the seat, or ending a spectator session).
    Leave,
}

// ---------------------------------------------------------------------------
// Lobby card catalog (docs/decisions/0012-lobby-protocol.md, issue #367)
//
// A lobby-phase connection can enumerate the complete public card pool and each
// advertised format's deck rules before a game exists. The projection is derived
// server-side from the one embedded `CardDatabase` and the format registry — it
// ships no bundled catalog copy — and its rules text is generated by the SAME
// server generator an in-game `CardView` uses, so the two can never disagree
// (ADR 0018 §7). It carries public card data only: never a deck, roster, or game
// state. The client requests it with [`LobbyCommand::RequestCatalog`] and the server
// replies with a single [`CatalogView`] frame; it is reference data, not per-connection
// lobby state, so it does not ride the pushed [`LobbyView`].
// ---------------------------------------------------------------------------

/// The current schema version carried in [`CatalogView::catalog_version`]. A single
/// frame carries the whole catalog today; the version leaves room to add paging (or
/// other additive fields) later without breaking older clients, so "the whole catalog
/// fits one frame" is deliberately **not** a hard wire invariant.
pub const CATALOG_VERSION: u32 = 1;

/// One supported card's public characteristics, as listed in a [`CatalogView`] (issue
/// #367). This is the browse-time counterpart of the in-game [`CardView`]: it carries
/// the card's stable identity and the same server-computed characteristics — including
/// the **generated** rules text an in-game `CardView` shows — but no per-game entity id,
/// because a catalog entry names a card by identity, not a specific instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogCard {
    /// The card's stable functional identity (ADR 0018 §3) — the same handle a
    /// [`SubmitDeck`] decklist uses ([`CardIdentity`]). Always present.
    pub functional_id: CardIdentity,
    /// Display name.
    pub name: String,
    /// The full type line, including any basic supertype, e.g. `"Basic Land — Forest"`
    /// or `"Creature — Elf Warrior"`.
    pub type_line: String,
    /// Displayed mana cost string, e.g. `"{1}{G}"`. `None`/omitted for a card without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// The card's rules text, **generated** by the server from its ability IR — byte-for-byte
    /// what an in-game [`CardView::rules_text`] shows (ADR 0018 §7). Empty (and omitted from
    /// the wire) for a vanilla card with no rules.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rules_text: String,
    /// Displayed power (a string so `*` and other non-numeric values round-trip). Present
    /// only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Displayed toughness; see [`CatalogCard::power`]. Present only for creatures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// The card's keyword abilities as lowercase wire names (e.g. `"flying"`), the same
    /// projection [`CardView::keywords`] carries. Omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
}

/// One advertised format's public deck rules and seat range, as listed in a
/// [`CatalogView`] (issue #367, ADR 0013 §4). It exposes exactly the server-side
/// deck-legality policy a [`SubmitDeck`] is validated against so a client can build a
/// legal deck ahead of time; a **permissive** format advertises its permissiveness
/// honestly, as `None` upper bounds rather than a sentinel number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFormat {
    /// The `game_setup` identifier that names this format ([`GameSetupId`]) — the same
    /// id a [`RoomConfig`] carries to create a room using it.
    pub game_setup: GameSetupId,
    /// Fewest cards a legal deck may hold (inclusive). `0` for a format with no minimum.
    pub min_deck_size: u32,
    /// Most cards a legal deck may hold (inclusive), or `None`/omitted for no upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_deck_size: Option<u32>,
    /// The most copies of any single non-exempt card a deck may hold, or `None`/omitted
    /// for **no copy limit** (an honestly permissive format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_copies: Option<u32>,
    /// Whether basic lands are exempt from [`max_copies`](CatalogFormat::max_copies) (the
    /// usual Magic rule, CR 100.2a).
    pub basic_land_exempt: bool,
    /// Fewest seats a room using this format may be created with (inclusive).
    pub min_seats: u8,
    /// Most seats a room using this format may be created with (inclusive).
    pub max_seats: u8,
}

/// The public card catalog and format deck rules, a lobby-phase **server → client**
/// frame answered to a [`LobbyCommand::RequestCatalog`] (issue #367). It is a versioned
/// single-frame projection of the complete supported card pool and every advertised
/// format, built server-side from the one embedded card database and the format
/// registry. Public data only — it never carries a deck, a roster, or any game state.
///
/// On the wire it is distinguished from a [`LobbyView`] by its `catalog_version` field
/// (a `LobbyView` carries none) and from a [`GameView`]/[`SpectatorView`] by carrying no
/// `phase`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogView {
    /// The projection's schema version ([`CATALOG_VERSION`]). Always present; it both
    /// versions the shape and serves as the wire discriminator against a `LobbyView`.
    pub catalog_version: u32,
    /// Every supported card, in a stable order. Omitted from the wire only for an empty
    /// catalog (a client treats a missing field as an empty list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CatalogCard>,
    /// Every advertised format's deck rules and seat range. Omitted from the wire when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<CatalogFormat>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use super::*;

    #[test]
    fn game_log_events_tag_their_type_and_round_trip() {
        // The new #259 vocabulary is a contract: each event serializes under its
        // snake_case `type`, and `damage_dealt` nests a `kind`-tagged target.
        let resolved = GameLogEvent::SpellResolved {
            player: "p0".into(),
            card: LogEntity {
                id: "card_3".into(),
                name: "Quickfire Bolt".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(&resolved).unwrap(),
            serde_json::json!({
                "type": "spell_resolved",
                "player": "p0",
                "card": { "id": "card_3", "name": "Quickfire Bolt" },
            })
        );

        let damage = GameLogEvent::DamageDealt {
            target: LogDamageTarget::Permanent {
                permanent: LogEntity {
                    id: "perm_7".into(),
                    name: "Thornback Boar".into(),
                },
            },
            amount: 3,
        };
        let json = serde_json::to_value(&damage).unwrap();
        assert_eq!(json["type"], "damage_dealt");
        assert_eq!(json["amount"], 3);
        assert_eq!(json["target"]["kind"], "permanent");
        assert_eq!(json["target"]["permanent"]["name"], "Thornback Boar");

        // Every new variant survives a JSON round trip.
        for event in [
            resolved,
            damage,
            GameLogEvent::SpellCountered {
                player: "p1".into(),
                card: LogEntity {
                    id: "card_9".into(),
                    name: "Runic Negation".into(),
                },
            },
            GameLogEvent::SpellFizzled {
                player: "p0".into(),
                card: LogEntity {
                    id: "card_3".into(),
                    name: "Quickfire Bolt".into(),
                },
            },
            GameLogEvent::HandKept {
                player: "p0".into(),
            },
            GameLogEvent::DamageDealt {
                target: LogDamageTarget::Player {
                    player: "p1".into(),
                },
                amount: 2,
            },
            GameLogEvent::PlayerEliminated {
                player: "p2".into(),
                reason: GameOverReason::LifeZero,
            },
        ] {
            let text = serde_json::to_string(&event).unwrap();
            let back: GameLogEvent = serde_json::from_str(&text).unwrap();
            assert_eq!(event, back);
        }
    }

    #[test]
    fn issue_342_player_eliminated_event_tags_its_type_and_reason() {
        // The elimination log event (issue #342) serializes under its snake_case
        // `type` and carries the same GameOverReason enum `game_over` uses.
        let event = GameLogEvent::PlayerEliminated {
            player: "p1".into(),
            reason: GameOverReason::Concede,
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            serde_json::json!({
                "type": "player_eliminated",
                "player": "p1",
                "reason": "concede",
            })
        );
    }

    #[test]
    fn choose_action_is_just_an_id() {
        let msg = ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
        };
        assert_eq!(msg.action_id, "a2");
    }

    #[test]
    fn client_message_uses_documented_wire_shape() {
        // A no-choice action: empty token and targets elide, so the minimal
        // `{type, action_id}` wire shape is preserved for backward compatibility.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a2".into(),
            token: String::new(),
            targets: vec![],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "choose_action", "action_id": "a2" })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn choose_action_carries_token_and_targets() {
        // A real targeted answer: id + content-binding token + the atomically
        // submitted selection, keyed per requirement slot.
        let msg = ClientMessage::ChooseAction(ChooseAction {
            action_id: "a3".into(),
            token: "h:9f2c".into(),
            targets: vec![TargetChoice {
                slot: "t0".into(),
                chosen: vec!["perm_bear".into()],
            }],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "choose_action",
                "action_id": "a3",
                "token": "h:9f2c",
                "targets": [{ "slot": "t0", "chosen": ["perm_bear"] }]
            })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_264_set_stops_message_uses_documented_wire_shape() {
        // The stops-preference message rides the same tagged `ClientMessage` envelope
        // as `choose_action`, carrying the stop phases as snake_case `Phase` names.
        let msg = ClientMessage::SetStops(SetStops {
            stops: vec![Phase::Upkeep, Phase::End],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "set_stops", "stops": ["upkeep", "end"] })
        );
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_264_empty_set_stops_elides_the_list() {
        // Clearing all stops sends an empty list, which elides — the minimal wire shape.
        let msg = ClientMessage::SetStops(SetStops { stops: vec![] });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "set_stops" }));
        let back: ClientMessage = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

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
    fn valid_action_serializes_type_and_omits_empty_subject() {
        let pass = ValidAction {
            mana_ability: false,
            id: "a1".into(),
            kind: "pass_priority".into(),
            label: "Pass".into(),
            subject: vec![],
            requirements: vec![],
            prompts: vec![],
            token: String::new(),
        };
        let json = serde_json::to_value(&pass).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "id": "a1", "type": "pass_priority", "label": "Pass" })
        );
    }

    #[test]
    fn cr_605_mana_ability_flag_round_trips_and_defaults_off() {
        // ADR 0025: `mana_ability` rides the wire only when true; a legacy
        // payload without the key deserializes to `false`.
        let tap = ValidAction {
            mana_ability: true,
            id: "a2".into(),
            kind: "activate_ability".into(),
            label: "{T}: Add {G}.".into(),
            subject: vec!["perm_1".into()],
            requirements: vec![],
            prompts: vec![],
            token: "h:1".into(),
        };
        let json = serde_json::to_value(&tap).unwrap();
        assert_eq!(json.get("mana_ability"), Some(&serde_json::json!(true)));
        let back: ValidAction = serde_json::from_value(json).unwrap();
        assert_eq!(back, tap);

        let legacy: ValidAction = serde_json::from_value(serde_json::json!({
            "id": "a1", "type": "activate_ability", "label": "x"
        }))
        .unwrap();
        assert!(!legacy.mana_ability);
    }

    #[test]
    fn valid_action_carries_requirements_and_token() {
        // A targeted spell: subject is the hand card, requirements advertise the
        // one target slot's legal candidates, and a content-binding token is
        // present for the client to echo back.
        let bolt = ValidAction {
            mana_ability: false,
            id: "a3".into(),
            kind: "cast_spell".into(),
            label: "Cast Lightning Bolt".into(),
            subject: vec!["c3".into()],
            requirements: vec![TargetRequirement {
                slot: "t0".into(),
                prompt: "target creature or player".into(),
                candidates: vec!["perm_bear".into(), "p1".into(), "p2".into()],
            }],
            prompts: vec![],
            token: "h:9f2c".into(),
        };
        let json = serde_json::to_value(&bolt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "id": "a3",
                "type": "cast_spell",
                "label": "Cast Lightning Bolt",
                "subject": ["c3"],
                "requirements": [{
                    "slot": "t0",
                    "prompt": "target creature or player",
                    "candidates": ["perm_bear", "p1", "p2"]
                }],
                "token": "h:9f2c"
            })
        );
        let back: ValidAction = serde_json::from_value(json).unwrap();
        assert_eq!(back, bolt);
    }

    #[test]
    fn option_prompt_round_trips_and_tags_its_kind() {
        // `option` (mulligan keep/take-another): a slot listing named choices, tagged
        // `kind: "option"` on the wire, answered with the chosen option id.
        let prompt = Prompt::Option {
            slot: "decision".into(),
            prompt: "Keep this hand or take a mulligan?".into(),
            options: vec![
                PromptOption {
                    id: "keep".into(),
                    label: "Keep this hand".into(),
                },
                PromptOption {
                    id: "mulligan".into(),
                    label: "Mulligan".into(),
                },
            ],
        };
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "option",
                "slot": "decision",
                "prompt": "Keep this hand or take a mulligan?",
                "options": [
                    { "id": "keep", "label": "Keep this hand" },
                    { "id": "mulligan", "label": "Mulligan" }
                ]
            })
        );
        let back: Prompt = serde_json::from_value(json).unwrap();
        assert_eq!(back, prompt);
    }

    #[test]
    fn select_from_zone_prompt_round_trips() {
        // `select_from_zone` (cleanup discard / mulligan bottoming): carries the zone,
        // its owner, how many to pick, and the candidate entity ids.
        let prompt = Prompt::SelectFromZone {
            slot: "discard".into(),
            prompt: "Choose a card to discard".into(),
            zone: "hand".into(),
            owner: "p0".into(),
            count: 1,
            candidates: vec!["card_1".into(), "card_2".into(), "card_3".into()],
        };
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "select_from_zone",
                "slot": "discard",
                "prompt": "Choose a card to discard",
                "zone": "hand",
                "owner": "p0",
                "count": 1,
                "candidates": ["card_1", "card_2", "card_3"]
            })
        );
        let back: Prompt = serde_json::from_value(json).unwrap();
        assert_eq!(back, prompt);
    }

    #[test]
    fn order_prompt_round_trips() {
        // `order` (ordering simultaneous triggers / scry): the items to arrange, whose
        // answer is a permutation of exactly these ids.
        let prompt = Prompt::Order {
            slot: "triggers".into(),
            prompt: "Order these triggered abilities".into(),
            items: vec!["stack_1".into(), "stack_2".into()],
        };
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "order",
                "slot": "triggers",
                "prompt": "Order these triggered abilities",
                "items": ["stack_1", "stack_2"]
            })
        );
        let back: Prompt = serde_json::from_value(json).unwrap();
        assert_eq!(back, prompt);
    }

    #[test]
    fn valid_action_carries_prompts_and_is_answered_by_target_choice() {
        // A prompt-bearing action rides on `valid_actions` exactly like a targeted
        // one: it carries its prompt slots and a content-binding token, and the client
        // answers each slot with a `TargetChoice` keyed by `slot`.
        let action = ValidAction {
            mana_ability: false,
            id: "a0".into(),
            kind: "mulligan_decision".into(),
            label: "Mulligan decision".into(),
            subject: vec![],
            requirements: vec![],
            prompts: vec![
                Prompt::Option {
                    slot: "decision".into(),
                    prompt: "Keep or mulligan?".into(),
                    options: vec![
                        PromptOption {
                            id: "keep".into(),
                            label: "Keep".into(),
                        },
                        PromptOption {
                            id: "mulligan".into(),
                            label: "Mulligan".into(),
                        },
                    ],
                },
                Prompt::SelectFromZone {
                    slot: "bottom".into(),
                    prompt: "Bottom 1 card".into(),
                    zone: "hand".into(),
                    owner: "p0".into(),
                    count: 1,
                    candidates: vec!["card_1".into(), "card_2".into()],
                },
            ],
            token: "t0123456789abcdef".into(),
        };
        let json = serde_json::to_value(&action).unwrap();
        // `prompts` sits alongside `requirements` in the same wire object.
        assert!(json.get("prompts").is_some());
        assert_eq!(json["prompts"][0]["kind"], serde_json::json!("option"));
        assert_eq!(
            json["prompts"][1]["kind"],
            serde_json::json!("select_from_zone")
        );
        let back: ValidAction = serde_json::from_value(json).unwrap();
        assert_eq!(back, action);

        // The answer keys each slot with a `TargetChoice` (option id + selected ids).
        let answer = ChooseAction {
            action_id: "a0".into(),
            token: "t0123456789abcdef".into(),
            targets: vec![
                TargetChoice {
                    slot: "decision".into(),
                    chosen: vec!["keep".into()],
                },
                TargetChoice {
                    slot: "bottom".into(),
                    chosen: vec!["card_1".into()],
                },
            ],
        };
        let back: ChooseAction =
            serde_json::from_value(serde_json::to_value(&answer).unwrap()).unwrap();
        assert_eq!(back, answer);
    }

    #[test]
    fn valid_action_without_prompts_omits_the_field() {
        // Backward-compat wire shape: an action with no prompts elides the field, so
        // existing (targeting/plain) actions serialize exactly as before.
        let pass = ValidAction {
            id: "a1".into(),
            kind: "pass_priority".into(),
            label: "Pass".into(),
            ..Default::default()
        };
        let json = serde_json::to_value(&pass).unwrap();
        assert!(json.get("prompts").is_none());
    }

    #[test]
    fn legacy_valid_action_without_token_or_requirements_deserializes() {
        // A payload from a server that predates this shape omits both new fields;
        // they must default (empty requirements, empty token) rather than fail.
        let json = r#"{ "id": "a1", "type": "pass_priority", "label": "Pass" }"#;
        let action: ValidAction = serde_json::from_str(json).unwrap();
        assert!(action.requirements.is_empty());
        assert_eq!(action.token, "");
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
        };

        let json = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
        // The receiver's own stats survive the round trip (issue #255).
        assert_eq!(back.me.life, 18);
        assert_eq!(back.me.library_size, 52);
    }

    #[test]
    fn issue_255_self_view_round_trips_and_defaults_when_omitted() {
        // The receiver's own public stats round-trip on their own...
        let me = SelfView {
            life: 15,
            library_size: 40,
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
    fn prompts_contract_fixture_round_trips_and_matches_typed_fields() {
        // Cross-language contract fixture (issue #56/#156): a pre-game mulligan frame
        // whose `mulligan_decision` action carries an `option` prompt (keep/mulligan)
        // and a `select_from_zone` bottoming prompt. The web client's `wire.test.ts`
        // consumes these exact bytes; a rename/retype here (or there) fails a test.
        let json = include_str!("../fixtures/gameview-prompts.json");
        let view: GameView = serde_json::from_str(json).unwrap();

        // Round-trips through serde JSON without loss.
        let reencoded = serde_json::to_string(&view).unwrap();
        let back: GameView = serde_json::from_str(&reencoded).unwrap();
        assert_eq!(back, view);

        let decision = &view.valid_actions[0];
        assert_eq!(decision.kind, "mulligan_decision");
        assert!(!decision.token.is_empty(), "a prompt action is token-bound");
        assert_eq!(decision.prompts.len(), 2);

        // First slot: the `option` keep/mulligan decision.
        let Prompt::Option { slot, options, .. } = &decision.prompts[0] else {
            panic!("first prompt is an option");
        };
        assert_eq!(slot, "decision");
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["keep", "mulligan"],
        );

        // Second slot: the `select_from_zone` bottoming over the hand.
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            candidates,
            ..
        } = &decision.prompts[1]
        else {
            panic!("second prompt is a select_from_zone");
        };
        assert_eq!(slot, "bottom");
        assert_eq!(zone, "hand");
        assert_eq!(owner, "p0");
        assert_eq!(*count, 1);
        assert_eq!(candidates, &["card_10".to_string(), "card_11".to_string()]);
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
    fn lobby_command_hello_omits_absent_token() {
        // First contact carries no token; the minimal `{type}` wire shape must be
        // preserved so an older/fresh client stays compatible.
        let msg = LobbyCommand::Hello(Hello { token: None });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "hello" }));
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn lobby_command_hello_round_trips_with_token() {
        // A reconnect echoes the previously issued session token verbatim.
        let msg = LobbyCommand::Hello(Hello {
            token: Some("s:ab12".into()),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "hello", "token": "s:ab12" })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn lobby_command_create_room_carries_config() {
        let msg = LobbyCommand::CreateRoom(CreateRoom {
            config: RoomConfig {
                seats: 4,
                game_setup: "standard_2p".into(),
            },
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "create_room",
                "config": { "seats": 4, "game_setup": "standard_2p" }
            })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn lobby_command_join_room_round_trips() {
        let msg = LobbyCommand::JoinRoom(JoinRoom {
            room_id: "r:7f3".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "join_room", "room_id": "r:7f3" })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn lobby_command_submit_deck_round_trips_and_elides_empty() {
        // A populated decklist round-trips as a flat list of identities.
        let msg = LobbyCommand::SubmitDeck(SubmitDeck {
            cards: vec!["ci_bear".into(), "ci_bear".into(), "ci_forest".into()],
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "submit_deck",
                "cards": ["ci_bear", "ci_bear", "ci_forest"]
            })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);

        // An empty decklist elides the `cards` field entirely.
        let empty = LobbyCommand::SubmitDeck(SubmitDeck { cards: vec![] });
        let json = serde_json::to_value(&empty).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "submit_deck" }));
    }

    #[test]
    fn lobby_command_ready_and_leave_round_trip() {
        let ready = LobbyCommand::Ready(Ready { ready: true });
        let json = serde_json::to_value(&ready).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "ready", "ready": true }));
        assert_eq!(serde_json::from_value::<LobbyCommand>(json).unwrap(), ready);

        let leave = LobbyCommand::Leave;
        let json = serde_json::to_value(&leave).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "leave" }));
        assert_eq!(serde_json::from_value::<LobbyCommand>(json).unwrap(), leave);
    }

    #[test]
    fn issue_367_request_catalog_command_is_a_bare_tag() {
        // The catalog request is a unit command: just its `type` discriminator, like
        // `leave`, so it round-trips as `{"type":"request_catalog"}`.
        let msg = LobbyCommand::RequestCatalog;
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json, serde_json::json!({ "type": "request_catalog" }));
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_367_catalog_view_round_trips_and_elides_defaults() {
        // A card with rules text and P/T, and one with neither, exercise both the
        // present and the elided wire shapes; the format advertises its bounds and a
        // permissive `None` copy limit.
        let view = CatalogView {
            catalog_version: CATALOG_VERSION,
            cards: vec![
                CatalogCard {
                    functional_id: "serra_angel".into(),
                    name: "Serra Angel".into(),
                    type_line: "Creature — Angel".into(),
                    mana_cost: Some("{3}{W}{W}".into()),
                    rules_text: "Flying, vigilance".into(),
                    power: Some("4".into()),
                    toughness: Some("4".into()),
                    keywords: vec!["flying".into(), "vigilance".into()],
                },
                CatalogCard {
                    functional_id: "forest".into(),
                    name: "Forest".into(),
                    type_line: "Basic Land — Forest".into(),
                    mana_cost: None,
                    rules_text: "{T}: Add {G}.".into(),
                    power: None,
                    toughness: None,
                    keywords: vec![],
                },
            ],
            formats: vec![CatalogFormat {
                game_setup: "standard_2p".into(),
                min_deck_size: 0,
                max_deck_size: None,
                max_copies: None,
                basic_land_exempt: true,
                min_seats: 2,
                max_seats: 8,
            }],
        };
        let json = serde_json::to_value(&view).unwrap();
        // The version is the wire discriminator (a `LobbyView` never carries it).
        assert_eq!(json["catalog_version"], 1);
        // A basic land elides its absent mana cost and P/T; a permissive format elides
        // its `None` upper bounds.
        assert_eq!(json["cards"][1].get("mana_cost"), None);
        assert_eq!(json["cards"][1].get("power"), None);
        assert_eq!(json["cards"][1].get("keywords"), None);
        assert_eq!(json["formats"][0].get("max_copies"), None);
        assert_eq!(json["formats"][0].get("max_deck_size"), None);
        assert_eq!(json["formats"][0]["min_deck_size"], 0);
        let back: CatalogView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn issue_351_lobby_command_spectate_room_round_trips() {
        let msg = LobbyCommand::SpectateRoom(SpectateRoom {
            room_id: "r:7f3".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "spectate_room", "room_id": "r:7f3" })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn issue_351_room_summary_carries_a_spectator_count_and_elides_zero() {
        // An in-progress room with spectators advertises the count.
        let watched = RoomSummary {
            room_id: "r:1".into(),
            config: RoomConfig {
                seats: 4,
                game_setup: "standard_ffa".into(),
            },
            filled: 4,
            spectators: 3,
            state: RoomState::InProgress,
        };
        let json = serde_json::to_value(&watched).unwrap();
        assert_eq!(json.get("spectators"), Some(&serde_json::json!(3)));
        assert_eq!(json.get("state"), Some(&serde_json::json!("in_progress")));
        assert_eq!(
            serde_json::from_value::<RoomSummary>(json).unwrap(),
            watched
        );

        // Zero spectators elide from the wire; an older payload without the field
        // deserializes to zero.
        let unwatched = RoomSummary {
            spectators: 0,
            ..watched.clone()
        };
        let json = serde_json::to_value(&unwatched).unwrap();
        assert!(json.get("spectators").is_none());
        let legacy: RoomSummary = serde_json::from_str(
            r#"{"room_id":"r:1","config":{"seats":4,"game_setup":"standard_ffa"},"filled":4,"state":"in_progress"}"#,
        )
        .unwrap();
        assert_eq!(legacy.spectators, 0);
    }

    #[test]
    fn issue_351_spectator_view_round_trips_and_has_no_receiver_fields() {
        // A populated, live spectator view over a three-player game with one seat
        // eliminated. Every seat is an OpponentView (public counts only).
        let view = SpectatorView {
            players: vec![
                OpponentView {
                    player_id: "p0".into(),
                    hand_size: 4,
                    life: 18,
                    library_size: 33,
                    graveyard_size: 2,
                    statuses: vec![],
                    eliminated: false,
                },
                OpponentView {
                    player_id: "p1".into(),
                    hand_size: 0,
                    life: 0,
                    library_size: 0,
                    graveyard_size: 7,
                    statuses: vec![],
                    eliminated: true,
                },
                OpponentView {
                    player_id: "p2".into(),
                    hand_size: 6,
                    life: 20,
                    library_size: 34,
                    graveyard_size: 1,
                    statuses: vec![],
                    eliminated: false,
                },
            ],
            battlefield: vec![],
            stack: vec![],
            graveyards: vec![],
            exile: vec![],
            phase: Phase::PrecombatMain,
            turn: 9,
            active_player: "p0".into(),
            seat_order: vec!["p0".into(), "p1".into(), "p2".into()],
            priority_player: Some("p0".into()),
            result: None,
            log: vec![],
            player_names: BTreeMap::new(),
        };
        let json = serde_json::to_value(&view).unwrap();
        // Redaction is structural: the type has no receiver/decision fields at all.
        for hidden in [
            "you",
            "me",
            "my_hand",
            "mana_pool",
            "valid_actions",
            "action_deadline",
            "stops",
            "auto_passed",
            "action_rejected",
        ] {
            assert!(
                json.get(hidden).is_none(),
                "a spectator view must never carry `{hidden}`"
            );
        }
        // Every seat appears as a public OpponentView; the eliminated seat is flagged.
        assert_eq!(json["players"].as_array().unwrap().len(), 3);
        assert_eq!(json["players"][1]["eliminated"], true);
        let back: SpectatorView =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn lobby_view_round_trips_populated() {
        let view = LobbyView {
            session: "s:ab12".into(),
            you: "p1".into(),
            name: Some("Alice".into()),
            room: Some(RoomView {
                room_id: "r:7f3".into(),
                config: RoomConfig {
                    seats: 2,
                    game_setup: "standard_2p".into(),
                },
                seats: vec![
                    SeatView {
                        seat: 0,
                        occupied_by: Some("p1".into()),
                        name: Some("Alice".into()),
                        decked: true,
                        ready: true,
                    },
                    SeatView {
                        seat: 1,
                        occupied_by: Some("p2".into()),
                        name: None,
                        decked: true,
                        ready: false,
                    },
                ],
            }),
            directory: vec![],
            valid_commands: vec!["submit_deck".into(), "unready".into(), "leave".into()],
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: LobbyView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn lobby_view_elides_empties_and_redacts_seat_flags() {
        // A connection with an identity but not yet in a room: `room` is absent
        // and a still-empty seat's `decked`/`ready`/`occupied_by` all elide.
        let view = LobbyView {
            session: "s:new".into(),
            you: "p9".into(),
            name: None,
            room: None,
            directory: vec![],
            valid_commands: vec!["create_room".into(), "join_room".into()],
        };
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("room").is_none());
        // An empty directory elides from the wire, like every other empty collection.
        assert!(json.get("directory").is_none());
        // `session` and `you` are always present on the wire (like `GameView::you`).
        assert_eq!(json.get("session"), Some(&serde_json::json!("s:new")));
        assert_eq!(json.get("you"), Some(&serde_json::json!("p9")));

        // An empty seat serializes to just its index.
        let empty_seat = SeatView {
            seat: 3,
            occupied_by: None,
            name: None,
            decked: false,
            ready: false,
        };
        let seat_json = serde_json::to_value(&empty_seat).unwrap();
        assert_eq!(seat_json, serde_json::json!({ "seat": 3 }));
        let back: LobbyView = serde_json::from_value(json).unwrap();
        assert_eq!(back, view);
    }

    #[test]
    fn lobby_view_ignores_unknown_fields() {
        // Forward-compat invariant: a newer server may add lobby fields; older
        // clients must still deserialize the message.
        let json = r#"{ "session": "s:1", "you": "p1", "some_future_field": true }"#;
        let view: LobbyView = serde_json::from_str(json).unwrap();
        assert_eq!(view.session, "s:1");
        assert_eq!(view.you, "p1");
        assert!(view.room.is_none());
    }

    #[test]
    fn lobby_command_ignores_unknown_fields() {
        // A command from a newer client with extra fields still deserializes.
        let json = r#"{ "type": "join_room", "room_id": "r:1", "future": 7 }"#;
        let cmd: LobbyCommand = serde_json::from_str(json).unwrap();
        assert_eq!(
            cmd,
            LobbyCommand::JoinRoom(JoinRoom {
                room_id: "r:1".into()
            })
        );
    }

    #[test]
    fn lobby_view_defaults_identity_when_absent() {
        // A payload that omits `session`/`you` still deserializes, defaulting both
        // to `""` rather than failing the whole message.
        let json = r#"{ "valid_commands": ["hello"] }"#;
        let view: LobbyView = serde_json::from_str(json).unwrap();
        assert_eq!(view.session, "");
        assert_eq!(view.you, "");
        assert!(view.directory.is_empty());
        assert_eq!(view.valid_commands, vec!["hello".to_string()]);
    }

    #[test]
    fn room_summary_round_trips_and_tags_its_state() {
        // Issue #280: a directory entry carries the room id, its config summary, the
        // occupancy count, and the lifecycle state tagged snake_case on the wire.
        let gathering = RoomSummary {
            room_id: "r0".into(),
            config: RoomConfig {
                seats: 2,
                game_setup: "standard_2p".into(),
            },
            filled: 1,
            spectators: 0,
            state: RoomState::Gathering,
        };
        let json = serde_json::to_value(&gathering).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "room_id": "r0",
                "config": { "seats": 2, "game_setup": "standard_2p" },
                "filled": 1,
                "state": "gathering"
            })
        );
        assert_eq!(
            serde_json::from_value::<RoomSummary>(json).unwrap(),
            gathering
        );

        // The started state tags as `in_progress`.
        let in_progress = RoomSummary {
            state: RoomState::InProgress,
            filled: 2,
            ..gathering.clone()
        };
        let json = serde_json::to_value(&in_progress).unwrap();
        assert_eq!(json["state"], serde_json::json!("in_progress"));
        assert_eq!(
            serde_json::from_value::<RoomSummary>(json).unwrap(),
            in_progress
        );
    }

    #[test]
    fn lobby_view_directory_round_trips_and_elides_when_empty() {
        // Issue #280: the room directory rides on `LobbyView`, round-trips populated,
        // and elides from the wire when there are no rooms.
        let mut view = LobbyView {
            session: "s:ab12".into(),
            you: "p1".into(),
            name: None,
            room: None,
            directory: vec![],
            valid_commands: vec!["create_room".into(), "join_room".into()],
        };
        // Empty directory: the field elides entirely.
        assert!(serde_json::to_value(&view)
            .unwrap()
            .get("directory")
            .is_none());

        // Populated: a gathering room and an in-progress room both survive the trip.
        view.directory = vec![
            RoomSummary {
                room_id: "r0".into(),
                config: RoomConfig {
                    seats: 2,
                    game_setup: "standard_2p".into(),
                },
                filled: 1,
                spectators: 0,
                state: RoomState::Gathering,
            },
            RoomSummary {
                room_id: "r1".into(),
                config: RoomConfig {
                    seats: 4,
                    game_setup: "ffa-4".into(),
                },
                filled: 4,
                spectators: 2,
                state: RoomState::InProgress,
            },
        ];
        let back: LobbyView = serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert_eq!(back, view);
        assert_eq!(back.directory[0].state, RoomState::Gathering);
        assert_eq!(back.directory[1].state, RoomState::InProgress);
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
        };
        let json = serde_json::to_value(&view).unwrap();
        // The receiver's own seat id is always present on the wire (like `phase`),
        // not elided the way empty collections are.
        assert_eq!(json.get("you"), Some(&serde_json::json!("p1")));
        let back: GameView = serde_json::from_value(json).unwrap();
        assert_eq!(back.you, "p1");
    }

    #[test]
    fn set_name_command_round_trips() {
        // Issue #294: the display-name command is a tagged lobby command carrying the
        // requested name verbatim; the server validates it before storing.
        let msg = LobbyCommand::SetName(SetName {
            name: "Alice".into(),
        });
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "type": "set_name", "name": "Alice" })
        );
        let back: LobbyCommand = serde_json::from_value(json).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn seat_view_name_round_trips_and_elides_when_absent() {
        // Issue #294: a named occupant's display name rides in the roster and
        // round-trips; an unnamed (or empty) seat omits it entirely.
        let named = SeatView {
            seat: 0,
            occupied_by: Some("p1".into()),
            name: Some("Alice".into()),
            decked: true,
            ready: false,
        };
        let json = serde_json::to_value(&named).unwrap();
        assert_eq!(json.get("name"), Some(&serde_json::json!("Alice")));
        assert_eq!(serde_json::from_value::<SeatView>(json).unwrap(), named);

        let unnamed = SeatView {
            name: None,
            ..named.clone()
        };
        let json = serde_json::to_value(&unnamed).unwrap();
        assert!(json.get("name").is_none());
    }

    #[test]
    fn lobby_view_name_round_trips_and_elides_when_absent() {
        // Issue #294: the connection's own display name rides on the lobby view (so the
        // pre-game UI can show it before a seat exists) and elides when unset.
        let mut view = LobbyView {
            session: "s:ab12".into(),
            you: "p1".into(),
            name: Some("Alice".into()),
            room: None,
            directory: vec![],
            valid_commands: vec!["set_name".into(), "create_room".into()],
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json.get("name"), Some(&serde_json::json!("Alice")));
        assert_eq!(serde_json::from_value::<LobbyView>(json).unwrap(), view);

        view.name = None;
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("name").is_none());
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
