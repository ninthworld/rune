/**
 * The four-player vertical slice (issue #499, ADR 0011).
 *
 * **One scenario, run twice.** Four real browser contexts, one real seeded
 * `rune-server`, the real client, playing a four-seat pod from the front door
 * through the whole coherent loop: keep the opening hand → explicit main-phase
 * priority → play a land → deliberately tap for mana → cast a targeted spell →
 * watch it sit on the stack naming its target → pass and resolve → declare an
 * attacker against one of *several* legal defending players → have that defender
 * block → damage and death → a disconnect/reconnect with live combat on the
 * board → and on through the rest of the turn. Then the identical scenario again
 * with `prefers-reduced-motion: reduce`, and the two runs are required to reach
 * the same decisions, the same final state, and the same pixels.
 *
 * **What makes this a four-player test and not a duel with extra tabs.** With
 * three living opponents the server asks a question a duel never asks: *whom is
 * this creature attacking?* The declaration carries a `defend_<attacker>` slot
 * per declared attacker (issue #341/#347), Confirm stays disabled until it is
 * answered, and the answer is a click on one seat's crest among several. That is
 * the browser half of #457's criterion, and it is asserted here by clicking one
 * defender, then a *different* one, and requiring the server to record the
 * second — a defender the suite never clicked can never be what the view shows.
 *
 * **The pod.** A four-seat free-for-all (`ffa-4`), the format the bundled
 * catalog can actually seat four players in and play combat with: the host takes
 * the burn deck (its targeted removal is the targeted-spell leg) and the other
 * three take the creature deck (their boards are what combat needs). The
 * `commander` format is the natural home for this scenario and is deliberately
 * *not* used yet — the catalog's commander-legal pool is eight mono-green
 * non-basics, so a singleton commander deck is necessarily 92 Forests and the
 * pod cannot reach blockers or a creature death inside any CI budget. Switching
 * is one constant here once the catalog grows.
 *
 * **The rules the whole suite is bound by** (`clients/web/AGENTS.md`): every move
 * is a click on a control the client actually rendered — nothing is submitted
 * through `window.__RUNE_TEST__`, which is read-only and used only to decide
 * *which* control to click — and no legality is computed anywhere in here. There
 * are no sleeps; every wait is a wait on a condition.
 */
import { test, expect } from './support/fixtures';
import type { Browser, Page, TestInfo } from '@playwright/test';
import {
  connect,
  createRoom,
  joinRoom,
  keepOpeningHand,
  playLandThroughUi,
  readPlane,
  readView,
  submitDeckAndReady,
  waitForReceiverBandRender,
  waitForTable,
} from './support/client';
import {
  castSpellThroughUi,
  declareAttackThroughUi,
  declareBlockersThroughUi,
  offeredAction,
  tapForManaThroughUi,
} from './support/table';
import { driveUntil } from './support/drive';
import { readCanvasHealth, readPlaneHealth } from './support/render';
import { captureStable, expectSameComposition } from './support/shots';
import type { HookView } from './support/hook';
import type { RuneServerOptions, RuneServerProcess } from './support/runeServer';
import { PREVIEW_URL } from './support/targets';

/** The four-seat free-for-all the pod is played in (see the file header). */
const GAME_SETUP = 'ffa-4';
/** Seats in the pod. */
const SEATS = 4;
/**
 * The pinned shuffle seed. Chosen — not stumbled into — by replaying this
 * scenario's own policy against the real server across seeds and keeping the one
 * that reaches every milestone in the fewest moves: a targeted removal spell
 * castable at an opponent's creature on turn 5, and an attack with several legal
 * defenders (one of them holding an untapped blocker) on turn 6.
 */
const RNG_SEED = 7;
/** Pinned starting life, so a format default can never lengthen the pod. */
const STARTING_LIFE = 20;
/** The host's deck: its targeted burn is the targeted-spell leg. */
const HOST_DECK = 'ember-onslaught';
/** The other three seats' deck: creature-dense and carrying no removal, so
 * boards survive long enough to attack and block. */
