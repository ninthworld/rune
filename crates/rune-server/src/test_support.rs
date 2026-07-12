//! Test-only: naming bundled cards by authored identity instead of interned handle.
//!
//! A [`CardId`] is interned by the engine's `build.rs` from the catalog's sort order
//! (ADR 0018 §3), so it is not a stable *name* for a card — authoring one new definition
//! renumbers its neighbours. A test that hard-coded `CardId(1)` would quietly start
//! asserting against a different card.
//!
//! Server tests therefore name cards the way the protocol does — by `functional_id` —
//! and resolve the handle through the database.

#![allow(clippy::expect_used, clippy::panic)]

use std::sync::OnceLock;

use rune_engine::{CardDatabase, CardId, FunctionalId};

/// The bundled catalog, parsed once for the whole test binary.
pub(crate) fn bundled() -> &'static CardDatabase {
    static DB: OnceLock<CardDatabase> = OnceLock::new();
    DB.get_or_init(|| CardDatabase::bundled().expect("the bundled catalog must load"))
}

/// The handle this build interned the bundled card `slug` under.
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
