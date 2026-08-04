/**
 * Everything about *this device* rather than about this game, behind one gear
 * (`docs/client-design.md` §9.6).
 *
 * **A dialog, not a destination**, opened by the gear that is in every topbar and closed by
 * Escape or a click outside. It is sectioned, with a rail of sections at its leading edge and the
 * section beside it — which is the rail this document once wanted for the whole client, at the
 * one scale where it earns its keep.
 *
 * **Cards** is how much of a card's face is ours, and each option is a **tile rendering the same
 * card in that view**, because the choice is the thing itself and three words could not say it.
 * **Card art** is ADR 0012's pipeline made visible to the player it belongs to: the switch, what
 * is stored, a bulk download for preparing before a game, and Clear.
 *
 * **The two sections govern each other, and that is a rule rather than a nicety.** With art
 * switched off the two faces that need pictures are dimmed and offer the way to turn it on;
 * switching art off returns the face to `Frame`; the bulk download is not offered while art is
 * off. **Clearing is never disabled** — freeing space must not depend on a setting.
 *
 * **Keyboard** is the third section and it is here for the same reason the others are: it is
 * about the device, and a shortcut nobody can find is a shortcut nobody uses.
 */
import { useEffect, useState } from 'react'

import { catalogFace, type CardFace } from './../card-face'
import { BINDINGS } from './../keys'
import type { ArtStyle } from './../art/settings'
import type { CatalogCard } from './../protocol'
import { useArtCache, useArtPreference } from './art'
import { Card } from './card/Card'

/** The card the face tiles are drawn with, when the catalog has not arrived to lend a real one. */
const SAMPLE: CardFace = {
  id: 'sample',
  name: 'Bramble Sentinel',
  manaCost: '{1}{G}',
  typeLine: 'Creature — Elf Warrior',
  rulesText: 'Vigilance\nWhenever this creature attacks, you gain 1 life.',
  keywords: [],
  stat: { kind: 'power_toughness', value: '2/3', label: 'Power/toughness' },
  counters: [],
  markers: [],
  tapped: false,
  cardTypes: ['creature'],
  colorIdentity: ['G'],
  summoningSick: false,
}

const FACES: { style: ArtStyle; label: string; note: string }[] = [
  { style: 'frame', label: 'Frame only', note: 'Nothing is fetched. The card is entirely ours.' },
  { style: 'window', label: 'Frame and art', note: "The illustration alone, inside SAGE's frame." },
  { style: 'full', label: 'Full card', note: 'The printed card face, fetched whole.' },
]