const GUEST_DECK = 'verdant-coalition';

/** Everything one pass of the scenario decided and ended at, for cross-run comparison. */
interface PassOutcome {
  /** Table seat order, as the server assigned it. */
  seatOrder: string[];
  /** The face name of the land played through the hand + dock. */
  landName: string;
  /** The targeted spell that was cast, and what it was aimed at. */
  cast: { name: string; targetName: string; stackDescription: string };
  /** The attack the suite declared. */
  attack: {
    attackerName: string;
    defender: string;
    offeredDefenders: string[];
    confirmBlockedWithoutDefender: boolean;
  };
  /** The blocks the defending seat declared. */
  blockerNames: string[];
  /** Every seat's life where the pass stopped (past the turn boundary). */
  livesAtEnd: number[];
  /** Card names in every seat's own graveyard where the pass stopped. */
  graveyardsAtEnd: string[][];
  /** The turn the attack happened on, and the turn the pod ran on to. */
  turns: { attack: number; final: number };
  /** The pinned screenshots, by name. */
  shots: Record<string, Buffer>;
}

/** Names of the permanents in a view, keyed by entity id, for readable outcomes. */
function nameOf(view: HookView, id: string): string {
  return (
    view.battlefield.find((permanent) => permanent.id === id)?.card.name ??
    view.my_hand.find((card) => card.id === id)?.name ??
    id
  );
}

/** Wait until the shell has no session moment staging, so a capture is settled. */
async function waitForSettledTable(page: Page): Promise<void> {
  await page.waitForFunction(
    () =>
      document.querySelector('[data-testid="live-match-table"]')?.getAttribute('data-moment') ===
      null,
    undefined,
    { timeout: 30_000 },
  );
}

/**
 * Play the whole scenario once, on its own freshly launched server and its own
 * four browser contexts. `reducedMotion` is the OS-level preference Playwright
 * emulates — the accessibility signal the client composes into its own motion
 * setting (`useReducedMotion`), not a test-only switch.
 */
