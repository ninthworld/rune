/**
 * Parity between this TypeScript mirror and the Rust wire authority.
 *
 * The fixtures under `crates/sage-protocol/fixtures/` are single-sourced: Rust tests
 * deserialize and round-trip the same files, so a field renamed, retyped, or removed in
 * `sage-protocol` fails there. This suite is the other half — it fails here when the Rust side
 * changed and this mirror did not.
 *
 * The load-bearing assertion is `parsed == fixture`, not merely "parse succeeded". Schemas here
 * strip unknown keys so the client tolerates a newer server (`protocol.ts`), which means a
 * field this mirror is missing would be silently discarded and a parse-only test would stay
 * green. Comparing the parsed value back to the fixture turns that silent strip into a failure
 * naming the exact field.
 */
import { readFileSync, readdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import type { ZodType } from 'zod'

import { CatalogView, GameView, LobbyView, SpectatorView } from './protocol'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

const readFixture = (name: string): unknown =>
  JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))

/**
 * Every fixture, and the schema it must satisfy. Any fixture file not listed here fails the
 * coverage test below, so adding one on the Rust side forces a decision about the mirror
 * rather than silently going unchecked.
 */
const CASES: ReadonlyArray<readonly [string, ZodType]> = [
  ['gameview.json', GameView],
  ['gameview-actions.json', GameView],
  ['gameview-board.json', GameView],
  ['gameview-commander.json', GameView],
  ['gameview-over.json', GameView],
  ['gameview-prompts.json', GameView],
  ['gameview-choice.json', GameView],
  ['gameview-optional.json', GameView],
  ['gameview-emblem.json', GameView],
  ['gameview-turn.json', GameView],
  ['lobbyview.json', LobbyView],
  ['lobbyview-open.json', LobbyView],
  ['catalogview.json', CatalogView],
  ['spectatorview.json', SpectatorView],
]

describe('protocol mirror parity', () => {
  it.each(CASES.map(([name]) => name))('%s parses against its schema', (name) => {
    const schema = CASES.find(([n]) => n === name)![1]
    expect(() => schema.parse(readFixture(name))).not.toThrow()
  })

  it.each(CASES.map(([name]) => name))('%s survives parsing with no field dropped', (name) => {
    const schema = CASES.find(([n]) => n === name)![1]
    const fixture = readFixture(name)
    // A key present in the fixture but absent from the schema is stripped by `parse`, so any
    // inequality here names a field the Rust side sends and this mirror does not declare.
    expect(schema.parse(fixture)).toEqual(fixture)
  })

  it('checks every committed fixture', () => {
    const onDisk = readdirSync(FIXTURES)
      .filter((f) => f.endsWith('.json'))
      .sort()
    const covered = CASES.map(([name]) => name).sort()
    expect(onDisk).toEqual(covered)
  })
})

describe('wire conventions the client depends on', () => {
  it('reads an absent `connected` as connected, not disconnected', () => {
    const view = GameView.parse(readFixture('gameview.json'))
    // The flag rides the wire only when false, so absence is the common case and must never
    // be rendered as a disconnected seat.
    expect(view.me?.connected).toBeUndefined()
    expect(view.opponents?.every((o) => o.connected === undefined)).toBe(true)
  })

  it('distinguishes an absent collection from an explicitly empty one', () => {
    // No schema declares a default: the mirror reports what the server said. Turning absence
    // into `[]` is a UI concern, and doing it here would break the parity assertion above.
    //
    // Both spellings reach a client. The server elides an empty collection, but this fixture
    // carries `valid_actions: []` explicitly, so a client that treats only `undefined` as
    // "nothing to do" would read a finished game as still offering actions. Consumers must
    // handle both — hence `normalize.ts` rather than defaults in the schema.
    const view = GameView.parse(readFixture('gameview-over.json'))
    expect(view.battlefield).toBeUndefined()
    expect(view.valid_actions).toEqual([])
  })

  it('discriminates a stack target by its `kind` tag', () => {
    const view = GameView.parse(readFixture('gameview.json'))
    const targets = view.stack?.flatMap((item) => item.targets ?? []) ?? []
    for (const target of targets) {
      expect(['player', 'permanent', 'card', 'stack']).toContain(target.kind)
    }
  })

  it('tolerates a field a newer server added', () => {
    const fixture = readFixture('gameview.json') as Record<string, unknown>
    const parsed = GameView.parse({ ...fixture, some_future_field: { anything: true } })
    // Tolerated, and dropped — the client never sees a field it does not declare.
    expect(parsed).not.toHaveProperty('some_future_field')
    expect(parsed).toEqual(fixture)
  })
})
