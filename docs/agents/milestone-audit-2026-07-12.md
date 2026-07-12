# M1–M3 milestone audit — pilot run (issue #188)

Status: **single unreviewed AI pass — Phase 1 (evidence) and Phase 2 (audit) only.**
Not a closeout decision. Nothing here closes a milestone, creates an issue, or
edits `docs/roadmap.md`'s checkboxes — those require the independent review,
human gate, and roadmap PR that [ADR 0017](decisions/0017-milestone-stewardship-cycle.md)
defines and that issue #188 requires before any of it happens. This document is
the auditable output of the parts of that cycle a single sandboxed session run
without GitHub write access, a second AI provider, or a maintainer present can
actually perform, kept as an example artifact for #187 per #188's stated goal.

## Phase 1 — evidence snapshot

- **Audited commit:** `e934917c735259362402ed9d7c97e652d96ae6cb` (`origin/main` /
  `main` at the start of this session).
- **Reconciliation time:** 2026-07-12T08:22Z.
- **Scope limitation:** this session has no GitHub credentials and no reachable
  GitHub API for this repository (`git remote -v` points at a local mirror that
  is unavailable mid-session, and the `gh` CLI is not installed) — this is a
  hard boundary of the agent-task sandbox, not a design choice. Every claim
  below is therefore sourced from **this checkout** (`git log`, source, docs)
  rather than the live GitHub issue/PR/milestone API. Where `docs/roadmap.md`
  cites an issue number, that citation is carried forward verbatim as the
  issue identifier, but its live state (open/closed, labels, milestone
  assignment) was **not** independently verified against GitHub and must be
  before any closeout decision.

### Verification run (at the audited SHA)

