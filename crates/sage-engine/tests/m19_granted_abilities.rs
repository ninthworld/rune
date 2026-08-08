//! An Aura on something other than a creature, and a grant that is a whole ability
//! rather than a keyword (CR 303.4, CR 613.1f, CR 605.1a).
//!
//! Three cards, one seam. Gift of Paradise enchants a **land** and gives it a mana
//! ability; Infernal Scarring gives a creature a dies trigger; Abnormal Endurance gives
//! one the same shape of trigger from a spell instead of an Aura. What they share is that
//! the granted ability has to arrive at the host as an ordinary ability — offered by
//! `valid_actions`, resolved off the stack when it is a mana ability, and collected by
//! the trigger diff — with nothing downstream able to tell it from a printed one.
//!
//! Every test drives the real [`apply_action`] pipeline over the bundled catalog. Cards
//! are named by their authored `functional_id`, never by an interned handle (ADR 0008 §3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sage_engine::{
    abilities_of_permanent, apply_action, characteristics, valid_actions, Action, CardDatabase,
    CardId, CardInstance, Color, FunctionalId, GameState, Permanent, PermanentId, PlayerId, Step,
    Target,
};

// ----- fixtures -------------------------------------------------------------

fn db() -> CardDatabase {
    CardDatabase::bundled().expect("bundled cards")
}

fn cid(db: &CardDatabase, slug: &str) -> CardId {
    let id = FunctionalId::try_from(slug.to_string()).expect("a well-formed identity");
    db.card_id(&id).expect("a bundled card")
}

