# The client, designed

**Status: binding. This is the layout authority for `clients/web`.** Where any other file states a
layout constraint that contradicts this one, this one wins and the other is stale.

This is a design specification, not an ADR — ADRs are written after a decision survives contact
with working code, and an ADR follows once this approach has. Binding and settled are different
things: what is fixed here is the *contract* — what must be visible, what may give way, and that
geometry is computed rather than inherited from content. How a given surface satisfies it is
still open, and a surface that satisfies it by other means is not in violation. It exists because the client was
built surface by surface without one, and the result is a table whose geometry is decided by
whatever content happens to be in it: rows that resize as permanents arrive, text that clips at
200% zoom, and a scrollbar inside the battlefield.

It is written in the order the design has to be *derived*, not the order it gets built: what a
player must be able to see, what may give way when there is not enough room, what arrangement
satisfies that at a given shape of screen, and only then how anything is sized.

---

## 1. The bar

| Class | Screens | The commitment |
| --- | --- | --- |
| **Optimized** | Desktop landscape, ≥1280×720 at 100% | A good experience. This is the version the game is designed for. |
| **Possible** | Common phone sizes, and any desktop zoomed to 200% | Every action reachable, every fact obtainable, nothing clipped. Not pleasant; playable. |
| **Unsupported** | Short edge < 320px, or long edge < 480px | Say so plainly, in place of a broken board. |

**Zoom and small screens are one problem.** A browser at 200% zoom does not scale the page — it
halves the layout viewport. 1280×720 at 200% is a 640×360 viewport, which is phone-landscape
territory. Everything in this document that makes a phone possible is the same mechanism that
makes zoom safe, and either can be tested by the other.

**The supported range spans about 25× in area** — 640×360 to 3440×1440. No single composition
covers that. What is constant is not the arrangement; it is the contract below.

---

## 2. The visibility contract

The client's job is to let a player choose among the actions the server offered. So the contract
is: **every valid action is reachable, and every fact needed to choose between them is
obtainable.** Everything else is a question of how much of it is on screen at once.

Three tiers. What moves between them as room runs out is §3.

### Tier 1 — always visible, no gesture

| What | Why it cannot wait for a gesture |
| --- | --- |
| Whose priority it is, and what is being asked | Without it a player does not know whether they are thinking or blocking the game. |
| The action affordance | The way to act must be in a fixed, known place. Hunting for it is the game not responding. |
| Your hand | It is your option set. A move you cannot see is a move you do not make. |
| Both battlefields | The state of the game. Named explicitly as non-negotiable. |
| Every seat's life total | The scoreboard. One glance decides whether you race or stabilize. |
| The stack, whenever it is non-empty | When it is empty it costs nothing; when it is not, it is the most urgent thing on screen. |
| The current step | It changes what every option means. It costs one word. |

### Tier 2 — one gesture, no hunting

Identity and detail of any object drawn on screen; the contents of any public pile; the full
rules text of anything; the turn structure beyond the current step; stop preferences.

"One gesture" means one click, tap, or keypress from where the player already is. It never means
scrolling to find something first.

### Tier 3 — on demand

The log, the settle summary, device settings, the deck builder.

### What "visible" means, quantitatively

A thing is visible when it can be *read*, not when it is present. For a permanent on the
battlefield that means, at minimum:

- **Which card it is** — SAGE ships no card art, so identity is the printed name. It is the one
  thing that cannot be inferred from a frame.
- **Tapped or not** — orientation; legible at any size.
- **Power and toughness**, for anything with them.
- **That it carries counters, damage, or attachments** — the mark, not necessarily the detail.
- **Whether it is actionable right now.**

Rules text, type line, mana cost, and art are *not* in this list. They are tier 2.

**The type rule: fit the text by shrinking it, and only remove it when it hits the floor. Never
truncate it to fit.** The order is shrink → wrap → remove, and truncation is not a step in it.

