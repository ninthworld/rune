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
proxy available here. The re-validation obligations on real hardware are
listed at the end.

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

## Measured evidence (in-container floor)

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

## Re-validation obligations

Before the Phase 2 (playable vertical slice) exit of #464, re-run the
harness scenarios (`window.__spike` in
[`prototypes/ui-2-5d-spike-v1.html`](../../prototypes/ui-2-5d-spike-v1.html),
protocol in [`spike-2-5d-findings.md`](spike-2-5d-findings.md)) on real
hardware: one mid-tier Android phone (~2019), one recent iPhone/iPad Safari,
one integrated-GPU laptop — plus the real client once its scene exists. Frame
budgets bind on those devices, not on the container proxy. Load budgets are
verified against the built bundle in CI once the Phase 1 client exists.