/// A two-player game at player 0's precombat main, with pools stocked so payability never
/// decides a test that is about attachment or a grant.
fn main_phase(db: &CardDatabase) -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    let forest = cid(db, "forest");
    for seat in 0..2 {
        for color in [
            Color::White,
            Color::Blue,
            Color::Black,
            Color::Red,
            Color::Green,
        ] {
            state.players[seat].mana_pool.add(color, 10);
        }
        state.players[seat].mana_pool.add_colorless(10);
        state.players[seat].library = (0..20).map(|_| state.new_instance(forest)).collect();
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

/// Cast `card` at `targets` and let the stack empty — the spell, and anything its
/// resolution puts on the stack behind it.
fn cast(
    state: &GameState,
    db: &CardDatabase,
    card: CardInstance,
    targets: Vec<Target>,
) -> GameState {
    let mut state = apply_action(
        state,
        &Action::CastSpell {
            card,
            mode: None,
            x: None,
            targets,
            payment: Vec::new(),
        },
        db,
    );
    while !state.stack.is_empty() {
        let next = apply_action(&state, &Action::PassPriority, db);
        assert_ne!(next, state, "the stack is not draining");
        state = next;
    }
    state
}

fn on_battlefield(state: &GameState, id: PermanentId) -> bool {
    state.battlefield.iter().any(|perm| perm.id == id)
}

/// The permanent an Aura cast from `card` became, once it has resolved.
fn permanent_of(state: &GameState, card: CardInstance) -> PermanentId {
    state
        .battlefield
        .iter()
        .find(|perm| perm.instance == card.id)
        .expect("the card resolved onto the battlefield")
        .id
}

fn attached_to(state: &GameState, id: PermanentId) -> Option<PermanentId> {
    state
        .battlefield
        .iter()
        .find(|perm| perm.id == id)
        .and_then(|perm| perm.attached_to)
}

/// The cast of `card` as `valid_actions` advertises it — the shape with its target slots
/// still empty, which the acting player fills from [`sage_engine::target_requirements`].
fn unfilled_cast(card: CardInstance) -> Action {
    Action::CastSpell {
        card,
        mode: None,
        x: None,
        targets: Vec::new(),
        payment: Vec::new(),
    }
}

/// The hosts an Aura cast from `card` may legally choose — the one required slot its
/// enchant restriction declares (CR 303.4a / 601.2c).
fn host_candidates(state: &GameState, db: &CardDatabase, card: CardInstance) -> Vec<Target> {
    let slots = sage_engine::target_requirements(state, db, &unfilled_cast(card));
    assert_eq!(slots.len(), 1, "an Aura declares exactly one host slot");
    assert!(!slots[0].optional, "the host slot is required");
    slots[0].candidates.clone()
}

/// Every activation `valid_actions` offers for `permanent`, by ability index.
fn offered_indices(state: &GameState, db: &CardDatabase, permanent: PermanentId) -> Vec<usize> {
    valid_actions(state, db)
        .into_iter()
        .filter_map(|action| match action {
            Action::ActivateAbility {
                permanent: p,
                index,
                ..
            } if p == permanent => Some(index),
            _ => None,
        })
        .collect()
}

// ----- an Aura on a land ----------------------------------------------------

#[test]
fn issue_740_gift_of_paradise_enchants_a_land_and_gains_three_life() {
    // CR 303.4a: an Aura's enchant restriction is whatever class its card names, and a
    // land is one of them. The whole cast goes through the ordinary path — one required
    // target slot, chosen at announcement — and the entry trigger resolves behind it.
    let db = db();
    let mut state = main_phase(&db);
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let gift = to_hand(&mut state, &db, "gift_of_paradise", PlayerId(0));
    let life = state.players[0].life;

    assert!(
        valid_actions(&state, &db).contains(&unfilled_cast(gift)),
        "the Aura is castable"
    );
    assert_eq!(
        host_candidates(&state, &db, gift),
        vec![Target::Permanent(land)],
        "a land is a legal host for an enchant-land Aura"
    );

    let state = cast(&state, &db, gift, vec![Target::Permanent(land)]);
    let aura = permanent_of(&state, gift);
    assert_eq!(attached_to(&state, aura), Some(land));
    assert_eq!(
        state.players[0].life,
        life + 3,
        "the enters-the-battlefield trigger gained three life"
    );
}

#[test]
fn issue_740_the_enchanted_land_offers_the_granted_mana_ability_and_it_uses_no_stack() {
    // CR 613.1f and CR 605.1a together: the granted `{T}: Add two mana of any one color`
    // is folded into the land's ability set beside its printed one, so `valid_actions`
    // offers both — and because every effect of it adds mana it is a mana ability, which
    // never uses the stack whoever granted it.
    let db = db();
    let mut state = main_phase(&db);
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let gift = to_hand(&mut state, &db, "gift_of_paradise", PlayerId(0));
    assert_eq!(
        offered_indices(&state, &db, land),
        vec![0],
        "a bare Forest offers only its printed mana ability"
    );

    let mut state = cast(&state, &db, gift, vec![Target::Permanent(land)]);
    assert_eq!(
        offered_indices(&state, &db, land),
        vec![0, 1],
        "the granted ability is offered from the host, after the printed one"
    );
    let granted = abilities_of_permanent(
        &state,
        &db,
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == land)
            .expect("the land is on the battlefield"),
    );
    assert!(
        sage_engine::is_mana_ability(&granted[1]),
        "a granted mana ability is still a mana ability (CR 605.1a)"
    );

    state.players[0].mana_pool = Default::default();
    let state = apply_action(
        &state,
        &Action::ActivateAbility {
            permanent: land,
            index: 1,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        &db,
    );
    assert!(
        state.stack.is_empty(),
        "a granted mana ability uses no stack (CR 605.3)"
    );
    assert!(
        state
            .battlefield
            .iter()
            .any(|perm| perm.id == land && perm.tapped),
        "the {{T}} in the granted cost tapped the land"
    );

    // One colour question for the whole amount: `two mana of any one color` is a single
    // decision, and answering it pays out both points in that colour.
    let state = apply_action(&state, &Action::AnswerColor { color: Color::Blue }, &db);
    assert_eq!(state.players[0].mana_pool.color_amount(Color::Blue), 2);
    assert_eq!(state.players[0].mana_pool.color_amount(Color::Green), 0);
}

#[test]
fn issue_740_the_aura_follows_its_land_to_the_graveyard() {
    // CR 704.5m over a host that is not a creature: the state-based action judges the
    // host by the Aura's own enchant restriction, so a destroyed land orphans the Aura
    // exactly as a dead creature does — and the granted mana ability goes with it,
    // because it was never stored anywhere to prune.
    let db = db();
    let mut state = main_phase(&db);
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let gift = to_hand(&mut state, &db, "gift_of_paradise", PlayerId(0));
    let state = cast(&state, &db, gift, vec![Target::Permanent(land)]);
    let aura = permanent_of(&state, gift);

    let mut state = state;
    let rift = to_hand(&mut state, &db, "tectonic_rift", PlayerId(0));
    let state = cast(&state, &db, rift, vec![Target::Permanent(land)]);

    assert!(!on_battlefield(&state, land), "the land was destroyed");
    assert!(
        !on_battlefield(&state, aura),
        "an Aura with no legal host is put into its owner's graveyard (CR 704.5m)"
    );
    assert!(
        state.players[0]
            .graveyard
            .iter()
            .any(|card| card.id == gift.id),
        "the Aura is in its owner's graveyard"
    );
}

#[test]
fn issue_740_an_enchant_land_aura_cannot_be_cast_at_a_creature() {
    // The restriction is a rule, not merely an unoffered button. A creature is absent
    // from the offers *and* refused at apply, which is what stops a stale or forged
    // action id from putting an Aura somewhere its enchant ability never allowed.
    let db = db();
    let mut state = main_phase(&db);
    let land = place(&mut state, &db, "forest", PlayerId(0));
    let creature = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let gift = to_hand(&mut state, &db, "gift_of_paradise", PlayerId(0));

    let candidates = host_candidates(&state, &db, gift);
    assert!(
        !candidates.contains(&Target::Permanent(creature)),
        "a creature is never offered as the host of an enchant-land Aura"
    );
    assert_eq!(candidates, vec![Target::Permanent(land)]);

    let at_creature = Action::CastSpell {
        card: gift,
        mode: None,
        x: None,
        targets: vec![Target::Permanent(creature)],
        payment: Vec::new(),
    };
    let after = apply_action(&state, &at_creature, &db);
    assert_eq!(after, state, "a forged host changes nothing");

    // And an Aura with no legal host at all is not castable: the enchant slot is
    // required, so a board with only creatures on it withholds the whole cast.
    let mut lonely = main_phase(&db);
    place(&mut lonely, &db, "centaur_courser", PlayerId(0));
    let orphan = to_hand(&mut lonely, &db, "gift_of_paradise", PlayerId(0));
    assert!(!valid_actions(&lonely, &db).contains(&unfilled_cast(orphan)));
}

// ----- a granted triggered ability ------------------------------------------

#[test]
fn issue_740_infernal_scarring_gives_its_host_a_dies_trigger() {
    // CR 613.1f / 603.6c: the Aura grants +2/+0 and a whole triggered ability. The
    // trigger is read off the snapshot the creature still existed in, so it fires on the
    // way out even though the Aura is on its way to the graveyard in the same
    // state-based-action pass.
    let db = db();
    let mut state = main_phase(&db);
    let bear = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let scarring = to_hand(&mut state, &db, "infernal_scarring", PlayerId(0));
    let state = cast(&state, &db, scarring, vec![Target::Permanent(bear)]);

    assert_eq!(
        characteristics(&state, bear, &db).power,
        Some(5),
        "a 3/3 under a +2/+0 Aura is a 5/3"
    );
    let abilities = abilities_of_permanent(
        &state,
        &db,
        state
            .battlefield
            .iter()
            .find(|perm| perm.id == bear)
            .expect("the host is on the battlefield"),
    );
    assert_eq!(
        abilities.len(),
        1,
        "a vanilla creature under this Aura has exactly the granted ability"
    );

    let mut state = state;
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));
    let hand = state.players[0].hand.len();
    let state = cast(&state, &db, murder, vec![Target::Permanent(bear)]);

    assert!(!on_battlefield(&state, bear), "the host died");
    assert_eq!(
        state.players[0].hand.len(),
        hand - 1 + 1,
        "Murder left the hand and the granted dies trigger drew a card"
    );
}

