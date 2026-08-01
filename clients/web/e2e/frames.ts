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

/**
 * Serve `frames` to the client instead of a server, and record what it sends back.
 *
 * The client opens its socket and says `hello`; everything after that is whatever this hands
 * it. Interception happens in the page, so no port is bound and the two tiers never collide.
 */
export async function serveFrames(page: Page, frames: readonly unknown[]) {
  // The route handler runs in Node, so what the client sends is captured directly.
  const sent: string[] = []
  let socket: { send(message: string): void } | undefined
  await page.routeWebSocket(/.*/, (ws) => {
    socket = ws
    ws.onMessage((message) => sent.push(String(message)))
    for (const frame of frames) ws.send(JSON.stringify(frame))
  })
  // `push` sends a frame *after* the page has acted, which is the only way to test what a
  // client does with the server's answer to its own click.
  return { sent, push: (frame: unknown) => socket?.send(JSON.stringify(frame)) }
}

/** Every `choose_action` the page has sent, oldest first. */
export const submissions = (sent: readonly string[]): Record<string, unknown>[] =>
  sent.map((raw) => JSON.parse(raw)).filter((message) => message.type === 'choose_action')

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

/** The one layout this client has (`clients/web/AGENTS.md`): desktop landscape. */
export const DESKTOP = { width: 1440, height: 900 }