async function playPod(
  browser: Browser,
  launch: (options?: RuneServerOptions) => Promise<RuneServerProcess>,
  info: TestInfo,
  pass: { label: string; reducedMotion: 'reduce' | 'no-preference' },
): Promise<PassOutcome> {
  const server = await launch({ rngSeed: RNG_SEED, startingLife: STARTING_LIFE });
  const contexts = await Promise.all(
    Array.from({ length: SEATS }, () => browser.newContext({ reducedMotion: pass.reducedMotion })),
  );
  const seats: Page[] = [];
  for (const context of contexts) seats.push(await context.newPage());
  const [host] = seats as [Page, ...Page[]];
  const shots: Record<string, Buffer> = {};

  try {
    await test.step(`${pass.label}: four players reach one four-seat room`, async () => {
      await connect(host, server.url, PREVIEW_URL);
      const roomId = await createRoom(host, GAME_SETUP, SEATS);
      for (const guest of seats.slice(1)) {
        await connect(guest, server.url, PREVIEW_URL);
        await joinRoom(guest, roomId);
      }
      await submitDeckAndReady(host, HOST_DECK);
      for (const guest of seats.slice(1)) await submitDeckAndReady(guest, GUEST_DECK);
      await Promise.all(seats.map(waitForTable));
      const kind = await host.evaluate(() => typeof window.__RUNE_TEST__);
      expect(kind, 'the target must run a build with VITE_RUNE_TEST_HOOKS set').toBe('object');
      await Promise.all(seats.map(keepOpeningHand));
    });

    await test.step(`${pass.label}: the pod renders as four live seats`, async () => {
      for (const [index, page] of seats.entries()) {
        const view = await readView(page);
        expect(view?.seat_order.length, `seat ${index}: the table seats four`).toBe(SEATS);
        expect(view?.opponents.length, `seat ${index}: three opponents`).toBe(SEATS - 1);
        expect(
          view?.opponents.every((opponent) => opponent.eliminated !== true),
          `seat ${index}: every opponent is still alive`,
        ).toBe(true);

        // The render path itself — the thing no headless test in this repo can
        // see. A four-seat plane is receiver + far side + two wings; a duel
        // would stage no wings at all, so this fails if the composition ever
        // collapses back to two.
        const canvas = await readCanvasHealth(page);
        expect(canvas.present && canvas.connected, `seat ${index}: live effects canvas`).toBe(true);
        expect(canvas.context, `seat ${index}: a live WebGL context`).not.toBeNull();
        const health = await readPlaneHealth(page);
        expect(health.present, `seat ${index}: the scene plane is mounted`).toBe(true);
        expect(health.centreHitsTable, `seat ${index}: the table is on screen`).toBe(true);
        const plane = await readPlane(page);
        expect(plane?.seats.length, `seat ${index}: four seats staged`).toBe(SEATS);
        expect(plane?.receiver?.seat, `seat ${index}: the receiver band is this seat`).toBe(
          await page.evaluate(() => window.__RUNE_TEST__?.view?.you),
        );
        expect(plane?.farSide, `seat ${index}: a focused opponent is staged`).toBeDefined();
        expect(plane?.wings.length, `seat ${index}: the other two opponents ride wings`).toBe(2);
      }
      await waitForSettledTable(host);
      shots['pod-table'] = await captureStable(host, `${pass.label}-pod-table`, info);
    });

    // ------------------------------------------------ priority, land, mana --
    const landName = await test.step(`${pass.label}: explicit priority, then a land`, async () => {
      const found = await driveUntil(
        seats,
        (views) => {
          const index = views.findIndex(
            (view) =>
              view !== null &&
              view.phase === 'precombat_main' &&
              view.active_player === view.you &&
              view.valid_actions.some((action) => action.type === 'play_land') &&
              view.valid_actions.some((action) => action.type === 'pass_priority'),
          );
          return index < 0 ? null : index;
        },
        { what: 'a main-phase priority window with a land to play' },
      );
      const actor = seats[found.found]!;
      const view = await readView(actor);

      // Explicit priority: the seat was *handed* the window (ADR 0020 settles
      // only idle ones) and the dock renders the server's own pass control for
      // it. Both halves matter — the rendered control is what makes it explicit.
      const pass = view?.valid_actions.find((action) => action.type === 'pass_priority');
      await expect(
        actor.getByTestId('action-bar').getByRole('button', { name: pass!.label }),
        'the dock should render the main-phase pass control',
      ).toBeVisible();

      const land = await playLandThroughUi(actor);
      await waitForReceiverBandRender(actor, land.permanentId);
      await expect
        .poll(async () => (await readPlaneHealth(actor)).visibleEntityIds, {
          message: 'the played land should be a laid-out card element on the scene plane',
        })
        .toContain(land.permanentId);

      // Deliberate mana activation (#463): select the land, then fire its ability
      // from the dock. The pool the client shows is the server's.
      const mana = await tapForManaThroughUi(actor);
      expect(mana.after, "tapping a land should add mana to this seat's pool").toBeGreaterThan(
        mana.before,
      );
      await expect(actor.getByTestId('hud-mana'), 'the pool should be on screen').toBeVisible();
      const tapped = await actor.evaluate(
        (id) => window.__RUNE_TEST__?.view?.battlefield.find((p) => p.id === id)?.tapped === true,
        mana.permanentId,
      );
      expect(tapped, 'the tapped land should read as tapped in the authoritative view').toBe(true);
      return land.name;
    });

    // ------------------------------------- a targeted spell, seen and resolved --
    const cast =
      await test.step(`${pass.label}: cast a targeted spell and resolve it`, async () => {
        const found = await driveUntil(
          seats,
          (views) => {
            for (const [index, view] of views.entries()) {
              if (view === null) continue;
              const action = view.valid_actions.find(
                (candidate) =>
                  candidate.type === 'cast_spell' &&
                  (candidate.requirements?.[0]?.candidates?.length ?? 0) > 0,
              );
              if (action === undefined) continue;
              // Aim at a creature someone else controls: the resolution is then
              // visible as damage on a specific object rather than a life total.
              const target = (action.requirements![0]!.candidates ?? []).find((id) =>
                view.battlefield.some(
                  (permanent) =>
                    permanent.id === id &&
                    permanent.card.power !== undefined &&
                    permanent.controller !== view.you,
                ),
              );
              if (target !== undefined) return { index, target };
            }
            return null;
          },
          { what: 'a targeted spell aimed at an opposing creature' },
        );
        const actor = seats[found.found.index]!;
        const before = await readView(actor);
        const targetName = nameOf(before!, found.found.target);
        const spell = await castSpellThroughUi(actor, {
          pickTarget: () => found.found.target,
        });

        // Stack placement, in the rendered chrome: the entry exists, it is the one
        // that resolves next, and the server's own description names the target —
        // the target path in the channel that survives reduced motion.
        const stacked = await actor.evaluate(() => window.__RUNE_TEST__?.view?.stack ?? []);
        expect(stacked.length, 'the cast spell should be on the stack').toBeGreaterThan(0);
        const top = stacked[stacked.length - 1]!;
        await expect(actor.getByTestId('stack-panel')).toBeVisible();
        await expect(actor.getByTestId(`stack-top-${top.id}`)).toBeVisible();
        expect(top.description, 'the stack entry should name the chosen target').toContain(
          targetName,
        );

        // Pass and resolve: the object leaves the stack and the thing it was
        // pointed at is measurably worse off (marked damage, or dead).
        await driveUntil(
          seats,
          (views) => {
            const view = views[found.found.index];
            if (view === null || view === undefined) return null;
            if (view.stack.length > 0) return null;
            const still = view.battlefield.find((permanent) => permanent.id === found.found.target);
            const dead = still === undefined;
            const hurt = (still?.damage ?? 0) > 0;
            return dead || hurt ? { dead, damage: still?.damage ?? 0 } : null;
          },
          { what: 'the targeted spell resolving onto its target' },
        );
        return { name: spell.name, targetName, stackDescription: top.description };
      });

    // --------------------------------------------- combat: attack, block, damage --
    const combat =
      await test.step(`${pass.label}: attack a chosen defender, and be blocked`, async () => {
        const found = await driveUntil(
          seats,
          (views) => {
            for (const [index, view] of views.entries()) {
              if (view === null) continue;
              const action = view.valid_actions.find((a) => a.type === 'declare_attackers');
              if (action === undefined) continue;
              const requirements = action.requirements ?? [];
              const attackers = requirements.find((slot) => slot.slot === 'attackers');
              if ((attackers?.candidates?.length ?? 0) === 0) continue;
              const defenders = requirements.find((slot) => slot.slot.startsWith('defend_'));
              const offered = defenders?.candidates ?? [];
              if (offered.length < 2) continue;
              // Only stop where a block is actually possible, so the blockers leg
              // can never quietly become a no-op.
              const blockable = offered.some((seat) =>
                view.battlefield.some(
                  (permanent) =>
                    permanent.controller === seat &&
                    permanent.card.power !== undefined &&
                    permanent.tapped !== true,
                ),
              );
              if (blockable) return index;
            }
            return null;
          },
          { what: 'an attack with several legal defenders, one of them able to block' },
        );
        const attacker = seats[found.found]!;
        const beforeAttack = await readView(attacker);
        const attackTurn = beforeAttack!.turn;

        const declared = await declareAttackThroughUi(attacker, {
          onDefenderChosen: async () => {
            shots['attack-assignment'] = await captureStable(
              attacker,
              `${pass.label}-attack-assignment`,
              info,
            );
          },
        });

        // The multiplayer property a duel cannot have: several legal defenders, a
        // Confirm that stayed shut until one was named, and a recorded assignment
        // that is the seat actually clicked last.
        expect(
          declared.offeredDefenders.length,
          'a four-player pod offers several legal defending players',
        ).toBeGreaterThanOrEqual(2);
        expect(
          declared.confirmBlockedWithoutDefender,
          'a declared attacker with no defending player must not be confirmable',
        ).toBe(true);
        expect(
          declared.offeredDefenders.slice(0, 1),
          'the recorded defender should not be the first one clicked',
        ).not.toContain(declared.defenderId);

        const defender = await (async () => {
          for (const page of seats) {
            if ((await readView(page))?.you === declared.defenderId) return page;
          }
          throw new Error(`no browser context is seated at ${declared.defenderId}`);
        })();

        // The defending seat is told, in words, on its own screen.
        await expect(
          defender.getByTestId('topbar-attacked'),
          'the attacked seat should read that it is under attack',
        ).toBeVisible();

        // Blockers, declared through the rendered controls by the seat that was
        // actually attacked.
        await expect
          .poll(async () => (await offeredAction(defender, 'declare_blockers')) !== null, {
            message: 'the attacked seat should be asked to declare blockers',
          })
          .toBe(true);
        const blocks = await declareBlockersThroughUi(defender);
        expect(blocks.length, 'the attacked seat should have blocked').toBeGreaterThan(0);
        await defender
          .waitForFunction(
            (blockerId) =>
              window.__RUNE_TEST__?.view?.battlefield.some(
                (permanent) => permanent.id === blockerId && permanent.blocking !== undefined,
              ) === true,
            blocks[0]!.blockerId,
            { timeout: 20_000 },
          )
          .catch(() => {
            throw new Error('the declared blocker was never recorded as blocking');
          });

        const beforeDamage = await readView(defender);
        const blockerIds = blocks.map((block) => block.blockerId);
        const blockerNames = blocks.map((block) => nameOf(beforeDamage!, block.blockerId));
        const attackerName = nameOf(beforeAttack!, declared.attackerId);
        return {
          declared,
          attacker,
          defender,
          attackTurn,
          attackerName,
          blockerIds,
          blockerNames,
        };
      });

    await test.step(`${pass.label}: the assignment is legible while combat is live`, async () => {
      // #457's browser criterion, half one: with blockers declared and damage not
      // yet dealt, the assignment is readable — in the authoritative view, in the
      // staged scene, and on the defending player's own screen.
      //
      // Asserted *here*, at the declare-blockers step, and not at the damage step,
      // because this is the last point the state is guaranteed to be observable at
      // all. The room settles every auto-passable seat before it broadcasts
      // (`room/driver.rs`, `settle_auto_passes` then `broadcast`) and rooms the
      // lobby creates run with auto-pass on, so a step where no seat has a
      // meaningful action is never published to any client. Casting is
      // sorcery-gated (`actions/generation.rs`), so only a seat holding an instant
      // and the mana for it keeps the combat-damage step observable — which is a
      // property of the seed's opening hands, not of the client under test. The
      // declare-blockers step, by contrast, is a decision this suite itself
      // answers, so it is always broadcast.
      const view = await readView(combat.attacker);
      const attacking = view?.battlefield.find(
        (permanent) => permanent.id === combat.declared.attackerId,
      );
      expect(
        attacking,
        'the declared attacker is on the board with blockers declared',
      ).toBeDefined();
      expect(attacking!.attacking, 'and reads as attacking').toBe(true);
      expect(attacking!.attacking_player, 'against the defending seat that was clicked').toBe(
        combat.declared.defenderId,
      );

      // The same fact in the *rendered* scene, not only the view.
      const plane = await readPlane(combat.attacker);
      const regions = [plane?.receiver, plane?.farSide, ...(plane?.wings ?? [])];
      expect(
        regions
          .flatMap((region) => region?.renders ?? [])
          .some((render) => render.memberIds.includes(attacking!.id) && render.attacking),
        'the scene should stage the attacker with its combat treatment',
      ).toBe(true);
      expect(
        regions.find((region) => region?.seat === combat.declared.defenderId)?.attacked,
        'the defending seat should wear the attacked marker',
      ).toBe(true);
      await expect(
        combat.defender.getByTestId('topbar-attacked'),
        "the defending seat's own screen should say it is under attack",
      ).toBeVisible();

      shots['combat-assignment'] = await captureStable(
        combat.attacker,
        `${pass.label}-combat-assignment`,
        info,
      );
    });

    await test.step(`${pass.label}: the assignment is honored through damage`, async () => {
      // #457's browser criterion, half two: carry the combat to damage and require
      // that the damage went where the declaration sent it.
      //
      // This is deliberately *not* "the attacker still reads as attacking at the
      // damage step". Two engine facts rule that formulation out. The end-of-combat
      // step's turn-based action clears `attacking` on every permanent
      // (`apply/combat.rs#remove_creatures_from_combat`, run on entering
      // `Step::EndCombat`, which follows `Step::CombatDamage` in `phase.rs`), so a
      // stop that accepted `end_combat` could only ever contradict itself. And an
      // attacker that traded with its blocker is not on the battlefield to read at
      // all. So the assertion is made on the thing that survives both: *where the
      // damage landed*. A defending player this suite never named could not be the
      // one whose creature took it.
      await driveUntil(
        seats,
        (views) => {
          const view = views[seats.indexOf(combat.attacker)];
          if (view === null || view === undefined) return null;
          // Damage is dealt as the combat-damage step's turn-based action, so any
          // view at or past that step has already had it applied.
          const dealt = ['combat_damage', 'end_combat', 'postcombat_main', 'end', 'cleanup'];
          return dealt.includes(view.phase) ? view.phase : null;
        },
        { what: 'a view at or past the combat damage step' },
      );

      const attackerView = await readView(combat.attacker);
      const marked = (attackerView?.battlefield ?? []).some(
        (permanent) => (permanent.damage ?? 0) > 0,
      );
      const buried = (attackerView?.graveyards ?? []).some((pile) =>
        pile.cards.some((card) => card.power !== undefined),
      );
      expect(marked || buried, 'combat damage should have been dealt').toBe(true);

      // The blocker this suite declared is the object the attacker's damage was
      // assigned to (CR 510.1a), so it carries marked damage or it died to it.
      // Either outcome proves the attack was routed at the seat that was clicked;
      // neither assumes the attacker itself survived the trade.
      const blockerId = combat.blockerIds[0];
      expect(blockerId, 'the combat should have had a declared blocker').toBeDefined();
      const blocker = attackerView?.battlefield.find((permanent) => permanent.id === blockerId);
      const defenderGraveyard =
        attackerView?.graveyards.find((pile) => pile.player_id === combat.declared.defenderId)
          ?.cards ?? [];
      const blockerTookDamage = (blocker?.damage ?? 0) > 0;
      const blockerDied =
        blocker === undefined &&
        defenderGraveyard.some((card) => card.name === combat.blockerNames[0]);
      expect(
        blockerTookDamage || blockerDied,
        "the declared blocker should carry the attacker's combat damage, or have died to it",
      ).toBe(true);

      shots['combat-damage'] = await captureStable(
        combat.attacker,
        `${pass.label}-combat-damage`,
        info,
      );
    });

    // ---------------------------------------------------------- reconnect --
    await test.step(`${pass.label}: a reconnect rebuilds the latest scene`, async () => {
      const reconnecting = combat.attacker;
      const beforeDrop = await readView(reconnecting);

      // Leave meaningful *presentation* state behind on this seat: an open
      // inspect surface, which nothing in the view carries. Combat is live on
      // the board underneath it, which is the state the reconnect must rebuild.
      const handCard = beforeDrop!.my_hand[0];
      expect(handCard, 'the reconnecting seat should have a card to inspect').toBeDefined();
      await reconnecting.getByTestId(`live-hand-card-${handCard!.id}`).click();
      await expect(
        reconnecting.getByTestId('card-inspect'),
        'the inspect surface should be open before the drop',
      ).toBeVisible();

      // Drop the connection the way a player does: reload the tab. The seat is
      // held open server-side and reclaimed with the stored session token.
      await reconnecting.reload();
      await waitForTable(reconnecting);
      await reconnecting.waitForFunction(
        (turn) => (window.__RUNE_TEST__?.view?.turn ?? 0) >= turn,
        beforeDrop!.turn,
        { timeout: 60_000 },
      );

      const rebuilt = await readView(reconnecting);
      expect(rebuilt?.you, 'the reclaimed seat is the same seat').toBe(beforeDrop!.you);
      expect(rebuilt?.turn, 'the rebuilt view is not older than the one it replaced').toBe(
        beforeDrop!.turn,
      );
      // The complete scene, not a fragment: every permanent that was on the
      // board is on it again, with the combat assignment intact.
      const before = beforeDrop!.battlefield.map((permanent) => permanent.id).sort();
      const after = (rebuilt?.battlefield ?? []).map((permanent) => permanent.id).sort();
      expect(after, 'the whole battlefield is rebuilt from the latest view').toEqual(before);
      const attackerAfter = rebuilt?.battlefield.find(
        (permanent) => permanent.id === combat.declared.attackerId,
      );
      if (attackerAfter !== undefined) {
        expect(attackerAfter.attacking_player, 'the attack assignment survives the drop').toBe(
          combat.declared.defenderId,
        );
      }
      const health = await readPlaneHealth(reconnecting);
      expect(health.centreHitsTable, 'the rebuilt table is really on screen').toBe(true);

      // …and the stale presentation state is gone. Nothing the player had open
      // is load-bearing across messages, so nothing may come back with them.
      await expect(
        reconnecting.getByTestId('card-inspect'),
        'the reconnect must not restore the inspect surface',
      ).toBeHidden();
      await expect(reconnecting.getByTestId('selection-echo')).toBeHidden();
      await expect(reconnecting.getByTestId('decision-sheet')).toBeHidden();
    });

    await test.step(`${pass.label}: a fresh view clears another seat's stale surface`, async () => {
      // The in-page half of the same rule, which a reload cannot speak for: a
      // seat that is *not* reconnecting also loses its open surface the moment
      // the server pushes a new complete view.
      const bystander = seats.find((page) => page !== combat.attacker)!;
      const view = await readView(bystander);
      const card = view!.my_hand[0];
      expect(card, 'the bystanding seat should have a card to inspect').toBeDefined();
      await bystander.getByTestId(`live-hand-card-${card!.id}`).click();
      await expect(bystander.getByTestId('card-inspect')).toBeVisible();

      await driveUntil(
        seats,
        (views) => {
          const now = views[seats.indexOf(bystander)];
          return now !== null && now !== undefined && now.turn > view!.turn ? now.turn : null;
        },
        { what: 'a fresh view for the bystanding seat' },
      );
      await expect(
        bystander.getByTestId('card-inspect'),
        'a fresh complete view supersedes every ephemeral presentation choice',
      ).toBeHidden();
    });

    // ------------------------------------------- on through the turn boundary --
    const finalTurn = await test.step(`${pass.label}: play continues past the turn`, async () => {
      const report = await driveUntil(
        seats,
        (views) => {
          const view = views[seats.indexOf(combat.attacker)];
          if (view === null || view === undefined) return null;
          if (view.turn <= combat.attackTurn) return null;
          // Back to the receiver: the seat that attacked is being asked for a
          // decision again on the far side of the boundary.
          return view.valid_actions.length > 0 ? view.turn : null;
        },
        { what: 'the turn boundary and a fresh decision for the attacking seat' },
      );
      expect(
        report.moves,
        'rendered controls should have carried the pod across the boundary',
      ).toBeGreaterThan(0);
      return report.found;
    });

    const finalViews = await Promise.all(seats.map(readView));
    return {
      seatOrder: finalViews[0]!.seat_order,
      landName,
      cast,
      attack: {
        attackerName: combat.attackerName,
        defender: combat.declared.defenderId,
        offeredDefenders: combat.declared.offeredDefenders,
        confirmBlockedWithoutDefender: combat.declared.confirmBlockedWithoutDefender,
      },
      blockerNames: combat.blockerNames,
      livesAtEnd: finalViews.map((view) => view!.me.life),
      graveyardsAtEnd: finalViews.map(
        (view) =>
          view!.graveyards.find((pile) => pile.player_id === view!.you)?.cards.map((c) => c.name) ??
          [],
      ),
      turns: { attack: combat.attackTurn, final: finalTurn },
      shots,
    };
  } finally {
    for (const context of contexts) await context.close();
    await server.stop();
  }
}

