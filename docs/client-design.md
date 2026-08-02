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

### 2.1 Say it once

Everything below is about *what must be on screen*. This is about how much of it to write, and it
applies to every surface in the client — the table as much as the lobby.

1. **A control says what it does. It does not explain itself.** A description of an option belongs
   where a player asks for it — on the option, at the moment of choosing — not printed beside the
   control forever.
2. **State is shown, not narrated.** If a fact needs a sentence to be legible, the thing drawing it
   is wrong; the sentence is not the fix.
3. **No identifier a player has no use for.** A room id, an object id, a seat id: those belong in a
   log.
4. **One box.** A panel inside a panel inside a panel, each with a heading, is a document's way of
   grouping. Grouping is what space and alignment are for.
5. **Ask a question once.** A heading, a legend, and a set of buttons that all say the same thing
   are one question written three times, and the buttons are the only one a player acts on.

The failure these describe is a single one, and it is not fixed by writing better sentences: **a
surface that narrates is a surface that has not been drawn.** Prose is what a document uses when it
cannot show something. This client can show it.

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
truncate it to fit.** Truncation is not a step in it.

The order is what gets *sacrificed*, and it is: type size, then line count, then completeness. But
"shrink then wrap" must not be read as *shrink all the way to the floor before considering a second
line* — that produces a wider card with smaller text than a narrow one, which is indefensible.
**Wrapping is one of the ways text fits at a given size, not a step after shrinking.** Stated
operationally: take the largest size at which the text fits within the lines the box can afford,
preferring fewer lines at equal size. Shrink only when no line count works at that size, and
abbreviate only at the floor.

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

### More screen is never a worse board

**For a fixed board, a card is never smaller on a larger viewport than on a smaller one.** Card size
must be non-decreasing in both viewport width and viewport height. This is a property, it is
testable by sweep, and it is not negotiable against any other rule here.

It has to be said because the ladder reads as though more rows are better, and they are not
inherently. Splitting into creature, other, and land rows buys a *scan by category*; it costs card
size, because the same height divided three ways draws smaller cards. A field with just enough height
to squeeze three rows past the 100px floor therefore draws a **worse** board than one with slightly
less height that merges — three rows of clipped 75px cards against one row of complete 130px cards.
That is the ladder read as a checklist rather than as a preference order.

So **row count is chosen to maximise card size, not to maximise rows.** The split is kept while it
is affordable and given up when it is not, on the same principle as §2's type rule: take the largest
size that works, and give up structure to get it. Losing the split costs the scan by category;
losing the card's text costs the card, and the card wins.

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

### Regions are sized by the viewport. Counts are absorbed by the cards.

This is the whole rule, and everything else in this section is a consequence of it.

**A region's height is a function of the viewport alone.** How many permanents a seat controls, how
many cards are in a hand, how deep the stack is — none of it changes the size or position of any
box. The layout a player learns on their first turn is the layout they have on their twentieth.

**A count is absorbed by the things inside the region, by the formula above.** Fifteen permanents in
a field sized for six do not make the field taller; they make the cards smaller, and then overlapped,
and then chips. That is what `clamp(FLOOR, fitted, ideal)` is for. The cards scale to the box; the
box never scales to the cards.

So **the two battlefields are always the same height**, whatever is on them, and the dividing line
across the middle of the table does not move for any game event. A seat that wipes the opponent's
board does not watch its own permanents jump to a new size and a new place. A seat playing its first
creature does not shove the other half of the table upward.

An earlier draft of this section said the opposite — that an empty row costs nothing and yields its
height to the seat with permanents on it. That is content-driven layout wearing a different hat, and
the argument for it (a crowded board needs the room) dissolves once the cards are doing the scaling:
the room was never the answer.

It is also wrong about what an empty battlefield *is*. "My opponent has nothing" is one of the most
important facts in the game, and the way to read it is by looking at an empty half of the table —
not by noticing the absence of a region. What the current client gets wrong is narrower and is still
worth fixing: it spends a card row's height printing the sentence **"No permanents."** The sentence
goes; the place stays.

**The stack is the exception, and the distinction is the point: the stack is an event, a battlefield
is a place.** An event that is not happening takes no room, so an empty stack has no box and the
board takes the width back. A place at the table does not stop existing because nobody has put
anything on it. Note what this does *not* license: the stack's box is decided by whether it exists,
never by how deep it is. A seven-deep stack and a one-deep stack get the same rail, and the depth is
absorbed by the items in it exactly as a permanent count is.

The hand-and-dock trade of §2 is the other departure, and it is deliberate rather than an oversight.
It is driven by **whether the game is asking you something** — a change of mode — and not by how
much content there is. That distinction is the whole test: a layout may respond to *what the player
is doing*, and may not respond to *how much stuff there is*.

