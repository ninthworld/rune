//! Colour identity (CR 903.4) and the legendary-creature check a commander is judged by
//! — read from structured card data, never from generated display text.

use super::*;

/// A card's **color identity** (CR 903.4), computed from **structured** card data
/// only — never from generated display text (issue #372). Three contributors, all
/// read through the engine's typed [`CardData`]:
///
/// 1. the card's color indicator / printed colors (CR 105.2), `CardData::colors`;
/// 2. the colored mana symbols in its mana cost (`CardData::mana_cost` pips); and
/// 3. the colored mana symbols in its **rules**, taken from the ability IR: every
///    [`Effect::AddMana`] is a `{color}` mana symbol printed in a rules ability
///    (e.g. a land's `{T}: Add {G}`), which is exactly what gives a basic Forest
///    its green identity.
///
/// Colorless mana ([`Effect::AddColorlessMana`]) contributes nothing — colorless is
/// not a color (CR 105.1) — so an artifact that taps for `{C}` stays identity-empty
/// and is legal under any commander.
///
/// Visible to the crate (issue #553) because the *same* computation now also feeds
/// the in-match `CommanderIdentity` a client renders a seat's identity gems from,
/// and (issue #700) the `color_identity` every `CardView` carries so a board is
/// scannable by colour — deck legality, the displayed identity, and the colour a
/// card is drawn in must never be able to disagree.
pub(crate) fn color_identity(db: &CardDatabase, card: CardId) -> HashSet<Color> {
    db.card(card)
        .map(|data| color_identity_of(db, data))
        .unwrap_or_default()
}

/// [`color_identity`], for a caller that already holds the [`CardData`].
///
/// The card projection has the data and not the handle, and the handle is only
/// needed to reach the *code* tier of the abilities (ADR 0008 §5) — so it is looked
/// back up from the authored identity rather than duplicating the computation. One
/// function, three contributors, whichever end a caller comes in from.
pub(crate) fn color_identity_of(db: &CardDatabase, data: &CardData) -> HashSet<Color> {
    let mut identity = HashSet::new();
    // 1. Color indicator / printed colors.
    identity.extend(data.colors.iter().copied());
    // 2. Colored mana-cost pips.
    let cost = parse_mana_cost(&data.mana_cost);
    for (count, color) in [
        (cost.white, Color::White),
        (cost.blue, Color::Blue),
        (cost.black, Color::Black),
        (cost.red, Color::Red),
        (cost.green, Color::Green),
    ] {
        if count > 0 {
            identity.insert(color);
        }
    }
    // 3. Colored mana symbols in the card's rules (its abilities), from the IR. A
    //    *restricted* mana symbol (CR 106.6) is still a coloured mana symbol printed in
    //    the rules, so it counts exactly as an unrestricted one does.
    let abilities = db
        .card_id(&data.functional_id)
        .map(|card| abilities_of(db, card))
        .unwrap_or_default();
    for ability in abilities {
        if let Ability::Activated { effects, .. } | Ability::Triggered { effects, .. } = ability {
            for effect in effects {
                match effect {
                    Effect::AddMana { color, .. } | Effect::AddRestrictedMana { color, .. } => {
                        identity.insert(color);
                    }
                    _ => {}
                }
            }
        }
    }
    // A spell ability that itself mints colored mana counts too (CR 903.4).
    for effect in &data.spell_effects {
        if let Effect::AddMana { color, .. } | Effect::AddRestrictedMana { color, .. } = effect {
            identity.insert(*color);
        }
    }
    identity
}

/// Whether `card` is a **legendary creature** — carries both the structured
/// [`Supertype::Legendary`] and the [`CardType::Creature`] type — read through
/// `db` (CR 903.5a, the default commander eligibility). An unknown id is not one;
/// the lobby rejects unknown ids before legality is checked.
pub(super) fn is_legendary_creature(db: &CardDatabase, card: CardId) -> bool {
    db.card(card).is_some_and(|data| {
        data.supertypes.contains(&Supertype::Legendary) && data.has_type(CardType::Creature)
    })
}
