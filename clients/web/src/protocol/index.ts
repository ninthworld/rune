/**
 * RUNE protocol — the TypeScript mirror of the `rune-protocol` crate.
 *
 * These are the wire shapes the server and every client share (see
 * `docs/protocol.md` and `crates/rune-protocol/src/lib.rs`). The client is a dumb
 * renderer: it displays a {@link GameView}} and echoes back a `ChooseAction` with
 * the `id` of one issued {@link ValidAction}}. It never computes legality, cost,
 * or effect — all displayed characteristics are server-computed.
 *
 * Any change to these shapes is a contract change: it must update the Rust crate
 * and `docs/protocol.md` in the same PR.
 */

/** Opaque player identity (server-assigned). */
export type PlayerId = string;

/** Opaque per-game entity id: a card, permanent, or stack object. */
export type EntityId = string;

// Shared types are exported directly; re-export all modules at the root.
export * from './card.js';
export * from './action.js';
export * from './result.js';
export * from './view.js';
export * from './log.js';
export * from './spectator.js';
export * from './client.js';
export * from './lobby.js';
export * from './catalog.js';
