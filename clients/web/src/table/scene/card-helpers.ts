import type { CardView, Counter } from '../../protocol';
import { cardVisualSignature, type CardDisplayData } from '../../card/cardFactory';
import { artKeyFor } from '../../card/art/artStore';
import type { GlyphName } from '../../chrome/glyphs';
import { deriveColorIdentity } from '../colorIdentity';
import type { RenderedCard, BandRowKind } from './types';
import type { ValidAction } from '../../protocol';

/** Map a server card + permanent state onto the factory's display data. */
export function toDisplayData(
  card: CardView,
  opts: {
    tapped?: boolean;
    counters?: Counter[];
    selected: boolean;
    actionable: boolean;
    landGlyph?: GlyphName;
    attacking?: boolean;
    attackingPlayer?: string;
    blocking?: boolean;
    blockedBy?: number;
    markedDamage?: number;
  },
): CardDisplayData {
  return {
    name: card.name,
    typeLine: card.type_line,
    colorIdentity: deriveColorIdentity(card),
    manaCost: card.mana_cost,
    power: card.power,
    toughness: card.toughness,
    counters: opts.counters,
    tapped: opts.tapped,
    selected: opts.selected,
    attacking: opts.attacking,
    attackingPlayer: opts.attackingPlayer,
    blocking: opts.blocking,
    blockedBy: opts.blockedBy,
    markedDamage: opts.markedDamage,
    actionable: opts.actionable,
    landGlyph: opts.landGlyph,
    keywords: card.keywords,
    hasActivatedAbility: hasActivatedAbilityText(card.rules_text),
    artKey: artKeyFor(card.functional_id),
  };
}

/**
 * Whether a card's printed rules text describes a **latent activated ability** (issue
 * #320). This is a display heuristic over the server-generated rules text (ADR 0018),
 * **not** rules computation: an activated ability is printed as `"cost: effect"`, so a
 * cost/effect colon marks one — independently of whether the ability is payable right
 * now (that "live" state is the gold edge bar's job, driven by `valid_actions`). If a
 * dedicated view field for this ever ships, this heuristic is the swap point.
 */
export function hasActivatedAbilityText(rulesText?: string): boolean {
  return rulesText !== undefined && /:\s/.test(rulesText);
}

/**
 * Which type-grouped row a permanent belongs to (issue #318), derived from the
 * **server-computed type line** alone — the client knows no rules. A permanent that
 * is any kind of creature/planeswalker/battle goes to the front row (so an animated
 * land or crewed Vehicle migrates up when its types change); a land goes to the back
 * chip row; everything else (artifacts, enchantments/auras) is support. The creature
 * test comes first so an "Artifact Creature" or "Land Creature" reads as a creature.
 */
export function rowKindForType(typeLine: string): BandRowKind {
  if (/\b(Creature|Planeswalker|Battle)\b/.test(typeLine)) return 'creatures';
  if (/\bLand\b/.test(typeLine)) return 'lands';
  return 'support';
}

/** The glyph for a basic land's chip, or `undefined` for a nonbasic land / non-land. */
export function basicLandGlyph(typeLine: string): GlyphName | undefined {
  if (!/\bBasic\b/.test(typeLine)) return undefined;
  if (/\bPlains\b/.test(typeLine)) return 'land-plains';
  if (/\bIsland\b/.test(typeLine)) return 'land-island';
  if (/\bSwamp\b/.test(typeLine)) return 'land-swamp';
  if (/\bMountain\b/.test(typeLine)) return 'land-mountain';
  if (/\bForest\b/.test(typeLine)) return 'land-forest';
  return undefined;
}

/**
 * A fingerprint of a card's offered subject-actions, used only as part of the ×N
 * grouping key. Two permanents whose full visual state AND offered action shapes
 * (type + label, in server order) are identical are interchangeable from the
 * player's point of view, so they may fold into one stack — activating the stack
 * submits the representative's action id, and the server's next view splits the
 * stack exactly where states diverge. Entity-bound ids are deliberately excluded
 * (they always differ); this is a presentation key, never legality.
 */
export function actionFingerprint(actions: ValidAction[]): string {
  return actions.map((a) => `${a.type} ${a.label}`).join('');
}

/**
 * Collapse identical-state permanents in one row into `×N` stacks (issue #318). The
 * grouping key is the card's **full visual signature** (tap state, counters, and all
 * interactive flags included) plus its offered-action fingerprint, so a stack never
 * hides a differing card — "four Plains, one tapped" reads as an untapped ×3 beside
 * a tapped single. Ordinary actionability does NOT force a card to render alone:
 * four untapped Plains each offering the same tap-for-mana action fold into one
 * activatable ×4 stack (activation fires the representative's action). A card that
 * carries a *pick-specific* affordance — target candidacy, a multi-select pick, the
 * current selection, combat participation, or an attachment relationship — is never
 * folded in, so every individually-addressable permanent stays its own render for
 * prompts and clicks (ui-requirements §Table and zones).
 */
export function groupStacks(
  cards: Omit<RenderedCard, 'rect' | 'stackCount' | 'memberIds'>[],
): Omit<RenderedCard, 'rect'>[] {
  const result: Omit<RenderedCard, 'rect'>[] = [];
  const stackAt = new Map<string, number>();
  for (const card of cards) {
    const individual =
      card.targetable ||
      card.chosen ||
      card.data.selected === true ||
      card.data.attacking === true ||
      card.data.blocking === true ||
      card.attachedTo !== undefined ||
      (card.attachments?.length ?? 0) > 0;
    if (individual) {
      result.push({ ...card, stackCount: 1, memberIds: [card.entityId] });
      continue;
    }
    const key = `${cardVisualSignature(card.data, card.tier)}|${actionFingerprint(card.actions)}`;
    const at = stackAt.get(key);
    if (at === undefined) {
      stackAt.set(key, result.length);
      result.push({ ...card, stackCount: 1, memberIds: [card.entityId] });
    } else {
      const group = result[at]!;
      group.memberIds.push(card.entityId);
      group.stackCount += 1;
      group.data = { ...group.data, stackCount: group.stackCount };
    }
  }
  return result;
}