The floor is **9px effective** for text on a card and 11px for chrome. That is deliberately small,
and it is what XMage proves: at roughly 9px it fits a card's name, cost, type line, keyword
abilities, and P/T into a 72×100 tile, and every one of them is *complete*. Complete-and-small is
readable; large-and-truncated is not. `Troll Asce` is a card you can recognise; `C…` is not
information at all.

The current client has this exactly inverted — it draws text at a comfortable size and then cuts it
off, which spends the space and delivers nothing.

### The seat bar

A seat bar carries a name, a life total, five zone counts, status marks, and commander state. At
360px it cannot carry all of that, and life may never degrade. The resolution:

**Life and status marks are always drawn. The five zone counts fold into one control that expands
in one gesture.** Life is tier 1 and stays tier 1; the counts drop to tier 2, which is where they
belonged — a library count is something you check, not something you read continuously.

The fold is **symmetric**: both seats degrade at the same point and in the same way. Your own
counts are the ones you act on and there is a real argument for keeping them longer, but the cost
is that a player learns two different bars and then has to work out which one they are looking at.
One bar, read the same way at both ends of the table, is worth more than the extra count.

### The action affordance and the hand

Both are tier 1 and on a phone both want the bottom edge, which is the only comfortably reachable
region. They cannot both have it, so they take turns:

**The hand is full height while the game is not asking anything. The moment there is something to
answer, the hand collapses to a peek strip and the dock takes the space it freed.** One gesture
restores the hand — which is what the player does when the action is about a card in it.

This is a deliberate rejection of a permanently-reserved dock. A fixed bar costs hand height at
every size, including the sizes where the hand is already tightest, and it spends that height on
controls that are blank most of the time. The peek strip still shows enough of every card to count
them, so the hand never becomes invisible — it becomes small, and only while something else is
urgent.

The affordance is still in a *fixed, known place*: the bottom band. What changes is how much of
that band it takes, and it only ever grows when there is something to take.

---

## 3. The degradation ladder

Room always runs out somewhere. The design is the *order* in which things give way — stated once
here, so it is derivable rather than hand-tuned per screen.

Applied in order, each step taken only when the one before it is exhausted:

1. **Padding and gaps compress** toward their minimums.
2. **Cards shrink** toward the legibility floor.
3. **Secondary text drops from the card face** — rules text first, then type line, then mana cost.
   Name, P/T, and state marks remain.
4. **Cards overlap within their row**, fanned so the exposed strip is the top-left: the name band.
   A fanned row reads as a column of names. Pitch tightens as the count grows.
5. **Rows merge.** Creatures, other permanents, and lands stop being separate rows and become one.
6. **Card faces become chips.** Below a row height of 100px effective a card tile is no longer a
   card — it is a landscape chip carrying name, P/T, and state marks, with the face one gesture
   away. Rendering a 60px "card" is the current failure mode: it has the shape of information
   without being readable.
7. **Rails collapse.** The turn strip becomes a single current-step chip; the stack becomes a badge
   carrying the top item's name and a count, expanding on gesture.
8. **The side column becomes a drawer.** Preview, log, and settle move behind one gesture.

**Merging rows before chipping cards is deliberate**, and it is the one place in this ladder where
the order is load-bearing rather than merely sensible. A split battlefield is budgeted for two rows
of cards; merged, the same height buys one row of *twice the size*. So a field that cannot afford
two 100px rows can very often afford one, and the choice is between a merged row of readable cards
and two rows of chips. The card wins — losing the split costs the scan by category, and losing the
face costs the card. It is also what makes §4's Tall band possible: that band promises rows merge
*and* cards stay cards, and only this order delivers both.

### What never degrades, at any size

- Whose priority it is, and what is being asked.
- That an action is available, and a fixed place to take it.
- Every seat's life total.
- The top item of a non-empty stack, by name.
- The ability to identify any drawn object in one gesture.
- **No region of the board ever scrolls.** A board that has to be scrolled to be seen is not a
  board. When content exceeds the room, the ladder above applies — it never falls through to
  overflow. (Piles opened on demand are not the board and may scroll.)

---

## 4. Arrangements

### The invariant

