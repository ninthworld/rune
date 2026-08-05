# The client, designed

**Status: binding. This is the layout authority for `clients/web`.** Where any other file states a
layout constraint that contradicts this one, this one wins and the other is stale.

**It now describes `clients/prototype`, and the prototype is the authority above it.** The design
was derived here in prose and then built in a sandbox where building it cost nothing, and the
sandbox answered a number of questions this document had only reasoned about. Where the two
disagreed, the prototype won and this document was changed to match it — every such reversal is
marked below with what it replaced, because a rule that was argued for at length and then dropped
is worth more as a correction than as a deletion. What the prototype has not exercised is marked
too: it is still the rule, and it is not yet evidence.

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
| Every seat's battlefield | The state of the game. Named explicitly as non-negotiable. |
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

**The type rule: every run of text is set at the largest size that fits its own box, and nothing is
ever truncated.** Truncation is not a step in it, and neither is removal — a card that is drawn
draws all of it.

**The card is one drawing, scaled, and its text is fitted in the card's own grid rather than in
device pixels.** §6 states the mechanism; what matters here is the consequence: a name that fits at
one card size fits at every card size, because the box and the type shrink together. There is no
device-pixel floor at which a run of text changes what it does, and there is no size at which a
card stops being a card and becomes something else.

That is the reversal, and it is a large one. Earlier drafts set a **9px effective** floor for card
text, made it the switch below which a tile became a chip, and derived a whole tier of the
degradation ladder from it. The prototype has neither, and reads better without them: fitting each
run to its own box in the card's grid means the drawing is always complete and always in
proportion, where a floor plus a chip meant a board could change *what it was made of* between two
window sizes.

What survives from that argument is the part that was actually load-bearing, and it survives
because it is the reason the fitting policy is what it is: **complete-and-small is readable;
large-and-truncated is not.** XMage is the demonstration — a complete name, cost, type line, keyword
line, and P/T inside a 72×100 tile. `Troll Asce` is a card you can recognise; `C…` is not
information at all. The 72×100 tile stays in §5 as the size the maintainer wants to be told about.
It is no longer a floor under anything.

**What the prototype has not answered: how small is too small.** Nothing stops a row from being
short enough that its cards are illegible, because nothing in the prototype measures a card against
a legibility threshold — the row is sized by the viewport and the card takes what it is given. The
old floor at least asked the question. Treat a genuinely unreadable card as a defect in the region
allocation above it (§5), and if a threshold is wanted later, it is a *report* and not a switch
(§3, "Scale first. Remove last.").

### The seat bar

A seat carries a name, a life total, five zone counts, floating mana, and status marks, in a bar
down the leading edge of its own half of the table. It carries all of them **at every size**, and
nothing in it folds.

**A zone is a drawn glyph and a count, not a word and a count.** That is what makes the whole set
fit: the labels were the reason the bar wanted more height than a seat has, and a library, a
graveyard, an exile and a command zone are four shapes a player learns once. Each is a bounded box
rather than a run of text, which is also what gives a target or a drop somewhere to land later.

**Which zones exist is a property of the game, not of a seat.** Either every bar has a command zone
or none does. A seat missing one would shift every glyph after it, and then the same drawing would
sit in a different place on the bar beside it.

This reverses two rules at once. An earlier draft required the counts to **fold into one control**
below a threshold, on the reasoning that a bar could not carry five of them and life; and it
required the counts to be a **labelled stack, "not a row of glyphs to decode"**. The prototype
carries all five as glyphs at 110px wide, and at 74px on a phone, with room left for the mana pool
— so the constraint the fold answered does not arise, and the thing the fold was going to cost
(learning where a count is, then learning a second bar that hides it) is not paid. Glyphs are what
made that possible, which is why the second rule went with the first.

### The action affordance and the hand

Both are tier 1, and both live in the bottom band at every size: **the action bar sits immediately
above the hand, permanently.** It is a fixed, known place in the strongest sense — it does not move
and it does not change size with the question.

This reverses an earlier draft that had them **trade the band by mode** — a full-height hand while
nothing was pending, collapsing to a peek strip the moment the game asked something. The argument
was that a permanent bar costs hand height at every size and spends it on controls that are blank
most of the time. In the prototype the bar is never blank: it always says what the game is asking
and offers the way to answer, and where there is nothing to answer it says which step you are in
and offers `Done`. A region that always has something to say does not need to be traded for, and a
band that changes size when the game asks you something is a board that moves underneath the cards
you were reading.

The hand takes a fixed share of the table's height, and its cards are sized from the band they were
given (§5) rather than the band from the cards.

---

## 3. The degradation ladder

Room always runs out somewhere. The design is the *order* in which things give way — stated once
here, so it is derivable rather than hand-tuned per screen.

### Scale first. Remove last.

**The first answer to "it does not fit" is always to make it smaller — never to take something
away.** Removal is what happens when scaling has genuinely run out, and it is a failure to be
reported, not a design move to reach for.

Earlier drafts of this section had that backwards. They read as a menu of things to drop, and they
were implemented faithfully: rules text that would not fit was deleted rather than set smaller; the
art window was eaten to make room for text; creature and land rows were merged so the remaining
cards could be drawn at full size. Every one of those is the same mistake — **trading a feature away
to keep something else at its preferred size**, when scaling both would have kept both.

So the rule the ladder below serves:

- **Cannot fit the art and the rules text? Set the text smaller.** Never eat the window.
- **Cannot fit three rows? Give each row less height.** Do not merge the rows.
- **Cannot fit the cards across the row? Pile the copies, then let the row pan.** Do not shrink the
  cards to fit a count — width is not an input to a card's size at all (§5).
- Only when everything has been scaled as far as it goes does anything come off.

**A size that seems too small is a review threshold, not a licence to drop.** The client still draws
it, and the maintainer is the one who decides whether it has gone too far. A silent removal hides the
question; a small card asks it. (The prototype reports nothing — §10, question 1.)

### The ladder

Applied in order, and **each step is a last resort reached only when scaling within the step before
it is exhausted**:

1. **Padding and gaps compress** toward their minimums.
2. **Everything on the card scales together** — the tile, its type, its art window, its plaque. The
   card is one drawing and it has one size (§6). Proportions hold; nothing is dropped to protect
   anything else's preferred size, because nothing on the card can be dropped at all.
3. **Identical permanents pile.** Untapped copies of the same permanent occupy one slot, cascading
   downward so the row reads as a ladder of title bars you can count. A pile is fitted to the row's
   height, so four lands are exactly as tall as the creature beside them (§5).
4. **The row pans.** A row too full for its width keeps its cards at the size the row's height gave
   them and scrolls horizontally, masked at whichever edge has more beyond it. This is the board's
   only admitted overflow and it is deliberate — see "What never degrades" below.
5. **Rows merge.** One row per group the server's `card_types` produced; that count falls only when
   a field cannot give each row a drawable tile at all. This is **late**: the scan by category is
   what a player uses to read a board at a glance, and a shorter row of smaller cards is nearly
   always the better trade.
6. **The side column becomes a drawer**, and can be dismissed outright at any width. Preview,
   helpers, stack, log and chat move behind one gesture, and the board takes the column back.

**Steps 4 and 5 of the old ladder are gone.** They were: *secondary content leaves the card face,
in this order — rules text, then art, then type line, then mana cost*; and *card faces become
chips*. Neither exists in the prototype, because the card is a single drawing whose parts are
fitted to boxes in its own grid: there is no size at which it has "room for the name but not the
type line", so there is nothing for those steps to describe. The fanned overlap they sat beside is
gone too, replaced by the pan in step 4.

That is the largest single simplification the prototype made. What it costs is the graceful
sub-70px tile the chip was invented for; what it buys is that a board is made of the same thing at
every size, and that no threshold can make a window resize change what a permanent *is*.

### The split is kept, and the cards get smaller

