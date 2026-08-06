//! What an answer *does*: the cards a selection moves, the mana or the recorded name a
//! colour or a card-name answer produces, and the effects an accepted `you may` hands
//! back to be spliced onto the remainder.
//!
//! Split from the question half ([`super`]) along the seam the whole module is built on.
//! That half says what is being asked, who may answer, and what a legal answer looks
//! like; this half carries one out. Nothing here decides legality — every caller has
//! established it already — so each function writes rather than re-deciding.

use super::*;

/// Carry out `request` for the chosen cards (and, where the outcome says so, for the
/// ones passed over).
///
/// The caller has already established that `chosen` is a legal answer; this moves cards
/// and records the log event, and decides nothing.
pub(crate) fn apply_choice_outcome(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) {
    match request.outcome {
        ChoiceOutcome::Discard => discard_chosen(state, request.subject, chosen),
        ChoiceOutcome::BottomChosen => bottom_chosen(state, request.subject, chosen),
        ChoiceOutcome::TakeAndBottomRest(destination) => {
            take_and_bottom_rest(state, request, chosen, destination, db);
        }
        ChoiceOutcome::TakeAndShuffle(destination) => {
            take_and_shuffle(state, request, chosen, destination, db);
        }
    }
}

/// Discard cards to pay an additional cast cost (CR 601.2b).
///
/// The same move as any other discard, deliberately: a card discarded to a cost is
/// discarded (CR 701.8), and routing it here rather than writing a second hand-to-
/// graveyard move is what keeps that true as discards grow triggers and replacements.
/// It exists only because the cost path has already *chosen* the cards — they arrive in
/// the payment — so it needs the second half of a choice without the first.
pub(crate) fn discard_to_cost(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    discard_chosen(state, subject, chosen);
}

/// Move the chosen cards from the subject's hand to their graveyard (CR 701.8) and log
/// how many moved — never which, since a hand is hidden and a count is all the other
/// seats are entitled to. (The graveyard itself is public, so the cards become visible
/// there on their own.)
fn discard_chosen(state: &mut GameState, subject: PlayerId, chosen: &[CardInstanceId]) {
    let mut discarded = 0u32;
    for id in chosen {
        let Some(player) = state.players.get_mut(subject.0) else {
            break;
        };
        let Some(pos) = player.hand.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.hand.remove(pos);
        player.graveyard.push(card);
        discarded += 1;
    }
    if discarded > 0 {
        state.record_event(GameEvent::CardsDiscarded {
            player: subject,
            count: discarded,
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
    // The bottom of the library is index 0, and the first card chosen ends up deepest.
    moved.reverse();
    for card in moved {
        player.library.insert(0, card);
    }
}

/// Take the chosen cards to `destination` and bottom every *other* card this choice
/// looked at, in a random order drawn from the seeded RNG.
///
/// The looked-at set is the request's own window onto the library
/// ([`ChoiceZone::LibraryTop`]) rather than the filtered candidate list: a look at the
/// top four bottoms all four, not only the ones that could have been taken.
fn take_and_bottom_rest(
    state: &mut GameState,
    request: &ChoiceRequest,
    chosen: &[CardInstanceId],
    destination: FoundDestination,
    db: &CardDatabase,
) {
    let ChoiceZone::LibraryTop(count) = request.zone else {
        return;
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
    // Every looked-at card leaves the library first, so nothing below can find a card
    // in two places at once; each is then put where the answer says.
    if let Some(player) = state.players.get_mut(request.subject.0) {
        player
            .library
            .retain(|card| !looked_at.iter().any(|seen| seen.id == card.id));
    }
    let (taken, mut rest): (Vec<CardInstance>, Vec<CardInstance>) = looked_at
        .into_iter()
        .partition(|card| chosen.contains(&card.id));
    for card in taken {
        place_card(state, request.subject, card, destination, db);
    }
    // "in a random order" — the seeded stream, so the same seed replays the same
    // bottom order and the chooser learns nothing about their future draws.
    let mut rng = SplitMix64::new(state.rng_seed);
    rng.shuffle(&mut rest);
    state.rng_seed = rng.state();
    if let Some(player) = state.players.get_mut(request.subject.0) {
        for card in rest.into_iter().rev() {
            player.library.insert(0, card);
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
) {
    for id in chosen {
        let Some(player) = state.players.get_mut(request.subject.0) else {
            break;
        };
        let Some(pos) = player.library.iter().position(|card| card.id == *id) else {
            continue;
        };
        let card = player.library.remove(pos);
        place_card(state, request.subject, card, destination, db);
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
}

/// Put one card that has already been removed from its zone into `destination`.
///
/// A card headed for the battlefield goes through the one battlefield-entry seam
/// ([`GameState::put_card_onto_battlefield`]), so it mints a fresh
/// [`PermanentId`](crate::PermanentId), applies its own CR 614 enters-the-battlefield
/// replacements, and is picked up by the trigger diff exactly as a resolving permanent
/// spell is.
fn place_card(
    state: &mut GameState,
    subject: PlayerId,
    card: CardInstance,
    destination: FoundDestination,
    db: &CardDatabase,
) {
    match destination {
        FoundDestination::Hand => {
            if let Some(player) = state.players.get_mut(subject.0) {
                player.hand.push(card);
            }
        }
        // Both battlefield arms discard the seam's answer: a found card that names a
        // colour or a card as it enters has its entry deferred onto the choice queue,
        // and nothing here needs the id of a permanent that does not exist yet.
        FoundDestination::Battlefield => {
            state.put_card_onto_battlefield(card, subject, false, None, db);
        }
        FoundDestination::BattlefieldTapped => {
            state.put_card_onto_battlefield(card, subject, true, None, db);
        }
    }
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

/// Answer the pending yes-or-no: hand the accepted effects back to be spliced onto the
/// front of the suspended remainder, or `None` for a decline.
///
/// Charging for the acceptance happens here too, because the charge and the answer are
/// one act — a `yes` that could not pay would be a `no` that had already moved cards.
/// The caller has established payability ([`confirm_is_payable`]); an unpayable cost
/// reaching here is treated as a decline rather than granting a free effect.
pub(crate) fn take_confirmed_effects(
    state: &mut GameState,
    chooser: PlayerId,
    request: &ConfirmRequest,
    accept: bool,
) -> Option<Vec<Effect>> {
    if !accept {
        state.record_event(GameEvent::OptionalDeclined { player: chooser });
        return None;
    }
    if let Some(cost) = &request.cost {
        let paid = state
            .players
            .get(chooser.0)
            .and_then(|player| player.mana_pool.pay(&crate::mana::parse_mana_cost(cost)));
        match (paid, state.players.get_mut(chooser.0)) {
            (Some(pool), Some(player)) => player.mana_pool = pool,
            _ => {
                state.record_event(GameEvent::OptionalDeclined { player: chooser });
                return None;
            }
        }
    }
    state.record_event(GameEvent::OptionalApplied { player: chooser });
    Some(request.effects.clone())
}