**Reading order is always: opponent, board, you, hand — top to bottom.** It does not rearrange for
any screen. Density changes; the spatial metaphor does not, so what a player learns on a desktop
still holds on a phone. Everything else about an arrangement is negotiable.

### Bands

Chosen by the shape of the viewport, not by device.

| Band | Shape | Arrangement |
| --- | --- | --- |
| **Wide** | ratio ≥ 1.3, height ≥ 640 | The optimized version. Seats stacked vertically, turn rail at the left edge, stack rail at the right, side panel as a drawer. Card faces throughout. |
| **Ultrawide** | ratio ≥ 2.0 | Same composition. Extra width goes to board rows — more cards before overlap — with the board's content held to a maximum width so a glance does not have to travel the whole screen. Rails stay pinned to the edges. |
| **Square** | ratio 0.8–1.3 | Same vertical order. Turn rail becomes a horizontal strip beneath the header; stack becomes an edge tab. Ladder steps 1–4 as needed. |
| **Tall** | ratio < 0.8 (phone portrait) | Vertical order preserved and comfortable — height is what portrait has. Rails collapse (step 7), zone counts fold into the seat bars, rows merge, cards overlap early. Cards stay cards. |
| **Short** | height < 480 (phone landscape, 200% zoom on a small desktop) | The hard case: height is the scarce resource. Full ladder, including chips (step 5). Hand is a peek strip by default and expands on gesture. |

At the Tall and Short bands the hand and the dock trade the bottom band as §2 describes: hand full
height while nothing is pending, peek strip the moment something is. At Wide, Ultrawide, and Square
there is room for both and the dock sits above the hand permanently, as it does today.

**Height, not width, decides whether a permanent is a card.** Card faces survive while their row
is at least 100px tall. That is why portrait phones keep cards and short-landscape does not — and
it is a rule that can be evaluated, not a judgment call per screen.

---

## 5. Geometry

Everything below is a function of available space and object count. Nothing is a function of
content: no region's size is ever set by the text inside it, which is the defect the whole
document exists to remove.

**Scene units.** The table is laid out in absolute coordinates, not flow. A region's position is
computed, never the residue of what came before it. This is what makes a scrollbar impossible
rather than merely discouraged.

**Card width within a row** — given the row's width `W`, a count `N`, minimum gap `g`:

```
ideal   = the band's designed card width
fitted  = (W - (N-1)·g) / N
width   = clamp(FLOOR, fitted, ideal)
```

When `fitted < FLOOR`, the row switches from spacing to overlap and pitch becomes
`(W - width) / (N - 1)`, bounded below by the width of a legible name strip. Cards never shrink
below `FLOOR` — they overlap instead.

**Card proportion is 63:88 (0.716)**, the printed proportion, at every size. A permanent tile is
that plus a fixed allowance for state marks.

**Sizes, effective px** (subject to review — these are the numbers the rest of the spec is
checkable against):

| | Minimum | Designed |
| --- | --- | --- |
| Permanent tile | 72 × 100 | 130 × 182 |
| Hand card | 100 × 140 | 150 × 209 |
| Stack item | chip | 130 × 182 |
| Chip (below card floor) | 96 × 30 | — |

The 72×100 minimum is measured off XMage, which fits a complete name, cost, type line, keyword
line, and P/T into that box. The hand's minimum is larger because the hand is where a player
*chooses*, and a name there may never be abbreviated at all.

**Region heights** are fractions of scene height with stated minimums, allocated in contract
order: the tier-1 items are satisfied first, and what is left goes to the board rows. An empty row
costs nothing — a seat with no permanents yields its height to the seat that has them, which is
the opposite of today, where an empty opponent board reserves 200px to say "No permanents."

---

## 6. The card

The card is the atom: the hand, both battlefields, the stack, opened piles, and the deck builder
all draw it, so a rule fixed here is fixed everywhere. It is also where the client fails most
visibly today — a hand in which every card reads `C…`, `Dis…`, `L…` is not a degraded hand, it is
an unusable one.

### The principle

**The name owns its band. Nothing shares that row.**