**Creatures stay in their own row and everything else sits behind them, and the cards shrink to make
that possible.** The scan by category is how a player reads a board at a glance, and it is worth far
more than any particular card size. Merging is step 5 of the ladder — a late resort, reached only
when the cards have already been scaled as far as they go.

An earlier draft said the opposite: *"row count is chosen to maximise card size… losing the split
costs the scan by category, losing the card's text costs the card, and the card wins."* That framing
assumed one of them had to lose, which is the mistake this whole section now exists to name. Neither
has to. Two rows of smaller cards is two rows.

**Two rows, not three, and that is a reversal.** Artifacts, enchantments and planeswalkers had a row
of their own, which cost the two rows a game is actually played in a fifth of the board each and, on
the common board that has none of them, drew a dividing line across the field for nothing. They sit
in the back row now, with the lands: it is the row of things that sit there, and a player scanning
for a blocker is scanning the front row either way. The gain is not only the height — it is that the
field's boxes no longer depend on *what kind of permanent is in the game*, which was the last thing
on the board whose presence could change a region.

### More screen is never a worse board

**For a fixed board, nothing is smaller or less complete on a larger viewport than on a smaller
one.** Card size, and the amount drawn on a card, are both non-decreasing in viewport width and
height. This is a property, it is testable by sweep, and it is not negotiable against any other rule
here.

It is stated separately because it is what catches a ladder applied as a checklist. Crossing a
threshold upward must never take something away — not a row, not the art window, not a line of rules
text. If more screen ever produces less board, the arrangement is wrong, not the screen.

**There is no admitted exception any more.** An earlier draft carved one out for the card's title
bar — where the name and the cost share a box, no ordering leaves both non-decreasing, so the
lower-ranked one gives. That was a consequence of fitting both parts in device pixels. On this card
both are drawn in the card's own grid, the pips take a constant share of it, and every part of every
card is therefore monotone in one variable: the card's size (§5, §6). The property holds without a
carve-out, which is the strongest form it could take.

### What never degrades, at any size

- Whose priority it is, and what is being asked.
- That an action is available, and a fixed place to take it.
- Every seat's life total.
- The top item of a non-empty stack, by name.
- The ability to identify any drawn object in one gesture.
- **No region of the board ever grows a scrollbar, and no region of the board ever scrolls
  vertically.** A board you have to scroll *down* to see is not a board: the vertical extent is the
  table, and every seat, both rows of every field, the hand and the chrome are on screen at once at
  every supported size. When vertical content exceeds the room, the ladder applies — it never falls
  through to overflow.

**A row pans sideways, and that is the one exception.** An earlier draft stated the rule as *no
region of the board ever scrolls*, full stop, and paid for it with the fan: a row too full for its
width overlapped its cards so the exposed strip was the name band. The prototype pans instead — the
row keeps its cards at full size and slides, with a masked edge as the tell that there is more and
a grab cursor as the invitation. It never draws a scrollbar, because a bar would eat the height the
cards are sized from and a bar across the board is a defect in its own right.

Why the reversal: a fan is a worse answer to the same problem than it looks. Thirty fanned cards are
thirty cards you cannot read, thirty overlapping name bands you have to count along, and a pointer
target one strip wide; thirty panned cards are the same thirty cards at full size, of which you see
as many as the row is wide. The board is still fully readable — it is fully readable *in two
gestures instead of one*, and only on a row that has more permanents than a screen can hold at a
readable size, which is the case where one gesture was never going to be enough. (Piles opened on
demand are not the board and may scroll in either direction.)

---

## 4. Arrangements

### The invariant

**Reading order is always: opponent, board, you, hand — top to bottom.** It does not rearrange for
any screen. Density changes; the spatial metaphor does not, so what a player learns on a desktop
still holds on a phone. Everything else about an arrangement is negotiable.

### One arrangement, varied only under duress

**A thing is in the same place at every size unless it cannot be.** Sameness is the default, and
every difference between one screen's arrangement and another's has to be paid for by a stated
constraint — a region that will not fit, a gesture that does not exist on the device. A difference
introduced for any other reason is a second layout to learn, and it is why the client currently puts
the turn in three different places depending on the window.

The test to apply to any such difference: *name the thing that does not fit.* If nothing does, the
arrangement is the same.

This mostly replaces per-band arrangement with **per-band density**, which was always the intent of
§3 — the ladder changes how much is drawn, not where it is.

### Bands

Chosen by an absolute measure of the viewport, not by device and not by proportion.

**A band names a shape. An absolute constraint licenses an arrangement**, and here they have
collapsed into the same thing — there is one constraint, so there is one boundary. The reasoning
that forced that collapse is worth keeping: a ratio cannot be read as geometry. Take a viewport whose
width is fixed and whose height grows; it would cross from one ratio band into another without
gaining a pixel of width, so any difference sized by width would hand a screen that only got *taller*
a worse board, which §3 forbids outright. What makes a seat bar, a strip, or a panel undrawable is a
count of pixels in one named dimension, never a proportion.

| Band | The shape it names |
| --- | --- |
| **Wide** | the reference shape — a desktop window |
| **Narrow** | **width < 900** — phone portrait and landscape, and a 1280px or 1600px desktop at 200% zoom |

An earlier draft named five bands off aspect ratio — Wide, Ultrawide, Square, Tall, Short — and then
spent a paragraph explaining that a ratio cannot license an arrangement and that only absolute
constraints move anything. The prototype has one absolute constraint and therefore one boundary, and
the five names turned out to be describing screens rather than deciding anything. The paragraph that
warned about this was right; the table above it was the thing it was warning about.

**The reference arrangement is what both shapes draw**, top to bottom: a topbar, the opponents'
band, the phase strip on the midline, your own half, the action bar, your hand — with a side column
at the trailing edge spanning from the band to the hand. Each seat carries its bar at the leading
edge of its own half.

Three things differ at Narrow, and each names the thing that does not fit:

| What differs | The constraint that licenses it |
| --- | --- |
| The **side column becomes a drawer** over the board, on the same gesture that dismisses it at Wide. | **Width.** A 300px column out of fewer than 900 leaves the board too narrow to draw a seat's bar and a row of cards beside it. |
| The **phase strip moves from the midline to under the topbar**. | **Width**, again, and specifically the strip's own: eleven step pills and the turn label do not fit one line below about 900px, and a strip that wraps to three lines on the midline stops being a dividing line and becomes a third region between the halves. |
| The **opponents tile at most two across** instead of four, and the seat bar narrows from 110px to 74px with its zone glyphs in three columns instead of five. | **Width.** Below two seats abreast a seat cannot hold its bar and a card; the bar's own width is what the glyph grid is reflowing to keep. |

Everything else that gets called a band difference is not one. More cards at full size on a wide
screen, a pile where there are copies, a row that pans because it is full, rows merging when a field
cannot give each of them a drawable tile: those are §5's geometry answering the room it was given,
by the same formula at both bands. A band does not decide them and must not be written as though it
does.

### Many seats

The table is not two halves. **Two to eight seats**, and the arrangement holds by tiling rather than
by rearranging:

- **The opponents tile a grid as square as the board allows** — at most four across at Wide, two at
  Narrow.
- **The band takes one share of the table's height per row of seats it uses.** Three opponents in
  one row and mine below it split the table in two; five opponents in two rows take two shares to
  my one. That is what keeps every seat on screen the same height as mine, which is the property
  that matters — a seat is a seat.
- **Every seat carries the same bar**, including the zones that are at zero (§2).
- **Focus** is the answer to eight seats on a small screen: one gesture on a seat keeps its board
  and collapses every other opponent to its bar alone, so the tiling becomes a row of bars plus the
  one board you asked for. It is a *view* of the table and changes nothing about the game.

The reading order of the invariant is unchanged — opponents, board, you, hand, top to bottom.

### 4.1 The turn is not a rail

