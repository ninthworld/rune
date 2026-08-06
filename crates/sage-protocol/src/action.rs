//! The interactivity contract: the [`ValidAction`]s the server offers and the
//! target/choice [`Prompt`]s they pose (docs/decisions/0004-targeting-model.md).

use serde::{Deserialize, Serialize};

use crate::{ActionDestination, EntityId, PlayerId};

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
/// Both are decided in docs/decisions/0004-targeting-model.md (§Protocol).
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
    ///. Omitted from the wire when `false`.
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
    /// content-binding [`token`](ValidAction::token) (reject-stale, ADR 0004).
    /// Omitted from the wire when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<Prompt>,
    /// The server-authoritative **destinations** this action may be taken to (issue
    /// #554) — see [`ActionDestination`]. The complete set of drop regions a
    /// direct-manipulation gesture may offer for this action, and the *only* source
    /// of them: a client derives its drop targets from this list alone and **fails
    /// closed**, offering no drop target at all for an action that names none.
    ///
    /// Deliberately separate from [`subject`](Self::subject), which names what the
    /// action belongs *to* (the card it renders on); this names where it goes.
    /// Omitted from the wire when empty, which is the common case — most actions
    /// (passing, conceding, a mana ability) are a click, not a drag, and a client
    /// that ignores this field loses nothing, since drag is optional input and every
    /// action stays reachable by click, keyboard, and touch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub destinations: Vec<ActionDestination>,
    /// What this action **costs in mana**, printed and as the game has it now (CR
    /// 601.2f) — see [`ActionCost`]. Present on a cast; omitted for everything else.
    ///
    /// The client renders what a spell costs and computes no cost of its own, so a cost
    /// the game has changed has to arrive as a number rather than as something to work
    /// out from a reducer on the board. Both halves ride together because the
    /// presentation is a comparison: the modified cost is what a player pays and the
    /// printed one is what the card still says, and neither is legible as a change
    /// without the other.
    ///
    /// Additive: omitted from the wire for every action that is not a cast, and a client
    /// that ignores it renders exactly what it always did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<ActionCost>,
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

/// What a cast costs in mana: the cost on the card, and the cost the game will actually
/// charge (CR 601.2f).
///
/// The two are the same string for nearly every cast, and a client may draw the modified
/// one unconditionally. They differ when a cost-modification effect is in force — a
/// permanent that makes a class of spells cheaper or dearer — and then the difference is
/// the whole point: the card keeps its printed cost, and the surface a player acts on
/// carries the modified one, marked against the printed one beside it.
///
/// Both are `{...}` notation, the same symbols [`CardView::mana_cost`](crate::CardView)
/// uses. Display text: a client matches the symbols it can draw and never parses one for
/// a value — the arithmetic that produced the modified cost is the server's, and a client
/// that reproduced it would be computing cost.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCost {
    /// The cost printed on the card, e.g. `"{4}{G}"`. Empty for a card with no printed
    /// mana cost.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub printed: String,
    /// The cost this cast is offered and charged at, e.g. `"{2}{G}"` — the printed cost
    /// plus the commander tax where one applies (CR 903.8), after every cost
    /// modification. `"{0}"` for a cost reduced to nothing, which is a real cost and not
    /// an absent one.
    pub modified: String,
}

