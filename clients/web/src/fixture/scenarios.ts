/**
 * Real normalized `GameView` fixtures for the Phase 1 2.5D battlefield exit.
 * Every scenario flows through `stagePlane`; no fixture supplies renderer-only
 * geometry or computes legality.
 */
import type { CardView, GameView, Permanent, PlayerId, StackItem } from '../protocol';
import { normalizeGameView } from '../wire';
import { bears, menagerie, seatTable, type PlanePermSpec } from '../table/plane.fixture';
import type { PlaneStagingState, PlaneViewport } from '../table/plane';
import type { PersistentEffect, TransientInvocation } from '../table/effects';
import { SCENE_HUES } from '../sceneTokens';

/** One authoritative frame in a fixture sequence. */
export interface FixtureFrame {
  /** Human-readable beat name. */
  label: string;
  /** Complete authoritative view. */
  view: GameView;
  /** Ephemeral one-view staging state. */
  staging?: PlaneStagingState;
  /** Declarative effects that remain live for this frame. */
  effects?: PersistentEffect[];
  /** One transient spawned as the frame becomes current. */
  transient?: TransientInvocation;
}

/** One selectable layout/motion scenario. */
export interface FixtureScenario {
  /** Stable URL/test id. */
  id: string;
  /** Selector label. */
  label: string;
  /** What this scenario proves. */
  description: string;
  /** Reference logical viewport. */
  viewport: PlaneViewport;
  /** One or more authoritative frames. */
  frames: FixtureFrame[];
}

const DESKTOP = { width: 1280, height: 800 };
const WIDE = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };
/** 21:9 ultrawide — surplus width goes to the wings, not the corridor. */
const ULTRAWIDE = { width: 2560, height: 1080 };
/** Tablet landscape at the geometry floor — full desktop staging holds. */
const TABLET = { width: 1180, height: 820 };

function card(
  id: string,
  name: string,
  typeLine: string,
  manaCost?: string,
  power?: string,
  toughness?: string,
): CardView {
  return {
    id,
    name,
    type_line: typeLine,
    mana_cost: manaCost,
    functional_id: name.toLowerCase().replaceAll(/[^a-z0-9]+/g, '_'),
    power,
    toughness,
  };
}

function hand(count: number, prefix = 'hand'): CardView[] {
  const names = [
    'Vale Sentinel',
    'Ember Adept',
    'Moonlit Archive',
    'Runebound Grove',
    'Pale Court Envoy',
    'Tideglass Drake',
    'Moss-Cloaked Guide',
    'Cinderwake Ritual',
  ];
  return Array.from({ length: count }, (_, index) => {
    const creature = index % 3 !== 2;
    return card(
      `${prefix}-${index}`,
      names[index % names.length]!,
      creature ? 'Creature — Spirit' : index % 2 === 0 ? 'Land' : 'Sorcery',
      creature ? `{${1 + (index % 4)}}{U}` : undefined,
      creature ? String(1 + (index % 5)) : undefined,
      creature ? String(2 + (index % 4)) : undefined,
    );
  });
}

function stack(count: number): StackItem[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `stack-${index}`,
    controller: `p${2 + (index % 3)}`,
    description: [
      'Echo of the Vale',
      'Emberwake Invocation',
      'Shield the Assembly',
      'Return to the Pale Court',
    ][index % 4]!,
    source: index % 2 === 0 ? `p${2 + (index % 3)}-beast-0` : undefined,
  }));
}