const size = (bytes: number): string =>
  bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${(bytes / 1e6).toFixed(1)} MB`

/**
 * What a cached card costs, modelled rather than measured.
 *
 * The entries this client holds are URLs; the pictures themselves live in the browser's own
 * image store, which no page can measure. So the figure is a typical card's art crop and full
 * face, and it is described as an estimate wherever it is shown.
 */
const PER_CARD = 236_000

export function Settings({
  cards,
  onClose,
}: {
  /** The catalog, when this connection has one — what a bulk download would fetch. */
  cards: readonly CatalogCard[]
  onClose(): void
}) {
  const [section, setSection] = useState<'cards' | 'art' | 'keys'>('cards')
  const { preference, setPreference, clear } = useArtPreference()
  const { cached, waiting, download, stop } = useArtCache()

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const fetching = preference.source === 'scryfall'
  const sample = cards.find((card) => card.rules_text !== undefined && card.mana_cost !== undefined)
  const face = sample ? catalogFace(sample) : SAMPLE
  const total = cards.length
  const share = total === 0 ? 0 : Math.min(1, cached / total)

  const setArt = (on: boolean) =>
    // Turning it off returns the face to the one that needs no picture: the two that do could
    // not be drawn, and a face that cannot be drawn is not a setting.
    setPreference({
      ...preference,
      source: on ? 'scryfall' : 'procedural',
      style: on ? preference.style : 'frame',
    })

  return (
    <div className="zone-view" onClick={onClose}>
      <div
        className="zone-panel set-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="zone-head">
          <span className="zone-title">Settings</span>
          <span className="zone-tally" />
          <button className="zone-close" onClick={onClose} title="Close">
            ✕
          </button>
        </div>
        <div className="set-body">
          <div className="set-rail">
            {(
              [
                ['cards', 'Cards'],
                ['art', 'Card art'],
                ['keys', 'Keyboard'],
              ] as const
            ).map(([key, label]) => (
              <button
                key={key}
                className={`rail-btn${section === key ? ' rail-on' : ''}`}
                onClick={() => setSection(key)}
              >
                {label}
              </button>
            ))}
          </div>

          <div className="set-content">
            {section === 'cards' && (
              <section className="set-section">
                <h3 className="set-head">How cards are drawn</h3>
                <p className="set-note">
                  Applies everywhere a card appears — the board, your hand, the preview.
                </p>
                <div className="face-row">
                  {FACES.map((option) => {
                    const off = !fetching && option.style !== 'frame'
                    return (
                      <button
                        key={option.style}
                        className={`face-tile${preference.style === option.style ? ' face-on' : ''}${
                          off ? ' face-off' : ''
                        }`}
                        disabled={off}
                        aria-pressed={preference.style === option.style}
                        onClick={() => setPreference({ ...preference, style: option.style })}
                      >
                        <span className="face-card">
                          <Card face={face} />
                        </span>
                        <span className="face-label">{option.label}</span>
                        <span className="face-note">{option.note}</span>
                      </button>
                    )
                  })}
                </div>
                {/* The one piece of SAGE's frame a `full` face may keep. Offered here rather
                    than beside the art switch because it is about *how a card is drawn*, which
                    is what this section is; it is dimmed when no face it applies to is
                    chosen, rather than hidden, so the option is discoverable from the tile
                    that would use it. */}
                <div className={`set-row${preference.style === 'full' ? '' : ' set-row-off'}`}>
                  <span className="set-row-text">
                    <span className="set-row-label">Name and mana cost on full cards</span>
                    <span className="set-note">
                      Draws SAGE&apos;s name band over the printed one, where the printed cost is
                      hardest to read at board size. Power, toughness, counters and damage are
                      always drawn — those are the server&apos;s numbers, not the card&apos;s.
                    </span>
                  </span>
                  <button
                    role="switch"
                    aria-checked={preference.fullArtBand}
                    aria-label="Name and mana cost on full cards"
                    disabled={preference.style !== 'full'}
                    className={`switch${preference.fullArtBand ? ' switch-on' : ''}`}
                    onClick={() =>
                      setPreference({ ...preference, fullArtBand: !preference.fullArtBand })
                    }
                  >
                    <span className="switch-knob" />
                  </button>
                </div>

                {!fetching && (
                  <p className="set-note set-link">
                    Both of these need pictures.{' '}
                    <button
                      className="link-btn"
                      onClick={() => {
                        setArt(true)
                        setSection('art')
                      }}
                    >
                      Turn on card art
                    </button>{' '}
                    to use them.
                  </p>
                )}
              </section>
            )}

            {section === 'art' && (
              <section className="set-section">
                <h3 className="set-head">Card art</h3>
                <div className="set-row">
                  <span className="set-row-text">
                    <span className="set-row-label">Fetch art as I play</span>
                    <span className="set-note">
                      Your browser asks Scryfall directly, and keeps what comes back on this device.
                      Nothing passes through the SAGE server, and nothing is bundled, served, or
                      redistributed.
                    </span>
                  </span>
                  <button
                    role="switch"
                    aria-checked={fetching}
                    aria-label="Fetch art as I play"
                    className={`switch${fetching ? ' switch-on' : ''}`}
                    onClick={() => setArt(!fetching)}
                  >
                    <span className="switch-knob" />
                  </button>
                </div>

                <div className="set-block">
                  <div className="meter-head">
                    <span className="set-row-label">Downloaded</span>
                    <span className="meter-size">about {size(cached * PER_CARD)}</span>
                  </div>
                  <div className="meter">
                    <span className="meter-fill" style={{ width: `${share * 100}%` }} />
                  </div>
                  <div className="meter-foot">
                    {total === 0
                      ? `${cached.toLocaleString()} cards`
                      : `${cached.toLocaleString()} of ${total.toLocaleString()} cards`}
                    {waiting > 0 && <span className="meter-busy">downloading…</span>}
                    {!fetching && waiting === 0 && (
                      <span className="meter-off">card art is turned off</span>
                    )}
                  </div>
                  <div className="set-acts">
                    {waiting > 0 ? (
                      <button className="action-done action-alt" onClick={stop}>
                        Stop
                      </button>
                    ) : (
                      <button
                        className="action-done"
                        disabled={!fetching || total === 0 || cached >= total}
                        onClick={() =>
                          download(
                            cards.map((card) => ({
                              key: card.functional_id,
                              name: card.name,
                            })),
                          )
                        }
                      >
                        {total > 0 && cached >= total ? 'All art downloaded' : 'Download all art'}
                      </button>
                    )}
                    {/* Never disabled while there is anything to free: getting space back must
                        not depend on a setting. */}
                    <button
                      className="helper-btn helper-concede"
                      disabled={cached === 0}
                      onClick={clear}
                    >
                      Clear cache
                    </button>
                  </div>
                </div>
              </section>
            )}

            {section === 'keys' && (
              <section className="set-section">
                <h3 className="set-head">Keyboard</h3>
                <p className="set-note">
                  The skip keys are not this client passing for you: each sends one stop preference
                  and one pass, and the pacing that follows is the server acting on a preference it
                  stores.
                </p>
                <dl className="key-list">
                  {BINDINGS.map((binding) => (
                    <div key={binding.keys} className="key-row">
                      <dt>
                        <kbd>{binding.keys}</kbd>
                      </dt>
                      <dd>{binding.does}</dd>
                    </div>
                  ))}
                </dl>
              </section>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
