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
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)] // panics are the failure signal in tests
mod tests {
    use crate::*;

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
            destinations: vec![],
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
        // `mana_ability` rides the wire only when true; a legacy
        // payload without the key deserializes to `false`.
        let tap = ValidAction {
            mana_ability: true,
            id: "a2".into(),
            kind: "activate_ability".into(),
            label: "{T}: Add {G}.".into(),
            subject: vec!["perm_1".into()],
            requirements: vec![],
            prompts: vec![],
            destinations: vec![],
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
                // A bolt's one slot is mandatory, so the flag elides from the wire —
                // the assertion below is the proof that an older client sees no change.
                optional: false,
                candidates: vec!["perm_bear".into(), "p1".into(), "p2".into()],
            }],
            prompts: vec![],
            destinations: vec![],
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
        // `kind: "option"` on the wire, answered with the chosen option id. A choice
        // that owes another slot names it in `requires` (issue #451); one that owes
        // nothing omits the field entirely.
        let prompt = Prompt::Option {
            slot: "decision".into(),
            prompt: "Keep this hand or take a mulligan?".into(),
            options: vec![
                PromptOption {
                    id: "keep".into(),
                    label: "Keep this hand".into(),
                    requires: vec!["bottom".into()],
                },
                PromptOption {
                    id: "mulligan".into(),
                    label: "Mulligan".into(),
                    requires: vec![],
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
                    { "id": "keep", "label": "Keep this hand", "requires": ["bottom"] },
                    { "id": "mulligan", "label": "Mulligan" }
                ]
            })
        );
        let back: Prompt = serde_json::from_value(json).unwrap();
        assert_eq!(back, prompt);
    }

    #[test]
    fn legacy_option_without_requires_deserializes_as_self_contained() {
        // A payload from a server that predates the coupling field omits it; the
        // choice must default to "owes no other slot" rather than failing to decode.
        let json = r#"{ "id": "keep", "label": "Keep this hand" }"#;
        let option: PromptOption = serde_json::from_str(json).unwrap();
        assert!(option.requires.is_empty());
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
            min: None,
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
    fn issue_604_a_select_from_zone_states_a_lower_bound_only_when_it_has_one() {
        // A choice a player may legally under-fill (scry any number, take up to one,
        // fail to find) carries `min` alongside the maximum...
        let scry = Prompt::SelectFromZone {
            slot: "choice".into(),
            prompt: "Choose up to 2 cards to put on the bottom of your library".into(),
            zone: "library".into(),
            owner: "p0".into(),
            count: 2,
            min: Some(0),
            candidates: vec!["card_1".into(), "card_2".into()],
        };
        let json = serde_json::to_value(&scry).unwrap();
        assert_eq!(json["min"], serde_json::json!(0));
        assert_eq!(json["count"], serde_json::json!(2));
        assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), scry);

        // ...and an exact one elides it, so an existing bottoming or cleanup discard
        // serializes byte-for-byte as it always did.
        let exact = Prompt::SelectFromZone {
            slot: "discard".into(),
            prompt: "Choose a card to discard".into(),
            zone: "hand".into(),
            owner: "p0".into(),
            count: 1,
            min: None,
            candidates: vec!["card_1".into()],
        };
        assert!(serde_json::to_value(&exact).unwrap().get("min").is_none());

        // A payload from a server that predates the field reads as exact, not as
        // "at least zero" — the safe direction, since it is the shape that was meant.
        let legacy: Prompt = serde_json::from_str(
            r#"{"kind":"select_from_zone","slot":"discard","prompt":"x","zone":"hand","owner":"p0","count":1}"#,
        )
        .unwrap();
        let Prompt::SelectFromZone { min, count, .. } = legacy else {
            panic!("a select_from_zone");
        };
        assert_eq!((min, count), (None, 1));
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
    fn issue_554_number_prompt_round_trips_and_tags_its_kind() {
        // `number` (X, a divided value): a slot carrying the server's inclusive
        // bounds, answered with the chosen value as a decimal string in the same
        // `TargetChoice` shape every other slot kind uses.
        let prompt = Prompt::Number {
            slot: "x".into(),
            prompt: "Choose a value for X".into(),
            min: 0,
            max: 4,
        };
        let json = serde_json::to_value(&prompt).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "kind": "number",
                "slot": "x",
                "prompt": "Choose a value for X",
                "min": 0,
                "max": 4
            })
        );
        assert_eq!(serde_json::from_value::<Prompt>(json).unwrap(), prompt);

        // Both bounds always ride the wire — a zero `min` is not elided, so the range
        // reads completely rather than by inference.
        let one_only = serde_json::to_value(Prompt::Number {
            slot: "n".into(),
            prompt: "How many?".into(),
            min: 1,
            max: 1,
        })
        .unwrap();
        assert_eq!(one_only["min"], 1);
        assert_eq!(one_only["max"], 1);

        // The answer is the numeral as a string, in the shared slot-answer shape.
        let answer = TargetChoice {
            slot: "x".into(),
            chosen: vec!["3".into()],
        };
        assert_eq!(
            serde_json::to_value(&answer).unwrap(),
            serde_json::json!({ "slot": "x", "chosen": ["3"] })
        );
    }

    #[test]
    fn issue_554_destinations_ride_the_action_and_elide_when_it_names_none() {
        // A cast names the stack; the client derives its drop region from this alone.
        let cast = ValidAction {
            id: "a3".into(),
            kind: "cast_spell".into(),
            label: "Cast Lightning Bolt".into(),
            subject: vec!["c3".into()],
            destinations: vec![ActionDestination {
                kind: "zone".into(),
                id: "stack".into(),
                owner: String::new(),
                label: "Stack".into(),
            }],
            ..Default::default()
        };
        let json = serde_json::to_value(&cast).unwrap();
        assert_eq!(
            json["destinations"],
            serde_json::json!([{ "type": "zone", "id": "stack", "label": "Stack" }])
        );
        assert_eq!(serde_json::from_value::<ValidAction>(json).unwrap(), cast);

        // An action with nowhere to go elides the field entirely, so a client has no
        // drop target at all for it — the fail-closed default.
        let pass = ValidAction {
            id: "a1".into(),
            kind: "pass_priority".into(),
            label: "Pass".into(),
            ..Default::default()
        };
        assert!(serde_json::to_value(&pass)
            .unwrap()
            .get("destinations")
            .is_none());

        // A payload from a server that predates the field decodes to no destinations,
        // which reads the same way: no drop target.
        let legacy: ValidAction =
            serde_json::from_str(r#"{ "id": "a1", "type": "pass_priority", "label": "Pass" }"#)
                .unwrap();
        assert!(legacy.destinations.is_empty());
    }

    #[test]
    fn issue_554_submission_correlates_an_answer_with_its_acknowledgement() {
        // The client puts an opaque id on the message...
        let msg = ChooseAction {
            action_id: "a2".into(),
            submission: "s:17".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&msg).unwrap(),
            serde_json::json!({ "action_id": "a2", "submission": "s:17" })
        );

        // ...and the server echoes it back verbatim, with its verdict.
        let ack = ActionAck {
            submission: "s:17".into(),
            accepted: true,
        };
        assert_eq!(ack.submission, msg.submission);

        // A client that does not correlate sends exactly the message it always sent,
        // and an older client's message still decodes (to no correlation id).
        let plain = ChooseAction {
            action_id: "a2".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::json!({ "action_id": "a2" })
        );
        let legacy: ChooseAction = serde_json::from_str(r#"{"action_id":"a2"}"#).unwrap();
        assert!(legacy.submission.is_empty());
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
                            requires: vec!["bottom".into()],
                        },
                        PromptOption {
                            id: "mulligan".into(),
                            label: "Mulligan".into(),
                            requires: vec![],
                        },
                    ],
                },
                Prompt::SelectFromZone {
                    slot: "bottom".into(),
                    prompt: "Bottom 1 card".into(),
                    zone: "hand".into(),
                    owner: "p0".into(),
                    count: 1,
                    min: None,
                    candidates: vec!["card_1".into(), "card_2".into()],
                },
            ],
            destinations: vec![],
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
            submission: String::new(),
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
    fn issue_604_choice_contract_fixture_round_trips_and_matches_typed_fields() {
        // Cross-language contract fixture: a mid-resolution scry. Its `player_choice`
        // action carries one `select_from_zone` whose bounds are a *range*, the cards
        // it asks about ride the receiver-only `revealed` channel, and the log carries
        // the two count-only events this work adds. The web client's `protocol.test.ts`
        // consumes these exact bytes.
        let json = include_str!("../fixtures/gameview-choice.json");
        let view: GameView = serde_json::from_str(json).unwrap();
        let reencoded = serde_json::to_string(&view).unwrap();
        assert_eq!(serde_json::from_str::<GameView>(&reencoded).unwrap(), view);

        let choice = &view.valid_actions[0];
        assert_eq!(choice.kind, "player_choice");
        assert!(!choice.token.is_empty(), "a prompt action is token-bound");
        let Prompt::SelectFromZone {
            slot,
            zone,
            owner,
            count,
            min,
            candidates,
            ..
        } = &choice.prompts[0]
        else {
            panic!("the choice is a select_from_zone");
        };
        assert_eq!(slot, "choice");
        assert_eq!(zone, "library");
        assert_eq!(owner, "p0");
        assert_eq!((*count, *min), (2, Some(0)), "any number of the two");
        assert_eq!(candidates, &["card_20".to_string(), "card_21".to_string()]);

        // The cards the choice is about are shown to this receiver alone, by ids the
        // prompt's candidates name — so a client can render what it is being asked.
        assert_eq!(
            view.revealed
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            candidates.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        // Both new log events carry counts and seats, never card identities.
        assert!(matches!(
            view.log[0].event,
            GameLogEvent::CardsDiscarded { count: 2, .. }
        ));
        assert!(matches!(
            view.log[1].event,
            GameLogEvent::LibrarySearched { .. }
        ));
    }

    #[test]
    fn issue_610_optional_effect_contract_fixture_round_trips_and_matches_typed_fields() {
        // Cross-language contract fixture: the yes-or-no of an optional effect. It adds
        // no wire shape — the question rides the `option` prompt the mulligan decision
        // already uses — so what this pins is the *composition*: a `player_choice`
        // carrying an option slot, the mana ability offered beside it (CR 605.3a), and
        // the two seat-only log events. The web client's `protocol.test.ts` consumes
        // these exact bytes.
        let json = include_str!("../fixtures/gameview-optional.json");
        let view: GameView = serde_json::from_str(json).unwrap();
        let reencoded = serde_json::to_string(&view).unwrap();
        assert_eq!(serde_json::from_str::<GameView>(&reencoded).unwrap(), view);

        let choice = &view.valid_actions[0];
        assert_eq!(choice.kind, "player_choice");
        assert!(!choice.token.is_empty(), "a prompt action is token-bound");
        let Prompt::Option {
            slot,
            prompt,
            options,
        } = &choice.prompts[0]
        else {
            panic!("the yes-or-no is an option prompt");
        };
        assert_eq!(slot, "choice");
        assert_eq!(prompt, "Pay {1} to draw a card?");
        assert_eq!(
            options.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
            ["accept", "decline"],
        );
        // Neither choice owes another slot: a yes-or-no is self-contained.
        assert!(options.iter().all(|option| option.requires.is_empty()));

        // The seat is asked to pay, so the mana it could pay with is on offer too.
        assert!(view.valid_actions[1].mana_ability);

        // Both new log events name a seat and nothing else — never what was offered,
        // and never the pool that could or could not afford it.
        assert!(matches!(
            view.log[0].event,
            GameLogEvent::OptionalApplied { .. }
        ));
        assert!(matches!(
            view.log[1].event,
            GameLogEvent::OptionalDeclined { .. }
        ));
        // A yes-or-no shows nobody any cards: there is no revealed channel on this view.
        assert!(view.revealed.is_empty());
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
        // Keeping owes the bottoming slot; taking another hand owes nothing (#451).
        assert_eq!(options[0].requires, ["bottom".to_string()]);
        assert!(options[1].requires.is_empty());

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
}
