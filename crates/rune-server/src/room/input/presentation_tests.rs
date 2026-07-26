//! Tests for the **presentation contract** (issue #594): the ordered, bounded window of
//! display-only moments the room produces and projects onto every view.
//!
//! A sibling of [`super::pacing_tests`] rather than more of it. That module asks where a
//! settle must *stop* (issue #455); this one asks what a seat is owed about the ground a
//! settle covered *without* stopping — the order things happened in, which no board diff
//! can recover and which the client is forbidden to invent (`AGENTS.md`: zero game logic
//! in the client).
//!
//! Everything here is deterministic and sleeps nowhere: the server never waits on
//! presentation, and neither do these tests. Dwell is entirely the client's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rune_engine::{
    GameEvent, GameLogEntry as EngineLogEntry, Permanent, PermanentId, StackId, StackObject,
    StackObjectKind, Step, Target,
};
use rune_protocol::{
    ChooseAction, MomentKind, MomentObject, MomentZone, PresentationMoment, PRESENTATION_WINDOW,
};

use super::*;
use crate::room::test_support::*;
use crate::test_support::fixture;

/// Append `event` to a state's authoritative history the way the engine would, so a
/// hand-built fixture and the engine share one sequence space — a duplicated sequence
/// would make the trail's cursor skip real events.
fn log_event(state: &mut GameState, event: GameEvent) {
    let sequence = state.next_log_sequence;
    state.next_log_sequence += 1;
    state.log.push(EngineLogEntry { sequence, event });
}

/// Seat 1's Shock, already cast and aimed at seat 0's Llanowar Elves, with seat 0
/// holding priority over it in seat 1's upkeep and no way to answer — the scenario the
/// whole contract exists for. The cast is on the record, as it is for any spell the room
/// actually saw cast.
///
/// Seat 0 also holds a stocked hand and library it can never cast (no lands, no mana), so
/// the same fixture proves the information-safety guard: those cards are in hidden zones
/// for the entire run.
fn removal_resolving_on_a_helpless_seat() -> GameState {
    let mut state = GameState::new_two_player();
    state.turn = 2;
    state.active_player = PlayerId(1);
    state.step = Step::Upkeep;
    for seat in 0..2 {
        state.players[seat].library = (0..8)
            .map(|_| state.new_instance(fixture("onakke_ogre")))
            .collect();
    }
    state.players[0].hand = vec![
        state.new_instance(fixture("snapping_drake")),
        state.new_instance(fixture("walking_corpse")),
    ];

    let elves_card = fixture("llanowar_elves");
    let elves_instance = state.new_instance(elves_card);
    let elves = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id: elves,
        instance: elves_instance.id,
        card: elves_card,
        controller: PlayerId(0),
        tapped: false,
        entered_turn: 0,
        attacking: None,
        blocking: None,
        damage: 0,
        counters: Default::default(),
        attached_to: None,
    });

    let shock = state.new_instance(fixture("shock"));
    let sid = StackId(state.mint_id());
    state.stack.push(StackObject {
        id: sid,
        controller: PlayerId(1),
        kind: StackObjectKind::Spell { card: shock },
        targets: vec![Target::Permanent(elves)],
    });
    state.priority = PlayerId(0);
    log_event(
        &mut state,
        GameEvent::SpellCast {
            player: PlayerId(1),
            card: shock,
        },
    );
    state
}

