//! Which questions an effect asks, and the one rule that decides whether a question is
//! *posed* at all.
//!
//! The front half of the module's seam: [`choices_for_effect`] reads an effect and says
//! what it wants to know, and [`pose_choices`] either queues that or settles it outright.
//! A question with no legal answer is answered here rather than asked, which is the whole
//! of the never-stall guarantee (ADR 0013 §5) — in one place, rather than once per effect.

use super::*;

/// Whether `player` could pay `cost` if they tapped everything they have — their pool
/// plus every point of mana their untapped sources could still add.
///
/// This, not the current pool, is what decides whether an optional cost is *posed*: a
/// player with an empty pool and two untapped Forests can pay `{1}`, and auto-declining
/// them would take away a decision the rules give them. The estimate is the same
/// deliberate over-estimate [`crate::priority_has_no_meaningful_action`] makes — every
/// mana ability of every untapped source, as though one permanent could be tapped for
/// all of them — and errs in the same safe direction: it can only ever *offer* a choice
/// that turns out unpayable, which the chooser simply declines, never withhold one they
/// could have taken.
fn cost_could_be_paid(
    state: &GameState,
    player: PlayerId,
    cost: Option<&str>,
    db: &CardDatabase,
) -> bool {
    let Some(cost) = cost else {
        return true;
    };
    crate::actions::potential_mana_pool(state, player, db)
        .can_pay(&crate::mana::parse_mana_cost(cost))
}

/// Pose `choices`, or settle the ones that have no legal answer outright.
///
/// Returns whether anything was actually queued — i.e. whether the caller must suspend.
/// A question that is not a decision is answered here instead of being asked, which is
/// the whole of the "a choice with no legal answer resolves without stalling" guarantee,
/// in one place rather than per effect:
///
/// - a card selection whose clamped maximum is zero is applied with an empty selection
///   (which still shuffles a searched library and still bottoms a looked-at pile);
/// - an optional cost no amount of tapping could pay is declined, and *recorded* as
///   declined, so the log never quietly omits a decision the player was entitled to.
pub(crate) fn pose_choices(
    state: &mut GameState,
    choices: Vec<(PlayerId, ChoiceQuestion)>,
    db: &CardDatabase,
) -> bool {
    let mut queued = false;
    for (chooser, question) in choices {
        match &question {
            ChoiceQuestion::Cards(request) => {
                let (_, max) = choice_bounds(state, request, db);
                if max == 0 {
                    apply_choice_outcome(state, request, &[], db);
                    continue;
                }
            }
            ChoiceQuestion::Confirm(request) => {
                if !cost_could_be_paid(state, chooser, request.cost.as_deref(), db) {
                    state.record_event(GameEvent::OptionalDeclined { player: chooser });
                    continue;
                }
            }
            // A color question always has five legal answers, so it is always posed —
            // the one shape with no "nothing to ask" case at all.
            //
            // A replacement-ordering question and a card-name question are never posed
            // from here: neither is asked by an effect resolving. Both are asked by the
            // battlefield-entry seam, which queues them directly
            // ([`GameState::begin_battlefield_entry`](crate::GameState)).
            ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_) => {}
        }
        state.pending_choices.push(PendingChoice {
            chooser,
            question,
            resume: None,
        });
        queued = true;
    }
    queued
}

/// Attach `resume` to the most recently queued choice, so the rest of the suspended
/// object's resolution happens once, after the *last* choice its effect posed.
///
/// Called only when [`pose_choices`] queued something, so the queue is non-empty.
pub(crate) fn attach_resume(state: &mut GameState, resume: Resume) {
    if let Some(last) = state.pending_choices.last_mut() {
        last.resume = Some(resume);
    }
}

