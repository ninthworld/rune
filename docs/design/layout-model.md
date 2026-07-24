# The battlefield layout model — staging 2–6 players

The layout model for the 2.5D client (issue #470, under
[ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md)): how every
seat's region is staged on the battlefield plane at each player count, how
focus behaves, and the degradation ladder for stress-case boards. The look
and motion of what is staged belongs to
[`visual-system.md`](visual-system.md); every number here lives inside
[`presentation-budgets.md`](presentation-budgets.md).

Evidence: the staging prototype
[`prototypes/ui-2-5d-layouts-v1.html`](../../prototypes/ui-2-5d-layouts-v1.html)
(reference-only; one `stagePlane()` function drives every scenario) and the
committed concept mocks in
[`../ui-concepts/layouts-v1/`](../ui-concepts/layouts-v1/):

| Mock | Scenario proven |
| --- | --- |
| `layout-duel-v1.jpg` | 2 players — full-width far side, no focus concept |
| `layout-commander4-v1.jpg` | 4-player Commander — the primary target |
| `layout-six-v1.jpg` | 6 players — digest rung, two peripherals per side |
| `layout-tokens-v1.jpg` | ~150 permanents — ×N piles, wrapping rows |
| `layout-bighand-v1.jpg` | 16-card hand — fan compression + 44 px paging |
| `layout-combat-v1.jpg` | multi-attacker web across two defenders |
| `layout-stackweb-v1.jpg` | 8-deep mixed stack rail + gang block + stack-entry targeting |
| `layout-phone-v1.jpg` | phone portrait — summary tiles, focused board, stack sheet open |

The mocks are layout evidence, not visual-quality targets — surface
treatment, art, and finish come from the visual system and Phase 1.

## The plane and its fixed slots

The battlefield plane carries three permanent slot groups; they never
reorder, and no region ever renders on top of another (ADR 0023's
by-construction rule, carried onto the plane):

- **The receiver's band** — always the full-width bottom third (±), the
  largest card tiers, with the crest cluster at its outer edge and piles at
  the inner corner. The hand fan, prompts, and the action dock live in
  screen space below/over it and never move (one action home, carried).
- **The far side** — the focused opponent's expanded board: wide, top
  center, one tier below the receiver's.
- **The wings** — peripheral opponents staged outward from the top, up to
  **two per side**, alternating left/right in seat order, at smaller tiers.

The **center corridor** between the far side and the receiver's band stays
clear: it is the interaction area for targeting paths, combat webs, the
resolving object, and temporary staging. Nothing parks there.

## Staging per player count

| Players | Far side | Wings | Wing rung |
| --- | --- | --- | --- |
| 2 | the opponent, full width | — | — |
| 3 | focused opponent | 1 (one side) | full board, larger wing |
| 4 (primary) | focused opponent | 1 per side | full board |
| 5 | focused opponent | 2 left, 1 right | digest (see ladder) |
| 6 | focused opponent | 2 per side | digest |

Seat order is stable (from `GameView.seat_order`, carried): a seat's wing
slot never reshuffles because of game state, and a bystander mounting
mid-game reads the same table as everyone else. Every seat keeps its crest
cluster and piles visible **at every count and every rung** — crests are the
selection surface for player-targeting and attack declaration, so they can
never degrade away. An eliminated seat keeps its slot with the eliminated
treatment (public zones stay browsable).

## Focus model

- At three or more players, **exactly one opponent is focused** (the far
  side). Default focus follows relevance: the active opponent during their
  turn, otherwise the next opponent in turn order.
- **Manual focus** — activating any wing crest, board, or summary tile
  (pointer, touch, or keyboard select/confirm) re-stages that seat into the
  far side. Manual focus is ephemeral presentation state: dropped on the
  next view and re-derived, exactly like selection (one-view
  reconstruction).
- **Candidates pierce every rung** (the one mechanism; carried intent from
  the shipped compact model, made consistent with single focus): a prompt's
  candidate objects **always render individually and pickable in place**, at
  every rung — a digest wing renders its candidate cards on top of its
  digest chips, and a phone summary tile grows a candidate strip. Answering
  a prompt therefore **never requires a focus change**; the far side stages
  the first candidate-bearing board for context only, and focus remains
  exactly one board. (Proven in the six-player mock: a digest wing renders
  its ringed candidate beside its chips.)
- **Off-focus activity is never silent**: a wing seat's action fires the
  quiet crest ping + log entry from the motion grammar, and combat against
  any seat draws its paths and attacked ring regardless of focus.

## Camera