/// One choice step of a multi-step [`ValidAction`]: a single target slot the
/// player must fill, listing exactly the legal candidates the server computed.
/// The client renders the prompt, highlights the candidates, and computes no
/// legality of its own (docs/decisions/0004-targeting-model.md §Client).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetRequirement {
    /// Stable slot id the client echoes back as [`TargetChoice::slot`] to key its
    /// answer to this step. Opaque; the client never parses it.
    pub slot: String,
    /// Human-readable prompt describing what to choose, e.g. `"target creature"`.
    pub prompt: String,
    /// Whether this slot may be left **unanswered** — the "up to" of "put a +1/+1
    /// counter on each of up to two target creatures" (CR 601.2c).
    ///
    /// `false` for every slot of an ordinary targeted spell or ability, which must be
    /// filled or the submission is rejected. An effect that may name fewer targets than
    /// it allows is advertised as its maximum number of slots, of which the ones past its
    /// minimum carry this; the client omits them from its answer, or sends them empty,
    /// and the server accepts either.
    ///
    /// Additive: omitted from the wire when `false`, so an older client that ignores it
    /// sees exactly the contract it always saw — and a server that never sets it is
    /// indistinguishable from the one before this field existed.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub optional: bool,
    /// The legal candidate entity ids for this slot — the **only** choices the
    /// client may offer. Enumerated O(N) per slot, never the cartesian product of
    /// combinations across slots (docs/decisions/0004-targeting-model.md
    /// §Enumeration).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<EntityId>,
    /// The [`candidates`](Self::candidates) that answering this slot with them would
    /// **tap** — the attackers in a declaration that are not vigilant (CR 508.1f,
    /// CR 702.20b).
    ///
    /// A declaration is assembled a creature at a time and nothing is sent until it is
    /// confirmed, so a client showing what the choice does has to know what the choice
    /// does. Vigilance is a keyword judgment and a client must make none, so the server
    /// states the answer per candidate and the client turns the cards it is told to turn —
    /// and turns them back when they come out of the slot, since nothing has happened yet.
    ///
    /// A subset of `candidates`, in the same order, and **empty for every slot whose
    /// answer taps nothing**: an ordinary spell's target slot, a blocker assignment
    /// (blocking does not tap, CR 509.1), the defender slots of a declaration.
    ///
    /// Additive: omitted when empty, and a client that ignores it renders exactly what it
    /// always did.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taps: Vec<EntityId>,
    /// The entity this slot is **about**, when it is about one (issue #700).
    ///
    /// A combat declaration is several slots that all list the same candidates and
    /// differ only in whose choice they are: one `defend_*` slot per attacker naming
    /// what *that* attacker attacks (CR 508.1a), one `block_*` slot per attacker
    /// naming what blocks *it* (CR 509.1a). The slot ids already encode it, and every
    /// client that wanted to say "this attacker is attacking that seat" had to parse
    /// them to find out — which the slot id's own contract forbids ("Opaque; the
    /// client never parses it").
    ///
    /// So it is stated. With it a client can ask the choices one subject at a time,
    /// draw the arrow from the attacker it belongs to, and show a per-attacker slot
    /// only once that attacker is in the declaration — none of which is a rules
    /// judgment, because the pairing is the server's and this only publishes it.
    ///
    /// `None`/omitted for a slot that is about the action as a whole — an ordinary
    /// spell's target slot, the `attackers` multi-select — which is every slot that
    /// existed before this field. Additive in both directions: a server that never
    /// sets it is indistinguishable from the one before it, and a client that ignores
    /// it sees exactly the contract it always saw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<EntityId>,
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
    /// The action's other slots **this choice** owes an answer to (issue #451): a
    /// mulligan's *keep* requires the `bottom` slot filled to its exact `count`,
    /// while *mulligan* requires nothing. Without it a client cannot tell which
    /// named choice owes which slot, so it either blocks a legal choice or offers
    /// one the server must reject. This states a coupling the server already
    /// enforces on resolution; the client only enables or disables the choice
    /// accordingly and derives no legality of its own. Empty for a self-contained
    /// choice, and omitted from the wire then.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

/// One legal value of a [`Prompt::Number`] slot, and what choosing it costs.
///
/// This exists because of X. `min` and `max` describe a *range*, and a range is enough
/// for a number that costs nothing — how many counters to remove, how much of a divided
/// effect goes where. The value of X in a mana cost is not that: choosing it changes what
/// the spell costs, and **the client may not work out what a spell costs** (`AGENTS.md`,
/// zero game logic in the client). So the server states each value's cost outright rather
/// than sending `{X}{R}` and leaving a multiplication to whoever draws the bar.
///
/// A client walks these as the stepper's stops (`docs/client-design.md` §6.7): the
/// current value, a decrement and an increment that move along the list and stop at its
/// ends, and this entry's `cost` shown beside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberValue {
    /// The value itself — one of the numbers between the slot's `min` and `max`.
    pub value: u32,
    /// What the action costs at this value, in printed `{...}` notation (`"{3}{R}"`).
    ///
    /// The whole cost, never a delta and never a cost with an `X` still in it. Display
    /// text and the input to nothing: a client renders the symbols and compares nothing.
    /// Omitted from the wire for a number that costs nothing, which is every
    /// [`Prompt::Number`] that is not an X in a cost.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cost: String,
}