/// The choices `effect` poses, if it poses any: `(chooser, request)` pairs in the order
/// they must be answered. `None` for every effect that resolves without asking.
///
/// `targets` are the targets the announcement chose for this effect's group, in slot
/// order, so a "target player discards" reaches the seat the caster aimed at rather
/// than a seat derived here — and an optional effect's chosen target is carried into
/// the question it poses.
///
/// `resolution` is the frame the question is asked in, and one question reads it: a search
/// whose size is a [`DerivedAmount`](crate::DerivedAmount) takes that number here
/// (CR 608.2), before the choice is posed, because the *bounds* of the question are what
/// the amount decides.
pub(crate) fn choices_for_effect(
    state: &GameState,
    effect: &Effect,
    controller: PlayerId,
    source_card: Option<CardId>,
    targets: &[Target],
    resolution: crate::resolve::Resolution,
    db: &crate::card::CardDatabase,
) -> Option<Vec<(PlayerId, ChoiceQuestion)>> {
    let target = targets.first().copied();
    match effect {
        Effect::Discard {
            player_ref,
            count,
            chosen_by,
            filter,
        } => {
            let subjects = match target {
                // A targeting reference names the one seat that was aimed at.
                Some(Target::Player(seat)) => vec![seat],
                _ => crate::apply::non_targeting_subjects(state, *player_ref, controller),
            };
            Some(
                subjects
                    .into_iter()
                    .map(|subject| {
                        let chooser = match chosen_by {
                            crate::ability::Chooser::Owner => subject,
                            crate::ability::Chooser::Controller => controller,
                        };
                        (
                            chooser,
                            ChoiceQuestion::Cards(ChoiceRequest {
                                subject,
                                zone: ChoiceZone::Hand,
                                filter: filter.clone(),
                                source_card,
                                min: u32::from(*count),
                                max: u32::from(*count),
                                outcome: ChoiceOutcome::Discard,
                            }),
                        )
                    })
                    .collect(),
            )
        }
        Effect::Scry { count } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: CardFilter::Any,
                source_card,
                // Any number, including none (CR 701.17).
                min: 0,
                max: u32::from(*count),
                outcome: ChoiceOutcome::BottomChosen,
            }),
        )]),
        Effect::LookAtTop {
            count,
            take,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: filter.clone(),
                source_card,
                // Taking is optional ("you may reveal…"), so nothing forces a pick.
                min: 0,
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndBottomRest(*destination),
            }),
        )]),
        Effect::SearchLibrary {
            take,
            take_amount,
            filter,
            destination,
        } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::Library,
                filter: filter.clone(),
                source_card,
                // A player may always fail to find (CR 701.19c).
                min: 0,
                // The printed number, or the one the card takes off the game — read once,
                // here, because it is the size of the question rather than something the
                // answer could still change (CR 608.2).
                max: match take_amount {
                    Some(amount) => {
                        crate::condition::derived_amount(state, amount, controller, resolution, db)
                    }
                    None => u32::from(*take),
                },
                outcome: ChoiceOutcome::TakeAndShuffle(*destination),
            }),
        )]),
        // One question per point of mana, so a player producing two in any combination
        // really is asked twice and may answer differently each time. They are separate
        // queue entries rather than one multi-answer question because that is what makes
        // the second question askable *after* seeing the first answered, and because the
        // resume machinery already attaches the rest of the resolution to the last of
        // them. `Add two mana of any one color` is the same queue with the arithmetic the
        // other way round: one question, paying out the whole amount — which is what
        // stops a single-colour clause from being answerable twice.
        Effect::AddManaAnyColor {
            amount,
            same_color,
            restriction,
        } => {
            let (questions, each) = if *same_color {
                (1, *amount)
            } else {
                (*amount, 1)
            };
            Some(
                (0..questions)
                    .map(|_| {
                        (
                            controller,
                            ChoiceQuestion::Color(ColorRequest {
                                outcome: ColorOutcome::AddMana {
                                    amount: each,
                                    restriction: restriction.clone(),
                                },
                            }),
                        )
                    })
                    .collect(),
            )
        }
        // The one question the *controller* always answers, whoever else the ability
        // names: an optional effect is theirs to take or leave (CR 608.2).
        Effect::May { cost, effects } => Some(vec![(
            controller,
            ChoiceQuestion::Confirm(ConfirmRequest {
                cost: cost.clone(),
                effects: effects.clone(),
                targets: targets.to_vec(),
            }),
        )]),
        _ => None,
    }
}
