/**
 * The plane's depth ladder (issue #582).
 *
 * `shellLayout.test.ts` pins the SHELL ladder — the one that decides whether a
 * decision can be covered by chrome. This pins the other one: the order objects
 * are painted in *inside* the scene, which the plane host isolates with
 * `isolation: isolate` and which therefore never competes with the shell's.
 *
 * The maintainer's report is why it exists: "Cards render **under** the cluster
 * while other overlays render above it, so the stacking is falling out of
 * render order rather than out of a declared depth model." The order was in
 * fact declared, as eleven bare numbers scattered across two stylesheets with
 * no statement anywhere of what they meant — which is indistinguishable from
 * render order to anyone reading the code, and unfixable without guessing.
 *
 * jsdom paints nothing, so the stylesheet is where this is provable. These
 * tests read the CSS source rather than a computed style.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

/** Every `--rune-z-*` token declared in the chrome tokens, by name. */
function ladder(tokens: string): Map<string, number> {
  const declared = new Map<string, number>();
  for (const [, name, value] of tokens.matchAll(/--rune-z-([a-z-]+):\s*(-?\d+);/g)) {
    declared.set(name!, Number(value));
  }
  return declared;
}

describe('the plane depth ladder', () => {
  const tokens = css('../../chrome/tokens.css');
  const sheets = {
    'live-plane.module.css': css('./live-plane.module.css'),
    'live-plane-cluster.module.css': css('./live-plane-cluster.module.css'),
    'live-match.module.css': css('./live-match.module.css'),
  };

  it('leaves no bare z-index in any of the match stylesheets', () => {
    for (const [name, sheet] of Object.entries(sheets)) {
      const bare = sheet.match(/z-index:\s*-?\d+;/g);
      expect(bare, `${name} still writes ${bare?.join(', ')}`).toBeNull();
    }
  });

  it('reads only tokens that exist', () => {
    const declared = ladder(tokens);
    for (const [name, sheet] of Object.entries(sheets)) {
      for (const [, token] of sheet.matchAll(/z-index:\s*var\(--rune-z-([a-z-]+)\)/g)) {
        expect(declared.has(token!), `${name} reads undeclared --rune-z-${token}`).toBe(true);
      }
    }
  });

  it('paints a seat’s identity over its own board, and a candidate over both', () => {
    const z = ladder(tokens);
    const rung = (name: string): number => {
      const value = z.get(name);
      expect(value, `--rune-z-${name} is not declared`).toBeDefined();
      return value!;
    };
    // The ordering decision itself, stated once. The medallion is the
    // player-targeting surface and can never degrade away (layout-model
    // §Staging), so it outranks the cards — but the reservation in
    // `plane/regions.ts` is what means it never has to prove that over a card
    // in its own row.
    expect(rung('plane-ground')).toBeLessThan(rung('plane-fixture'));
    expect(rung('plane-fixture')).toBeLessThan(rung('plane-card'));
    expect(rung('plane-card')).toBeLessThan(rung('plane-zone'));
    expect(rung('plane-zone')).toBeLessThan(rung('plane-cluster'));
    expect(rung('plane-cluster')).toBeLessThan(rung('plane-cluster-detail'));
    expect(rung('plane-cluster-detail')).toBeLessThan(rung('plane-cluster-status'));
    expect(rung('plane-cluster-status')).toBeLessThan(rung('plane-cluster-rail'));
    expect(rung('plane-cluster-rail')).toBeLessThan(rung('plane-ghost'));
    // A prompt candidate is what the server is waiting on: it pierces every
    // rung of the degradation ladder, so it pierces the depth ladder too.
    expect(rung('plane-candidate')).toBeGreaterThan(rung('plane-ghost'));
    // The host's own three layers: the staged plane, then the passive effects
    // surface, then the semantic controls, which must be reachable over
    // everything they act on.
    expect(rung('plane-stage')).toBeLessThan(rung('plane-effects'));
    expect(rung('plane-effects')).toBeLessThan(rung('plane-controls'));
    // A lit drop region sits behind the individual targets carved out of it.
    expect(rung('plane-drop-board')).toBeLessThan(rung('plane-drop-target'));
  });

  it('orders the hand band’s own fan', () => {
    const z = ladder(tokens);
    expect(z.get('hand-card')!).toBeLessThan(z.get('hand-selected')!);
    expect(z.get('hand-selected')!).toBeLessThan(z.get('hand-lifted')!);
    expect(z.get('hand-lifted')!).toBeLessThan(z.get('hand-pager')!);
  });

  it('assigns each staged slot the rung its name claims', () => {
    // The mapping a reader has to be able to check: a card is a card, a
    // medallion is a cluster, a pile is a zone. Anything reassigned here is a
    // deliberate change to the depth model and should read as one.
    const expected: [string, string][] = [
      ["[data-slot='region']", 'plane-ground'],
      ["[data-slot='handfan']", 'plane-fixture'],
      ["[data-layer='entities']", 'plane-card'],
      ["[data-slot='zone']", 'plane-zone'],
      ["[data-slot='rack']", 'plane-zone'],
      ["[data-slot='tile']", 'plane-cluster'],
      ["[data-layer='ghosts']", 'plane-ghost'],
    ];
    const sheet = sheets['live-plane.module.css'];
    for (const [selector, token] of expected) {
      const at = sheet.indexOf(`.plane ${selector} {`);
      expect(at, `${selector} has no rule`).toBeGreaterThanOrEqual(0);
      const block = /\{([^}]*)\}/.exec(sheet.slice(at))?.[1] ?? '';
      expect(block, `${selector} is not on --rune-z-${token}`).toContain(
        `z-index: var(--rune-z-${token});`,
      );
    }
    const cluster = sheets['live-plane-cluster.module.css'];
    for (const [selector, token] of [
      ["[data-slot='crest']", 'plane-cluster'],
      ["[data-slot='plate']", 'plane-zone'],
      ["[data-slot='gem']", 'plane-cluster-detail'],
      ["[data-slot='life']", 'plane-cluster-status'],
      ["[data-slot='pip']", 'plane-cluster-status'],
      ["[data-slot='chip']", 'plane-cluster-rail'],
    ] as [string, string][]) {
      const at = cluster.indexOf(`.plane ${selector} {`);
      expect(at, `${selector} has no rule`).toBeGreaterThanOrEqual(0);
      const block = /\{([^}]*)\}/.exec(cluster.slice(at))?.[1] ?? '';
      expect(block, `${selector} is not on --rune-z-${token}`).toContain(
        `z-index: var(--rune-z-${token});`,
      );
    }
  });
});