#[test]
fn issue_740_abnormal_endurance_grants_a_dies_trigger_that_brings_its_host_back() {
    // The same grant from a spell rather than an Aura, and the one granted ability that
    // outlives the grant: the trigger fires on the way out (CR 603.6c), and what it does
    // afterwards reaches the card in the graveyard — which is why a dies trigger's source
    // records both the permanent that was and the card it became (CR 603.10a).
    let db = db();
    let mut state = main_phase(&db);
    let bear = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let endurance = to_hand(&mut state, &db, "abnormal_endurance", PlayerId(0));
    let state = cast(&state, &db, endurance, vec![Target::Permanent(bear)]);
    assert_eq!(characteristics(&state, bear, &db).power, Some(5));

    let mut state = state;
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));
    let state = cast(&state, &db, murder, vec![Target::Permanent(bear)]);

    assert!(
        !on_battlefield(&state, bear),
        "the permanent that died is gone — a return is a new object"
    );
    let returned = state
        .battlefield
        .iter()
        .find(|perm| perm.printed.card() == Some(cid(&db, "centaur_courser")))
        .expect("the granted trigger returned the creature");
    assert!(returned.tapped, "it comes back tapped");
    assert_eq!(
        characteristics(&state, returned.id, &db).power,
        Some(3),
        "the new object is a plain 3/3: neither the pump nor the grant followed it"
    );
    assert!(
        state.players[0].graveyard.is_empty()
            || !state.players[0]
                .graveyard
                .iter()
                .any(|card| card.id == returned.instance),
        "the card left the graveyard rather than being copied out of it"
    );
}