Region heights are otherwise allocated in contract order: the tier-1 minimums are satisfied first,
and what is left over goes to the board.

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
2. **Cost.** **In the name band, at its trailing edge** — where a printed card puts it, and where a
   player's eye already goes. Pips are graphic and read at sizes text does not: no baseline, no
   wrapping, no hyphenation.

   An earlier draft overlaid the cost on the art's top-right corner to stop it eating the name. That
   solved the right problem the wrong way. The name was losing because the band was divided before
   either part was fitted, and the fix for that is the fitting policy — not moving the cost somewhere
   nobody looks, over art it tints against, in a corner that means nothing.

   **The band is fitted in priority order: the name first, against the width the cost would leave;
   and if that would push the name below its floor, the cost goes — never the name.** Identity
   outranks reference, which is §2's tier ordering applied inside one band instead of across the
   screen. Below the chip threshold (§3) the cost is gone regardless: the server already states what
   is playable through `valid_actions`, so cost is reference rather than a decision input. Dropping
   it is a presentation judgment and not a rules one — the client still computes no affordability.
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

**Tapped is a quarter turn.** The card rotates 90°, and it animates into it, because that is what
tapping *is* — the single most universal convention in the game, the one every player and every
client already shares. A pattern laid over an upright card is a private language nobody has learnt,
and no amount of internal consistency makes it read.

An earlier draft of this section replaced the turn with a hatch, reasoning that rotating a card
whose identity is text takes the text sideways, and that a landscape footprint is a case the row
packer would have to model. The first is true and is a real cost; it is simply outweighed. **A
tapped permanent is one you are not currently reading** — you know what it is, and what you need
from it at a glance is precisely the one bit the turn communicates better than anything else can.
The second argument has expired: the packer exists, and reserving the rotated footprint is now an
ordinary part of laying out a row rather than a special case bolted onto flow layout.

Two things follow.

**The row reserves the turned footprint**, so a tapped permanent never collides with its
neighbours and a row does not reflow when something taps. Height is what a rotated card costs, and
it is charged whether or not anything is currently tapped — otherwise the board moves when a
creature attacks, which §5 forbids.

**The "Tapped" badge stays deleted.** The turn is the statement; a pill that says the word is the
same fact twice (§2.1). The word remains for assistive technology, which can perceive neither a
turn nor a mark.

**At the chip tier there is no turn.** A 96×30 chip is already landscape, so rotating it says
nothing. There, and only there, tapped is a mark — and it must be a pattern or a glyph rather than
a tint alone, because every fact has to survive without colour.

### The four presentations

Not scaled copies of one another — each drops what it has no room for, in ladder order:

| | Draws |
| --- | --- |
| **Full** — preview, inspector | Everything, including complete rules text. |
| **Designed** — the hand, and a battlefield with the height to spare | Name, cost, art, type line, rules text if it fits at floor, marks. |
| **Compact** — most battlefields, crowded rows, the Tall band | Name, marks, and the type line where it fits. No rules text; art and cost only if what is left allows. |
| **Chip** — below a 100px row | Name, P/T, marks. No art. Landscape. |

Which one is drawn follows from the box available, never from the call site: a battlefield card and
a hand card of the same size are the same card.

The type line survives into `compact` because §5's 72×100 target is measured off a tile that has
one, and dropping it to protect the art window would be backwards — art is the element that
degrades to nothing without costing a fact. What a presentation names is the *order* things leave
in, not a fixed manifest: below the name and the marks, everything is drawn while it fits and
dropped whole when it does not.

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

## 6.5 The dock, and how a question is asked

Every region got a degradation ladder in §3. The dock's *contents* never did — so when a question
is bigger than the dock's band, `overflow: hidden` cuts it, and a player loses the controls they
were being asked to use. That is the defect; the verbosity below is why the question was too big in
the first place.

### What is on screen today

Asking a player to keep or mulligan currently states the same fact four times:

| | |
| --- | --- |
| the dock's tone line | *"The game is waiting on your answer"* |
| a heading | *"Keep or mulligan"* |
| a fieldset legend | *"Keep this hand or take a mulligan?"* |
| two buttons | `Keep this hand` · `Mulligan` |

**The buttons are the question.** The other three are chrome, and together they push the row of
controls beneath them out of the band. This is §2.1 rules 4 and 5, in the game rather than the
lobby.

### The board answers; the dock stays small

**A question about objects on the screen is answered on the screen.** The objects that can answer it
are highlighted, clicking one answers it, and the dock carries only what the board cannot: a tally
of what has been chosen against what is needed, the way to commit, and the way to cancel.

That is how the question is asked at a real table — you point at the creature — and it is already
what `interaction.ts` does. What changes is that the dock stops *also* listing every subject as a
button, which is what makes it grow without limit.

The dock is then bounded by the *shape* of a question rather than by its size: a tally and two
controls is the same height whether there are two legal blockers or twenty. Nothing has to clip
because nothing has to grow.

