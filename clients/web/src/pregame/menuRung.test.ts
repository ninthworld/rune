/**
 * The menu rung actually reaches the surfaces that read it
 * (`control-language.md` §3.4, issue #566).
 *
 * ## The two bugs this exists to catch
 *
 * **A scale multiplier cannot be derived from the viewport.** The obvious rung
 * is a unitless `clamp(1, calc(100vmin / 620), 1.6)` that every §3.1 length
 * multiplies by. `100vmin / 620` is a *length*, so the clamp mixes a length
 * with two numbers and the declaration is invalid — dropped at computed-value
 * time, taking every property that multiplied by it down with it. That shipped
 * in the first cut of this work: in a browser the pregame's type ramp, its
 * plaque widths, and its primary control all fell back to `auto`/inherited. So
 * the rung is a `clamp()` of **lengths**, one per §3.1 token, and this file
 * recomputes every one of them from the value it restates.
 *
 * **A derived custom property freezes at its declaring scope.** A `var()`
 * inside a custom property is substituted at computed-value time on the element
 * the property is declared on, and it is that already-substituted value which
 * inherits. So `:root { --w: calc(var(--base) * var(--scale)) }` cannot see a
 * `--scale` a descendant overrides. That was the second cut. The rung avoids it
 * entirely by re-pointing the §3.1 tokens themselves, which are read by ordinary
 * properties on descendants and therefore resolve per element — but the model
 * below still encodes the rule, so a future derived token cannot reintroduce it.
 *
 * A string search for the token names sees neither failure: in both, every
 * declaration was present and spelled right. So this file **resolves the
 * stylesheets under the substitution model** — `:root`, then each rung scope,
 * following `composes` — and asserts the numbers a scope actually lays out from.
 *
 * ## What it is not
 *
 * It is a model of CSS custom-property resolution, not a browser. It covers
 * substitution scope, inheritance, and the `clamp`/`calc`/`min`/`max`
 * arithmetic these tokens use. It cannot see an *invalid* declaration — the
 * first bug above — which is why the clamp forms were checked in a real browser
 * and why what is painted stays the maintainer's check under the repo's testing
 * policy.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { CONTROL, MENU_RUNG } from '../table/controls/controlTokens';

function css(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), 'utf8');
}

/** The stylesheets a pregame surface resolves its custom properties through. */
const SHEETS: Record<string, string> = {
  'tokens.css': css('../chrome/tokens.css'),
  'pregame.module.css': css('./pregame.module.css'),
  'pregamePlaces.module.css': css('./pregamePlaces.module.css'),
};

/** A scope's custom properties: name → value, unsubstituted. */
type Vars = Map<string, string>;

/** A viewport, for the `vmin` in the rung clamps. */
interface Viewport {
  width: number;
  height: number;
}

function stripComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, '');
}

/** One rule's declaration block, brace-balanced, comments removed. */
function ruleBody(sheet: string, selector: string): string {
  const clean = stripComments(SHEETS[sheet]!);
  const at = clean.indexOf(`${selector} {`);
  if (at < 0) throw new Error(`${sheet} has no rule for ${selector}`);
  const start = clean.indexOf('{', at);
  let depth = 0;
  for (let i = start; i < clean.length; i += 1) {
    if (clean[i] === '{') depth += 1;
    else if (clean[i] === '}') {
      depth -= 1;
      if (depth === 0) return clean.slice(start + 1, i);
    }
  }
  throw new Error(`${sheet}: unterminated rule for ${selector}`);
}

/** Split a declaration block on top-level `;`. */
function declarations(body: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of body) {
    if (ch === '(') depth += 1;
    else if (ch === ')') depth -= 1;
    if (ch === ';' && depth === 0) {
      out.push(current);
      current = '';
    } else current += ch;
  }
  out.push(current);
  return out.map((decl) => decl.trim()).filter((decl) => decl.length > 0);
}

