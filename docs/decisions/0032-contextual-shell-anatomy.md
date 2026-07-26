# ADR 0032: Contextual shell anatomy — the battlefield is the interface

- Status: accepted
- Date: 2026-07-25
- Issue: #464 (master 2.5D visual pivot), #534
- Supersedes: [ADR 0023](0023-fixed-shell-anatomy.md)

## Context

[ADR 0023](0023-fixed-shell-anatomy.md) replaced a floating-chrome table with a
**carved, fixed layout**: a permanent top status bar, opponent panels, the
receiver's battlefield panel, a right rail for stack and activity, and a bottom
shell owning identity, piles, hand, and the action dock. Regions never floated
over one another and never reordered.

That decision was correct for what it was solving. The floating model produced a
recurring defect class — chrome overlapping the hand, action popups clipping at
the screen edge, the tray drifting relative to its neighbours, prompt overlays
colliding with controls — and no amount of per-region polish could fix a failure
mode the architecture guaranteed. Carving the regions made every UI state's home
explicit and made travel animations and drag targets deterministic.

Two things have changed since.

**1. The approved visual direction is battlefield-first.** The 2.5D pivot
(ADR 0029, ADR 0030) culminated in an approved baseline set (issue #547,
commit `e58300b`): `rune-2.5d-interface-baseline.jpg`, `rune-player-control-ui.jpg`,
`rune-zones-interaction.jpg`, and the environment family sheet. Every one of them
shows the same thing — a large open illustrated arena carrying the cards, with
small edge-anchored controls that appear where and when they are relevant. There
is no permanent top bar, no permanent bottom decision dock, and no permanent
right rail in any approved image. The controls live in a lower-right cluster; the
phase plaque sits at its foot; decisions appear as contextual plaques near their
subject. Issue #534 exists to perform exactly that change, and issues #531–#535
compose against it.

**2. ADR 0023's central claim did not hold empirically.** Its Consequences
section stated that "the overlap/clipping defect class is eliminated by
construction." It was not. Issue #528 — fixed while implementing this sprint —
found the mulligan decision sheet rendering at `z-index: 14` beneath a shell
region at `z-index: 20`, where that region's z-index created a stacking context
no descendant could escape. The result was a soft-lock: the game asked a question
the player could not read or answer. The same issue found hand cards staged with
roughly half their height outside their band, and the compact composition
overlapping the hand and controls mutually.

Those are precisely the defects ADR 0023 said carving had eliminated, occurring
inside the carved shell it prescribed. Fixed regions did not prevent overlap;
they relocated where overlap happened and made it less visible in review. What
actually prevents it is an explicit, tested layer and containment contract —
which is what #528 introduced (`clients/web/src/table/live/shellLayout.ts`), and
which is independent of whether the regions are permanent or contextual.

## Decision

**The battlefield owns the viewport. Chrome is contextual, not carved.**

1. **No permanent dashboard regions.** The always-present top status bar, bottom
   decision dock, and right stack/activity rail are removed from the normal match
   composition. Phase, priority, actions, settings, stack, and history are
   presented as compact contextual surfaces that appear where and when they are
   relevant and are otherwise absent.

2. **One action home is retained, and it moves.** ADR 0023's commitment stands:
   every server-offered action renders in a single action home, selecting an
   entity routes its actions there, and per-card action popups remain removed.
   ADR 0004's contextual echo *is* that home. Its **location** changes from the
   bottom shell beside the hand to the lower-right control cluster the baselines
   show. `ActionDock.tsx` is refactored accordingly under #534.

3. **One blue-emphasised primary action per state.** The cluster carries at most
   one primary control at a time, with its label rendered verbatim from the
   server. Utility controls and the phase plaque are subordinate to it.

4. **Overlap is prevented by contract, not by geometry.** Every surface that can
   cover another declares its layer from the shared `--rune-z-*` ladder, and the
   binding rule is: *a layer may only be covered by a layer the player explicitly
   invoked and can dismiss without answering it.* A decision the game is waiting
   on outranks chrome. Containment and non-overlap are asserted by test
   (`shellLayout.ts`, `shellLayout.test.ts`), not by inspection. This contract,
   not permanence, is what retires the ADR 0023 defect class.

5. **Zone homes stay stable.** Each seat keeps a fixed, physical home for its
   library, graveyard, exile, and command zone
   ([`zone-geography.md`](../design/zone-geography.md)). Contextual chrome does
   not make zone travel non-deterministic, because zones are scene objects, not
   chrome.

6. **The density ladder stays.** Degenerate boards are absorbed by tier
   step-down and ×N folding
   ([`presentation-budgets.md`](../design/presentation-budgets.md)), never by
   unbounded growth.

7. **The hand remains a shell region**, not a scene-drawn object. ADR 0023 moved
   it out of the battlefield scene; that part was right and is unchanged.

The design authorities for this anatomy are
[`control-language.md`](../design/control-language.md) (#543),
[`stack-and-relationships.md`](../design/stack-and-relationships.md) (#541),
[`seat-identity.md`](../design/seat-identity.md) (#539), and
[`zone-geography.md`](../design/zone-geography.md) (#540). Where they and this
ADR disagree, this ADR governs the anatomy and they govern the detail.

## Consequences

- The approved baselines become implementable. Under ADR 0023 they were not:
  every one of them contradicts a permanent region the ADR required.
- The battlefield is visually primary, which is the entire point of the 2.5D
  pivot. Cards get the viewport back.
- **The overlap/clipping risk returns as a real risk** and must be held off by
  the layer contract and its tests rather than by construction. This is the
  explicit trade. It is defensible only because the fixed shell demonstrably did
  not deliver the guarantee it promised, and because the contract is now tested
  where the guarantee never was.
- Contextual surfaces need designed enter/exit behaviour and reduced-motion
  equivalents. A control that appears and disappears is harder to learn than one
  that is always present; discoverability now depends on the surface appearing
  reliably at the moment it becomes relevant.
- Accessibility gets harder in a specific way: a screen-reader user cannot rely
  on a stable landmark set. Every contextual surface must announce itself on
  appearance and remain keyboard-reachable, per `control-language.md` §11–12.
- `TopBar.tsx`, `Rail.tsx`, `MePanel.tsx`, `PanelChrome.tsx`, `ActionDock.tsx`,
  and `PhaseIndicator.tsx` are all reworked or retired under #534. `styles.ts`'s
  fixed-shell geometry and `scene/types.ts`'s carved band rects lose their
  ADR 0023 rationale and follow.
- **Status (#567).** The one action home is now the whole lower-right corner, not
  just the control cluster: the decision surface (`table/decision/DecisionArea`)
  stacks directly above the cluster's primary, utilities, mana reservoir, and
  phase plaque, and is the single place a decision is stated and answered.
  `PromptStrip.tsx` and `DecisionSheet.tsx` are deleted — the first had no laid-out
  home left after this ADR removed the bottom shell, and both restated a question
  the plaque was already titling. `MePanel.tsx` is deleted rather than reworked;
  its only surviving content was the receiver's mana pool, which is now the
  cluster's reservoir. Commitment 4's layer contract is unchanged and is exactly
  what let the decision move: the area is a sibling of the shell's regions on the
  `decision` rung, so no chrome can paint over it.
- `ui-blueprint.md` and `ui-redesign-plan.md` described the ADR 0023 anatomy and
  are superseded on that point; #511's audit retires them.
- ADR 0003 (DOM/canvas split), ADR 0004 (subject-owned actions), ADR 0029 and
  ADR 0030 (2.5D direction and architecture) are unaffected.
