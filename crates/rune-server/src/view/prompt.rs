//! Prompt projection and the content-binding token.

use super::*;

/// The content-binding token for an action, hashed from the exact content the
/// client is answering: its `kind`, `subject`, `requirements` (target slots), and
/// `prompts` (the option/select-from-zone/order slots, issue #156). ADR 0009
/// §Protocol specifies a hash/echo of the content — not a random nonce — so the
/// server stays stateless: it never stores a per-id secret, it recomputes the token
/// from the freshly regenerated action. Two actions with different content therefore
/// hash to different tokens, which is what lets [`resolve_action`] reject a stale or
/// redirected id whose token no longer matches — for a prompt-bearing action just as
/// for a targeted one.
pub(crate) fn content_token(
    kind: &str,
    subject: &[String],
    requirements: &[TargetRequirement],
    prompts: &[Prompt],
) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    subject.hash(&mut hasher);
    // `TargetRequirement` intentionally does not derive `Hash` (it is a wire type),
    // so fold its fields in explicitly, length-prefixed to stay unambiguous.
    requirements.len().hash(&mut hasher);
    for req in requirements {
        req.slot.hash(&mut hasher);
        req.prompt.hash(&mut hasher);
        req.candidates.hash(&mut hasher);
    }
    // `Prompt` is likewise a non-`Hash` wire enum; fold each variant's tag and fields
    // in explicitly so a change to any prompt content re-derives a different token.
    prompts.len().hash(&mut hasher);
    for prompt in prompts {
        hash_prompt(prompt, &mut hasher);
    }
    format!("t{:016x}", hasher.finish())
}

/// Fold one wire [`Prompt`] into `hasher` for [`content_token`]: a per-variant tag
/// byte followed by its fields, length-prefixed where variable, so two prompts that
/// differ anywhere hash differently.
fn hash_prompt(prompt: &Prompt, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    match prompt {
        Prompt::Option {
            slot,
            prompt,
            options,
        } => {
            0u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            options.len().hash(hasher);
            for option in options {
                option.id.hash(hasher);
                option.label.hash(hasher);
            }
        }
        Prompt::SelectFromZone {
            slot,
            prompt,
            zone,
            owner,
            count,
            candidates,
        } => {
            1u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            zone.hash(hasher);
            owner.hash(hasher);
            count.hash(hasher);
            candidates.hash(hasher);
        }
        Prompt::Order {
            slot,
            prompt,
            items,
        } => {
            2u8.hash(hasher);
            slot.hash(hasher);
            prompt.hash(hasher);
            items.hash(hasher);
        }
    }
}

/// The opaque wire entity id naming the specific game object an engine [`Target`]
/// points at, reusing the same per-instance id scheme every other action uses
/// ([`card_entity_id`]/[`permanent_entity_id`]/[`player_id`]). This is what makes a
/// projected candidate — and a returned selection — name one unambiguous object.
pub(crate) fn target_entity_id(target: Target) -> String {
    match target {
        Target::Player(seat) => player_id(seat),
        Target::Permanent(id) => permanent_entity_id(id),
        Target::Card(id) => card_entity_id(id),
        Target::Spell(id) => stack_entity_id(id),
    }
}

/// The human-readable prompt for an ability-target slot's [`TargetSpec`]. Kept
/// exhaustive so a new spec forces a matching wire prompt here.
pub(crate) fn target_spec_prompt(spec: TargetSpec) -> &'static str {
    match spec {
        TargetSpec::AnyPlayer => "Choose target player",
        TargetSpec::AnyPermanent => "Choose target permanent",
        TargetSpec::AnyCreature => "Choose target creature",
        TargetSpec::SpellOnStack => "Choose target spell",
        TargetSpec::AnyTarget => "Choose any target",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::test_support::{cleanup_over_hand_limit, state_with_hand};

    /// Every emitted action carries a non-empty content-binding token, and the
    /// token is a function of the action's content: two actions of the same kind
    /// that name different subjects hash to different tokens. This is what lets a
    /// stale positional id be caught when its action content changes.
    #[test]
    fn every_action_carries_a_content_bound_token() {
        let db = CardDatabase::bundled().unwrap();
        let (state, _) = state_with_hand(&[fixture("forest"), fixture("forest")]);

        let view = personalized_view(&state, &db, PlayerId(0));
        assert!(view.valid_actions.iter().all(|a| !a.token.is_empty()));

        // Same `kind`, different subject instance -> different token.
        let land_tokens: Vec<&str> = view
            .valid_actions
            .iter()
            .filter(|a| a.kind == "play_land")
            .map(|a| a.token.as_str())
            .collect();
        assert_eq!(land_tokens.len(), 2);
        assert_ne!(land_tokens[0], land_tokens[1]);

        // The token is deterministic: recomputing the same action reproduces it,
        // which is exactly what makes server-side verification stateless.
        let pass = &view.valid_actions[0];
        assert_eq!(
            pass.token,
            content_token(&pass.kind, &pass.subject, &pass.requirements, &pass.prompts),
        );
    }

    /// Content binding (ADR 0009) covers the new prompt shapes too: a token captured
    /// for a `select_from_zone` discard while the hand is one shape is rejected once
    /// the hand — and so the prompt's candidates — has changed, exactly as it is for a
    /// targeted action. A stale prompt answer can never rebind.
    #[test]
    fn stale_token_on_a_prompt_action_is_rejected() {
        let db = CardDatabase::bundled().unwrap();
        let (mut state, hand) = cleanup_over_hand_limit();

        // Capture the answer a client would send for the discard action now.
        let before = personalized_view(&state, &db, state.active_player);
        let discard_before = before
            .valid_actions
            .iter()
            .find(|a| a.kind == "discard")
            .expect("a discard is offered while over the hand limit");
        let stale = ChooseAction {
            action_id: discard_before.id.clone(),
            token: discard_before.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[0].id)],
            }],
        };

        // The hand changes (a card leaves), so the prompt's candidates — and thus the
        // action's content token — change under the same positional id.
        let seat = state.active_player.0;
        state.players[seat].hand.remove(1);
        let after = personalized_view(&state, &db, state.active_player);
        let discard_after = after
            .valid_actions
            .iter()
            .find(|a| a.id == stale.action_id)
            .expect("the id is still offered");
        assert_ne!(
            discard_before.token, discard_after.token,
            "changed candidates re-derive a different content token",
        );

        // The stale token no longer matches, so the answer is rejected.
        assert!(resolve_action(&state, &db, state.active_player, &stale).is_none());

        // The current token for that same id does resolve, proving it is the token
        // (not the bare id) that binds a prompt answer.
        let fresh = ChooseAction {
            action_id: discard_after.id.clone(),
            token: discard_after.token.clone(),
            targets: vec![TargetChoice {
                slot: "discard".to_string(),
                chosen: vec![card_entity_id(hand[0].id)],
            }],
        };
        assert_eq!(
            resolve_action(&state, &db, state.active_player, &fresh),
            Some(Action::Discard { card: hand[0] }),
        );
    }
}