| Command | Result |
|---|---|
| `make check` (engine-lint, engine-test, client-check, client-audit, runner-test) | **pass** — all 141 runner tests, full Rust workspace, and the client suite green. |
| `cargo test --workspace` | **pass** — `rune_engine` 245, `rune_server` 82 (+4 integration files), `rune_protocol` 33, `rune_cli` 45 (+2 integration files), `rune_server` bin/integration tests all `0 failed`. |
| `cargo deny check advisories licenses bans sources` | **pass** — `advisories ok, bans ok, licenses ok, sources ok`. |
| `make e2e` | **not run.** `npx playwright install --with-deps chromium` requires root (`su: Authentication failure` in this sandbox) and no pre-provisioned Chromium exists at `~/.cache/ms-playwright`. This is an environment limitation, not a code failure — the E2E specs below were confirmed present and their intent read, but never executed in this session. A real `make verify`/`make e2e` run in an environment with the pinned browser (or the provider sandbox image, #216) is required evidence before any E2E-dependent criterion is marked `met` in a closeout decision. |

### TODO / stub sweep

`rg -e TODO -e 'todo!' -e 'unimplemented!' -e FIXME` across `crates/` and
`clients/web/src` (excluding test files): **zero matches.** Gaps in this
codebase are consistently documented in prose (`docs/roadmap.md`'s "Where we
are", `docs/rules-coverage.md`'s `partial` rows) rather than left as code
markers.

### ADR / protocol state

All 17 ADRs (`0001`–`0017`) are `Status: accepted`; none `proposed` or
`superseded`. `docs/protocol.md` documents the lobby contract, `GameView`
shape, combat/multi-select requirements, and the `option`/`select_from_zone`/
`order` prompt slots (§"Prompt slots", lines ~175–302) — i.e. the M3 prompt-type
protocol work (#156) is documented, not just coded.

### Rules coverage

`docs/rules-coverage.md` has 43 rows citing `CR 1xx`/`3xx`/`5xx`/`6xx`/`7xx`.
Every CR citation named in M3's exit criteria (601.2c, 608.2b, 603.6c/700.4,
303.4/704.5m/n, 702.7/702.9/702.10/702.15/702.17/702.19/702.20, 614.1c/614.12)
has at least one matching row with a code anchor and a test anchor.

### Architectural invariants and legal constraints (AGENTS.md hard rules)

- **Zero I/O in the engine:** `crates/rune-engine/Cargo.toml`'s only
  dependencies are `serde`/`serde_json`, with a comment pinning that to
  ADR 0006 and requiring a new ADR for anything further — no tokio, sockets,
  timers. Holds.
- **Zero game logic in the client:** a grep for engine-only vocabulary
  (`is_legal`, `combat_damage`, `state_based_action`) in `clients/web/src`
  found one hit, a string literal enumerating a display label in
  `protocol.ts` (a UI-facing enum tag, not a computation) — no rules logic
  found client-side. Holds, at the depth this pass checked (not an
  exhaustive audit).
- **No card images / WotC branding:** no `.png`/`.jpg`/`.jpeg`/`.webp` assets
  under `clients/web/public` or `clients/web/src`. Consistent with
  `docs/brief.md`'s "Legal Considerations" (custom rendering, no bundled
  images). Card names in `data/oracle.json` were not individually checked
  against real Magic card names in this pass — recommend the independent
  review spot-check a sample for the M3 starter-set criterion once real
  content lands (currently moot: the fixture set is 32 cards, not the
  starter set).

### Documented gaps carried forward (from `docs/roadmap.md`'s prose)

- Engine effect vocabulary and multi-select wiring gaps named in the "Where we
  are" section as of the doc's own last edit (see finding below) — largely
  superseded by code landed since, see the audit matrix.
- `rules-coverage.md`'s own `partial` rows name real, still-open gaps that are
  **not** exit-criterion blockers per se (e.g. double strike, regeneration,
  planeswalkers/battles, non-combat deathtouch/lifelink, player-chosen damage
  assignment order) — these are scope decisions, not defects, and are called
  out here only so they aren't rediscovered as "new" gaps in M4+ planning.

### Key finding: `docs/roadmap.md` is materially stale relative to `main`

`docs/roadmap.md` was last edited at commit `0f20d86` ("docs: reconcile roadmap
with shipped M1/M2 wave and break M3 into issues"), which the audited history
places well before the current tip. Every issue `docs/roadmap.md` lists as
**open** in its M1/M2 feature tables and exit criteria (#140–#145) and the
bulk of the M3 feature table (#146–#159) has a corresponding commit already on
`main`:

| Issue (per roadmap) | Roadmap says | Commit on `main` |
|---|---|---|
| #144 — e2e real-server smoke tier | open | `e882359 test(e2e): real-server two-client smoke tier (ADR 0011)` |
| #145 — e2e scripted full game | open | `66a80af test(e2e): scripted full game to victory (two clients)` |
| #140 — server requirements projection | open | `28ca160 feat(server): project multi-select requirements candidates into the view` |
| #141 — client game-over screen | open | `48bc353 feat(client): game-over screen mirroring GameView.result` |
| #142 — client stack panel | open | `59ced94 feat(client): stack panel rendering GameView.stack` |
| #143 — client multi-select UX | open | `091d80b feat(client): multi-select UX for combat declarations and bottoming` |
| #146 — oracle/printing split | open | `dd33bff feat(engine): oracle/printing split with PrintingDatabase (ADR 0013)` |
| #147 — cast every card type, real timing | open | `ac76fbe feat(engine): cast all card types with correct timing (CR 117.1a/304.1/307.1)` |
| #148 — spell targets + counterspell | open | `ef63e62 feat(engine): spell targets at cast + counterspell (CR 601.2c/701.5)` |
| #149 — effect IR wave | open | `0a51cd9 feat(engine): effect IR wave — damage/destroy/life/counters` |
| #150 — until-end-of-turn pump | open | `5aedb05 feat(engine): until-end-of-turn pump with cleanup expiry (CR 514.2)` |
| #151 — dies triggers | open | `0b114a1 feat(engine): dies / leaves-battlefield triggers (CR 603.6c/700.4)` |
| #152 — auras | open | `1109adc feat(engine): auras — enchant, attachment, and aura SBAs (CR 303.4/704.5m/n)` |
| #153 — combat keywords I | open | `48cf411 feat(engine): combat keywords I — flying/reach/vigilance/haste` |
| #154 — combat keywords II | open | `d87db4a feat(engine): combat keywords II — first strike/trample/deathtouch/lifelink` |
| #155 — replacement pipeline | open | `69ca5a8 feat(engine): ETB replacements — enters tapped / with counters (CR 614)` |
| #156 — prompt types on the wire | open | `c1876bc feat(protocol): option/select_from_zone/order prompt types (#156)` |
| #157 — client prompt UX | open | `e4700d8 feat(client): prompt UX for option/select_from_zone/order (#157)` |
| #158 — format registry + deck validation | open | `25bf3c6 feat(server): format registry + deck validation in pre-game gate (ADR 0013 §4)` |
| #159 — CLI rule-based agent | open | `4c8e40b feat(cli): rule-based agent that plays a legal game to a win` |

**#160 (starter set + preconstructed decks) is the one exception** — see the
M3 matrix entry below; it has genuinely not landed.

This is the single most important output of this audit: the roadmap's
"open queue" section and per-milestone checkboxes have not been reconciled
since the M3 breakdown was filed, even though the features have been merged.
Closing M1/M2 and all but one M3 criterion is very likely correct, but that
determination — and the roadmap edit — is exactly what Phase 3's human gate
and Phase 5's roadmap PR are for, not this document.

## Phase 2 — audit matrix

Status values follow ADR 0017 exactly: `met`, `partial`, `unmet`, `obsolete`.
Issue-closure state is **not** independently verified (see the scope
limitation above) — every citation below is a code/doc/test anchor in this
checkout, per ADR 0017's "issue closure is never sufficient evidence" rule.

### M1 — Take a seat

| # | Criterion | Status | Evidence | Gap / action |
|---|---|---|---|---|
| 1 | Playwright e2e drives real Chromium, connection → room → join → decks → ready → first `GameView`, in CI | `met` (code); **evidence class incomplete** | `clients/web/e2e/real-server-smoke.spec.ts` (`real-server two-client smoke` / `two browsers walk the full lobby to a rendered first GameView on both`), harness `real-server.ts`. Roadmap marks this unchecked citing #144 as still open — contradicted by the commit above. | The spec exists and reads as exactly this criterion, but **no run of it was observed in this session** (browser unavailable, see Phase 1). Before marking `met` in a closeout: run it (or confirm its CI job is green) at this SHA. |
| 2 | `docs/protocol.md` documents lobby contract; `rune-protocol` round-trips it; "two messages" framing amended | `met` | `docs/protocol.md` (lobby section); `crates/rune-protocol/src/lib.rs` round-trip tests. Already checked `[x]` in roadmap; no evidence contradicts it. | — |
| 3 | Explicit rooms with config, no auto-seating, no game before all seats filled/decked/ready | `met` | `crates/rune-server/src/lobby.rs`, `crates/rune-server/tests/lobby.rs`, `tests/pregame.rs`. Already `[x]`. | — |
| 4 | Reconnect via session token | `met` | `crates/rune-server/tests/lobby.rs :: a_returning_socket_reconnects_to_its_held_seat_by_token_end_to_end`, `lobby.rs :: issue_113_reconnect_token_never_leaks_a_held_seat_referencing_48`, `room.rs :: reconnect_is_brought_current_with_a_full_view`. Already `[x]`. | — |
| 5 | Engine `GameSetup`, deck loading, seeded shuffle, opening hands, London mulligan | `met` | `crates/rune-engine/src/mulligan.rs :: cr_103_5_*` (4 tests), ADR 0014. Already `[x]`. | — |
| 6 | `docs/rules-coverage.md` exists + CR-citation convention documented | `met` | File exists, 43 rows; convention in `docs/coding-standards.md`. Already `[x]`. | — |
| 7 | ADRs accepted: e2e strategy, lobby protocol, card identity | `met` | ADR 0011/0012/0013, all `Status: accepted`. Already `[x]`. | — |

**M1 recommendation:** all seven criteria are supportable as `met` once #1's
E2E run is actually observed (in CI or a browser-capable environment) rather
than only read as source. Recommend the closeout gate require that one
concrete check before approving M1.

### M2 — Play to the win

| # | Criterion | Status | Evidence | Gap / action |
|---|---|---|---|---|
| 1 | Turn-based actions real (untap/draw/cleanup, CR 502/504/514) | `met` | Already `[x]`; `apply.rs :: issue_118_combat_marked_damage_is_cleared_at_cleanup_cr_514_2` and others. | — |
| 2 | Combat: attackers/blockers/damage/lethal SBA | `met` | Already `[x]`; `crates/rune-engine/src/combat.rs`, `apply.rs` (issue_117/118 suites). | — |
| 3 | Game over first-class, `GameView.result`, room stop, client result screen | `met` (code); **evidence class incomplete for the client half** | Engine/protocol/server: `apply.rs :: issue_119_*` (7 tests), `room.rs :: issue_119_final_broadcast_carries_the_game_result`, `view.rs :: issue_119_terminal_result_projects_onto_the_view`. Client: `clients/web/src/table/GameOverOverlay.tsx`, wired into `Table.tsx:226`, tested in `GameOverOverlay.test.tsx` (5 cases: Victory/Defeat/Draw/a11y/fallback). Roadmap marks this `[ ]` citing #141 as open — contradicted by the commit above. | Roadmap text is stale, not the code. No further gap found. |
| 4 | Web client renders the stack and a `valid_actions`-driven combat flow | `met` (code); **evidence class incomplete** | `clients/web/src/table/StackPanel.tsx` wired into `Table.tsx:396`, tested (`StackPanel.test.tsx`, 6+ cases); `multiSelect.ts`/`multiSelect.test.ts` for combat-declaration flow; server-side projection `crates/rune-server/src/view.rs :: issue_140_ability_target_requirements_project_and_a_selection_resolves`. Roadmap marks this `[ ]` citing #142/#143/#140 as open — contradicted. | Same as above: doc staleness, not a code gap. No further gap found. |
| 5 | E2E scripted full game, two automated clients, lobby → victory | `met` (code); **evidence class incomplete** | `clients/web/e2e/scripted-full-game.spec.ts` (`real-server scripted full game` / `two browsers play a full game to a rendered victory screen (LifeZero)`), `scripted-game.ts`. Roadmap marks this `[ ]` citing #145 as open — contradicted. | Same E2E-execution gap as M1 criterion 1: spec exists and reads correctly but was not run in this session. |

**M2 recommendation:** functionally complete in code; the only real blocker to
`met` across the board is the same one as M1 — an actual, observed E2E run
(CI-green or locally run with a provisioned browser) of the real-server and
scripted-full-game tiers at this SHA.

### M3 — A real card pool

| # | Criterion | Status | Evidence | Gap / action |
|---|---|---|---|---|
| 1 | ADR 0013 implemented: oracle/printing split, reprint invariant | `met` (mechanism); **content criterion not met — see #12 below** | `crates/rune-engine/data/oracle.json`, `data/sets/FIX.json`, `data/sets/FIX2.json`; `dd33bff feat(engine): oracle/printing split with PrintingDatabase (ADR 0013)`. | The mechanism is real; whether a CI test proves "adding a reprint changes zero rules logic" specifically was not independently located by name in this pass — recommend the second review confirm that specific test exists, not just the printing model. |
| 2 | Every card type casts with correct timing | `met` | `actions.rs :: issue_147_*` (sorcery-speed gate, instant off-turn, unpayable-spell tests); commit `ac76fbe`. | — |
| 3 | Spells target at cast, counterspell works end to end | `met` | `apply.rs :: issue_148_counterspell_counters_a_creature_spell_end_to_end_cr_701_5`, `issue_148_counterspell_fizzles_when_its_target_resolves_first_cr_608_2b`; commit `ef63e62`. | — |
| 4 | Effect IR: damage/destroy/life/counters, until-EOT pump, counterspell | `met` | `apply.rs :: issue_149_*` (destroy/damage/life/counters), `issue_150_*` (pump + cleanup expiry); commits `0a51cd9`, `5aedb05`. | — |
| 5 | Dies triggers beyond ETB, one seam | `met` | `triggers.rs :: issue_151_collect_triggers_detects_a_death_by_battlefield_to_graveyard_diff`, `apply.rs :: issue_151_dies_trigger_fires_from_*` (3 paths); commit `0b114a1`. | — |
| 6 | Auras: enchant targeting, attachment, static effect, aura SBAs | `met` | `docs/rules-coverage.md` CR 303.4 row; `resolve.rs :: issue_152_aura_resolves_attached_to_its_target_and_boosts_it_cr_303_4d`, `sba.rs :: cr_704_5m_*`/`cr_704_5n_*`; commit `1109adc`. | — |
| 7 | Core keywords: flying/reach/vigilance/haste; first strike/trample/deathtouch/lifelink | `met` | `docs/rules-coverage.md` CR 702.x rows (13 citations found); commits `48cf411`, `d87db4a`. | — |
| 8 | Replacement pipeline: enters tapped / with counters, no longer a no-op | `met` | `docs/rules-coverage.md` CR 614.1c/614.12 rows; `resolve.rs :: issue_155_zero_zero_entering_with_two_counters_lives_cr_614_12`; commit `69ca5a8`. | — |
| 9 | Prompt types `option`/`select_from_zone`/`order` wire-to-UI | `met` | `docs/protocol.md` §"Prompt slots"; `crates/rune-protocol` (`c1876bc`); client UX (`e4700d8`). | — |
| 10 | Deck validation in pre-game gate against format registry | `met` | `25bf3c6 feat(server): format registry + deck validation in pre-game gate (ADR 0013 §4)`. Specific test names not individually cited in this pass — recommend the second review name them. | Minor evidence-depth gap, not a functional one. |
| 11 | `rune-cli --agent` plays a legal, seed-deterministic game to a win, rule-based | `met` | `4c8e40b feat(cli): rule-based agent that plays a legal game to a win`. | — |
| 12 | Starter set ships: ~100–150 invented oracle cards, ≥4 preconstructed decks, two decks play a complete game | **`unmet`** | `crates/rune-engine/data/oracle.json` has **32** cards (list length checked directly); `data/sets/FIX.json` has 32, `data/sets/FIX2.json` has 1. No preconstructed-deck files found anywhere in the tree (`find . -iname '*deck*.json'` — zero results). | This is a real, unimplemented content gap, not a documentation staleness artifact — the oracle/printing **mechanism** (#146) is done, but the **content** it was built to carry is not. This is the concrete candidate for a Phase 4 gap issue once M3 closeout is decided. |
| 13 | `docs/rules-coverage.md` covers every CR section named above | `met` | Verified directly: every CR citation in this criterion list (601.2c, 608.2b, 603.6c/700.4, 303.4/704.5m/n, 702.7/9/10/15/17/19/20, 614.1c/614.12) has a matching row. | — |

**M3 recommendation:** 12 of 13 criteria are supportable as `met`; criterion
12 (the starter set content) is genuinely `unmet` — the card pool is still
the original 32-card/two-set fixture data, not a 100–150-card starter set, and
no preconstructed decks exist. **Per issue #188's own Phase 4 rule, M3 should
not be treated as closeable-complete until this is resolved, and M4 should
stay coarse** (no detailed decomposition) until a follow-up gap issue for the
starter-set content lands and the cycle reruns closeout.

## What this document does not do, and why

Per ADR 0017 and issue #188's explicit gating, none of the following happened
in this session, and none should be inferred from the `met` verdicts above:

- **No independent second-pass review.** ADR 0017 requires the Audit Reviewer
  to run in a fresh context, "preferably through a different provider" — a
  single session cannot honestly produce its own independent check.
- **No human closeout gate.** No milestone is closed by this document; the
  `met`/`partial`/`unmet` calls above are proposals for that gate, exactly as
  ADR 0017 defines the Auditor's output.
- **No `docs/roadmap.md` edit.** The checkbox/table staleness identified above
  is deliberately left as a **finding**, not a self-applied fix — reconciling
  it is Phase 5's roadmap PR, gated on Phase 3's approval.
- **No gap issues created, no GitHub milestones closed, no M4 decomposition.**
  M3 is not approved complete (criterion 12), so per #188's own Phase 4 rule
  this session stops here rather than expanding M4.
- **No live GitHub read/write.** Issue/PR/milestone state above is inferred
  from this checkout's git history and docs only, not the GitHub API — flagged
  wherever it matters (the "evidence class incomplete" E2E rows, and every
  "roadmap says open, commit says shipped" row).

## Suggested next steps (for a maintainer or the next cycle stage, not self-executed)

1. Run the E2E suite (or confirm the `E2E` CI job is green) for
   `real-server-smoke.spec.ts` and `scripted-full-game.spec.ts` at
   `e934917c735259362402ed9d7c97e652d96ae6cb` — this is the one piece of
   evidence this session could not produce and that blocks a fully-`met` M1/M2.
2. Commission an independent Audit Reviewer pass (fresh context, ideally a
   different provider) against this document and the same evidence.
3. Bring both to a maintainer for the closeout gate: M1 and M2 look
   closeable pending #1 above; M3 should stay open pending the starter-set
   content gap (criterion 12).
4. Reconcile `docs/roadmap.md`'s checkboxes and open-queue prose against
   whatever the closeout gate actually approves — do not carry this
   document's `met` calls into the roadmap verbatim without that approval.
5. File a gap issue for M3 criterion 12 (starter-set content: ~100–150 cards,
   ≥4 preconstructed decks) once M3's closeout status is decided.
6. Feed this pilot run's friction points back into #187: notably, that a
   single-session sandboxed agent-task cannot reach live GitHub state at all,
   which the real Evidence Collector (ADR 0017, slice 1) will need to solve
   for directly rather than assume away.
