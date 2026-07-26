/**
 * The **presentation caption** (issue #594) — the one line that says what the
 * board just did, at the pace {@link usePresentationTrail} sets.
 *
 * ADR 0020's settle loop applies many actions before it broadcasts, and the
 * per-seat channel is latest-value, so a receiver is handed a *final* board where
 * the game passed through a sequence of causal states. Diffing two boards can say
 * a creature is gone; only this ordered sequence says whether it resolved and
 * died, was countered on the stack, or fizzled. That distinction is the entire
 * reason the surface exists.
 *
 * ## What it is, and what it must never become
 *
 * - **It is a caption, not a prompt.** Nothing here is acknowledgeable: no button,
 *   no dismissal, no confirmation, no focus. It is `pointer-events: none`, so it
 *   cannot take a click meant for the board underneath it, and it never covers a
 *   decision (`--rune-z-shell`, below `--rune-z-decision`).
 * - **It reconstructs no rules.** Every word comes off a fact the server stated in
 *   the moment: the retained name (fixed at record time and never re-resolved, so
 *   a dead permanent still reads correctly), the amounts, the zones. Names of
 *   *players* come from the view's own map through {@link playerName}. No legality,
 *   no cost, no effect is derived here — the same discipline as
 *   {@link describeEvent}, which turns the authoritative log into prose.
 * - **It is not the record.** {@link GameView.log} is, and the activity ticker
 *   already announces it in the shell's single activity live region. This surface
 *   would say the same things a beat later, so it is `aria-hidden`: a second live
 *   region duplicating the first is noise, not access. The one fact it carries
 *   that the log does not — that the server auto-passed this seat — is already
 *   announced by the phase plaque's `role="status"` badge, whose accessible name
 *   is the same auto-passed path (issue #455).
 *
 * ## Reduced motion
 *
 * There is nothing here to reduce. The caption does not enter, exit, tween, or
 * transition in any mode: it appears at full opacity for its dwell and is replaced
 * by the next one, exactly as the activity ticker's lines are. Reduced motion
 * therefore renders the identical surface with the identical words — it removes
 * only *travel* elsewhere in the scene, which the trail reports as
 * {@link StagedMoment.travel} and this surface exposes as `data-travel`.
 */
import type { CardView, GameOverReason, GameResult, GameView, PlayerId } from '../../protocol';
import type { MomentKind, MomentObject, MomentZone } from '../../protocol';
import { playerName } from '../../playerNames';
import { CardArt } from '../../card/dom';
import { domCardArt } from '../planeDisplayData';
import { phaseLabel } from '../logComposition';
import type { StagedMoment } from './presentationTrail';
import s from './presentation-trail.module.css';

/** The view fields the caption needs: the display-name map, for players only. */
type NamingView = Pick<GameView, 'player_names'>;

export interface PresentationTrailProps {
  /**
   * The moment on screen now, straight from {@link usePresentationTrail}, or
   * `null` when the trail is quiet — in which case the surface renders nothing at
   * all rather than an empty plate. It reserves no track and no space.
   */
  staged: StagedMoment | null;
  /** The latest view. Read for `player_names` and nothing else. */
  view: NamingView;
}

/** One composed caption. */
interface TrailCaption {
  /** The sentence. */
  text: string;
  /**
   * A second, quieter line. Today only the auto-pass reason — the concise "no
   * response available" that tells a player why a run of steps went past them
   * without a stop.
   */
  cue?: string;
  /**
   * The **retained** public face the caption is about, when the room had one
   * cached. Retained means renderable after the object is gone: the whole point of
   * the snapshot is that a permanent's face survives its own death.
   */
  card?: CardView;
}

/** Where a zone move ended, as a reader would say it. */
const ZONE_WORDS: Record<MomentZone, string> = {
  battlefield: 'the battlefield',
  graveyard: 'the graveyard',
  exile: 'exile',
  hand: 'hand',
  library: 'the library',
  stack: 'the stack',
  command: 'the command zone',
};

/** Why a game or a seat ended (display only; CR 104). */
function reasonWords(reason: GameOverReason): string {
  switch (reason) {
    case 'life_zero':
      return 'life total reached zero';
    case 'decked':
      return 'drew from an empty library';
    case 'concede':
      return 'conceded';
    default:
      // Tolerate an unknown future reason without inventing meaning for it.
      return reason;
  }
}

/** The terminal sentence, winner or draw. */
function resultWords(result: GameResult, view: NamingView): string {
  const clause = reasonWords(result.reason);
  if (result.winner === undefined) return `Game over — draw (${clause})`;
  return `Game over — ${playerName(view, result.winner)} wins (${clause})`;
}

/** A player's display name, never derived from the id when a real one exists. */
function who(view: NamingView, id: PlayerId): string {
  return playerName(view, id);
}

/** The retained name of an object, as the server fixed it at record time. */
function what(object: MomentObject): string {
  return object.name;
}

/**
 * Compose one moment into its caption, or `null` when this build cannot read the
 * kind the server named (the normalizer's `kindUnknown`).
 *
 * A moment whose kind is unclassified still holds the surface for its dwell — the
 * scheduler staged it, and dropping a beat would misrepresent the order — it just
 * says nothing, which is the honest rendering of "a newer server knows something
 * this client does not".
 */