function namedBoard(): PlanePermSpec[] {
  return [
    { id: 'p1-warden', controller: 'p1', name: 'Runic Warden', power: '4', toughness: '5' },
    { id: 'p1-seer', controller: 'p1', name: 'Vale Seer', power: '2', toughness: '3' },
    { id: 'p1-relic', controller: 'p1', name: 'Moon Dial', type_line: 'Artifact' },
    { id: 'p1-grove', controller: 'p1', name: 'Runebound Grove', type_line: 'Basic Land — Forest' },
    { id: 'p1-isle', controller: 'p1', name: 'Tideglass Isle', type_line: 'Basic Land — Island' },
    { id: 'p2-drake', controller: 'p2', name: 'Tideglass Drake', power: '3', toughness: '4' },
    { id: 'p2-oracle', controller: 'p2', name: 'Sky Oracle', power: '2', toughness: '2' },
    { id: 'p2-lantern', controller: 'p2', name: 'Azure Lantern', type_line: 'Artifact' },
    { id: 'p2-isle', controller: 'p2', name: 'Mistbound Isle', type_line: 'Basic Land — Island' },
    { id: 'p3-giant', controller: 'p3', name: 'Ember Giant', power: '6', toughness: '5' },
    { id: 'p3-adept', controller: 'p3', name: 'Cinder Adept', power: '3', toughness: '2' },
    { id: 'p3-forge', controller: 'p3', name: 'Old Forge', type_line: 'Artifact' },
    { id: 'p3-crag', controller: 'p3', name: 'Ember Crag', type_line: 'Basic Land — Mountain' },
    { id: 'p4-envoy', controller: 'p4', name: 'Pale Court Envoy', power: '3', toughness: '3' },
    { id: 'p4-guard', controller: 'p4', name: 'Marble Guard', power: '2', toughness: '5' },
    { id: 'p4-oath', controller: 'p4', name: 'Court Oath', type_line: 'Enchantment' },
    { id: 'p4-field', controller: 'p4', name: 'Pale Field', type_line: 'Basic Land — Plains' },
  ];
}

function furnish(
  base: GameView,
  options: {
    hand?: CardView[];
    stack?: StackItem[];
    names?: Record<PlayerId, string>;
    phase?: GameView['phase'];
    priority?: PlayerId;
  } = {},
): GameView {
  return {
    ...base,
    me: { life: 40, library_size: 61 },
    my_hand: options.hand ?? hand(7),
    stack: options.stack ?? [],
    phase: options.phase ?? base.phase,
    priority_player: options.priority ?? base.priority_player,
    player_names: options.names ?? {
      p1: 'Aria',
      p2: 'Bram',
      p3: 'Cyra',
      p4: 'Dain',
      p5: 'Elowen',
      p6: 'Fenn',
    },
    opponents: base.opponents.map((opponent, index) => ({
      ...opponent,
      life: 40 - index * 3,
      library_size: 58 - index * 2,
      graveyard_size: index,
    })),
  };
}

function commanderView(perms: PlanePermSpec[] = namedBoard()): GameView {
  return furnish(
    seatTable({
      opponents: 3,
      perms,
      active: 'p2',
      validActions: [
        { id: 'fixture-pass', type: 'pass_priority', label: 'Pass priority' },
        {
          id: 'fixture-activate',
          type: 'activate_ability',
          label: 'Activate Vale Seer',
          subject: ['p1-seer'],
        },
      ],
    }),
    { priority: 'p1' },
  );
}

function updatePermanent(view: GameView, id: string, update: Partial<Permanent>): GameView {
  return {
    ...view,
    battlefield: view.battlefield.map((permanent) =>
      permanent.id === id ? { ...permanent, ...update } : permanent,
    ),
  };
}

function commanderSequence(): FixtureFrame[] {
  const opening = commanderView();
  const drew = { ...opening, my_hand: [...opening.my_hand, ...hand(1, 'drawn')] };
  const playedSpec: PlanePermSpec = {
    id: 'p1-sentinel',
    controller: 'p1',
    name: 'Vale Sentinel',
    power: '4',
    toughness: '4',
  };
  const played = {
    ...commanderView([...namedBoard(), playedSpec]),
    my_hand: drew.my_hand.filter((entry) => entry.id !== 'drawn-0'),
  };
  const tapped = updatePermanent(played, 'p1-sentinel', { tapped: true });
  const combat = {
    ...updatePermanent(
      updatePermanent(tapped, 'p1-warden', {
        attacking: true,
        attacking_player: 'p3',
        tapped: true,
      }),
      'p2-drake',
      { attacking: true, attacking_player: 'p4', tapped: true },
    ),
    phase: 'declare_blockers' as const,
  };
  const blocked = updatePermanent(
    updatePermanent(combat, 'p3-adept', { blocking: 'p1-warden' }),
    'p4-guard',
    { blocking: 'p2-drake' },
  );
  const resolved = {
    ...blocked,
    stack: [],
    battlefield: blocked.battlefield.filter((permanent) => permanent.id !== 'p3-adept'),
    phase: 'postcombat_main' as const,
  };
  return [
    { label: 'Opening composition', view: opening },
    { label: 'Draw to hand', view: drew },
    { label: 'Play to battlefield', view: played },
    { label: 'Tap and settle', view: tapped },
    { label: 'Focus Ember Reach', view: tapped, staging: { focusSeat: 'p3' } },
    {
      label: 'Combat paths',
      view: blocked,
      staging: { focusSeat: 'p3' },
      effects: [
        {
          id: 'attack:p1-warden',
          category: 'attack-path',
          from: { ref: 'p1-warden' },
          to: { ref: 'seat:p3' },
          accent: SCENE_HUES.orange.value,
        },
        {
          id: 'attack:p2-drake',
          category: 'attack-path',
          from: { ref: 'p2-drake' },
          to: { ref: 'seat:p4' },
          accent: SCENE_HUES.orange.value,
        },
        {
          id: 'block:p3-adept',
          category: 'blocker-link',
          from: { ref: 'p3-adept' },
          to: { ref: 'p1-warden' },
          accent: SCENE_HUES.orange.value,
        },
      ],
    },
    {
      label: 'Resolution and impact',
      view: resolved,
      staging: { focusSeat: 'p3' },
      transient: {
        category: 'resolution',
        target: { ref: 'p1-warden' },
        accent: SCENE_HUES.red.value,
        magnitude: 1.2,
      },
    },
  ];
}

