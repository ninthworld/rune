//! The two choices a spell's controller makes **as it is announced** (CR 601.2b), as
//! wire prompts: the mode, and the value of X.
//!
//! Split out of the action projection because they are one cohesive thing — the
//! announcement — and because both have the same unusual shape: a question with no object
//! on the board to point at, which `docs/client-design.md` §6.5 sends to the dock. The
//! prompt kinds they ride already existed; nothing here invents a wire shape.

use super::*;

/// The slot a modal cast's mode is chosen on (CR 700.2).
pub(crate) const MODE_SLOT: &str = "mode";

/// The slot a cast's value for X is announced on (CR 601.2b).
pub(crate) const X_SLOT: &str = "x";

/// The option id for mode `index` — recomputed and matched, never parsed.
pub(crate) fn mode_option_id(index: u8) -> String {
    format!("mode_{index}")
}

/// The **announcement** slots of a cast, in the order CR 601.2 makes the choices:
/// the mode, then X.
///
/// Both are dock controls rather than things on the board — there is no object to point
/// at for either (`docs/client-design.md` §6.7) — so both ride the prompt shapes that
/// already exist for a question with no subject.
///
/// The mode is a [`Prompt::Option`], and each of its options **names the target slots
/// that mode owes** in `requires`. That is the same mechanism the mulligan keep uses for
/// its bottoming, and it is what lets a client offer both modes honestly: it can tell
/// which slots to ask for once a mode is picked, without knowing anything about what
/// either mode does.
///
/// X is a [`Prompt::Number`] carrying **the legal values and what each one costs**. The
/// range comes with them and agrees with them; the costs are there because multiplying
/// `{X}{R}` out is deciding what a spell costs and no client may do that.
pub(crate) fn announcement_prompts(
    state: &GameState,
    db: &CardDatabase,
    action: &Action,
) -> Vec<Prompt> {
    let mut prompts = Vec::new();
    if let Some(modes) = modal_cast_modes(state, db, action) {
        let name = match action {
            Action::CastSpell { card, .. } => card_name(card.card, db),
            _ => String::new(),
        };
        prompts.push(Prompt::Option {
            slot: MODE_SLOT.to_string(),
            prompt: "Choose one".to_string(),
            options: modes
                .iter()
                .map(|mode| PromptOption {
                    id: mode_option_id(mode.index),
                    // The mode's own generated sentence, the same words the card prints
                    // for that bullet — a player choosing a mode reads what they are
                    // choosing, not "Mode 2".
                    label: crate::rules_text::mode_text(
                        &name,
                        &sage_engine::SpellMode {
                            effects: mode.effects.clone(),
                        },
                    ),
                    requires: (0..sage_engine::target_requirements(
                        state,
                        db,
                        &cast_with_announcement(action, Some(mode.index), None),
                    )
                    .len())
                        .map(|index| target_slot(Some(mode.index), index))
                        .collect(),
                })
                .collect(),
        });
    }
    let values = sage_engine::x_options(state, db, action);
    if !values.is_empty() {
        prompts.push(Prompt::Number {
            slot: X_SLOT.to_string(),
            prompt: "Choose a value for X".to_string(),
            min: values.first().map_or(0, |option| option.value),
            max: values.last().map_or(0, |option| option.value),
            values: values
                .into_iter()
                .map(|option| NumberValue {
                    value: option.value,
                    cost: option.cost,
                })
                .collect(),
        });
    }
    prompts
}