**The turn strip is not tier 1 and must stop being drawn as though it were.** §2 puts *the current
step* in tier 1 and *the turn structure beyond it* in tier 2 — one gesture. A twelve-step rail
pinned to an edge at all times inverts that, and it is expensive: ~112px of width, which is most of
a card, spent on something a player glances at rarely.

It is also **already being read somewhere else.** The dock says `Your move · Turn 7 · Declare
blockers`, and that is where a player actually looks to find out what step it is — beside the
controls they are about to use, not across the screen.

So:

- **The current step and turn number live in the action bar**, in the line under the prompt: *Your
  turn / Precombat Main*. Tier 1, satisfied by text already beside the controls, costing no region.
- **The whole turn is a horizontal strip of step pills with the current one lit**, on the midline
  between the halves at Wide and under the topbar at Narrow. It is one line of chrome, it is read
  across rather than down, and it is where combat is already read across.
- **The stop preferences are a tray of named helpers in the side column** — *to next turn*, *to end
  step*, *to your turn*, *skip stack*, *to prior end*, *cancel skip* — beside `Concede`.

Two reversals here. The first is small: an earlier draft made the full turn **one compact control
that expands on a gesture**, on the reasoning that a twelve-step rail is expensive. The prototype
draws every step, always — as a strip, which costs a line of height rather than 112px of width, and
at that price a permanently-readable turn is worth more than the gesture it saves. *The turn is not
a rail* survives intact; what it was arguing against was the **rail**, not the strip.

The second: the stops moved **off** the turn strip. An earlier draft insisted a preference divorced
from the strip it applies to is one nobody edits. In the prototype they are named for what they do
rather than for which step they stop at, and named that way they are not really about the strip at
all — *to your turn* is a sentence, not a checkbox on `Untap`. They sit with the other things you
do to the game rather than with the picture of where the game is.

### 4.2 The seat bar

Life, library, hand, graveyard, exile and commander state are things a player should **never hunt
for**, and squeezing them into a 40px bar made them exactly that — folded behind a disclosure at
small sizes and cramped at large ones.

Each seat gets a **bar down the leading edge of its own half**: 110px at Wide, 74px at Narrow, in
the space the turn rail used to hold. It carries, top to bottom:

1. **Name and life**, on one pointable area — the player is a target, and a target is a box.
2. **The zone glyphs and their counts**, in a grid that reflows from five columns to three (§2).
3. **The mana pool**, when there is floating mana. It is the one thing in the bar drawn as a
   *recess* rather than a raised pane: mana sits **in** a pool, and lighting it from above would
   make it a button.

**The name is fitted to the bar, not to its own line.** It is the one run of text on the board whose
length a *player* chose, and the bar's height belongs to the seat, so a name is set at the size the
stylesheet gives it while that fits and smaller when it does not — wrapping, never an ellipsis (§3),
and never leaving a region a player would have to scroll. It has a floor, below which it is a mark
rather than a name; a bar that would need to go under it is a seat with no room for a player in it,
which is a defect in the region allocation above it (§2) and not something the name can answer.

A short window is also where the bar itself is set smaller — the head, the zone buttons and the pool
tighten together, by the same "scale first, remove last" that governs everything else (§3). What does
*not* happen there is a re-stacking: a seat that only got shorter must not be handed a different
arrangement than the seat beside it.

An earlier draft called this a **column** and had it become a **bar above and below its own half**
below 640px of width or 480px of height, because a column carrying five labelled counts needs both.
Glyphs removed the constraint (§2), so there is one bar, in one place, at every size — which is what
§4 asks for and what the earlier draft was reluctantly spending an arrangement to avoid.

**What decides whether a permanent is still a card is nothing**: it is always a card, at whatever
size the row gives it (§2, §6). The threshold this section used to defer to no longer exists.

---

## 5. Geometry

Everything below is a function of available space and object count. Nothing is a function of
content: no region's size is ever set by the text inside it, which is the defect the whole
document exists to remove.

**The mechanism is a grid of computed tracks and a container query, not absolute coordinates.** An
earlier draft called for **scene units** — the table positioned in absolute coordinates by one pure
function, on the reasoning that flow is what lets content decide geometry. The property that
argument was protecting is the one that matters and it is kept in full; the prototype simply gets it
a cheaper way. Every track in the table is a fixed size or a share (`fr`), every one of them floors
at zero so no content can push it open, and a region that holds cards declares itself a **size
container** so the things inside it measure against *the box*, never against each other.

The test is unchanged, and it is the whole of §5: **no region's size is ever set by what is inside
it.** How that is enforced is an implementation choice. A pure `scene()` is one way; a grid whose
every track is computed is another, and it is the one that survived contact with eight seats.

### A card's size is the height of the region it is in

That is the whole formula. Given a row of height `H`:

```
card height = H            (divided by the pile's spread, below)
card width  = card height × 207/291
```

There is no width term, no count term, no `ideal`, and no `FLOOR`. A row's height comes from the
viewport (below); the card takes it; the width follows from the printed proportion.

This replaces `clamp(FLOOR, (W − (N−1)·g)/N, ideal)` and everything derived from it — the soft-down
hard-sideways floor, the switch to overlap, the chip threshold. Those all existed to answer *what a
row does when it has too many cards for its width*, and the prototype answers that with the pan
(§3, step 4) rather than by resizing anything. Taking width out of the card's size is what makes the
rest of the section true by construction:

- **A permanent arriving never resizes anything.** Not the card, not the row, not the seat. The
  eleventh creature does not shrink the other ten; it is simply further along the row.
- **More screen is never a worse board**, trivially, because card size is monotone in one variable.
- **A tapped permanent reserves its turned footprint** — the slot is as wide as the row is tall, so
  a card that taps never lies over its neighbour and the row does not reflow. The height is charged
  whether or not anything is tapped.

**Piles are the one thing a count changes.** Untapped copies of the same permanent share a slot and
cascade downward by a fixed fraction of a card; the pile as a whole is fitted to the row's height,
so its cards are shorter by exactly what the cascade spends and four lands are exactly as tall as
the creature beside them. Attachments use the same construction stepped **down and to the right**,
so an equipped creature never reads as two unrelated piles side by side, and they are laid down
*behind* what they are attached to — the creature is the whole card and the Equipment shows only its
title bar, because the creature is the thing that attacks, blocks and dies.

### The row count is the board's, never the card's

**A field draws two rows, split by the server's `card_types` — creatures nearest the middle, and
everything else behind them — and that count does not fall to buy card size.** It falls only when
the field cannot give each row a drawable tile at all, which is §3's step 5 and the bottom of the
ladder. The count is now *fixed* rather than derived from what is on the board: both rows are drawn
whether or not anything is in them, so the layout a player learns on their first turn is the one
they have on their twentieth.

This is stated here, in the geometry, because the packer is where the rule is actually spent. The
objective is *not* "the row count whose worst row draws the biggest tile" — that objective always
merges, since one row of `N` cards is never smaller than two rows of the same `N`. Height is what
the split costs, and the split is what is kept.

**Permanents a player has no reason to tell apart are one pile, not a row of boxes.** Eight Forests
is eight identical tiles and, on a phone, the whole width of the board spent on its least
interesting half — so a run of them overlaps into a fan with a count on it, the way a player stacks
them on a table. The condition is strict and it is a presentation rule with no rules content: same
card identity, same tap state, same counters, damage, markers, attachments and stated
relationships, and not the object the current question is about. Anything the board would have drawn
differently on one of them breaks the pile, so no fact is ever behind a card and every arrow still
has a whole card to reach.

**The battlefield is as wide as the room.** There is no cap on it. An earlier draft held the board's
content to a maximum width so that a glance would not have to cross a very wide screen; that bought a
real concern at the price of permanent dead space, which is a fixed size imposed on a region and the
thing this document exists to remove.

