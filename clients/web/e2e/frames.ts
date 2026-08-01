/**
 * The harness the fixture-driven browser tier is built on.
 *
 * No server, no engine, no game — just "given exactly this view, the browser renders this". The
 * fixtures are the same files the Rust tests pin and the unit suite parses, so this tier cannot
 * drift from the wire shape; what it adds over a unit test is a real build, in a real browser,
 * painting real DOM.
 *
 * Shared by every `*views.spec.ts` file, so the interception, the fixture path, and the reading
 * of what the client sent are written once. Not a spec file itself: the `views` project matches
 * on the filename, and this one does not.
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import type { Page } from '@playwright/test'

const FIXTURES = join(
  dirname(fileURLToPath(import.meta.url)),
  '../../../crates/sage-protocol/fixtures',
)

export const fixture = (name: string): Record<string, unknown> =>
  JSON.parse(readFileSync(join(FIXTURES, name), 'utf8'))

/** The half of a `WebSocketRoute` these helpers use, so a caller needs no Playwright type. */
interface Socket {
  send(message: string): void
  close(options?: { code?: number; reason?: string }): void
}

/**
 * Serve one round of frames per socket the client opens, and record what it sends back.
 *
 * The client opens its socket and says `hello`; everything after that is whatever this hands
 * it. Interception happens in the page, so no port is bound and the two tiers never collide.
 *
 * More than one round exists because the client **reconnects**: closing a socket makes it open
 * another, and what the server says on that second socket is the whole substance of resuming.
 * A round past the end of the list repeats the last one, so a page that reconnects twice is not
 * a test that has to enumerate it.
 */
export async function serveSockets(page: Page, rounds: readonly (readonly unknown[])[]) {
  // The route handler runs in Node, so what the client sends is captured directly.
  const sent: string[] = []
  const sockets: Socket[] = []
  await page.routeWebSocket(/.*/, (ws) => {
    const frames = rounds[Math.min(sockets.length, rounds.length - 1)] ?? []
    sockets.push(ws)
    ws.onMessage((message) => sent.push(String(message)))
    for (const frame of frames) ws.send(JSON.stringify(frame))
  })
  return {
    sent,
    sockets,
    // `push` sends a frame *after* the page has acted, which is the only way to test what a
    // client does with the server's answer to its own click.
    push: (frame: unknown) => sockets.at(-1)?.send(JSON.stringify(frame)),
    /** Drop the live socket, as a network does. The client is expected to open another. */
    drop: () => sockets.at(-1)?.close({ code: 1006 }),
  }
}

/** The single-socket case, which is most of them. */
export async function serveFrames(page: Page, frames: readonly unknown[]) {
  return serveSockets(page, [frames])
}

/** Every message of one `type` the page has sent, oldest first. */
export const messages = (sent: readonly string[], type: string): Record<string, unknown>[] =>
  sent.map((raw) => JSON.parse(raw)).filter((message) => message.type === type)

/** Every `choose_action` the page has sent, oldest first. */
export const submissions = (sent: readonly string[]): Record<string, unknown>[] =>
  messages(sent, 'choose_action')

/**
 * The page itself does not scroll, in either axis.
 *
 * Asked by trying to scroll rather than by comparing `scrollWidth` — a region that clips its
 * own overflow still inflates the root element's reported scroll width in Chrome, and what
 * actually matters to a player is whether the table can be scrolled out from under them.
 */
export const pageFits = (page: Page) =>
  page.evaluate(() => {
    window.scrollTo(9999, 9999)
    return { x: window.scrollX === 0, y: window.scrollY === 0 }
  })

/**
 * A representative desktop viewport — the Wide band of `docs/client-design.md` §4, comfortably
 * inside the Optimized class. It is one sample, not the layout: a test asserting behaviour that
 * must hold *at every* supported size belongs in `scale.views.spec.ts`, which sweeps the bands.
 */
export const DESKTOP = { width: 1440, height: 900 }
