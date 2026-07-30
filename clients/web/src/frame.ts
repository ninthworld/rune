/**
 * Classifying one server → client frame.
 *
 * Server frames are **untagged**: there is no envelope naming the message type, so a client
 * discriminates structurally. The rules are the protocol's, not this client's invention
 * (`docs/protocol.md`), and they are checked in a fixed order because they overlap — a
 * `GameView` and a `SpectatorView` both carry `phase`, and only `you` separates them.
 *
 *   1. `lobby_error` present  → `LobbyErrorFrame`  (a key carried by no other frame)
 *   2. `catalog_version`      → `CatalogView`      (a `LobbyView` carries none)
 *   3. `phase` and `you`      → `GameView`
 *   4. `phase`, no `you`      → `SpectatorView`    (a seated view always serializes `you`)
 *   5. otherwise              → `LobbyView`
 *
 * An unrecognized or unparseable frame is reported as `unknown` rather than thrown: a newer
 * server may send something this client has no concept of, and the connection must survive it.
 */
import { CatalogView, GameView, LobbyErrorFrame, LobbyView, SpectatorView } from './protocol'

export type ServerFrame =
  | { kind: 'lobby'; view: LobbyView }
  | { kind: 'game'; view: GameView }
  | { kind: 'spectator'; view: SpectatorView }
  | { kind: 'catalog'; view: CatalogView }
  | { kind: 'lobby_error'; frame: LobbyErrorFrame }
  | { kind: 'unknown'; reason: string; raw: unknown }

const has = (value: Record<string, unknown>, key: string): boolean =>
  Object.prototype.hasOwnProperty.call(value, key)

/** Classify and parse one decoded JSON payload. Never throws. */
export function classifyFrame(raw: unknown): ServerFrame {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return { kind: 'unknown', reason: 'frame is not a JSON object', raw }
  }
  const value = raw as Record<string, unknown>

  const parse = (
    kind: Exclude<ServerFrame['kind'], 'unknown' | 'lobby_error'>,
    schema: { safeParse: (v: unknown) => { success: boolean; data?: unknown; error?: unknown } },
  ): ServerFrame => {
    const result = schema.safeParse(value)
    if (!result.success) {
      return { kind: 'unknown', reason: `${kind} frame failed validation`, raw }
    }
    // The cast is safe: each schema's output is the type its branch declares.
    return { kind, view: result.data } as ServerFrame
  }

  if (has(value, 'lobby_error')) {
    const result = LobbyErrorFrame.safeParse(value)
    return result.success
      ? { kind: 'lobby_error', frame: result.data }
      : { kind: 'unknown', reason: 'lobby_error frame failed validation', raw }
  }
  if (has(value, 'catalog_version')) return parse('catalog', CatalogView)
  if (has(value, 'phase')) {
    return has(value, 'you') ? parse('game', GameView) : parse('spectator', SpectatorView)
  }
  return parse('lobby', LobbyView)
}

/** Decode a raw socket payload, then classify it. Never throws. */
export function decodeFrame(text: string): ServerFrame {
  try {
    return classifyFrame(JSON.parse(text))
  } catch {
    return { kind: 'unknown', reason: 'payload is not valid JSON', raw: text }
  }
}
