//! Test-only: naming cards by authored identity instead of interned handle.
//!
//! A [`CardId`] is interned by `build.rs` from the catalog's sort order (ADR 0018 §3),
//! so it is not a stable *name* for a card — adding one card renumbers its neighbours.
//! A test that hard-coded `CardId(1)` would therefore start silently asserting against a
//! different card the next time someone authors a definition earlier in the alphabet.
//!
//! Tests name cards the way printings, decklists, and the protocol do: by
//! [`FunctionalId`]. The handle is resolved through the database, never written down.

#![allow(clippy::expect_used, clippy::panic)]

use std::sync::OnceLock;

use crate::card::CardDatabase;
use crate::id::{CardId, FunctionalId};

/// The bundled catalog, parsed once for the whole test binary.
pub(crate) fn bundled() -> &'static CardDatabase {
    static DB: OnceLock<CardDatabase> = OnceLock::new();
    DB.get_or_init(|| CardDatabase::bundled().expect("the bundled catalog must load"))
}

/// The handle this build interned the bundled card `slug` under.
///
/// Panics if no such card is bundled — a test naming a card that does not exist is a
/// bug in the test, and failing loudly beats asserting against `CardId(0)`.
pub(crate) fn fixture(slug: &str) -> CardId {
    id_in(bundled(), slug)
}

/// The handle `db` interned `slug` under — for a test that builds its own catalog.
pub(crate) fn id_in(db: &CardDatabase, slug: &str) -> CardId {
    let functional_id =
        FunctionalId::try_from(slug.to_string()).expect("a test names a well-formed identity");
    db.card_id(&functional_id)
        .unwrap_or_else(|| panic!("`{slug}` is not in this catalog"))
}
