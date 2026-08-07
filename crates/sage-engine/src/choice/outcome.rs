//! What an answer *does*: the cards a selection moves, the mana or the recorded name a
//! colour or a card-name answer produces, the arrangement an ordering puts a remainder
//! back in, and the effects an accepted `you may` hands back to be spliced onto the
//! remainder.
//!
//! Split from the question half ([`super`]) along the seam the whole module is built on.
//! That half says what is being asked, who may answer, and what a legal answer looks
//! like; this half carries one out. Nothing here decides legality — every caller has
//! established it already — so each function writes rather than re-deciding.

use super::*;
use crate::id::PermanentId;

/// Carry out `request` for the chosen cards (and, where the outcome says so, for the
/// ones passed over).
///
/// The caller has already established that `chosen` is a legal answer; this moves cards
/// and records the log event, and decides nothing.
///
/// Returns what the answer left behind for its caller to carry: a **follow-up question**
/// where the outcome owes one — a look whose remainder its controller orders
/// (CR 701.17-adjacent "in any order") answers one question and immediately owes a second
/// — and the **permanent it put onto the battlefield**, where it put one there.
pub(crate) fn apply_choice_outcome(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) -> ChoiceAftermath {
    match request.outcome {
        ChoiceOutcome::Discard => {
            discard_chosen(state, request.subject, chosen, request.caused_by, db);
            ChoiceAftermath::default()
        }
        ChoiceOutcome::BottomChosen => {
            bottom_chosen(state, request.subject, chosen);
            ChoiceAftermath::default()
        }
        ChoiceOutcome::TakeAndBottomRest { destination, order } => {
            take_and_bottom_rest(state, request, chosen, destination, order, db)
        }
        ChoiceOutcome::TakeAndShuffle(destination) => ChoiceAftermath {
            next: None,
            entered: take_and_shuffle(state, request, chosen, destination, db),
        },
    }
}

/// What answering a card choice left behind — the two things its caller has to carry on
/// with, and nothing else.
///
/// A struct rather than a tuple because the two are unrelated: one is a question still
/// owed, the other a fact about what already happened. Both are `None` for every outcome
/// that neither asks again nor puts anything onto the battlefield, which is most of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ChoiceAftermath {
    /// A second question this answer created, to be queued behind it.
    pub next: Option<ChoiceQuestion>,
    /// The permanent this answer put onto the battlefield (CR 614's entry seam), which is
    /// what a reflexive trigger asking about *this way* names.
    pub entered: Option<crate::id::PermanentId>,
}

/// Discard cards to pay an additional cast cost (CR 601.2b).
///
/// The same move as any other discard, deliberately: a card discarded to a cost is
/// discarded (CR 701.8), and routing it here rather than writing a second hand-to-
/// graveyard move is what keeps that true as discards grow triggers and replacements.
/// It exists only because the cost path has already *chosen* the cards — they arrive in
/// the payment — so it needs the second half of a choice without the first.
pub(crate) fn discard_to_cost(
    state: &mut GameState,
    subject: PlayerId,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) {
    // A cost is paid by the player casting or activating, so they are the cause — which
    // is what keeps `a spell an **opponent** controls caused you to discard` false of a
    // card its own controller pitched.
    discard_chosen(state, subject, chosen, Some(subject), db);
}

/// Move the chosen cards from the subject's hand to their graveyard (CR 701.8) and log
/// how many moved — never which, since a hand is hidden and a count is all the other
/// seats are entitled to. (The graveyard itself is public, so the cards become visible
/// there on their own.)
fn discard_chosen(
    state: &mut GameState,
    subject: PlayerId,
    chosen: &[CardInstanceId],
    caused_by: Option<PlayerId>,
    db: &CardDatabase,
) {
    let mut discarded = 0u32;
    for id in chosen {
        let Some(player) = state.players.get_mut(subject.0) else {
            break;
        };
        let Some(pos) = player.hand.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.hand.remove(pos);
        state.put_card_in_graveyard(subject, card, db);
        discarded += 1;
    }
    if discarded > 0 {
        state.record_event(GameEvent::CardsDiscarded {
            player: subject,
            count: discarded,
            caused_by,
        });
    }
}

/// Put the chosen cards on the bottom of the subject's library in the order they were
/// chosen — scry's half that moves anything (CR 701.17). The looked-at cards left
/// unchosen are already on top in their existing order, so they need no work.
fn bottom_chosen(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    let Some(player) = state.players.get_mut(subject.0) else {
        return;
    };
    let mut moved = Vec::with_capacity(chosen.len());
    for id in chosen {
        if let Some(pos) = player.library.iter().position(|card| card.id == *id) {
            moved.push(player.library.remove(pos));
        }
    }
    put_on_bottom(state, subject, moved);
}