/// One way to pay one pip of a cost: a permanent to tap and what tapping it that way
/// produces (CR 601.2f–g).
///
/// The `source` is what a player clicks and the `id` is what the client sends back, and
/// they are **deliberately not the same field**. A permanent with more than one mana
/// ability — every dual land is `{T}: Add {W}` *and* `{T}: Add {U}` — appears in a slot's
/// candidates once per ability it could pay that pip with, each with its own `id` and its
/// own [`label`](Self::label). So "which color did you mean?" is a question the client can
/// see it needs to ask, without knowing anything about mana: **ask when the same `source`
/// appears more than once for the slot being filled, and offer the labels.**
///
/// Where the choice would not matter, the server does not offer it. A generic pip is paid
/// equally well by either half of a dual land, so it lists the permanent once and the
/// player is never asked a question with one meaningful answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaOption {
    /// Opaque id the client echoes back as this slot's chosen value. Never parsed — it
    /// identifies an activation, and how is the server's business.
    pub id: String,
    /// The permanent this taps: the entity a player clicks on the board.
    pub source: EntityId,
    /// What this option produces, e.g. `"{W}"` — the label for the disambiguating
    /// question, and never anything the client computes a cost from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Whether taking this option **taps** its [`source`](Self::source) — the `{T}` in
    /// `{T}: Add {G}` (CR 602.2a).
    ///
    /// A payment is assembled a source at a time and sends nothing until the cast is
    /// confirmed, so the board a player is looking at while they choose is one the server
    /// has not been told about yet. Stating this lets the client turn each source as it is
    /// spent and turn it back when it is taken out — the same thing that will happen for
    /// real when the cast goes in — without deciding it: a mana ability that sacrifices
    /// its source, or costs life, taps nothing, and no client can tell which is which
    /// without reading the cost.
    ///
    /// Additive: omitted when `false`. A client that ignores it draws the board it always
    /// drew, and a server that never sets it is indistinguishable from the one before this
    /// field existed.
    #[serde(default, skip_serializing_if = "crate::is_false")]
    pub taps: bool,
}