/** The custom properties a rule declares, in source order. */
function customProperties(body: string): Vars {
  const out: Vars = new Map();
  for (const decl of declarations(body)) {
    const colon = decl.indexOf(':');
    if (colon < 0) continue;
    const name = decl.slice(0, colon).trim();
    if (name.startsWith('--')) out.set(name, decl.slice(colon + 1).trim());
  }
  return out;
}

/**
 * A rule plus everything it `composes`, innermost first — the class list an
 * element actually carries. CSS Modules merges class names onto one element, so
 * a composed rule's declarations resolve against **that element's** cascaded
 * values, which is precisely the mechanism the fix relies on.
 */
function withComposed(sheet: string, selector: string): { sheet: string; selector: string }[] {
  const body = ruleBody(sheet, selector);
  const composed: { sheet: string; selector: string }[] = [];
  for (const decl of declarations(body)) {
    const match = /^composes:\s*([\w-]+)(?:\s+from\s+'\.\/([\w.-]+)')?$/.exec(decl);
    if (match) composed.push(...withComposed(match[2] ?? sheet, `.${match[1]!}`));
  }
  return [...composed, { sheet, selector }];
}

/** Substitute every `var()` in a value against an element's own cascaded map. */
function substitute(value: string, vars: Vars, seen: ReadonlySet<string>): string {
  return value.replace(/var\(\s*(--[\w-]+)\s*\)/g, (_match, name: string) => {
    if (seen.has(name)) throw new Error(`custom-property cycle at ${name}`);
    const raw = vars.get(name);
    if (raw === undefined) throw new Error(`unresolved ${name}`);
    return substitute(raw, vars, new Set([...seen, name]));
  });
}

/**
 * The resolved custom properties of one element: everything it inherits, then
 * everything its own rules declare, then substitution against that merged map.
 * Inherited entries arrive already substituted and pass through untouched —
 * which is exactly why a derived token declared on an ancestor cannot see a
 * scale the descendant overrode.
 */
function resolveScope(parent: Vars, rules: { sheet: string; selector: string }[]): Vars {
  const declared: Vars = new Map();
  for (const rule of rules) {
    for (const [name, value] of customProperties(ruleBody(rule.sheet, rule.selector))) {
      declared.set(name, value);
    }
  }
  const merged: Vars = new Map([...parent, ...declared]);
  const resolved: Vars = new Map();
  for (const [name, value] of merged) {
    resolved.set(name, substitute(value, merged, new Set([name])));
  }
  return resolved;
}

/** Evaluate a fully substituted length/number to px. Throws on anything the
 * rung tokens do not use, so an unsupported form fails loudly. */