function combatSpecs(): PlanePermSpec[] {
  const attackers = menagerie('p1', 8).map((entry, index) => ({
    ...entry,
    attacking: true,
    attacking_player: index % 2 === 0 ? 'p3' : 'p4',
  }));
  const blockers = menagerie('p3', 6).map((entry, index) => ({
    ...entry,
    blocking: attackers[index % attackers.length]!.id,
  }));
  return [...attackers, ...blockers, ...menagerie('p2', 4), ...menagerie('p4', 4)];
}

function combatEffects(view: GameView): PersistentEffect[] {
  const effects: PersistentEffect[] = [];
  for (const permanent of view.battlefield) {
    if (permanent.attacking_player) {
      effects.push({
        id: `attack:${permanent.id}`,
        category: 'attack-path',
        from: { ref: permanent.id },
        to: { ref: `seat:${permanent.attacking_player}` },
        accent: SCENE_HUES.orange.value,
      });
    }
    if (permanent.blocking) {
      effects.push({
        id: `block:${permanent.id}`,
        category: 'blocker-link',
        from: { ref: permanent.id },
        to: { ref: permanent.blocking },
        accent: SCENE_HUES.orange.value,
      });
    }
  }
  return effects;
}

const duel = furnish(
  seatTable({
    opponents: 1,
    perms: [...menagerie('p1', 7), ...menagerie('p2', 8), ...bears('p1', 5)],
  }),
);
const six = furnish(
  seatTable({
    opponents: 5,
    perms: Array.from({ length: 6 }, (_, index) => menagerie(`p${index + 1}`, 8)).flat(),
    active: 'p5',
  }),
  { priority: 'p5' },
);
const tokens = furnish(
  seatTable({
    opponents: 3,
    perms: [
      ...bears('p1', 60, { prefix: 'local-token' }),
      ...bears('p2', 45, { prefix: 'far-token' }),
      ...bears('p3', 30, { prefix: 'left-token' }),
      ...bears('p4', 25, { prefix: 'right-token' }),
    ],
  }),
);
const three = furnish(
  seatTable({
    opponents: 2,
    perms: [...menagerie('p1', 5), ...menagerie('p2', 6), ...menagerie('p3', 4)],
    active: 'p2',
  }),
  { priority: 'p1' },
);
const five = furnish(
  seatTable({
    opponents: 4,
    perms: [
      ...menagerie('p1', 5),
      ...menagerie('p2', 6),
      ...menagerie('p3', 4),
      ...menagerie('p4', 3),
      ...menagerie('p5', 5),
    ],
    active: 'p2',
  }),
  { priority: 'p1' },
);
const ultrawide = furnish(
  seatTable({
    opponents: 5,
    perms: Array.from({ length: 6 }, (_, index) => menagerie(`p${index + 1}`, 6)).flat(),
    active: 'p3',
  }),
  { priority: 'p3' },
);
const tablet = furnish(seatTable({ opponents: 3, perms: namedBoard(), active: 'p2' }), {
  priority: 'p1',
});
const bigHand = { ...commanderView(), my_hand: hand(16, 'wide-hand') };
const combat = furnish(seatTable({ opponents: 3, perms: combatSpecs(), active: 'p1' }), {
  phase: 'declare_blockers',
  priority: 'p1',
});
const deepStack = furnish(
  seatTable({ opponents: 3, perms: [...menagerie('p1', 4), ...menagerie('p2', 4)] }),
  { stack: stack(8), phase: 'precombat_main', priority: 'p3' },
);
const phone = furnish(
  seatTable({
    opponents: 3,
    perms: Array.from({ length: 4 }, (_, index) => menagerie(`p${index + 1}`, 6)).flat(),
    active: 'p4',
  }),
  { stack: stack(3), priority: 'p4' },
);

