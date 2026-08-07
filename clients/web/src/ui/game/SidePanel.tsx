/**
 * The column beside the board: what you are looking at, how fast the game runs, and the three
 * lists that all want the same space.
 *
 * **The preview follows the look** (§6.6). Reading a card costs no click at all — the pointer is
 * the gesture, and the card is drawn whole at the top of this column while the board stays
 * visible underneath it. Right-clicking pins it, so the pointer can go back to the game without
 * the preview being replaced by whatever it passes over.
 *
 * Under it is the pace: the whole stop preference as three buttons, in the recess a run of small
 * panes needs so it does not read as a row of unrelated controls. **None of them is this client
 * passing for you** — each sends one `set_stops` and one pass, and everything after that is the
 * server's settle acting on a preference it stores (ADR 0010). Concede sits at the end of the
 * same strip, in the one colour nothing else on the board wears.
 *
 * Then the tabs. **Stack comes first because it is the only one you have to read now** — it is
 * the game asking you to respond — and its tab carries a count, so a spell going on the stack is
 * visible from the Log tab too. Anything the server is showing this seat right now (`revealed`)
 * is drawn above the stack, because it is the same kind of fact: something to read before the
 * next click.
 *
 * **Chat has nothing behind it.** The protocol carries none, and the tab is drawn saying so
 * rather than being left out and having to be designed back in.
 */
import { useState } from 'react'

import type { CardFace } from './../../card-face'
import { describe, kindOf } from './../../game-log'
import type { GameLogEntry, StackItem } from './../../protocol'
import { presetWording, type StopPreset } from './../../turn'
import { Card } from './../card/Card'
import { Symbols } from './../card/Symbols'
import type { Surface } from './surface'

/** One object on the stack, with what the server said about it. */
export interface StackEntry {
  item: StackItem
  face: CardFace
  /** Who put it there, and what it is aimed at, in the words the view stated. */
  who: string
  kind: string
  /**
   * Which spell or ability this entry *is*, in the server's own words (`StackItem.description`),
   * or absent where that only repeats the card's name.
   *
   * A stack entry showing a name, a controller, and targets does not say which of a permanent's
   * abilities was activated (issue #715), and no client may work that out — composing an
   * ability's text from its effects is rules interpretation, which is why the server sends this
   * already composed. Held here rather than folded into `kind` because the two answer different
   * questions: `kind` is what sort of object it is, this is which one.
   */
  detail?: string
  targets: readonly string[]
}

const TABS = ['Stack', 'Log', 'Chat'] as const
type Tab = (typeof TABS)[number]

const PRESETS: readonly StopPreset[] = ['everywhere', 'mains', 'nowhere']

/** Short enough for a button; the whole sentence stays in the title. */
const PRESET_LABELS: Record<StopPreset, string> = {
  everywhere: 'Every step',
  mains: 'My mains',
  nowhere: 'Only when asked',
}