**A battlefield row starts at its leading edge. The hand is centred until it overflows.** The two
differ because one of them pans and the other does not: a row that can slide has to be anchored at
its start, or the pan origin moves when a permanent arrives and the board shifts under the player.
The hand has a fixed count in view and no such constraint, so it is centred while it fits and
anchored the moment it does not.

An earlier draft argued for centring *both*, on the grounds that left-alignment pushes a short row
against one edge and leaves dead space beside it. That cost is real and it is accepted on the
battlefield: a row of three creatures sits at the leading edge, in line with the three below it and
with every other seat's, and a column of rows that agree on where they start is easier to read
across than a column of rows each centred on its own count.

**Card proportion is 207:291**, the drawing's own grid, at every size — a hair narrower than the
printed 63:88 because the frame's border is inside it rather than around it. There is no separate
tile: the state marks are drawn on the card (§6), so a permanent's footprint is the card's, plus the
turned footprint if it can tap.

**Sizes, effective px** (subject to review — these are reference points, not thresholds):

| | Worth reporting below | Comfortable |
| --- | --- | --- |
| Permanent | 72 × 100 | 130 × 182 |
| Hand card | 100 × 140 | 150 × 209 |
| Stack item | 46 tall | 130 × 182 |

72×100 is measured off XMage, which fits a complete name, cost, type line, keyword line, and P/T
into that box. The hand's is larger because the hand is where a player *chooses*.

**None of these is enforced by anything.** They are the sizes below which the maintainer wants to be
told, and the prototype tells nobody — a short row simply draws a small card. That is the open end
of removing the floor (§2), and it is recorded here rather than hidden: if a report is wanted, this
table is what it reports against.

### Regions are sized by the viewport. Counts are absorbed by the row.

This is the whole rule, and everything else in this section is a consequence of it.

**A region's height is a function of the viewport alone.** How many permanents a seat controls, how
many cards are in a hand, how deep the stack is — none of it changes the size or position of any
box. The layout a player learns on their first turn is the layout they have on their twentieth.

**A count is absorbed by the row it is in — by piling identical copies, and then by panning.** It is
never absorbed by the card, which has one size and takes it from the region (above). Fifteen
permanents in a field sized for six do not make the field taller, and they do not make the cards
smaller either: they make the row longer than the screen, and the row slides.

An earlier draft made the cards absorb it — *"they make the cards smaller, and then overlapped, and
then chips"* — which is why that draft needed a floor, a fan and a chip tier. Moving the absorption
from the card to the row removed all three (§3).

So **every seat's field is the same height**, whatever is on it, and the lines across the table do
not move for any game event. A seat that wipes an opponent's board does not watch its own permanents
jump to a new size and a new place. A seat playing its first creature does not shove another half of
the table upward. With more than two seats the same property is what the band's per-row share buys
(§4, "Many seats").

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

**Focus is the other departure**, and it is the same distinction the other way up: it responds to
*what the player asked for* and not to how much is on the table. Collapsing six opponents to their
bars to look at one board is a view a player chose; it is not the layout reacting to the game.

The hand-and-dock trade that used to be named here is gone (§2): the action bar is permanent, so
there is no mode that changes a band's size.

Region heights are otherwise allocated in contract order: the tier-1 minimums are satisfied first,
and what is left over goes to the board. At Wide the table's share is roughly **seven parts board to
three parts hand**, with the topbar, the phase strip and the action bar taking what they need first.

---

## 5.5 The material

The document above says where everything goes. This says what it is made of, which is the part the
prototype added outright — earlier drafts settled that the client is dark and left the rest to each
surface, and the result was that a topbar, a seat bar, a phase strip and an action bar looked like
four unrelated boxes that happened to share a colour.

**There is one material: a tinted surface, lit along its top edge, the tint falling away below it,
casting onto whatever it sits over.** The light comes from above and only from above. That single
rule is what makes a run of unrelated bars read as one made thing.

The falloff is stated in **pixels, not percentages**, so a 32px topbar and a full-height sidebar are
lit by the same amount and a tall pane does not wash out.

Three states, and everything in the client is one of them:

| | What it is | Where |
| --- | --- | --- |
| **Pane** | Raised. Lit along its top edge, casting a shadow in the direction it faces. | The topbar, the phase strip, the seat bar, the side column, every button, a seat card. |
| **Recess** | Sunk. Shadowed along its top edge instead of lit. Holds things that sit *in* it. | The mana pool, the helper tray, the filter strip, a panel's tab row, a track a switch runs in. |
| **Glass** | A real backdrop blur, and a thinner scrim because the blur is doing the separating. | Only where something genuinely passes behind: a dialog over the table, the drawer over the board, a pinned card over everything. |

**Glass is never used where nothing is behind.** A blur over a flat ground is an expensive way to
draw a slightly different flat ground, and it is a lie about depth.

**A recess is a statement, not a decoration.** Mana sits *in* a pool; lighting it from above would
make it a button, and it is not one.

### The lit pane is what "current" looks like

One treatment, everywhere: **a deeper blue pane with a brighter keyline, lit along its top edge in
the accent.** It means *this is the one*, and it is the same drawing whether it marks the current
phase pill, the chosen server, the selected deck, the live tab, my own seat, the focused seat, the
selected settings section, or the format a new table will use. A player learns it once.

Nothing else may mean that. A second treatment for "current" is a second thing to learn, and the two
will disagree within a month.

### Colour never carries a fact alone

Ready is a green dot **and** a green rim **and** the word *Ready*. A full table is a grey dot **and**
a muted row **and** the word *Full*. The tone of the action bar (§6.5) is beside a sentence that
says the same thing. Every fact survives being read by someone who cannot see the colour, and the
colour is what makes it fast for everyone else.

### The card is drawn under the same light

The card is not exempt and must not look it. What is raised off the ivory slab — the title bar, the
type bar, the P/T plaque — is lit along its top edge; what is sunk into it — the art window, the
text field — is shadowed along that same edge. One raking sheen crosses the whole face last, so the
parts read as one sheet of glass rather than five separately lit pieces, and one lit rim runs the
card's border exactly as it runs a pane's.

That is why a card lying on this table looks like it belongs to it, and it is the strongest argument
for the material being stated once here rather than per surface.

---

## 6. The card

The card is the atom: the hand, every battlefield, the stack, opened piles, and the deck builder
all draw it, so a rule fixed here is fixed everywhere. It is also where the client fails most
visibly today — a hand in which every card reads `C…`, `Dis…`, `L…` is not a degraded hand, it is
an unusable one.

### The principle

**The card is one drawing, in its own grid, at one size.**

It is a single SVG laid out in a 207×291 pixel grid — every bar, every window, every plaque is a
path in that grid — and it is scaled by exactly one number. A permanent, a hand card, a stack
thumbnail and a full-screen preview are the *same drawing*, and there is no variant a caller can
pass.

**Every run of text is fitted to its own box in that grid**, by bisection between a ceiling and a
floor stated in grid units: the name shrinks until it fits the width left in the title bar, the type
line likewise, the rules text is set as large as its field will take, and the P/T as large as its
plaque will hold. Because the box and the type are both in the card's grid, the answer is the same
whether the card is drawn at 600px or 60px — which is why there is one drawing and not four.

Two consequences that the rest of this section is mostly the working-out of: **nothing on a card is
ever truncated**, because the size gives way first; and **nothing on a card is ever dropped**,
because there is no state in which one part has room and another does not.

The rules text is the one with no design size at all — it is simply set as large as its field will
take, up to the point where body text stops looking like body text. Two words of reminder text fill
the same field a paragraph needs, and no card carries a half-empty text box.

### Anatomy, from the top

The frame is an ivory slab on a black ground, with the bars raised off the slab and the windows sunk
into it. The slab's rounded foot stops high of the card's bottom edge, so the text field overhangs
it into a dark well — which is where the state marks live.

