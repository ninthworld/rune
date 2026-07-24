/**
 * Pure, presentation-only derivations over a {@link GameView} (and its scene) used
 * by the {@link Table} composition root. Every function here reads data the view
 * already carries and derives nothing about rules, legality, cost, or effect — the
 * client stays a pure renderer of `valid_actions[]` (AGENTS.md hard rule). Kept
 * separate from the component so the table body stays focused on wiring.
 */
import type { EntityId, GameView, PlayerId } from '../protocol';
import type { InspectTarget } from './CardInspect';
import type { Binding } from './ShortcutHelp';
import type { RenderedCard, TableScene } from './scene';

/**
 * The opponents (by id) whose *battlefield* carries an offered card-level
 * interaction in the current view (issue #400): a permanent that is the subject of
 * a `valid_action` or a candidate of one of its requirement slots. On the
 * phone-portrait summary-tile composition these boards must stay reachable, so the
 * first such opponent is expanded automatically — the collapse never hides an
 * offered action. Derived purely from the view (candidates on the receiver's own
 * board, hand, or non-board zones are handled elsewhere and excluded here).
 */
export function opponentsWithBoardCandidates(view: GameView): Set<PlayerId> {
  const offered = new Set<EntityId>();
  for (const action of view.valid_actions) {
    for (const subjectId of action.subject ?? []) offered.add(subjectId);
    for (const req of action.requirements ?? []) {
      for (const candidate of req.candidates ?? []) offered.add(candidate);
    }
  }
  const localId = view.you || undefined;
  const result = new Set<PlayerId>();
  if (offered.size === 0) return result;
  for (const perm of view.battlefield) {
    if (perm.controller !== localId && offered.has(perm.id)) result.add(perm.controller);
  }
  return result;
}

/** Find a rendered card anywhere in the scene by entity id. */
export function findCard(scene: TableScene, id: EntityId | null): RenderedCard | undefined {
  if (id === null) return undefined;
  for (const band of scene.bands) {
    const hit = band.cards.find((card) => card.entityId === id);
    if (hit) return hit;
  }
  return scene.hand.find((card) => card.entityId === id);
}

/**
 * A display-name lookup across every zone whose cards the view exposes (hand,
 * battlefield, graveyards, exile). Used to label the decision sheet's rows for a
 * `select_from_zone`/`order` over a non-canvas zone; an id with no known card
 * (e.g. a hidden library card or an abstract ordered trigger) falls back to its id.
 */
export function cardNameOf(view: GameView, id: EntityId): string {
  for (const card of view.my_hand) if (card.id === id) return card.name;
  for (const perm of view.battlefield) if (perm.id === id) return perm.card.name;
  for (const pile of view.graveyards)
    for (const card of pile.cards) if (card.id === id) return card.name;
  for (const pile of view.exile)
    for (const card of pile.cards) if (card.id === id) return card.name;
  return id;
}

/** Whether an id is rendered as a canvas card (hand or battlefield) in this view. */
export function isOnCanvas(view: GameView, id: EntityId): boolean {
  return view.my_hand.some((card) => card.id === id) || view.battlefield.some((p) => p.id === id);
}

/**
 * Whether the view itself poses a forced decision (issue #267): a subject-less
 * action carrying target requirements or non-target prompts — a mulligan, discard,
 * order, mode choice, or a combat declaration the server is asking the receiver to
 * resolve. These land the table in focus mode straight from the view (so a fresh
 * mount is in the right mode), independent of any in-progress client selection. A
 * subject action (e.g. casting a targeted spell from a card) is the player's
 * optional move, not a forced decision, so it does not by itself force focus.
 */
export function demandsDecision(view: GameView): boolean {
  return view.valid_actions.some(
    (action) =>
      (action.subject === undefined || action.subject.length === 0) &&
      ((action.requirements?.length ?? 0) > 0 || (action.prompts?.length ?? 0) > 0),
  );
}

/**
 * Resolve an entity id to what the inspect popover should show (issue #261),
 * searching every zone whose objects the view carries: the receiver's hand, the
 * battlefield (a permanent contributes its current face plus dynamic state), the
 * public graveyard/exile piles, and the stack. Presentation-only lookup over data
 * already in the view — it derives nothing. Returns `null` for an id that is not
 * inspectable in this view (e.g. it left its zone on a fresh frame).
 */
export function resolveInspect(view: GameView, id: EntityId): InspectTarget | null {
  for (const card of view.my_hand) if (card.id === id) return { kind: 'card', card };
  for (const perm of view.battlefield) {
    if (perm.id === id) {
      // Attachment relationship (issue #333), resolved from the view for both sides:
      // the host this permanent is attached to (if visible), and the attachments this
      // permanent hosts. Presentation-only lookup — the client derives no rules.
      const host =
        perm.attached_to !== undefined
          ? view.battlefield.find((p) => p.id === perm.attached_to)
          : undefined;
      const attachments = view.battlefield
        .filter((p) => p.attached_to === perm.id)
        .map((p) => ({ id: p.id, name: p.card.name }));
      return {
        kind: 'card',
        card: perm.card,
        tapped: perm.tapped,
        counters: perm.counters,
        attachedTo: host ? { id: host.id, name: host.card.name } : undefined,
        attachments: attachments.length > 0 ? attachments : undefined,
      };
    }
  }
  for (const pile of view.graveyards) {
    for (const card of pile.cards) if (card.id === id) return { kind: 'card', card };
  }
  for (const pile of view.exile) {
    for (const card of pile.cards) if (card.id === id) return { kind: 'card', card };
  }
  for (const item of view.stack) if (item.id === id) return { kind: 'stack', item };
  return null;
}

/**
 * The live keyboard bindings shown in the shortcut reference (issue #266): Pass is
 * marked available only when the action is actually offered and no pick is
 * in-flight, so the reference reflects the current view, not a static cheat-sheet.
 */
export function buildShortcutBindings(passOffered: boolean): Binding[] {
  return [
    {
      id: 'arrows',
      keys: '← → ↑ ↓',
      description: 'Move focus across regions and items',
      available: true,
    },
    {
      id: 'enter',
      keys: 'Enter',
      description: 'Activate focused control / confirm',
      available: true,
    },
    {
      id: 'space',
      keys: 'Space',
      description: 'Toggle / activate focused control',
      available: true,
    },
    { id: 'pass', keys: 'P', description: 'Pass priority', available: passOffered },
    { id: 'inspect', keys: 'I', description: 'Inspect the focused card', available: true },
    { id: 'escape', keys: 'Esc', description: 'Cancel or close', available: true },
    { id: 'toggle-help', keys: '?', description: 'Toggle this help', available: true },
  ];
}