/// Put `cards` on the bottom of the subject's library, the **first one deepest**.
///
/// The single convention every bottoming in the engine follows — a scry's chosen cards,
/// a look's random remainder, a look's *ordered* remainder — so "the order they were
/// chosen" means one thing everywhere rather than one thing per caller. The cards must
/// already have been removed from wherever they were.
fn put_on_bottom(state: &mut GameState, subject: PlayerId, cards: Vec<CardInstance>) {
    let Some(player) = state.players.get_mut(subject.0) else {
        return;
    };
    // The bottom of the library is index 0, so inserting back-to-front leaves the first
    // card of `cards` at index 0 — the deepest.
    for card in cards.into_iter().rev() {
        player.library.insert(0, card);
    }
}

/// Take the chosen cards to `destination` and deal with every *other* card this choice
/// looked at, as `order` says.
///
/// The looked-at set is the request's own window onto the library
/// ([`ChoiceZone::LibraryTop`]) rather than the filtered candidate list: a look at the
/// top four bottoms all four, not only the ones that could have been taken.
///
/// The two orders take deliberately different roads through the library:
///
/// - [`BottomOrder::Random`] settles here. Every looked-at card leaves the library, the
///   taken ones go to `destination`, and the rest are shuffled from the seeded stream and
///   put on the bottom.
/// - [`BottomOrder::Chosen`] settles *later*. Only the taken cards leave; the remainder
///   is untouched and therefore still sitting on top of the library, exactly where an
///   [`OrderRequest`] over the top N of it can find it — which is what lets the follow-up
///   question derive its own cards rather than carry a snapshot (ADR 0013 §2). **No
///   randomness is drawn on this road at all**, so a game whose looker orders their own
///   remainder replays byte-identically.
fn take_and_bottom_rest(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    destination: FoundDestination,
    order: BottomOrder,
    db: &CardDatabase,
) -> ChoiceAftermath {
    let ChoiceZone::LibraryTop(count) = request.zone else {
        return ChoiceAftermath::default();
    };
    let looked_at: Vec<CardInstance> = state
        .players
        .get(request.subject.0)
        .map(|player| {
            player
                .library
                .iter()
                .rev()
                .take(count as usize)
                .copied()
                .collect()
        })
        .unwrap_or_default();
    let taken: Vec<CardInstance> = looked_at
        .iter()
        .filter(|card| chosen.contains(&card.id))
        .copied()
        .collect();
    let remainder = looked_at.len() - taken.len();
    // Whichever road follows, the cards being *taken* leave the library first, so nothing
    // below can find one in two places at once.
    if let Some(player) = state.players.get_mut(request.subject.0) {
        player
            .library
            .retain(|card| !taken.iter().any(|seen| seen.id == card.id));
    }
    let mut entered = None;
    for card in taken {
        entered = place_card(state, request.subject, card, destination, db).or(entered);
    }
    match order {
        BottomOrder::Random => {
            let mut rest: Vec<CardInstance> = looked_at
                .into_iter()
                .filter(|card| !chosen.contains(&card.id))
                .collect();
            if let Some(player) = state.players.get_mut(request.subject.0) {
                player
                    .library
                    .retain(|card| !rest.iter().any(|seen| seen.id == card.id));
            }
            // "in a random order" — the seeded stream, so the same seed replays the same
            // bottom order and the chooser learns nothing about their future draws.
            let mut rng = SplitMix64::new(state.rng_seed);
            rng.shuffle(&mut rest);
            state.rng_seed = rng.state();
            put_on_bottom(state, request.subject, rest);
            ChoiceAftermath {
                next: None,
                entered,
            }
        }
        // "in any order" — the looker decides, and a remainder of nothing or of one card
        // is not a decision (ADR 0013 §5). The one-card case still has to *move*, so it
        // is bottomed here rather than asked about.
        BottomOrder::Chosen => {
            let next = match u8::try_from(remainder).unwrap_or(u8::MAX) {
                0 => None,
                1 => {
                    let single = state
                        .players
                        .get_mut(request.subject.0)
                        .and_then(|player| player.library.pop());
                    put_on_bottom(state, request.subject, single.into_iter().collect());
                    None
                }
                count => Some(ChoiceQuestion::Order(OrderRequest {
                    subject: request.subject,
                    count,
                })),
            };
            ChoiceAftermath { next, entered }
        }
    }
}

