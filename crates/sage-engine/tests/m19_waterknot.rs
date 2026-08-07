//! Waterknot: an Aura that taps what it lands on and keeps it there (issue #706).
//!
//! `Enchant creature. When this Aura enters, tap enchanted creature. Enchanted creature
//! doesn't untap during its controller's untap step.`
//!
//! Two things were missing, and they are deliberately different kinds of thing:
//!
//! - **Tapping the host** is an effect with no target. The card does not print the word
//!   *target*, because the Aura already chose what it enchants when it was cast
//!   (CR 601.2c); aiming again would be a second choice, and one the player could point
//!   somewhere else.
//! - **Not untapping** is a continuous modification of one rule (CR 502.4), not a
//!   characteristic and not the one-shot `skips_untap` flag a resolution sets. The
//!   difference shows in the second untap step: a flag is spent by the first one, and
//!   this is not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    apply_action, characteristics, forced_declaration_without_choice,
    priority_has_no_meaningful_action, Action, CardDatabase, CardId, CardInstance, Color,
    FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step, Target,
};

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    state.players[0].turn_began = state.turn;
    for player in &mut state.players {
        for color in [Color::Blue, Color::White, Color::Green] {
            player.mana_pool.add(color, 6);
        }
        player.mana_pool.add_colorless(6);
    }
    // A vanilla creature rather than a land: with an empty pool nothing in hand is a
    // meaningful action, so the walk between turns never stops to ask.
    let filler = cid(db, "bogstomper");
    for seat in 0..2 {
        let library: Vec<_> = (0..20).map(|_| state.new_instance(filler)).collect();
        state.players[seat].library = library;
    }
    state
}

fn place(
    state: &mut GameState,
    db: &CardDatabase,
    slug: &str,
    controller: PlayerId,
) -> PermanentId {
    let card = cid(db, slug);
    let instance = state.new_instance(card).id;
    let id = PermanentId(state.mint_id());
    state.battlefield.push(Permanent {
        id,
        instance,
        printed: card.into(),
        controller,
        ..Default::default()
    });
    id
}

fn to_hand(state: &mut GameState, db: &CardDatabase, slug: &str, seat: PlayerId) -> CardInstance {
    let instance = state.new_instance(cid(db, slug));
    state.players[seat.0].hand.push(instance);
    instance
}

fn tapped(state: &GameState, id: PermanentId) -> bool {
    state
        .battlefield
        .iter()
        .any(|perm| perm.id == id && perm.tapped)
}

/// Walk forward to seat 0's **next** turn, which means its untap step has happened.
///
/// The step itself is not a place the game stops — its turn-based action runs and play
/// moves on — so the observable fact is asserted on the far side of it, in the upkeep.
fn past_my_next_untap(state: &GameState, db: &CardDatabase) -> GameState {
    let mut state = state.clone();
    let goal = state.turn + 2;
    for _ in 0..400 {
        if state.turn >= goal && state.active_player == PlayerId(0) && state.step != Step::Untap {
            return state;
        }
        let action = if priority_has_no_meaningful_action(&state, db) {
            Action::PassPriority
        } else {
            forced_declaration_without_choice(&state, db).unwrap_or_else(|| match state.step {
                Step::DeclareAttackers => Action::DeclareAttackers {
                    attackers: Vec::new(),
                },
                Step::DeclareBlockers => Action::DeclareBlockers { blocks: Vec::new() },
                step => panic!("the walk stalled at turn {} {step:?}", state.turn),
            })
        };
        state = apply_action(&state, &action, db);
    }
    panic!("the walk ran past its cap");
}

/// Cast the Aura onto `host` and let it resolve, trigger and all.
fn enchant(
    state: &GameState,
    db: &CardDatabase,
    aura: CardInstance,
    host: PermanentId,
) -> GameState {
    let state = apply_action(
        state,
        &Action::CastSpell {
            card: aura,
            mode: None,
            x: None,
            targets: vec![Target::Permanent(host)],
            payment: Vec::new(),
        },
        db,
    );
    let state = apply_action(&state, &Action::PassPriority, db);
    let state = apply_action(&state, &Action::PassPriority, db);
    // The Aura is on the battlefield; its enters trigger is on the stack.
    let state = apply_action(&state, &Action::PassPriority, db);
    apply_action(&state, &Action::PassPriority, db)
}

