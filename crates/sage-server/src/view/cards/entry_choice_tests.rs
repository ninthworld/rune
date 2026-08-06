//! The answers a permanent's controller gave **as it entered** (CR 614.12), on the wire.
//!
//! A cohesive slice of the permanent projection rather than part of the general card
//! tests: both fields are a decision a player made rather than anything about the card,
//! both are public, and the rule they share is the one worth a test of its own — a client
//! renders what it is sent and derives neither.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::test_support::fixture;
use crate::view::test_support::put_permanent;

/// The permanent `id` as both seats and a spectator see it, so a test asserting a public
/// fact asserts it is *public* rather than merely present in one view.
fn everywhere(
    state: &GameState,
    db: &CardDatabase,
    id: PermanentId,
) -> Vec<sage_protocol::Permanent> {
    let wanted = permanent_entity_id(id);
    let mut seen: Vec<sage_protocol::Permanent> = (0..state.players.len())
        .map(|seat| personalized_view(state, db, PlayerId(seat)))
        .map(|view| {
            view.battlefield
                .into_iter()
                .find(|perm| perm.id == wanted)
                .expect("the permanent is on the battlefield")
        })
        .collect();
    seen.push(
        crate::view::spectator_view(state, db)
            .battlefield
            .into_iter()
            .find(|perm| perm.id == wanted)
            .expect("a spectator sees the battlefield too"),
    );
    seen
}

/// The **card** a permanent named as it entered (CR 614.12, issue #738) reaches every
/// seat, as the catalog's own name for that card.
///
/// The engine records a functional identity and the wire carries a name, which is the one
/// translation the projection makes: a client never learns what a `CardId` is, and the
/// only names that can ever appear here are names the catalog already holds.
#[test]
fn issue_738_the_named_card_is_projected_to_every_seat_as_a_catalog_name() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let moon = put_permanent(
        &mut state,
        fixture("alpine_moon"),
        PlayerId(0),
        false,
        false,
    );
    let plain = put_permanent(
        &mut state,
        fixture("onakke_ogre"),
        PlayerId(0),
        false,
        false,
    );
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == moon)
        .unwrap()
        .named_card = Some(fixture("highland_lake"));

    for view in everywhere(&state, &db, moon) {
        assert_eq!(
            view.named_card.as_deref(),
            Some("Highland Lake"),
            "every seat and every spectator is told what was named"
        );
    }
    for view in everywhere(&state, &db, plain) {
        assert_eq!(
            view.named_card, None,
            "a permanent that named nothing carries nothing"
        );
    }
}

/// The **colour** half of the same seam, asserted the same way: public, stated, and never
/// the permanent's own colour.
#[test]
fn issue_738_the_chosen_colour_is_projected_to_every_seat() {
    let db = CardDatabase::bundled().unwrap();
    let mut state = GameState::new_two_player();
    let mare = put_permanent(
        &mut state,
        fixture("diamond_mare"),
        PlayerId(1),
        false,
        false,
    );
    state
        .battlefield
        .iter_mut()
        .find(|perm| perm.id == mare)
        .unwrap()
        .chosen_color = Some(sage_engine::Color::Red);

    for view in everywhere(&state, &db, mare) {
        assert_eq!(view.chosen_color, Some(sage_protocol::Color::Red));
        assert!(
            view.card.color_identity.is_empty(),
            "a colourless artifact that named red is still colourless"
        );
    }
}
