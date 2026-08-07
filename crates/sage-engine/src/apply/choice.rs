//! Answering the mid-resolution player choice the game is waiting on.

use super::*;
use crate::choice::{
    apply_choice_outcome, apply_order_outcome, apply_permanent_choice, pending_player_choice,
    take_confirmed_effects, ChoiceQuestion, PendingChoice,
};
use crate::id::{CardInstanceId, PermanentId};
use crate::resolve::resume_after_choice;

/// Answer the choice at the head of [`GameState::pending_choices`](crate::GameState)
/// with `chosen`, then let the suspended resolution continue.
///
/// Three steps, in this order and no other:
///
/// 1. **Take the choice off the queue.** It is answered; leaving it there while its
///    outcome runs would make [`pending_player_choice`] briefly disagree with reality
///    and, worse, let the resumed resolution see its own unanswered question.
/// 2. **Carry out the outcome** — the chosen cards move, and so do the ones passed over
///    where the outcome says so (a search shuffles, a look bottoms what it looked at).
/// 3. **Resume**, if this was the last choice its effect posed: the rest of the
///    suspended object's effects, and for a spell the card's final zone (CR 608.3).
///    That continuation may itself pose a further choice, which simply queues behind
///    whatever is left — a card that draws and then discards suspends twice.
///
/// An outcome may hand back a **follow-up question** instead of finishing — *put the rest
/// on the bottom in any order* is a second decision that only exists once the first is
/// answered, since until then nobody knows what "the rest" is. That question is queued
/// and the suspended remainder is carried onto it, so resuming still happens exactly
/// once, after the *last* question the effect asked. Nothing else changes: it is the same
/// queue, the same chooser, and the same hand-off.
///
/// Legality has already been established by [`crate::apply_action`]'s gate
/// ([`crate::choice::answer_is_legal`]), so this writes rather than re-deciding. An
/// answer with no choice pending is a no-op.
pub(crate) fn apply_answer_choice(
    state: &mut GameState,
    chosen: &[CardInstanceId],
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Cards(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Cards(request) = &answered.question else {
        return;
    };
    let aftermath = apply_choice_outcome(state, request, chosen, db);
    // What the answer put onto the battlefield is written onto the resumed resolution
    // ([`Resolution::entered`](crate::Resolution)) — the only moment it could be, since
    // *which* card was taken is the player's decision and no event records an entry. It
    // rides onto a follow-up question too, so a look whose remainder is ordered still
    // reaches the rest of the card knowing what it put there.
    let mut resume = answered.resume;
    if let (Some(resume), Some(entered)) = (resume.as_mut(), aftermath.entered) {
        resume.resolution.entered = Some(entered);
    }
    if let Some(question) = aftermath.next {
        // Behind anything the outcome itself queued (a found permanent naming a colour
        // as it enters), because those are questions about steps that already happened.
        state.pending_choices.push(PendingChoice {
            chooser: answered.chooser,
            question,
            resume,
        });
        return;
    }
    if let Some(resume) = resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **card ordering** with `order`: put those cards on the bottom of
/// the library in that order, then let the suspended resolution continue.
///
/// The same three steps [`apply_answer_choice`] takes. It is the second half of a look
/// that says *in any order*: the first answer decided what was taken, this one decides
/// where the rest sits, and the [`Resume`](crate::Resume) rode across from the first
/// question to this one so the rest of the card happens after both.
///
/// The answer has already been checked to be a permutation of the freshly recomputed
/// remainder ([`crate::choice::order_answer_is_legal`]), so this writes rather than
/// re-deciding — and it **draws nothing from the RNG**, which is the whole difference
/// between this road and the random one: a replay that feeds back the same answers has to
/// leave the seeded stream exactly where an ordered bottoming found it.
///
/// An answer with no ordering choice pending is a no-op.
pub(crate) fn apply_answer_order(
    state: &mut GameState,
    order: &[CardInstanceId],
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Order(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Order(request) = &answered.question else {
        return;
    };
    apply_order_outcome(state, request, order);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **permanent choice** with `chosen`: sacrifice what was named, then
/// let the suspended resolution continue.
///
/// The same three steps [`apply_answer_choice`] takes, over objects on the battlefield
/// rather than cards in a zone. Legality has already been established by
/// [`crate::apply_action`]'s gate
/// ([`crate::choice::answer_permanents_is_legal`](crate::choice)), so this writes rather
/// than re-deciding, and an answer with no permanent choice pending is a no-op.
///
/// **How many were named is written onto the resumed resolution**
/// ([`Resolution::sacrificed`](crate::Resolution)), which is the only moment it could be:
/// the size of an open sacrifice is the player's decision, so it does not exist until here,
/// and a sacrificed land leaves no event behind for a later effect to count (CR 700.4 —
/// only a creature dies). It is what the `up to that many` of a search that follows reads.
pub(crate) fn apply_answer_permanents(
    state: &mut GameState,
    chosen: &[PermanentId],
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Permanents(_)) = pending_player_choice(state).map(|p| &p.question)
    else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Permanents(request) = &answered.question else {
        return;
    };
    apply_permanent_choice(state, request, chosen, db);
    if let Some(mut resume) = answered.resume {
        resume.resolution.sacrificed = resume
            .resolution
            .sacrificed
            .saturating_add(u32::try_from(chosen.len()).unwrap_or(u32::MAX));
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **color choice** with `color`: carry out whatever the question was
/// for, then let any suspended resolution continue.
///
/// The same three steps [`apply_answer_choice`] takes, and the simplest instance of
/// them: there is no candidate set to re-derive, because every color is always a legal
/// answer (CR 105.1), and no aftermath for the answers not given. An effect producing
/// more than one mana queued one question per point, so answering this one leaves the
/// next at the head of the queue and the player is asked again — which is the whole
/// meaning of "in any combination of colors".
///
/// The other thing a colour answer can do is **finish a battlefield entry** (CR 614.12):
/// the permanent that was waiting for it arrives here, colour and all. It carries no
/// [`Resume`](crate::Resume) — nothing was suspended, because the entry is the last step
/// of a resolution rather than one of its effects — so the `if let` below simply finds
/// nothing, which is the correct amount of special-casing.
///
/// An answer with no color choice pending is a no-op.
pub(crate) fn apply_answer_color(
    state: &mut GameState,
    color: crate::mana::Color,
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Color(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Color(request) = &answered.question else {
        return;
    };
    crate::choice::apply_color_outcome(state, answered.chooser, request, color, db);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **amount** question with `value`: charge it as generic mana, and
/// create the reflexive ability the payment bought — with the value substituted into its
/// effects (CR 603.11).
///
/// The substitution is the point. `Return target creature card with mana value X` is a
/// sentence about a number nobody had until this moment, and the ability that says it is
/// aimed *after* this, when it goes on the stack. Writing the value into the effects here
/// means every later reader — the candidate enumeration, the legality gate, the CR 608.2b
/// re-check — sees an ordinary spec naming a concrete mana value, and none of them has to
/// know an X was ever involved.
///
/// Legality — that an amount is owed at all, and that `value` is within the bounds
/// recomputed against the current pool — has already been established by
/// [`crate::apply_action`]'s gate. An answer with no amount pending is a no-op.
pub(crate) fn apply_answer_number(state: &mut GameState, value: u32, db: &CardDatabase) {
    let Some(ChoiceQuestion::Number(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Number(request) = &answered.question else {
        return;
    };
    // Charged through the one pool seam every other payment uses. The gate established it
    // is payable, so this cannot fail — and if the pool somehow could not cover it, the
    // ability the payment bought is simply not created.
    let cost = crate::mana::ManaCost {
        generic: u8::try_from(value).unwrap_or(u8::MAX),
        ..Default::default()
    };
    let charged = state
        .players
        .get(answered.chooser.0)
        .and_then(|player| player.mana_pool.pay(&cost));
    let paid = match (charged, state.players.get_mut(answered.chooser.0)) {
        (Some(pool), Some(player)) => {
            player.mana_pool = pool;
            true
        }
        _ => false,
    };
    if paid && !request.effects.is_empty() {
        if let Some(source) = request.source {
            let source_power = crate::characteristics::characteristics(state, source, db).power;
            state
                .reflexive_triggers
                .push(crate::reflexive::PendingReflexive {
                    controller: answered.chooser,
                    source,
                    source_power,
                    effects: crate::choice::with_paid_x(&request.effects, value),
                });
        }
    }
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **CR 614.12 card-naming choice** with `named`: record it on the/// Answer the pending **CR 614.12 card-naming choice** with `named`: record it on the
/// entry that was waiting and let the permanent arrive.
///
/// The same three steps [`apply_answer_choice`] takes, and — like the colour answer's
/// entry arm — what is suspended is an *event* rather than a resolution, so there is no
/// [`Resume`](crate::Resume) and the continuation is
/// [`GameState::begin_battlefield_entry`]. Routing back through that one function is what
/// lets a card that also names a colour be asked both without either question knowing
/// about the other.
///
/// Legality — that a card-naming choice is owed at all, and that `named` is in its freshly
/// derived candidate list — has already been established by [`crate::apply_action`]'s
/// gate. An answer with no card-naming choice pending is a no-op.
pub(crate) fn apply_answer_card_name(
    state: &mut GameState,
    named: crate::id::CardId,
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::CardName(_)) = pending_player_choice(state).map(|p| &p.question)
    else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::CardName(request) = &answered.question else {
        return;
    };
    crate::choice::apply_card_name_outcome(state, request, named, db);
}

/// Answer the pending **CR 616.1 ordering choice**: apply the named replacement to the
/// suspended battlefield entry, then hand the entry back to the layer.
///
/// The same three steps [`apply_answer_choice`] takes, over a different kind of
/// suspended thing. What was suspended here is not a resolution but an *event* — the
/// entry is waiting in no zone at all — so there is no [`Resume`](crate::Resume) to run
/// and the continuation is [`GameState::begin_battlefield_entry`], the one function the
/// entry seam itself calls. That is the whole point of routing it back rather than
/// finishing here: if two replacements still apply the question is asked again, if one
/// does it is applied, and if none does the permanent arrives — by exactly the code an
/// unreplaced entry goes through.
///
/// The option is re-derived from the state and the event, never read back from a list
/// snapshotted when the question was posed, and `index` has already been bounds-checked
/// by [`crate::apply_action`]'s gate. An answer with no ordering choice pending is a
/// no-op.
pub(crate) fn apply_answer_replacement(state: &mut GameState, index: u8, db: &CardDatabase) {
    let Some(ChoiceQuestion::Replacement(_)) = pending_player_choice(state).map(|p| &p.question)
    else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Replacement(request) = answered.question else {
        return;
    };
    let mut entry = request.entry;
    let options = crate::replacement::applicable_to_entry(state, db, &entry);
    let Some(&option) = options.get(usize::from(index)) else {
        return;
    };
    if crate::replacement::apply_to_entry(state, &mut entry, option, db)
        == crate::replacement::EntryOutcome::Replaced
    {
        return;
    }
    state.begin_battlefield_entry(entry, db);
}

/// Answer the pending **CR 614.12 permanent choice**: record the named permanent's
/// copiable values on the entry that was waiting for them, and complete that entry.
///
/// The same three steps [`apply_answer_choice`] takes, over the same kind of suspended
/// thing [`apply_answer_color`] finishes: an arrival, not a resolution. There is no
/// [`Resume`](crate::Resume) — the entry is the last step of whatever produced it — so the
/// permanent simply arrives, already a copy.
///
/// A `None` answer is a decline (or a question the board could not answer): the entry
/// completes copying nothing, which is exactly a permanent entering as itself. Legality
/// has been established by [`crate::apply_action`]'s gate; this writes rather than
/// re-deciding. An answer with no permanent choice pending is a no-op.
pub(crate) fn apply_answer_permanent(
    state: &mut GameState,
    chosen: Option<crate::id::PermanentId>,
    db: &CardDatabase,
) {
    let Some(ChoiceQuestion::Permanent(_)) = pending_player_choice(state).map(|p| &p.question)
    else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Permanent(request) = &answered.question else {
        return;
    };
    crate::choice::apply_permanent_outcome(state, request, chosen, db);
    if let Some(resume) = answered.resume {
        resume_after_choice(state, resume, db);
    }
}

/// Answer the pending **yes-or-no** with `accept`, then let the suspended resolution
/// continue (CR 608.2 — see [`crate::Effect::May`]).
///
/// The same three steps [`apply_answer_choice`] takes, with one difference that is the
/// whole design: an accepted effect is not applied *here*. It is spliced onto the front
/// of the suspended remainder and resumed through the ordinary effect walk, so
///
/// - the optional effects and the effects that followed them run in one pass, in
///   printed order;
/// - an accepted effect that poses a *further* choice suspends exactly as any other
///   effect would, with no second mechanism;
/// - declining is the same code path with nothing spliced, which is what makes "a
///   decline leaves the game as if the effect were absent" true by construction rather
///   than by matching two branches carefully.
///
/// The offer's **targets** are spliced with its effects, for the third reason: a `may`
/// declares the group of the effect it wraps, so the target was chosen at announcement
/// and must be handed back to that effect on acceptance — and must go nowhere at all on
/// a decline, or the next effect in the remainder would inherit a target that was never
/// aimed at it. The wrapped effect re-checks it on the way through (CR 608.2c), so a
/// target that has become illegal is skipped rather than applied.
///
/// A cost whose payment the chooser **picks** — a sacrifice, a discard — is not paid
/// here either: accepting poses that selection as an ordinary choice and hangs the whole
/// remainder, spliced effects and all, behind it. Three things follow, and they are the
/// reason the payment is a question rather than a field on the answer: the cost is paid
/// before the effects it bought, a permanent sacrificed to it dies down the one
/// leaves-battlefield seam every other sacrifice uses, and the player picks *which* one
/// with the same action a mandatory sacrifice is answered by.
///
/// Paying happens in [`take_confirmed_effects`], atomically with the acceptance.
/// Legality — including whether the cost is payable at all — has already been
/// established by [`crate::apply_action`]'s gate. An answer with no yes-or-no pending is
/// a no-op.
pub(crate) fn apply_answer_confirm(state: &mut GameState, accept: bool, db: &CardDatabase) {
    // CR 608.2f: the same "no" also declines an offer to play a card (issue #787). It is
    // the only answer that question takes — the "yes" is the player really playing it —
    // so it is settled here rather than in a variant of its own.
    if pending_player_choice(state).is_some_and(|p| p.question.play_card().is_some()) {
        decline_offered_play(state, db);
        return;
    }
    let Some(ChoiceQuestion::Confirm(_)) = pending_player_choice(state).map(|p| &p.question) else {
        return;
    };
    let answered = state.pending_choices.remove(0);
    let ChoiceQuestion::Confirm(request) = &answered.question else {
        return;
    };
    let taken = take_confirmed_effects(state, answered.chooser, request, accept, db);
    // A confirmation is the only choice its effect poses, so it always carries the
    // remainder; without one there is nothing left to resolve and nothing to splice on.
    let Some(mut resume) = answered.resume else {
        return;
    };
    let mut owed = None;
    // CR 603.11: an accepted `you may pay … when you do` does not splice its effects into
    // this resolution. It creates an ability, which goes on the stack through the ordinary
    // trigger seam and is aimed there — after the payment, which is the whole point.
    // …unless the payment itself is still owed. `You may pay {X}` has an *amount* left to
    // name, and the ability the payment buys is a sentence about that amount — so the
    // reflexive is created by the answer to the number instead, with X written into it.
    // Creating it here would be creating an ability that says X and means nothing.
    let payment_owes_an_amount = taken
        .as_ref()
        .and_then(|taken| taken.payment.as_ref())
        .is_some_and(|(_, question)| matches!(question, ChoiceQuestion::Number(_)));
    if let Some(taken) = &taken {
        if request.reflexive && !payment_owes_an_amount && !taken.effects.is_empty() {
            let source_power = request
                .source
                .and_then(|id| crate::characteristics::characteristics(state, id, db).power);
            if let Some(source) = request.source {
                state
                    .reflexive_triggers
                    .push(crate::reflexive::PendingReflexive {
                        controller: answered.chooser,
                        source,
                        source_power,
                        effects: taken.effects.clone(),
                    });
            }
        }
    }
    // A reflexive offer normally has nothing left to splice or to ask. The exception is
    // the one above: its payment is a question, and it has to be posed.
    if payment_owes_an_amount {
        owed = taken.and_then(|taken| taken.payment);
    } else if let Some(mut taken) = taken.filter(|_| !request.reflexive) {
        let mut effects = taken.effects;
        effects.append(&mut resume.effects);
        resume.effects = effects;
        // The branch's own targets, which is the offer's on an acceptance and none on a
        // decline — an `unless` branch may not aim at anything (CR 601.2c chose the
        // offer's targets for the effect it wraps).
        taken.targets.append(&mut resume.targets);
        resume.targets = taken.targets;
        owed = taken.payment;
    }
    // The payment was established as answerable before the acceptance was recorded, so
    // `pose_choices` queues it; the `else` is the honest handling of a question that
    // turned out to have no answer after all, and resolves rather than stalling.
    if let Some(payment) = owed {
        if crate::choice::pose_choices(state, vec![payment], db) {
            crate::choice::attach_resume(state, resume);
            return;
        }
    }
    resume_after_choice(state, resume, db);
}

/// The player declined an offer to play a card (CR 608.2f, issue #787): the card takes the
/// other branch its own sentence named, and the suspended resolution picks up.
///
/// The consequence rides on the request rather than on the effects that follow, because by
/// the time the answer arrives nothing else knows which card was offered.
fn decline_offered_play(state: &mut GameState, db: &CardDatabase) {
    let answered = state.pending_choices.remove(0);
    state.interrupted_priority = None;
    if let Some(request) = answered.question.play_card() {
        match request.declined {
            crate::choice::DeclineOutcome::Exile => {
                if let Some(player) = state.players.get_mut(request.subject.0) {
                    if let Some(pos) = player
                        .library
                        .iter()
                        .position(|card| card.id == request.card.id)
                    {
                        let card = player.library.remove(pos);
                        player.exile.push(card);
                    }
                }
            }
            // Nothing to do: the card is where it was, which is the whole of the branch —
            // and for Chaos Wand that means it goes back with the rest, below.
            crate::choice::DeclineOutcome::Stay => {}
        }
        // Whatever the sentence said to put back goes back now. Nothing was played, so
        // every card the offer named is still there to bottom.
        crate::choice::bottom_the_rest(state, request, None);
    }
    if let Some(resume) = answered.resume {
        crate::resolve::resume_after_choice(state, resume, db);
    }
}