1. **Title bar.** A raised bar with the **name leading and the cost following at the trailing
   edge** — where a printed card puts it and where a player's eye already goes. The cost keeps its
   natural width; the name is fitted into what is left, on one line, shrinking until it fits and
   never wrapping and never truncating.

   **This reverses the fitting order.** An earlier draft required the opposite — *the name is fitted
   against the whole band and the cost takes the width the name did not use* — with a long argument
   that cost-first is not monotone in the box, that one pixel of width can carry the cost over a
   threshold and hand the name a narrower band than it had before, and a measurement of 14 inverted
   names across the supported range. That argument was sound about a card whose parts are fitted in
   *device* pixels. It does not apply to this card: the pips are drawn in the card's own grid at a
   fixed size, so the width they take is a constant of the drawing rather than a function of the
   box, and the name's share is therefore constant too. There is no threshold left to cross. The
   name is fitted second and is monotone anyway.

   Pips are graphic and read at sizes text does not: no baseline, no wrapping, no hyphenation. They
   are the project's own discs, never an official symbol and never a downloaded one.
2. **Art window.** Sunk into the slab, flush under the title bar, the largest single element, a
   fixed rectangle in the grid. **By default it is a plain dark field tinted from the card's own
   colour — not a composition.** A player-supplied illustration fills it when ADR 0012's pipeline
   has one, cropped to fill rather than letterboxed inside it.

   An earlier draft called for **procedural composition by default**, so the window is never empty.
   The prototype draws the tinted field instead and it is the better answer: a generated picture is
   a picture, and at the sizes a board actually draws it is a smear that competes with the name for
   the eye without ever identifying anything. A quiet coloured field says *this is a card and here
   is its colour*, which is all the window owes when there is no art.
3. **Type bar.** A second raised bar sitting on the art window's foot, fitted the same way as the
   name. It carries the set mark at its trailing edge where a printing has one.
4. **Rules text.** A sunk field under the type bar, its lower third overhanging the slab into the
   dark. `{T}` and the rest of the braces become pips inline, and a line that is a bare keyword is
   set bold. On a creature the field stops short of the P/T plaque rather than running under it.
5. **State marks.** P/T on a raised plaque in the bottom trailing corner; counters as pills along
   the foot of the art; summoning sickness as a wash over the whole face with a dashed rim.

   **The P/T plaque prints the number you act on, and the numeral says so**: a creature grown by
   counters shows its current power and toughness, coloured green when it is above the printed one
   and red when below. Counters that move P/T are named by their **total** rather than by a kind and
   a multiplier — two +1/+1 counters read `+2/+2`, because that is what a player thinks they have —
   and every other kind keeps its name and a count. There are far more kinds of counter than there
   are marks worth drawing, so one translucent pill shape carries all of them.
6. **Type line degradation** is unchanged in principle and unreached in practice: **by rule, not by
   ellipsis** — drop the subtype after the em-dash, then supertypes, leaving the card type. The
   prototype never reaches it because the type line shrinks to fit first.

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

**A tapped permanent turns whole**, attachments and pile and all: the slot rotates, not the top
card, so an equipped creature that taps takes its Equipment with it and a targeting arrow aimed at
it lands on the turned box.

### There is one presentation

An earlier draft specified **four** — `full`, `designed`, `compact`, `chip` — each dropping what it
had no room for, chosen from the box by a function no caller could override. That whole apparatus is
gone. There is one card, it draws everything it has, and it does so at every size, because each run
of text is fitted to a box in the card's own grid rather than to a number of device pixels (§6, "The
principle").

The one thing that still varies is **how much of the face is ours**, and that is a player setting
rather than a size (§9):

| | Draws |
| --- | --- |
| **Frame** | The drawing, whole. Nothing is fetched. The default. |
| **Frame and art** | The same drawing, with a fetched illustration filling the art window. |
| **Full card** | The fetched card face, whole, in place of the drawing — none of the frame, because none of it is ours in this view. |

The permanent's own marks — counters, damage, markers, summoning sickness — are drawn **over**
whichever face is underneath, because they are true of the permanent and not of the card. **So is
the stat**, and that is the correction this line needed: a full card face is a *printed* card, and
a printed 2/2 standing in for a creature the server just called a 4/4 is a wrong board rather than
a plainer one. Everything the server computed rides over the picture, and none of it is a setting.

**The name band is the one part of the frame a full card may keep**, and it is a setting because it
is the only one that is genuinely a duplicate: the printed band is already there. It is offered
because it is also the part of a fetched image that suffers most — an official title bar is
unreadable at board size and the printed cost is a row of symbols this client draws several times
larger. A player who wants their board scannable turns it on; nobody is moved into it.

The observation the old `compact` tier was making survives and is worth keeping: run §5's numbers
against a real screen and a 1920×1080 desktop draws permanents at roughly 155px, not at the 182px
the sizes table calls comfortable. That is the right answer rather than a shortfall — **a
battlefield full of complete 155px cards beats a battlefield of half as many beautiful ones** — and
now it needs no tier to say so, because the card at 155px is the same card.

### Density

The other half of what XMage proves. Its permanent tile is ~72×100 and is *full* — near-zero
padding, a small art share, and text everywhere else — which is how it fits eleven permanents to a
row, three rows, a seven-card hand, and both rails on one screen with nothing cut off.

Ours were the inverse: a 108px tile that was mostly padding and art window, showing `C…`.

**The card's parts do not compete at all.** An earlier draft said the art "takes the space that is
left" and "degrades to nothing without costing a fact", and that was implemented literally — a card
whose rules text is long lost its art window entirely so the text could stay at its preferred size.
That is §3's mistake at the scale of one card, and the draft that followed fixed it by ordering the
sacrifice: text smaller first, art eaten last.

The prototype removes the question. Every box in the grid is fixed; the text is fitted to the box it
was given; nothing is ever taken from one part to pay for another. A card with four lines of rules
text and a card with four words have the same art window, the same bars and the same plaque — they
differ only in how large the rules text is set.

**Everything on a card is written once.** A keyword that appears in the rules text is not also
printed as a separate italic line — that is one fact twice (§2.1), and it is currently costing a
line of the very space the art is being taken for. Where the server's prose states a keyword, the
prose is the statement; the separate keyword line exists only for a card whose keywords are *not* in
its drawn text.

### The check this has to pass

**No card abbreviates a name, anywhere, at any supported size.** There is nothing left to qualify
that with: the name is fitted by shrinking within the card's own grid, so it fits by construction,
and a card that is too small to read is a small card rather than a truncated one. An ellipsis
anywhere on a card face is a defect.

The corresponding check on the other side is §5's: that a region never sizes itself to its contents.
Between them, the board is always complete and always the same shape.

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

### The tone tracks the turn, not the urgency

The bar is a tinted pane in the material of §5.5, and its tint says **where in the turn you are**:

| Tone | When |
| --- | --- |
| **Green** | The turn's bookends — upkeep, the end step, a mulligan decision. |
| **Blue** | A main phase: you may cast at will. |
| **Red** | Combat is live and the choice costs something. |

An earlier draft had the tone say **what the controls are currently for** — your move, the game is
asking, in flight, confirm this, waiting, over. That is a real distinction and it is still drawn, in
the words: the prompt says what is being asked and the line under it says which step you are in.
What the tone is *for* is the thing words are bad at, which is telling you at a glance that the game
has moved somewhere different. Tying it to the turn means the bar changes colour when the situation
changes, rather than flickering between two shades of "asking" within one step.

The buttons take their colour from the bar, so a green step's `Done` is green. The quieter half of a
pair — cancel, decline, mulligan — is cut *into* the bar as a recess rather than raised off it: a
hole, not a pane (§5.5).

### The rules

1. **The prompt is drawn once, in one place, or not at all.** Where the options state the question,
   the options *are* the question.
2. **The tone is shown and the fact is written.** The bar's colour is not carrying anything on its
   own (§5.5) — the prompt and the step line say it in words beside it.
