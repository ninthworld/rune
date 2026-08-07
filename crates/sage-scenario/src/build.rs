//! Turning an authored [`Scenario`] into a validated [`GameState`] the room can run.
//!
//! **A position is constructed, not played into.** The alternative — scripting normal
//! actions from turn one until the board looks right — is slow, brittle, and needs a
//! different script every time a card in the way changes; so this starts from the engine's
//! own in-progress scaffold ([`GameState::new_multiplayer_with_seed`]) and writes the
//! position onto it directly. Every id it writes is minted from the engine's monotonic
//! counter, so the result is indistinguishable from a state the engine reached by playing.
//!
//! **Nothing is placed until everything is checked.** The build runs to completion in
//! memory and is then asked one last question — can the seat holding priority actually do
//! something? — before the caller is allowed to bind a socket. That last check is what
//! separates "this file is wrong" from "this file opened a browser onto a game that cannot
//! move", and it is the only one the field-by-field checks cannot make on their own.
//!
//! What is deliberately *not* here is any notion of a deck, a format, or legality: a
//! scenario is a position, and the server's format registry judges decklists for games that
//! start at turn one. A scenario's library is whatever the author wrote, in the order they
//! wrote it.

use std::collections::HashMap;

use sage_engine::{
    valid_actions, CardDatabase, CardId, CardInstance, Duration, EffectAffects, Face, FunctionalId,
    GameState, Modification, Permanent, PermanentId, PlayerId, Printed, StaticEffect, Step,
};
use sage_server::AiKind;

use crate::error::{ScenarioError, Site};
use crate::scenario::{PermanentScenario, PlayerScenario, Scenario};

/// A built, validated position and everything the runner needs to seat it.
#[derive(Clone, Debug)]
pub struct Position {
    /// The authoritative state the room is constructed around.
    pub state: GameState,
    /// Each seat's plan, in seat order: what to call it, and who plays it.
    pub seats: Vec<SeatPlan>,
    /// The scenario's own label, for the line the runner prints.
    pub name: Option<String>,
    /// The scenario's note, for the same line.
    pub note: Option<String>,
    /// The seed the engine and every AI seat draw from.
    pub seed: u64,
}

/// How one seat is played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatPlan {
    /// The display name every `GameView` labels this seat with.
    pub name: Option<String>,
    /// The server-side AI driving it, or `None` for a seat a person plays.
    pub ai: Option<AiKind>,
}

/// Build and validate the position `scenario` describes against the card catalog `db`.
///
/// # Errors
/// Returns the first [`ScenarioError`] the file is wrong about, in the order an author
/// would want to hear them: the shape of the table, then the seats it names, then the cards
/// it names, then the relationships between permanents, and finally whether the whole thing
/// is a position anyone can act in.
pub fn build(scenario: &Scenario, db: &CardDatabase) -> Result<Position, ScenarioError> {
    let seats = scenario.players.len();
    if seats < 2 {
        return Err(ScenarioError::NotEnoughPlayers(seats));
    }
    if scenario.turn == 0 {
        return Err(ScenarioError::TurnIsZero);
    }
    if scenario.active_player >= seats {
        return Err(ScenarioError::NoSuchSeat {
            field: "active_player",
            seat: scenario.active_player,
            seats,
        });
    }
    let priority = scenario.priority.unwrap_or(scenario.active_player);
    if priority >= seats {
        return Err(ScenarioError::NoSuchSeat {
            field: "priority",
            seat: priority,
            seats,
        });
    }
    let plans = seat_plans(scenario)?;

    let mut state = GameState::new_multiplayer_with_seed(seats, scenario.seed);
    state.turn = scenario.turn;
    state.active_player = PlayerId(scenario.active_player);
    state.priority = PlayerId(priority);
    state.step = Step::from(scenario.step);
    // The scaffold begins past any mulligan (CR 103.5): a scenario is a game already in
    // progress, and there is no opening hand to keep.
    state.mulligan = None;
    mark_turns_begun(&mut state, scenario.active_player, seats, scenario.turn);

    // Every permanent's label, so `attached_to` can be resolved once they all exist.
    let mut labels: HashMap<&str, PermanentId> = HashMap::new();
    for (seat, player) in scenario.players.iter().enumerate() {
        place_zones(&mut state, seat, player, db)?;
        place_battlefield(&mut state, seat, player, db, seats, &mut labels)?;
    }
    attach(&mut state, scenario, &labels)?;

    // The two questions the field-by-field checks cannot answer, and the ones that separate
    // "this file is wrong" from "this file opened a browser onto a game that cannot move".
    //
    // A seat at zero life is not *yet* a loss — `has_lost` is written by the state-based
    // actions, which have not run on a state nobody has acted in — so asking the engine
    // whether the game is over would say no and then end it on the first click. Checking
    // the life total says the same thing while there is still something to say it about.
    if let Some((seat, life)) = state
        .players
        .iter()
        .enumerate()
        .find_map(|(seat, player)| (player.life <= 0).then_some((seat, player.life)))
    {
        return Err(ScenarioError::LethalLife { seat, life });
    }
    if valid_actions(&state, db).is_empty() {
        return Err(ScenarioError::NoLegalAction);
    }

    Ok(Position {
        state,
        seats: plans,
        name: scenario.name.clone(),
        note: scenario.note.clone(),
        seed: scenario.seed,
    })
}