function evaluate(expr: string, viewport: Viewport): number {
  const tokens = (expr.match(/[a-z]+\(|\(|\)|,|[-+*/]|[\d.]+[a-z%]*/gi) ?? []).map((t) => t.trim());
  let at = 0;
  const peek = (): string | undefined => tokens[at];
  const take = (): string => {
    const token = tokens[at];
    if (token === undefined) throw new Error(`unexpected end of "${expr}"`);
    at += 1;
    return token;
  };
  const expect = (token: string): void => {
    if (take() !== token) throw new Error(`expected "${token}" in "${expr}"`);
  };
  const literal = (token: string): number => {
    const parsed = /^([\d.]+)([a-z%]*)$/i.exec(token);
    if (!parsed) throw new Error(`not a number: "${token}" in "${expr}"`);
    const value = Number(parsed[1]);
    switch (parsed[2]!.toLowerCase()) {
      case '':
      case 'px':
        return value;
      case 'vmin':
        return (value / 100) * Math.min(viewport.width, viewport.height);
      case 'vmax':
        return (value / 100) * Math.max(viewport.width, viewport.height);
      case 'vw':
        return (value / 100) * viewport.width;
      case 'vh':
        return (value / 100) * viewport.height;
      default:
        throw new Error(`unsupported unit "${parsed[2]}" in "${expr}"`);
    }
  };
  function primary(): number {
    const token = take();
    if (token === '(') {
      const value = sum();
      expect(')');
      return value;
    }
    if (token === '-') return -primary();
    if (token.endsWith('(')) {
      const fn = token.slice(0, -1).toLowerCase();
      const args = [sum()];
      while (peek() === ',') {
        take();
        args.push(sum());
      }
      expect(')');
      if (fn === 'calc') return args[0]!;
      if (fn === 'min') return Math.min(...args);
      if (fn === 'max') return Math.max(...args);
      if (fn === 'clamp') return Math.max(args[0]!, Math.min(args[1]!, args[2]!));
      throw new Error(`unsupported function "${fn}" in "${expr}"`);
    }
    return literal(token);
  }
  function product(): number {
    let value = primary();
    while (peek() === '*' || peek() === '/') {
      const op = take();
      const rhs = primary();
      value = op === '*' ? value * rhs : value / rhs;
    }
    return value;
  }
  function sum(): number {
    let value = product();
    while (peek() === '+' || peek() === '-') {
      const op = take();
      const rhs = product();
      value = op === '+' ? value + rhs : value - rhs;
    }
    return value;
  }
  const result = sum();
  if (at !== tokens.length) throw new Error(`trailing tokens in "${expr}"`);
  return result;
}

/** The `:root` scope. */
function rootScope(): Vars {
  return resolveScope(new Map(), [{ sheet: 'tokens.css', selector: ':root' }]);
}

/** A pregame scope's resolved properties, `composes` followed. */
function scopeOf(sheet: string, selector: string, parent: Vars = rootScope()): Vars {
  return resolveScope(parent, withComposed(sheet, selector));
}

/** A resolved property as a number at a viewport. */
function value(vars: Vars, name: string, viewport: Viewport): number {
  const raw = vars.get(name);
  if (raw === undefined) throw new Error(`${name} is not declared in this scope`);
  return evaluate(raw, viewport);
}

/** The reference geometries §3.4's rungs are stated against. */
const REFERENCES: Viewport[] = [
  { width: 390, height: 844 },
  { width: 1180, height: 820 },
  { width: 1280, height: 800 },
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
  { width: 2560, height: 1440 },
];

/**
 * Every §3.1 token a menu rung restates, with the value it restates. These are
 * the numbers `control-language.md` §3.1/§2.2 fixes; a rung may make one fluid
 * and may never make it a different number.
 */
const SCALED: [token: string, base: number][] = [
  ['--rune-control-h-primary', CONTROL.hPrimary],
  ['--rune-control-h', CONTROL.h],
  ['--rune-control-hit', CONTROL.hit],
  ['--rune-control-w-cluster', CONTROL.wCluster],
  ['--rune-control-w-pair', CONTROL.wPair],
  ['--rune-cluster-margin', CONTROL.clusterMargin],
  ['--rune-cluster-gap', CONTROL.clusterGap],
  ['--rune-touch', 44],
  ['--rune-type-display', 30],
  ['--rune-type-title', 20],
  ['--rune-type-heading', 16],
  ['--rune-type-body-lg', 14],
  ['--rune-type-body', 13],
  ['--rune-type-caption', 12],
  ['--rune-type-micro', 11],
  ['--rune-type-action', 24],
];

/** The rung a scope takes, and the class in `pregame.module.css` that sets it. */
const SCOPES = [
  { rung: 'open' as const, sheet: 'pregame.module.css', selector: '.stage', via: '.rungOpen' },
  {
    rung: 'dense' as const,
    sheet: 'pregamePlaces.module.css',
    selector: '.ring',
    via: '.rungDense',
  },
];

describe('the resolver itself', () => {
  it('models substitution at the DECLARING scope, which was the second bug', () => {
    // A hand-built pair that reproduces it, so a reader can check the model is
    // the CSS one and not a convenient reading of it.
    const resolve = (parent: Vars, own: [string, string][]): Vars => {
      const merged: Vars = new Map([...parent, ...own]);
      return new Map(
        [...merged].map(([name, val]) => [name, substitute(val, merged, new Set([name]))]),
      );
    };
    const nowhere = { width: 0, height: 0 };
    const ancestor = resolve(new Map(), [
      ['--base', '100px'],
      ['--scale', '1'],
      ['--derived', 'calc(var(--base) * var(--scale))'],
    ]);
    expect(evaluate(ancestor.get('--derived')!, nowhere)).toBe(100);

    // A descendant that only overrides the scale inherits the ANCESTOR's
    // already-substituted `--derived`, so it does not move.
    const overrideOnly = resolve(ancestor, [['--scale', '2']]);
    expect(evaluate(overrideOnly.get('--derived')!, nowhere)).toBe(100);

    // Re-pointing the token the consumer actually reads always works, because
    // an ordinary property resolves on the element that has the override. That
    // is the shape the rung uses.
    const repointed = resolve(ancestor, [['--base', '200px']]);
    expect(evaluate(repointed.get('--base')!, nowhere)).toBe(200);
  });

  it('evaluates the arithmetic the rung tokens use', () => {
    const vp = { width: 1440, height: 900 };
    expect(evaluate('clamp(268px, 43.2258vmin, 428.8px)', vp)).toBeCloseTo(389.03, 2);
    expect(evaluate('clamp(268px, 43.2258vmin, 428.8px)', { width: 390, height: 844 })).toBe(268);
    expect(evaluate('clamp(268px, 43.2258vmin, 428.8px)', { width: 3840, height: 2160 })).toBe(
      428.8,
    );
    expect(evaluate('calc(268px * 2.4)', vp)).toBeCloseTo(643.2, 10);
    expect(evaluate('min(100px, 20px, 50px)', vp)).toBe(20);
    expect(() => evaluate('calc(1em * 2)', vp)).toThrow(/unsupported unit/);
  });
});

describe('the menu rung reaches the surfaces that read it', () => {
  it('restates each §3.1 token as a clamp derived from that token, not a new number', () => {
    // The rung is arithmetic on §3.1, so every clamp is recomputed here rather
    // than transcribed: a token whose floor is not its own §3.1 value, or whose
    // fluid term does not match its rung's basis, is a second size vocabulary.
    const root = rootScope();
    for (const { rung } of SCOPES) {
      const { basis, max } = MENU_RUNG[rung];
      const prefix = rung === 'dense' ? 'dense-' : '';
      for (const [token, base] of SCALED) {
        const name = `--rune-menu-${prefix}${token.replace('--rune-', '')}`;
        const declared = root.get(name);
        expect(declared, `${name} is not declared`).toBeDefined();
        for (const viewport of REFERENCES) {
          const fluid = (base / basis) * Math.min(viewport.width, viewport.height);
          expect(
            evaluate(declared!, viewport),
            `${name} at ${viewport.width}×${viewport.height}`,
          ).toBeCloseTo(Math.max(base, Math.min(fluid, base * max)), 2);
        }
      }
    }
  });

  it('never draws a menu control smaller than the match control', () => {
    // The floor of every rung is the §3.1 value itself, so a small or zoomed
    // viewport draws the menus at match scale and never under it — which is
    // what keeps the 44px anchor D1 pins from being undercut.
    const root = rootScope();
    for (const { rung } of SCOPES) {
      const prefix = rung === 'dense' ? 'dense-' : '';
      for (const [token, base] of SCALED) {
        const name = `--rune-menu-${prefix}${token.replace('--rune-', '')}`;
        for (const viewport of REFERENCES) {
          expect(evaluate(root.get(name)!, viewport), name).toBeGreaterThanOrEqual(base);
        }
      }
    }
  });

  it('re-points every scaled token in each rung scope', () => {
    // The forget-proofing: a scope on a rung that misses a token would draw
    // that one length at match scale beside fifteen that grew.
    for (const { rung, sheet, selector } of SCOPES) {
      const scope = scopeOf(sheet, selector);
      const { basis, max } = MENU_RUNG[rung];
      for (const viewport of REFERENCES) {
        for (const [token, base] of SCALED) {
          const fluid = (base / basis) * Math.min(viewport.width, viewport.height);
          expect(
            value(scope, token, viewport),
            `${selector} draws ${token} off its rung at ${viewport.width}×${viewport.height}`,
          ).toBeCloseTo(Math.max(base, Math.min(fluid, base * max)), 2);
        }
      }
    }
  });

  it('leaves the match on §3.1 exactly', () => {
    // `:root` is the match. Nothing there may be fluid, or the rung has leaked
    // into an arena that is full of cards.
    const root = rootScope();
    for (const [token, base] of SCALED) {
      expect(evaluate(root.get(token)!, { width: 3840, height: 2160 }), token).toBe(base);
    }
  });

  it('puts the ring on the DENSE rung, inside a stage already on the open one', () => {
    // The override that matters: the ring is a descendant of the stage, and if
    // its re-pointing did not win, its seats would take the open rung and
    // outgrow the box that has to hold them.
    for (const viewport of REFERENCES) {
      const stage = scopeOf('pregame.module.css', '.stage');
      const ring = scopeOf('pregamePlaces.module.css', '.ring', stage);
      const open = value(stage, '--rune-control-w-cluster', viewport);
      const dense = value(ring, '--rune-control-w-cluster', viewport);
      expect(dense).toBeLessThanOrEqual(open);
      expect(dense).toBeCloseTo(
        Math.max(
          CONTROL.wCluster,
          Math.min(
            (CONTROL.wCluster / MENU_RUNG.dense.basis) * Math.min(viewport.width, viewport.height),
            CONTROL.wCluster * MENU_RUNG.dense.max,
          ),
        ),
        2,
      );
    }
    // …and strictly smaller wherever the open rung has actually grown.
    const wide = { width: 1920, height: 1080 };
    const stage = scopeOf('pregame.module.css', '.stage');
    const ring = scopeOf('pregamePlaces.module.css', '.ring', stage);
    expect(value(ring, '--rune-control-w-cluster', wide)).toBeLessThan(
      value(stage, '--rune-control-w-cluster', wide),
    );
  });

  it('computes the ring’s collision floor from the ring’s own cluster width', () => {
    // The floor is the left-hand side of the inequality that keeps a seat and
    // the centre plaque apart, and a seat's height moves with the rung. A floor
    // computed from the unscaled 268px is the bug wearing a passing test.
    const body = stripComments(SHEETS['pregamePlaces.module.css']!);
    const rule = /\.ring \{([^}]*)\}/.exec(body)?.[1] ?? '';
    const floor = /min-height:\s*([^;]+);/.exec(rule)?.[1]?.trim();
    expect(floor, '.ring states no min-height').toBeDefined();
    const stage = scopeOf('pregame.module.css', '.stage');
    const ring = scopeOf('pregamePlaces.module.css', '.ring', stage);
    for (const viewport of REFERENCES) {
      expect(evaluate(substitute(floor!, ring, new Set()), viewport)).toBeCloseTo(
        value(ring, '--rune-control-w-cluster', viewport) * 2.4,
        6,
      );
    }
  });

  it('discovers the rung scopes rather than trusting a list', () => {
    // A third scope added without composing a rung would take its ancestor's,
    // silently. The scopes above are the ones the stylesheets actually declare.
    const found: string[] = [];
    for (const sheet of ['pregame.module.css', 'pregamePlaces.module.css']) {
      const clean = stripComments(SHEETS[sheet]!);
      for (const [, , selector] of clean.matchAll(/(^|\n)(\.[\w-]+) \{/g)) {
        const body = ruleBody(sheet, selector!);
        if (!/composes:\s*rung/.test(body)) continue;
        found.push(`${sheet}${selector}`);
      }
    }
    expect(found).toEqual(['pregame.module.css.stage', 'pregamePlaces.module.css.ring']);
    expect(SCOPES.map((s) => `${s.sheet}${s.selector}`)).toEqual(found);
  });
});
