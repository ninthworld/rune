//! Card, permanent, and zone projection into wire views.

use super::*;
use crate::format::color_identity_of;

/// Whether this battlefield object **is** somebody's commander (CR 903.3, issue
/// #553) — the marker `Permanent::is_commander` carries.
///
/// Matched on the card **instance**, which is the engine's designation key
/// ([`CommanderState::instance`](sage_engine::CommanderState)): a
/// [`PermanentId`] is minted fresh on every battlefield entry, so a recast
/// commander is a new object but the same instance. Every seat's designation is
/// checked rather than only the controller's, because a commander that has changed
/// control is still its owner's commander.
///
/// This is a *lookup*, not a derivation: nothing about a name, a zone, or a type
/// line participates, which is exactly why a client cannot compute it.
pub(crate) fn is_commander_permanent(state: &GameState, perm: &sage_engine::Permanent) -> bool {
    state.players.iter().any(|player| {
        player
            .commander
            .is_some_and(|c| c.instance == perm.instance)
    })
}

/// The keywords a permanent has now that its **printed card does not** (CR 613 layer 6,
/// CR 613.1f), as the words a card prints them with.
///
/// [`permanent_card_view`] already projects the *current* keyword set, which is what
/// combat and evasion are judged on. What that set cannot say is which of those words are
/// new — and "new" is exactly what a player needs to see after casting a pump: the
/// creature has trample until end of turn, and the card in front of them says nothing.
/// The difference is computed here, where both halves are in hand, rather than left to a
/// client to work out by matching generated prose against keyword names.
///
/// A token's printed face is the token itself, so an effect that granted it a keyword on
/// top of the ones it was created with reads the same way. Order follows the engine's
/// current keyword set, so two projections of one board agree.
pub(crate) fn granted_keywords(
    state: &GameState,
    perm: &sage_engine::Permanent,
    db: &CardDatabase,
) -> Vec<String> {
    let printed: Vec<Keyword> = match perm.printed.face(db) {
        Some(PrintedFace::Card(data)) => data.keywords.clone(),
        Some(PrintedFace::Token(token)) => token.keywords.clone(),
        None => Vec::new(),
    };
    characteristics(state, perm.id, db)
        .keywords
        .iter()
        .filter(|keyword| !printed.contains(keyword))
        .map(|&keyword| crate::rules_text::keyword_phrase(keyword))
        .collect()
}

/// Projects a permanent's stored engine counters into the wire [`Counter`] list.
///
/// Ordering follows the permanent's `BTreeMap<CounterKind, _>` iteration, which
/// is sorted by [`CounterKind`] and therefore stable across runs. Absent kinds
/// are simply not emitted, so a permanent with no counters yields an empty
/// `Vec` (the `skip_serializing_if` wire shape stays unchanged).
pub(crate) fn permanent_counters(perm: &sage_engine::Permanent) -> Vec<Counter> {
    perm.counters
        .iter()
        .map(|(&kind, &count)| Counter {
            kind: counter_kind_str(kind).to_owned(),
            count,
        })
        .collect()
}

/// Map the engine's turn [`Step`] onto the protocol [`Phase`]. The two enums are
/// deliberately decoupled (`sage-engine` never depends on `sage-protocol`), so the
/// mapping is written out here.
pub(crate) fn phase_of(step: Step) -> Phase {
    match step {
        Step::Untap => Phase::Untap,
        Step::Upkeep => Phase::Upkeep,
        Step::Draw => Phase::Draw,
        Step::PrecombatMain => Phase::PrecombatMain,
        Step::BeginCombat => Phase::BeginCombat,
        Step::DeclareAttackers => Phase::DeclareAttackers,
        Step::DeclareBlockers => Phase::DeclareBlockers,
        Step::CombatDamage => Phase::CombatDamage,
        Step::EndCombat => Phase::EndCombat,
        Step::PostcombatMain => Phase::PostcombatMain,
        Step::End => Phase::End,
        Step::Cleanup => Phase::Cleanup,
    }
}

