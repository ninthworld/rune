//! Lightweight identity newtypes.
//!
//! These are placeholders until the card database (issue #9). They exist so the
//! rest of the engine can talk about cards, players, and permanents by stable id
//! instead of by object reference.

/// Identifies a card definition (a specific printing/oracle entry).
///
/// A card keeps the same `CardId` in every zone; it is not the battlefield
/// identity (see [`PermanentId`]) nor the per-copy identity (see
/// [`CardInstanceId`]). Two copies of the same printing share one `CardId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardId(pub u64);

/// The engine's printing-independent card identity (ADR 0013).
///
/// An oracle card owns exactly one of these; every printing references it. The
/// integer [`CardId`] *is* the oracle id — it keys the oracle
/// [`crate::CardDatabase`] and every rules read — so `OracleId` is a documentary
/// alias, not a distinct type. Printings carry no rules; they resolve to their
/// characteristics through this id.
pub type OracleId = CardId;

/// Identifies one physical card in a game, distinct from every other copy.
///
/// Minted fresh from [`crate::GameState::mint_id`] when a card first enters a
/// zone, so two copies of the same [`CardId`] (two Forests in hand) are
/// individually addressable in library, hand, graveyard, exile, and on the
/// stack. Unlike [`PermanentId`], which is reborn on each battlefield entry, an
/// instance id stays with the physical card as it moves between those zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardInstanceId(pub u64);

/// One physical card occupying a zone: its per-game [`CardInstanceId`] paired
/// with the [`CardId`] of the printing it is a copy of.
///
/// This pairing is the mapping from instance identity to printed card that lets
/// duplicate copies stay distinguishable within a zone (`Vec<CardInstance>`),
/// rather than collapsing to a bare `CardId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardInstance {
    /// This copy's unique per-game identity.
    pub id: CardInstanceId,
    /// The printing this copy represents.
    pub card: CardId,
}

/// Identifies a seat at the table by index into [`crate::GameState::players`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PlayerId(pub usize);

/// Fresh identity assigned to a permanent each time it enters the battlefield.
///
/// Zone-change identity is the mechanism: the "second time on the battlefield"
/// has a different `PermanentId` than the first, so there are no zone-change
/// counters (see `crates/rune-engine/AGENTS.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PermanentId(pub u64);
