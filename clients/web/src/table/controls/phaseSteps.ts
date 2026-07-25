/**
 * The turn's **step vocabulary**: the twelve step names, the five phase groups,
 * and the pip states derived from them (`docs/design/control-language.md` §5,
 * issue #534).
 *
 * These are the tables `table/PhaseIndicator.tsx` shipped privately. The plaque
 * replaces the indicator's *presentation* and must keep its *semantics*, so they
 * are carried here verbatim rather than re-invented — and here rather than inside
 * `PhasePlaque.tsx`, because a module that exports both a component and shared
 * constants breaks Fast Refresh (the client's lint config says so, and this
 * directory is the only place in `src/` that would have warned).
 *
 * When C6 retires the top-centre indicator, this is where its two tables already
 * live. Nothing in this file counts, remembers, or predicts: every function is a
 * lookup on the single `view.phase` value.
 */
import type { Phase } from '../../protocol';

/**
 * The readable name of each step, carried unchanged from `PhaseIndicator.tsx`.
 * The plaque's title line prints `STEP_NAME[view.phase]`.
 */
export const STEP_NAME: Record<Phase, string> = {
  untap: 'Untap',
  upkeep: 'Upkeep',
  draw: 'Draw',
  precombat_main: 'Main Phase 1',
  begin_combat: 'Begin Combat',
  declare_attackers: 'Declare Attackers',
  declare_blockers: 'Declare Blockers',
  combat_damage: 'Combat Damage',
  end_combat: 'End of Combat',
  postcombat_main: 'Main Phase 2',
  end: 'End Step',
  cleanup: 'Cleanup',
};

/** One phase group: an id, a label, and the steps it contains. */
export interface PhaseGroup {
  id: string;
  label: string;
  phases: readonly Phase[];
}

/**
 * The five phase groups the step pips render (D3), in turn order — carried
 * unchanged from `PhaseIndicator.tsx`. `main` deliberately appears twice (pre-
 * and post-combat). Membership is a fixed classification of the phase sequence:
 * it maps the single `view.phase` to where it sits and derives nothing from
 * history.
 *
 * The baselines draw four pips in panel 6 and three in situ. Those counts are
 * illustrative of the *form*; D3 renders the groups the client already
 * classifies, so the row means something a player can check against the step list
 * behind the chevron.
 */
export const PHASE_GROUPS: readonly PhaseGroup[] = [
  { id: 'beginning', label: 'Beginning', phases: ['untap', 'upkeep', 'draw'] },
  { id: 'main-1', label: 'Main', phases: ['precombat_main'] },
  {
    id: 'combat',
    label: 'Combat',
    phases: [
      'begin_combat',
      'declare_attackers',
      'declare_blockers',
      'combat_damage',
      'end_combat',
    ],
  },
  { id: 'main-2', label: 'Main', phases: ['postcombat_main'] },
  { id: 'ending', label: 'Ending', phases: ['end', 'cleanup'] },
];

/** A pip's state, in the D3 vocabulary. */
export type PipState = 'passed' | 'current' | 'upcoming';

/**
 * The pip row's states for a phase. A group counts as `passed` only once the
 * current group is known — an unclassifiable phase leaves every pip `upcoming`
 * rather than inventing progress the view did not state.
 */
export function pipStates(phase: Phase): PipState[] {
  const current = PHASE_GROUPS.findIndex((group) => group.phases.includes(phase));
  return PHASE_GROUPS.map((_, index) => {
    if (index === current) return 'current';
    return current >= 0 && index < current ? 'passed' : 'upcoming';
  });
}
