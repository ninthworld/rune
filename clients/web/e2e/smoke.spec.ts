/**
 * The browser smoke canary (issue #279, ADR 0011).
 *
 * One spec. Two real browser contexts, one real seeded `rune-server`, the real
 * dev-server client. It walks the shipped path — front door → lobby → room →
 * deck → ready → mulligan → table — and then asserts the three things no
 * headless test in this repo can assert:
 *
 * 1. The table actually renders: the effects surface holds a live, attached
 *    WebGL canvas and the scene plane is populated and hit-testable. This is the
 *    regression guard for the StrictMode canvas detach — the bug that shipped
 *    with CI green because jsdom cannot see a render path.
 * 2. A land can be played *through the rendered UI* — hand card, then the dock's
 *    action chip — and the permanent then appears in the receiver's own band of
 *    the staged scene and as a laid-out card element on the plane.
 * 3. Priority can be passed around a full turn boundary through the rendered
 *    dock, and the server-owned turn number advances.
 *
 * Every decision about *what* to click is read from the server's `valid_actions`
 * via the read-only hook; nothing is submitted through it, and no legality is
 * computed here. There are no sleeps.
 */
import { test, expect } from './support/fixtures';
import {
  advancePastTurnBoundary,
  connect,
  createDuelRoom,
  joinRoom,
  keepOpeningHand,
  passPriorityThroughDock,
  playLandThroughUi,
  readPlane,
  readView,
  seatOfferedAction,
  submitDeckAndReady,
  turnOf,
  waitForReceiverBandRender,
  waitForTable,
} from './support/client';
import { readCanvasHealth, readPlaneHealth } from './support/render';

test('two browsers play real turns through the rendered table', async ({ browser, runeServer }) => {
  const hostContext = await browser.newContext();
  const guestContext = await browser.newContext();
  const host = await hostContext.newPage();
  const guest = await guestContext.newPage();
  const seats = [host, guest];

  await test.step('both players reach the room through the front door', async () => {
    await connect(host, runeServer.url);
    const roomId = await createDuelRoom(host);
    await connect(guest, runeServer.url);
    await joinRoom(guest, roomId);
  });

  await test.step('both submit a starter deck and ready up', async () => {
    await submitDeckAndReady(host);
    await submitDeckAndReady(guest);
  });

  await test.step('the first GameView mounts the table for both seats', async () => {
    await Promise.all(seats.map(waitForTable));
    // The hook is published by the mounted table, so this is the first point it
    // can exist. Fail here rather than confusingly later if the smoke target
    // was run without `VITE_RUNE_TEST_HOOKS`.
    const kind = await host.evaluate(() => typeof window.__RUNE_TEST__);
    expect(kind, 'the smoke target must run a build with VITE_RUNE_TEST_HOOKS set').toBe('object');
  });

  await test.step('both keep their opening hand', async () => {
    await Promise.all(seats.map(keepOpeningHand));
  });

  // ---------------------------------------------------------------- canary --
  // The assertion this whole suite exists for. A missing canvas (StrictMode
  // detach) or a blank plane fails right here.
  await test.step('the table renders: live canvas, populated scene plane', async () => {
    for (const [index, page] of seats.entries()) {
      const who = index === 0 ? 'host' : 'guest';

      const canvas = await readCanvasHealth(page);
      expect(canvas.present, `${who}: the effects surface should hold a canvas`).toBe(true);
      expect(canvas.connected, `${who}: that canvas should be in the live document`).toBe(true);
      expect(canvas.context, `${who}: the canvas should hold a live WebGL context`).not.toBeNull();
      expect(canvas.backingWidth, `${who}: canvas backing store should be sized`).toBeGreaterThan(
        0,
      );
      expect(
        canvas.drawingBufferWidth,
        `${who}: WebGL drawing buffer should be sized`,
      ).toBeGreaterThan(0);
      expect(canvas.cssWidth, `${who}: the canvas should be laid out`).toBeGreaterThan(0);
      expect(canvas.cssHeight, `${who}: the canvas should be laid out`).toBeGreaterThan(0);

      const plane = await readPlaneHealth(page);
      expect(plane.present, `${who}: the scene plane should be mounted`).toBe(true);
      expect(plane.hostWidth, `${who}: the scene plane should be laid out`).toBeGreaterThan(0);
      expect(plane.hostHeight, `${who}: the scene plane should be laid out`).toBeGreaterThan(0);
      expect(plane.centreHitsTable, `${who}: the middle of the table should hit-test`).toBe(true);
    }
  });

  // ------------------------------------------------ play a land, see it land --
  const actor = await test.step('find the seat the server offers the turn to', async () => {
    // The suite computes nothing: whichever seat the server hands `play_land`
    // to is the one that plays it.
    const page = await seatOfferedAction(seats, 'play_land');
    const view = await readView(page);
    expect(view?.active_player, 'the land is offered to the active player').toBe(view?.you);
    return page;
  });

  const land =
    await test.step('play a land by clicking the hand card and its dock chip', async () => {
      const before = await readPlaneHealth(actor);
      // `playLandThroughUi` resolves only once the authoritative view carries the
      // new permanent, so the board really changed — not just the presentation.
      const played = await playLandThroughUi(actor);
      expect(
        before.visibleEntityIds,
        'the land should not already have been on the board',
      ).not.toContain(played.permanentId);
      return played;
    });

  await test.step('the permanent appears in the receiver band and on the plane', async () => {
    // The derived scene: it is staged in the receiver's own band…
    await waitForReceiverBandRender(actor, land.permanentId);
    const plane = await readPlane(actor);
    expect(plane?.receiver?.seat, 'the receiver band belongs to the acting seat').toBe(
      (await readView(actor))?.you,
    );

    // …and it is actually painted: a laid-out card element on the DOM plane.
    await expect
      .poll(async () => (await readPlaneHealth(actor)).visibleEntityIds, {
        message: 'the played land should be a laid-out card element on the scene plane',
      })
      .toContain(land.permanentId);
  });

  // ------------------------------------------------------- a turn boundary --
  await test.step('the rendered dock carries the game across a turn boundary', async () => {
    const before = await turnOf(actor);
    expect(before, 'the game should be past the pre-game').toBeGreaterThan(0);
    const boundary = await advancePastTurnBoundary(seats, before);
    expect(boundary.turn, 'the server-owned turn number should advance').toBeGreaterThan(before);
    // Guard against the step going vacuous: if the boundary were ever crossed
    // with no rendered control involved, it would stop saying anything about
    // the UI. (ADR 0020 automation settles *idle* windows only; every window
    // that puts a decision to a player is answered here by clicking.)
    expect(boundary.clicks, 'rendered dock controls should have carried the turn').toBeGreaterThan(
      0,
    );

    // The table survived the turn boundary — still rendering, still not blank.
    const canvas = await readCanvasHealth(actor);
    expect(canvas.present && canvas.connected, 'the canvas should survive a turn boundary').toBe(
      true,
    );
    const plane = await readPlaneHealth(actor);
    expect(plane.centreHitsTable, 'the table should still be on screen').toBe(true);
  });

  await test.step("the dock's own pass control is live", async () => {
    // Explicit, because automation can carry a whole turn without any seat ever
    // being offered a pass: prove the rendered pass control itself submits.
    await passPriorityThroughDock(seats);
  });

  await hostContext.close();
  await guestContext.close();
});
