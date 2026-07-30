//! Contextual action labels and action destinations (issue #554).
//!
//! Two presentation facts about an offered action that are **rules judgments**, and
//! therefore the server's to make:
//!
//! - what a pass of priority should be *called* right now — "Resolve" when passing
//!   would resolve the top of the stack, "Pass" otherwise; and
//! - where an action may be *taken to*, the complete set of drop regions a client is
//!   allowed to offer for it.
//!
//! Both are read off already-computed engine state; neither adds a rule. Split from
//! `actions.rs` so that file stays within the size ceiling in
//! `docs/coding-standards.md`.

use super::*;

/// The **contextual** label for a pass of priority (issue #554).
///
/// "Pass" and "Resolve" are the same engine action but not the same decision, and
/// telling them apart is a rules judgment — it depends on the stack *and* on how many
/// players still in the game have already passed in unbroken succession (CR 117.4:
/// the top of the stack resolves when all players pass in succession; CR 800.4a: an
/// eliminated seat neither receives nor passes priority). The server owns that
/// judgment; a client that tried to make it would be reimplementing priority, and
/// would get a multiplayer table wrong the moment a seat was eliminated.
///
/// The label is the *only* thing that changes: the action, its id, and its content
/// token are identical either way, so this affects presentation and nothing else.
pub(crate) fn pass_priority_label(state: &GameState) -> String {
    if pass_would_resolve_top_of_stack(state) {
        "Resolve".to_string()
    } else {
        "Pass".to_string()
    }
}

/// Whether passing priority right now would **resolve the top of the stack** rather
/// than move priority along (CR 117.4 / CR 608.2): there is something on the stack,
/// and this pass completes the round of unbroken passes among the players still in
/// the game. Mirrors the condition
/// [`apply_pass_priority`](sage_engine::apply_action) itself applies, read from the
/// same stored state, so the offered label and the applied effect cannot disagree.
pub(crate) fn pass_would_resolve_top_of_stack(state: &GameState) -> bool {
    !state.stack.is_empty()
        && state.consecutive_passes.saturating_add(1) >= state.living_player_count()
}

