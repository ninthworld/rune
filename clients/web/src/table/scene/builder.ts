import type { EntityId, GameView, Permanent, PlayerId } from '../../protocol';
import { identityAccent } from '../identityAccents';
import {
  type SceneGeometry,
  type TableScene,
  type RenderedCard,
  type TargetingScene,
  type CombatLink,
  type AttackTarget,
} from './types';
import { layPanel, layHand } from './row-layout';
import { toDisplayData, rowKindForType, basicLandGlyph, groupStacks } from './card-helpers';
import { actionsFor, declarationFor } from './action-helpers';
import { localPlayerIdOf, orderedOpponentIds, bandLabel, zoneCountsOf } from './band-helpers';

/**
 * Build the full scene from a view and the shell's carved geometry. `selectedId`
 * marks the currently selected entity so its card draws a selection ring (and a
 * selected hand card lifts); it never changes what is offered.
 *
 * When `targeting` is supplied the scene enters targeting mode: only the listed
 * candidate cards are targetable (highlighted with the targeting ring), every
 * other card is dimmed and non-interactive, and normal subject-actions are
 * suppressed so the sole interaction is picking a target. The candidates come
 * straight from the server; the scene derives no legality (ADR 0009 §Client).
 */
export function buildTableScene(
  view: GameView,
  selectedId: EntityId | undefined,
  geometry: SceneGeometry,
  targeting?: TargetingScene,
): TableScene {
  const localPlayerId = localPlayerIdOf(view);
  const subjectActions = view.valid_actions.filter((a) => a.subject && a.subject.length > 0);
  const candidateSet = targeting ? new Set(targeting.candidates) : null;
  const chosenSet = targeting ? new Set(targeting.selected ?? []) : null;

  const blockerCountByAttacker = new Map<EntityId, number>();
  const combatLinks: CombatLink[] = [];
  const attackTargets: AttackTarget[] = [];
  for (const perm of view.battlefield) {
    if (perm.blocking !== undefined) {
      blockerCountByAttacker.set(
        perm.blocking,
        (blockerCountByAttacker.get(perm.blocking) ?? 0) + 1,
      );
      combatLinks.push({ blocker: perm.id, attacker: perm.blocking });
    }
    if (perm.attacking_player !== undefined) {
      attackTargets.push({ attacker: perm.id, defender: perm.attacking_player });
    }
  }

  const withTargeting = (
    card: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds' | 'targetable' | 'chosen'>,
  ): Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'> => {
    if (candidateSet === null) return { ...card, targetable: false, chosen: false };
    const targetable = candidateSet.has(card.entityId);
    const chosen = targetable && (chosenSet?.has(card.entityId) ?? false);
    return {
      ...card,
      data: {
        ...card.data,
        selected: chosen,
        targeting: targetable,
        dimmed: !targetable,
        actionable: false,
      },
      actions: [],
      declaration: undefined,
      targetable,
      chosen,
    };
  };

  const byController = new Map<PlayerId, Permanent[]>();
  for (const perm of view.battlefield) {
    const list = byController.get(perm.controller) ?? [];
    list.push(perm);
    byController.set(perm.controller, list);
  }

  const orderedOpponents = orderedOpponentIds(view);
  const ordered: PlayerId[] = [...orderedOpponents];
  for (const controller of byController.keys()) {
    if (!ordered.includes(controller) && controller !== localPlayerId) ordered.push(controller);
  }

  const toRenderable = (
    perm: Permanent,
    cluster?: { attachedTo?: EntityId; attachments?: EntityId[] },
  ): Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'> => {
    const actions = actionsFor(perm.id, subjectActions);
    const declaration =
      actions.length === 0 ? declarationFor(perm.id, view.valid_actions) : undefined;
    const rowKind = rowKindForType(perm.card.type_line);
    const landGlyph = rowKind === 'lands' ? basicLandGlyph(perm.card.type_line) : undefined;
    return withTargeting({
      entityId: perm.id,
      zone: 'battlefield',
      tier: 'support',
      name: perm.card.name,
      data: toDisplayData(perm.card, {
        tapped: perm.tapped,
        counters: perm.counters,
        selected: perm.id === selectedId,
        actionable: actions.length > 0 || declaration !== undefined,
        landGlyph,
        attacking: perm.attacking,
        attackingPlayer: perm.attacking_player,
        blocking: perm.blocking !== undefined,
        blockedBy: blockerCountByAttacker.get(perm.id),
        markedDamage: perm.damage,
      }),
      actions,
      declaration,
      attachedTo: cluster?.attachedTo,
      attachments: cluster?.attachments,
    });
  };

  const renderablesFor = (
    perms: Permanent[],
  ): Record<import('./types').BandRowKind, Omit<RenderedCard, 'rect'>[]> => {
    const bandPermById = new Map<EntityId, Permanent>(perms.map((p) => [p.id, p]));
    const clustersUnderHost = (p: Permanent): boolean => {
      if (p.attached_to === undefined) return false;
      const host = bandPermById.get(p.attached_to);
      return host !== undefined && host.attached_to === undefined;
    };
    const attachmentsByHost = new Map<EntityId, Permanent[]>();
    for (const p of perms) {
      if (!clustersUnderHost(p)) continue;
      const list = attachmentsByHost.get(p.attached_to!) ?? [];
      list.push(p);
      attachmentsByHost.set(p.attached_to!, list);
    }
    const inRow = (kind: string): Omit<RenderedCard, 'rect'>[] => {
      const renderables: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'>[] = [];
      for (const p of perms) {
        if (rowKindForType(p.card.type_line) !== kind) continue;
        if (clustersUnderHost(p)) continue;
        const attachments = attachmentsByHost.get(p.id);
        renderables.push(
          toRenderable(p, attachments ? { attachments: attachments.map((a) => a.id) } : undefined),
        );
        for (const att of attachments ?? []) {
          renderables.push(toRenderable(att, { attachedTo: p.id }));
        }
      }
      return groupStacks(renderables);
    };
    return { creatures: inRow('creatures'), support: inRow('support'), lands: inRow('lands') };
  };

  const frames =
    localPlayerId === undefined ? [...geometry.opponents, geometry.you] : geometry.opponents;
  const bands = [];
  ordered.forEach((playerId, index) => {
    const frame = frames[Math.min(index, frames.length - 1)] ?? geometry.you;
    const perms = byController.get(playerId) ?? [];
    if (frame.summary) {
      bands.push({
        playerId,
        isLocal: false,
        cards: [],
        rows: [],
        isEmpty: perms.length === 0,
        label: bandLabel(view, playerId, false),
        zones: zoneCountsOf(view, playerId, false),
        accent: identityAccent(view, playerId),
        rect: frame.rect,
        headerRect: frame.header,
        pileRect: frame.piles,
        densityRung: 0,
        summary: true,
      });
      return;
    }
    const laid = layPanel(renderablesFor(perms), frame.content, geometry.tiers.opp);
    bands.push({
      playerId,
      isLocal: false,
      cards: laid.cards,
      rows: laid.rows,
      isEmpty: perms.length === 0,
      label: bandLabel(view, playerId, false),
      zones: zoneCountsOf(view, playerId, false),
      accent: identityAccent(view, playerId),
      rect: frame.rect,
      headerRect: frame.header,
      pileRect: frame.piles,
      densityRung: laid.densityRung,
      summary: false,
    });
  });
  if (localPlayerId !== undefined) {
    const perms = byController.get(localPlayerId) ?? [];
    const laid = layPanel(renderablesFor(perms), geometry.you.content, geometry.tiers.you);
    bands.push({
      playerId: localPlayerId,
      isLocal: true,
      cards: laid.cards,
      rows: laid.rows,
      isEmpty: perms.length === 0,
      label: bandLabel(view, localPlayerId, true),
      zones: zoneCountsOf(view, localPlayerId, true),
      accent: identityAccent(view, localPlayerId),
      rect: geometry.you.rect,
      headerRect: geometry.you.header,
      pileRect: geometry.you.piles,
      densityRung: laid.densityRung,
      summary: false,
    });
  }

  const handCards: Omit<RenderedCard, 'rect'>[] = view.my_hand.map((card) => {
    const actions = actionsFor(card.id, subjectActions);
    const base = withTargeting({
      entityId: card.id,
      zone: 'hand' as const,
      tier: 'hand' as const,
      name: card.name,
      data: toDisplayData(card, {
        selected: card.id === selectedId,
        actionable: actions.length > 0,
      }),
      actions,
    });
    return { ...base, stackCount: 1, memberIds: [card.id] };
  });
  const hand = layHand(handCards, geometry.hand, selectedId);

  return {
    width: geometry.width,
    height: geometry.height,
    bands,
    hand,
    handRegion: { rect: geometry.hand, label: 'Your hand' },
    localPlayerId,
    combatLinks,
    attackTargets,
  };
}