export function SidePanel({
  open,
  preview,
  pinned,
  onUnpin,
  revealed,
  stack,
  log,
  label,
  preset,
  onPreset,
  onConcede,
  concedeAsked,
  surface,
}: {
  open: boolean
  /** The card the pointer is over, or the one that was pinned there. */
  preview?: CardFace
  pinned: boolean
  onUnpin(): void
  revealed: readonly CardFace[]
  stack: readonly StackEntry[]
  log: readonly GameLogEntry[]
  label(id: string): string
  preset?: StopPreset
  /**
   * Absent for a reader with no seat: pace is a preference the server stores against one, and a
   * spectator has none. With no concede either, the strip is not drawn empty — it is not drawn.
   */
  onPreset?(preset: StopPreset): void
  /** Absent when the server is not currently offering the action. */
  onConcede?(): void
  concedeAsked: boolean
  surface: Surface
}) {
  const [tab, setTab] = useState<Tab>('Stack')

  return (
    <div className={`log${open ? ' log-open' : ''}`}>
      <div className="preview-section">
        {preview && (
          <>
            <Card face={preview} />
            {pinned && (
              <button className="preview-unpin" onClick={onUnpin}>
                Unpin
              </button>
            )}
          </>
        )}
      </div>

      {(onPreset || onConcede) && (
        <div className="helper-strip" role="group" aria-label="Pace">
          {onPreset &&
            PRESETS.map((entry) => (
              <button
                key={entry}
                className={`helper-btn${preset === entry ? ' view-on' : ''}`}
                title={presetWording(entry)}
                aria-pressed={preset === entry}
                onClick={() => onPreset(entry)}
              >
                {PRESET_LABELS[entry]}
              </button>
            ))}
          {onConcede && (
            <button className="helper-btn helper-concede" onClick={onConcede}>
              {concedeAsked ? 'Yes, concede the game' : 'Concede'}
            </button>
          )}
        </div>
      )}

      <div className="panel">
        <div className="panel-tabs">
          {TABS.map((name) => (
            <button
              key={name}
              className={`panel-tab${tab === name ? ' panel-tab-on' : ''}`}
              onClick={() => setTab(name)}
            >
              {name}
              {name === 'Stack' && stack.length > 0 && (
                <span className="panel-count">{stack.length}</span>
              )}
            </button>
          ))}
        </div>

        <div className="panel-body" role="region" aria-label={tab}>
          {tab === 'Stack' && (
            <>
              {revealed.length > 0 && (
                <>
                  <div className="stack-head">shown to you</div>
                  {revealed.map((face) => (
                    <div
                      key={face.id}
                      className="stack-item"
                      onMouseEnter={() => surface.trace(face.id)}
                      onMouseLeave={() => surface.trace(undefined)}
                    >
                      <div className="stack-thumb">
                        <Card face={face} onActivate={surface.activate} />
                      </div>
                      <div className="stack-text">
                        <div className="stack-name">{face.name}</div>
                        <div className="stack-kind">{face.typeLine}</div>
                      </div>
                    </div>
                  ))}
                </>
              )}

              {stack.length === 0 && revealed.length === 0 ? (
                <div className="panel-empty">The stack is empty.</div>
              ) : (
                stack.length > 0 && (
                  <>
                    <div className="stack-head">resolves next</div>
                    {stack.map((entry) => (
                      <div
                        key={entry.item.id}
                        className="stack-item"
                        onMouseEnter={() => surface.trace(entry.item.id)}
                        onMouseLeave={() => surface.trace(undefined)}
                      >
                        <div className="stack-thumb">
                          <Card
                            face={entry.face}
                            anchor={entry.item.id}
                            state={surface.stateOf(entry.item.id)}
                            link={surface.linkOf(entry.item.id)}
                            onActivate={surface.activate}
                            onInspect={surface.inspect}
                          />
                        </div>
                        <div className="stack-text">
                          <div className="stack-name">{entry.face.name}</div>
                          <div className="stack-kind">
                            {entry.who} · {entry.kind}
                          </div>
                          {/* **Which** spell or ability this entry is (issue #715). The card name
                              above it names the *source*, and a permanent with three abilities put
                              three identical-looking entries on the stack. The server composes this
                              from the object's own effects — the same formatter that writes the
                              card's rules text — so it distinguishes two abilities of one card, and
                              it keeps naming the ability after its source has left the battlefield,
                              where the thumbnail cannot. Drawn only where it says something the
                              name does not, which for a spell is nothing. */}
                          {entry.detail !== undefined && (
                            <div className="stack-detail">{entry.detail}</div>
                          )}
                          {entry.targets.length > 0 && (
                            <div className="stack-target">→ {entry.targets.join(', ')}</div>
                          )}
                        </div>
                      </div>
                    ))}
                  </>
                )
              )}
            </>
          )}

          {tab === 'Log' &&
            (log.length === 0 ? (
              <div className="panel-empty">Nothing has happened yet.</div>
            ) : (
              log.map((entry, index) => {
                const kind = kindOf(entry.event)
                const text = describe(entry.event, label)
                return kind === 'step' ? (
                  <div key={index} className="log-turn">
                    {text}
                  </div>
                ) : (
                  <div key={index} className={`log-line log-${kind}`}>
                    <Symbols text={text} />
                  </div>
                )
              })
            ))}

          {tab === 'Chat' && <div className="panel-empty">A table carries no chat yet.</div>}
        </div>
      </div>

      {tab === 'Chat' && (
        <div className="chat-entry">
          <input
            className="chat-input"
            aria-label="Say something"
            placeholder="Say something"
            disabled
          />
        </div>
      )}
    </div>
  )
}