/** The complete layout-model scenario set plus the primary Commander sequence. */
export const FIXTURE_SCENARIOS: readonly FixtureScenario[] = [
  {
    id: 'commander4',
    label: 'Commander — four players',
    description: 'Primary baseline composition and the complete Phase 1 motion sequence.',
    viewport: WIDE,
    frames: commanderSequence(),
  },
  {
    id: 'duel',
    label: 'Duel',
    description: 'Full-width far side with no multiplayer focus concept.',
    viewport: DESKTOP,
    frames: [{ label: 'Duel board', view: duel }],
  },
  {
    id: 'three',
    label: 'Three players',
    description: 'Focused far side plus one full-board wing on a single side.',
    viewport: WIDE,
    frames: [{ label: 'Three-seat stage', view: three, staging: { focusSeat: 'p2' } }],
  },
  {
    id: 'five',
    label: 'Five players',
    description: 'Focused far side plus the 2-left / 1-right digest wing split.',
    viewport: WIDE,
    frames: [{ label: 'Five-seat 2+1 split', view: five, staging: { focusSeat: 'p2' } }],
  },
  {
    id: 'six',
    label: 'Six players',
    description: 'Focused far side plus two digest wings per side.',
    viewport: WIDE,
    frames: [{ label: 'Six-seat digest', view: six, staging: { focusSeat: 'p5' } }],
  },
  {
    id: 'ultrawide',
    label: 'Ultrawide 21:9',
    description: 'Surplus width spent on the wings; the corridor stays capped.',
    viewport: ULTRAWIDE,
    frames: [{ label: 'Ultrawide six-seat', view: ultrawide, staging: { focusSeat: 'p3' } }],
  },
  {
    id: 'tablet',
    label: 'Tablet landscape',
    description: 'Full desktop four-player staging held at the 1180×820 floor.',
    viewport: TABLET,
    frames: [{ label: 'Tablet four-player', view: tablet, staging: { focusSeat: 'p2' } }],
  },
  {
    id: 'tokens',
    label: 'Token wall',
    description: '160 permanents exercising independent folding and wrapping ladders.',
    viewport: WIDE,
    frames: [{ label: 'Token stress', view: tokens }],
  },
  {
    id: 'big-hand',
    label: 'Big hand',
    description: 'Sixteen-card screen-space hand fan over the staged plane.',
    viewport: WIDE,
    frames: [{ label: 'Sixteen-card hand', view: bigHand }],
  },
  {
    id: 'combat-web',
    label: 'Combat web',
    description: 'Multi-defender attack paths and doubled blocker links.',
    viewport: WIDE,
    frames: [
      {
        label: 'Split attack',
        view: combat,
        staging: { focusSeat: 'p3' },
        effects: combatEffects(combat),
      },
    ],
  },
  {
    id: 'deep-stack',
    label: 'Deep stack',
    description: 'Eight-deep mixed stack rail beside a four-player board.',
    viewport: WIDE,
    frames: [{ label: 'Eight stack objects', view: deepStack }],
  },
  {
    id: 'phone',
    label: 'Phone portrait',
    description: 'Summary tiles, one focused board, receiver band, and stack sheet.',
    viewport: PHONE,
    frames: [{ label: 'Compact four-player', view: phone, staging: { focusSeat: 'p3' } }],
  },
];

/** Find a scenario by URL/test id, falling back to the primary Commander fixture. */
export function fixtureScenario(id: string | null | undefined): FixtureScenario {
  return FIXTURE_SCENARIOS.find((scenario) => scenario.id === id) ?? FIXTURE_SCENARIOS[0]!;
}

/**
 * Normalize a fixture again through the wire boundary. Tests use this to prove
 * every scenario is a complete reconstructable `GameView`, not renderer state.
 */
export function normalizeFixture(view: GameView): GameView {
  return normalizeGameView(JSON.parse(JSON.stringify(view)) as unknown);
}