/// The display name of a card, or a stable placeholder if the id is unknown.
pub(crate) fn card_name(card: CardId, db: &CardDatabase) -> String {
    db.card(card)
        .map(|data| data.name.clone())
        .unwrap_or_else(|| format!("Unknown card {}", card.0))
}

/// The display name of a battlefield permanent — the card's name, or the token's
/// (CR 111.3: a token's name is whatever the effect that created it gave it).
///
/// Every prompt, label, and stack sentence that names a permanent goes through here
/// rather than through [`card_name`], so a token is named by what it is instead of
/// being reported as an unknown card.
pub(crate) fn permanent_name(perm: &sage_engine::Permanent, db: &CardDatabase) -> String {
    perm.printed.face(db).map_or_else(
        || match perm.printed.card() {
            Some(card) => format!("Unknown card {}", card.0),
            None => "Token".to_string(),
        },
        |face| face.name().to_string(),
    )
}

/// Build the full [`CardView`] for a card the viewer is entitled to see.
pub(crate) fn card_view(entity_id: String, card: CardId, db: &CardDatabase) -> CardView {
    match db.card(card) {
        Some(data) => full_card_view(entity_id, data, db),
        None => unknown_card_view(entity_id, Some(card)),
    }
}

/// The defensive placeholder view for an object the server cannot resolve: a card
/// handle absent from the database, or — with no handle at all — a token whose
/// characteristics somehow did not come through. Carries no identity and no rules, so
/// a client renders something legible rather than nothing.
fn unknown_card_view(entity_id: String, card: Option<CardId>) -> CardView {
    CardView {
        id: entity_id,
        name: match card {
            Some(card) => format!("Unknown card {}", card.0),
            None => "Token".to_string(),
        },
        type_line: String::new(),
        mana_cost: None,
        rules_text: String::new(),
        functional_id: String::new(),
        token: card.is_none(),
        power: None,
        toughness: None,
        loyalty: None,
        keywords: Vec::new(),
        // Nothing is known about this object, so nothing is claimed about its types.
        // Empty is "not stated", which is the honest answer for a defensive placeholder.
        card_types: Vec::new(),
        color_identity: Vec::new(),
    }
}

/// The wire [`CardType`] for an engine [`sage_engine::CardType`].
///
/// Two closed sets that mean the same thing, mapped exhaustively rather than shared:
/// `sage-protocol` depends on serde and nothing else, which is what lets a client
/// build against the contract without building the engine. The cost is this function,
/// and the benefit is that adding an engine type will not compile until someone has
/// decided what it is called on the wire.
fn card_type(card_type: sage_engine::CardType) -> CardType {
    match card_type {
        sage_engine::CardType::Land => CardType::Land,
        sage_engine::CardType::Creature => CardType::Creature,
        sage_engine::CardType::Artifact => CardType::Artifact,
        sage_engine::CardType::Enchantment => CardType::Enchantment,
        sage_engine::CardType::Instant => CardType::Instant,
        sage_engine::CardType::Sorcery => CardType::Sorcery,
        sage_engine::CardType::Planeswalker => CardType::Planeswalker,
        sage_engine::CardType::Battle => CardType::Battle,
    }
}

/// A set of engine [`Color`](sage_engine::Color)s as the wire's colour letters, in
/// canonical WUBRG order (CR 105.1).
///
/// Written once and shared by everything that publishes colours — a card's colour
/// identity and a commander's — so two projections of the same identity are
/// byte-identical and no client ever has to sort.
pub(crate) fn colors_in_wubrg(
    colors: &std::collections::HashSet<sage_engine::Color>,
) -> Vec<sage_protocol::Color> {
    [
        (sage_engine::Color::White, sage_protocol::Color::White),
        (sage_engine::Color::Blue, sage_protocol::Color::Blue),
        (sage_engine::Color::Black, sage_protocol::Color::Black),
        (sage_engine::Color::Red, sage_protocol::Color::Red),
        (sage_engine::Color::Green, sage_protocol::Color::Green),
    ]
    .into_iter()
    .filter(|(engine, _)| colors.contains(engine))
    .map(|(_, wire)| wire)
    .collect()
}