**The fallback is not optional.** A subject no surface draws — a card in a face-down pile, an
ability with no permanent, a choice that is not about an object at all — has nothing to click, and
those subjects must remain reachable. They stay in the dock as controls, and the existing
disclosure of every action stays exactly as it is: it is the guarantee that no action is reachable
only by finding its object.

### The rules

1. **The prompt is drawn once, in one place, or not at all.** Where the options state the question,
   the options *are* the question.
2. **The tone is shown, not written.** A dock that is asking looks like it is asking; it does not
   also say so in a sentence.
3. **A question never scrolls and is never clipped.** If it does not fit, the fault is that
   subjects are being listed that the board could have answered.
4. **Everything reachable by pointer is reachable by keyboard**, including answering on the board.
5. The dock's own band is `scene()`'s, as with every other region. It responds to *whether the game
   is asking* — a change of mode — and never to how much there is to ask about (§5).

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

### 9.1 What is wrong with it now

Worth recording precisely, because every rule below is an answer to something on this list.

- **`Card art` is a top-level button** in the header, competing with the game for the most expensive
  space on the screen, to reach one device preference.
- **`Display name` is a bare input and a `Set name` button, parked in the chrome permanently.** It
  is a one-time setup task that every screen pays for, forever.
- **Everything is narrated.** *"Plays a random legal action each decision. A simple sparring
  partner."* · *"Waiting on — Seat 1 — No deck yet · Seat 2 — Nobody here yet"* · *"1v1 · 2 seats ·
  public · id `r0`"*. The last of those prints a room identifier no player has any use for.
- **Three nested boxes** — page, then table, then Seats, then Deck — each with its own heading, to
  carry about six facts.
- **A full-width native `<select>` whose arrow is clipped at the right edge at 120% zoom.**
- **Roughly 40% of the screen is empty** below it all.

### 9.2 The rules the lobby is drawn by

These were written for the lobby and they are **not lobby rules** — they are §2.1, applied here.
Read them there. The two that are specific to a pre-game screen: **setup happens in setup**, so a
task performed once does not live in the chrome of every screen; and **everything about the device
is one destination** rather than a scatter of buttons (§9.6).

### 9.3 Connect

The first screen. It exists because a player should arrive at the lobby already being somebody,
rather than finding an input box in the header asking who they are.

- **Name**, prefilled with the last one this device used. A returning player presses one key.
- **Server**, a list of predefined servers each carrying its **region**, plus a quick localhost
  entry and a custom address. The list is client-side configuration — the protocol has no server
  directory and this document is not proposing one — and the custom entry is what keeps that from
  being a limitation.
- **Connect.**
- The gear reaches settings from here, so card art can be set up before ever joining a table.

Neither field is a wire change: `hello` carries a token and nothing else, and the name is set by
the command the client already sends.

### 9.4 The shell

A navigation rail and a content region. The rail carries the destinations — **Play**, **Decks**,
**Settings** — with the player's identity and the gear at its foot, out of the way of the thing
they came to do.

**Which destination you are on is the client's answer. Which contract you are on is the server's.**
That distinction is what keeps "no client-held phase" true: a `GameView` arriving replaces the
whole shell, because the contract changed; choosing Decks does not, because it did not.

At narrow widths the rail becomes a bar; the destinations do not change and neither does their
order.

### 9.5 Play

**Land on the tables list.** A row is: what the table is, how full it is, and one button. Occupancy
chooses which advertised command the button leads with, exactly as `lobby.ts` already decides — no
new rule.

Joining replaces the list with the table you are at, which shows the seats, what each is waiting
on, your deck, and how to leave. **Waiting-on is drawn on the seat it belongs to**, not summarised
in a sentence underneath. The AI kinds and the starter decks come from `CatalogView` as they do
now; what changes is that choosing one is not a native `<select>` and its description is not
printed beside it.

### 9.6 Settings, and the art section

One destination, sectioned. Card art stops being a button in the header and becomes a **section
about obtaining and managing art**: which source to use, what is currently cached and how to clear
it, card backs, and symbols. That is a real surface with real state in it, and it was never going
to fit behind a header button.

The rules of ADR 0012 are unchanged and this section is where they become visible to the player:
the fetch is theirs, the cache is their device's, and nothing is bundled, served, proxied, or
redistributed.

### 9.7 Decks

The deck builder is the densest surface in the product and has its own design pass. What is settled
here: it is a destination in the shell, reachable without being at a table; a saved deck is
**device-local**, in the manner of ADR 0012's art preference, and is an *input* to `submit_deck`
rather than a substitute for it; and `deck.ts` still computes no legality — the verdict stays the
server's `LobbyRejection`.

---

## 10. Open questions

None. Questions raised after this document became binding go here.