/// Resolve each seat's name and AI kind, refusing a kind the server does not have.
fn seat_plans(scenario: &Scenario) -> Result<Vec<SeatPlan>, ScenarioError> {
    scenario
        .players
        .iter()
        .enumerate()
        .map(|(seat, player)| {
            let ai = match player.ai.as_deref() {
                None => None,
                Some(kind) => {
                    Some(
                        AiKind::from_id(kind).ok_or_else(|| ScenarioError::UnknownAi {
                            seat,
                            kind: kind.to_string(),
                            known: AiKind::all().iter().map(|k| k.id().to_string()).collect(),
                        })?,
                    )
                }
            };
            Ok(SeatPlan {
                name: player.name.clone(),
                ai,
            })
        })
        .collect()
}

/// Stamp each seat's most recently begun turn (CR 302.6's other half).
///
/// The active player's turn is the current one; every other seat's is the most recent turn
/// before it, walking backwards through the seating order — which is what a round-robin
/// turn sequence leaves behind. A seat that would land before turn 1 has simply not had a
/// turn yet, which is `0` and is what makes a permanent it controls summoning sick.
fn mark_turns_begun(state: &mut GameState, active: usize, seats: usize, turn: u32) {
    for back in 0..seats {
        let seat = (active + seats - back) % seats;
        if let Some(player) = state.players.get_mut(seat) {
            player.turn_began = turn.saturating_sub(u32::try_from(back).unwrap_or(u32::MAX));
        }
    }
}

/// Put a seat's life, mana, and non-battlefield cards where the file says they are.
fn place_zones(
    state: &mut GameState,
    seat: usize,
    player: &PlayerScenario,
    db: &CardDatabase,
) -> Result<(), ScenarioError> {
    let hand = resolve_all(&player.hand, seat, Site::Hand, db)?;
    // The file lists the library top card first, because that is the order a person reads
    // one in; the engine draws off the end of the vector, so it is stored reversed.
    let mut library = resolve_all(&player.library, seat, Site::Library, db)?;
    library.reverse();
    let graveyard = resolve_all(&player.graveyard, seat, Site::Graveyard, db)?;
    let exile = resolve_all(&player.exile, seat, Site::Exile, db)?;
    let command = resolve_all(&player.command, seat, Site::Command, db)?;

    let mint = |state: &mut GameState, cards: Vec<CardId>| -> Vec<CardInstance> {
        cards
            .into_iter()
            .map(|card| state.new_instance(card))
            .collect()
    };
    let hand = mint(state, hand);
    let library = mint(state, library);
    let graveyard = mint(state, graveyard);
    let exile = mint(state, exile);
    let command = mint(state, command);

    let Some(seat_state) = state.players.get_mut(seat) else {
        return Ok(());
    };
    seat_state.life = player.life;
    seat_state.hand = hand;
    seat_state.library = library;
    seat_state.graveyard = graveyard;
    seat_state.exile = exile;
    seat_state.command = command;
    seat_state.mana_pool.white = player.mana.white;
    seat_state.mana_pool.blue = player.mana.blue;
    seat_state.mana_pool.black = player.mana.black;
    seat_state.mana_pool.red = player.mana.red;
    seat_state.mana_pool.green = player.mana.green;
    seat_state.mana_pool.colorless = player.mana.colorless;
    Ok(())
}