#[test]
fn issue_740_a_granted_dies_trigger_needs_the_grant_to_have_been_there() {
    // The control: the same creature, killed the same way, with nothing granting it
    // anything. Nothing is drawn and nothing comes back, so the two tests above are
    // measuring the grant rather than the death.
    let db = db();
    let mut state = main_phase(&db);
    let bear = place(&mut state, &db, "centaur_courser", PlayerId(0));
    let murder = to_hand(&mut state, &db, "murder", PlayerId(0));
    let hand = state.players[0].hand.len();
    let state = cast(&state, &db, murder, vec![Target::Permanent(bear)]);

    assert!(!on_battlefield(&state, bear));
    assert_eq!(
        state.players[0].hand.len(),
        hand - 1,
        "no card was drawn without Infernal Scarring"
    );
    assert!(
        state.battlefield.is_empty(),
        "nothing came back without Abnormal Endurance"
    );
}

// ----- the grant is ordered against a removal (CR 613.1f) -------------------

/// A creature that can silence itself, and an Aura that hands one an activated ability.
/// Nothing in M19 both loses all abilities and is enchantable in the same test, so the
/// pair is authored inline (ADR 0009).
const SILENCE: &str = r#"[
    {"schema_version":1,"functional_id":"test_mute","name":"Test Mute",
     "types":["creature"],"subtypes":["Golem"],"mana_cost":"{2}","colors":[],
     "power":2,"toughness":2,
     "abilities":[
       {"type":"activated","cost":[{"kind":"mana","mana":"{1}"}],
        "effects":[{"kind":"alter_abilities_self","lose_all":true}]}]},
    {"schema_version":1,"functional_id":"test_gift","name":"Test Gift",
     "types":["enchantment"],"subtypes":["Aura"],"mana_cost":"{1}{G}","colors":["green"],
     "attachment":{"kind":"aura","attach_to":"any_creature",
       "abilities":[{"type":"activated","cost":[{"kind":"tap"}],
                     "effects":[{"kind":"gain_life","player_ref":"controller","amount":1}]}]}}
]"#;

/// The inline board: player 0's main phase with mana to spend and nothing else going on.
fn silence_board() -> GameState {
    let mut state = GameState::new_two_player();
    state.step = Step::PrecombatMain;
    state.priority = PlayerId(0);
    state.consecutive_passes = 0;
    for seat in 0..2 {
        state.players[seat].mana_pool.add_colorless(10);
    }
    state
}

/// Hang an Aura placed from `slug` on `host`, minting it *now* so its CR 613.7 timestamp
/// is later than anything already in force.
fn hang(state: &mut GameState, db: &CardDatabase, slug: &str, host: PermanentId) {
    let aura = place(state, db, slug, PlayerId(0));
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == aura)
        .expect("the Aura is on the battlefield")
        .attached_to = Some(host);
}

/// Activate `index` on `permanent` and let the stack empty.
fn activate(
    state: &GameState,
    db: &CardDatabase,
    permanent: PermanentId,
    index: usize,
) -> GameState {
    let mut state = apply_action(
        state,
        &Action::ActivateAbility {
            permanent,
            index,
            targets: Vec::new(),
            payment: Vec::new(),
        },
        db,
    );
    while !state.stack.is_empty() {
        state = apply_action(&state, &Action::PassPriority, db);
    }
    state
}

#[test]
fn issue_740_an_aura_hung_after_a_silencing_still_grants_its_ability() {
    // CR 613.1f: layer 6 is ordered by timestamp, and a grant that speaks last grants.
    // Before an attachment could hand over a written-out ability, "has this lost
    // everything?" was a boolean and that was exact; it no longer is, and this is the
    // case that says so.
    let db = CardDatabase::from_json(SILENCE).expect("an inline definition");
    let mut state = silence_board();
    let mute = place(&mut state, &db, "test_mute", PlayerId(0));

    let mut after = activate(&state, &db, mute, 0);
    assert!(
        characteristics(&after, mute, &db).abilities.is_empty(),
        "the creature silenced itself"
    );

    hang(&mut after, &db, "test_gift", mute);
    assert_eq!(
        characteristics(&after, mute, &db).abilities.len(),
        1,
        "an Aura hung afterwards grants what it grants"
    );
    assert_eq!(
        offered_indices(&after, &db, mute),
        vec![0],
        "and the granted activation is offered from the host"
    );
}

#[test]
fn issue_740_a_silencing_after_the_aura_takes_the_granted_ability_with_it() {
    // The other order, and the reason the fold is a fold: a loses-all clears everything
    // that applied before its timestamp, granted or printed alike.
    let db = CardDatabase::from_json(SILENCE).expect("an inline definition");
    let mut state = silence_board();
    let mute = place(&mut state, &db, "test_mute", PlayerId(0));
    hang(&mut state, &db, "test_gift", mute);
    assert_eq!(characteristics(&state, mute, &db).abilities.len(), 2);

    let silenced = activate(&state, &db, mute, 0);
    assert!(
        characteristics(&silenced, mute, &db).abilities.is_empty(),
        "the grant was already in force, so the silencing took it too"
    );
}