/// One engine [`Color`](sage_engine::Color) as the wire's colour letter.
///
/// The single-value counterpart of [`colors_in_wubrg`], for the one colour a permanent
/// named as it entered (CR 614.12) rather than a set a card belongs to. Exhaustive, so a
/// sixth colour would have to be answered here rather than silently dropped.
pub(crate) fn wire_color(color: sage_engine::Color) -> sage_protocol::Color {
    match color {
        sage_engine::Color::White => sage_protocol::Color::White,
        sage_engine::Color::Blue => sage_protocol::Color::Blue,
        sage_engine::Color::Black => sage_protocol::Color::Black,
        sage_engine::Color::Red => sage_protocol::Color::Red,
        sage_engine::Color::Green => sage_protocol::Color::Green,
    }
}

/// The wire name for an engine [`Keyword`], as the client expects it in
/// [`CardView::keywords`] (e.g. `"flying"`, `"first_strike"`). Kept exhaustive so
/// a new engine keyword forces a matching wire string here rather than silently
/// going unnamed.
fn keyword_str(keyword: Keyword) -> &'static str {
    match keyword {
        Keyword::Flying => "flying",
        Keyword::Reach => "reach",
        Keyword::Vigilance => "vigilance",
        Keyword::Haste => "haste",
        Keyword::Defender => "defender",
        Keyword::Menace => "menace",
        Keyword::FirstStrike => "first_strike",
        Keyword::Trample => "trample",
        Keyword::Deathtouch => "deathtouch",
        Keyword::Lifelink => "lifelink",
        Keyword::DoubleStrike => "double_strike",
        Keyword::Hexproof => "hexproof",
        Keyword::Indestructible => "indestructible",
    }
}

/// Project engine [`CardData`] onto the wire [`CardView`]. Power/toughness become
/// strings so non-numeric values round-trip (`sage-protocol`); an empty mana cost
/// is elided rather than sent as `""`; printed keywords project to their lowercase
/// wire names for display.
///
/// The card's rules text is **generated** here from its ability IR
/// ([`crate::rules_text`], ADR 0008 §7) rather than read from a stored string — the
/// catalog holds no prose — and its authored `functional_id` rides along as the stable
/// presentation identity (ADR 0008 §8). A scripted card's hand-authored text comes from
/// the engine's escape hatch — keyed, like the catalog itself, on the card's authored
/// `functional_id` rather than its build-interned handle (ADR 0008 §3), and guaranteed
/// by the loader to exist whenever the definition declares `scripted: true`.
pub(crate) fn full_card_view(entity_id: String, data: &CardData, db: &CardDatabase) -> CardView {
    CardView {
        id: entity_id,
        name: data.name.clone(),
        type_line: data.type_line(),
        mana_cost: (!data.mana_cost.is_empty()).then(|| data.mana_cost.clone()),
        rules_text: rules_text(data, scripted_rules_text(&data.functional_id)),
        functional_id: data.functional_id.to_string(),
        token: false,
        power: data.power.map(|p| p.to_string()),
        toughness: data.toughness.map(|t| t.to_string()),
        // CR 306.5b: the printed starting loyalty of a planeswalker, `None` for
        // everything else. What a planeswalker on the battlefield has *now* is its
        // `loyalty` counter, projected by `permanent_counters`.
        loyalty: data.loyalty.map(|l| l.to_string()),
        keywords: data
            .keywords
            .iter()
            .map(|&kw| keyword_str(kw).to_owned())
            .collect(),
        // The same `types` `type_line()` above is rendered from, so the sentence and
        // the set are one projection and cannot drift apart.
        card_types: data.types.iter().map(|&t| card_type(t)).collect(),
        // CR 903.4, through the *same* computation deck legality and a seat's
        // identity gems use (`format::color_identity_of`), so the colour a card is
        // drawn in cannot disagree with the colours it is legal under.
        color_identity: colors_in_wubrg(&color_identity_of(db, data)),
    }
}

