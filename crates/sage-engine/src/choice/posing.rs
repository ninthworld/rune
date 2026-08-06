//! Which questions an effect asks, and the one rule that decides whether a question is
//! *posed* at all.
//!
//! The front half of the module's seam: [`choices_for_effect`] reads an effect and says
//! what it wants to know, and [`pose_choices`] either queues that or settles it outright.
//! A question with no legal answer is answered here rather than asked, which is the whole
//! of the never-stall guarantee (ADR 0013 §5) — in one place, rather than once per effect.

use super::*;

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
///
/// Settling a card selection may leave a **follow-up** owed — a look whose remainder its
/// controller arranges takes two questions, and skipping the first does not skip the
/// second — so the settled branch queues whatever it hands back, and that queued question
/// is what the caller's [`attach_resume`] then lands on.
pub(crate) fn pose_choices(
    state: &mut GameState,
    choices: Vec<(PlayerId, ChoiceQuestion)>,
    db: &CardDatabase,
) -> bool {
    let mut queued = false;
    for (chooser, question) in choices {
        let question = match &question {
            ChoiceQuestion::Cards(request) => {
                let (_, max) = choice_bounds(state, request, db);
                if max == 0 {
                    match apply_choice_outcome(state, request, &[], db) {
                        Some(follow_up) => follow_up,
                        None => continue,
                    }
                } else {
                    question
                }
            }
            ChoiceQuestion::Confirm(request) => {
                if !cost_could_be_paid(state, chooser, request, db) {
                    state.record_event(GameEvent::OptionalDeclined { player: chooser });
                    continue;
                }
                question
            }
            // A color question always has five legal answers, so it is always posed —
            // the one shape with no "nothing to ask" case at all.
            //
            // A replacement-ordering question and a card-name question are never posed
            // from here: neither is asked by an effect resolving. Both are asked by the
            // battlefield-entry seam, which queues them directly
            // ([`GameState::begin_battlefield_entry`](crate::GameState)).
            //
            // Neither is a card ordering: no effect declares one, it is only ever the
            // follow-up a settled selection hands back above — already judged worth
            // asking, since a remainder of nothing or of one card never produces one.
            ChoiceQuestion::Color(_)
            | ChoiceQuestion::CardName(_)
            | ChoiceQuestion::Replacement(_)
            | ChoiceQuestion::Order(_) => question,
            // A sacrifice of nothing is not a question: a player with no permanent of
            // the named class simply sacrifices none, exactly as a player with an empty
            // hand discards none. There is no aftermath to apply for the answers not
            // given, so unlike a card selection this one is skipped outright.
            ChoiceQuestion::Permanents(request) => {
                let (_, max) = permanent_choice_bounds(state, request, db);
                if max == 0 {
                    continue;
                }
                question
            }
        };
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
/// `source` is what the suspended ability came from (CR 113.3), and both things read off
/// it are resolved **here, as the question is posed**, for the same reason: the source
/// may be gone by the time the answer is given. Its printed card answers a
/// `same_name_as_source` filter, and the permanent itself is the `another` a sacrifice
/// cost excludes.
///
/// `resolution` and `db` are here for the verbs whose *size* is derived (CR 608.2): a
/// search whose size is a [`DerivedAmount`](crate::DerivedAmount), the number of cards to
/// discard, and the number of permanents to sacrifice are all read as the question is
/// **posed**, from the state as it stands then, and the request carries the number rather
/// than the source. That is what makes the amount taken once — a permanent that leaves
/// while the answer is being given cannot change a count already written down.
pub(crate) fn choices_for_effect(
    state: &GameState,
    effect: &Effect,
    controller: PlayerId,
    source: Option<crate::stack::AbilitySource>,
    targets: &[Target],
    resolution: crate::resolve::Resolution,
    db: &crate::card::CardDatabase,
) -> Option<Vec<(PlayerId, ChoiceQuestion)>> {
    let source_permanent = source.and_then(crate::stack::AbilitySource::permanent);
    // The printed card a `same_name_as_source` filter compares against. A token has no
    // card to compare against, and no card in a library or a hand can share an identity
    // it has not got (CR 111), so it simply matches nothing.
    let source_card = source_permanent.and_then(|id| {
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == id)
            .and_then(|perm| perm.printed.card())
    });
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
        // The derived-count discard: one question per named seat, and the count is read
        // *of that seat* — `each player discards half the cards in their hand` is each
        // of them halving their own hand, so three seats produce three different
        // numbers. Fixed here, as the question is posed (CR 608.2).
        Effect::DiscardByAmount { player_ref, amount } => {
            let subjects = match target {
                Some(Target::Player(seat)) => vec![seat],
                _ => crate::apply::non_targeting_subjects(state, *player_ref, controller),
            };
            Some(
                subjects
                    .into_iter()
                    .map(|subject| {
                        let count = crate::condition::derived_amount(
                            state, amount, controller, subject, resolution, db,
                        );
                        (
                            subject,
                            ChoiceQuestion::Cards(ChoiceRequest {
                                subject,
                                zone: ChoiceZone::Hand,
                                filter: CardFilter::Any,
                                source_card,
                                min: count,
                                max: count,
                                outcome: ChoiceOutcome::Discard,
                            }),
                        )
                    })
                    .collect(),
            )
        }
        // CR 701.17: one question per named seat, each over their own permanents, each
        // sized by a number read of them as it is posed. The chooser is always the
        // sacrificing player — CR 701.17b has no other shape.
        Effect::Sacrifice {
            player_ref,
            amount,
            card_type,
        } => {
            let subjects = match target {
                Some(Target::Player(seat)) => vec![seat],
                _ => crate::apply::non_targeting_subjects(state, *player_ref, controller),
            };
            Some(
                subjects
                    .into_iter()
                    .map(|subject| {
                        let count = crate::condition::derived_amount(
                            state, amount, controller, subject, resolution, db,
                        );
                        (
                            subject,
                            ChoiceQuestion::Permanents(PermanentRequest {
                                subject,
                                card_type: *card_type,
                                // A mandatory sacrifice names a card type and never a
                                // subtype, and it excludes nothing: "sacrifice half the
                                // creatures you control" counts the source among them.
                                subtype: None,
                                except: None,
                                min: count,
                                max: count,
                                outcome: PermanentOutcome::Sacrifice,
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
        // One question here, and possibly a second later: the taking is asked now, and
        // the arrangement of what is left over — when the card says *in any order* — is
        // posed by the outcome once the taking is answered and the remainder is known.
        Effect::LookAtTop {
            count,
            take,
            take_min,
            filter,
            destination,
            bottom_order,
        } => Some(vec![(
            controller,
            ChoiceQuestion::Cards(ChoiceRequest {
                subject: controller,
                zone: ChoiceZone::LibraryTop(*count),
                filter: filter.clone(),
                source_card,
                // The card's own floor: `0` for the "you may reveal…" looks, `1` for one
                // that says *put one of them into your hand*. Clamped by
                // [`choice_bounds`] either way, so it never becomes a stall.
                min: u32::from(*take_min),
                max: u32::from(*take),
                outcome: ChoiceOutcome::TakeAndBottomRest {
                    destination: *destination,
                    order: *bottom_order,
                },
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
                    Some(amount) => crate::condition::derived_amount(
                        state, amount, controller, controller, resolution, db,
                    ),
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
                source: source_permanent,
                effects: effects.clone(),
                targets: targets.to_vec(),
            }),
        )]),
        _ => None,
    }
}