/// The server-authoritative [`ActionDestination`]s this action may be taken to
/// (issue #554) — the complete set of drop regions a client may offer for it, and the
/// only source of them.
///
/// Deliberately conservative: an action gets a destination only where dropping it
/// *somewhere* is what taking it means. Playing a land puts it onto the battlefield
/// (CR 305.1), casting a spell and activating a non-mana ability put an object on the
/// stack (CR 601.2a / CR 602.2a), and a CR 903.9a commander return names the owner's
/// command zone. Everything else — passing, conceding, the combat declarations, the
/// pre-game decisions, and a **mana ability**, which never uses the stack (CR 605.1a)
/// and is a one-click gesture under ADR 0025 — names none, so a client fails closed
/// and offers no drop target rather than inventing one.
pub(crate) fn action_destinations(
    state: &GameState,
    action: &Action,
    mana_ability: bool,
) -> Vec<ActionDestination> {
    let zone = |id: &str, owner: String, label: &str| ActionDestination {
        kind: "zone".to_string(),
        id: id.to_string(),
        owner,
        label: label.to_string(),
    };
    match action {
        Action::PlayLand { .. } => vec![zone("battlefield", String::new(), "Battlefield")],
        // A mana ability produces mana without using the stack, so there is nowhere
        // to drop it; the lighter one-click gesture (ADR 0025) is the whole input.
        Action::ActivateAbility { .. } if mana_ability => Vec::new(),
        Action::CastSpell { .. } | Action::ActivateAbility { .. } => {
            vec![zone("stack", String::new(), "Stack")]
        }
        Action::ReturnCommanderToCommandZone { .. } => {
            vec![zone("command", player_id(state.priority), "Command zone")]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::test_support::fixture;
    use crate::view::personalized_view;

    /// The action the offered view labels as a pass of priority.
    fn pass_label(view: &sage_protocol::GameView) -> &str {
        view.valid_actions
            .iter()
            .find(|a| a.kind == "pass_priority")
            .map(|a| a.label.as_str())
            .expect("a pass is offered to the priority holder")
    }

    #[test]
    fn issue_554_pass_is_labelled_resolve_only_when_it_would_resolve_the_stack() {
        // The judgment a client must not make: the *same* engine action reads as
        // "Pass" or "Resolve" depending on the stack and on how many living seats have
        // already passed in unbroken succession.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;

        // Empty stack: passing moves priority along, so the label is "Pass".
        assert_eq!(
            pass_label(&personalized_view(&state, &db, PlayerId(0))),
            "Pass"
        );

        // Something on the stack, but nobody has passed yet: the opponent still gets
        // priority first, so this pass does *not* resolve anything.
        let spell = state.new_instance(fixture("walking_corpse"));
        let stack_id = StackId(state.mint_id());
        state.stack.push(sage_engine::StackObject {
            id: stack_id,
            controller: PlayerId(1),
            kind: StackObjectKind::Spell { card: spell },
            targets: Vec::new(),
        });
        assert_eq!(
            pass_label(&personalized_view(&state, &db, PlayerId(0))),
            "Pass"
        );

        // The opponent has passed: this pass completes the round, so the top resolves
        // and the label says so.
        state.consecutive_passes = 1;
        assert_eq!(
            pass_label(&personalized_view(&state, &db, PlayerId(0))),
            "Resolve"
        );

        // The decision is not a seat count: with a third seat that has been eliminated
        // (CR 800.4a), a round is still two passes, so the label stays "Resolve" — the
        // exact case a client counting `seat_order` would get wrong.
        let mut three = GameState::new_multiplayer(3);
        three.step = state.step;
        three.stack.clone_from(&state.stack);
        three.players[2].has_lost = true;
        three.consecutive_passes = 1;
        assert_eq!(
            pass_label(&personalized_view(&three, &db, PlayerId(0))),
            "Resolve"
        );
    }

    #[test]
    fn issue_554_destinations_name_only_where_taking_an_action_puts_something() {
        // A land goes to the battlefield; a spell goes to the stack; a mana ability and
        // a pass name nowhere, so a client fails closed and offers no drop target.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.players[0].hand = vec![
            state.new_instance(fixture("forest")),
            state.new_instance(fixture("walking_corpse")),
        ];
        crate::view::test_support::put_permanent(
            &mut state,
            fixture("llanowar_elves"),
            PlayerId(0),
            false,
            false,
        );

        let view = personalized_view(&state, &db, PlayerId(0));
        let of = |kind: &str| {
            view.valid_actions
                .iter()
                .find(|a| a.kind == kind)
                .unwrap_or_else(|| panic!("a {kind} action is offered"))
                .clone()
        };

        let land = of("play_land");
        assert_eq!(land.destinations.len(), 1);
        assert_eq!(land.destinations[0].kind, "zone");
        assert_eq!(land.destinations[0].id, "battlefield");
        // The battlefield is shared, so no owner is named.
        assert!(land.destinations[0].owner.is_empty());

        assert!(of("pass_priority").destinations.is_empty());
        assert!(of("concede").destinations.is_empty());

        // A mana ability never uses the stack (CR 605.1a), so it names no destination
        // even though every other activation does.
        let ability = of("activate_ability");
        assert!(ability.mana_ability, "the Elves' ability produces mana");
        assert!(ability.destinations.is_empty());
    }

    #[test]
    fn issue_554_a_cast_names_the_stack() {
        // Separated from the case above because a castable creature needs mana on the
        // battlefield, which would also change what else is offered.
        let db = CardDatabase::bundled().unwrap();
        let mut state = GameState::new_two_player();
        state.step = Step::PrecombatMain;
        state.players[0].hand = vec![state.new_instance(fixture("walking_corpse"))];
        for _ in 0..3 {
            crate::view::test_support::put_permanent(
                &mut state,
                fixture("swamp"),
                PlayerId(0),
                false,
                false,
            );
        }
        state.players[0].mana_pool.add(sage_engine::Color::Black, 3);

        let view = personalized_view(&state, &db, PlayerId(0));
        let cast = view
            .valid_actions
            .iter()
            .find(|a| a.kind == "cast_spell")
            .expect("the creature is castable with three black mana");
        assert_eq!(cast.destinations.len(), 1);
        assert_eq!(cast.destinations[0].id, "stack");
    }
}