The camera is the plane's single perspective transform (ADR 0030) — fixed
angle, no free camera, no zoom gesture in v1. What "camera movement" exists
is **re-staging**: focus changes tween regions between slots as
scene-geometry changes (300–500 ms staging class, reduced-motion snaps).
Inspect never depends on the camera: it is a fixed screen-space surface at
every geometry (budget rule).

## The degradation ladder

Engaged **per region, independently** — one hoarding player never shrinks
the others (carried rule). In order:

1. **Tier step-down** — the region's card tier drops one rung.
2. **×N folding** — identical-full-state permanents (grouping key including
   the offered-action fingerprint, carried from the shipped client) fold
   into a splayed physical pile with a count badge. Combat participants,
   attachment clusters, the current selection, and any prompt candidate
   always force individual renders — folding never removes a pickable
   object.
3. **Row wrapping** — rows wrap within the region's slot; the slot's height
   is fixed by the stage, so wrapping trades row height, not neighbor
   space.
4. **Digest rung** (wings only) — below a width/count threshold a wing
   board stops drawing cards and shows its **digest**: a count chip for
   **every battlefield permanent category present** — creatures (including
   folded tokens), **other permanents** (artifacts, enchantments,
   planeswalkers, battles), and lands — plus pile counts. A board is never
   summarized to fewer categories than it holds, so a
   noncreature-heavy board can never read as empty. Load-bearing state
   stays visible at the rung: combat participation and attacked/priority
   markers ride the crest and drawn paths, attachment and detailed state
   remain one activation away (manual focus) and always available through
   inspect — and **prompt candidates pierce the rung** (rendered
   individually, per the focus model), so nothing a decision needs is ever
   behind the digest. The far side and the receiver never digest.
5. **Compact change-of-kind** (phone portrait, 3+ players) — the receiver
   keeps the full anatomy at the bottom (fan, dock, prompt strip — the one
   action home never moves); the focused opponent keeps a drawn board; every
   other opponent collapses to a **summary tile**: crest, life, hand/library
   counts, commander data, and the attacked/active markers. Activating a
   tile re-stages focus in place. A phone duel still draws both boards in
   full (tiles engage only at 2+ opponents) — both carried from the shipped
   #400 model, restaged in the new language.

## Stress dispositions

| Stress case (#464 workstream 4) | Mechanism (mock) |
| --- | --- |
| Many identical tokens | ×N piles at rung 2; a swarm batch animates within the budget window (`tokens`) |
| Wide/tall boards | the ladder in its stated order — tier step-down (rung 1), then ×N folding (rung 2), with wrapping (rung 3) absorbing what remains inside the fixed slot (`tokens`) |
| Large hands | the fan compresses spacing and rotation before card size; when exposed spacing would drop below the 44 px floor, the fan **pages** (page size derived from the floor, ≥44 px page controls, board stays visible); focus/hover lifts one card clear at full tier (`bighand`) |
| Six visible players | wings at the digest rung with all-category counts; crests always live; a candidate pierces the digest (`six`) |
| Multi-attacker, multi-defender | paths terminate at defender crests; every attacked seat wears the ring; a defender wing is auto-focus-eligible (`combat`) |
| Deep mixed stack + complex block | the screen-space stack rail condenses to compact rows at depth (controller accent on the **slot**, gold marks the next to resolve) and fits beside the phase/action chrome; three blockers gang one attacker with doubled-stroke links while a stack ability targets a blocker (`stackweb`); on phone the stack opens as a scrollable sheet, readable at depth 8 (`phone`) |
| Phone portrait | rung 5, with ≥44 px tiles, dock, and sheet rows (`phone`) |

## Interaction guarantees

Unchanged and binding at every rung: every physical object stays
addressable in prompts (folding and digests never remove a pick — the
prompt's candidates force renders or open the zone-browser-style pick
surface); every interactive target ≥ 44 px; ownership reads from region
bounds + nameplate + crest (never card color); keyboard and touch reach
focus, tiles, and crests through the same select/confirm verbs.

## Hand-offs and open items

- **Phase 1** implements `stagePlane()` as pure scene geometry (the
  successor of `buildTableScene`'s band layout) with these slots and rungs,
  and the staging tween in the reconciler.
- **#471** supplies environment art composed around the fixed slot groups
  (the corridor and wings constrain where environmental detail may live).
- Open for Phase 1 tuning, not re-decision: the 5-player asymmetric wing
  split (2+1) needs playtest validation; ultrawide should spend surplus
  width on the wings before the corridor; tablet landscape sits between
  desktop staging and the compact change-of-kind and keeps desktop staging
  per the budgets' geometry floor (1180×820).
