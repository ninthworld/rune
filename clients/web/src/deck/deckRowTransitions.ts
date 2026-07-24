/**
 * Running-deck row transitions (issue #508, motion grammar §8) for the deck
 * builder. The deck list is keyed by card identity, so React reuses one `<li>`
 * for every copy of a card — which means the CSS mount animation fires only for a
 * card's *first* copy, and a card leaving the deck unmounts with no exit. This hook
 * closes both gaps purely with scene data, no game logic:
 *
 * - **Enter** — a brand-new card row mounts and rides the CSS zone-travel class.
 * - **Copy-count change** — a persisting row carries a monotonic `changeSeq` that
 *   ticks each time its copy count moves, so the view can re-trigger the count
 *   animation on a row React would otherwise leave untouched.
 * - **Leave** — a removed row is held in the render list, marked `leaving`, for one
 *   zone-travel duration so its exit can play, then dropped. Under reduced motion
 *   (`durationMs === 0`) the row is dropped immediately — the carried snap contract.
 *
 * The merge itself ({@link mergeDeckRows}) is a pure function so the enter / count
 * change / leave distinctions are unit-testable without a DOM or timers.
 */
import { useEffect, useRef, useState } from 'react';

/** The source-of-truth shape a running-deck row is derived from (counts → rows). */
export interface DeckRowModel {
  id: string;
  name: string;
  count: number;
  isCommanderCandidate: boolean;
}

/** A row as rendered: the model plus its transition phase and count-change tick. */
export interface DeckRowRender extends DeckRowModel {
  /** `present` rows are in the deck; `leaving` rows were removed and are animating out. */
  phase: 'present' | 'leaving';
  /**
   * Increments every time a *persisting* row's copy count changes. `0` on the row's
   * first mount (an enter, not a count change), so a view can tell the two apart and
   * re-trigger the count animation only on a genuine copy add/remove.
   */
  changeSeq: number;
}

/**
 * Fold the next source-of-truth models into the current render list, preserving
 * order and deciding each row's phase. Pure — the hook wraps it with timers.
 *
 * @param keepLeaving when false (reduced motion), removed rows are dropped outright
 *   instead of held as `leaving`, so the snap contract needs no exit animation.
 */
export function mergeDeckRows(
  current: readonly DeckRowRender[],
  models: readonly DeckRowModel[],
  keepLeaving: boolean,
): { rows: DeckRowRender[]; leaving: string[] } {
  const modelById = new Map(models.map((m) => [m.id, m]));
  const placed = new Set<string>();
  const rows: DeckRowRender[] = [];
  const leaving: string[] = [];

  // Walk the current order first so persisting and leaving rows hold their place.
  for (const cur of current) {
    const model = modelById.get(cur.id);
    if (model) {
      const changed = model.count !== cur.count;
      rows.push({
        ...model,
        phase: 'present',
        changeSeq: cur.phase === 'leaving' || changed ? cur.changeSeq + 1 : cur.changeSeq,
      });
      placed.add(cur.id);
    } else if (cur.phase === 'present' && keepLeaving) {
      rows.push({ ...cur, phase: 'leaving' });
      leaving.push(cur.id);
      placed.add(cur.id);
    } else if (cur.phase === 'leaving') {
      // Still animating out from an earlier removal — keep it until its timer fires.
      rows.push(cur);
      placed.add(cur.id);
    }
    // else: a present row removed under reduced motion — dropped outright.
  }

  // Append genuinely new cards in model order — these mount and play the enter class.
  for (const model of models) {
    if (!placed.has(model.id)) rows.push({ ...model, phase: 'present', changeSeq: 0 });
  }

  return { rows, leaving };
}

/**
 * React binding over {@link mergeDeckRows}: holds the render list in state, ticks
 * `changeSeq` on copy changes, and schedules each leaving row's removal after
 * `durationMs`. Pass `sceneMotionMs('zoneTravel', reducedMotion)` so the exit
 * collapses to an immediate drop under reduced motion.
 */
export function useDeckRowTransitions(
  models: readonly DeckRowModel[],
  durationMs: number,
): DeckRowRender[] {
  const [rows, setRows] = useState<DeckRowRender[]>(() =>
    models.map((m) => ({ ...m, phase: 'present', changeSeq: 0 })),
  );
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  const keepLeaving = durationMs > 0;

  // A content signature so the fold runs on real model changes, not every render
  // (models is a fresh array each time). Order matters, so it is part of the key.
  const signature = models
    .map((m) => `${m.id}:${m.count}:${m.isCommanderCandidate ? 1 : 0}`)
    .join('|');

  useEffect(() => {
    setRows((prev) => {
      const { rows: next, leaving } = mergeDeckRows(prev, models, keepLeaving);
      for (const id of leaving) {
        const existing = timers.current.get(id);
        if (existing) clearTimeout(existing);
        timers.current.set(
          id,
          setTimeout(() => {
            timers.current.delete(id);
            setRows((live) => live.filter((row) => !(row.id === id && row.phase === 'leaving')));
          }, durationMs),
        );
      }
      // A row that came back before its exit finished keeps its slot — cancel its timer.
      for (const row of next) {
        if (row.phase === 'present' && timers.current.has(row.id)) {
          clearTimeout(timers.current.get(row.id));
          timers.current.delete(row.id);
        }
      }
      return next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [signature, durationMs]);

  useEffect(() => {
    const map = timers.current;
    return () => {
      for (const timer of map.values()) clearTimeout(timer);
      map.clear();
    };
  }, []);

  return rows;
}