/// Project a **permanent's printed face** onto the wire, whether it is a card or a
/// token (CR 111).
///
/// A card defers to [`full_card_view`], so nothing about an ordinary permanent's
/// projection changes. A token differs in exactly the two ways it differs in the
/// engine: it carries **no `functional_id`** — there is no card identity behind it, so
/// the field a client would cache or look art up by is empty, and `token` says why
/// rather than leaving the client to infer it from an absence — and its rules text is
/// generated from the abilities the creating effect gave it, through the same
/// formatter a card's text comes from.
fn face_card_view(entity_id: String, face: PrintedFace<'_>, db: &CardDatabase) -> CardView {
    match face {
        PrintedFace::Card(data) => full_card_view(entity_id, data, db),
        PrintedFace::Token(token) => CardView {
            id: entity_id,
            name: token.name.clone(),
            type_line: face.type_line(),
            // CR 111.3: a token has no mana cost, so the field is elided entirely.
            mana_cost: None,
            rules_text: token_rules_text(token),
            // A token is not a card and has no authored identity (ADR 0008 §3).
            functional_id: String::new(),
            token: true,
            power: token.power.map(|p| p.to_string()),
            toughness: token.toughness.map(|t| t.to_string()),
            // The effect IR creates no planeswalker token, so a token never has one.
            loyalty: None,
            keywords: token
                .keywords
                .iter()
                .map(|&kw| keyword_str(kw).to_owned())
                .collect(),
            // A token has types like anything else; having no card behind it (CR 111)
            // says nothing about what it is on the battlefield.
            card_types: token.types.iter().map(|&t| card_type(t)).collect(),
            // CR 111.3: a token's colours are whatever the creating effect gave it,
            // and it has no cost and no card behind it to read anything else from.
            color_identity: colors_in_wubrg(&token.colors.iter().copied().collect()),
        },
    }
}

/// Build the [`CardView`] for a battlefield permanent, projecting its **current**
/// power/toughness (CR 613 layer 7c) and keywords (CR 613.1f, layer 6) from the
/// engine's computed [`characteristics`] rather than the printed card. This is what
/// makes counters, until-end-of-turn pumps, and an attachment's P/T grant
/// (CR 303.4 / 301.5) visible on the wire — a Boar enchanted with a `+2/+2` Aura, or
/// equipped with a `+2/+1` Axe, projects as a 5/4 — and, equally, what makes a granted
/// keyword show up like a printed one: a creature enchanted with an Aura granting flying
/// projects with `flying`. Every
/// other field is the printed projection ([`card_view`]); a non-creature keeps its
/// absent P/T.
pub(crate) fn permanent_card_view(
    state: &GameState,
    perm: &sage_engine::Permanent,
    db: &CardDatabase,
) -> CardView {
    let mut view = match perm.printed.face(db) {
        Some(face) => face_card_view(permanent_entity_id(perm.id), face, db),
        None => unknown_card_view(permanent_entity_id(perm.id), perm.printed.card()),
    };
    let current = characteristics(state, perm.id, db);
    view.power = current.power.map(|p| p.to_string());
    view.toughness = current.toughness.map(|t| t.to_string());
    // CR 613 layer 6 (CR 613.1f): project the *current* keywords, so a keyword
    // granted by an Aura, an anthem, or an until-end-of-turn pump appears on the wire
    // exactly like a printed one.
    view.keywords = current
        .keywords
        .iter()
        .map(|&kw| keyword_str(kw).to_owned())
        .collect();
    view
}

/// Build the [`ZonePile`]s for a public per-player pile (graveyard or exile),
/// skipping empty piles so the wire stays terse.
pub(crate) fn zone_piles(
    state: &GameState,
    pick: impl Fn(&Player) -> &Vec<CardInstance>,
    db: &CardDatabase,
) -> Vec<ZonePile> {
    state
        .players
        .iter()
        .enumerate()
        .filter_map(|(seat, player)| {
            let cards = pick(player);
            if cards.is_empty() {
                return None;
            }
            Some(ZonePile {
                player_id: player_id(PlayerId(seat)),
                cards: cards
                    .iter()
                    .map(|&inst| card_view(card_entity_id(inst.id), inst.card, db))
                    .collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