SAGE ships no card art, and ADR 0012's player-supplied art is opt-in and may be absent, so the
printed name is the *only* thing that identifies a card. Anything competing with it for horizontal
space is competing with the card's identity — and today the mana cost wins that fight, which is how
a 100px card spends 45px on `{1}{U}{U}` and leaves 20px for the name.

### Anatomy, from the top

1. **Name band.** Full width, and it is fitted rather than clipped: shrink toward the 9px floor,
   then wrap to a second line, and only then abbreviate. **In the hand, a name is never abbreviated
   at all** — that is where a player chooses, and XMage's hand shows `Simian Spirit Guide` and
   `Temple of Deceit` complete at ~118px wide. On the battlefield an abbreviation must still leave a
   recognisable card: XMage's `Troll Asce` is one, `C…` is not.
2. **Cost.** Overlaid on the art's top-right corner, out of the name's row. Pips are graphic and
   read at sizes text does not — no baseline, no wrapping, no hyphenation. Below the chip threshold
   (§3, step 5) the cost drops entirely: the server already states what is playable through
   `valid_actions`, so cost is reference rather than a decision input. This is a presentation
   judgment and not a rules one — the client still computes no affordability.
3. **Art window.** Fixed proportion, the largest single element. Procedural composition by default,
   a player-supplied illustration over it when one is present.
4. **Type line.** Degrades **by rule, not by ellipsis**: drop the subtype after the em-dash first,
   then supertypes, leaving the card type. `Legendary Creature — Elf Druid` becomes `Creature`,
   never `Legendary Cr…`.
5. **Rules text.** Drawn only when it fits at its floor. **An empty text box is never drawn** —
   today a hand card renders a blank black band where the text did not fit, which is a container
   outliving its content.
6. **State marks.** P/T or loyalty, counters, marked damage, tapped, and whether the object is
   actionable. These are the last things to go, and they are graphic wherever they can be.

### Tapped

**Tapped is drawn as an overlay mark on an upright card, not as a rotation.** XMage does exactly
this on a crowded board — a mark across the face — and it is why its permanents stay readable in
the state they spend most of the game in. Rotating a card whose identity is *text* destroys the
identity to communicate one bit, and it costs a landscape footprint on top of that.

Two consequences. The name survives while tapped. And **the "Tapped" badge disappears**: the mark
is the statement, and today the badge is a rotated pill laid over the art of a card whose name is
already sideways. The word stays for assistive technology, which can see neither a mark nor a
rotation.

Rotation may return at the Full presentation, where the card is large enough that nothing is lost.

### The four presentations

Not scaled copies of one another — each drops what it has no room for, in ladder order:

| | Draws |
| --- | --- |
| **Full** — preview, inspector | Everything, including complete rules text. |
| **Designed** — the hand, and a battlefield with the height to spare | Name, cost, art, type line, rules text if it fits at floor, marks. |
| **Compact** — most battlefields, crowded rows, the Tall band | Name, art, marks. No type line, no rules text; cost only if the width allows. |
| **Chip** — below a 100px row | Name, P/T, marks. No art. Landscape. |

Which one is drawn follows from the box available, never from the call site: a battlefield card and
a hand card of the same size are the same card.

**`compact` is the normal battlefield presentation, not a degradation.** Run §5's numbers against a
real screen: two rows of 182px cards, for two seats, plus a 217px hand and the chrome, needs more
than 1080px of height. A 1920×1080 desktop — the most common one there is — therefore draws
`compact` permanents at roughly 155px, and `designed` first appears around 1440p. That is the right
answer rather than a shortfall, and it is exactly the density §6 argues for: a battlefield full of
complete 155px cards beats a battlefield of half as many beautiful ones. `designed` on the
battlefield is what a very tall screen buys, and the hand is where it is the default.

### Density

The other half of what XMage proves. Its permanent tile is ~72×100 and is *full* — near-zero
padding, a small art share, and text everywhere else — which is how it fits eleven permanents to a
row, three rows, a seven-card hand, and both rails on one screen with nothing cut off.