3. **A question never scrolls and is never clipped.** If it does not fit, the fault is that
   subjects are being listed that the board could have answered.
4. **Everything reachable by pointer is reachable by keyboard**, including answering on the board.
5. The dock's own band is computed like every other region's (§5), and it is **fixed**: it responds
   neither to how much there is to ask about nor to whether anything is being asked at all.

### A question about several objects is asked one object at a time

A combat declaration is one message with several choices in it, and asking for all of them at once
is what made attacking with two creatures at two defenders unanswerable: every defender slot lists
the same candidates, so a click on a seat could mean any of them and meant whichever came first.

**So a subject the declaration names is *aimed*.** Choosing an attacker asks what that attacker
attacks, and until it is answered the board lights only what that attacker may attack and the click
means only that. Then the next attacker. The pairing is not the client's to work out — the server
states which attacker each slot belongs to — and neither is the sequence: a slot about a choice the
same action asks you to make simply is not a question until that choice is made.

Two consequences follow, and both are the point:

- **An arrow is drawn per choice, as it is made** (§6.6), so the declaration is read as a picture
  rather than as a tally.
- **Two ways out, and they are different questions.** *Start again* takes back every answer and
  leaves the question standing — a declaration aimed at the wrong things is undone in one click
  without also undoing *declaring* — and it is offered only while there is something to take back.
  *Cancel* leaves the question, every time: a player who has decided not to cast this spell should
  not press one button twice to stop being asked about it. Escape is Cancel. Nothing was sent for
  either, so neither undoes anything.

### You say what you are playing, then pay for it

Making mana and then finding the card is the wrong way round: a player decides to cast the Bear and
*then* works out which lands to turn. The client used to require the opposite, because a card the
server has not offered an action for owns no click, so the one moment a player most wants to click a
card was the moment it was inert.

**Clicking it now states an intent.** The bar names the card and shows two things beside each other
— what it costs as printed, and what is floating — the mana sources stay live on the board, and
**Confirm goes live the moment the server offers the cast**. Nothing about this is the client
deciding: it never adds the pips up, never compares the two rows, and never concludes that a cost is
paid. It holds a card's id and waits to be told.

The intent is the only thing besides an unanswered submission that survives a message, and it has to
be: making mana is one message per source. It claims nothing, and it ends by itself — the card is
cast, or it leaves the hand, or the player presses Cancel.

**What this does not offer is untapping a source to pay differently**, and that is a rules
limitation rather than a missing feature: mana that has been made has been made, at a table as much
as here, and the alternative — holding the taps back and releasing them on Confirm — would mean this
client deciding when a cost was covered, which is the one thing it must never do.

### A spell asks what it is aimed at, then what it costs

A cast is one action with two kinds of question — its targets and its pips — and they are asked in
the order the rules pay them: targets are chosen as the spell is put on the stack (CR 601.2c), the
cost is paid after (CR 601.2f–h). So the bar asks the targets first and the pips appear once they
are answered. Leading with the payment put a cost line above a question nobody had answered yet, and
a player who paid it in full still could not cast: a target slot the server did not mark *optional*
must be filled or the submission is rejected, so **Confirm stays dark until it is** — the same
slot-counting the pips already get, over the same server-stated flag.

**A choice you have made is drawn on the board, before it is sent.** A land picked for a pip and a
creature put into a declaration both turn sideways where they stand, because that is what they are
about to be; clicking either again stands it back up, and Cancel stands all of them up at once.
Which ones turn is the server's word, per candidate (`docs/protocol.md`) — a creature with vigilance
attacks without tapping and a mana ability that pays some other way taps nothing, and both are
keyword and cost judgments this client does not make. Nothing has been sent while any of this is on
screen, which is exactly why the client has to draw it: the server's board still has every one of
them standing up.

## 6.6 Reading, pointing, and looking inside

Three surfaces the earlier drafts named in passing and never specified. All three exist for the same
reason: **reading must not cost a click**, because a click is how you act.

### The preview follows the look

The pointer over any card — on a battlefield, in the hand, in the stack, in an opened pile — draws
that card whole at the top of the side column. It follows the *look*, not the selection, and it
takes no gesture at all.

**Holding a card pins it**, drawn as large as the screen allows over a glass scrim, dismissed by a
tap anywhere or Escape. That is the answer for a phone, which has no hover, and for the card you
want to keep looking at while you think.

### A pile is a dialog

Opening a zone — a graveyard, an exile, a command zone, a library the game is asking you to search —
is a dialog over the table in the glass of §5.5: a grid of whole cards, filling by card width so the
column count is the viewport's answer, **and it may scroll**, because a pile is not the board (§3).

Its head says whose pile it is and how many are in it. **When the game asked the question, the
dialog carries the answer**: the cards become selectable, the footer says what choosing one will do,
and the commit is the only raised control in it. When the game did not ask, it is a place to look
and nothing is selectable.

### An arrow is a statement about the board, not a control

Targets and combat are drawn as arrows on **one overlay above the whole table**, because a row clips
and pans and an arrow leaves its row immediately. It takes no pointer events at all.

- **It never carries a fact alone.** Everything an arrow says is also said in words somewhere a
  screen reader reaches (§5.5). The arrow is the fast copy, not the only one.
- **Both ends are objects the client tagged**, and an arrow is drawn only when both are on screen.
  An end inside an unopened pile has no box; an arrow pointing confidently at a card nobody can see
  is worse than the sentence that still names it.
- **An arrow is built like the action bar** (§5.5): a dark-cased tinted body with a brighter
  keyline, so it holds an edge over an ivory card as well as over the black table.
- **Two tones**: targeting and combat. They are read together often enough — a spell aimed at an
  attacker — that telling them apart matters more than either being pretty.
- **A declaration draws its arrows while it is still being made.** A combat declaration is several
  choices in one message, and three attackers pointed at two defenders is a fact about the player's
  own intent that no wording in the bar can hold. So an answer being assembled draws the same two
  tones from the same stated ids, and they disappear with the draft. It is still not the client
  stating anything about the game: every end is a `subject` the server named and a `candidate` it
  enumerated, and the picture is of the message about to be sent.

---

## 7. Type

**There are two scales, and they are measured in different units.** That is the correction this
section needed: an earlier draft put card type and chrome type on one table in one unit, which is
what made the 9px floor look like it applied to both.

**Card type is set in the card's own 207×291 grid** and scales with the card. The numbers below are
grid units, and each run is fitted by bisection between its ceiling and its floor (§6):

| Role | Ceiling | Floor |
| --- | --- | --- |
| Name | 10 | 6 |
| Type line | 8.5 | 5.5 |
| Rules text | 22 | 4.5 |
| P/T | 20 | 7 |

The rules text's ceiling is the only real dial on the card: it is the point past which body text
stops being body text, for the card that has two words to say.

**Chrome type is set in device pixels** and does not scale with anything:

| Role | Size |
| --- | --- |
| Section heading, table name, prompt | 14–15px |
| Body, log, chat, a row's name | 12–13px |
| Labels, counts, secondary prose | 10–11px |
| An uppercase field label | 10px, letterspaced |

Chrome is read across the screen and holds a larger floor than card text, which is read at a glance
and in place. **11px is the floor for anything a player has to act on**, and the small uppercase
labels are the one thing allowed below it, because a field label is read once and then never again.

What gets sacrificed, in order, is **size, then line count**. There is no third step: completeness
is not negotiable anywhere in this client (§6).

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

### What the prototype retired

Recorded together, because the list is the point: reasoning in prose produced a great deal of
apparatus that building it showed to be unnecessary.

