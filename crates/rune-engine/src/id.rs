//! Lightweight identity newtypes.
//!
//! These are placeholders until the card database (issue #9). They exist so the
//! rest of the engine can talk about cards, players, and permanents by stable id
//! instead of by object reference.

/// Identifies a card definition (a specific printing/oracle entry).
///
/// A card keeps the same `CardId` in every zone; it is not the battlefield
/// identity (see [`PermanentId`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CardId(pub u64);

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
