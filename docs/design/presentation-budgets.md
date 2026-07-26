# Presentation budgets — performance, devices, animation, accessibility

Normative budgets for the 2.5D client (issue #468, under
[ADR 0029](../decisions/0029-2-5d-presentation-direction.md) /
[ADR 0030](../decisions/0030-2-5d-presentation-architecture.md), master issue
#464). Design and implementation issues cite these numbers instead of taste;
changing a number changes this document, with a linked measurement.

Evidence base: the architecture spike's measurements
([`spike-2-5d-findings.md`](spike-2-5d-findings.md)) plus CPU-throttled runs
recorded below. All in-container numbers are a **software-rendering floor**
(headless Chromium, SwiftShader, no GPU); CPU throttle is the low-end-device
proxy available here.

Two evidence tracks, with different standing:

- **Frame, memory, and timing budgets** are still carried by the container
  proxy, which is **provisional and non-binding** — see
  [§Measured evidence](#measured-evidence--in-container-proxy-provisional-non-binding).
  They bind on the device classes named in
  [§Real-hardware validation](#real-hardware-validation--outstanding), which is
  outstanding.
- **Load and asset budgets** are enforced on the built production bundle by CI
  on every change — see
  [§Enforcement](#enforcement-ci-load-budget-gate-issue-510).

## Device and browser envelope

| Class | Definition | Reference geometry |
| --- | --- | --- |
| Desktop / laptop | Evergreen Chrome, Edge, Firefox (last 2 majors); Safari 16.4+ | 1440×900; supported down to 1280×800 |
| Tablet landscape | Same browsers, touch-first | 1180×820 (the full-anatomy floor, per the blueprint's requirement matrix) |
| Phone | iOS Safari 16.4+, Android Chrome ~2019 mid-tier and up | 390×844 portrait |
| Android build | The planned Android-accessible build (Chrome/WebView engine) | Treated as the phone class; the 4–6× CPU-throttle proxy below stands in until device runs |

Aspect ratios that must be exercised: 16:9, 16:10, 3:2, 21:9 ultrawide, and
narrow desktop windows down to 360 px wide. Input classes: precise pointer +
hover, touch, keyboard — every budget below applies to all three (controller
remains a future capability, not a layout constraint).

## Quality levels

Three levels plus two orthogonal controls — reduced motion and effect
density. Auto-detected on first run, always user-overridable in settings.

| | High | Standard | Lite (floor) |
| --- | --- | --- | --- |
| Scene (perspective plane, staging, cards, tap/travel motion) | full | full | **full** |
| Effects density (particles, glows, streams) | full | reduced (~40%) | brief pulses and edge flashes only |
| Shadows | layered / dynamic | static | single flat shadow |
| Environmental animation, parallax | on | reduced | off (static backdrop) |
| Batch-event staging | per-event stagger | per-event stagger | batch collapses to one stagger |

- **The scene is never degraded.** The perspective composition, ownership
  staging, tactility, and travel motion *are* the game's readability — and
  they are the measured-cheap part (see evidence). Lite must not regress to
  the pre-pivot utilitarian dashboard; it is the same scene with quiet
  effects.
- **Reduced motion is orthogonal to quality**: at any level it snaps every
  animation to its end state with zero layout or state difference (the
  shipped contract, carried forward). An **effect-density control** is
  likewise available independent of the quality level.

### First-run auto-detection (implemented, issue #505)

The settings surface (`clients/web/src/table/PresentationSettings.tsx`, backed by
the device-local `table/settings/presentationSettings.ts`, reachable from both the
front door and the in-match game menu) picks a starting quality level on first run
and always lets the player override it — the chosen level is shown, never applied
silently, and every choice persists device-local (the ADR 0024 / ADR 0027 idiom,
no protocol change). The heuristic is deliberately conservative — misdetecting a
capable device down to Lite is worse than starting at Standard:

- **Default: Standard.** No signal, or any healthy-capability reading, starts here.
- **Drop to Lite** only on a clear low-capability signal, in order: `saveData`
  (Data Saver on) → `navigator.deviceMemory < 4` GB → `navigator.hardwareConcurrency
  ≤ 2` cores.
- **High is never auto-selected** — it is an explicit opt-in.

Density defaults to `reduced` and motion to `system` (follow the OS); the motion
preference composes with `prefers-reduced-motion` as **OS-on OR user-on ⇒ reduced**.
The OS request is authoritative — an accessibility setting is never overridden by an
in-app preference — so `reduced` forces the snap even with the OS setting off, while
`full` prefers motion only when the OS allows it and still yields to an OS "reduce".

## Performance budgets

| Budget | Desktop | Tablet / phone (mid) | Floor (Lite) |
| --- | --- | --- | --- |
| Sustained frame rate, idle and during animation | 60 fps | 60 fps | 30 fps |
| p95 frame time under stress states | ≤ 16.7 ms | ≤ 16.7 ms | ≤ 33.3 ms |
| Input → visible response (select, tap, dock action) | ≤ 100 ms | ≤ 100 ms | ≤ 100 ms |
| JS heap, in-match | ≤ 256 MB | ≤ 128 MB | ≤ 128 MB |
| Reconnect / fast-forward full scene rebuild | ≤ 50 ms | ≤ 100 ms | ≤ 100 ms |
| Scene DOM budget | ≤ 15 000 nodes total; ≤ 12 nodes per card face at battlefield tiers | same | same |

Hard rules (from ADR 0030, binding at every level):

- **Input is never gated on animation.** The authoritative view applies
  immediately; hit targets exist at their final rects the moment a scene is
  built.
- **The effects layer idles at zero cost** (render-on-demand; no per-frame
  work while nothing is animating).
- **Scene updates are incremental** (reconcile by entity id); full rebuilds
  are reserved for reconnect/fast-forward.
- **Sustained or dense effects run on the WebGL layer**, never a
  full-viewport 2D canvas (measured disqualification, see evidence). Particle
  caps per level: High ≤ 400 live, Standard ≤ 150, Lite ≤ 40.

Stress states the budgets are validated against (from #464 workstream 4):
four-player Commander at ~120 permanents; a 240-permanent degenerate board;
six visible players; 12+ card hands; ×N token walls; multi-defender combat
with drawn paths; an 8-deep stack — each while an animation batch and a
targeting session are live.

## Load and asset budgets

| Budget | Number |
| --- | --- |
| Interactive code bundle (gzipped, excluding art/audio) | ≤ 1.0 MB |
| Bundled fonts | ≤ 60 KB total (today: ~14 KB) |
| One environment theme (compressed) | ≤ 1.5 MB |
| Total first-match download at default quality (code + default theme + UI assets) | ≤ 4 MB |
| Cold start → interactive lobby (mid-tier phone, 4G) | ≤ 5 s |
| Lobby → match presentation ready (theme cached) | ≤ 2 s |

Assets beyond the default theme lazy-load and cache with content-hashed,
cache-forever URLs; a match must be fully playable before any optional asset
(alternate themes, audio) arrives. Card art stays governed by ADR 0024
(player-side, device-cached) and never blocks play. Asset formats, licensing,
and versioning policy are issue #471's deliverable and must fit these size
ceilings.

The **card-frame plates** (issue #570, `card-representation.md` §3.12) are the
one set that can never be deferred: the frame is on every card, so a lazily
loaded frame is a frameless first match. They are budgeted accordingly — ~46 KB
for the whole set, which is what the two decisions in §3.12 buy. Carrying no
body colour means one set instead of one per theme; being nine-sliced on a
ratio of `W` means one set instead of one per tier.

### Enforcement (CI load-budget gate, issue #510)

The three size budgets are checked by `clients/web/scripts/checkLoadBudget.js`
(`npm run budget`). It rides `make client-check` immediately after
`npm run build`, so the Client CI job measures the **built production bundle in
`dist/`** — never dev-server output — and fails on any violation. The ceilings
live in `clients/web/scripts/loadBudget.js` and nowhere else; that module is
pure and unit-tested (`loadBudget.test.js`) including the over-budget paths, so
the failure behaviour itself is covered.

How a built file is counted:

| Class | Matches | Counted as | Budgets |
| --- | --- | --- | --- |
| `code` | `.js`, `.mjs`, `.css`, `.html` | gzipped (zlib default level, what a CDN compresses with on the fly) | interactive bundle, first match |
| `font` | `.woff2`, `.woff`, `.ttf`, `.otf` | transfer size (already compressed) | fonts, first match |
| `asset` | everything else that ships | gzipped, or raw for already-compressed formats | first match |
| `deferred` | `card-art/**` (ADR 0024, player-side and opt-in) and `lazy/**` | not counted | none |

The classification is deliberately fail-closed: an unrecognized file lands in
`asset` and counts against the first-match set. Keeping something out of that
set is an explicit act — put it under `lazy/`, the convention for alternate
environment themes and audio, which must never block a playable match. The gate
also refuses to run against a build made with `VITE_RUNE_FIXTURE_HARNESS=true`,
since that bundle carries the fixture route and is not what ships.

Measured with the issue #548 production-asset drop (`npm run build &&
npm run budget`):

| Budget | Ceiling | Measured | Used | Headroom |
| --- | --- | --- | --- | --- |
| Interactive code bundle (gzipped, excl. art/audio) | ≤ 1.0 MB | 311.64 kB | 31.2 % | 688.36 kB |
| Bundled fonts | ≤ 60 KB | 14.52 kB | 24.2 % | 45.48 kB |
| First-match download at default quality | ≤ 4 MB | 714.11 kB | 17.9 % | 3.29 MB |

Composition: code 311.64 kB gzipped, `rune-display` WOFF2 14.52 kB, and
first-match presentation assets 387.95 kB. Deferred Runic Vale upgrades,
alternate studies, and card art are listed by the gate but excluded from the
first-match sum.

The environment and repository-weight ceilings are enforced separately by
`npm run assets`, which checks the ADR 0031 ledger, hashes, and shipping trees.
Runic Vale is **475.36 kB** across every quality layer (31.7 % of 1.5 MB);
all 27 presentation assets total **1.58 MB** (13.2 % of 12 MB).

The two wall-clock budgets stay owed to the real-hardware runs because neither
is a property of a build artifact:

- Cold start → interactive lobby ≤ 5 s and lobby → match presentation ready
  ≤ 2 s — device-and-network timings, measured per
  [§Real-hardware validation](#real-hardware-validation--outstanding).

## Animation budgets

Durations are Standard-quality defaults; High may stagger more richly within
the same caps; reduced motion snaps everything.

| Motion class | Duration | Notes |
| --- | --- | --- |
| Micro feedback (hover lift, selection, legality pulse) | 80–150 ms | never delays the action it decorates |
| Tap / untap | 150–250 ms | rotation tween; footprint pre-reserved |
| Zone travel (draw, play, discard, exile, die) | 250–400 ms | FLIP ghost; destination addressable at 0 ms |
| Staging / focus / camera change | 300–500 ms | scene-geometry tween |
| Resolution / impact effects | ≤ 600 ms | effects layer; gameplay state already applied |
| Turn / phase / priority transitions | ≤ 500 ms | non-blocking banner or staging cue |
| Simultaneous batch (mass untap, board wipe, token swarm) | ≤ 80 ms stagger per item, ≤ 800 ms total window | items beyond the window land together |

- Any presentation sequence longer than **600 ms** must be skippable
  (interaction or setting), and rapid successive views collapse to the latest
  (fast-forward) — presentation never buffers gameplay.
- Engine, server, headless, and AI-only games never wait on any of this
  (ADR 0029 invariant).

## Accessibility budgets

- Interactive targets ≥ **44 CSS px** in every input mode; a battlefield card
  at the smallest tier keeps at least a 44 px-wide hotspot.
- Text: chrome body ≥ 12 px; critical values (life, P/T, counts, timers)
  ≥ 12 px semibold; card names ≥ 11 px at battlefield tiers, with the
  glyph + inspect path carrying identity when a tier is too small for prose.
  Text scaling to 125 % must not clip critical values or shrink hit targets.
- Contrast: readable text ≥ 4.5:1 against its surface; state badges and
  indicator shapes ≥ 3:1.
- Non-color channels for every state (carried from
  [`ui-requirements.md`](ui-requirements.md)): ownership = region position +
  nameplate; legality = gold **edge-bar shape**; selection = ring; targeting =
  ring + drawn path; priority = crest treatment + position; tap = rotation.
  No state may be color-only at any quality level.
- Inspection is independent of battlefield card size: the inspect surface
  renders at a fixed screen-space tier at every geometry.
- Reduced motion: every animation, at every quality level, snaps with no
  layout or state difference; prompt and log text stays in the DOM for
  screen readers.

## Measured evidence — in-container proxy (provisional, non-binding)

> **Provisional.** Every number in this section comes from headless Chromium
> with software rendering and a CPU throttle, in a container with no GPU. It is
> a useful floor and a regression tripwire, and it is **not** evidence that any
> frame, heap, or rebuild budget is met. Nothing here satisfies the budgets
> document: the binding record is the device-class evidence in
> [§Real-hardware validation](#real-hardware-validation--outstanding), which
> replaces these rows once it exists.

Unthrottled software rendering (from the spike):
idle/tween **~55–60 fps at up to 245 perspective DOM cards**; full-viewport
2D-canvas effect repaints ~9 fps (the disqualifying number); reconnect
rebuild 1.4–6.5 ms; JS heap ~1.4 MB for the scene alone.

CPU-throttled (low-end proxy, same harness, software rendering):

| Scenario | 4× throttle | 6× throttle |
| --- | --- | --- |
| Idle @125 cards | 57.3 fps (p95 16.8 ms) | 57.1 fps (p95 16.7 ms) |
| Mass untap tween @125 | 50.3 fps (p95 33.4 ms) | 48.0 fps (p95 16.8 ms) |
| 2D-canvas arrows + bursts @125 | 9.3 fps | 8.1 fps |
| Reconnect rebuild | 10–43 ms | 10–30 ms |

Reading: the scene path stays above the 30 fps floor (48–57 fps,
~1.6–1.9× headroom) even under the combined handicap of software rendering
and a 6× CPU throttle; the reconnect budget holds with ≥2× margin; and the
2D-canvas effects path fails every tier, which is why ADR 0030's
WebGL-effects rule is a budget-level requirement. One at-the-line result:
the 4× mass-untap p95 (33.4 ms) grazes the floor-tier p95 cap (≤ 33.3 ms) —
within the harness's sampling resolution, so it is recorded as at the line,
not a pass, and is one more reason the floor budgets bind on the real
hardware runs below rather than on this proxy.

## Real-hardware validation — OUTSTANDING

**Status: not done, and owed by a human.** No measurement on any of the three
required device classes has been recorded. Physical hardware cannot be reached
from the agent sandbox, so issue #510 landed only its agent-implementable half
— the CI load-budget gate above and this procedure. Until the tables below are
filled in:

- the frame, heap, rebuild, DOM, and cold-start budgets have **no binding
  evidence**, only the container proxy marked provisional above;
- the at-the-line 4× mass-untap p95 (33.4 ms against the ≤ 33.3 ms Lite cap)
  stands **unresolved**; and
- the Phase 4 exit gate (#511) **cannot claim** the presentation-budget
  criterion.

Everything the runs need is in place: the fixture harness, the probe script,
and the skeleton tables. What is missing is the devices.

### Required device classes

| Class | Requirement | Quality levels to run | Status |
| --- | --- | --- | --- |
| A — mid-tier Android phone | ~2019 mid-tier (e.g. Pixel 3a / Galaxy A50 class), Android Chrome | Standard and Lite | outstanding |
| B — recent iPhone or iPad | Current iOS/iPadOS Safari | Standard and Lite | outstanding |
| C — integrated-GPU laptop | No discrete GPU; evergreen Chrome or Safari | Standard, plus Lite for the floor tier | outstanding |

### Procedure

Repeatable, and the same on every class. Record the client commit with each
run.

1. **Build the harness bundle** (measurement only — the CI gate refuses to
   size this build):
   `cd clients/web && VITE_RUNE_FIXTURE_HARNESS=true npm run build`
   then serve it on the LAN: `npm run preview -- --host`.
2. **Open the fixture on the device**:
   `http://<host>:4173/fixtures/2.5d?scenario=commander4&quality=standard`.
3. **Attach a console** to that page — Android Chrome via desktop Chrome's
   `chrome://inspect`; iOS Safari via macOS Safari's Develop menu; the laptop
   via its own devtools.
4. **Paste** [`clients/web/scripts/deviceBudgetProbe.js`](../../clients/web/scripts/deviceBudgetProbe.js)
   into the console, then run:
   `await runeDeviceBudgetProbe({ label: 'Pixel 3a — Android 12, Chrome 126' })`.
   It walks the required scenarios, reads the live
   [`fixture/metrics.ts`](../../clients/web/src/fixture/metrics.ts) report for
   each, and prints a Markdown table. Paste it into this document under the
   matching class below.
5. **Repeat at `?quality=lite`** — the level is read from the URL at mount, so
   it takes a reload — and paste that block too.
6. **Mass-untap verdict**: on class A at Lite, select `commander4`, step the
   sequence to the tap/untap frames (`selectFrame`), and let the probe sample
   the tween. Record the p95 against the ≤ 33.3 ms Lite cap and state a verdict
   in §Verdicts below. This is the specific at-the-line result carried over
   from the proxy run.
7. **Real client, not just the fixture**: play a four-player Commander game on
   each class and record cold start → interactive lobby and lobby → match
   presentation ready, with the device on a throttled 4G profile for class A.
8. **Every miss gets a disposition** — fixed, or an exception recorded here
   with its rationale and a linked follow-up issue. A blank row is not a pass.

### Evidence — class A, mid-tier Android phone (~2019)

**Device:** _pending_  **User agent:** _pending_  **Viewport / DPR:** _pending_
**Client commit:** _pending_  **Run date:** _pending_

| Scenario | Quality | Idle fps | Idle p95 ms | Tween fps | Tween p95 ms | Rebuild ms | DOM nodes | Heap MB | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commander4 | — | — | — | — | — | — | — | — | outstanding |
| six | — | — | — | — | — | — | — | — | outstanding |
| tokens | — | — | — | — | — | — | — | — | outstanding |
| big-hand | — | — | — | — | — | — | — | — | outstanding |
| combat-web | — | — | — | — | — | — | — | — | outstanding |
| deep-stack | — | — | — | — | — | — | — | — | outstanding |
| phone | — | — | — | — | — | — | — | — | outstanding |

### Evidence — class B, recent iPhone / iPad Safari

**Device:** _pending_  **User agent:** _pending_  **Viewport / DPR:** _pending_
**Client commit:** _pending_  **Run date:** _pending_

| Scenario | Quality | Idle fps | Idle p95 ms | Tween fps | Tween p95 ms | Rebuild ms | DOM nodes | Heap MB | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commander4 | — | — | — | — | — | — | — | — | outstanding |
| six | — | — | — | — | — | — | — | — | outstanding |
| tokens | — | — | — | — | — | — | — | — | outstanding |
| big-hand | — | — | — | — | — | — | — | — | outstanding |
| combat-web | — | — | — | — | — | — | — | — | outstanding |
| deep-stack | — | — | — | — | — | — | — | — | outstanding |
| phone | — | — | — | — | — | — | — | — | outstanding |

Safari does not expose `performance.memory`, so the heap column stays blank on
this class; record heap from the Web Inspector's Timelines instead.

### Evidence — class C, integrated-GPU laptop

**Device:** _pending_  **User agent:** _pending_  **Viewport / DPR:** _pending_
**Client commit:** _pending_  **Run date:** _pending_

| Scenario | Quality | Idle fps | Idle p95 ms | Tween fps | Tween p95 ms | Rebuild ms | DOM nodes | Heap MB | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commander4 | — | — | — | — | — | — | — | — | outstanding |
| six | — | — | — | — | — | — | — | — | outstanding |
| tokens | — | — | — | — | — | — | — | — | outstanding |
| big-hand | — | — | — | — | — | — | — | — | outstanding |
| combat-web | — | — | — | — | — | — | — | — | outstanding |
| deep-stack | — | — | — | — | — | — | — | — | outstanding |
| phone | — | — | — | — | — | — | — | — | outstanding |

### Load timings on real hardware

| Measurement | Budget | Class A (4G) | Class B | Class C | Status |
| --- | --- | --- | --- | --- | --- |
| Cold start → interactive lobby | ≤ 5 s | — | — | — | outstanding |
| Lobby → match presentation ready (theme cached) | ≤ 2 s | — | — | — | outstanding |
| One environment theme (compressed) | ≤ 1.5 MB | 475.36 kB (artifact gate) | same artifact | same artifact | pass |

The three size budgets in the same table are covered by CI and need no device
run; see [§Enforcement](#enforcement-ci-load-budget-gate-issue-510).

### Verdicts

| Question | Verdict | Evidence |
| --- | --- | --- |
| Mass-untap tween p95 on real Lite-class hardware (proxy: 33.4 ms vs ≤ 33.3 ms cap — at the line, not a pass) | **outstanding** | — |
| Every frame/heap/rebuild/DOM budget row on classes A, B, C | **outstanding** | — |
| Interactive bundle, fonts, first-match size | **pass** | CI gate, [§Enforcement](#enforcement-ci-load-budget-gate-issue-510) |

The spike harness (`window.__spike` in
[`prototypes/ui-2-5d-spike-v1.html`](../../prototypes/ui-2-5d-spike-v1.html),
protocol in [`spike-2-5d-findings.md`](spike-2-5d-findings.md)) remains
available for a like-for-like comparison against the original spike numbers,
but the shipped client's fixture route is the primary harness now — it exercises
the real `PlaneReconciler` + `CardFace` stack.
