/**
 * Where your card art comes from — the consent step of ADR 0012, and the switch it guards.
 *
 * The decision this panel is asking a player to make is a real one, so it is written out rather
 * than reduced to a toggle: turning on a third-party source means *this browser* will request
 * card images from *that service*, and the only thing it will ever send is the names of the cards
 * being resolved. The project ships and serves no imagery either way; nothing downloaded here is
 * ever re-uploaded, proxied, or shared with another client, including the other player in the
 * same game.
 *
 * Off is the default and stays the default. Nothing infers consent, nothing turns it on after an
 * update, and switching back drops every image it fetched.
 */
import { useEffect } from 'react'

import { SCRYFALL } from './../art/source'
import type { ArtSourceId, ArtStyle } from './../art/settings'
import { useArtPreference } from './art'

export function ArtSettings({ onClose }: { onClose(): void }) {
  const { preference, setPreference, clear } = useArtPreference()

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])

  const setSource = (source: ArtSourceId) => setPreference({ ...preference, source })
  const setStyle = (style: ArtStyle) => setPreference({ ...preference, style })

  return (
    <div className="inspector-backdrop" onClick={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Card art"
        className="inspector art-settings"
        onClick={(event) => event.stopPropagation()}
      >
        <h2>Card art</h2>

        <fieldset className="art-settings__group">
          <legend>Source</legend>

          <label>
            <input
              type="radio"
              name="art-source"
              checked={preference.source === 'procedural'}
              onChange={() => setSource('procedural')}
            />{' '}
            <strong>Drawn here</strong> — a generated picture per card. Nothing is downloaded, and
            this is what every card falls back to.
          </label>

          <label>
            <input
              type="radio"
              name="art-source"
              checked={preference.source === 'scryfall'}
              onChange={() => setSource('scryfall')}
            />{' '}
            <strong>{SCRYFALL.label}</strong> — your browser requests card images directly from{' '}
            {SCRYFALL.label} and keeps them on this device only. The card names being looked up are
            the only thing sent. SAGE itself ships, stores, and serves no card images.
          </label>
        </fieldset>

        {preference.source === 'scryfall' && (
          <fieldset className="art-settings__group">
            <legend>Style</legend>

            <label>
              <input
                type="radio"
                name="art-style"
                checked={preference.style === 'window'}
                onChange={() => setStyle('window')}
              />{' '}
              <strong>Illustration only</strong> — inside SAGE&rsquo;s frame, which keeps drawing
              the name, cost, type, and stat.
            </label>

            <label>
              <input
                type="radio"
                name="art-style"
                checked={preference.style === 'full'}
                onChange={() => setStyle('full')}
              />{' '}
              <strong>Whole card</strong> — the card image is the face. Everything the server
              computes still draws on top of it, so a buffed creature never reads as its printed
              numbers.
            </label>
          </fieldset>
        )}

        <p className="art-settings__note">
          Images are cached on this device and never leave it. Clearing them is always safe — a card
          with no image drawn draws itself.
        </p>

        <p>
          <button type="button" onClick={clear}>
            Forget cached art
          </button>
        </p>

        <button type="button" className="inspector__close" onClick={onClose} autoFocus>
          Close
        </button>
      </div>
    </div>
  )
}
