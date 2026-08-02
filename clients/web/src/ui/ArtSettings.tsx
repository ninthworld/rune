/**
 * Where your card art comes from — the consent step of ADR 0012, and the switch it guards.
 *
 * The decision this asks a player to make is a real one, so what each source *does* is written
 * on the option itself: turning on a third-party source means *this browser* will request card
 * images from *that service*, and the only thing it will ever send is the names of the cards
 * being resolved. The project ships and serves no imagery either way; nothing downloaded here is
 * ever re-uploaded, proxied, or shared with another client, including the other player in the
 * same game.
 *
 * Off is the default and stays the default. Nothing infers consent, nothing turns it on after an
 * update, and switching back drops every image it fetched.
 *
 * One export, embedded by both settings surfaces: the shell's Settings destination
 * (`docs/client-design.md` §9.6) and the game's own gear. The standalone dialog this file used
 * to carry is gone — it existed because the lobby had nowhere to put a settings surface, and now
 * the lobby *is* a shell with a destination for exactly this.
 */
import { SCRYFALL } from './../art/source'
import type { ArtSourceId, ArtStyle } from './../art/settings'
import { useArtCache, useArtPreference } from './art'
import { Choice } from './controls'

export function ArtControls() {
  const { preference, setPreference, clear } = useArtPreference()
  const cached = useArtCache()

  return (
    <>
      <Choice
        label="Art source"
        columns
        value={preference.source}
        options={[
          {
            value: 'procedural',
            label: 'Drawn here',
            detail: 'A generated picture per card. Nothing is downloaded.',
          },
          {
            value: 'scryfall',
            label: SCRYFALL.label,
            detail: `Your browser fetches card images from ${SCRYFALL.label} and keeps them on this device. Only card names are sent.`,
          },
        ]}
        onChange={(source) => setPreference({ ...preference, source: source as ArtSourceId })}
      />

      {preference.source === 'scryfall' && (
        <Choice
          label="Art style"
          columns
          value={preference.style}
          options={[
            {
              value: 'window',
              label: 'Illustration only',
              detail: 'Inside SAGE’s frame, which keeps drawing the name, cost, type, and stat.',
            },
            {
              value: 'full',
              label: 'Whole card',
              detail:
                'The card image is the face. Everything the server computes still draws on top.',
            },
          ]}
          onChange={(style) => setPreference({ ...preference, style: style as ArtStyle })}
        />
      )}

      {/* What is cached, as the number it is. A card with no image drawn draws itself, so
          clearing is always safe and needs no warning printed beside the control. */}
      <p className="settings__cache">
        <span className="settings__count">{cached}</span>
        <span className="settings__unit">cards cached on this device</span>
        <button type="button" onClick={clear} disabled={cached === 0}>
          Forget them
        </button>
      </p>
    </>
  )
}