/// Put a seat's permanents on the shared battlefield, recording their labels.
fn place_battlefield<'a>(
    state: &mut GameState,
    seat: usize,
    player: &'a PlayerScenario,
    db: &CardDatabase,
    seats: usize,
    labels: &mut HashMap<&'a str, PermanentId>,
) -> Result<(), ScenarioError> {
    for entry in &player.battlefield {
        let card = resolve(&entry.card, seat, Site::Battlefield, db)?;
        let controller = match entry.controller {
            None => seat,
            Some(other) if other < seats => other,
            Some(other) => {
                return Err(ScenarioError::NoSuchSeat {
                    field: "controller",
                    seat: other,
                    seats,
                })
            }
        };
        let entered_turn = entered_turn(state, controller, entry, &entry.card)?;
        let id = PermanentId(state.mint_id());
        let instance = state.new_instance(card).id;
        if let Some(label) = entry.label.as_deref() {
            if labels.insert(label, id).is_some() {
                return Err(ScenarioError::DuplicateLabel {
                    label: label.to_string(),
                });
            }
        }
        state.battlefield.push(Permanent {
            id,
            instance,
            printed: Printed::Card {
                card,
                face: Face::from(entry.face),
            },
            // The seat a permanent is listed under owns it (CR 400.7), which is where it
            // goes when it leaves the battlefield. A different controller is a continuous
            // effect, applied below.
            controller: PlayerId(seat),
            tapped: entry.tapped,
            entered_turn,
            damage: entry.damage,
            counters: entry
                .counters
                .iter()
                .filter(|(_, count)| **count > 0)
                .map(|(kind, count)| (*kind, *count))
                .collect(),
            ..Permanent::default()
        });
        // A control change is CR 613 layer 2, never a written-down controller: every rule
        // that asks reads the computed answer, so a scenario expressing one this way is
        // expressing the same thing an `Act of Treason` does.
        if controller != seat {
            state.static_effects.push(StaticEffect {
                source: id.0,
                affects: EffectAffects::SpecificPermanent(id),
                modification: Modification::GainControl(PlayerId(controller)),
                duration: Duration::WhileOnBattlefield,
            });
        }
    }
    Ok(())
}

/// The turn a permanent came under its controller's control, from whether the file says it
/// is summoning sick (CR 302.6 — the engine's test is `entered_turn >= turn_began`).
fn entered_turn(
    state: &GameState,
    controller: usize,
    entry: &PermanentScenario,
    card: &str,
) -> Result<u32, ScenarioError> {
    let turn_began = state
        .players
        .get(controller)
        .map_or(0, |player| player.turn_began);
    if entry.summoning_sick {
        // Anything at or past the controller's current turn is sick, and the current turn
        // always is: `turn_began` is never later than it.
        return Ok(state.turn);
    }
    if turn_began == 0 {
        // Nothing this seat controls can have been controlled since a turn it has never
        // begun. Saying so beats silently handing back a sick permanent the file asked not
        // to be sick.
        return Err(ScenarioError::CannotBeEstablished {
            seat: controller,
            card: card.to_string(),
        });
    }
    Ok(turn_began - 1)
}

/// Resolve every `attached_to` once every permanent exists, so a file may attach in either
/// order.
fn attach(
    state: &mut GameState,
    scenario: &Scenario,
    labels: &HashMap<&str, PermanentId>,
) -> Result<(), ScenarioError> {
    // Walk the file and the battlefield in the same order they were built in, so the nth
    // authored permanent is the nth one placed.
    let mut placed = state
        .battlefield
        .iter()
        .map(|perm| perm.id)
        .collect::<Vec<_>>()
        .into_iter();
    let mut attachments: Vec<(PermanentId, PermanentId)> = Vec::new();
    for (seat, player) in scenario.players.iter().enumerate() {
        for entry in &player.battlefield {
            let Some(id) = placed.next() else { break };
            let Some(host) = entry.attached_to.as_deref() else {
                continue;
            };
            let host = *labels
                .get(host)
                .ok_or_else(|| ScenarioError::UnknownLabel {
                    seat,
                    label: host.to_string(),
                })?;
            if host == id {
                return Err(ScenarioError::AttachedToItself {
                    label: entry.attached_to.clone().unwrap_or_default(),
                });
            }
            attachments.push((id, host));
        }
    }
    for (id, host) in attachments {
        if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == id) {
            perm.attached_to = Some(host);
        }
    }
    Ok(())
}

/// Resolve one authored `functional_id` against the catalog.
fn resolve(
    card: &str,
    seat: usize,
    site: Site,
    db: &CardDatabase,
) -> Result<CardId, ScenarioError> {
    FunctionalId::try_from(card.to_string())
        .ok()
        .and_then(|id| db.card_id(&id))
        .ok_or_else(|| ScenarioError::UnknownCard {
            seat,
            site,
            card: card.to_string(),
        })
}

/// Resolve a whole authored list, reporting the first name that does not.
fn resolve_all(
    cards: &[String],
    seat: usize,
    site: Site,
    db: &CardDatabase,
) -> Result<Vec<CardId>, ScenarioError> {
    cards
        .iter()
        .map(|card| resolve(card, seat, site, db))
        .collect()
}

#[cfg(test)]
mod tests;