/// It arrives, and the creature it landed on is tapped — no second choice asked.
#[test]
fn waterknot_taps_what_it_enchants_as_it_arrives() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let other = place(&mut state, &db, "onakke_ogre", PlayerId(1));
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));

    let state = enchant(&state, &db, aura, victim);

    assert!(tapped(&state, victim), "the enchanted creature is tapped");
    assert!(
        !tapped(&state, other),
        "and nothing else was, though it was just as much a creature"
    );
    // The Aura is attached to what it tapped, which is the same choice made once.
    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.attached_to == Some(victim)),
        "attached to its host"
    );
}

/// And it keeps it there: the untap step passes over the enchanted creature and untaps
/// everything else its controller has.
#[test]
fn waterknot_holds_its_host_down_through_the_untap_step() {
    let db = db();
    let mut state = main_phase(&db);
    // Seat 0 enchants its *own* creature, so the untap step under test is seat 0's.
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let free = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));
    let state = enchant(&state, &db, aura, victim);
    // Tap the other one too, so the step has something to prove it still works.
    let mut state = state;
    if let Some(perm) = state.battlefield.iter_mut().find(|perm| perm.id == free) {
        perm.tapped = true;
    }

    let state = past_my_next_untap(&state, &db);

    assert!(tapped(&state, victim), "still held down");
    assert!(!tapped(&state, free), "and everything else untapped");
}

/// The restriction is **continuous**, not a flag the first untap step spends: a second
/// untap step passes over the creature exactly as the first did.
#[test]
fn waterknot_is_not_spent_by_one_untap_step() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));
    let state = enchant(&state, &db, aura, victim);

    let state = past_my_next_untap(&state, &db);
    assert!(tapped(&state, victim), "held through the first");

    let state = past_my_next_untap(&state, &db);
    assert!(tapped(&state, victim), "and through the next one too");
}

/// It is the Aura that holds it: destroy the Aura and the creature untaps with nothing
/// to clear.
#[test]
fn a_creature_freed_from_the_aura_untaps_again() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));
    let mut state = enchant(&state, &db, aura, victim);

    // The Aura leaves — as a Disenchant would send it, and as the CR 704.5m state-based
    // action would if its host had died.
    state.battlefield.retain(|perm| perm.attached_to.is_none());
    assert!(tapped(&state, victim), "still tapped, just no longer held");

    let state = past_my_next_untap(&state, &db);

    assert!(
        !tapped(&state, victim),
        "the restriction left with the Aura, and nothing had to be cleared"
    );
}

/// **Whose ability it is** is observable, and this is the observation: a creature that
/// loses all its abilities is still held down, because the restriction was never one of
/// its abilities — it is the Aura's sentence about the creature (CR 303.4).
///
/// The difference is why the ability is authored on the Aura with an `attached_to` scope
/// rather than *granted* to the host, which would have been the shorter road.
#[test]
fn losing_all_abilities_does_not_free_the_enchanted_creature() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));
    let mut state = enchant(&state, &db, aura, victim);

    // The layer-6 effect a "loses all abilities" card produces, applied directly: the
    // catalog has no creature-side printing of it yet, and what is under test is the
    // interaction rather than any one card.
    let timestamp = state.mint_id();
    state.static_effects.push(sage_engine::StaticEffect {
        source: timestamp,
        affects: sage_engine::EffectAffects::SpecificPermanent(victim),
        modification: sage_engine::Modification::LoseAllAbilities,
        duration: sage_engine::Duration::WhileOnBattlefield,
    });

    let state = past_my_next_untap(&state, &db);

    assert!(
        tapped(&state, victim),
        "the Aura still holds it: the restriction was never the creature's ability"
    );
}

/// Nothing about the creature's characteristics changed — the restriction is a rule
/// modification, and the permanent is exactly the creature it was.
#[test]
fn the_restriction_changes_no_characteristic() {
    let db = db();
    let mut state = main_phase(&db);
    let victim = place(&mut state, &db, "onakke_ogre", PlayerId(0));
    let before = characteristics(&state, victim, &db);
    let aura = to_hand(&mut state, &db, "waterknot", PlayerId(0));

    let state = enchant(&state, &db, aura, victim);

    let after = characteristics(&state, victim, &db);
    assert_eq!(after.power, before.power);
    assert_eq!(after.toughness, before.toughness);
    assert_eq!(
        after.keywords, before.keywords,
        "and no keyword was granted"
    );
}