/// Take the chosen cards to `destination` and shuffle the searched library
/// (CR 701.19c) — including when nothing was found, since the shuffle is what stops a
/// failed search from being a free look at the deck.
fn take_and_shuffle(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    destination: FoundDestination,
    db: &CardDatabase,
) -> Option<PermanentId> {
    let mut entered = None;
    for id in chosen {
        let Some(player) = state.players.get_mut(request.subject.0) else {
            break;
        };
        let Some(pos) = player.library.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.library.remove(pos);
        entered = place_card(state, request.subject, card, destination, db).or(entered);
    }
    let mut library = state
        .players
        .get(request.subject.0)
        .map(|player| player.library.clone())
        .unwrap_or_default();
    let mut rng = SplitMix64::new(state.rng_seed);
    rng.shuffle(&mut library);
    state.rng_seed = rng.state();
    if let Some(player) = state.players.get_mut(request.subject.0) {
        player.library = library;
    }
    state.record_event(GameEvent::LibrarySearched {
        player: request.subject,
    });
    entered
}

/// Put one card that has already been removed from its zone into `destination`.
///
/// A card headed for the battlefield goes through the one battlefield-entry seam
/// ([`GameState::put_card_onto_battlefield`]), so it mints a fresh
/// [`PermanentId`](crate::PermanentId), applies its own CR 614 enters-the-battlefield
/// replacements, and is picked up by the trigger diff exactly as a resolving permanent
/// spell is.
///
/// Returns the permanent it made, for the reflexive trigger that asks what was put onto
/// the battlefield *this way*. A card headed for a hand made none, and so does a found
/// card whose entry the seam **deferred** onto the choice queue — one that names a colour
/// or a card as it enters (CR 614.12) is not on the battlefield yet, and there is no id
/// to hand back until it is.
fn place_card(
    state: &mut GameState,
    subject: PlayerId,
    card: CardInstance,
    destination: FoundDestination,
    db: &CardDatabase,
) -> Option<PermanentId> {
    match destination {
        FoundDestination::Hand => {
            if let Some(player) = state.players.get_mut(subject.0) {
                player.hand.push(card);
            }
            None
        }
        FoundDestination::Battlefield => {
            state.put_card_onto_battlefield(card, subject, false, None, db)
        }
        FoundDestination::BattlefieldTapped => {
            state.put_card_onto_battlefield(card, subject, true, None, db)
        }
    }
}

/// Carry out `request` for the ordering just submitted: the named cards leave the top of
/// the subject's library and go to the bottom, the **first named deepest**.
///
/// The counterpart of [`apply_choice_outcome`] for the ordering question, and the whole
/// of what an ordering answer does. The caller has established that `order` is a
/// permutation of exactly the cards the request covers
/// ([`order_answer_is_legal`](super::order_answer_is_legal)), so a card named here that
/// is somehow no longer on top is skipped rather than hunted for elsewhere.
///
/// **Nothing here draws from the RNG.** The order is the player's, and reaching for the
/// seeded stream to place cards a player already placed would fork every later shuffle
/// on replay.
pub(crate) fn apply_order_outcome(
    state: &mut GameState,
    request: &OrderRequest,
    order: &[CardInstanceId],
) {
    let Some(player) = state.players.get_mut(request.subject.0) else {
        return;
    };
    let mut moved = Vec::with_capacity(order.len());
    for id in order {
        if let Some(pos) = player.library.iter().position(|card| card.id == *id) {
            moved.push(player.library.remove(pos));
        }
    }
    put_on_bottom(state, request.subject, moved);
}

/// Carry out `request` for the colour just named — the colour counterpart of
/// [`apply_choice_outcome`], and the only place a colour answer has a consequence.
///
/// The queue entry is popped by the caller, exactly as a card selection's is, and the
/// rest of a suspended resolution rides on the [`Resume`] attached to the last question
/// of the effect.
pub(crate) fn apply_color_outcome(
    state: &mut GameState,
    chooser: PlayerId,
    request: &ColorRequest,
    color: Color,
    db: &CardDatabase,
) {
    match &request.outcome {
        ColorOutcome::AddMana {
            amount,
            restriction,
        } => {
            let Some(player) = state.players.get_mut(chooser.0) else {
                return;
            };
            match restriction {
                Some(restriction) => {
                    player
                        .mana_pool
                        .add_restricted(color, *amount, restriction.clone());
                }
                None => player.mana_pool.add(color, *amount),
            }
        }
        // The deferred half of a battlefield entry (CR 614.12): the answer is written
        // onto the event and the event is handed straight back to the entry seam, which
        // asks whatever is still unanswered and then lets the permanent arrive.
        ColorOutcome::RecordOnEntry(entry) => {
            let mut entry = entry.clone();
            entry.chosen_color = Some(color);
            state.begin_battlefield_entry(entry, db);
        }
    }
}