/**
 * One pass's budget.
 *
 * A four-context pod is bound by *software* rasterization, not by the game: the
 * suite runs Chromium's SwiftShader backend (no GPU on a CI runner), the scene's
 * ambient environment animates continuously under full motion
 * (`live-plane.module.css#ambientDrift`), and compositing four 1280×900 frames at
 * animation rate in software saturates every core a runner has. Measured on a
 * 4-core box, the browser's GPU process alone accrues CPU time at ~3.6 cores for
 * the whole pass. So the budget is generous by necessity, and per *pass* rather
 * than for both: a shared budget cannot say which pass was slow, and a pass that
 * overruns should fail as itself.
 */
const PASS_BUDGET_MS = 30 * 60_000;

/**
 * The full-motion pass's outcome, handed to the reduced-motion pass.
 *
 * Module state shared between tests is normally a smell; `test.describe.serial`
 * is the sanctioned mechanism for exactly this, and the alternative — one test
 * containing both passes — is what this replaces. Serial also means a failed
 * first pass *skips* the second instead of spending another half-hour proving
 * the same thing twice.
 */
let fullMotion: PassOutcome | null = null;

test.describe.serial('a four-player pod, played twice', () => {
  test('four browsers play the pod through the rendered table', async ({
    browser,
    launchRuneServer,
  }, info) => {
    test.setTimeout(PASS_BUDGET_MS);
    fullMotion = await playPod(browser, launchRuneServer, info, {
      label: 'pass-1',
      reducedMotion: 'no-preference',
    });
  });

  test('the same pod under prefers-reduced-motion reaches the same place', async ({
    browser,
    launchRuneServer,
  }, info) => {
    test.setTimeout(PASS_BUDGET_MS);
    const full = fullMotion;
    expect(full, 'the full-motion pass should have produced an outcome').not.toBeNull();

    const reduced = await playPod(browser, launchRuneServer, info, {
      label: 'pass-2',
      reducedMotion: 'reduce',
    });

    await test.step('the reduced-motion run reaches the same decisions', async () => {
      // Nothing about what the pod decided may depend on animation: same seats,
      // same land, same spell at the same target, same attacker at the same
      // defender out of the same offered set, same blockers.
      expect(reduced.seatOrder).toEqual(full!.seatOrder);
      expect(reduced.landName).toEqual(full!.landName);
      expect(reduced.cast).toEqual(full!.cast);
      expect(reduced.attack).toEqual(full!.attack);
      expect(reduced.blockerNames).toEqual(full!.blockerNames);
    });

    await test.step('…and the same final states', async () => {
      expect(reduced.livesAtEnd).toEqual(full!.livesAtEnd);
      expect(reduced.graveyardsAtEnd).toEqual(full!.graveyardsAtEnd);
      expect(reduced.turns).toEqual(full!.turns);
    });

    await test.step('the screenshot set is stable across the two runs', async () => {
      for (const name of Object.keys(full!.shots)) {
        await expectSameComposition(name, full!.shots[name]!, reduced.shots[name]!, info);
      }
    });
  });
});