/// A non-target choice slot a [`ValidAction`] may pose, a **generalization of the
/// [`TargetRequirement`] slot pattern** (slot + prompt + candidates, bound by the
/// action's content [`token`](ValidAction::token), ADR 0004) to three further
/// shapes the engine already needs to pose:
///
/// - [`Prompt::Option`] — pick exactly one of N named choices (also the clean
///   shape for a yes/no such as the mulligan keep/take-another decision).
/// - [`Prompt::SelectFromZone`] — pick `count` cards from a zone (cleanup
///   discard-to-max, mulligan bottoming, future tutors).
/// - [`Prompt::Order`] — arrange N items into an order (ordering simultaneous
///   triggers, scry).
/// - [`Prompt::Number`] — choose a number in a server-stated range (the value of
///   X, how much of a divided effect goes where).
///
/// Every shape shares the same discipline as a target requirement: the server
/// enumerates the only legal choices, the client renders them and derives nothing,
/// and the answer is one [`TargetChoice`] keyed by `slot` and submitted
/// **atomically** in a single [`ChooseAction`]. The action's content-binding
/// `token` folds in every prompt, so a stale/redirected answer whose prompt content
/// has changed is rejected (ADR 0004 stale-view protection). The `kind` tag
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
    /// Pick between [`min`](Prompt::SelectFromZone::min) and
    /// [`count`](Prompt::SelectFromZone::count) entity ids from a zone. The slot is
    /// answered with the selected ids in [`TargetChoice::chosen`]; each must be one of
    /// [`candidates`](Prompt::SelectFromZone::candidates), and the answer's *order* is
    /// preserved (it is the order a scry puts cards on the bottom in).
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
        /// The **most** ids that may be chosen; with [`min`](Self::SelectFromZone::min)
        /// absent, also exactly how many must be.
        count: u32,
        /// The **fewest** ids that may be chosen, when that differs from `count`.
        ///
        /// Absent means the selection is exact — the shape every prompt before this had
        /// and the one a bottoming or a cleanup discard still has. It is present for a
        /// choice a player may legally answer with less than the maximum: scrying *any
        /// number* of the cards looked at, taking *up to* one of them, or failing to
        /// find on a search (CR 701.19c). Omitted from the wire when absent, so an
        /// exact-count prompt serializes exactly as before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<u32>,
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
    /// Choose a **number** within the server-stated inclusive range
    /// [`min`](Prompt::Number::min)..=[`max`](Prompt::Number::max) — the value of X
    /// in a cost, how many counters to remove, how much of a divided effect goes to
    /// one recipient (issue #554).
    ///
    /// The slot is answered with the chosen number rendered as a decimal string, as
    /// the single entry of [`TargetChoice::chosen`] (e.g. `["3"]`). It shares
    /// [`TargetChoice`] with every other slot kind rather than adding a parallel
    /// numeric answer channel, so one atomic [`ChooseAction`] still answers a whole
    /// action and the content [`token`](ValidAction::token) still binds every slot.
    ///
    /// **The bounds are the server's**, computed from available mana, the source's
    /// text, and the game state; the client offers a control over exactly this range
    /// and computes no affordability of its own. A *divided* value is posed as one
    /// `number` slot per recipient, each with its own bounds, and the server
    /// validates the total on resolution — the client never enforces a sum.
    Number {
        /// Stable slot id the client echoes back as [`TargetChoice::slot`].
        slot: String,
        /// Human-readable prompt describing what the number means, e.g. `"Choose a
        /// value for X"`.
        prompt: String,
        /// The smallest legal value, inclusive. Often `0` (X may be zero); always
        /// serialized, so the range reads completely rather than by inference.
        min: u32,
        /// The largest legal value, inclusive. Always serialized.
        max: u32,
        /// Every legal value **and what choosing it costs** (see [`NumberValue`]) —
        /// present exactly when the number is the X of a mana cost.
        ///
        /// The server enumerates them because working out what `{X}{R}` costs at X = 3
        /// is deciding what a spell costs, which no client may do. When present this
        /// list, not the range, is the set of stops a stepper walks; the two agree, and
        /// `min`/`max` remain so a client that ignores this field still sees a range it
        /// understands.
        ///
        /// Additive: omitted when empty, so a costless number slot serializes exactly as
        /// it did before this field existed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        values: Vec<NumberValue>,
    },
    /// Pay **one pip** of a mana cost by tapping something (CR 601.2f–g).
    ///
    /// One slot per unit of the cost: `{1}{W}` is posed as two of these, and a cast that
    /// needs no mana poses none. That is the whole design, and the reason for it is what
    /// it lets a client do without knowing any rules — **the "still to pay" line is the
    /// unfilled slots**, drawn from their [`pip`](Prompt::PayMana::pip) symbols. Filling
    /// one removes a pip; taking it back out puts it back. Nothing subtracts a cost from
    /// anything, which is exactly the arithmetic a client must never do (hybrid pips,
    /// restricted mana and cost reduction are the server's to reason about, and a client
    /// that got any of them right today would get the next one wrong).
    ///
    /// It also makes "can this be cast yet" a slot-counting question: every mandatory slot
    /// filled means the cost is covered, which is the same test that already enables a
    /// submit for a targeted spell.
    ///
    /// The slot is answered with one [`ManaOption::id`] as its single `chosen` entry.
    /// Sources are **not** shared between slots: a permanent can be tapped once, so a
    /// client must not offer one already spent on another slot of the same action, and a
    /// submission that names it twice is rejected.
    ///
    /// A cast paid out of mana already floating (CR 605.3) poses **no** pay-mana slots at
    /// all — the pool covers it and there is nothing to choose.
    PayMana {
        /// Stable slot id the client echoes back as [`TargetChoice::slot`].
        slot: String,
        /// Human-readable prompt for this pip, e.g. `"Pay {W}"`.
        prompt: String,
        /// The mana symbol this slot pays, e.g. `"{W}"` or `"{1}"` — the pip a client
        /// draws in the running cost line, and the *only* thing it needs in order to show
        /// what is left to pay. Display text: matched against the symbols a client can
        /// render and never parsed for a value.
        pip: String,
        /// The ways this pip may be paid — the **only** answers the client may submit.
        /// Enumerated O(sources) per slot, never the combinations across slots.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        candidates: Vec<ManaOption>,
    },
}

#[cfg(test)]
mod tests;