/// Spawn a room with automation on and the human default stops, join `seats`, and return
/// each joined seat's first view — the one the opening settle comes to rest on.
async fn opened(
    state: GameState,
    seats: usize,
) -> (
    Vec<GameView>,
    Vec<watch::Receiver<Option<GameView>>>,
    RoomHandle,
    tokio::task::JoinHandle<()>,
) {
    let (handle, task) = Room::new(state, db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_stop_policy(StopPolicy::HumanMainPhases)
        .spawn();
    let mut receivers = Vec::new();
    for seat in 0..seats {
        let (tx, mut rx) = view_channel();
        handle.send(RoomInput::Join { seat, outbox: tx });
        let _ = wait_for_view(&mut rx).await;
        receivers.push(rx);
    }
    // Every join is itself a broadcast (a seat's connection state is public), so an
    // earlier seat has a *later* view waiting than the one it was handed. Mark them all
    // seen here, so a test that awaits a change after this really does await one.
    let views = receivers
        .iter_mut()
        .map(|rx| rx.borrow_and_update().clone().expect("a pushed view"))
        .collect();
    (views, receivers, handle, task)
}

/// The position of the first moment matching `pred`, for order assertions.
fn index_of(window: &[PresentationMoment], pred: impl Fn(&MomentKind) -> bool) -> usize {
    window
        .iter()
        .position(|moment| pred(&moment.kind))
        .unwrap_or_else(|| panic!("no such moment in {:?}", kinds(window)))
}

fn kinds(window: &[PresentationMoment]) -> Vec<&MomentKind> {
    window.iter().map(|moment| &moment.kind).collect()
}

/// Every retained object a moment refers to — the only place a card face can travel.
fn objects(kind: &MomentKind) -> Vec<&MomentObject> {
    match kind {
        MomentKind::Cast { object, .. }
        | MomentKind::Resolved { object, .. }
        | MomentKind::Countered { object, .. }
        | MomentKind::Fizzled { object, .. }
        | MomentKind::ZoneMove { object, .. }
        | MomentKind::Died { object } => vec![object],
        MomentKind::Attacked { attackers, .. } => attackers.iter().collect(),
        _ => Vec::new(),
    }
}

fn skipped(window: &[PresentationMoment]) -> Vec<&PresentationMoment> {
    window
        .iter()
        .filter(|moment| matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
        .collect()
}

#[tokio::test]
async fn issue_594_a_resolved_removal_spell_reaches_the_skipped_seat_in_order() {
    // The acceptance criterion. A spell seat 0 had no answer to resolves *inside* one
    // settle, and the single broadcast that follows must carry the sequence, not just
    // the outcome: the board it arrives with shows a missing creature and nothing else,
    // and no diff of two boards can say whether it was countered, killed, or exiled.
    let (views, _rx, handle, task) = opened(removal_resolving_on_a_helpless_seat(), 1).await;
    let window = &views[0].presentation;

    let cast = index_of(window, |kind| matches!(kind, MomentKind::Cast { .. }));
    let resolved = index_of(window, |kind| matches!(kind, MomentKind::Resolved { .. }));
    let died = index_of(window, |kind| matches!(kind, MomentKind::Died { .. }));
    let moved = index_of(window, |kind| {
        matches!(
            kind,
            MomentKind::ZoneMove {
                from: MomentZone::Battlefield,
                to: MomentZone::Graveyard,
                ..
            }
        )
    });
    assert!(
        cast < resolved && resolved < died && died < moved,
        "cast → resolved → died → zone_move, in that order: {:?}",
        kinds(window)
    );

    // The causal chain is *stated*. Adjacency is not causation — a settle interleaves
    // independent seats' events — so the server says which resolution the death came
    // from and which death the travel came from.
    assert_eq!(
        window[died].cause,
        Some(window[resolved].id),
        "the death names the resolution that caused it"
    );
    assert_eq!(
        window[moved].cause,
        Some(window[died].id),
        "and the travel names the death"
    );

    // The objects are named from what the events recorded, not from the current board —
    // the creature is not on it any more.
    let MomentKind::Died { object } = &window[died].kind else {
        panic!("checked above");
    };
    assert_eq!(object.name, "Llanowar Elves");
    assert!(!views[0]
        .battlefield
        .iter()
        .any(|perm| perm.card.name == "Llanowar Elves"));

    // And the seat is told where its own priority was taken, in the same ordered stream.
    assert_eq!(
        skipped(window).len(),
        1,
        "one grouped moment, not one per pass"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_the_skipped_seat_is_never_prompted_to_acknowledge_a_moment() {
    // Presentation is advisory and never load-bearing: it must not become a decision.
    // The seat that was passed through the resolution receives the moments on a view
    // that offers it nothing to click — the game is waiting on the *other* seat, exactly
    // as it would be with the field absent.
    let (views, _rx, handle, task) = opened(removal_resolving_on_a_helpless_seat(), 1).await;
    assert!(
        !views[0].presentation.is_empty(),
        "the moments did reach this seat"
    );
    assert!(
        views[0].valid_actions.is_empty(),
        "and it is not asked to acknowledge them: {:?}",
        views[0].valid_actions
    );
    assert!(
        views[0].action_deadline.is_none(),
        "nothing is being timed on this seat's behalf either"
    );
    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_a_settle_across_empty_steps_and_a_turn_states_both_boundaries() {
    // The pacing beat a client cannot compute. A repeated step means an extra combat
    // phase (CR 506.1) or an extra cleanup (CR 514.3a) at least as often as it means a
    // new turn, so the server states the turn boundary; and the whole stretch a seat was
    // carried through is ONE moment, because six passes are one skipped stretch and
    // saying so six times reads as six events.
    let mut state = spell_less_state();
    state.step = Step::End;
    let (views, _rx, handle, task) = opened(state, 2).await;
    let window = &views[0].presentation;

    let phases: Vec<Phase> = window
        .iter()
        .filter_map(|moment| match moment.kind {
            MomentKind::PhaseChange { phase } => Some(phase),
            _ => None,
        })
        .collect();
    assert!(
        phases.len() >= 3,
        "the settle crossed several empty steps: {phases:?}"
    );
    let turns: Vec<u32> = window
        .iter()
        .filter_map(|moment| match moment.kind {
            MomentKind::TurnChange { turn, .. } => Some(turn),
            _ => None,
        })
        .collect();
    assert_eq!(turns, vec![2], "exactly one turn boundary, stated once");

    // The moments are labelled with where the game *was*, not where it is: the view has
    // already moved on to the position the settle came to rest at. The per-seat
    // `phases_skipped` is deliberately excluded — it closes the batch but is labelled
    // where its *path* began, so it is the one moment whose position looks backwards.
    let crossed: Vec<u32> = window
        .iter()
        .filter(|moment| !matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
        .map(|moment| moment.turn)
        .collect();
    assert!(
        crossed.contains(&1) && crossed.contains(&2),
        "the window spans both turns: {crossed:?}"
    );
    assert!(
        crossed.windows(2).all(|pair| pair[0] <= pair[1]),
        "and never goes backwards: {crossed:?}"
    );

    // One grouped `phases_skipped`, carrying the whole path — including the entries from
    // both turns, which is what makes it a path rather than a boolean.
    let paths = skipped(window);
    assert_eq!(paths.len(), 1, "one moment for the whole stretch");
    let MomentKind::PhasesSkipped { steps, reason } = &paths[0].kind else {
        panic!("checked above");
    };
    assert!(
        steps.len() >= 2 && steps.iter().any(|step| step.turn == 1),
        "the path starts where the seat still held priority: {steps:?}"
    );
    assert_eq!(*reason, rune_protocol::AutoPassReason::NoResponseAvailable);
    assert_eq!(
        (paths[0].turn, paths[0].phase),
        (steps[0].turn, steps[0].phase),
        "labelled where the stretch began"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_coalescing_broadcasts_lose_no_moments() {
    // The core no-loss criterion. A seat's outbox is a latest-value `watch`: a view
    // pushed while an earlier one is still unread *replaces* it, exactly as the ack note
    // in `broadcast.rs` describes. A trail the room drained on send would therefore lose
    // precisely the moments coalescing was meant to preserve. Carrying the recent
    // unconsumed suffix on every view instead means whichever view actually arrives
    // carries the lot.
    //
    // Seat 1 stops twice; seat 0 stops nowhere and never reads its outbox in between, so
    // two whole batches land on one unread channel.
    let (handle, task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::On)
        .with_stops(vec![vec![], vec![Phase::Draw, Phase::PostcombatMain]])
        .spawn();
    let (tx0, mut rx0) = view_channel();
    let (tx1, mut rx1) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let _ = wait_for_view(&mut rx0).await;
    let mut seat1 = wait_for_view(&mut rx1).await;

    // Seat 1 acts twice. Seat 0's channel is never read between the two, so the room
    // pushes it two views and it can only ever observe the newest.
    let mut earlier: Vec<u64> = Vec::new();
    for round in 0..2 {
        let pass = seat1
            .valid_actions
            .iter()
            .find(|action| action.kind == "pass_priority")
            .expect("seat 1 was handed priority at its stop")
            .clone();
        handle.send(RoomInput::Message {
            seat: 1,
            message: ClientMessage::ChooseAction(ChooseAction {
                action_id: pass.id.clone(),
                token: pass.token.clone(),
                ..Default::default()
            }),
        });
        seat1 = wait_for_view(&mut rx1).await;
        if round == 0 {
            // Everything public seat 1 has seen so far must still be on seat 0's newest
            // view after the *second* advance overwrites the first.
            earlier = seat1
                .presentation
                .iter()
                .filter(|moment| !matches!(moment.kind, MomentKind::PhasesSkipped { .. }))
                .map(|moment| moment.id)
                .collect();
            assert!(!earlier.is_empty(), "the first advance produced moments");
        }
    }

    let latest = rx0.borrow_and_update().clone().expect("a pushed view");
    let carried: Vec<u64> = latest.presentation.iter().map(|moment| moment.id).collect();
    for id in &earlier {
        assert!(
            carried.contains(id),
            "moment {id} was dropped when two broadcasts coalesced: {carried:?}"
        );
    }
    let batches: std::collections::BTreeSet<u64> = latest
        .presentation
        .iter()
        .map(|moment| moment.batch)
        .collect();
    assert!(
        batches.len() >= 2,
        "the surviving window spans both settles: {batches:?}"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_ids_are_monotonic_batches_group_a_settle_and_streams_are_per_seat() {
    // Identity, and the gaps in it. Ids are a watermark, so they only ever increase;
    // a batch says "these things happened because of that one turn of the crank"; and a
    // seat's stream is missing the other seat's per-seat moments, which is the sanctioned
    // reason a receiver sees gaps and must never try to fill them.
    let (views, mut receivers, handle, task) =
        opened(removal_resolving_on_a_helpless_seat(), 2).await;
    let seat0 = views[0].presentation.clone();
    let seat1 = views[1].presentation.clone();

    for window in [&seat0, &seat1] {
        assert!(
            window.windows(2).all(|pair| pair[0].id < pair[1].id),
            "ids are strictly increasing: {:?}",
            window.iter().map(|moment| moment.id).collect::<Vec<_>>()
        );
    }
    let opening: std::collections::BTreeSet<u64> =
        seat0.iter().map(|moment| moment.batch).collect();
    assert_eq!(opening.len(), 1, "one settle is one batch: {opening:?}");

    // Each seat's own `phases_skipped` is theirs alone.
    let own = skipped(&seat0);
    let theirs = skipped(&seat1);
    assert_eq!((own.len(), theirs.len()), (1, 1));
    assert_ne!(own[0].id, theirs[0].id);
    assert!(
        !seat0.iter().any(|moment| moment.id == theirs[0].id),
        "seat 0's stream has a gap where seat 1's moment sits"
    );
    assert!(!seat1.iter().any(|moment| moment.id == own[0].id));

    // A spectator owns no seat, so it receives the public stream and neither seat's
    // per-seat moment (ADR 0022).
    let (spectator_tx, mut spectator_rx) = watch::channel(None);
    handle.send(RoomInput::JoinSpectator {
        outbox: spectator_tx,
    });
    let watching = wait_for_spectator_view(&mut spectator_rx).await;
    assert!(
        !watching.presentation.is_empty(),
        "the public moments do reach a spectator"
    );
    assert!(
        watching
            .presentation
            .iter()
            .all(|moment| !matches!(moment.kind, MomentKind::PhasesSkipped { .. })),
        "and never a per-seat one"
    );
    let public: Vec<u64> = watching.presentation.iter().map(|m| m.id).collect();
    let seated: Vec<u64> = seat0.iter().map(|m| m.id).collect();
    assert!(
        public.iter().all(|id| seated.contains(id)),
        "a spectator's window is the seated window minus the seat's own moments: \
         {public:?} vs {seated:?}"
    );

    // A second settle is a second batch, ordered after the first and never renumbered.
    let pass = views[1]
        .valid_actions
        .iter()
        .find(|action| action.kind == "pass_priority")
        .expect("seat 1 rests holding priority")
        .clone();
    handle.send(RoomInput::Message {
        seat: 1,
        message: ClientMessage::ChooseAction(ChooseAction {
            action_id: pass.id.clone(),
            token: pass.token.clone(),
            ..Default::default()
        }),
    });
    let next = wait_for_view(&mut receivers[0]).await;
    let batches: Vec<u64> = next
        .presentation
        .iter()
        .map(|moment| moment.batch)
        .collect();
    assert!(
        batches.windows(2).all(|pair| pair[0] <= pair[1]),
        "batches are contiguous runs in id order: {batches:?}"
    );
    assert!(
        batches.last() > batches.first(),
        "the second settle is a later batch: {batches:?}"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_a_departed_permanent_still_renders_from_its_retained_face() {
    // The whole reason a moment retains a snapshot. The view that carries the death is
    // the one where the creature is already gone, so a reference by id alone would
    // resolve to nothing at exactly the moment it is needed.
    let (views, _rx, handle, task) = opened(removal_resolving_on_a_helpless_seat(), 1).await;
    let window = &views[0].presentation;

    let died = index_of(window, |kind| matches!(kind, MomentKind::Died { .. }));
    let MomentKind::Died { object } = &window[died].kind else {
        panic!("checked above");
    };
    let face = object
        .card
        .as_ref()
        .expect("the room retained the public face while the permanent was still there");
    assert_eq!(face.name, "Llanowar Elves");
    assert_eq!(face.id, object.id, "the face keeps the object's own handle");
    assert!(
        !views[0].battlefield.iter().any(|perm| perm.id == object.id),
        "and the permanent is absent from the board this moment arrived with"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_no_moment_ever_carries_a_face_from_a_hidden_zone() {
    // Information safety. A moment crosses seats — every opponent and every spectator
    // read the same public moments — so a retained private face could not be redacted
    // after the fact. Seat 0 holds a stocked hand and library for the whole run; nothing
    // in any receiver's stream may name or picture one of those cards.
    let state = removal_resolving_on_a_helpless_seat();
    let hidden: Vec<String> = state
        .players
        .iter()
        .flat_map(|player| player.hand.iter().chain(player.library.iter()))
        .map(|instance| crate::view::card_entity_id(instance.id))
        .collect();
    assert!(!hidden.is_empty(), "the fixture really does hide cards");

    let (views, _rx, handle, task) = opened(state, 2).await;
    let mut seen = 0usize;
    for view in &views {
        for moment in &view.presentation {
            for object in objects(&moment.kind) {
                seen += 1;
                assert!(
                    !hidden.contains(&object.id),
                    "a hidden card ({}) reached the presentation window",
                    object.id
                );
                if object.card.is_some() {
                    assert!(
                        !hidden.contains(&object.id),
                        "and certainly never with a face"
                    );
                }
            }
        }
    }
    assert!(
        seen > 0,
        "the run really did produce moments carrying objects"
    );

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_automation_off_still_carries_moments_and_never_skips_a_seat() {
    // ADR 0020's opt-in stays opt-in. With automation off the room applies nothing on a
    // seat's behalf, so no `phases_skipped` can ever be produced — the accumulator stays
    // empty — while the ordinary moments still ride, because the coalescing outbox that
    // makes them necessary has nothing to do with automation.
    let (handle, task) = Room::new(spell_less_state(), db()).spawn();
    let (tx0, mut rx0) = view_channel();
    let (tx1, mut rx1) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    handle.send(RoomInput::Join {
        seat: 1,
        outbox: tx1,
    });
    let opening = wait_for_view(&mut rx0).await;
    let _ = wait_for_view(&mut rx1).await;
    assert!(
        opening.presentation.is_empty(),
        "nothing has happened yet, so the field elides entirely"
    );

    // Both seats pass in succession, which advances the step by hand. Each broadcast
    // reaches both channels, so both are drained each round — otherwise the next await
    // would return the *previous* round's view and prove nothing.
    let pass_for = |view: &GameView| {
        let action = view
            .valid_actions
            .iter()
            .find(|action| action.kind == "pass_priority")
            .expect("the priority holder is offered a pass");
        ClientMessage::ChooseAction(ChooseAction {
            action_id: action.id.clone(),
            token: action.token.clone(),
            ..Default::default()
        })
    };
    handle.send(RoomInput::Message {
        seat: 0,
        message: pass_for(&opening),
    });
    let mid = wait_for_view(&mut rx1).await;
    let _ = wait_for_view(&mut rx0).await;
    handle.send(RoomInput::Message {
        seat: 1,
        message: pass_for(&mid),
    });
    let latest = wait_for_view(&mut rx0).await;

    assert!(
        latest
            .presentation
            .iter()
            .any(|moment| matches!(moment.kind, MomentKind::PhaseChange { .. })),
        "the step change still produced a moment: {:?}",
        kinds(&latest.presentation)
    );
    assert!(
        skipped(&latest.presentation).is_empty(),
        "and automation being off means no seat was ever skipped"
    );
    assert!(!latest.auto_passed, "which the older indicator agrees with");

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_repeated_identical_events_arrive_as_one_counted_moment() {
    // Aggregation. A run of identical beats — the same trigger six times, the same
    // damage four times — would spend the whole window on one repetition, so consecutive
    // identical kinds inside a batch collapse to one caption with a tally.
    let mut state = spell_less_state();
    for _ in 0..3 {
        log_event(
            &mut state,
            GameEvent::DamageDealt {
                target: rune_engine::DamageTarget::Player(PlayerId(0)),
                amount: 1,
            },
        );
    }
    let (handle, task) = Room::new(state, db()).spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view = wait_for_view(&mut rx0).await;

    let damage: Vec<&PresentationMoment> = view
        .presentation
        .iter()
        .filter(|moment| matches!(moment.kind, MomentKind::Damage { .. }))
        .collect();
    assert_eq!(damage.len(), 1, "three identical hits are one moment");
    assert_eq!(damage[0].count, 3, "and the tally says how many");

    drop(handle);
    task.await.unwrap();
}

#[tokio::test]
async fn issue_594_a_long_settle_never_exceeds_the_presentation_window() {
    // The bound is part of the contract. A settle with nothing to stop for runs to its
    // cap, and the view that follows must not become the most expensive message the
    // protocol sends at exactly the moment the receiver is least able to watch it. A
    // client that is behind by more than the window catches up by watching the newest —
    // there is no backfill in this protocol.
    let (handle, task) = Room::new(spell_less_state(), db())
        .with_auto_pass(AutoPassPolicy::On)
        .spawn();
    let (tx0, mut rx0) = view_channel();
    handle.send(RoomInput::Join {
        seat: 0,
        outbox: tx0,
    });
    let view = wait_for_view(&mut rx0).await;

    assert!(
        view.turn > 2,
        "the settle really did run a long way: turn {}",
        view.turn
    );
    assert!(
        view.presentation.len() <= PRESENTATION_WINDOW,
        "the carried window is bounded: {} moments",
        view.presentation.len()
    );
    assert!(
        view.presentation
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id),
        "and what survives is the newest suffix, still in order"
    );
    assert!(
        view.presentation.first().map(|moment| moment.id) > Some(0),
        "a window that starts above the first id is normal, not a lost message"
    );

    drop(handle);
    task.await.unwrap();
}