function captionOf(kind: MomentKind | undefined, view: NamingView): TrailCaption | null {
  if (kind === undefined) return null;
  switch (kind.kind) {
    case 'cast':
      return {
        text: `${who(view, kind.player)} casts ${what(kind.object)}`,
        card: kind.object.card,
      };
    case 'resolved':
      return { text: `${what(kind.object)} resolves`, card: kind.object.card };
    case 'countered':
      return { text: `${what(kind.object)} is countered`, card: kind.object.card };
    case 'fizzled':
      return { text: `${what(kind.object)} fizzles`, card: kind.object.card };
    case 'zone_move':
      // Both endpoints, because the destination alone cannot say what the movement
      // *was*: a commander reaching the command zone from exile and one reaching it
      // from a graveyard are different events (CR 903.9a), and the server does real
      // work to prove which — it declines to emit the moment at all rather than
      // guess an origin. Naming only the arrival throws that away and reproduces
      // exactly the ambiguity the pair was added to remove.
      return {
        text: `${what(kind.object)}: ${ZONE_WORDS[kind.from]} → ${ZONE_WORDS[kind.to]}`,
        card: kind.object.card,
      };
    case 'died':
      return { text: `${what(kind.object)} dies`, card: kind.object.card };
    case 'damage': {
      const target =
        kind.target.kind === 'player' ? who(view, kind.target.player) : kind.target.permanent.name;
      return { text: `${target} takes ${kind.amount} damage` };
    }
    case 'life': {
      const verb = kind.amount >= 0 ? 'gains' : 'loses';
      return { text: `${who(view, kind.player)} ${verb} ${Math.abs(kind.amount)} life` };
    }
    case 'attacked':
      return {
        text:
          kind.attackers.length === 0
            ? `${who(view, kind.player)} declares no attackers`
            : `${who(view, kind.player)} attacks with ${kind.attackers.map(what).join(', ')}`,
      };
    case 'blocked':
      return {
        text:
          kind.blocks.length === 0
            ? `${who(view, kind.player)} declares no blockers`
            : `${who(view, kind.player)} blocks with ${kind.blocks
                .map((block) => block.blocker.name)
                .join(', ')}`,
      };
    case 'drew':
      return {
        text: `${who(view, kind.player)} draws ${kind.count} ${kind.count === 1 ? 'card' : 'cards'}`,
      };
    case 'turn_change':
      return { text: `Turn ${kind.turn} — ${who(view, kind.active_player)}` };
    case 'phase_change':
      return { text: phaseLabel(kind.phase) };
    case 'phases_skipped': {
      // ONE caption for the whole path the server folded into this moment, never
      // one per priority window: a settle can pass a seat through a dozen of them,
      // and a caption each would spend the window on the nothing that happened.
      const first = kind.steps[0];
      const last = kind.steps[kind.steps.length - 1];
      const where =
        first === undefined
          ? ''
          : last === undefined || first === last
            ? ` ${phaseLabel(first.phase)}`
            : ` ${phaseLabel(first.phase)} → ${phaseLabel(last.phase)}`;
      return {
        text: `Auto-passed${where}`,
        cue: kind.reason === 'forced_declaration' ? 'forced declaration' : 'no response available',
      };
    }
    case 'eliminated':
      return { text: `${who(view, kind.player)} is eliminated (${reasonWords(kind.reason)})` };
    case 'game_over':
      return { text: resultWords(kind.result, view) };
    default:
      return null;
  }
}

/** The retained face, drawn only when the player's own art pipeline has an image
 * for it (ADR 0024: device-local, opt-in, never shipped by the project). Without
 * one the caption's own words carry the card, which is why this is optional
 * decoration and never the information. */
function RetainedFace({ card }: { card: CardView }) {
  const art = domCardArt(card);
  if (art === undefined) return null;
  return (
    <span className={s.face} data-testid="presentation-trail-face">
      <CardArt url={art.url} mode={art.full ? 'panelFull' : 'panel'} />
    </span>
  );
}

/**
 * Render the staged moment as one ordered caption. Renders nothing when the trail
 * is quiet, and nothing when the staged kind is one this build cannot read.
 */
export function PresentationTrail({ staged, view }: PresentationTrailProps) {
  if (staged === null) return null;
  const { moment } = staged;
  const caption = captionOf(moment.kind, view);
  if (caption === null) return null;
  return (
    <div
      className={s.trail}
      data-testid="presentation-trail"
      // The moment's own identity and position, as data: assertable, and never
      // parsed for meaning. `id` is an opaque ordering handle — a gap between two
      // of them is normal and is not a lost message.
      data-moment-id={moment.id}
      data-moment-kind={moment.kind?.kind}
      data-count={moment.count}
      data-travel={staged.travel || undefined}
      // A caption about a board the player is already looking at, in a shell that
      // already announces the authoritative log once. See the module doc comment.
      aria-hidden="true"
    >
      {caption.card && <RetainedFace card={caption.card} />}
      <div className={s.lines}>
        <p className={s.caption} data-testid="presentation-trail-caption">
          {caption.text}
          {moment.count > 1 && (
            <span className={s.count} data-testid="presentation-trail-count">
              {`×${moment.count}`}
            </span>
          )}
        </p>
        {caption.cue !== undefined && (
          <p className={s.cue} data-testid="presentation-trail-cue">
            {caption.cue}
          </p>
        )}
      </div>
    </div>
  );
}