/// Record the card just named on the entry that was waiting for it (CR 614.12) and hand
/// the event back to the battlefield-entry seam.
///
/// The card-name counterpart of [`apply_color_outcome`]'s second arm, and deliberately
/// the same two lines: an answer fills one slot on the event and re-enters
/// [`GameState::begin_battlefield_entry`], which is the one function that decides whether
/// anything is still owed. Routing back rather than completing here is what lets a card
/// that asked two questions be asked the second one without a branch saying so, and what
/// makes the loop terminate — every pass fills a slot that is never emptied.
pub(crate) fn apply_card_name_outcome(
    state: &mut GameState,
    request: &CardNameRequest,
    named: CardId,
    db: &CardDatabase,
) {
    let mut entry = request.entry.clone();
    entry.named_card = Some(named);
    state.begin_battlefield_entry(entry, db);
}

/// Record the copiable values the named permanent has right now (CR 707.2) on the entry
/// that was waiting for them, and hand the event back to the battlefield-entry seam.
///
/// The copy counterpart of [`apply_card_name_outcome`], and deliberately the same two
/// lines: an answer fills one slot on the event and re-enters
/// [`GameState::begin_battlefield_entry`], which is the one function that decides whether
/// anything is still owed.
///
/// `named` is `None` for a decline — `You may have this enter as a copy …`, answered with
/// "no". The entry then completes copying nothing, which is exactly what declining means,
/// and the empty slot is filled with a decision rather than left to be asked again: the
/// `copies_on_entry` gate is only reached while `entry.copied` is empty, so the recorded
/// `None` would loop. It is therefore written as a *taken* decision — see
/// [`PendingEntry::copied`](crate::replacement::PendingEntry).
///
/// The snapshot is taken **now** and never again (CR 707.2b/707.2c): whatever the named
/// permanent's copiable values are at this instant is what the copy has for as long as it
/// is one.
pub(crate) fn apply_permanent_outcome(
    state: &mut GameState,
    request: &CopyChoiceRequest,
    named: Option<crate::id::PermanentId>,
    db: &CardDatabase,
) {
    match &request.outcome {
        CopyChoiceOutcome::RecordOnEntry { entry, subject } => {
            let copied = named
                .and_then(|id| crate::copy::copiable_values_of(state, id))
                .map(|printed| crate::copy::CopiedValues {
                    printed,
                    subject: *subject,
                });
            let mut entry = entry.clone();
            entry.copied = Some(copied);
            state.begin_battlefield_entry(entry, db);
        }
    }
}

/// Put the cards a [`PlayCardRequest`] named on the **bottom of their owner's library, in a
/// random order** — the *then put the exiled cards that weren't cast this way on the bottom
/// of that library* of Chaos Wand (issue #787).
///
/// `played` is the card the player took the offer on, if they did: it is on the stack and
/// must not be swept back. Everything else the request named goes back wherever it currently
/// is, which for this card is exile.
///
/// The order is drawn from the **seeded** stream (ADR 0006), so the same seed replays the
/// same bottom order and nobody learns anything about the library's future by watching.
///
/// The owner is whoever the cards belong to — not the player who was offered them. Chaos
/// Wand digs through an opponent's library and puts what it did not cast back into *that*
/// library.
pub(crate) fn bottom_the_rest(
    state: &mut GameState,
    request: &crate::choice::PlayCardRequest,
    played: Option<crate::id::CardInstanceId>,
) {
    let mut rest: Vec<CardInstance> = request
        .bottom_after
        .iter()
        .copied()
        .filter(|card| Some(card.id) != played)
        .collect();
    if rest.is_empty() {
        return;
    }
    // Lift them out of whichever pile they are sitting in. Only the owner's zones are
    // searched, because a card being put back into a library is being put back into its
    // owner's (CR 400.3).
    let mut owners: Vec<crate::id::PlayerId> = Vec::new();
    for (index, player) in state.players.iter_mut().enumerate() {
        let before = player.exile.len() + player.library.len();
        player
            .exile
            .retain(|card| !rest.iter().any(|c| c.id == card.id));
        player
            .library
            .retain(|card| !rest.iter().any(|c| c.id == card.id));
        if player.exile.len() + player.library.len() != before {
            owners.push(crate::id::PlayerId(index));
        }
    }
    let mut rng = SplitMix64::new(state.rng_seed);
    rng.shuffle(&mut rest);
    state.rng_seed = rng.state();
    // Every card this effect exiled came off one library, so there is exactly one owner to
    // put them back under; a stray card whose owner cannot be found is dropped rather than
    // handed to somebody it does not belong to.
    if let Some(owner) = owners.first().copied() {
        put_on_bottom(state, owner, rest);
    }
}
