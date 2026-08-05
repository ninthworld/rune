/**
 * What the deck is made of: its curve, its colours, and its types.
 *
 * Three readings of one list, in the sidebar under the deck's name. **Each answers a different
 * question** — how expensive is this deck, how much of each colour is it asking for, and what
 * kind of cards is it — so each gets the form that answers it: bars over an ordered scale, a
 * ring over parts of a whole, and a row per type ordered by size.
 *
 * **Colour is the data here, not decoration.** A deck's colours are the five the game prints, in
 * the palette the card frames already wear (`cards.css`), so a green bar is the green a Forest
 * is. That also means hue cannot be re-chosen for contrast, which is why every colour carries its
 * pip and its number beside it: nothing here is readable by hue alone.
 *
 * Nothing is a verdict. These are counts of what the server described — a curve is not "good",
 * and no line here says a deck is legal.
 */
import type { DeckStats as Stats, DeckTally } from './../../builder'
import { Pip } from './../card/Pips'

/** The frame palette, by colour letter — the same tones `cards.css` washes a card in. */
const INK: Record<string, string> = {
  W: '#d8d2b4',
  U: '#89aed2',
  B: '#8b8496',
  R: '#d29677',
  G: '#9cb58c',
  C: '#b0b0ac',
}

/** A ring of parts. `pathLength` makes every slice a percentage, so no arc maths is needed. */
function Ring({ slices, total }: { slices: readonly DeckTally[]; total: number }) {
  // Each slice starts where every slice before it ended, worked out up front rather than by
  // carrying a running total through the render.
  const shares = slices.map((slice) => (total === 0 ? 0 : (slice.count / total) * 100))
  const starts = shares.map((_, at) => shares.slice(0, at).reduce((sum, share) => sum + share, 0))

  return (
    <svg className="stat-ring" viewBox="0 0 42 42" role="img" aria-label="Colours in the deck">
      <circle className="stat-ring-track" cx="21" cy="21" r="15.9155" fill="none" strokeWidth="5" />
      {slices.map((slice, index) => {
        const share = shares[index]!
        const at = starts[index]!
        return (
          <circle
            key={slice.key}
            cx="21"
            cy="21"
            r="15.9155"
            fill="none"
            stroke={INK[slice.key] ?? INK.C}
            strokeWidth="5"
            pathLength="100"
            // A 1% gap of the surface between neighbours, so two close tones stay two slices.
            strokeDasharray={`${Math.max(share - 1, 0)} ${100 - Math.max(share - 1, 0)}`}
            strokeDashoffset={-at}
            transform="rotate(-90 21 21)"
          />
        )
      })}
    </svg>
  )
}

export function DeckStats({ stats }: { stats: Stats }) {
  const tallest = Math.max(1, ...stats.curve.map((step) => step.count))
  const colored = stats.colors.reduce((sum, slice) => sum + slice.count, 0)
  const most = Math.max(1, ...stats.types.map((type) => type.count))

  if (stats.total === 0)
    return <div className="builder-stats builder-stats-empty">No cards yet.</div>

  return (
    <div className="builder-stats">
      <section className="stat-group" aria-label="Mana curve">
        <span className="files-label">Mana curve — spells</span>
        <div className="stat-curve">
          {stats.curve.map((step) => (
            <span className="stat-bar" key={step.key}>
              <span className="stat-plot">
                <span className="stat-n">{step.count || ''}</span>
                <span
                  className="stat-fill"
                  style={{ height: `${(step.count / tallest) * 100}%` }}
                />
              </span>
              <span className="stat-tick">{step.label}</span>
            </span>
          ))}
        </div>
      </section>

      <section className="stat-group" aria-label="Colors">
        <span className="files-label">Colors</span>
        <div className="stat-colors">
          <Ring slices={stats.colors} total={colored} />
          <ul className="stat-legend">
            {stats.colors.map((slice) => (
              <li key={slice.key}>
                <Pip symbol={slice.key} />
                <span className="stat-legend-name">{slice.label}</span>
                <span className="stat-n">{slice.count}</span>
              </li>
            ))}
          </ul>
        </div>
      </section>

      <section className="stat-group" aria-label="Card types">
        <span className="files-label">Card types</span>
        <ul className="stat-rows">
          {stats.types.map((type) => (
            <li key={type.key}>
              <span className="stat-row-name">{type.label}</span>
              <span className="stat-track">
                <span
                  className="stat-row-fill"
                  style={{ width: `${(type.count / most) * 100}%` }}
                />
              </span>
              <span className="stat-n">{type.count}</span>
            </li>
          ))}
        </ul>
      </section>
    </div>
  )
}