| Retired | Replaced by |
| --- | --- |
| The 9px effective type floor | Text fitted in the card's own grid; no device-pixel threshold exists (§2, §6) |
| The chip presentation | Nothing. A card is a card at every size (§3, §6) |
| The four presentations | One drawing, one size, plus a player's choice of how much of the face is theirs (§6) |
| Fanned overlap in a full row | The row pans, cards at full size (§3, §5) |
| `clamp(FLOOR, fitted, ideal)` | Card size is the region's height (§5) |
| The seat-count fold | Glyphs, so all five counts fit at every size (§2, §4.2) |
| The hand-and-dock trade | A permanent action bar above the hand (§2) |
| Five aspect-ratio bands | One width constraint (§4) |
| The turn as a control that expands | The whole turn as a strip, always drawn (§4.1) |
| The navigation rail and its three destinations | A topbar per screen; settings as a dialog (§9.0) |
| Name-first fitting in the title bar | Cost keeps its constant width; the name is fitted into the rest (§6) |
| Procedural art in the window | A tinted field until a player supplies a picture (§6) |

Every one of these was argued for at length in an earlier draft of this document, and the arguments
are kept beside their reversals rather than deleted. A rule that was replaced tells the next reader
what has already been tried; a rule that was quietly removed tells them nothing, and they will
propose it again.

---

## 9. Before the game

Four screens: **connect**, **the lobby**, **the table room**, and **settings**. They are not the
table and they do not use the table's geometry — their content is a list whose length the server
decides, not a board whose geometry a viewport decides.

What they *do* share with the table is everything else: the same dark ground, the same material
(§5.5), the same type scale (§7), and the same card (§6). A card in a deck list and a card on a
battlefield are the same drawing. That is what makes the front door read as the same product as the
table, and it is the part the current lobby fails.

What this settles:

- These screens are conventional responsive compositions. §5's geometry does not apply to them.
- Everything else in this document does: nothing clipped, nothing truncated, no native form
  control, nothing narrated (§2.1), and no server identifier a player has no use for.
- **A list here may scroll.** The board may not. These are not in tension: §3's rule is about the
  board. Say which one a region is before deciding.
- **Only one region of a screen scrolls, and the page itself never does.** The topbar, a header, a
  filter strip, an action footer and a side panel are fixed; the list between them is what moves.
- Tier 1 before a game exists: which table you are at, who is in it, what it is waiting on, and how
  to leave.

### 9.0 There is no shell

**The topbar is the navigation.** Where you can go sits at its leading edge; who you are, the gear,
and the side panel's toggle sit at its trailing edge; the screen fills everything below it.

This reverses the largest structural claim this document made about the pre-game — that it is a
**client shell**: *a persistent navigation rail carrying Play, Decks and Settings, with identity at
its foot, becoming a bar at narrow widths.* The prototype has no rail, no persistent navigation
region, and no Decks destination; settings is a dialog rather than a place.

The argument that produced the rail was right about what it was actually arguing. A spatial lobby —
a pre-game *table*, a *room* you walk into — has to hand off to a conventional surface the moment a
player builds a deck, and the deck editor is the densest surface in the product. All of that
stands. What it does not establish is that the other screens need a rail beside them: four
destinations, one of which is a dialog, is not a rail's worth of navigation. A rail is what a
product with a dozen places needs. This one has a front door, a list, a room, a deck editor, and a
settings sheet.

**The deck editor is a destination, and it is still not a rail** (§9.7). Building a deck needed the
conventional surface that argument predicted, so it got a screen; a button on the lobby's topbar and
one in the seat's editor is the whole of how you reach it. It differs from the two server screens in
the way that matters here: it is a place *this client* chose to be, drawn over whichever screen you
were on, and leaving it puts you back on that screen rather than deciding which one you get. The
guarantee below is untouched — the screen underneath it is still the server's answer.

**What the shell was protecting is kept in full**: *which destination you are on is the client's
answer; which contract you are on is the server's.* A `GameView` arriving replaces the screen,
because the contract changed. Opening settings replaces nothing — it is a dialog over the screen you
were already on, which is that guarantee in its strongest form.

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
is behind one gear** rather than scattered across a header (§9.6).

### 9.3 Connect

The first screen. It exists because a player should arrive at the lobby already being somebody,
rather than finding an input box in the header asking who they are.

One panel, centred on the dark ground, carrying four things:

- **The wordmark.** `SAGE`, set large and widely tracked, filled with the same top-lit gradient
  every pane carries (§5.5), with *Server Authoritative Game Engine* under it and the four initials
  picked out in the accent. This is the one place the product says its own name, and picking out the
  initials teaches the acronym without spending a second line of copy on it.
- **Name**, prefilled with the last one this device used. A returning player presses one key.
- **Server**, a recessed list — a status dot, a name, an address, a latency — carrying the
  predefined servers, a localhost entry, and a custom one. The list is client-side configuration;
  the protocol has no server directory and this document is not proposing one, and the custom entry
  is what keeps that from being a limitation. **Choosing `Custom` reveals its address field directly
  beneath the list**, so the address is adjacent to the choice it belongs to rather than parked
  elsewhere in the form.
- **Connect**, cut into the panel as a recess while the form is incomplete and raised as a pane the
  moment it is not — the disabled state is a state of the material, not a greyed-out button.

Neither field is a wire change: `hello` carries a token and nothing else, and the name is set by
the command the client already sends.

**The gear is on it**, so card art can be set up before ever joining a table. The prototype had not
put the button there; the client does, at the trailing edge of the topbar where every other screen
carries it.

### 9.4 The lobby

**Land on the tables list.** Above it, a header saying what the list is, how much of it you are
seeing, and the one button that makes a new one. Below that, the filters in a recess: a search, the
formats as a segmented control, and an open-seats toggle.

**A row is a grid, and every row uses the same columns**, so name, format, occupancy and the button
line up down the whole list. A row carries: a status dot, the table's name over who is hosting it,
the format, the occupancy, and one button.

**Occupancy is drawn as seat pips and a count** — one mark per seat, filled for taken. A table's
fullness is then read without reading a number, and the count is there for when you want the number.
A full table is muted throughout, its button says `Full`, and it is not pressable. An invite-only
table carries a lock beside its name.

Occupancy chooses which advertised command the button leads with, exactly as `lobby.ts` already
decides — no new rule.

**The side column is the tabbed panel the table uses** (§9.5), here carrying **Chat** and
**Players**, the latter with a count. Same vocabulary, same place, both before and during a game.

**The topbar carries the way out and the way to your decks**: `← Disconnect` at its leading edge and
`Deck Editor` beside it, then who you are, the gear, and the panel toggle at the trailing edge. Deck
building is the one thing here that is not about a table, which is why it is on the bar rather than
in the list (§9.0, §9.7).

**Creating a table is a dialog** over the list, in the glass of §5.5: name, format, seats, access,
and **undo**. Its footer restates what will be made in one line, so the summary is read where the
commit is, and the create button is the only raised thing in it.

### 9.5 The table room

Joining replaces the list with **the table you are at, as its own screen** rather than as a panel in
the list. Topbar, the table's name and how full it is, the rules strip, the seats, an action footer,
and the same tabbed side panel — here **Chat** and **Watching**.

**A table's rules are chosen when it is made and shown where it is played.** The strip under the
title carries them plainly — format, seats, access, mulligan, clock, undo — and **a rule that
changes how the game plays is drawn in its own colour**: undo allowed in green, no undo in red,
beside the words that say the same thing (§5.5).

**Undo is a table rule**, named here because it is the first of its kind: chosen at creation, fixed
for the life of the table, and visible to everyone at it. A player must never have to ask whether
this table lets an action be taken back.

**A seat is a card, and the seats tile the way the board's do** — at most four across, two on a
phone. Each carries a ready dot, the player's name, a host badge where it applies, the deck, the
state in words, and — on your own seat only — `Edit` and `Change`. Your seat is ringed the way the
active field is on the board. **An empty seat is a hole in the table, not a pane on it**: a dashed
recess carrying `Open seat` and an invitation. The host sees a kick on every seat but their own.