Ours are the inverse: a 108px tile that is mostly padding and art window, showing `C…`. **The art
window takes the space that is left, not a fixed share** — it is the one element that degrades to
nothing without costing a fact, since SAGE ships no art and identity lives in the name.

### The check this has to pass

**No presentation abbreviates a name in the hand, at any supported size**, and no presentation
anywhere abbreviates below what keeps a card recognisable. At the 72px minimum a 9px name yields
roughly 13 characters a line and 26 across two — enough for `Exclusion Ritual` and
`Vraska, Golgari…`. If a name cannot be fitted at the floor, the box was sized wrong; that is a
sizing defect to fix in §5, not a permitted state.

---

## 7. Type

One scale, in scene units, so every size moves together and no element can clip independently.

| Role | Floor | Designed |
| --- | --- | --- |
| Card name | 9px | 13px |
| Card type line, rules text | 9px | 12px |
| P/T, counters, life | 9px | 14px (life larger) |
| UI labels, controls | 11px | 13px |
| Log, secondary prose | 10px | 12px |

Card text floors lower than chrome text on purpose: a card is read at a glance and in place, and
XMage demonstrates that ~9px is legible for it. Chrome is read across the screen and holds its
larger floor.

Fitting order is **shrink → wrap → abbreviate**, and the third step is a defect anywhere a player
chooses from what they are reading.

---

## 8. What this retired

Two constraints in `clients/web/AGENTS.md`, one sentence in `docs/brief.md`, and the header comment
in `game.css` fixed the composition to a single desktop shape and named a bounded scrolling region
as the thing the geometry protects. All of it was bootstrap-era scaffolding written to get the
project moving, and none of it was ever decided as design — but because it lived in `AGENTS.md` it
read as a decision, and it was defended as one.

They were deleted rather than rewritten, and replaced by a pointer here. Two things survived because
they *are* decisions and not scaffolding: the client is dark, for a stated reason, and WebGL stays
out until something demonstrates it is needed. DOM stays too, for the reason #640 gave and because
there is a great deal of UI here; what changes is that it stops being used as a document.

The general rule this is an instance of: **do not write a provisional constraint into `AGENTS.md`.**
The circumstance that justified it is not recorded next to it, so what survives is a rule with no
expiry, and the next reader inherits a decision nobody made.

---

## 9. The lobby

**The lobby is a client shell, not a table and not a room.** A persistent navigation region, a
content region beside it, and the pre-game surfaces as content within it: tables as rows, decks as
a locker, the catalog as the builder's own space.

The register was a real choice and the other two were tempting. A pre-game *table* — the board seen
empty, with chairs you take — is continuous with the game and reuses the scene model outright, and
a *room* you walk into is the strongest sense of place either would give. Both lose to the same
fact: the deck builder is the densest surface in the product, it is unavoidably a list of hundreds
of cards with filters over it, and it lives here. A spatial lobby has to hand off to a conventional
one the moment a player builds a deck, and that seam is worse than not having the metaphor. The
shell absorbs the builder instead of fighting it, and it is the register that scales when there are
many tables, many decks, and many formats.

The atmosphere is carried by the *drawing*, not the metaphor: same dark ground, same card
component, same type. A catalog card and a battlefield card are the same card — that is what makes
the front door read as the same product as the table, and it is the part the current lobby fails.

What this settles:

- The lobby does **not** use `scene()`. It is a conventional responsive composition, and it is the
  one place where that is correct — its content is a list whose length the server decides, not a
  board whose geometry a viewport decides.
- Everything else in this document still applies to it: nothing clipped, nothing below the type
  floor, text fitted rather than truncated, and no native form control.
- **A list in the shell may scroll.** The board may not. These are not in tension: §3's rule is
  about the board, and the shell is not the board. Say which one a region is before deciding.
- Tier 1 before a game exists: which table you are at, who is in it, what it is waiting on, and how
  to leave.

The composition itself — what the navigation carries, how the builder packs a catalog, what a table
row shows — is a design pass of its own, tracked as a child of #652 and written into this section
before any of it is built.

---

## 10. Open questions

None. Questions raised after this document became binding go here.