**A seat's deck is drawn as its colours**, at a size that reads across the room. **In a commander
game it is drawn as the card the deck is built around**: the real card frame at ~64px, ringed in the
gold the command zone already wears, with the commander's name in gold beneath it and the deck's
name under that. The seat's row grows taller to afford it. This is the one place in the client where
a card is used as an *identity* rather than as an object, and it earns that — a commander is what a
Commander deck is called.

**Waiting-on is drawn on the seat it belongs to**, not summarised in a sentence underneath. Ready is
green in the dot, the rim and the word, everywhere it appears.

**The footer is the board's action bar, doing the same job before the game starts** (§6.5): blue
while the table is still waiting, green once every seat is ready, carrying the tally in words, your
`Ready`, and — for the host — `Start game`, which is not pressable until it would do something.

The AI kinds and the starter decks come from `CatalogView` as they do now; what changes is that
choosing one is not a native `<select>` and its description is not printed beside it.

### 9.6 Settings

**A dialog, not a destination**, opened by the gear that is in every topbar and closed by Escape or
a click outside. It is sectioned, with a rail of sections at its leading edge and the section beside
it — which is the rail this document once wanted for the whole client, at the one scale where it
earns its keep.

**Cards** — how much of a card's face is ours: `Frame`, `Frame and art`, `Full card` (§6). Each
option is a **tile rendering the same sample card in that view**, because the choice is the thing
itself and three words could not say it. Under them, the one option that belongs to a face rather
than to the pipeline: whether a `Full card` also wears SAGE's name band (§6). It is dimmed rather
than hidden while another face is chosen, so it is discoverable from the tile that would use it and
nothing appears out of nowhere when that tile is picked.

**Card art** — the ADR 0012 pipeline, made visible to the player it belongs to:

- A switch for fetching art as you play, with the sentence that matters under it: the browser asks
  the source directly and keeps what comes back on this device, and nothing passes through the SAGE
  server. Nothing is bundled, served, proxied, or redistributed.
- What is stored: the size, a meter, and how many of the supported cards are held.
- **Download all**, with progress and a way to stop it, for preparing before a game.
- **Clear**, which frees the space.

**The two sections govern each other, and that is a rule rather than a nicety.** With art switched
off the two faces that need pictures are dimmed and offer the way to turn it on; switching art off
returns the face to `Frame`; the bulk download is not offered while art is off. **Clearing is never
disabled** — freeing space must not depend on a setting.

### 9.7 Decks

**Building a deck is a screen; adjusting one is a dialog at the table.** Those are two different
sizes of question and they get two different surfaces, in the same vocabulary.

> This section is the exception to the document's own provenance. Everything above it records what
> `clients/prototype` settled; the deck editor was never prototyped, and what follows was designed
> in the shipping client instead. It is binding on `clients/web` like the rest, but it is evidence
> from one build rather than from the sandbox — read it as less settled than §6.

#### The deck editor, as its own screen

Reached from the lobby's topbar, and from a seat by way of the small editor. Four regions, and the
topbar is its navigation like every other screen's (§9.0):

- **The pool**, filling the top half: every card the catalog holds, drawn as whole cards in a grid
  that fits as many per row as the width takes. Double-click puts a copy in the deck.
- **The search**, across the top of the pool: a sets menu, a name box, and the five colours plus
  colourless and lands as toggles. **Every toggle starts on** — switching one off is the player
  saying they do not want to see that kind, so an untouched search hides nothing. The sets menu is
  drawn and says it has nothing behind it, because `CatalogCard` carries no printings.
- **The options bar**, between the halves: how the deck below is read (`Full cards`, `Stacked`,
  `Titles`), what its columns are cut by (`Mana cost`, `Color`, `Card type`), and which pile sits
  beside it (`Commander`, `Sideboard`, or neither). All three are device state and none of them
  changes the deck.
- **The deck**, filling the bottom half: columns of cards, each headed by what it holds and how
  many — `3 Mana (7 cards)`. Double-click takes a copy out. A pile beside it holds the commander or
  the sideboard, with one button whose arrow points where the picked card would go.

The right sidebar carries the card viewer the board uses (§6.6, the same component), the deck's
name and its Load/Save, and the deck summarised three ways: curve, colours, types.

**Three readings of one deck, because a card is worth different amounts of space at different
moments.** `Full cards` is for looking at what you have; `Stacked` overlaps each card to its title
bar, which is how a decklist is read; `Titles` is the printed title bar alone, and it is the
*card's own bar* rather than a list styled to look like one — the same SVG, cropped, so the list
and the card can never drift apart. A land is not on the mana curve and not in the `0` column: it
has no cost, rather than a cost of nothing.

**Selection is one copy, not one card.** A deck holding four of a land holds four cards on the
table, and clicking the third lights the third.

#### At the table

The seat's chooser **is** the editor's loader — the same dialog, listing this device's decks, the
bundled starters, and a file. The seat's editor is the small edit: the deck and the cards beside it
as two lists of equal width, one click sending a copy across, the card under the pointer drawn
whole between them, and the pane beside the deck toggling between sideboard and commander. Rows are
the cards' own title bars, as `Titles` draws them. On a phone the preview is what gives way; the
lists are the tool.

`Deck editor…` opens the screen over the table and the way back submits what you built, so the
larger question is one button away and answering it does not cost the seat.

#### What is device-local, and what is not

A deck is **kept on the device** and interchanged as a file (ADR 0018): `submit_deck` is what
reaches the server, and it carries a flat list plus a commander. A **sideboard is therefore a
device-local note** — it round-trips through storage and files, and the footer of the seat editor
says plainly that it is not sent, rather than letting a player discover it when it does not arrive.

**No client here decides legality.** The counts are arithmetic on counts, the copy limit the format
published is quoted and never enforced, the summaries state what the server described, and the
verdict stays the server's `LobbyRejection`. What a seat *shows* the table — its colours and its
commander — is the server's own summary of a deck nobody else may read (`docs/protocol.md`,
`SeatView`), not something this client works out about somebody else's cards.

**Both scrollbars in the deck editor are deliberate**, and they are the one place §5's "no region
scrolls" rule is relaxed: a catalog is unbounded and a deck is unbounded, so the pool scrolls down
and the deck's columns scroll both ways. Nothing else on the screen does, and the regions around
them are sized so they cannot push the page wider than the window.

---

## 10. Open questions

These are what the prototype did not answer. They are open because nothing has exercised them, not
because they are undecided in principle — and each names what would settle it.

1. **How small is too small.** Removing the type floor and the chip (§2, §3) removed the only thing
   that asked whether a card had become unreadable. A row is sized by the viewport and the card
   takes what it is given, at any size. Settled by: a sweep across the supported range that reports
   the smallest permanent each shape produces, checked against §5's table by the maintainer.
2. **The deck editor at catalog scale** (§9.7). The screen exists and is playable, but it has only
   ever been built against a catalog of ~134 cards, where every filter is instant and no list is
   long enough to need virtualising. Settled by: driving it with a catalog large enough to make the
   pool grid and the search expensive, and reporting where it stops feeling immediate.
3. **A game with three or more seats, played.** The prototype tiles up to eight and focus works, but
   nobody has played a four-player game on it. Whether a seat at one quarter of the table is enough
   to play from is a judgment only playing can make.
4. **The settle, made legible.** The brief names this as the actual product hypothesis, and neither
   this document nor the prototype has designed it. What a settle did is currently a run of log
   lines; what it should be is unanswered.
5. **Spectating** — a count in the room and nowhere else. A connection the server puts on the
   spectator contract still lands on a screen that says only that it is not built.
6. **Chat, and who is in the lobby.** The client draws both panels and says they carry nothing,
   which is a placement decision rather than a design for either. What a table's chat should be —
   and whether the lobby's is the same surface — is unanswered.

Questions raised after this document became binding go here.
