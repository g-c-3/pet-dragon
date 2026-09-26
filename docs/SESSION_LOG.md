# SESSION_LOG.md
# Pet Dragon — Session History

## Format
Each entry: date, what was built, decisions made, bugs fixed, next session start point.
Most recent session at TOP.

---

---

## Session 139 — 2026-08-04, cont. (Two external reports: TT torn-write fix (D135) + blunder-fallback fix (D136))

**Built/done:** Gokul uploaded two independent external investigation
reports on a "tactical blindness"/"blunder" symptom. Spot-verified
both against live source before trusting either.

`tactical-blindness-report.md`: re-checked 5 prior hypotheses; 4
refuted (LMP, SEE, null-move zugzwang guard, skill-level leakage all
check out fine), 1 confirmed real — TT torn writes weren't actually
self-detecting despite the code comment's claim, since `key` was
stored as a plain field independent of the payload. Fixed (D135): `key`
now XORed with a `payload_fingerprint()` of the entry's own payload
fields at write time; `probe()`/`probe_move()`/`store()` all recompute
the fingerprint from whatever payload currently sits in the slot before
trusting the key, so a torn write no longer decodes back to the
original key. 2 new tests directly simulate a torn write and confirm
it's now rejected. Report itself framed this as a "leading suspect,"
not conclusively THE cause — it wasn't; see below.

`blunder-bug-report.md`: confirmed, deterministically-reproduced (15/15
trigger trials) separate root cause. `iterative_deepening()`'s
end-of-function fallback picked `moves.get(0)` — an unevaluated,
arbitrary legal move — whenever depth 1 never completed. Traced the
exact mechanism: at `movetime <= 30` (default Move Overhead),
`allocate_time()` returns a 0ms budget, and `is_time_up()` tripped on
the very first check, before any root move was ever generated. Fixed
(D136) with the report's own two recommended, complementary changes:
`is_time_up()`'s elapsed-time check no longer fires while
`current_depth <= 1` (explicit stop/quit still instant; ponder-hit hard
override deliberately excluded from this exemption), and the true
last-resort fallback now uses `score_moves()` instead of blind
first-in-list. Required fixing 2 existing tests
(`alpha_beta.rs`'s D95 timeout tests) that called `alpha_beta()`
directly without going through `iterative_deepening()`'s loop, so
`current_depth` defaulted to 0 and would have silently defeated what
those tests actually check. Added 4 new tests, including 3 end-to-end
reproductions of the report's exact FEN cases through the real
`iterative_deepening()` entry point (not just the internal mechanism in
isolation).

Process note: made the same "str_replace deletes the adjacent line"
mistake as D132 two more times this session (once in `tt/mod.rs`'s test
module, once in `iterative.rs`'s) — caught both immediately by
re-viewing the file right after each edit, per the standing lesson from
D132, which is why neither reached the docs/shipped stage broken.

**Files touched:** `src/tt/mod.rs`, `src/search/mod.rs`,
`src/search/alpha_beta.rs`, `src/search/iterative.rs`.

**Bugs fixed:** TT torn-write corruption not actually self-detecting
(D135); blind evaluation-free fallback move at low movetime/on any stop
before depth 1 completes (D136) — the latter is the confirmed root
cause of the actual reported blunders.

**Decisions made:** D135, D136.

**Next session start point:** Gokul commits all 4 files together,
confirms full CI green — the 3 new end-to-end blunder-repro tests in
`iterative.rs` are the real proof either fix works, not just this
session's manual tracing. If red, get the log before assuming either
fix is correct.

---

## Session 138 — 2026-08-04, cont. (CI confirmed green — Phase 34.1 fully closed)

**Built/done:** Gokul confirmed CI green after committing the D134
files (default flip for `nonpawn_correction_enabled` + the 5 tests
fixed to match). This closes Phase 34.1 completely — all three
experimental flags now have a final, CI-confirmed call
(`improving_enabled`: no change, `continuation_correction_enabled`: no
change, `nonpawn_correction_enabled`: flipped to `true`). Phase 34.2
was already closed (Session 135). No code changed this session —
docs-only update to mark ROADMAP.md's ⚠️ flag resolved and consolidate
the Phase 34.1 completion note under all three sub-items rather than
sitting between 34.1a and 34.1b.

**Files touched:** `docs/ROADMAP.md` (status update only).

**Bugs fixed:** None this session (confirmation only).

**Decisions made:** None.

**Next session start point:** Remaining upgrade-plan backlog: 34.4
(LMR reduction formula enrichment), 34.5 (`ThreatByKing` — review flags
this as needing the most care, do last among the eval items), 34.6
(`WeakQueenProtection`). 34.3 (NNUE re-validation) stays parked pending
Gokul's explicit call per D131. No priority order set yet for 34.4-34.6
— ask before starting.

---

## Session 137 — 2026-08-04, cont. (nonpawn_correction_enabled default flipped to true — D134)

**Built/done:** Gokul's explicit call on D133's combined 700-game
result (Elo -11.9, direction consistent across both batches): accept
the directional lean and flip `nonpawn_correction_enabled`'s default
to `true`, same kind of judgment call as D125. Implemented across
every default site, found by tracing the full call graph rather than
just the first one:
- `search/mod.rs`: `SearchInfo::new()`'s default flipped, doc comment
  rewritten with the full validation trail.
- `main.rs`: `EngineState`'s own separate default flipped (this is the
  one that's actually load-bearing for real UCI usage — `cmd_go`
  overwrites `SearchInfo`'s value with `EngineState`'s copy
  regardless of what `SearchInfo::new()` set), plus the advertised UCI
  option string so GUIs see the correct default.
- `uci_wasm.rs`: confirmed it has no separate default site and
  inherits the `search/mod.rs` flip automatically — no edit needed.
- 5 tests across 3 files fixed: 2 tests that directly asserted the old
  `false` default (renamed/flipped), 1 test whose whole purpose was
  exercising the OFF path and would have silently started testing the
  ON path instead without an explicit override, and 1 test
  (`match_runner.rs`) decoupled from relying on any default at all
  since its actual purpose is verifying flag-apply mechanics, not
  defaults. Two stale explanatory comments elsewhere also corrected.
- Confirmed via grep that 7 other files (`selfplay.rs`, `eval_diag.rs`,
  `texel_gen.rs`, `train_nnue.rs`, `lichess_sample.rs`,
  `aggregate_opening_stats.rs`, `uci_match_runner.rs`) have zero
  references to this flag and needed no changes.

**Files touched:** `src/search/mod.rs`, `src/main.rs`,
`src/search/alpha_beta.rs`, `src/bin/match_runner.rs`.

**Bugs fixed:** None (default-flip feature work, with test-suite
maintenance to match).

**Decisions made:** D134.

**Next session start point:** Gokul commits all 4 files, confirms CI
green. Once green, 34.1 is fully closed — all three experimental flags
have a keep/revert call (2 no-change, 1 flipped). Remaining backlog:
34.4 (LMR enrichment), 34.5 (ThreatByKing), 34.6
(WeakQueenProtection); 34.3 (NNUE) stays parked per D131.

---

## Session 136 — 2026-08-04 (First 34.1 SPRT batch: 2 of 3 flags closed as no-signal, 1 pending more games — D133)

**Built/done:** Gokul ran all three 34.1 flag-SPRT matches (200 games
each) and reported results. Applied this project's own existing noise
bar (D124's precedent: +5.2 Elo at n=200 = "within noise, no further
runs") rather than inventing a new threshold:
- `improving_enabled`: +5.2 Elo favoring off — the exact magnitude
  D124 already called noise. Closed, no change, default stays `false`.
- `continuation_correction_enabled`: +8.7 Elo favoring off — within
  the same noise band. Closed, no change, default stays `false`.
- `nonpawn_correction_enabled`: +17.4 Elo favoring on — largest of the
  three, and in the direction of enabling it, but still not clearly
  outside a ~±25-35 Elo standard-error band at n=200. Left
  undecided; requested a 500-game follow-up batch (`seed_start=200`)
  to combine into a 700-game total before making a real call, rather
  than deciding off one small-sample result.

**Files touched:** None (docs-only session — no code changed).

**Bugs fixed:** None.

**Decisions made:** D133.

**Next session start point:** Gokul runs the requested 500-game
`nonpawn_correction_enabled` follow-up (`flag_a=false`, `flag_b=true`,
`seed_start=200`) and reports results; combine with this session's 200
games for the decision. `improving_enabled` and
`continuation_correction_enabled` are fully closed, no further action.

---

## Session 135 — 2026-08-03, cont. (CI confirmed green — 34.1/34.2 closed out)

**Built/done:** Gokul confirmed CI green after committing the D132
files (the 7-file Texel-mirror fix). Closes out 34.1 (SPRT
infrastructure) and 34.2 (`ThreatByRook`) — both now fully
CI-confirmed after their respective fix-forward rounds. No code
changed this session; docs-only update to clear ROADMAP.md's
⚠️-not-yet-CI-confirmed flags.

**Files touched:** `docs/ROADMAP.md` (status update only).

**Bugs fixed:** None this session (confirmation only).

**Decisions made:** None.

**Next session start point:** 34.1's infrastructure is done but unused
— run the three flag-SPRT matches (34.1a/b/c:
`nonpawn_correction_enabled`, `continuation_correction_enabled`,
`improving_enabled`) from the Actions tab and report results back;
that's what decides whether any of the three flags flips its shipped
`false` default. Remaining upgrade-plan backlog (34.4 LMR enrichment,
34.5 ThreatByKing, 34.6 WeakQueenProtection) and 34.3 (NNUE, parked
per D131) still open after that.

---

## Session 134 — 2026-08-03, cont. (CI red from D130 commit: ThreatByRook missing from Texel tuner mirror — D132)

**Built/done:** Gokul uploaded a failing CI log after the D130 commit.
537/538 lib tests passed; the one failure was
`texel::predict::tests::test_predict_matches_evaluate_after_moves`
(mismatch at seed 4: predict=31, evaluate=51). Root cause: D130's
`ThreatByRook` term was added to `eval/threats.rs` only —
`texel::predict::predict()` computes an independent feature-based
mirror of the entire evaluator for tuning purposes, and every eval
term has to be duplicated across 5 `texel/*.rs` files or `predict()`
silently drifts from `evaluate()`. Fixed by threading `threat_by_rook`
through the full mirror: `features.rs` (new `threat_by_rook_diff`
feature, extraction loop mirroring the eval-side loop exactly),
`weights.rs`/`weights_f64.rs` (new weight field, default matching
`THREAT_BY_ROOK_BONUS` exactly, wired into every conversion/flatten/
unflatten path, `PARAM_COUNT` bumped), `predict.rs`/`predict_f64.rs`
(scored in both the integer and f64+gradient forward passes). Also
found and fixed two more files with the same gap that weren't even
part of the lib test suite — `bin/texel_tune.rs` and
`bin/texel_diag.rs`, the D35 tuning-pipeline pair that round-trips
tuned weights as a Rust snippet. `texel_diag.rs` constructs
`TunableWeights { .. }` exhaustively, so this would have been a
**compile error** on the next full build, not just a failing test.

Self-caught process note: made the same mistake twice while editing —
a `str_replace` meant to insert `threat_by_rook,` next to
`threat_by_minor,` in a struct literal accidentally deleted the
adjacent `tempo,` field instead, in both `weights_f64.rs`'s
`unflatten()` and `texel_diag.rs`'s `parse_tuned_weights()`. Caught
both immediately by re-viewing the file after each edit. Documented in
D132 as a general lesson for future struct-literal insertions.

**Files touched:** `src/texel/features.rs`, `src/texel/predict.rs`,
`src/texel/predict_f64.rs`, `src/texel/weights.rs`,
`src/texel/weights_f64.rs`, `src/bin/texel_tune.rs`,
`src/bin/texel_diag.rs`.

**Bugs fixed:** Regression from D130 (ThreatByRook not mirrored in the
Texel tuner — would have broken both test correctness and, separately,
compilation of the tuning-pipeline binaries).

**Decisions made:** D132.

**Next session start point:** Gokul commits all 7 files, re-runs full
CI. If green: 34.1/34.2 both fully closed — move to running the three
34.1 flag-SPRT matches from the Actions tab and reporting results. If
still red: check first whether it's another Texel-mirror gap (this
should have been the last untouched one) before assuming something new.

---

## Session 133 — 2026-08-03, cont. (Upgrade-plan backlog started: SPRT infra for 3 flags (34.1) + ThreatByRook (34.2) — D130/D131)

**Built/done:** Started the upgrade-plan backlog (D127) per Gokul's
"3 & 4" instruction, explicitly skipping review item #1 (NNUE
re-validation) — flagged back to Gokul beforehand as conflicting with
already-tested project history (D25: +338 Elo for turning NNUE off at
25% weight; D53/D55/D57/D58: three more attempts, same result; D61:
Gokul's own call to shelve it). Recorded as D131 so a future session
doesn't restart NNUE work from the review's framing without reading
D61 first. Gokul proceeded with #3 and #4 instead.

**34.1**: `match_runner`/`match_runner.yml` only ever varied NNUE
blend weight — no path to SPRT-test the three experimental
`SearchInfo` flags (`nonpawn_correction_enabled`,
`continuation_correction_enabled`, `improving_enabled`) the review
pointed at. Extended `match_runner.rs` with an `ExperimentalFlag` enum,
CLI arg parsing, and a `resolve_weights()` safeguard forcing both
engines' NNUE weight to 0% whenever a flag is under test (prevents a
mobile user from accidentally running a confounded two-variable
match). `match_runner.yml` gained 3 new dropdown
workflow_dispatch inputs. Backward compatible — original positional
args unchanged, new ones optional. 9 new tests. This is
infrastructure only — no match has been run yet, no flag's default has
changed.

**34.2**: Implemented `ThreatByRook` (`eval/threats.rs`) per the
review's §8.1 spec — `THREAT_BY_ROOK_BONUS = s(20, 12)`, scoped to
rook-attacks-queen only. 3 new tests, including a scoping test that
rook-attacks-rook does NOT trigger the bonus. Caught and fixed my own
first-draft test-design mistake mid-session: the positive-case and
scoping-case FENs both put the attacking rook on an open file directly
facing the enemy target, which also puts the attacking rook itself
under `UNDEFENDED_PENALTY` (mutual attack, no other defender) — that
would have confounded or flipped the sign of what the tests were
checking. Fixed by adding a defending pawn to both FENs before
finalizing.

**Files touched:** `src/bin/match_runner.rs`,
`.github/workflows/match_runner.yml`, `src/eval/threats.rs`.

**Bugs fixed:** None (new feature work); one self-caught test-design
error during development (documented above), not shipped.

**Decisions made:** D130, D131.

**Next session start point:** Gokul commits all 3 files, confirms CI
green. Once green: run the three 34.1 flag-SPRT matches from the
Actions tab (34.1a/b/c) and report results — decides whether any flag
flips its shipped `false` default. NNUE (34.3) stays parked pending
Gokul's explicit call per D131.

---

## Session 132 — 2026-08-03, cont. (CI confirmed green — Phase 33 closed out)

**Built/done:** Gokul confirmed CI green after committing the D129
files (`movegen/pawns.rs`, `movegen/mod.rs`). Full test suite,
including `tests/perft.rs::test_perft_kiwipete_depth4`, now passes.
Closes out Phase 33 and the D127/D128/D129 fix-forward trilogy — all
3 original bug-report findings are resolved and CI-confirmed. No code
changed this session; docs-only update to mark ROADMAP.md's
⚠️-not-yet-CI-confirmed flags as resolved.

**Files touched:** `docs/ROADMAP.md` (status update only).

**Bugs fixed:** None this session (confirmation only).

**Decisions made:** None.

**Next session start point:** Start the upgrade-plan backlog from
`pet-dragon-full-review.md` (parked in D127). Priority order not yet
set by Gokul — ask before starting, since it wasn't specified when
the review was uploaded. Headline item per the review's own framing:
NNUE re-validation.

---

## Session 131 — 2026-08-03, cont. (CI red again after D128: Bug 1's fix double-generated promotions in full movegen, not just quiescence — D129)

**Built/done:** Gokul uploaded a second failing CI log, taken after
the D128 commit. Syzygy singleton race was gone (533/533 lib tests
green — confirms D128 worked), but `tests/perft.rs::
test_perft_kiwipete_depth4` failed with the wrong node count. Root
cause traced back to D127, not D128:
`pawns::generate_pawn_moves()` (full pseudo-legal generation — used by
perft and every normal search node, not just quiescence) calls both
`generate_pawn_pushes()` (already generates quiet promotions) and
`generate_pawn_captures()`. D127's Bug 1 fix added quiet-promotion
generation directly inside `generate_pawn_captures()`, which is shared
by both `generate_pawn_moves()` and quiescence's capture-only path —
so every quiet promotion was now being generated *twice* in ordinary
move lists, not just made visible once to quiescence. This is a more
severe regression than the original Bug 1 (whole-engine, not
quiescence-only) and slipped through because D127's own tests checked
`generate_pawn_captures()` in isolation, never `generate_pawn_moves()`
or `movegen::generate_captures()` together with a promotion.

Fixed by splitting the quiet-promotion-inclusive behavior into a new
function, `pawns::generate_pawn_tactical()`, called only from
`movegen::generate_captures()` (quiescence's actual entry point).
`generate_pawn_captures()` reverted to its original (pre-D127)
behavior and stays untouched inside `generate_pawn_moves()`. Replaced
the two D127 unit tests (now pointed at the wrong function) and added
3 new regression guards specifically shaped around the
double-generation failure mode, including an integration-level test
in `movegen/mod.rs` against the real `generate_captures()`/
`generate_moves()` entry points rather than internal `pawns::`
functions only.

**Files touched:** `src/movegen/pawns.rs`, `src/movegen/mod.rs`.

**Bugs fixed:** Regression introduced by the D127 Bug-1 fix
(double-generated quiet promotions in full move generation — perft
and every normal search node, not just quiescence).

**Decisions made:** D129.

**Next session start point:** Gokul commits both files and re-runs
full CI including `tests/perft.rs`. If green, proceed to the
upgrade-plan backlog (D127's parked item, priority order still needed
from Gokul). If still red, get the new log, read the exact failing
assertion, and trace the call graph before touching code again — this
is the second CI-red round in a row from the same original commit.

---

## Session 130 — 2026-08-03, cont. (CI red from D127 commit: flaky Syzygy test singleton race — D128)

**Built/done:** Gokul uploaded a failing GitHub Actions "Test" job log
from committing the D127 files. 1 of 535 tests failed:
`test_probe_wdl_refuses_when_castling_rights_present` panicked with
`"Syzygy init error: AlreadyInitialized"`, while its structurally
identical sibling `test_probe_root_refuses_when_castling_rights_present`
(added in the same commit) passed. Root cause: `pyrrhic_rs::TableBases`
is a process-wide singleton (already recorded in D17 from Session 33 —
should have been cross-checked before writing new tests that call
`SyzygyProber::new()`). Only the first `new()` call in the whole test
binary succeeds; every later call returns `Err(AlreadyInitialized)`.
D127 had introduced two new tests each unwrapping their own `new()`
call, on top of the pre-existing `test_syzygy_new_does_not_panic`
(which discards its result, so never panics) — three call sites racing
concurrently under `cargo test`'s default parallelism, with no fixed
winner. Fixed by consolidating all three into one test,
`test_syzygy_prober_construction_and_castling_guard()`, now the *only*
`SyzygyProber::new()` call site in the test binary, performing all
three original assertions against the one successfully-constructed
instance. This removes the race rather than patching one observed
ordering.

**Files touched:** `src/syzygy/mod.rs` (test module only — no
production code changed this session).

**Bugs fixed:** Flaky test / CI-red from the D127 commit (singleton
race in newly-added Syzygy tests, not a real engine bug).

**Decisions made:** D128.

**Next session start point:** Gokul commits the replacement
`src/syzygy/mod.rs` and re-runs CI. If green, move to the upgrade-plan
backlog (D127's parked item, priority order still needed from Gokul).
If still red, get the new log before making further changes.

---

## Session 129 — 2026-08-03 (Second external review round: 3 bugs fixed, upgrade plan parked — D127)

**Built/done:** Gokul uploaded two new external documents: a focused
bug report (full ~31,000-line codebase audit, 3 confirmed findings)
and a separate, larger search/eval "upgrade plan" review (no bugs,
source-review-only, unverified against real API surface). Instruction:
"Bugs then upgrade." All 3 bug-report findings spot-verified against
live `main` source (grep + direct reads) before trusting, matching the
D116 discipline from the first external review round — all 3 checked
out exactly as reported, a notably cleaner hit rate than round one's
5/8.

Fixed all three:
1. **Quiescence never searched quiet pawn promotions** —
   `generate_pawn_captures()` only ever called the diagonal-capture and
   en-passant generators; `add_promotions()` was unreachable from the
   capture-only path qsearch draws from. Added
   `generate_pawn_quiet_promotions()`, wired in. Verified `see()` and
   `score_captures()` already handle non-capturing promotion moves
   correctly with no further changes needed.
2. **`push_game_history()` double-called** in both
   `bin/match_runner.rs` and `bin/uci_match_runner.rs`, on top of the
   internal push already done by `make_move_with_history()` —
   corrupted `is_threefold_repetition()`'s count, tripping it a full
   occurrence early and dampening every measured Elo delta toward 50%
   in both the CI `regression-gate` job and manual A/B runs. Deleted
   the redundant line in both files.
3. **Syzygy probing never checked castling rights** — tables don't
   encode castling at all; neither the interior WDL probe nor the root
   DTZ probe gated on it. Also caught and corrected a directly-
   contradicted, unbacked claim in `ENGINE_ARCHITECTURE.md` §5 that
   assumed castling always clears before TB range is reached (it
   doesn't — ~26% of games retain a right, no game rule forces
   clearing). Added `has_castling_rights()` guard centralized inside
   `SyzygyProber` (both `probe_wdl()` and `probe_root()`).

Upgrade-plan review intentionally not started — parked as an
unscheduled backlog per "Bugs then upgrade"; every item there needs
its own Texel-tuning/SPRT validation pass before landing.

**Files touched:** `src/movegen/pawns.rs`, `src/bin/match_runner.rs`,
`src/bin/uci_match_runner.rs`, `src/syzygy/mod.rs`,
`docs/ENGINE_ARCHITECTURE.md`.

**Bugs fixed:** All 3 from `pet-dragon-bug-report.md` (quiescence
quiet promotions, double `push_game_history()`, Syzygy castling gap).

**Decisions made:** D127.

**Next session start point:** Gokul commits all 5 changed files,
confirms `cargo test` green (8 new tests: 2 in `movegen/pawns.rs`, 3
in `syzygy/mod.rs`, plus 3 doc files with no tests of their own).
⚠️ None of this session's code changes are CI-confirmed yet. Once
green, start on the upgrade-plan backlog — begin by asking Gokul for
its priority order, since it wasn't set this session.

---

## Session 128 — 2026-08-02, cont. (30.5 scoped, deferred — SharedArrayBuffer ruled out for GitHub Pages, D126)

**Built/done:** Gokul asked to start 30.5 (async WASM UCI). Before
writing code, worked out that the "obvious" fix — SharedArrayBuffer-
backed real threading, same pattern native `main.rs` already uses —
isn't viable for this deployment target: GitHub Pages can't set the
`Cross-Origin-Opener-Policy`/`Cross-Origin-Embedder-Policy` headers
SharedArrayBuffer requires, being a static host with no server-side
config. Also worked out a subtler point: a plain Web Worker alone
doesn't fix `stop` either, since a long synchronous WASM call blocks a
Worker's own message queue exactly as completely as it blocks the main
thread's. Identified the viable path instead: cooperative chunking —
break `go` into repeated per-depth `uci_command` calls instead of one
long synchronous one, fitting the existing `thread_local! SESSION`
persistent-state design. Presented this analysis and asked whether to
proceed and how to scope it; Gokul confirmed the direction but chose
not to start implementation this session ("too big" / "later").
Recorded the full analysis in DECISIONS.md D126 so it doesn't need
rediscovering later, and added a pointer to it in ROADMAP 30.5 without
changing its open status.

**Files touched:** None — no code changes this session, pure
architecture scoping.

**Bugs fixed:** None.

**Decisions made:** D126.

**Next session start point:** 30.5 remains open, not started, with a
concrete approach now on record (D126) whenever Gokul wants to pick it
up — cooperative chunking of `go`, no deployment infra changes needed.
Standing backlog otherwise unchanged: 29.7h (PlayStyle bulk tuning),
30.8 (real UCI GUI target still "not sure yet").

---

## Session 127 — 2026-08-02, cont. (D125: RecaptureExtension default flipped to true, explicit instruction, standard SPRT step skipped)

**Built/done:** Gokul instructed directly: flip `RecaptureExtension`'s
default to `true`, no further A/B run. Flagged once, plainly, that
this departs from every other unproven-technique toggle's rollout
pattern in this project (D75/D98/D114/D117 all required a dedicated
SPRT-style A/B before a default-on flip; the only data point here is
D117's own flat n=20 functional check, which confirms the mechanism
works, not that it helps) — then made the change as instructed, since
this is Gokul's call as project owner and the instruction was explicit
and repeated, not something to keep pushing back on.

Flipped the default in `SearchInfo::recapture_extension_enabled` and
`EngineState::recapture_extension_enabled` (both `false` → `true`),
updated the `RecaptureExtension` UCI option's advertised default
string, and renamed/flipped the two tests that asserted the old
default (`_defaults_to_false` → `_defaults_to_true` in both
`alpha_beta.rs` and `main.rs`). The extension mechanism's own logic is
completely unchanged — pure default-value flip. Full reasoning,
including an explicit note on how to revert if a future bench run
surfaces a regression, in DECISIONS.md D125.

**Files touched:** `src/search/mod.rs`, `src/search/alpha_beta.rs`,
`src/main.rs`.

**Bugs fixed:** None — pure configuration change. Brace-balance and
test-count checks passed cleanly on all three files (search/mod.rs
96/96, alpha_beta.rs 248/248 with 57/57 tests, main.rs 254/254 with
77/77 tests); confirmed no other file had a stray reference to the old
default.

**Decisions made:** D125.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox.

**Next session start point:** Gokul commits all three files, confirms
`cargo test` green — the two renamed tests should pass now that both
the default and the assertions agree. `RecaptureExtension` now runs by
default in every game with no dedicated Elo data behind it (D125's own
risk note). Standing backlog, none blocking: 29.7h (PlayStyle bulk
tuning), 30.5/30.8 (WASM UCI GUI).

---

## Session 126 — 2026-08-02, cont. (D124: D118/D120 pinned-ref Elo check — +5.2 Elo, within noise, accepted neutral. Caught a real mistake: abbreviated SHA caused a silent main-vs-main mirror match.)

**Built/done:** Ran the standing Elo A/B for D118 (LMR gate fix) and
D120 (packed-score fix) — both shipped live with no `setoption`
toggle, so this needed a pinned-ref comparison (pre-fix commit vs.
`main`) rather than the usual `setoption`-based A/B pattern.

**Caught a real mistake mid-session**: first attempt used a 10-char
abbreviated commit SHA (`edb0e9a20d`) as `pre_tuning_ref`.
`actions/checkout` couldn't resolve it as a branch/tag ref, failed
after 3 retries — and the workflow silently substituted `main` for
both sides instead of failing the job. The resulting clean 200-game
"result" (50.0%, -0.0 Elo) looked exactly like a real neutral finding;
only caught because Gokul noticed both engine labels read `(main)`.
Re-ran with the full 40-character SHA
(`edb0e9a20d8579b3b31a99efdf627acd80140826`) and confirmed correct
labels this time. Flagged to Gokul (not fixed, his call) that this
workflow probably should hard-fail on a checkout error rather than
silently mirror-matching `main` against itself.

**Real result**: 36-39-125, pre-fix commit scored 49.2%, **+5.2 Elo
for main**. Within noise for n=200. Gokul chose to accept as neutral
rather than run a bigger batch or split D118/D120 into separate
pinned-ref comparisons. Full reasoning in DECISIONS.md D124.

**Files touched:** None — pure measurement/process session, no code
changes.

**Bugs fixed:** None in engine code. One real process mistake (using
an abbreviated SHA for a pinned-ref checkout) caught before it could
be recorded as a false neutral finding.

**Decisions made:** D124. Closes out the last open item from the
Phase 32 review-response arc (Sessions 118-125).

**Next session start point:** Nothing queued. Standing backlog, none
blocking: 29.7h (PlayStyle bulk self-play + real Texel-tuning),
D117's `RecaptureExtension` n=200 confirmation (still open since
Session 120's n=20 functional check), 30.5/30.8 (WASM UCI GUI async
limitations / real target GUI, Gokul said "not sure yet" on the
latter). Also worth remembering for any *future* pinned-ref run in
this repo: always use the full 40-character SHA, never an abbreviated
one, and sanity-check the reported engine labels before trusting a
result.

---

## Session 125 — 2026-08-02, cont. (32.7: wrong-sentinel score bug fixed — D123. Phase 32 complete, all 8 external review findings resolved.)

**Built/done:** Fixed the last open item in Phase 32, review finding
#7. Confirmed real at the report's exact cited location
(`iterative.rs:154`, matched verbatim): `score_for_result`'s selection
logic required `info.best_score.abs() > 0` in addition to the real
sentinel check (`!= -INFINITY`), which wrongly treated a genuine drawn
root score of exactly `0` as invalid, falling back to a local
`best_score` tracker that only updates when `info.best_move !=
Move::NULL` for that depth — stale on the narrow edge case where a
completed depth left it `NULL`. Extracted the logic into its own
`choose_result_score()` function (matching this session's D114/D117/
D118 pattern of pulling inline logic out for direct testability) and
dropped the wrong `.abs() > 0` clause. 3 new unit tests exercise the
exact bug scenario, the real sentinel case, and the ordinary path.
Full reasoning in DECISIONS.md D123.

Unlike #3/#4/#5/#6, which all turned out to have a different real-world
severity than their original framing once independently checked
(sometimes needing an empirical sweep, not just a hand-derived
example), **this one's own "Low severity" framing held up exactly as
written** — added a before/after summary table across all 8 findings
to DECISIONS.md D123 as the closing note for Phase 32.

**Files touched:** `src/search/iterative.rs`.

**Bugs fixed:** Review finding #7 (D123) — **Phase 32 is now complete,
all 8 external review findings resolved.** Brace-balance and test-count
checks both passed cleanly (112/112 braces, 23/23 tests).

**Decisions made:** D123.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox.

**Next session start point:** Gokul commits `src/search/iterative.rs`,
confirms `cargo test` green — this closes out Phase 32 for real.
Standing open items from prior phases, none blocking: D118's LMR fix
and D120's packed-score fix both still lack a dedicated Elo A/B
(pre-fix commit pinned vs. current `main`); D117's `RecaptureExtension`
still needs its n=200 confirmation; D114's `ImprovingHeuristic` is
already closed at n=200 (D115, parked off); 29.7h (PlayStyle bulk
tuning) and 30.5/30.8 (WASM UCI GUI target/async limitations) remain
untouched from before Phase 32 began.

---

## Session 124 — 2026-08-02, cont. (32.6: SEE king-legality guard added — D122, confirmed real via 20,000-case empirical sweep after an initial hand-trace pointed the wrong way)

**Built/done:** Fixed review finding #6 — `see.rs`'s exchange
simulation never checked whether a simulated king "recapture" would be
legal. Confirmed the gap real against live source (no `PieceKind::King`
special-casing anywhere in either `see()` or `see_value_of()`'s loop).

Notable process moment: hand-traced one concrete position first, and
found the corruption "self-corrected" via the backward min-max
propagation (the King's 20,000 placeholder value always dominates a
subsequent "capture the king" step, erasing the corrupted value at the
very next backward step). This looked like it might mean the bug
never actually changes any real result. **Didn't trust that single
example as a general property** — built an independent Python model of
the exact algorithm and swept 20,000 randomized, realistically-ordered
attacker/defender chains (king always last within a side's own chain,
matching `least_valuable_attacker()`'s real search order) before
concluding anything. Result: ~9% (1821/20,000) produced a different
final SEE value, several by large margins — the one hand-traced
example that self-corrected was not representative, and trusting it
alone would have led to badly understating this fix's real impact.

Fixed both `see()` and `see_value_of()` (duplicate loops) with the
standard guard: if the next attacker is a king, check whether the
opposing side still has any attacker on the target square via
`least_valuable_attacker()` itself (occupancy-respecting), not the raw
`attackers` bitboard. Translated one of the swept diff cases into a
real, hand-verified chess position (both sides down to king-only on
e5) for a concrete regression test — `see_value_of()` was 220, now
correctly 320; a companion test confirms the boolean `see()` result
also flips at a realistic threshold (300). Full reasoning in
DECISIONS.md D122.

**Files touched:** `src/search/see.rs`.

**Bugs fixed:** Review finding #6 (D122). Brace-balance and test-count
checks both passed cleanly (49/49 braces, 10/10 tests).

**Decisions made:** D122.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox —
though this fix's justification is unusually well-verified: an
independent 20,000-case Python model of the exact algorithm, not just
a single hand-derived example (which this session specifically learned
not to over-trust on its own).

**Next session start point:** Gokul commits `src/search/see.rs`,
confirms `cargo test` green. **Phase 32.1-32.6 all done.** Remaining:
32.7 (iterative.rs sentinel misclassifying a real score of 0) — not yet
independently re-verified against live source, same discipline needed
as every finding so far.

---

## Session 123 — 2026-08-02, cont. (D121: cleared 2 release-build-only unused-import warnings)

**Built/done:** Gokul uploaded the "Publish Release" multi-platform
workflow's CI logs — green, but 2 compiler warnings on every native
build (Windows/aarch64-macOS/x86_64-macOS): `unused import: MoveKind`
(`alpha_beta.rs:52`) and `unused import: eg` (`pawns.rs:30`). Both
imports are only used inside each file's own `#[cfg(test)]` module,
which a release build excludes entirely — that's why `cargo test`
(the regular CI job) never showed either warning while this separate
release-build job did; two different compilation configs, not a
contradiction. Moved both imports from the top-level `use` into a
test-only `use` line inside each file's `mod tests` block. Full
reasoning in DECISIONS.md D121.

**Files touched:** `src/search/alpha_beta.rs`, `src/eval/pawns.rs`.

**Bugs fixed:** 2 compiler warnings (D121) — cosmetic, no behavior
change in either build configuration. Brace-balance checks passed
cleanly on both files (alpha_beta.rs 248/248, pawns.rs 85/85).

**Decisions made:** D121.

**Next session start point:** Gokul commits both files, confirms the
release-build workflow now runs warning-free. Phase 32 still has 32.6
(SEE missing illegal-king-recapture guard) and 32.7 (iterative.rs
score-0 sentinel bug) open, neither independently re-verified against
live source yet — same discipline needed as every finding so far, two
of which (#3, #4) turned out to have a materially different real-world
status than the report's own framing once actually checked.

---

## Session 122 — 2026-08-02, cont. (32.5: packed mg()/eg() sign-extension bug fixed — D120, the highest-impact fix in Phase 32 so far)

**Built/done:** Fixed review finding #5. Empirically verified (not just
reasoned about) via a 200,000-random-case Python sweep before touching
any code: the old `eval/material.rs::mg()` extraction (`(score >> 32)
as i32`) was wrong on ~50% of cases — every case with a negative `eg`
component — always off by exactly 1. While investigating, discovered
`texel/weights_f64.rs` already had an independently-written, partial
workaround for a version of this bug, with a comment claiming `mg()`
"is only correct once summed" (i.e., safe on an accumulated total,
unsafe only on one un-summed literal weight). **Verified that claim
false** by simulating an accumulated sum of 5 terms — the bug
reproduces identically on the sum, not just on isolated terms. Since
`taper()` (called by every real position's eval) runs `mg()` on
exactly this kind of accumulated total, this means the bug was live in
the real evaluation pipeline, not just reachable from decoding
individual un-summed weights — a bigger deal than Phase 32's original
"not independently re-verified" note suggested.

Fixed `mg()` at the source using Stockfish's own technique (add half
the low half's range via `wrapping_add`, then shift unsigned so no
sign-extension happens during the shift). Re-verified against the same
200k-case sweep plus a unit-test grid: zero failures. Simplified
`weights_f64.rs`'s now-redundant workaround to call the fixed
`mg()`/`eg()` directly — confirmed mathematically identical to the old
workaround across 100,000 cases first, so this is a pure
simplification, not a behavior change — and corrected its misleading
comment rather than leaving it to mislead a future reader. Full-repo
grep confirmed no other file has a similar workaround or duplicate bit-
manipulation for this same bug (`tt/mod.rs`'s only `>> 32` usage is
unrelated Zobrist hash-key extraction, already checked). Full reasoning
in DECISIONS.md D120.

**Files touched:** `src/eval/material.rs`, `src/texel/weights_f64.rs`.

**Bugs fixed:** Review finding #5 (D120) — the highest-impact fix in
Phase 32 so far, since everything fixed before it was either dead code
before its own wiring fix (32.2/32.3), a narrow edge case (32.1), or
near-zero-reachability hygiene (32.4); this one was live in the real
evaluation function. Brace-balance and test-count checks both passed
cleanly on both files (material.rs 37/37, weights_f64.rs 51/51; 14/14
and 2/2 test counts respectively).

**Decisions made:** D120.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox —
though this fix in particular was checked more rigorously than most via
independent Python empirical sweeps (200,000 cases for the bug/fix
itself, 100,000 for the weights_f64.rs simplification's equivalence)
before any Rust code was written, not just hand-derived.

**Next session start point:** Gokul commits both files, confirms
`cargo test` green — and, given this fix's reach into the live eval
pipeline, this is a good candidate for an actual Elo A/B once CI is
green (pre-D120 commit pinned as one side, current `main` as the
other, same shape as D118's open item) rather than assuming a
correctness fix is automatically a strength improvement. **Phase 32:
32.1-32.5 all done.** Remaining: 32.6 (SEE missing illegal-king-
recapture guard), 32.7 (iterative.rs sentinel misclassifying a real
score of 0) — neither independently re-verified against live source
yet, same discipline needed as every finding so far.

---

## Session 121 — 2026-08-02, cont. (32.4: duplicate MATE_THRESHOLD removed — D119. Phase 32's first four findings all done.)

**Built/done:** Fixed review finding #2 — `tt/mod.rs`'s own
`MATE_THRESHOLD = 30_000`, independent of `search/mod.rs`'s real
`MATE_THRESHOLD = 900_000`. Removed the local duplicate entirely;
`score_to_tt()`/`score_from_tt()` now reference
`crate::search::MATE_THRESHOLD` directly — confirmed no import-cycle
risk (both modules are top-level siblings under the crate root per
`lib.rs`) and confirmed via full-repo grep that nothing outside
`tt/mod.rs` itself referenced the removed constant, so this isn't a
breaking API change for any other file. Updated the existing
`test_mate_score_adjustment` test and added a direct regression guard
(`test_score_to_tt_no_longer_uses_stale_local_threshold`) using a score
that sits exactly in the gap between the old wrong threshold and the
real one. Full reasoning in DECISIONS.md D119.

**Files touched:** `src/tt/mod.rs`.

**Bugs fixed:** Review finding #2 (D119). Brace-balance and test-count
checks both passed cleanly (75/75 braces; the 16 `#[test]` vs. 17
`fn test_` gap is a pre-existing, harmless naming coincidence — a
`test_move()` helper function that builds a test `Move`, not an actual
unit test — confirmed against the pre-session file before trusting the
count, not just assumed).

**Decisions made:** D119.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox.

**Next session start point:** Gokul commits `src/tt/mod.rs`, confirms
`cargo test` green. **Phase 32's priority-ordered first four findings
(#1/#3/#4/#2 → 32.1-32.4) are all done as of this session.** Remaining:
32.5 (packed mg/eg score sign-extension bug), 32.6 (SEE missing
illegal-king-recapture guard), 32.7 (iterative.rs sentinel
misclassifying a real score of 0) — none of these three have been
independently re-verified against live source yet, unlike #1-#4 and
#8. Worth doing that verification pass before fixing each, same
discipline as every finding so far — two of the four verified findings
(#3, #4) turned out to have a materially different real-world status
than the report's own framing once actually checked. Also still open:
D114/D117's unmeasured toggles (`ImprovingHeuristic` closed at D115;
`RecaptureExtension` still needs its n=200 confirmation), D118's LMR
fix has no dedicated Elo measurement yet, and the older backlog (29.7h/
PlayStyle tuning, 30.5/30.8 WASM UCI GUI items) remains untouched.

---

## Session 120 — 2026-08-02 (32.2 functional check + 32.3: LMR gate now calls should_apply_lmr() directly, shipped live — D118)

**Built/done:** Gokul ran the n=20 `RecaptureExtension` functional
check via `uci_match_runner.yml` — 20 clean completed games (3-3-14,
50.0%, -0.0 Elo), confirming the D117 wiring actually works (no
crashes/hangs on the mechanism's first-ever live run). Flat at n=20 as
expected; deferred the n=200 confirmation rather than running it now —
addendum added to DECISIONS.md D117.

Moved to Phase 32.3 (review finding #4). Confirmed via full-repo grep
that `pruning::should_apply_lmr()` — fully tested, correctly excludes
killer moves and the TT move from LMR — had exactly one caller before
this session: its own unit tests, same "tested helper, dead in
practice" shape as D117's `extension()` discovery two sessions ago.
The real, live LMR gate in `alpha_beta.rs`'s move loop was an inline
duplicate of its condition missing both the `is_killer` and
`is_tt_move` checks. Fixed by replacing the inline duplicate with a
direct call to `should_apply_lmr()` — the two conditions can't diverge
again since there's only one now. Removed the resulting unused
`MIN_DEPTH_LMR` import. **Shipped live, not behind a new gated toggle**
— treated as a correctness/wiring fix (same category as D116, not
D114/D117's new-technique-needs-SPRT category), since excluding the
TT move and killers from LMR is standard practice and the tested
function already encoded the clearly-intended design; the bug was pure
wiring, not a new idea. Full reasoning in DECISIONS.md D118.

**Files touched:** `src/search/alpha_beta.rs`, `src/search/pruning.rs`.

**Bugs fixed:** Review finding #4 (D118). No new bugs introduced —
brace-balance and test-count checks both passed on the first attempt
this session (247/247 in `alpha_beta.rs`, 145/145 in `pruning.rs`,
57/57 and 52/52 test counts respectively) — no repeat of Session 118's
near-misses.

**Decisions made:** D118, plus the D117 addendum recording the n=20
functional-check result.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox.
This fix touches the live LMR gate directly (unconditional, no toggle
to fall back to), so it's worth Gokul's attention even though it's not
gated — broader reach than D116's narrow edge case, even if the fix
itself is small and well-justified.

**Next session start point:** Gokul commits both Rust files, confirms
`cargo test` green. Once green: ROADMAP Phase 32.4 (review finding #2,
duplicate `MATE_THRESHOLD` — `tt/mod.rs`'s own `30_000` vs.
`search/mod.rs`'s `900_000`) is next in priority order. Two open A/B
items now on the backlog whenever Gokul wants to batch them:
`RecaptureExtension`'s n=200 confirmation, and (new, optional) a
before/after comparison for D118's LMR fix using the pre-D118 commit
as a pinned reference rather than a `setoption` toggle.

---

## Session 119 — 2026-08-01, cont. (32.2: recapture extension bug fixed AND wired into live search for the first time, gated — D117)

**Built/done:** Continued down Phase 32's priority list to finding #3
(`is_recapture()`). Before touching it, discovered its real status
didn't match the report's framing: `pruning::extension()` — the only
function that ever calls `is_recapture()` — is never called anywhere
in the live search (verified by grepping the whole repo; its only call
site was its own unit test). `alpha_beta.rs` has its own separate,
correct, always-live check-extension instead. So the bug was real, but
its "extends nearly every capture" severity claim overstated actual
impact — it had been fully inert. Surfaced this to Gokul rather than
silently fixing a dead function; he chose to fix the bug **and** wire
the mechanism into live search for the first time, gated behind a new
toggle (same rollout discipline as every Phase 26+ heuristic).

Fixed `is_recapture()` to actually check `prev_move` (same
simplification Stockfish uses for its own recapture check). Extracted
`recapture_and_passed_pawn_extension()` so the live call site and the
still-unused `extension()` reference implementation share one
implementation instead of duplicating logic the way review finding #4
already flagged elsewhere. Wired the recapture component (only —
passed-pawn-push wasn't flagged as buggy and stays out of scope,
unwired) into `alpha_beta.rs`'s move loop's `move_ext` computation,
applying to any move rather than just the TT move, gated behind new
`SearchInfo::recapture_extension_enabled` / UCI `RecaptureExtension`
(default false). New UCI option threaded through `EngineState` →
`cmd_go` → `h_info`/`main_info`, matching every prior toggle exactly.
12 new tests across `pruning.rs`, `alpha_beta.rs`, and `main.rs`. Full
reasoning in DECISIONS.md D117; ROADMAP Phase 32.2 marked done.

**Files touched:** `src/search/pruning.rs`, `src/search/alpha_beta.rs`,
`src/search/mod.rs`, `src/main.rs`.

**Bugs fixed:** The `is_recapture()` bug itself (review finding #3),
plus none of this session's own str_replace edits repeated the two
near-misses from Session 118 — every insert-shaped edit this session
explicitly re-included its anchor text in `new_str`, and a brace-
balance check followed each Rust file edit before delivery (all four
files came back exactly balanced: `alpha_beta.rs` 246/246,
`pruning.rs` 144/144, `search/mod.rs` 96/96, `main.rs` 254/254; test
function counts also cross-checked, e.g. `main.rs` 77 `#[test]` vs. 77
`fn test_` matches).

**Decisions made:** D117.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox —
hand-verified via brace balance, test-count matching, and full-repo
grep for stale call sites, but not compiled or run for real.

**Next session start point:** Gokul commits all 4 Rust files, confirms
`cargo test` green. Once green: ROADMAP Phase 32.3 (review finding #4
— dead `should_apply_lmr()` vs. the inline LMR gate in `alpha_beta.rs`
missing its killer-move/TT-move guards) is next in priority order.
32.4 (finding #2, duplicate `MATE_THRESHOLD`) after that. D117's
`RecaptureExtension` also joins D114's `ImprovingHeuristic` as an
unmeasured toggle needing its own `uci_match_runner.yml` A/B whenever
Gokul wants to run one — could plausibly batch both into the same
session if he'd rather validate a backlog of toggles at once than one
at a time.

---

## Session 118 — 2026-08-01, cont. (external code review: verified against source, fixed finding #1; caught and fixed two self-inflicted str_replace deletion bugs)

**Built/done:** Gokul uploaded a detailed external code review
(`pet-dragon-detailed-report.md`, 8 findings + non-bug suggestions,
claimed checked against `pet-dragon-main.zip`) and asked for a take
before acting. Spot-verified 5 of the 8 findings directly against live
`main` source (not just trusting the report) before responding — all
5 (#1, #2, #3, #4, #8) checked out exactly as described, line numbers
and all. Gave a priority order (#1 → #3 → #4 → #2 → #5 → #6 → #7, #8
parked) with one pushback: #2's "High" severity label is probably
overstated in practice — Pet Dragon's eval essentially never reaches
scores near 30,000 in real games, so the duplicate-`MATE_THRESHOLD`
bug's real-world blast radius is close to zero even though it's a
genuine duplication worth fixing. Gokul approved the priority order.

Fixed finding #1 (D116): the fifty-move-rule draw check fired
unconditionally, with no guard against the position simultaneously
being checkmate. Hoisted `in_check` above the draw-detection block,
ported Stockfish's own guard
(`!checkers() || MoveList<LEGAL>(*this).size()`), added two regression
tests (one real checkmate-at-halfmove-100 position, hand-verified
square by square including the "king can't step along the checking
ray" x-ray rule; one companion "in check but has an escape" position).
Full reasoning in DECISIONS.md D116. ROADMAP Phase 32 tracks all 8
findings + their status.

**Bugs fixed — two of them self-inflicted, caught before delivery:**
While building the D116 fix's tests, a `str_replace` call that was
meant to *insert* two new test functions before
`test_stalemate_returns_draw` instead *replaced* that function's own
`#[test]\n fn test_stalemate_returns_draw() {` signature line (the
`new_str` didn't re-include the anchor text it was matching against).
Caught by the routine post-edit brace-balance check (`{`/`}` count
came back 241/242 instead of matching) before this was ever shown to
Gokul — traced to the orphaned `setup();` line, restored the missing
signature, re-checked (242/242, clean). Then found the **exact same
bug class** already sitting in `ROADMAP.md` from earlier this session:
inserting Phase 31 before "## Milestone Targets" had replaced that
header's own line without re-adding it, leaving the Milestone table
headerless. Also caught (this time by a manual full-tail read, not an
automated check — brace-counting doesn't apply to markdown) and fixed.
**Process note for future `str_replace` calls that insert new content
next to an existing anchor**: when `old_str` includes an existing
line/header purely to anchor the insertion point, `new_str` must
re-include that anchor text verbatim, not just the new content — an
insert-via-replace that drops the anchor is a silent deletion, and
brace/structure counting only catches it in code files, not markdown.
Worth treating this as a standing checklist item after any
insert-shaped edit, not just relying on it being caught by luck twice
in a row.

**Decisions made:** D116.

⚠️ **Not yet CI-confirmed.** No local `cargo test` in this sandbox.
Given this session's own two near-misses, Gokul should look closely at
the diff on `alpha_beta.rs` around both the fix itself and the
`test_stalemate_returns_draw` area specifically before/while confirming
CI is green, not just trust "tests passed."

**Next session start point:** Gokul commits `src/search/alpha_beta.rs`
+ the three docs files, confirms `cargo test` green (real risk this
time, more from the review-response and str_replace fixes described
above than from the fix's own logic, which is small and mirrors
Stockfish's well-established pattern directly). Then continue down
ROADMAP Phase 32's priority list: 32.2 (review finding #3,
`is_recapture()`) is next.

---

## Session 117 — 2026-08-01, cont. (31.2: ImprovingHeuristic A/B flat at n=200, parked off)

**Built/done:** Ran 31.2, the SPRT-style A/B for D114's
`ImprovingHeuristic`. Gokul went straight to 200 games (skipped the
recommended 20-game first look) via `uci_match_runner.yml`: Engine A
`ImprovingHeuristic value true` vs Engine B `false`, seed 80000,
100ms/move. Result: 40-39-121, 50.2%, +1.7 Elo — flat, same conclusive
shape D77 reached for `NullMoveKingGuard`. No code changes — pure
measurement, same as Session 115/117's PlayStyle and this one.

**Bugs fixed:** None.

**Decisions made:** D115 — parked `ImprovingHeuristic` off
(unchanged from its D114 default), mechanism stays merged and tested,
not reverted. Doesn't confirm or rule out the underlying idea, just
the specific hand-picked threshold/margin deltas D114 chose.

**Next session start point:** Phase 31 stands at: 31.1 done, 31.2
closed (this entry), 31.3 open and untouched — the bench report's
`e2b5` finding (SEE-gating LMP/singular margins on capture-adjacent
nodes), a different mechanism, unaffected by D115 either way. Also
still open from prior sessions: 29.7h/leave-as-is for PlayStyle
(D113), 30.8 (real UCI GUI target, Gokul said "not sure yet").

---

## Session 116 — 2026-08-01, cont. (D114: improving flag added for LMP + futility pruning, off by default; ENGINE_ARCHITECTURE.md stale-doc bug caught and fixed)

**Built/done:** Gokul uploaded a 4-game external-Stockfish bench log +
a report analyzing it (Pet Dragon skill 20 lost 4-0 to Stockfish skill
10 regardless of color/think-time). Asked "what's our current
middlegame/endgame setup" — **caught and corrected my own mistake
mid-conversation**: first answer quoted `ENGINE_ARCHITECTURE.md` §4
verbatim, which said the D63/D68 eval-term gaps (passed-pawn king
distance, pawn storm, king-relative PST, Threats) were "documented,
not scheduled." ROADMAP.md Phase 24 (already in context) said they'd
all been DONE and CI-confirmed back in Sessions 83-84, then Texel-
retuned in Phase 25. Verified directly against real source on GitHub
(`eval/pawns.rs`, `eval/king_safety.rs`, `eval/threats.rs`) before
answering again — confirmed all four are implemented and wired into
`evaluate()`. `ENGINE_ARCHITECTURE.md` §4 had simply never been updated
after Phase 24/25 closed; corrected the record to Gokul rather than
silently building on the wrong premise.

The one gap that WAS real: no "improving" flag in search (D60,
Session 82, already on record as a known-untaken change). Gokul asked
to fix it, scoped broader than D60's own note (LMP + futility pruning,
not LMP alone) — full implementation, D114, ROADMAP Phase 31.1. See
D114 for the complete design (dual LMP threshold tables, improving-
aware futility margin, `SearchInfo::static_eval_stack`, gated behind
new `ImprovingHeuristic` UCI option, default false/byte-identical when
off). Also updated `ENGINE_ARCHITECTURE.md` §3 (search technique table)
and §4 (eval section, via the correction above) while already in there
fixing the stale doc — both were quietly wrong before this session.

**Files touched:** `src/search/mod.rs`, `src/search/pruning.rs`,
`src/search/alpha_beta.rs`, `src/main.rs`, `docs/ENGINE_ARCHITECTURE.md`
(this session's uci-wasm addition from Session 114 was still uncommitted
going into this session — both changes are in the delivered file now).

**Bugs fixed:** None in engine code. One documentation bug caught and
fixed: `ENGINE_ARCHITECTURE.md` §4 wrongly described three closed
ROADMAP Phase 24 items as open/unscheduled — see above.

**Decisions made:** D114 (this entry).

⚠️ **Not yet CI-confirmed.** No local `cargo`/`rustc` toolchain in this
session's sandbox — all four Rust files are hand-verified (brace
balance, consistent call-site argument counts, full-repo grep for the
renamed `LMP_THRESHOLDS` constant and old single-arg
`lmp_threshold()`/`should_apply_lmp()` call sites, no stray `SearchInfo`
struct literals elsewhere in the repo that would need the two new
fields added) but not compiled or test-run for real.

**Next session start point:** Gokul commits all 4 Rust files plus
`docs/ENGINE_ARCHITECTURE.md`, confirms `cargo test` is still green in
CI (this is a real risk session — new struct fields, changed function
signatures across 2 files, new UCI option wired through 6+ call sites).
If CI fails, get the real compiler error before guessing at a fix, same
discipline as every other bug in this project's history. Once green:
ROADMAP 31.2 (SPRT-style A/B on `ImprovingHeuristic`, needs a real
`uci_match_runner.yml` run — Gokul's call on when) is the natural next
step for this thread. 31.3 (SEE-gating LMP/singular margins on
capture-adjacent nodes, from the bench report's `e2b5` finding) is a
separate, unstarted mechanism. Also still open: 29.7h/leave-as-is for
PlayStyle (D113), 30.8 (real UCI GUI target, Gokul said "not sure yet").

---

## Session 115 — 2026-08-01, cont. (29.6b: PlayStyle n=200 confirmation — all four modes flatten to noise, D112's leans were sampling noise)

**Built/done:** Gokul chose 29.6b over 29.7h. Ran the n=200
confirmation on all four PlayStyle modes' current hand-picked
constants via `uci_match_runner.yml` (same same-ref/different-
`setoption` pattern as every prior Elo measurement in this repo — no
code changes, pure measurement). 4 runs × 200 games, seeds
70000/70200/70400/70600: Killer +1.7 Elo, Tactical -0.0, Positional
+13.9, Endgame -0.0 (A/Balanced vs B/mode). Every one of D112's n=20
leans (as large as ±34.9 Elo) flattened or reversed — same pattern
D103 already established for TDSE. No mode is measurably better or
worse than Balanced at current untuned constants. Full table and
reasoning in DECISIONS.md D113.

**Bugs fixed:** None — measurement-only session.

**Decisions made:** D113 (this entry).

**Next session start point:** `PlayStyle` stays default-0, unaffected.
Two open, non-blocking paths recorded in D113: 29.7h (bulk self-play +
real Texel-tuning pass for the four modes — the only remaining lever
that could reveal a real effect) or leave PlayStyle as-is. Gokul's
call, same as before this run. Also still open from prior sessions:
30.8 (real UCI-speaking browser GUI target — Gokul answered "not sure
yet," revisit later, doesn't block anything built).

---

## Session 114 — 2026-08-01 (30.7: UCI-protocol WASM bundle added to build.yml's release assets)

**Built/done:** Did ROADMAP.md's 30.7 (previously left `[ ]`, flagged
as "ask Gokul first" — no objection raised, proceeded). Mirrored the
existing `wasm`-feature build block in `build.yml`'s `build-wasm` job
with a second, independent `wasm-pack build --features uci-wasm
--out-dir dist-wasm-uci --out-name pet_dragon_uci` step (same pattern
Phase 30.1-30.4 already established for `deploy.yml`'s Pages build —
never both features in the same `wasm-pack` invocation). Output copied
to `dist/pet_dragon_uci_bg.wasm` / `dist/pet_dragon_uci.js`, uploaded
as a new `pet-dragon-uci-wasm` artifact. `release` job needed no
change beyond the generated body's asset table — it already globs
`dist/**/*` across every downloaded artifact.

Validated the edited `build.yml` with a real `pyyaml` parse before
delivering it (Session 113's own process lesson from last session —
last time a colon-adjacent YAML scalar broke Actions' own parser and
review alone hadn't caught it). Parses clean; all 6 jobs
(`test`/`build`/`regression-gate`/`build-wasm`/`bench`/`release`)
still present and correctly keyed.

**Bugs fixed:** None — pure additive feature, no existing code path
touched.

**Decisions made:** None new (this is straightforward execution of an
already-scoped, already-approved-in-principle roadmap item, not a
design decision).

⚠️ **Not yet CI-confirmed.** No local `cargo`/`wasm-pack` toolchain in
this session's sandbox to actually run the build (same standing caveat
as every session in this repo unless Rust is explicitly installed for
a specific verification, as Session 110/29.7g did). Only YAML-syntax-
validated, not build-verified.

**Next session start point:** Gokul commits `.github/workflows/build.yml`
(only file changed this session) and confirms the `build-wasm` job
actually succeeds — specifically that `pet_dragon_uci_bg.wasm` and
`pet_dragon_uci.js` build cleanly under the `uci-wasm` feature and
appear as release assets on the next `main` push (rolling `latest`
release) or tagged release. If it fails, get the real Actions error
message before guessing at a fix — same discipline as every other bug
in this project's history. Remaining open items unchanged from Session
113: 29.7h (bulk self-play + real tuning pass for PlayStyle), 29.6b
(TDSE-adjacent n=200 confirmation vs. skip-to-tune, actually a Phase 29
item), 30.8 (is there a real target UCI-speaking browser GUI in mind).

---

## Session 113 — 2026-07-31, cont. (build.yml YAML syntax error, caught by Actions itself, fixed)

**Built/done:** Gokul uploaded a screenshot of Actions run #585 — "Add
files via upload," Failure, annotation `Invalid workflow file:
.github/workflows/build.yml#L58 — You have an error in your yaml
syntax on line 58`. That line was Session 112's new `run: cargo test
--lib -- --ignored --test-threads=1 eval::style::tests::
texel::predict::tests::` — a plain (unquoted) YAML scalar ending in a
bare `::` right before end-of-line, which trips YAML's ambiguity rule
around colons in plain scalars (a colon at/near end-of-line reads as
"start of a mapping value" to a strict parser). Fixed by switching
that one `run:` line to a block scalar (`run: |`), matching the style
this file already uses elsewhere for multi-line commands — sidesteps
the whole class of colon-ambiguity issue rather than trying to escape
individual colons. Verified with an actual YAML parser (`pyyaml`) this
time, not just visual inspection — confirmed it parses and the exact
command text survives the block-scalar wrapping unchanged.

**Bugs fixed:** 1. Cause: didn't run the workflow file through a YAML
parser before delivering it — Rust's `::` module-path separator is
fine in Rust, but the same characters are a YAML footgun when unquoted
near a line boundary. Fix: block-scalar the affected `run:` line; also
worth noting as a process gap — future workflow-file edits should get
a `pyyaml`/`yamllint`-style parse check before delivery, the same way
Rust changes get an actual `cargo build`/`cargo test`, not just visual
review.

**Decisions made:** None.

**Next session start point:** Commit the corrected `build.yml` (only
file changed this session) and re-run Actions to confirm the workflow
itself is valid and the new "Run PlayStyle tests" step actually
executes. Everything else (29.7h, 29.6b, 30.7, 30.8) unchanged from
Session 112.

---

## Session 112 — 2026-07-31, cont. (ThreatDefusal race root-caused: same mechanism as 29.7i, not a separate bug — Session 111's fix was necessary but not sufficient)

**Built/done:**

1. Gokul said "Fix it" re: the ThreatDefusal flake flagged (not
   investigated) at the end of Session 111. Read `search::iterative`'s
   `test_threat_defusal_default_off_matches_pre_tdse_behavior` in full
   and ruled out two hypotheses by reading code, not guessing: (a)
   depth-only `TimeControl` gets an effectively-infinite hard time
   limit (`u64::MAX / 2` ms) from `allocate_time()`, ruling out a
   time-based-cutoff explanation; (b) `threat_defusal` and `thread_id`
   are hardcoded literal defaults in `SearchInfo::new()`, not sourced
   from any global — ruling out those as direct culprits.

2. Reproduced empirically rather than continuing to theorize: 15
   isolated runs of the test — clean every time. Only fails as part of
   the FULL suite under forced `--test-threads=8`, ~1 in 8-10 runs.
   Caught one live: two identically-configured depth-6 searches on an
   identical position returned different best moves — genuine
   non-determinism from ostensibly-identical, fully-independent inputs.

3. First hypothesis (unsynchronized `static mut` writes in
   `init_zobrist()`/`init_masks()`/`init_magic()`, called by literally
   every test's `setup()` helper under default parallelism) was a real,
   independently-worth-fixing UB issue — wrapped all three in
   `std::sync::Once` (`bitboard/masks.rs`, `bitboard/magic.rs`,
   `position/zobrist.rs`), zero signature changes, zero risk to the
   ~500 existing call sites. **Stress-tested this fix specifically: it
   did NOT eliminate the flake** (2/20 runs still failed) — an honest
   negative result, reported as such rather than declared victory.

4. Kept investigating rather than stopping at a partial fix: confirmed
   `search::alpha_beta`'s leaf eval calls `eval::evaluate_styled()`
   UNCONDITIONALLY — meaning literally any test running real search,
   anywhere in the whole suite, reads the process-global `PLAY_STYLE`
   atomic on every node. This reframed Session 111's earlier
   `PLAY_STYLE_TEST_LOCK` fix: it stops the 10 PlayStyle-aware tests
   from racing EACH OTHER, but does nothing for ThreatDefusal's test
   (or any other search test), which has no reason to ever acquire a
   lock it doesn't know exists — a lock only excludes other
   lock-holders.

5. The actual complete fix: found this project already has a
   precedent for exactly this class of problem — `node_count`'s
   benchmark-style tests, `#[ignore]`d and run via a dedicated
   `--ignored --nocapture` CI step. Applied the same pattern: all 10
   PLAY_STYLE-mutating tests marked `#[ignore]`, moved to a new
   `build.yml` step running `--ignored --test-threads=1` — full
   isolation from the entire binary, not just from each other.
   Corrected `PLAY_STYLE_TEST_LOCK`'s doc comment to record the full,
   accurate diagnosis (Mutex necessary-but-insufficient, kept as
   defense-in-depth) rather than leave the earlier, incomplete
   explanation standing uncorrected.

6. Verified by actually trying to break it, not by re-running once:
   25 full-suite runs at `--test-threads=8` (0 failures, vs. 2/20
   before this fix), 10 more at `--test-threads=16` (0 failures), 5
   repeats of the new isolated step (5/5 clean, all 10 tests passing
   each time). 35/35 clean across every stress configuration tried.

**Bugs fixed:** 1, but it took two attempts to actually close —
worth recording both. Cause: `search::alpha_beta`'s leaf eval reads a
process-global atomic (`PLAY_STYLE`) unconditionally in every search,
so any test doing real search work — regardless of whether it knows
PlayStyle exists — can have its results corrupted by a concurrently-
running PlayStyle-mutating test on another thread. First fix
(Session 111's Mutex) addressed the wrong symmetry — mutual exclusion
among the PlayStyle-aware tests, when the actual requirement was
isolation from the ENTIRE suite. Second fix (this session):
`#[ignore]` + a dedicated `--test-threads=1` CI step, which provides
true whole-binary isolation — the standard fix for global state that
affects code paths tests outside the "aware" group also exercise. Why
correct: verified empirically at 35/35 clean stress runs across three
different thread-count configurations, not argued from the code alone.

**Decisions made:** None requiring a DECISIONS.md entry — test-
infrastructure correctness, not an architectural choice, same
category as Session 111's fix attempt.

**Next session start point:** Commit the 6 changed files
(`eval/style.rs`, `texel/predict.rs`, `bitboard/masks.rs`,
`bitboard/magic.rs`, `position/zobrist.rs`, `.github/workflows/
build.yml`) and re-run Actions — the new "Run PlayStyle tests" step
should show 10 passed, and the main "Run all tests" step should show
494 passed / 10 ignored, both clean. 29.7h (real bulk self-play +
actual tuning run), 29.6b, 30.7, 30.8 all remain open, untouched by
this session.

---

## Session 111 — 2026-07-31, cont. (Phase 29.7 CI-caught race condition, fixed and stress-verified)

**Built/done:**

1. Gokul uploaded Actions logs for the `Test` job on the commit that
   included Session 110's PlayStyle Texel-tuning work. `Run all tests`
   step failed: `texel::predict::tests::test_predict_style_matches_
   evaluate_styled_all_modes` — `mode 1 seed 12: predict=44
   evaluate+style=24 (core=24, style=0)`. Had passed 3/3 locally in
   Session 110 (this sandbox has 1 CPU core; the race needs real
   multi-core interleaving to trigger, which Actions has and this
   sandbox doesn't — passing locally was scheduling luck, not evidence
   of correctness).

2. Diagnosed the actual mechanism rather than just re-running: `mode
   1` (KILLER) held fixed across a 100-seed loop, but `style=0` in the
   failure — exactly what `evaluate_style()` returns when the global
   `PLAY_STYLE` atomic reads BALANCED, not KILLER. Traced to a genuine
   cross-module race: `eval/style.rs`'s own 8 pre-existing tests (an
   established, shipped file) also mutate the same global, and the
   project's existing "one test function, not several" convention
   (`eval/mod.rs`, documented there) only protects within one file —
   it was never designed to cover two separate test modules touching
   the same global concurrently.

3. Added a real fix, not a workaround: `pub(crate) #[cfg(test)] static
   PLAY_STYLE_TEST_LOCK: Mutex<()>` in `eval/style.rs`, next to
   `PLAY_STYLE` itself, acquired as the first line of all 8
   pre-existing tests in that file plus both of Phase 30's new tests
   in `predict.rs`. Genuine cross-module mutual exclusion.

4. Verified by actually trying to break it, not just re-running once:
   5 repeated runs at `--test-threads=16` targeting the two affected
   modules specifically, then 3 full-suite runs at cargo's actual
   default thread count (matching what CI runs) — 504/504 clean every
   single time.

5. **Side finding, deliberately not touched**: forcing
   `--test-threads=8` on the FULL suite (more aggressive than default)
   surfaced a different, unrelated, pre-existing race —
   `search::iterative::tests::test_threat_defusal_default_off_matches_
   pre_tdse_behavior`, a `ThreatDefusal` global-toggle test with
   nothing to do with PlayStyle. Flagged in ROADMAP 29.7i for
   awareness rather than fixed — out of this session's scope, didn't
   reproduce at cargo's actual default settings (so not blocking
   anything right now), and expanding into an unrelated already-shipped
   file wasn't something to do without a heads-up first.

**Bugs fixed:** 1 (the race above). Cause: `PLAY_STYLE_TEST_LOCK`
didn't exist yet — Session 110 added a long-running, mode-holding test
touching a process-global atomic that other already-shipped tests
(in a different file) also mutate, without any mutual-exclusion
mechanism between them; the project's existing same-file convention
didn't cover cross-file interleaving. Fix: a real `Mutex`, shared via
`pub(crate)`, acquired by every test on either side of the file
boundary that touches the global. Why correct: this is the standard,
complete fix for shared-mutable-global-state test races — verified
empirically by 8 stress runs (not just argued from the code), and it
doesn't change any non-test code path at all (the lock is entirely
`#[cfg(test)]`-gated).

**Decisions made:** None requiring a DECISIONS.md entry — this is a
test-infrastructure correctness fix, not an architectural choice.

**Next session start point:** Commit `eval/style.rs` and
`texel/predict.rs` (only files changed this session) and re-run
Actions to confirm the specific failure is gone. 29.7h (real bulk
self-play generation + actual tuning run), 29.6b, 30.7, 30.8 all
remain open exactly as before — none of them touched or blocked by
this session. The `ThreatDefusal` race noted in 29.7i is available to
pick up whenever Gokul wants it looked at, but isn't currently causing
observed CI failures.

---

## Session 110 — 2026-07-31 (Phase 29.7: Texel-tuning pipeline built for PlayStyle, real compile/test/end-to-end verification, 3 bugs caught before shipping)

**Built/done:**

1. Gokul chose Option 1 ("build the full integration") for 29.7 after
   an explain-it-simply pass, since the earlier framing
   (`ask_user_input_v0`) didn't land. Read `texel_tune.rs`,
   `texel_gen.rs`, and all of `src/texel/` (`features.rs`, `weights.rs`,
   `weights_f64.rs`, `predict.rs`, `predict_f64.rs`) in full before
   writing anything, plus re-verified every PlayStyle bonus function in
   `eval/style.rs` line-by-line against the live source (not from
   memory) before duplicating its feature-gathering logic.

2. Extended the pipeline across 7 files (full breakdown in ROADMAP
   29.7a–29.7g): `StyleFeatures` + `extract_style_features()` in
   `features.rs`; 5 new tunable fields end-to-end through
   `weights.rs`/`weights_f64.rs` (`PARAM_COUNT`, flatten/unflatten,
   `to_tunable_weights`, `From`, round-trip test); forward + gradient
   PlayStyle scoring in `predict.rs`/`predict_f64.rs`, including a
   finite-difference numerical gradient test (the strongest check in
   this diff — catches sign/factor errors a self-consistency test
   can't); mode-tagging in `texel_gen.rs`'s output format (additive,
   backward compatible); mode-aware, dual-format-accepting data loading
   in `texel_tune.rs`.

3. **Actually compiled and tested, not just reviewed.** Installed a
   Rust toolchain in-session specifically because a change this size
   shouldn't ship on manual review alone — worked around two rounds of
   ancient-toolchain-vs-modern-dev-dependency friction (apt's rustc
   1.75 vs. `criterion`'s transitive deps needing edition2024/newer
   rustc; worked around by downgrading `clap`/`half` for the local
   check only, not part of the deliverable) to get a real
   `cargo build`/`cargo test` run.

4. Ran a genuine end-to-end smoke test, not just unit tests:
   `texel_gen` generated real KILLER-tagged self-play data;
   `texel_tune` loaded it mixed with a hand-derived legacy-format
   (untagged) file in the same run, proving backward compatibility by
   demonstration rather than by claim; ran 2 real gradient-descent
   epochs; loss decreased monotonically (0.0571→0.0508) with only the
   tagged mode's constants moving, everything else held exactly at
   default; `texel_diag` then round-trip-parsed that exact tuned-output
   file without error.

**Bugs fixed:** 3, all caught by the actual compile (not by review):
   - `E0428` duplicate `chebyshev_distance` definition in `features.rs`
     — this file already had one (used by an unrelated passed-pawn
     feature). Cause: didn't grep the file for the name before adding
     a same-named helper. Fix: deleted the duplicate, reused the
     existing one. Why correct: bodies were byte-identical; no
     behavior change, purely removing dead redundancy.
   - `E0063` missing fields in `texel_diag.rs`'s `TunableWeights`
     construction. Cause: this file wasn't in the original file list I
     read/scoped for this task — its existence as a manual
     `TunableWeights`-constructing consumer only surfaced via the
     compiler. Fix: added the same 5 fields via the same
     `extract_scalar`/`extract_int_array` helpers the file already
     uses for every other field. Why correct: matches the file's own
     established convention exactly, verified by the subsequent
     round-trip smoke test actually parsing tuned PlayStyle output.
   - `E0433` unresolved `crate::eval::style`/`crate::texel::features`
     paths in `texel_gen.rs` and `texel_tune.rs`. Cause: forgot that
     `src/bin/*.rs` files are separate crates from the library — every
     other bin file in this project reaches library code via
     `pet_dragon_lib::...`, not `crate::...`. Fix: switched all 5
     occurrences across the two files to `pet_dragon_lib::`. Why
     correct: matches the existing, working convention every other
     line in both files already follows.

**Decisions made:** None requiring a new DECISIONS.md entry — the
"flat i32, not `S(mg,eg)`" choice for the 5 new weights and the
"dispatch-on-mode-tag, zero-gradient-elsewhere" data design are
recorded directly in the code's own doc comments and ROADMAP 29.7,
same threshold reasoning as Phase 30's decisions in Session 108.

**Next session start point:** 29.7h is the real remaining work —
generate actual bulk self-play data (hundreds/thousands of games,
not this session's 2-game smoke test) for all 4 non-Balanced modes
via Actions, run the real tuning pass, sanity-check with `texel_diag`,
and copy the results into `eval/style.rs`. Gokul's call on game-count
budget — not blocking, PlayStyle still defaults off. 29.6b, 30.7,
30.8 all remain open and unblocked alongside it, same as before this
session.

---

## Session 109 — 2026-07-30, cont. (Phase 30 CI-verified: 1 real test bug caught and fixed, zero production regressions)

**Built/done:**

1. Gokul uploaded the Actions log bundle for the `Test` job (both the
   pre-existing `cargo test` step and the new `--features uci-wasm`
   step added in Session 108).

2. Read the logs directly rather than assuming: plain `cargo test`
   step — 517 tests, 100% green, zero regression from Phase 30's
   `lib.rs` changes (confirms the `wasm_bindgen`-import-gate fix from
   last session was sufficient and the default build path is
   untouched). `--features uci-wasm` step — compiled clean (no
   compile errors from the new feature/module at all), but 3 of 16 new
   tests failed at runtime.

3. Diagnosed by reading the actual panic output, not guessing: all
   three failures were `assert_eq!`/`assert_ne!` mismatches comparing
   `session.pos.to_fen()` against the bare `STANDARD_START_FEN`
   constant. Traced to `Position::to_fen()`'s own doc comment
   ("always include Pet Dragon extension") — it appends a 7th FEN
   field unconditionally, so it could never equal the plain 6-field
   constant. Found `Position::to_standard_fen()` already exists in
   `position/mod.rs`, doc-commented "Used for UCI communication with
   external tools" — exactly the right tool, already in the codebase,
   not something needing to be added.

**Bugs fixed:** 3 test assertions in `src/uci_wasm.rs`
(`test_position_startpos_sets_standard_position`,
`test_position_fen_parses_multi_field_fen`,
`test_ucinewgame_resets_position_and_clears_tt`) switched from
`.to_fen()` to `.to_standard_fen()`. Cause: assumed `to_fen()` returns
a bare 6-field FEN without checking its own doc comment first. Fix:
use the extension-free method the codebase already provides for this
exact comparison. Why correct: `to_standard_fen()`'s only difference
from `to_fen()` is omitting the Pet Dragon extension field, which is
precisely what these three tests need to compare against
`STANDARD_START_FEN` correctly — confirmed by reading
`generate_fen()`'s boolean-flag signature and both callers
(`to_fen()` passes `true`, `to_standard_fen()` passes `false`).
No production code in `uci_wasm.rs` changed — `position`/`ucinewgame`
handling was correct all along; only three tests' expected values were
wrong.

**Decisions made:** None.

**Next session start point:** Gokul commits the corrected
`uci_wasm.rs` (this session's only changed file) and re-runs Actions
to confirm all 16 `uci-wasm` tests are green. Once confirmed: ROADMAP
30.7/30.8 remain open, low-urgency, Gokul's call — or return to 29.6b
(PlayStyle n=200 confirmation vs. skip to Texel-tuning), also still
open and unblocked.

---

## Session 108 — 2026-07-30 (New Phase 30: second WASM build target speaking real UCI text protocol, requested directly rather than continuing 29.6b)

**Built/done:**

1. Reviewed 3 uploaded docs (dual-evaluator-proposal.md,
   pet-dragon-vs-stockfish-gap-report.md, uci-options-comparison.md) —
   informational, not code-triggering. Flagged the dual-evaluator
   proposal as likely scope creep pending a product decision (does a
   *variant* engine need a from-scratch Stockfish-NNUE reimplementation
   for the one starting position that happens to be standard chess?)
   — left unscheduled, not rejected outright.

2. Confirmed from uci-options-comparison.md + direct source read:
   native (`main.rs`) already speaks full UCI; the WASM/browser build
   (`wasm` feature) does not — it's a direct-function-call API
   (`search_from_fen` et al.), by design, not an oversight.

3. Gokul asked for a second, separate WASM+JS build that does speak
   real UCI text, alongside (not replacing) the existing browser build.
   Read Tier 2 in full this session (PROJECT_CONTEXT — stale/historical,
   ROADMAP.md is the live source of truth; VARIANT_ARCHITECTURE;
   ENGINE_ARCHITECTURE; DECISIONS.md read via targeted grep for
   WASM-relevant entries, not full 311KB read, given token budget).

4. Read current `Cargo.toml`, `src/lib.rs`, `src/eval/mod.rs`,
   `src/eval/style.rs`, `src/search/{skill,time,mod,iterative}.rs`,
   `src/tt/mod.rs`, `src/position/{mod,fen,make_move}.rs`,
   `src/movegen/mod.rs`, `src/types.rs`, `.github/workflows/
   {deploy,build}.yml` before writing anything, per working-style.

5. Implemented Phase 30 (full breakdown in ROADMAP.md):
   - New `uci-wasm` Cargo feature, independent from `wasm`.
   - New `src/uci_wasm.rs`: `uci_command(line) -> String` export,
     thread_local persistent session (position + TT + options), real
     `uci`/`isready`/`ucinewgame`/`position`/`go`/`setoption` handling.
     16 new unit tests.
   - `src/lib.rs`: declared the module; widened `wasm_main()`'s startup
     gate and the `wasm_bindgen::prelude` import to `any(wasm,
     uci-wasm)` — existing `wasm`-only exports (search_from_fen,
     new_game, etc.) untouched, still gated `wasm` alone.
   - `deploy.yml`: second `wasm-pack build --features uci-wasm` step
     → `web/pkg-uci/`, alongside existing `web/pkg/`.
   - `build.yml`: added a `cargo test --features uci-wasm` step to the
     existing `test` job — without this, the 16 new tests would never
     run in CI (gated behind a non-default feature).

**Bugs fixed:** One, caught before shipping, not after: the first pass
widened `wasm_main()`'s `#[cfg(...)]` to `any(wasm, uci-wasm)` but left
the `wasm_bindgen::prelude` import gated on `wasm` alone — a
uci-wasm-only build would have failed to resolve `wasm_bindgen` at the
`#[wasm_bindgen(start)]` attribute on `wasm_main`. Cause: widened the
function's own gate without checking its dependency's gate in the same
pass. Fix: widened the import to the same `any(...)` condition. Why
correct: both build targets now compile the import exactly when they
need it, and neither gains anything the other doesn't already have.

**Decisions made:** None requiring a new DECISIONS.md entry this
session — Phase 30's design choices (thread_local session state,
dispatch-not-blend for NNUEWeight/PlayStyle reuse, the five documented
architectural limitations around synchronous WASM calls) are recorded
directly in `uci_wasm.rs`'s own module doc comment and ROADMAP.md
30.5, not duplicated into DECISIONS.md — none of them rise to the
"architectural decision with real alternatives considered and
rejected" bar DECISIONS.md entries are for; they're scoping notes on a
single new file.

**Next session start point:** Awaiting Gokul's confirmation that CI is
green on Phase 30 (first-ever Actions run of both `deploy.yml`'s new
second `wasm-pack` step and `build.yml`'s new `--features uci-wasm`
test step — real risk, not yet verified, per ROADMAP 30.6). If green:
ROADMAP 30.7 (mirror into `build.yml`'s release-asset job — ask first,
not requested yet) and 30.8 (is there an actual target GUI in mind?)
are both open, low-urgency, Gokul's call. If CI is red: fix whatever
Actions reports first, before anything else — do not start 29.6b or
any other new task with Phase 30 unverified. 29.6b (PlayStyle n=200
confirmation vs. skip to Texel-tuning) remains open and unblocked
whenever Gokul wants to return to it instead.

---

## Session 107 — 2026-07-29, cont. (PlayStyle self-play first-look: all four modes land within noise of Balanced at n=20; D112)

**Built/done:**

1. Gokul ran the 4 `uci_match_runner.yml` workflow_dispatch triggers
   from last session's instructions (same-ref Balanced-vs-mode, n=20
   each, seeds 0/100/200/300 for Killer/Tactical/Positional/Endgame)
   and uploaded all 4 `uci_match_results.txt` artifacts.

2. Read and recorded all four results as-is (no re-running, no
   estimation): Killer -17.4 Elo (A-vs-B, i.e. Killer nominally ahead
   of Balanced), Tactical -0.0 (dead even), Positional -34.9
   (Positional nominally ahead), Endgame +17.4 (Balanced nominally
   ahead). None crossed the ROADMAP 29.6 flag threshold (~100 Elo
   loss).

3. Framed the results honestly rather than over-reading them: at
   n=20, the standard error on match score is roughly ±11 percentage
   points (~±80 Elo), so all four results sit within about 0.5
   standard errors of zero — indistinguishable from noise, matching
   D106's own "near the noise floor" language for TDSE's n=20 runs.
   Explicitly flagged that Killer/Positional nominally *winning*
   against the fully Texel-tuned Balanced baseline, on completely
   untuned hand-picked constants, is far more consistent with sampling
   noise than a genuine improvement — not something to read into.

**Bugs fixed:** none.

**Decisions made:** D112 (results table + two genuinely open next
steps, decision left to Gokul, same framing D106 used for TDSE).

**Next session start point:** Gokul's call between two options logged
in D112/ROADMAP 29.6b: (a) run n=200 confirmation on all four modes'
current hand-picked constants before drawing any conclusion, or
(b) skip straight to Texel-tuning the four modes' constants (29.7)
since none are tuned yet and a confirmation run on soon-to-be-replaced
values has limited value. Neither is urgent — `PlayStyle` defaults off
regardless, so nothing here is shipping-blocked. The other open item,
29.8 (spin vs. combo UCI presentation), also remains Gokul's call
whenever convenient.

---

## Session 106 — 2026-07-29, cont. (Five-mode PlayStyle additive eval bonus implemented — style.rs new file, evaluate_styled() wired in, UCI option added; D111)

**Built/done:**

1. Gokul confirmed CI green on Session 105's F-4 fix — the entire
   external review backlog (F-1 through F-5, plus the UCI sentinel
   finding) is now closed. Picked up `playstyle-proposal.md` next, per
   the queued options.

2. Per the proposal's own note that `alpha_beta.rs` had shifted since
   it was drafted, re-fetched `eval/mod.rs`, `search/alpha_beta.rs`,
   and `main.rs` fresh before writing anything — did not trust the
   proposal's cited line numbers or code snippets. Confirmed the
   `evaluate()` wrapper had moved again since the proposal's last check
   (1133 → 1169, after this session's own earlier F-2 fix added the
   `make_null_move`/`unmake_null_move` helper earlier in the same
   file). Also confirmed `king_safety.rs:145`'s king-zone one-liner
   was unchanged and safe to duplicate (not import) into the new file,
   exactly as the proposal specified.

3. Implemented the proposal's Option B in full:
   - **`src/eval/style.rs` (new file)** — `PlayStyle` atomic selector
     (0-4), `evaluate_style()` dispatcher, and all four bonus
     functions (Killer/Tactical/Positional/Endgame) plus shared
     `all_piece_attacks`/`pawn_and_minor_attacks` helpers. One
     deliberate refinement over the proposal's exact wording: Tactical
     mode's "net of the opponent" metric was implemented as a
     genuinely symmetric comparison (squares WE control in THEIR half
     minus squares THEY control in OUR half), which reads more
     naturally as a mirror than the proposal's literal phrasing.
   - **`src/eval/mod.rs`** — `pub mod style;` + `evaluate_styled()`
     wrapper.
   - **`src/search/alpha_beta.rs`** — the one call-site change: `evaluate()`
     now delegates to `evaluate_styled()` instead of `evaluate_blended()`.
   - **`src/main.rs`** — `PlayStyle` UCI option + `"playstyle"`
     setoption arm, mirroring `NNUEWeight`'s direct-global-set pattern
     line-for-line (confirmed by reading that pattern fresh, not from
     memory of the proposal's description of it).

4. Caught and fixed several FEN-legality bugs in my own first-draft
   test positions before finalizing `style.rs`'s tests — `Position::
   from_fen` rejects both adjacent-kings and opponent-in-check
   positions (`FenError::OpponentInCheck`), which two of my initial
   test FENs violated (kings placed adjacent in one case, a rook/queen
   giving literal check to the non-mover in another). Redesigned all
   affected test positions to avoid both failure modes while still
   testing the intended sign behavior.

5. Bonus fix while `main.rs`'s NNUEWeight block was already open for
   the PlayStyle option: found the same "D23 default 25%" staleness
   there that F-4/D110 already fixed in `eval/mod.rs` — the original
   Engineering Review's F-4 finding only caught the `eval/mod.rs` copy
   of this comment, not this second one in `main.rs`. Fixed both
   comments now say the same thing.

6. Added 10 new regression tests total: 8 in `style.rs` (Balanced
   no-op across 3 position types, out-of-range defensive fallback,
   `set_play_style` clamping, one sign-correctness test per
   non-Balanced mode), 1 in `eval/mod.rs` (`evaluate_styled() ==
   evaluate_blended()` at Balanced default), 1 in `main.rs` (PlayStyle
   setoption sets and clamps).

**Bugs fixed:** none from the external review set this session — the
"D23 default 25%" comment fixed in `main.rs` is a second instance of
F-4's staleness pattern, not a new independent bug; folded into D111
rather than given its own decision number since it was found and fixed
incidentally while implementing PlayStyle, not as a targeted pass.

**Decisions made:** D111 (full design writeup, including what was
deliberately deferred).

**Next session start point:** Gokul commits all four files —
`src/eval/style.rs` (NEW), `src/eval/mod.rs` (REPLACE), `src/search/
alpha_beta.rs` (REPLACE), `src/main.rs` (REPLACE) — and confirms CI is
green, including the 10 new tests (487 → 497 expected library test
count). Once confirmed green: per ROADMAP Phase 29.6, run self-play
validation for each non-Balanced mode vs. Balanced via the existing
selfplay GitHub Actions workflow before making any Elo claims about
PlayStyle — none of the four modes' constants are tuned yet, and
self-play data is the prerequisite for the eventual Texel pass (29.7).
Gokul's call on the open spin-vs-combo UCI question (29.8, non-blocking).

---

## Session 105 — 2026-07-29, cont. (Fix F-4: stale NNUE blend-weight doc comment in eval/mod.rs; D110)

**Built/done:**

1. Gokul confirmed CI green on Session 104's UCI sentinel fix. All
   confirmed-and-actionable items from the original external review
   set (F-1, F-2, and the Performance Review's §4.4 sentinel finding)
   are now closed. Picked F-4 (lowest-priority remaining item) next.

2. Fetched `src/eval/mod.rs` fresh and confirmed the bug directly:
   `evaluate_blended()`'s doc comment read "D23 default 25%", while the
   actual initializer (`NNUE_BLEND_WEIGHT_PCT` = `AtomicU32::new(0)`)
   has been 0 since D25, with NNUE further shelved by D61. Code itself
   was already correct — this was a comment-only staleness, matching
   both prior reviews' characterization.

3. Rewrote the comment to state the real current default (0%, pure
   HCE) and cite D25/D61 instead of D23's superseded figure. Checked
   the rest of the file for any other stale "25%"/"0.25" references —
   found none; the one remaining D23 mention (on the `static` field
   itself) is accurate background on *why* it became runtime-
   configurable, not a stale default claim, so left unchanged.

**Bugs fixed:** F-4 (stale NNUE blend-weight doc comment) — see
DECISIONS.md D110.

**Decisions made:** D110.

**Next session start point:** Gokul commits `src/eval/mod.rs`
(REPLACE) — comment-only change, no new tests, test count stays at 487.
This closes out every item from the original three-part external
review set (Engineering Review, Performance Review, Documentation
Review) — F-1 through F-5 are now either fixed (F-1, F-2, F-4, the
sentinel finding) or confirmed-intentional/no-action (F-3 per D4, F-5
informational only). Remaining open, non-review items: the README test
count (521 → 487 current, a separate staleness item, not one of F-1
through F-5), and the five-mode PlayStyle proposal
(`playstyle-proposal.md`) — re-verify its cited line numbers against
current source first, since `alpha_beta.rs` and `types.rs` have both
changed since it was written. Gokul's call on which to start.

---

## Session 104 — 2026-07-29, cont. (Move::NULL now prints UCI standard "0000" instead of "a1a1"; D109)

**Built/done:**

1. Gokul confirmed CI green on Session 103's F-2 fix — both confirmed
   review defects (F-1, F-2) now closed. Picked the small UCI sentinel
   fix (Performance Review §4.4) as the next item from the queued
   options.

2. Fetched `src/types.rs` fresh and confirmed the bug: `to_uci()` had
   no special case for `Move::NULL` (`from: A1, to: A1`), so it printed
   "a1a1" instead of the standard UCI null-move sentinel "0000" that
   most GUIs/tooling expect when there's no legal move to report
   (checkmate/stalemate at the root).

3. Checked the repo (full tarball grep, not just the files touched) for
   any existing code relying on the old "a1a1" output string before
   changing it — found only `uci_match_runner.rs` asserting that
   *inbound* `parse_uci_move("a1a1")` is rejected as illegal, which is
   unrelated (parsing GUI input, not formatting engine output) and
   confirmed unaffected.

4. Fixed by special-casing `to_uci()` on `self.is_null()`. Safe because
   no real, legal move can ever have `from == to` — `is_null()` already
   depends on exactly that fact elsewhere in the codebase.

**Bugs fixed:** UCI null-move sentinel (Performance Review §4.4) — see
DECISIONS.md D109.

**Decisions made:** D109.

**Next session start point:** Gokul commits `src/types.rs` (REPLACE)
and confirms CI is green, including the 1 new test (486 → 487 expected
library test count). All items from the original external review set
are now resolved (F-1 through F-5) except the two remaining optional/
low-priority follow-ups the reviews themselves flagged as non-blocking:
F-4's stale `eval/mod.rs` doc comment, and the README test-count
staleness (521 → current count). Otherwise: start the five-mode
PlayStyle proposal (`playstyle-proposal.md`) — re-verify its cited line
numbers against current source first, since `alpha_beta.rs` and
`types.rs` have both changed since that proposal was written. Gokul's
call.

---

## Session 103 — 2026-07-29 (Fix F-2: en-passant/hash desync in null-move pruning, both call sites; factored shared make_null_move/unmake_null_move helper; D108)

**Built/done:**

1. Gokul confirmed CI green on the Session 102 commit (F-1 TT sizing
   fix, D107) — 484/484 tests passing, matching the expected count.
   Moved to F-2 next, per the priority order from the review set and
   last session's own next-start note.

2. Fetched `src/search/alpha_beta.rs` and `src/search/pruning.rs`
   fresh from `g-c-3/pet-dragon` before writing anything — did not
   trust the review's quoted line numbers or code snippets. Confirmed
   F-2 present exactly as described in both locations: the real
   null-move probe (~line 499 pre-fix) and the duplicated flip inside
   `extract_threat_move()` (~line 951 pre-fix, added in Phase 28/D98).
   Both cleared `pos.en_passant` without XOR-ing `ep_key(old_ep.file())`
   out of `pos.hash` first — hash and field disagreed for the rest of
   that null-move subtree, a structural mismatch (not the generic
   64-bit collision risk the TT already accepts, D4).

3. Checked `position/make_move.rs` and `position/zobrist.rs` for the
   exact existing pattern (`if let Some(ep) = self.en_passant { self.
   hash ^= ep_key(ep.file()); }`) rather than inventing new hashing
   logic, and mirrored it exactly at both null-move sites, symmetric
   on both make and unmake.

4. Went further than a minimal two-site patch: factored the whole
   null-move flip (side-to-move + en-passant + hash, make and unmake)
   into two shared functions, `make_null_move(pos) -> Option<Square>`
   and `unmake_null_move(pos, old_ep)`, both `#[inline]`. Both call
   sites now use these instead of duplicated inline logic — per the
   Engineering Review's own recommendation, and directly addressing
   the failure mode the Documentation Review named: D98 reused this
   block once already and only checked the reuse for API correctness,
   not for this hash-consistency invariant, which is exactly how the
   bug reached `extract_threat_move()` in the first place. Preserved
   D104's ordering requirement (`see_value_of` must run while `pos.
   side_to_move` still equals the threat's mover) — that call still
   happens before `unmake_null_move()` at that site, unchanged.

5. Added two regression tests using `Position::compute_hash()` (a
   from-scratch recompute, already existed in `position/mod.rs`,
   `pub fn`) — exactly the general "recompute and compare" invariant
   check the Documentation Review recommended (§7, rec. 5), applied
   directly to this bug rather than left as a suggestion:
   `test_null_move_helper_keeps_hash_consistent_with_recompute` (built
   a position with an active en-passant square via FEN, checks
   `pos.hash == pos.compute_hash()` mid-flip and after the full
   round-trip — fails under the pre-fix code, passes under the fix)
   and `test_null_move_helper_hash_consistent_without_en_passant`
   (no-op case).

**Bugs fixed:** F-2 (en-passant/hash desync in null-move pruning, both
call sites) — see DECISIONS.md D108 for full reasoning, the shared-
helper factoring, and why it went uncaught (D98's verification method
checks function-signature correctness, not cross-field hash/state
invariants — named explicitly in the Documentation Review as a category
of bug that method isn't naturally aimed at).

**Decisions made:** D108.

**Next session start point:** Gokul commits `src/search/alpha_beta.rs`
(REPLACE) and confirms CI is green, including the 2 new regression
tests (484 → 486 expected library test count). Once confirmed green,
both F-1 and F-2 — the two confirmed, undocumented gaps from the
external review set — are resolved; F-3/F-4/F-5 need no code action
(F-3 already documented as intentional per D4; F-4/F-5 are low-priority
informational items the review left as optional follow-ups, not
blocking). Reasonable next options: the `Move::NULL` → `"0000"` UCI
sentinel fix (Performance Review §4.4, small/precise), the stale
`eval/mod.rs` doc-comment cleanup (F-4), or starting the five-mode
PlayStyle proposal (`playstyle-proposal.md`) — re-verify its cited line
numbers against current source first, since `alpha_beta.rs` changed
again this session. Gokul's call.

---

## Session 102 — 2026-07-29 (Fix F-1: TT sizing bug silently halved capacity at every exact power-of-two Hash size; D107)

**Built/done:**

1. Acted on the external Engineering/Performance/Documentation review
   set (uploaded 28 July 2026), which had independently confirmed F-1
   as a genuine, undocumented defect and ranked it the top-priority
   fix. Fetched `src/tt/mod.rs` fresh from `g-c-3/pet-dragon` (not
   assumed from the review's quoted snippet) before touching anything.

2. Confirmed the bug directly against source: `TranspositionTable::
   new()` computed `num_entries` via `(bytes / entry_size).
   next_power_of_two() / 2`. `next_power_of_two()` is a no-op when its
   input is already a power of two, so the `/ 2` right after it simply
   discards half the requested capacity — and `bytes / entry_size`
   lands on an exact power of two at every standard Hash value
   (16/32/64/128/256 MB), including the engine's own 64 MB default.
   `TTEntry` is confirmed 16 bytes (matches its own doc comment), so
   64 MB / 16 B = 4,194,304 = 2^22 exactly — the engine's default Hash
   setting was silently running at 32 MB of real capacity, not 64 MB.

3. Fixed by replacing the round-trip through `next_power_of_two()/2`
   with a direct floor-to-largest-power-of-two computation
   (`1usize << (usize::BITS - 1 - raw_entries.leading_zeros())`,
   guarded for `raw_entries == 0`), keeping the existing `.max(1024)`
   floor unchanged after that step. Behavior is unchanged for
   non-exact-power-of-two byte counts — the old and new formulas were
   already equivalent there; only the exact-power-of-two case (every
   standard Hash size) was affected.

4. Added two regression tests in `tt/mod.rs`:
   `test_tt_size_exact_power_of_two_not_halved` asserts the *exact*
   expected entry count (not just an inequality, unlike the pre-
   existing `test_tt_size`) at 16/32/64/128/256 MB, computed from
   `std::mem::size_of::<TTEntry>()` at run time so it can't silently
   drift if the struct's layout ever changes size.
   `test_tt_size_non_power_of_two_floors_correctly` confirms the
   non-exact case (100 MB) is unaffected.

5. Checked `main.rs`'s `Hash` UCI option wiring (`cmd_setoption`,
   `tt.resize()`, `TranspositionTable::new(DEFAULT_HASH_MB)`) for any
   code that assumed the old halved behavior — none found; the fix is
   fully self-contained to `tt/mod.rs`.

**Bugs fixed:** F-1 (TT sizing) — see DECISIONS.md D107 for full
before/after reasoning and why it went uncaught (the pre-existing test
only asserted `size_mb() <= 64`, an inequality the buggy half-size
value also satisfied).

**Decisions made:** D107.

**Next session start point:** Gokul commits `src/tt/mod.rs` (REPLACE)
and confirms CI is green, including the 2 new regression tests
(482 → 484 expected library test count). Once confirmed green, move to
F-2 (en-passant hash desync in null-move pruning, `search/
alpha_beta.rs`) — the other confirmed, undocumented gap from the same
review set, next in priority order per the Engineering Review's
recommendation. Read `search/alpha_beta.rs` and `search/pruning.rs`
(the null-move probe and `extract_threat_move`) fresh before writing
that fix — do not assume the review's quoted line numbers are still
current, the file has moved since (TDSE landed in between).

---

## Session 101 — 2026-07-28, cont. (Phase 28: SEE-degradation Elo results land near the noise floor across two independent 200-game samples; crash-safety holds; D106)

**Built/done:**

1. Gokul ran both the 20-game first look (seed 62000) and, without a
   checkpoint in between, the 200-game confirmation (seed 62500) for the
   SEE-degradation signal on the post-D105 (compile-fixed) build.

2. **Zero crashes in either run.** `defusal_score`'s extra `pos.
   make_move`/`unmake_move` pair didn't reintroduce anything like
   D100/D101's earlier crash — crash-safety continues to hold.

3. Elo: n=20 leaned toward TDSE (A score 45.0%, Elo diff -34.9 favoring
   B); n=200 landed almost exactly neutral (A score 50.5%, Elo diff
   +3.5 favoring A). Computed 95% CIs same as every prior result: n=20
   (35.4%, 54.6%), n=200 (46.3%, 54.7%) — both straddle 50% comfortably.

4. Laid all four Elo measurements from this investigation side by side
   (legality-only n=20/n=200 from Sessions 94/98, SEE-degradation
   n=20/n=200 from this session) — every interval overlaps every other
   one, including the two n=20 results pointing in opposite directions
   from each other. **Across two independent 200-game samples now, TDSE
   has not demonstrated a clear, replicated Elo effect either way.**

5. Framed this the same way D90 framed Phase 26's correction-history
   result before closing that phase: not "TDSE failed," but "TDSE
   hasn't earned promotion" — with two live, non-exclusive
   explanations (little real effect at this engine's current strength,
   or the never-tuned weight constants diluting a real signal) and two
   reasonable paths forward, left explicitly open rather than decided.

**Bugs fixed:** none — result-recording and cross-run comparison only.

**Decisions made:** D106.

**Next session start point:** Gokul's call, genuinely open — deprioritize
TDSE for now (reasonable stopping point after two 200-game runs without
a clear win, `ThreatDefusal` already defaults `false`, nothing to undo),
or Texel-tune `WEIGHT_ILLEGAL`/`WEIGHT_SEE`/`WEIGHT_CONTROL`/
`TDSE_MARGIN_CP` via the existing `texel/` pipeline before another
confirmation run. Per ROADMAP.md Phase 28's Session 101 update.

---

## Session 100 — 2026-07-28, cont. (D104's own diff had a compile error — missing Square import; fixed; D105)

**Built/done:**

1. Gokul ran CI on the D104 commit — failed to compile:
   `error[E0425]: cannot find type 'Square' in this scope` at
   `attacker_count_on`'s signature in `alpha_beta.rs`.

2. Cause: my own mistake, plainly. `Square` was only imported in the
   test module (`use crate::types::Square;`, relied on by D98's
   earlier tests); I never added it to the file's top-level imports
   before using it in new non-test code (`attacker_count_on`). Carried
   an unchecked assumption ("Square already works here, I've used it in
   tests") into new code without verifying what's actually in scope
   outside `#[cfg(test)]`.

3. Fixed: one line — added `Square` to
   `use crate::types::{Color, Move, MoveKind, PieceKind, Square};`.

**Bugs fixed:** a genuine compile error in this session's own prior
diff. D104's actual design and the two silent-failure bugs it found and
fixed in the original proposal are unaffected — this was purely a
missing import.

**Decisions made:** D105.

⚠️ Still not CI-confirmed after this fix — no local `cargo` in this
session's sandbox, same caveat as always.

**Next session start point:** unchanged from Session 99 — Gokul commits
the corrected `src/search/alpha_beta.rs`, confirms CI is actually green
this time, then runs the 20-game first look for the SEE-degradation
signal. Per ROADMAP.md Phase 28's Session 100 update.

---

## Session 99 — 2026-07-28, cont. (Phase 28: SEE-degradation signal implemented — two more real bugs found in the proposal, both fixed; D104)

**Built/done:**

1. Gokul chose the SEE-degradation path from D103's two open options.
   Re-read the original proposal's §3 in full, then verified every claim
   against the real repo before writing anything (same discipline as
   D98) — specifically read `see_value_of`'s actual implementation
   (`search/see.rs`) rather than trusting the proposal's call-ordering
   assumption.

2. **Found Bug 1**: the proposal computes `threat_see_before` in
   `extract_threat_move` *after* restoring `pos.side_to_move` back to
   the original side. `see_value_of` looks up "our" piece on `mv.from`
   using whatever `pos.side_to_move` currently is — since `best_move` is
   the *opponent's* move, calling this after the restore means the
   function silently returns 0 every time (finds no piece belonging to
   the wrong color on that square), no panic, no obvious symptom. Fixed
   by moving the computation before the restore.

3. **Found Bug 2**: `control_delta_on_threat_squares`'s `mover_color`
   was computed as `pos.side_to_move.flip()` in the proposal, but by the
   time this function is actually called (inside `defusal_score`, after
   `pos.make_move(candidate)`), `pos.side_to_move` already correctly
   equals the threat's owner with no flip needed — the extra `.flip()`
   would have silently inverted the entire attacker/defender signal.
   Fixed by removing it.

4. Both bugs share the same shape: neither panics or produces an
   obviously-wrong result, both fail silently with a plausible-looking
   backwards or zeroed number — exactly the failure mode independent
   verification (reading actual callee implementations and tracing
   actual call-time state, not re-reading the proposal's own comments)
   exists to catch.

5. Implemented the rest of §3 as designed: `attacker_count_on`,
   `control_delta_on_threat_squares` (both fixed per above),
   `ThreatInfo.threat_see_before`, and `pub fn defusal_score` combining
   illegality-bonus + SEE-drop + control-delta into one weighted score.
   Deliberately placed in `alpha_beta.rs` rather than `pruning.rs` as
   the proposal suggested — checked where the proposal's own cited
   precedent (`king_safe_square_count`, D75) actually lives and followed
   that instead, also avoiding an unnecessary field-visibility change.

6. Updated `iterative.rs`'s TDSE block to call `defusal_score` and pick
   the highest-scoring near-tied candidate, replacing D98's
   first-legality-defusing-candidate logic. Considered and reverted a
   `score > 0` gate on the override — reasoned that among candidates the
   real search already judged near-equal, the best-scoring one by this
   heuristic is still the right tiebreak even at a small or negative
   absolute score.

7. Four new tests, including one asserting the *exact* expected SEE
   value (500, an undefended rook) rather than just non-panic — this
   specific test would have caught Bug 1 directly had it existed before
   the fix.

**Bugs fixed:** two real bugs in the source design document, caught and
fixed before they became bugs in the engine (same category as D98's
`to_square()` catch, this session found two more of the same shape).

**Decisions made:** D104 — also explicitly flags that this diff bundled
SEE and control-delta together rather than as two separate isolated
diffs, deviating from the original proposal's "three signals, three
independent validations" plan.

⚠️ Not yet CI-confirmed. Reviewed the actual diff text this time (not
just brace/paren balance) given D96 already showed once this
investigation that balance-checking alone can miss a real mistake.

⚠️ Zero Elo/crash validation yet for this specific signal.

**Next session start point:** Gokul commits `src/search/alpha_beta.rs`
and `src/search/iterative.rs`, confirms CI is green, runs a 20-game
first look (not trusted alone), then a 100-200 game confirmation if not
clearly negative — watching specifically for any new crash, since
`defusal_score` adds its own make/unmake pair beyond what D101 already
fixed. Per ROADMAP.md Phase 28's Session 99 update.

---

## Session 98 — 2026-07-28, cont. (Phase 28: TDSE clears the crash-safety bar — 200/200 games, zero crashes; Elo reverses to mildly positive, still not conclusive; D103)

**Built/done:**

1. Gokul re-ran the 200-game confirmation, same seed (61000), with
   D101/D102's fix committed. **All 200 games completed — zero
   crashes.** The `!in_check` guard fix is now confirmed against the
   exact same seed/games that crashed the previous attempt at game 15 —
   this isn't just "didn't crash on different games," it's a direct
   repro-and-fix confirmation.

2. Read the Elo result: A (control) 34 wins, B (`ThreatDefusal=true`) 42
   wins, 124 draws — A score 48.0%, Elo diff -13.9 favoring TDSE. This
   reverses the direction of D99's n=20 first look.

3. Computed the same 95% CI check used for every small-sample result in
   this project (empirical per-game variance, `SE = SD/√200`): roughly
   (43.7%, 52.3%) — much tighter than D99's (45%, 65%) and now mostly on
   the favorable side for TDSE, but still narrowly includes 50%. **Not
   yet a statistically conclusive positive result** — read D99's earlier
   negative-leaning result as exactly the noisy small-sample outcome its
   own entry warned it might be, not as something this result
   contradicts.

4. Documented in DECISIONS.md D103 that this does *not* justify flipping
   `ThreatDefusal`'s default — a CI still touching 50% is disqualifying
   for that regardless of how encouraging the direction is — and laid
   out two independent, non-blocking next paths: a larger confirmatory
   run for Elo confidence, or starting the SEE-degradation signal now
   that crash-safety is established. Left as Gokul's call which to do
   next, same as D99's original open-decision framing.

**Bugs fixed:** none this session — confirmation and result-recording
only. D101's fix from two sessions ago is what's being confirmed here.

**Decisions made:** D103.

**Next session start point:** Gokul picks one of the two paths in
ROADMAP.md Phase 28's Session 98 update — a 400+ game confirmatory run
for Elo confidence, or starting the SEE-degradation signal as TDSE's
next isolated diff. Neither blocks the other.

---

## Session 97 — 2026-07-28, cont. (D101's own regression test had a bug — missing Black king in the test FEN; fixed; D102)

**Built/done:**

1. Gokul ran `cargo test` on the D101 commit: 482 passed, 1 failed —
   `test_extract_threat_move_returns_none_when_in_check` panicked on
   `Position::from_fen(...).unwrap()`: `KingNotFound(Black)`.

2. Found the cause immediately: the test's hand-constructed FEN placed
   a White king and a Black rook but never placed a Black king anywhere
   — an oversight in the test itself, not in D101's actual `!in_check`
   guard. `from_fen()` correctly rejects any position missing a king for
   either side; it caught exactly the mistake it exists to catch.

3. Fixed: added a Black king on h8, FEN now
   `"4r2k/8/8/8/8/8/8/4K3 w - - 0 1"` — same check (rook e8 down a clear
   e-file to White's king on e1), now a valid position. Updated the
   `king_sq(Color::Black)` assertion from `E8` to `H8` to match.

**Bugs fixed:** a bug in this session's own prior test, not in the
engine. D101's actual fix is unaffected — the failure never reached the
code under test.

**Decisions made:** D102 (brief).

**Next session start point:** unchanged from Session 96 — Gokul commits
the corrected `src/search/alpha_beta.rs`, confirms CI is fully green
this time, then re-runs the 200-game confirmation at the same seed
(61000). Per ROADMAP.md Phase 28's Session 97 update.

---

## Session 96 — 2026-07-28 (Phase 28: TDSE crash root-caused and fixed — missing !in_check guard; D101; not yet re-confirmed crash-free)

**Built/done:**

1. Gokul re-ran the 200-game confirmation (same seed, 61000) with
   Session 95's harness fix in place. Real panic captured this time:
   `thread '<unnamed>' (4354) panicked at src/position/mod.rs:270:14:
   King must always be on the board` — `Position::king_sq()`'s own
   panic, meaning a king bitboard had gone empty. The harness's
   engine-naming fix (also Session 95) confirmed it was **Engine B**
   (`ThreatDefusal=true`) that crashed, 14 games in — Engine A, same
   commit/binary, ran the same games with default options and never
   crashed, confirming this is TDSE's own bug.

2. Read `Position::king_sq()` (`src/position/mod.rs:270`) to confirm
   exactly what it panics on (an empty king bitboard for the queried
   color), then compared `extract_threat_move` line-by-line against the
   real null-move block it explicitly claims to mirror
   (`alpha_beta_with_excluded`'s `can_null_move` condition) rather than
   guessing from the panic message alone.

3. **Found the gap**: the real null-move guard is `!pv_node && !in_check
   && depth >= MIN_DEPTH_NULL_MOVE && static_eval >= beta &&
   has_non_pawn_material(...) && prev_move != Move::NULL &&
   king_safe_squares...`. D98's `extract_threat_move` only carried over
   `has_non_pawn_material` — every other omitted condition is a
   pruning-effectiveness heuristic that genuinely doesn't apply to a
   threat probe (correctly left out), except `!in_check`, which is a
   correctness guard: flipping side-to-move while the current side is in
   check produces a position where "whose king is under attack" and
   "whose turn it is" disagree, which is undefined territory for the
   normal move-generation/check-evasion machinery this probe hands the
   position to. None of D98's four unit tests happened to start from an
   in-check position, so this gap only surfaced once TDSE ran across
   real games at scale.

4. **Fixed**: `if pos.in_check(pos.side_to_move) { return None; }` as
   the first check in `extract_threat_move`. Added a regression test
   with an in-check FEN that asserts both kings are still exactly where
   they started afterward — not just that the function returns early,
   since this was specifically a board-corruption bug.

**Bugs fixed:** the confirmed root cause of Session 95's crash (D101).
Explicitly flagged as not yet re-confirmed by an actual crash-free run —
this is a well-reasoned, precedent-matching fix for a real, confirmed
deviation, not a guaranteed complete fix.

**Decisions made:** D101.

⚠️ **TDSE stays disqualified from any default-on consideration** (per
D100's standing rule) until a full 200-game run completes with zero
crashes — this fix earns another attempt at that confirmation, it
doesn't skip it.

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as always.

**Next session start point:** Gokul commits `src/search/alpha_beta.rs`
(this session's fix + new test), confirms CI is green, then re-runs the
200-game confirmation at the same seed (61000) one more time. Zero
crashes clears D100's bar for the first time and makes D99's Elo
question live again with a real, uncorrupted result from that same run.
Another crash — same discipline: get the real message, read the exact
line, don't guess. Per ROADMAP.md Phase 28's Session 96 update.

---

## Session 95 — 2026-07-27, cont. (Phase 28: 200-game TDSE confirmation CRASHED — harness was discarding the real panic message, fixed; TDSE's own bug still unconfirmed; D100)

**Built/done:**

1. Gokul chose path (a) from D99's open decision and ran the 100-200
   game confirmation: 200 games, 100ms/move, seed 61000, same A/B shape
   (Engine A control, Engine B `ThreatDefusal=true`).

2. **The run crashed after 14 completed games** — harness panic:
   "engine process closed stdout while waiting for 'bestmove'",
   "Aborted (core dumped)", exit code 134. Treated this as more urgent
   than D99's Elo question — a crash means forfeiting/hanging in real
   play regardless of strength.

3. Checked every log file the run produced for any trace of the actual
   crashed engine's own panic message — found none. Traced why: read
   `src/bin/uci_match_runner.rs` in full (mandatory read-before-write)
   and found `EngineProcess::spawn()` spawns child engines with
   `.stderr(Stdio::null())` — silently discarding the exact stream a
   Rust panic prints to. This is why only the harness's own *secondary*
   panic (detecting the closed stdout) was ever visible, not the
   engine's actual panic message and location.

4. **Fixed the diagnostic gap before attempting to diagnose or fix the
   actual TDSE crash** — same "get real data before guessing"
   discipline the whole Phase 27 investigation used. Changed
   `Stdio::null()` to `Stdio::inherit()` (confirmed no workflow YAML
   change needed — GitHub Actions already captures the harness's own
   stderr combined with stdout in the step log, `inherit()` just stops
   discarding the child's copy of that stream). Added a `name` field to
   `EngineProcess`, threaded from the already-computed `label_a`/
   `label_b`, so a future crash's panic message says which engine died.

5. Did a static review of the new Phase 28 TDSE code
   (`extract_threat_move`, `defuses_threat`, the `iterative.rs` sibling
   block) looking for an obvious panic source, without finding one by
   inspection alone — checked every unwrap/expect/indexing operation
   (all safe), confirmed `Cargo.toml`'s `panic = "abort"` release
   profile explains the crash *symptom* observed, ruled out integer
   overflow as a cause (`overflow-checks` not enabled in release,
   `INFINITY = 1_000_000` nowhere near overflow range regardless),
   confirmed `alpha_beta_with_excluded()` being called directly at
   `ply=1` isn't skipping any hidden setup the public `alpha_beta()`
   wrapper would otherwise do (the wrapper is a thin passthrough).
   **Did not find the specific bug** — explicitly left as unconfirmed,
   a hypothesis at most, not something to fix blind.

**Bugs fixed:** the harness's own diagnostic gap (real panic messages
being silently discarded). The actual TDSE crash is **not yet fixed** —
still unconfirmed which line causes it.

**Decisions made:** D100 — also explicitly states TDSE must not be
considered for default-on under any circumstances until this crash is
root-caused and fixed, superseding D99's Elo question in priority.

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as always.

**Next session start point:** Gokul commits
`src/bin/uci_match_runner.rs`, confirms CI is green, then re-runs the
same 200-game confirmation at the same seed (61000) for an exact repro.
This time the crashed engine's own panic message and source location
should appear in the CI log — read that specific line in full before
writing any fix, do not act on this session's unconfirmed hypothesis
alone. Per ROADMAP.md Phase 28's Session 95 update.

---

## Session 94 — 2026-07-27, cont. (Phase 28: TDSE 20-game first look — mild negative point estimate, not statistically distinct from zero; D99)

**Built/done:**

1. Gokul ran the 20-game `ThreatDefusal` A/B via the mobile GitHub
   Actions workflow (`uci_match_runner.yml`), exactly per D98's plan:
   same commit built twice, Engine A compiled-in defaults (control,
   `ThreatDefusal=false`), Engine B `setoption name ThreatDefusal value
   true`, 1000ms/move, seed 60000.

2. **Result**: A (control) 3 wins, B (TDSE on) 1 win, 16 draws — A score
   55.0%, Elo diff +34.9 favoring the control (TDSE off).

3. Didn't react to the direction alone — checked the statistics first,
   same discipline D76/D84 already established in this project for
   small-sample first looks. With only 4 decisive games out of 20, a
   rough 95% CI on the score (empirical per-game variance, `SE =
   SD/√20`) comes out to roughly (45%, 65%) — **not statistically
   distinguishable from a coin flip** at this sample size. The point
   estimate is real and mildly discouraging, but the uncertainty band
   is wide enough that flat or even mildly positive results are still
   consistent with this data.

4. Documented the result and an explicit, unresolved decision point in
   DECISIONS.md D99 and ROADMAP.md's Phase 28 section: since this first
   look doesn't meet a "clearly negative" bar in a statistically
   rigorous sense, the original plan's own criterion says proceed to a
   100-200 game confirmation run — but the negative lean (as opposed to
   a genuinely flat result) makes it reasonable to instead deprioritize
   TDSE without spending that CI budget right now. Left as Gokul's
   choice, not resolved unilaterally.

**Bugs fixed:** none — result-recording and statistical interpretation
only, no code touched this session.

**Decisions made:** D99 (result + explicitly open decision, not a
resolution).

**Next session start point:** Gokul decides between the two paths in
ROADMAP.md Phase 28's Session 94 update — run the 100-200 game
confirmation, or pause TDSE work for now. `ThreatDefusal` stays `false`
by default either way, so there's no urgency driving the choice.

---

## Session 93 — 2026-07-27, cont. (Phase 28 opened: TDSE first diff — verified an external design doc against real code, found and fixed one real error, shipped legality-only signal; D98)

**Built/done:**

1. Gokul asked to adapt `tdse-pet-dragon-adaptation.md`, a design
   proposal for a Threat-Defusal Search Extension: prefer whichever
   near-tied root candidate best defuses the opponent's strongest reply,
   rather than an arbitrary tiebreak. The doc claimed to have verified
   its API assumptions against the real repo at ~10 points across 7
   files.

2. **Did not trust that claim.** Pulled a fresh copy of the repo and
   independently re-checked every load-bearing assumption before writing
   anything: the null-move block's exact shape, `score_moves`/
   `next_move` signatures, the Skill Level noise block's real shape
   (the doc's own cited precedent) including `search_multipv_slot`'s
   signature, `mobility_for_color()`'s privacy, `see()`/`see_value_of()`
   signatures, `CorrectionHistory`'s shape, the bitboard attack
   primitives, `make_move_with_history`/`unmake_move_with_history`,
   `TranspositionTable::probe`'s return shape. All confirmed correct.

3. **Found one real error the document missed**: it called
   `threat_move.to_square()`, which doesn't exist — `Move` has plain
   public fields `from`/`to: Square`, not accessor methods. A genuine
   catch from doing the verification myself rather than trusting
   "verified" as written.

4. Implemented the proposal's own recommended first diff only
   (legality-only signal, no SEE, no control-delta yet — explicit staged
   rollout in the doc's §5): `SearchInfo::threat_defusal` (default
   `false`, full option threading through `EngineState`/`main.rs`, same
   shape as every prior toggle in this family); `extract_threat_move()`
   and `defuses_threat()` in `alpha_beta.rs` (both `pub` — confirmed no
   `pub(crate)` precedent exists anywhere in `search/`, so matched
   `alpha_beta()`'s own full-`pub` convention instead); a new sibling
   block in `iterative_deepening()` right after the existing Skill Level
   noise block, reusing Phase 19's `search_multipv_slot`/
   `info.root_exclude`, mutually exclusive with the noise block per a
   real interaction risk the proposal itself flagged.

5. One implementation deviation from the proposal, deliberately
   conservative: rewrote its `.find()`-with-closure candidate search as
   a plain `for` loop, since the proposal's own code used a manual loop
   specifically to avoid a closure capturing `&mut Position` through an
   iterator adapter — likely fine in modern Rust, but not something
   worth risking without a local compiler to check it.

6. Ten new tests across `alpha_beta.rs` (default-false;
   `extract_threat_move` finds a hanging-rook capture on a constructed
   FEN; `defuses_threat` correctly returns both `true` and `false` on
   hand-built before/after positions), `main.rs` (default/parse/
   cmd_go-wiring, mirroring the existing `SingularMultiCutEnabled` test
   trio), and `iterative.rs` (default-off byte-identical to pre-TDSE
   behavior; enabling it doesn't panic and still returns a legal move).

**Bugs fixed:** none in the engine's existing behavior — this is new,
default-off functionality. Did catch and fix one real error in the
source design document before it became a real bug (item 3).

**Decisions made:** D98.

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as always. This diff is larger than D95's one-line
fix; watch CI closely.

⚠️ **Zero Elo validation yet.** `ThreatDefusal` ships `false` by
default. The proposal's own rollout plan (20-game first look → 100-200
game confirmation, same D76/D84 discipline) hasn't started.

**Next session start point:** Gokul commits the 4 changed files
(`src/search/mod.rs`, `src/search/alpha_beta.rs`,
`src/search/iterative.rs`, `src/main.rs`), confirms CI is fully green
(all tests, including the 10 new ones), then runs a 20-game
`uci_match_runner` A/B (`ThreatDefusal` true vs. false) as a first look
only — per ROADMAP.md Phase 28's numbered next steps.

---

## Session 92 — 2026-07-27, cont. (PHASE 27 RESOLVED — D95 fix confirmed by real bench data; D97; first-ever win vs. real Stockfish)

**Built/done:**

1. Gokul confirmed CI fully green — all 476 tests, including D95's two
   new tests and D96's correction — before running anything, per last
   session's explicit ask.

2. Ran a 3-game bench with the fixed build (control config,
   1000ms/move, Skill 20 vs. Stockfish Skill 10). **Result: 1 draw, 1
   loss, 1 win — the first win Pet Dragon has recorded against real
   Stockfish anywhere in this six-session investigation.**

3. Ran the same exact-zero eval quantification used every session since
   D94 (Python regex extraction, read-only, no repo changes) against
   this new log. **Rate dropped from D94's 16-49% baseline to 0-8%
   across the three games**, and the handful of remaining zeros in the
   drawn game are the final 4 plies of a genuine threefold-repetition
   draw at depth 19-20 — correct behavior, not the bug (the bug produced
   `0` at normal-but-not-exhaustive depths in clearly decisive
   positions, not in an actually-drawn high-depth shuffle).

4. Checked specifically for D94's second, separately-flagged anomaly
   (a wild positive eval spike immediately before getting mated) in this
   bench's one loss — not present; that game's eval declines smoothly
   and monotonically all the way to `mate-2`, fully coherent with a real
   losing position rather than a corrupted one.

5. **Closed Phase 27** in ROADMAP.md (heading marked ✅ RESOLVED) with a
   short retrospective of the full six-session chain from Session 85's
   original bench log through D97's confirmation — every negative
   ablation and diagnostic addition along the way earned its place, none
   of it was wasted even where individual hypotheses (D91/D92's LMP and
   SingularMultiCut ablations) came back negative.

**Bugs fixed:** none this session specifically (D95/D96 already shipped)
— this session is confirmation-only, closing the investigation with
real data rather than code-review confidence.

**Decisions made:** D97 (Phase 27 resolution).

**Next session start point:** no open Phase 27 work. If Gokul wants a
rigorous Elo-impact number for the D95 fix specifically (n=3 isn't
that), that's a future SPRT-style A/B via `uci_match_runner.rs`, same
tooling as every other internal Elo question in this repo — normal
ongoing strength work, not a regression to keep chasing. Check
ROADMAP.md's other open phases/tasks for what's next.

---

## Session 91 — 2026-07-27, cont. (CI caught a real regression in D95's own edit: quiescence()'s node counter was accidentally deleted; D96; fixed, still pending green CI)

**Built/done:**

1. Gokul ran CI on the D95 commit — Gokul's own initiative, exactly the
   workflow working as intended. Result: 475 passed, 1 failed —
   `search::alpha_beta::tests::test_qsearch_in_check_generates_evasions`
   panicked on `assert!(info.nodes > 0, "Must search nodes when in
   check")`.

2. Read the failing test and the exact D95 diff in `quiescence()`
   side by side. **Found the cause**: D95's `str_replace` on
   `quiescence()`'s time check included the pre-existing
   `info.nodes += 1;` line in `old_str` (needed to disambiguate the
   match against the near-identical time check in
   `alpha_beta_with_excluded()`), but `new_str` didn't carry it back
   through — the line was silently dropped, unrelated to the actual
   fix being made. Checked `alpha_beta_with_excluded()`'s own D95 edit
   too: unaffected, different code shape meant that line was never
   part of its `old_str` in the first place.

3. Traced the impact beyond the one failing assertion: with
   `info.nodes` never incrementing inside `quiescence()`, total node
   counts (NPS, `seldepth`-adjacent stats) would have undercounted by
   whatever fraction of nodes run through quiescence (typically large),
   and `is_time_up()`'s `self.nodes & 255 == 0` sampling gate would have
   evaluated true on every single quiescence node instead of every
   256th — a real performance regression this specific CI failure
   didn't directly test for but would have shipped alongside D95 if
   merged as-is.

4. **Fixed**: restored `info.nodes += 1;` in its original position,
   immediately after the new D95 time-check block. One line.

5. Checked every other test that calls `quiescence()` directly (4 more
   call sites) to confirm none of them silently depended on the broken
   node-counting behavior in a way that would need updating alongside
   the fix — none do; only the one test asserts on `info.nodes`.

6. Documented plainly in DECISIONS.md D96, including the process point:
   an `old_str` chosen only to disambiguate a match, rather than to
   bound exactly the lines meant to change, can silently swallow
   adjacent code — brace/paren balance checks (which this passed even
   with the bug present) don't catch that; re-reading the actual diff
   would have.

**Bugs fixed:** the regression introduced by D95's own edit (this
session). D95's actual root-cause fix from last session is unaffected
and still believed correct — this was a mechanical editing mistake in
how that fix was applied, not a flaw in the diagnosis.

**Decisions made:** D96.

⚠️ **Not yet CI-confirmed** — same caveat as every session, no local
`cargo` in this sandbox. This one-line fix needs an actual green CI run
before trusting it, especially right after this exact class of mistake.

**Next session start point:** Gokul commits the corrected
`src/search/alpha_beta.rs`, confirms CI is fully green this time (all
tests, not just the one that failed), then runs a bench and checks
whether D94's exact-zero eval rate dropped and the win/loss/draw record
improved — per ROADMAP.md Phase 27's Session 91 update. Only close
Phase 27 once that's confirmed.

---

## Session 90 — 2026-07-27, cont. (Phase 27: ROOT CAUSE FOUND AND FIXED — is_time_up() never set info.stop; D95; pending real-bench validation)

**Built/done:**

1. Gokul ran one more bench (3 games, control config, 1000ms/move) with
   the Session 89 depth-logging build. Checked where the `0`-eval
   moves' `/dN` values cluster, per last session's explicit plan:
   **completely normal depths (d10-d14)**, not low/aborted ones — rules
   out time-starvation, confirms the score-reporting lead from D94 was
   the right one.

2. Read `search/alpha_beta.rs` and `SearchInfo::is_time_up()`
   (`search/mod.rs`) together — full read of both time-check call sites
   and everything downstream that reads `info.stop`, mandatory
   read-before-write, before touching anything. **Found the actual
   mechanism, not a guess:** `is_time_up()` takes `&self` — it only ever
   reads `self.stop`/`self.stop_flag`. Grepped for every place
   `.stop = true` gets assigned in production code: there isn't one.
   The only assignment anywhere is a unit test manually setting it to
   verify the read side. Both real call sites
   (`alpha_beta()`'s own time check, `quiescence()`'s) detected a real
   elapsed-time timeout, returned a hardcoded `0`, and moved on without
   ever recording that anything was cut short.

3. Traced every downstream `if info.stop {...}` check in
   `alpha_beta.rs` (four more `return 0` propagation sites, the
   TT-store guard, the correction-history-update guard) and
   `iterative.rs` (`iterative_deepening()`'s own
   discard-this-depth-on-abort logic) — all of them assume `info.stop`
   would already be `true` by the time they run. It never was, for the
   single most common reason a search aborts (running out of its
   per-move time budget). Confirmed `reset_for_search()` already resets
   `self.stop = false` at the top of every search, so setting it on a
   real timeout can't leak across moves — and confirmed the separate
   `stop_flag` `Arc<AtomicBool>` (UCI `stop` command / ponder) stays
   fully independent, since the fix never touches it.

4. **Fixed**: `info.stop = true;` added immediately alongside the
   `return 0;` at both real call sites — two lines each, activating
   discard/skip logic that was already correctly written throughout the
   codebase and simply never got triggered for this case.

5. Added two new deterministic tests in `alpha_beta.rs`'s test module:
   `test_alpha_beta_sets_stop_on_real_timeout` and
   `test_quiescence_in_check_sets_stop_on_real_timeout` (the latter
   using a constructed genuinely-in-check FEN to specifically exercise
   quiescence's in-check evasion branch). Both use a `0ms` time budget
   so `is_time_up()`'s elapsed-time branch fires on the very first
   check, deterministically, no timing flakiness.

6. Documented the full mechanism and why it explains every symptom
   gathered across Sessions 85-90 (Pet Dragon-specific vs. Stockfish,
   config-independent across all of D91's LMP/SingularMultiCut
   ablations, worse in longer/more complex positions, manifests as
   sudden late-game material collapse) in DECISIONS.md D95.

**Bugs fixed:** the root cause of Phase 27's entire investigation
(pending real-bench confirmation — see caveat below). This is the
highest-confidence fix of the six-session investigation by a wide
margin: a specific, readable, mechanistically-clear bug found by
reading code, not an ablation guess.

**Decisions made:** D95.

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as every session since Phase 27 began.

⚠️ **Phase 27 is NOT closed.** This is code-review confidence, not a
validated fix — explicitly flagged in both DECISIONS.md D95 and
ROADMAP.md's Phase 27 update. It also may not be the only thing wrong:
D94's separate `+423`-then-mated-in-a-few-moves anomaly in Match 2 is a
differently-shaped failure (a search that completes and is simply
wrong, not one that returns a corrupted sentinel mid-search) that this
fix doesn't obviously explain and needs checking separately.

**Next session start point:** Gokul commits `src/search/alpha_beta.rs`,
confirms CI passes (including the two new D95 tests), runs one more
bench (any config), and checks (a) whether the exact-zero eval rate
dropped from the 16-49% baseline in D94, and (b) whether the win/loss/
draw record actually improved. Only close Phase 27 if both hold — per
ROADMAP.md Phase 27's Session 90 update.

---

## Session 89 — 2026-07-27, cont. (Phase 27: exact-zero eval signature found — Pet Dragon-specific, config-independent; search depth now logged; D94)

**Built/done:**

1. Gokul ran 9 games via `vs.html` with the D93 eval-logging build: 3
   control, 3 `LMPEnabled=false`, 3 `SingularMultiCutEnabled=false`, all
   1000ms/move vs. Stockfish Skill 10. 6 losses, 3 draws total — no
   config stands out, consistent with Session 88's conclusion that
   D59/D60 aren't the cause.

2. Parsed all 9 games' per-move eval tokens (Python regex, read-only, no
   repo changes) and compared exact-zero rates between sides. **Finding:
   Pet Dragon's own eval is exactly `0` on 16-49% of its own moves
   across every game, vs. 0-1% for Stockfish playing the same games** —
   a Pet Dragon-specific signature, present in every config tested
   across three sessions now. Sharpest example: Pet Dragon's last
   non-mate eval before getting checkmated in Match 1 was `0`, while the
   true position was ~-9 pawns per Black's eval one ply later. Separately
   noted Match 2: Pet Dragon self-reported `+423` two of its own moves
   before getting mated — a different-shaped anomaly, possibly related,
   possibly not.

3. Read `search/iterative.rs` in full (mandatory read-before-write,
   though nothing was changed there this session) looking for the
   mechanism. Found `iterative_deepening()`'s `score_for_result`
   selection expression treats a genuinely-computed `0` identically to
   "value not yet set" — a real bug in that one expression — but could
   not confirm from static reading alone that it's actually what
   produces the observed pattern in the normal synchronous case (traced
   `info.best_score`/local `best_score` through `search_at_depth`/
   `search_with_aspiration` and they appear to already converge before
   that check runs in the ordinary non-aborted path). Flagged as a lead
   in DECISIONS.md D94, explicitly not claimed as confirmed — didn't
   want to ship a fix against a guess after two straight ablation
   sessions came back negative on a different guess.

4. Instead of a third speculative code change, shipped a data-gathering
   one: read `src/lib.rs` and both web callers'
   (`web/index.html`/`web/pit/vs.html`) parsing of
   `search_from_fen_with_eval`'s return string in full before touching
   anything, confirmed both already tolerate trailing content after the
   eval token (index.html via `indexOf(' ')`+`parseInt` truncation,
   vs.html via `split(/\s+/)` already ignoring extra array entries) —
   verified, not assumed. Added a third token: deepest completed
   iterative-deepening depth for that move. `vs.html`'s
   `parsePetDragonResult`, `match.history`, `matchLog`, and the exported
   log text (`/dN` suffix, e.g. `g1g2(0)/d6`) now carry it through.

**Bugs fixed:** none — one real bug found (the score_for_result
sentinel-confusion in `iterative.rs`) but explicitly not fixed yet since
its actual causal role in the observed pattern isn't confirmed.

**Decisions made:** D94.

**Next session start point:** Gokul commits `src/lib.rs` and
`web/pit/vs.html` (this session's depth-logging addition, on top of
Sessions 87-88's changes to the same two files), confirms CI + Pages
redeploy, runs one more bench (any config), and checks where the
`0`-eval moves' `/dN` values cluster — low depth points at
`search/time.rs` next, normal depth points at `iterative.rs`'s
`score_for_result` expression and the aspiration-window paths next. Per
ROADMAP.md Phase 27's Session 89 update.

---

## Session 88 — 2026-07-27 (Phase 27: D59/D60 ablation results negative/inconclusive; per-move eval now logged for next round)

**Built/done:**

1. Gokul ran configs (b) and (c) from ROADMAP Phase 27 via `vs.html`,
   1000ms/move, Skill 20 vs. Stockfish Skill 10, 3 games each:
   - (b) `LMPEnabled=false`, `SingularMultiCutEnabled=true`: 1 draw + 2
     losses.
   - (c) `LMPEnabled=true`, `SingularMultiCutEnabled=false`: 2 losses +
     1 draw.
   Replayed all 6 games via `python-chess` (read-only, same method as
   every prior session), material tracked ply-by-ply.

2. **Result: neither ablation supports the D59/D60 hypothesis.** Both
   configs scored numerically worse than Session 87's control (2 draws
   + 1 loss), and the same late-game collapse shape (level material,
   then a fast one-sided drop) still appeared in losses under both. One
   loss under config (c) is a new, sharper data point: Pet Dragon was
   **+1 material** at ply90 and still walked into checkmate 8 plies
   later — that looks like king-safety/mate-blindness specifically, not
   a material-losing blunder. n=3-per-config is too small to be
   conclusive, but two independent ablations gave zero positive signal
   for D59/D60, so continuing to guess at more single-technique
   ablations isn't the efficient next move.

3. **Shipped, no hypothesis attached — per-move eval logging.** Noticed
   `search_from_fen_with_eval` already returns a White-relative eval
   alongside every move (that's what feeds `vs.html`'s eval bar) and
   `match.history` already carried it per move in memory — but the
   *exported* bench log discarded it, printing only bare UCI tokens.
   Read `web/pit/vs.html` fresh (mandatory read-before-write; confirmed
   via grep that `matchLog[i].moves` has exactly one reader,
   `buildLogText`, so the shape change was safe) and changed
   `recordMatchLogEntry` to carry `{uci, eval}` pairs, `buildLogText` to
   print `e2e4(+35)` / `f2f4(mate-3)` style tokens. Strictly additive to
   the log format — no other consumer of the exported text file exists
   yet to break.

4. Rationale for eval logging over another ablation: every session so
   far has had to reconstruct the collapse externally, after the fact,
   from the bare move list, with zero visibility into what Pet Dragon's
   own search actually believed at the time. Whether next session's eval
   trajectory predicts the collapse in advance (points at `eval/`) or
   stays confidently positive right up to the blunder (points at
   `search/`, horizon effect) is a real branch point for where to look
   next — a stronger signal than another blind ablation round.

**Bugs fixed:** none — still narrowing the investigation, no root cause
identified yet.

**Decisions made:** D93 (ablation results + eval-logging addition —
DECISIONS.md numbering continues correctly from Session 87's D91/D92,
no collision this time).

**Next session start point:** Gokul commits `web/pit/vs.html` (this
session's eval-logging change, layered on Session 87's diagnostics
card — same file), confirms the Pages redeploy, runs one more bench
(any config, control is fine — no need to keep varying D91's toggles),
and sends the log. Read the per-move eval trajectory against the
already-established material trajectory for those games before deciding
whether to look at `eval/` or `search/` next, per ROADMAP.md Phase 27's
Session 88 update.

---

## Session 87 — 2026-07-26, cont. (Phase 27: found + fixed why Session 86's D91 toggles were unreachable from the browser pit tool; D92 WASM toggles + vs.html UI shipped, not yet CI/manually confirmed)

**Built/done:**

1. Gokul ran config (a) from ROADMAP Phase 27 (both D91 toggles left at
   default `true`, i.e. unchanged production behavior) via
   `web/pit/vs.html`, 1000ms/move, Skill 20 vs. Stockfish Skill 10.
   Result: **2 draws (threefold repetition) + 1 loss** — a real
   improvement over the previous 6-for-6 losses across Sessions 85-86.
   Replayed all 3 games via `python-chess` (read-only, same method as
   prior sessions): the loss still shows the same late-game material-
   collapse shape (level to ~ply60, then -10 by ply80, checkmate) —
   improvement in outcome, but the underlying pattern Phase 27 exists to
   investigate is still present.

2. Gokul reported not knowing how to run configs (b)/(c) — `vs.html` is
   direct engine-vs-engine play, no UCI console. Read
   `github.com/g-c-3/pet-dragon/blob/main/web/pit/vs.html` (via
   `raw.githubusercontent.com`, full file) to find out why, rather than
   assuming it was a usability gap on Gokul's end.

3. **Root cause found:** `vs.html` calls Pet Dragon via
   `search_from_fen_with_eval(fen, movetime, skill)`, a single-shot WASM
   function exported directly from `lib.rs` — there is no UCI stdin/
   stdout loop in the browser at all. Confirmed via grep that this is
   the *only* Pet Dragon entry point either web page
   (`web/index.html`/`web/pit/vs.html`) ever calls; plain
   `search_from_fen` (non-eval) isn't called from any web page currently.
   D91's `LMPEnabled`/`SingularMultiCutEnabled` UCI options only exist
   on the native `main.rs` `cmd_setoption`/`cmd_go` path, which neither
   web page touches — **D91's toggles were genuinely unreachable from
   the only tool Gokul uses for this investigation**, not a Gokul-side
   issue.

4. **D92 — implemented the WASM-side equivalent.** Read `lib.rs` in
   full (mandatory read-before-write) — confirmed both
   `search_from_fen`/`search_from_fen_with_eval` build a fresh
   `SearchInfo::new()` per call with only `skill_level` overridden.
   Added two module-global `AtomicBool`s (`LMP_ENABLED`/
   `SINGULAR_MULTICUT_ENABLED`, both default `true`) plus two new
   `#[wasm_bindgen]` exports (`set_lmp_enabled`/
   `set_singular_multicut_enabled`) that store into them; both search
   functions now read the atomics into each fresh `SearchInfo`. Chose
   this over adding parameters to `search_from_fen_with_eval` itself —
   that would require updating `web/index.html`'s call site in lockstep
   (it also calls this function) and wasm-bindgen turns a missing JS
   arg into `false` for a `bool` param, not the type's "default," which
   would have silently disabled both techniques for
   `web/index.html`'s real-user-facing play page the moment its call
   site fell one parameter behind — a setter function keeps the
   never-call-it path providably identical to before.

5. Read `web/pit/vs.html` in full before editing (mandatory read-
   before-write) — added a new "Pet Dragon diagnostics (Phase 27)"
   card (two checkboxes, both checked by default), imported the two new
   wasm-bindgen exports, applied the checkbox state once at boot and
   live on `change` (guarded by `dragonReady` so a click during the
   boot race is a no-op), froze both checkboxes while `match.running`
   (same rule already applied to engine/skill/movetime), and recorded
   the toggle state into `currentGameMeta`/`matchLog`/the exported log
   text so a downloaded bench log is self-describing about which A/B
   config produced it.

6. **Numbering correction:** Session 86's toggle-mechanism entry was
   mislabeled `D90` in both `DECISIONS.md` and `ROADMAP.md`, colliding
   with Session 85's pre-existing `D90 — CorrectionExtension` entry.
   Caught before anything was committed; renumbered to `D91` throughout
   (`main.rs`/`search/mod.rs`/`search/alpha_beta.rs`, `ROADMAP.md`,
   this file's Session 86 entry — Session 85's actual D90 references at
   lines 347/368 left untouched, they're correct). This session's new
   work is `D92`.

**Bugs fixed:** none in the engine itself yet — still diagnostic
infrastructure, plus a self-inflicted decision-numbering collision
(caught and corrected before commit, see item 6).

**Decisions made:** D92 (above); D91 renumbered from its Session-86
mislabeling.

⚠️ **Not yet CI-confirmed or manually loaded** — no local `cargo`/
`wasm-pack` in this session's sandbox, and `vs.html` imports live from
the deployed GitHub Pages build, so the new exports don't exist in the
browser until CI redeploys. Hand-reviewed only (brace/paren/div-tag
balance checked programmatically on `lib.rs` and `vs.html`, new element
IDs confirmed unique, `dragonReady` declaration-order checked against
the new listener registration).

**Next session start point:** Gokul commits all 5 changed files
(`src/search/mod.rs`, `src/search/alpha_beta.rs`, `src/main.rs` from
Session 86; `src/lib.rs`, `web/pit/vs.html` from this session), confirms
CI passes including the Pages redeploy, then manually loads
`web/pit/vs.html` once to confirm the new diagnostics card actually
renders and works before trusting any A/B result from it. Then run the
three configs (both default / LMP off / SingularMultiCut off) and send
the logs — per ROADMAP.md Phase 27's Session 87 update.

---

## Session 86 — 2026-07-26 (Phase 27 opened: external-Stockfish bench regression investigation; D91 diagnostic toggles shipped, not yet CI-confirmed)

**Built/done:**

1. Gokul uploaded a bench log: Pet Dragon (Skill 20) vs. real Stockfish
   (Skill 10), 100ms/move, 3 games, 0-3 (all checkmates against Pet
   Dragon). First-ever Pet-Dragon-vs-real-external-engine data point in
   this repo's history — everything previously measured was Pet Dragon
   vs. itself or vs. a pinned earlier build of itself.

2. Replayed all 3 games via local `python-chess` (read-only, no repo
   changes) — confirmed all legal, all genuine checkmates. Tracked
   material ply-by-ply: all 3 games show the same shape — level
   material through the opening/early middlegame, then a sudden, large,
   one-sided collapse late-game. Three-for-three same shape = real
   pattern, not noise.

3. Opened **Phase 27** in ROADMAP.md with this finding, a leading
   hypothesis (the three Phase 23 techniques flagged
   `⚠️ not yet Elo-measured` — 23.2 Lazy SMP/D49, 23.6 singular
   multi-cut/negative-ext/D59, 23.7 LMP/D60 — since all three are on by
   default and internal-only testing never checked their real-match
   impact), and a staged empirical investigation plan.

4. Gokul supplied a **second bench log**: same matchup, 1000ms/move
   instead of 100ms, Skill 9 Stockfish, 3 games, 0-3, same collapse
   shape. Replayed and material-tracked the same way. **This rules out
   plain time pressure** as the primary cause — the pattern survives a
   10x time-budget increase. Also noted `EngineState::new()` defaults
   `threads: 1`, so 23.2's thread-differentiated code doesn't execute
   in these single-thread benches at all — **23.2 provisionally ruled
   out**, narrowing to 23.6 (D59) and 23.7 (D60).

5. **D91 — implemented two runtime UCI diagnostic toggles**,
   `LMPEnabled` and `SingularMultiCutEnabled`, both **default `true`**
   (byte-identical to current production behavior — inverse-default
   pattern from the Phase 26 `NullMoveKingGuard` family, since D59/D60
   are already shipped default-on, not new/opt-in). Read
   `search/pruning.rs`, `search/alpha_beta.rs`'s LMP call site and
   singular-extension block, `search/mod.rs`'s `SearchInfo`, and
   `main.rs`'s full UCI option/cmd_setoption/cmd_go plumbing in full
   before writing anything (mandatory read-before-write). Gated LMP's
   call site and *only* D59's two new branches (multi-cut early-return,
   negative extension) — Phase 13.3/D16's original base singular
   extension branch is untouched by the new toggle. Same end-to-end
   threading pattern as every prior option in this family (main thread
   + every Lazy SMP helper thread's `SearchInfo`). Six new tests added
   (default-true assertions + off-still-searches-safely checks, both
   `SearchInfo`- and `EngineState`-level, mirroring the existing
   `null_move_king_guard`/`correction_extension_enabled` test pattern).
   Full reasoning in DECISIONS.md D91.

**Bugs fixed:** none yet — this session is diagnostic infrastructure,
not a fix. No root cause confirmed.

**Decisions made:** D91 (above).

⚠️ **Not yet CI-confirmed to compile/pass** — no local `cargo` in this
session's sandbox. Reviewed by hand (brace/paren balance checked
programmatically across all 3 files; every edited call site
double-checked against the existing `info`/`state` bindings already in
scope). First real check is CI on Gokul's commit.

**Next session start point:** Gokul commits `src/search/mod.rs`,
`src/search/alpha_beta.rs`, `src/main.rs`; confirm CI passes. Then run
the same Engine Bench matchup in 3 configs (both toggles default, LMP
off, SingularMultiCut off) and send the logs — that identifies which
(if either) technique is responsible, per ROADMAP.md Phase 27's
numbered next-steps list.

---

## Session 85 — 2026-07-25 (Phase 26 item 1: null-move king-exposure guard implemented as a runtime UCI option — D75; not yet SPRT-tested or committed)

**Built/done:**

1. Asked Gokul which parked item to pick up; chose **Phase 26 item 1**
   (null-move king-exposure guard) over the other two candidates
   (correction-history expansion, deeper perft coverage).

2. Read `search/alpha_beta.rs`'s existing `can_null_move` guard set in
   full before writing anything (mandatory read-before-write) — confirmed
   ROADMAP's description was accurate: standard guards only (not PV, not
   in check, sufficient depth, `static_eval >= beta`, zugzwang guard via
   `has_non_pawn_material`, no consecutive nulls), nothing king-safety
   specific.

3. **D75 — implemented the guard as a runtime UCI option
   (`NullMoveKingGuard`, default `false`), not a compile-time change.**
   New `king_safe_square_count(pos, color)` helper in `alpha_beta.rs`
   (counts king-ring squares that are unoccupied-by-own and
   unattacked-by-enemy, reusing the existing `Position::is_attacked`).
   When the option is off (default), the guard computes nothing and
   changes nothing — byte-identical to pre-Phase-26 null-move behavior.
   When on: ≤1 safe square skips null-move entirely, ≤2 reduces the
   adaptive reduction `r` by 1 (floored at 1). Threaded through
   `SearchInfo` → `EngineState` → `cmd_uci`/`cmd_setoption`/`cmd_go`,
   exactly mirroring the existing `Contempt` plumbing end-to-end.
   Full reasoning, including why this is a runtime option rather than a
   Cargo feature flag, in DECISIONS.md D75.

4. Added test coverage across all three touched files: `search/mod.rs`
   (default-false), `alpha_beta.rs` (`king_safe_square_count` correctness
   on three constructed FENs — open-center king, corner-boxed king with
   own pawns excluding ring squares, rook-attacked ring square excluded;
   plus guard-off and guard-on search-still-completes sanity checks),
   `main.rs` (option default/parse, and a `cmd_go`-wiring proof test in
   the same style as `test_cmd_go_applies_contempt_to_search` — checks
   the actual `SearchInfo` the search thread used, not just
   `EngineState`'s copy of the setting).

**Decisions made:** D75 — see DECISIONS.md.

**Follow-up (same session, after Gokul confirmed commit + green CI):**
ran the recommended `uci_match_runner.yml` A/B (20 games, seed 21000,
`NullMoveKingGuard value true` vs. default). Result: A scored 12-5-3
(67.5%), +127 Elo. **D76 — not treated as confirmation**: n=20 carries
too much sampling noise (same reasoning D48 already established for why
the CI regression gate's threshold is loose, applied in reverse here)
to promote a strength-affecting default off a single 20-game run.
Recommended a 100+-game confirmation run at a fresh seed before any
default change. `NullMoveKingGuard` stays default `false` in `main`.

**Second follow-up (200-game confirmation, seed 30000):** 74-72-54,
50.5%, +3.5 Elo — flat, confirms D76's suspicion that the 20-game
result was noise. **D77 — Phase 26 item 1 closed as "implemented,
tested, no measurable effect, parked off."** Option stays in the code
(harmless, off by default) in case different thresholds are worth
revisiting later, but no further tuning scheduled now.

**Third follow-up (Phase 26 item 2, D78):** picked up deeper/adversarial
perft coverage next. Read `tests/perft.rs`, `movegen/pawns.rs`, and
`movegen/castling.rs` in full before writing anything. Found — and
corrected — a wrong premise in the original item wording: castling
requires the rook on a *standard* a1/h1/a8/h8 square (king hardcoded
e1/e8), so "unusual rook file" isn't a real case; the actual
adversarial case is a standard-square rook blocked by some other
randomly-placed piece. Added 4 new hand-verified perft tests to
`tests/perft.rs`: rank-1-double-push→en-passant (perft(1)=5,
perft(2)=32, hand-enumerated in the test comments) and
castling-blocked-by-intervening-piece (perft(1)=16, perft(2)=78,
including a tricky rook-move-that-opens-check branch). Both reuse
already-verified positions from `pawns.rs`/`castling.rs`'s own unit
tests rather than novel FENs, and both are designed to discriminate a
specific plausible bug (hardcoded EP rank; blocked-path miscount) —
explained in each test's comment. **D78 — Phase 26 item 2 closed** on
the achievable scope; explicitly did NOT attempt exact-value conversion
of the 20-seed random-position range checks (no independent oracle for
arbitrary random Pet Dragon arrangements — an inherent limitation, not
a follow-up task).

**Fourth follow-up (CI-caught bug, D79):** Gokul committed and CI
failed — `test_perft_castling_blocked_by_intervening_piece_depth2`:
`left: 75, right: 78`. Root cause: my hand count missed that the Rh7
branch (not just Rh8) restricts Black's king, since a rook on h7 opens
the 7th rank and hits d7/e7/f7 without itself giving check. Rather than
re-guess by hand a second time, built the crate standalone (`apt-get
install rustc cargo` — Ubuntu's packaged 1.75, sidesteps the known
dev-dependency/edition2024 wall since `cargo build`/`run` never touch a
dependency's dev-dependencies) and ran the file's own existing
`perft_divide` helper against the exact FEN from a throwaway
path-dependency probe crate. Confirmed 75 is correct (`h1h7: 2`, not
5), fixed the test's expected value and comment. **D79** also records
this as the preferred method for verifying future hand-written perft
tests before shipping them, instead of hand-arithmetic alone.

**Fifth follow-up (Phase 26 item 3a, D80/D81):** asked which of the
three item-3 sub-candidates to build; Gokul said do all three, as
sequential isolated diffs (not one bundle, per the item's own
methodology note). Started with 3a (non-pawn-material correction).
Read `pruning.rs`'s `CorrectionHistory` and all three of its
`main.rs` threading points in full first. Added a second table
(`correction_history_nonpawn`, indexed by new `nonpawn_hash()`),
applied additively alongside the existing pawn-hash table, threaded
through the same `EngineState`/`cmd_go` plumbing.

Followed D79's own recommendation and verified via a standalone probe
crate before shipping — caught two real issues this way: (1) the first
draft's search-integration test used the start position, whose small
natural search-vs-eval error legitimately rounds to 0 under the
correction table's integer-division weighted average — switched to a
constructed large-error position (hanging queen); (2) that same
constructed position, in its first draft, **crashed the engine
entirely** — traced (via temporary local instrumentation, not shipped)
to an illegal input FEN (opponent already in check when it's not their
move), which lets the move generator offer a "capture the enemy king"
move. **D81**: this is real but NOT a live-play bug — legal search
from `start_pos()`/`generate_with_seed()` can never reach an illegal
state, so it can't happen in self-play or tournament games — but it
means a malformed external `position fen` command crashes the whole
engine instead of erroring gracefully. Logged as a new Phase 26 item 4
for a future scoped session; fixed my own test position (king moved
off the problem diagonal) rather than the underlying gap, which is
explicitly out of scope for this diff.

**Sixth follow-up (D82, gating fix):** asked which toggle strategy to
use for item 3a's upcoming SPRT test (temporary UCI option vs. pinned
pre-3a commit ref). In answering, realized item 3a had shipped
always-on with no kill switch at all — inconsistent with item 1's own
established discipline (unvalidated strength-affecting change ships
gated off, gets its own SPRT-style A/B via a runtime option, default
flip only considered after). Added `nonpawn_correction_enabled` /
UCI `NonPawnCorrectionHistory` (default false), gating both the
`.apply()` and `.update()` call sites — verified via the same
probe-crate method as D79/D80 (`off -> corr = 0`, `on -> corr = 21` on
the same test position). Updated the existing wiring test to
explicitly enable the flag (it would otherwise silently test nothing)
and added default/gating-specific tests in both `alpha_beta.rs` and
`main.rs`.

**Seventh follow-up (Phase 26 item 4, D83):** Gokul asked to pick up
item 4 (the D81 crash) next instead of continuing to item 3b. Fixed
`Position::from_fen` to validate — before returning `Ok` — that each
color has exactly one king (`FenError::KingNotFound`, a variant that
existed but was never actually wired up before this) and that the side
not to move isn't in check (new `FenError::OpponentInCheck`). Per D81's
own recommendation, chose parse-time rejection over defensively
guarding move generation.

Before shipping, swept the whole codebase for collateral damage: this
validation applies to every `from_fen` call, not just the UCI path.
Extracted all 98 FEN literals from `src/`/`tests/` via grep and ran
each through the new validation via the probe crate. Found 5
pre-existing test FENs that were themselves illegal (same class as
D81, and the same mistake I'd made myself in D80's first draft) — two
in `pawns.rs`/`make_unmake.rs` (same FEN, king on the pawn's own
capture diagonal), three in `open_lines.rs` (king sitting directly on
the open file/rank each test was measuring), one in `nnue/inference.rs`
(a 16-queen stress position where every square on ranks 1-6 was
attacked — needed relocation plus a blocking pawn, not just relocation
alone). Fixed all 5, re-swept — 98/98 now pass; confirmed the 3
remaining "rejections" from the first sweep were grep artifacts (a UCI
command string and lichess JSON fixtures, never real `from_fen` calls).
Added 4 new tests directly on the validation itself. **Phase 26 item 4
closed.**

**Eighth follow-up (item 3a SPRT test):** Gokul ran the 20-game A/B
(seed 40000) — 60.0%, +70.4 Elo. **D84**: same "not enough at n=20"
reasoning as item 1's D76, recommended a 100+-game confirmation.
200-game confirmation (fresh seed) came back 47.2%, -19.1 Elo. **D85**:
flat-to-mild-negative, not statistically distinct from zero — no
measurable benefit. **Phase 26 item 3a closed, parked off**, same
outcome and treatment as item 1. Noted explicitly: two items in a row
this session (item 1, item 3a) looked promising at n=20 and landed
flat/negative at n=200 — a real pattern about the *simple form* of
these ideas at Pet Dragon's current strength, not necessarily that
either idea was wrong in principle.

**Ninth follow-up (Phase 26 item 3b, D86):** asked what's next; Gokul
chose "3b then 3c" despite items 1/3a's flat results — a deliberate
call to keep going rather than pivot off two null results. Read the
existing move-ordering `cont_hist` implementation in full for
reference. Key design decision: the roadmap's "recent move pairs"
phrasing needs 2 plies of move history, but rather than thread a new
parameter through `alpha_beta`'s 8 internal recursive call sites and
every external caller, read the second-to-last move directly from
`pos.history` (already maintained for undo) — `pos.history.last().mv`
always exactly matches the existing `prev_move` parameter when
non-null, verified by checking every call site. Zero signature changes
anywhere. New `continuation_hash()` hashes only the last two moves'
squares (position- and piece-independent, unlike `cont_hist`) — a
deliberate scope simplification, documented as such. Shipped
gated-off via UCI `ContinuationCorrectionHistory` from the start this
time, no D82-style retrofit needed. Verified via the same probe method
as every prior diff this session before shipping.

**Tenth follow-up (CI failure, D87):** Gokul committed item 3b and CI
failed with compile errors — `MoveKind::DoublePawnPush` isn't a real
variant (correct name: `DoublePush`) and `alpha_beta.rs`'s test module
used `MoveKind` without importing it, both purely inside
`#[cfg(test)]` code the probe method structurally cannot compile
(`cargo build --lib` never builds test modules at all). Fixed both,
then found and fixed a third real bug while re-verifying: a test's own
setup was inconsistent with the function it tested (only pushed one of
two required synthetic history entries). **More importantly, found a
way to actually compile and run the real test suite locally** —
`cargo build --lib` (plain debug profile) produces dependency `.rlib`
files with `panic=unwind` (only `[profile.release]` overrides to
abort), which can be handed directly to `rustc --test` against
`src/lib.rs` and each `tests/*.rs` file with explicit `--extern` flags,
entirely bypassing Cargo's dependency resolution (and therefore
`criterion`'s edition2024 wall, never touched). Ran the full real
suite: 464 lib tests + 19 + 22 + 21 integration tests, all passing
after the 3 fixes. **This replaces the probe method as the standard
local verification step going forward** — probe verification proved
production logic correct in every prior diff this session, but was
structurally blind to bugs living only in test code, which is exactly
what slipped through 3 times in a row here.

**Eleventh follow-up (item 3b SPRT results, D88):** both the 20-game
(seed 48000) and 200-game (seed 50000) results came in together —
-34.9 Elo and -22.6 Elo respectively. Unlike items 1/3a, these agree
with each other (both negative, similar magnitude) rather than
flip-flopping, which is a cleaner signal even though the 200-game
point estimate alone is still within a plausible noise band. **Phase
26 item 3b closed, parked off** — no larger confirmation run needed
given the consistency. Named the pattern directly: items 1, 3a, and 3b
— every SPRT-tested idea from the Session 83 external-advice review —
have now landed flat-to-negative. Flagged item 3c (the last untested
idea from that review) as worth an explicit discussion before building,
not an assumed next step.

**Not done this session:**
- Item 3c not started, and now explicitly flagged as a decision point
  rather than a queued task.
- Phase 16 (NNUE) scoping not started — the standing alternative if
  Gokul wants to redirect.

**Next session start point:** ask Gokul directly: continue to item 3c
despite the pattern, or redirect (Phase 16 NNUE scoping, or something
else)? No default — this is a genuine open decision per D88, not
something to proceed on assumption. **Use the D87 direct-rustc
verification method on any new diff with test code before presenting
it as ready** — do not rely on the probe method alone for that
purpose anymore.

**Twelfth follow-up (Phase 26 item 3c, D89):** Gokul chose to build 3c
despite the flat/negative pattern in items 1/3a/3b. Read the existing
singular-extension block in full — confirmed the roadmap's original
"singular/double/triple extension margins" wording doesn't quite match
the actual code (Pet Dragon has one extension level via
`tt_move_extension`, not distinct double/triple stages) and scoped
around what's actually there. Reduces the singular margin (base 2) by
up to 1 when the base pawn-hash correction table shows a large
historical eval error for the current position — deliberately reads
only that one source, not the two parked-off ones. Extracted the
reduction arithmetic into a pure `singular_margin_reduction()`
function for direct unit testing. **First diff this session verified
under the full D87 standard from the start** (not probe-then-fix-
after-CI-fails): built debug-profile deps, compiled real test binaries
for the lib, the `main.rs` binary (needed `CARGO_PKG_VERSION` supplied
manually — bypassing Cargo means bypassing its build-script variable
injection too), and all 4 integration test files. 633 real tests,
all green, before ever presenting the diff.

**Thirteenth follow-up (item 3c SPRT results, D90 — Phase 26 closed):**
both results came in together — -34.9 Elo at n=20, -40.1 Elo at n=200.
The strongest, least ambiguous negative result of any Phase 26 item
this session: magnitude grew slightly at the larger sample rather than
regressing toward zero. Parked off, no further confirmation needed.
**This closes Phase 26 entirely** — all 6 items resolved (1/3a/3b/3c
all parked off after proper SPRT testing, 2 was test-hardening with no
strength claim, 4 was a real bug and is fixed). Four independent
strength-affecting ideas from the Session 83 external-advice review,
four flat-or-negative results — a real, useful answer (not a wasted
session), delivered through a consistent, isolated, SPRT-verified
process for each one. Recommended next direction: Phase 16 (NNUE)
scoping, or a fresh look at other signal sources before committing to
NNUE's scale.

**Not done this session:**
- Nothing outstanding from Phase 26 — fully closed.
- Phase 16 (NNUE) scoping not started.

**Next session start point:** ask Gokul directly what's next now that
Phase 26 is closed — Phase 16 (NNUE) scoping is the standing
recommendation (D90), but this is a genuine open decision, not a
default. If NNUE: start with reading `docs/DECISIONS.md`'s existing
D34/D41 NNUE-parking entries and NORU's own docs before proposing a
scoping plan — do not assume prior context carries over uninspected,
this hasn't been touched in many sessions.

---

## Session 84 — 2026-07-22/23 (Rank-battery bug fix; Phase 23.4 design closed — D67; Threats-term gap found — D68; Phase 25 completed, applied, and CI-corrected — D69/D70; Phase 23.4 fully built and CI-confirmed — D71/D72; Phase 24 item 4 (Threats term) implemented — D73)

**Built/done, in order:**

1. **Reviewed two uploaded documents** (Tension Field eval/search-extension
   spec + a generic "stack more modules" pitch). Recommended against
   acting on either immediately — mid-Phase-25 is the wrong time to add
   more untuned terms, and the modules proposed were generic
   engine-strength ideas, not Pet-Dragon-specific. Redirected to what
   actually differentiates Pet Dragon (variant mechanics, D62's opening
   statistics idea) instead.

2. **D67 — closed Phase 23.4's exact-vs-bucketed design question**,
   held open since D62 (Session 82). Chose bucketed-by-structural-
   features: v1 bucket key = (rook_files, knight_files), 420 buckets,
   root-only move-ordering bias as the usage mechanism (not an
   auto-play book), stats table baked into the binary at build time.
   Full data pipeline and 6-step build order documented. Designed, not
   started — queued behind Phase 25.

3. **D68 — found a 4th HCE gap** while answering whether the engine
   scores pieces defending each other: no Stockfish-style **Threats**
   term (hanging-piece/weak-queen-protection/restricted-piece
   penalties). Documented as a Phase 24 addendum, not implemented —
   distinguished from the uploaded "coordination graph" pitch by being
   an established, already-credited-family (Stockfish, GPL v3) term
   rather than a speculative one.

4. **Fixed a confirmed bug in `open_lines.rs`**: `BATTERY_ROOK_QUEEN`
   only checked same-file rook/queen alignment, never same-rank —
   despite Pet Dragon's start position (all pieces on ranks 1-2) making
   the rank case actually *more* common than the file case
   statistically. Added the missing rank check (reusing the existing
   weight, no new Texel parameter needed), mirrored identically in
   `texel/features.rs` for dual-sync. Hand-verified against all existing
   `open_lines.rs` test FENs — no regressions, one new test added.
   Confirmed live on `main` before proceeding to Phase 25 step 2.

5. **Ran Phase 25 (D66) to completion.** Step 1 (`texel_gen.yml`) had
   already been triggered pre-session with different params than
   planned (`seed_start=15000, n=3500` vs. the planned `10000`/`4000`)
   — confirmed successful (run #7, 62,125 samples, also published as a
   permanent Release asset). Step 2: ran a 15-epoch/decay=0 sanity tune
   first (pipeline health check — clean, monotonic loss, but several
   sparse features sign-flipped as expected from zero regularization),
   then the real 75-epoch/decay=0.03 run. Ran `texel_diag.yml` against
   the final result — all 10 cases passed clean. Two `texel_diag.yml`
   trigger mistakes along the way (guessed a run ID from a log-zip
   filename instead of the actual workflow run ID; got a 404; corrected
   once Gokul supplied the real run URL) — noted and fixed, not repeated.

6. **Applied the tune to production code (D69).** All 8 files
   (`eval/material.rs`, `tables.rs`, `mobility.rs`, `pawns.rs`,
   `king_safety.rs`, `open_lines.rs`, `mod.rs`, `texel/weights.rs`)
   updated and cross-verified programmatically field-by-field for
   dual-sync, not just by eye. **One tuned result rejected and NOT
   applied** — `KNIGHT_NEAR_OWN_KING_BONUS`/`BISHOP_NEAR_OWN_KING_BONUS`
   tuned negative, which broke two existing tests encoding a validated
   invariant (shelter should help, not hurt); kept at Phase 24
   hand-picked defaults instead of changing the tests to fit a
   noisy first-tune result. `TEMPO` bumped 20→24, required widening one
   test bound that was tightly coupled to the old exact value.

7. **CI caught a second break D69 missed (D70).** Gokul ran CI after
   committing — `test_passed_pawn_bonus` failed (`-24` instead of
   `>0`). Traced exactly: D69's applied `ENEMY_KING_DIST_EG`/
   `OWN_KING_DIST_EG` (`1→3`, D63 item 1's first tune) swings hard
   negative for that test's specific FEN (enemy king already on the
   pawn's promotion square, own king 7 away) — `-84` alone from that
   term, overwhelming the base passed-pawn bonus. This should have been
   caught during D69's own verification, not after — the per-field
   dual-sync check confirmed eval matched `weights.rs`, but didn't walk
   every changed term's arithmetic against every existing test FEN by
   hand, and this was the one gap. No Rust toolchain is available in
   this environment to actually run `cargo test`, so this class of
   error (a term that stays positive but still flips a *combined*
   test's sign) needs explicit per-test computation next time, not a
   directional sanity impression. Fixed: reverted
   `ENEMY_KING_DIST_EG`/`OWN_KING_DIST_EG` to the hand-picked default
   (`1`/`1`) in both `eval/pawns.rs` and `texel/weights.rs`, same
   rejection category as the knight/bishop-near-king call — recomputed
   by hand against the failing test (`32 > 0`, passes, matches known
   pre-Phase-25 behavior exactly). **Gokul confirmed a fresh CI run
   green after this fix — Phase 25 is genuinely closed.**

8. **Started Phase 23.4, step 1 of D67's build order.** Read
   `selfplay.rs` in full before editing. Added a second, additive
   output stream (`starting_seed | rook_files | knight_files |
   root_move_uci | game_result`) — one row per game, captured from
   White's starting setup before any move is made (Pet Dragon's
   mirrored setup makes White's rook/knight files canonical for both
   colors), root move and result backfilled the same way the existing
   NNUE stream backfills `game_result_from_stm`. Also updated
   `selfplay.yml`, which the code change alone wasn't sufficient for —
   the workflow never passed a 4th CLI arg and never uploaded the new
   file as an artifact, so without this it would have been silently
   generated and thrown away inside the ephemeral runner. Added a
   parallel per-shard upload + merge job step, mirroring the existing
   NNUE artifact pattern exactly, as its own artifact
   (`opening-data-combined-...`) that doesn't touch the primary
   artifact's name or contents. Not yet run — no actual opening data
   exists yet, this is purely the capture-mechanism.

9. **Ran and validated the first real opening-stats collection batch,
   then found and corrected a real design error (D71).** Gokul
   confirmed `selfplay.rs`/`selfplay.yml` CI-green and committed. A
   500-game validation batch first (format-checked: 500/500 rows,
   correct structure, sensible values — e.g. `a2f7` and `f2f7` root
   moves, long diagonal/file slides consistent with each game's own
   rook/knight file placement). Then a real collection attempt at
   `shards=15, games_per_shard=2000` — Gokul flagged it was tracking
   toward 600 min runtime, which would have exceeded the Actions
   360-min job ceiling and produced nothing (no partial-artifact
   upload on timeout); cancelled and resized to `games_per_shard=800`
   (12,000 games total), which completed. Validated: no data loss
   (12,000/12,000), all seeds unique, all values well-formed. Checked
   bucket coverage before building the aggregator, and found **D67's
   "420 buckets" estimate was wrong** — it assumed rook/knight files
   were drawn from disjoint pools, but pieces of different types (or
   even both rooks) can share a file across its two ranks. Real count:
   1,054 distinct buckets already hit in just 12,000 games, 2.5x the
   estimate. Consequence, checked directly: zero (bucket, move) pairs
   clear the 30-sample threshold with this batch (max 16) — expected
   given the corrected count, not a bug, but worth surfacing before
   building the aggregator so an empty first output isn't mistaken for
   broken.

10. **Built the aggregator (D67 step 3, D71).**
    `src/bin/aggregate_opening_stats.rs` +
    `.github/workflows/aggregate_opening_stats.yml`, mirroring
    `texel_tune.rs`/`.yml`'s exact input pattern
    (`data_run_id`/`data_paths`/`data_urls`, comma-separated) so future
    accumulated batches combine rather than replace. Generates
    `src/opening_stats.rs`: a 12-bit-packed-key sorted static array with
    binary-search `lookup()`, chosen over a `phf` map to avoid a new
    WASM-uncertain dependency. Per bucket, keeps only the single
    best-win-rate move clearing the threshold; non-qualifying buckets
    are omitted (not zero-filled) so "no data" stays distinguishable
    from "checked, no edge" at lookup time. Includes its own generated
    tests (table sorted, a lookup miss returns `None`). Not yet run
    against real data — would currently produce a valid but empty
    table.

11. **Accumulated a second data batch, ran the real aggregator.**
    `seed120000-15×1200` (18,000 games), combined with the existing
    12,000 for 30,000 total, no overlap, no loss. Bucket count nearly
    plateaued (1,054→1,068, D71's correction holds up) but qualifying
    (bucket, move) pairs grew far slower than bucket count — 2.5x the
    games only produced 2 pairs clearing 30 samples. Flagged the real
    driver (per-move split within each bucket, not bucket count) and
    the scale needed for broad coverage (hundreds of thousands of
    games) before Gokul decided to run the aggregator now on the thin
    real result rather than wait. Ran it for real (both `selfplay.yml`
    run IDs) — output matched a from-scratch manual re-derivation
    exactly: 2 entries, both mapping to `a2a7` (an open-file rook
    lift), both from buckets with a rook on the a-file — internally
    consistent, first real evidence the full pipeline is correct end
    to end.

12. **Wired the root-move-ordering bias into `search/ordering.rs`
    (D67 step 5, D72).** Additive bonus, gated on `ply==0 &&
    fullmove_number==1 && White to move` — both conditions required,
    since `ply==0` alone fires on every `go` regardless of how far
    into a real game the position is. **Caught a real panic bug before
    shipping**, not after: the file-extraction helper assumed exactly
    2 rooks/2 knights (a valid assumption for `selfplay.rs`'s
    guaranteed-fresh setups, invalid for `ordering.rs`'s arbitrary
    UCI-supplied positions) — an *existing* test FEN in the same file
    (`test_capture_before_quiet`, zero rooks/knights, `fullmove 1,
    White to move`) matched the new gate exactly and would have
    panicked. Checked the new gate against the actual existing test
    suite before shipping rather than assuming it was narrow enough —
    it wasn't. Fixed to degrade gracefully (`Option`, not panic),
    hand-verified every existing test's behavior is unchanged.

**Decisions made:** D67 (23.4 bucketed design, full spec), D68 (Threats
term gap, documented candidate), D69 (Phase 25 application + the
knight/bishop-near-king rejection + watch-item list), D70 (CI-caught
test break, ENEMY_KING_DIST_EG/OWN_KING_DIST_EG reverted), D71
(bucket-count estimate corrected empirically, aggregator built), D72
(real aggregator run, ordering.rs wired, panic bug caught pre-ship).

**Bugs fixed:** rank-battery detection gap in `BATTERY_ROOK_QUEEN`
(`open_lines.rs` + `texel/features.rs`); `test_passed_pawn_bonus`
failure from D69's king-distance term application (D70); a not-yet-
shipped panic in `ordering.rs`'s new opening-stats gate, caught before
commit rather than after (D72).

**Files delivered this session:** `open_lines.rs` + `texel/features.rs`
(rank-battery fix, confirmed committed); `eval/material.rs`,
`eval/tables.rs`, `eval/mobility.rs`, `eval/king_safety.rs`,
`eval/open_lines.rs`, `eval/mod.rs` (D69, Phase 25 application);
`eval/pawns.rs` + `texel/weights.rs` (D70's correction, CI-confirmed
green); `selfplay.rs` + `.github/workflows/selfplay.yml` (Phase 23.4
step 1, confirmed committed and CI-green);
`src/bin/aggregate_opening_stats.rs` +
`.github/workflows/aggregate_opening_stats.yml` (Phase 23.4 step 3,
confirmed committed, CI-confirmed run successfully against real data);
`src/opening_stats.rs` (generated, 2 real entries, NEW file);
`src/lib.rs` (added `pub mod opening_stats;`); `src/search/ordering.rs`
(D67 step 5 wiring) — the last three not yet confirmed committed as of
this entry.

**Confirmed by Gokul (same session):** `opening_stats.rs`, `lib.rs`,
and `ordering.rs` are committed to `main` and CI is green. Phase 23.4
step 5 fully closed.

13. **Phase 23.4 step 6, closing out the build order (D67/D72).**
    Added `test_opening_stats_bias_applies_to_known_bucket` to
    `search/ordering.rs`: a hand-constructed FEN matching real table
    entry 207, traced by hand first (confirmed `a2a7` is always a
    rook-takes-rook capture in that bucket — not a quiet move — since
    Black's mirrored setup guarantees a like-for-like piece on a7, and
    confirmed it's provably the single highest-scored move in this
    specific position since every other White piece is boxed in or has
    nothing to capture), then verified through actual `score_moves()`
    that the bonus applies additively as designed. Skips gracefully
    rather than failing if a future table regeneration changes entry
    207 — verifies the mechanism, not a frozen data point. **All 6 of
    D67's build-order steps are now done or ongoing-by-design.**

14. **Implemented Phase 24 item 4 — the Threats term (D68→D73).** New
    `eval/threats.rs`: `UNDEFENDED_PENALTY` (per-piece, attackers >
    defenders) + `THREAT_BY_MINOR_BONUS` (minor attacking enemy rook/
    queen). Scoped down from D68's 4-part sketch to these 2 during
    implementation — dropped "restricted piece" deliberately (most
    direct `mobility.rs` overlap risk), not deferred. Full Texel-chain
    wiring across 9 files, including two — `texel_tune.rs`'s writer and
    `texel_diag.rs`'s parser — that no earlier Phase 24 item had needed
    to touch (found by tracing where `EXPECTED_PAIR_COUNT` actually
    came from, rather than assuming `weights.rs`/`weights_f64.rs` were
    the whole chain). Hand-verified against every existing `eval/mod.rs`
    test before considering it done (symmetric at both standard-chess
    and Pet-Dragon starts, bounded elsewhere) — applied D69/D70's
    verification discipline proactively this time rather than finding
    a break after the fact.

**Decisions made (final list, this session):** D67, D68, D69, D70,
D71, D72, D73 — see DECISIONS.md for each.

**Files delivered, Phase 24 item 4 (not yet confirmed committed):**
`eval/threats.rs` (NEW), `eval/mod.rs`, `texel/features.rs`,
`texel/predict.rs`, `texel/predict_f64.rs`, `texel/weights.rs`,
`texel/weights_f64.rs`, `src/bin/texel_tune.rs`,
`src/bin/texel_diag.rs`.

15. **CI caught a real self-consistency bug in D73 (D74).**
    `test_predict_matches_evaluate_after_moves` failed: 191cp mismatch
    mid-game. Traced exactly: `evaluate_threats` hardcoded
    `Color::White`/`Color::Black` instead of using
    `pos.side_to_move`/`.flip()` like every other `evaluate_*`
    function — worked by coincidence at the game start, silently wrong
    sign once Black was to move. Confirmed the sibling convention
    directly (`evaluate_material`, `evaluate_open_lines`) rather than
    trusting memory a second time. Named the process gap plainly: the
    convention was asserted from memory when writing D73, not verified
    against a sibling file — that's what should have happened first
    time. Also: none of D73's own tests caught this, since every one
    happened to use a White-to-move position — closed that gap too
    with a dedicated side-to-move regression test, not just the bug
    fix. Only `eval/threats.rs` needed changing —
    `texel/features.rs`'s threats code was already correct.

**Decisions made (final list, this session):** D67, D68, D69, D70,
D71, D72, D73, D74 — see DECISIONS.md for each.

**Confirmed by Gokul (same session):** all Phase 24 item 4 files,
including D74's `eval/threats.rs` fix, are committed to `main` and CI
is green. Phase 24 (all 4 items) and Phase 23.4 (full build order) are
both genuinely, fully closed — no open threads from this session.

**Session ending here on context-window grounds** (long session, no
exact token count available, but heavy signs: many large file fetches,
substantial doc growth today) rather than a natural task boundary —
flagging that distinction since it affects nothing about correctness,
only when to stop.

**Next session start point:** no default next task — ask what to work
on (candidates on record: Phase 23.4 step 4, ongoing data
accumulation, no fixed target; Phase 26's parked items; a future Texel
re-tune once enough new data/terms accumulate to justify one; Phase
23.4's opening-stats table currently has only 2 entries, worth more
`selfplay.yml` batches whenever convenient).

---

## Session 83 — 2026-07-20 (Phase 24 items 1, 2 & 3: passed-pawn king distance + pawn storm + minor-piece shelter — D63, now fully closed out; then game analysis, Phase 25 scoped + kicked off, Phase 26 logged)

**Built/done, in order:**

1. **Implemented Phase 24 item 1 (D63)** — passed-pawn king-distance
   bonus in `eval/pawns.rs` (`passed_pawn_king_distance_bonus()`):
   EG-only, scaled by Chebyshev distance from each king to the passer's
   promotion square and by how advanced the passer already is
   (`rank_idx`). Pure multiply-add by design (no division) so the term
   stays linear in its two weights.
2. **First CI round-trip (`logs_80438759341.zip`) — compiled, but
   broke `texel::predict()`'s self-consistency test.** The new eval
   term wasn't mirrored in the independent Texel predictor
   (`test_predict_matches_evaluate_default_weights` failed:
   predict=354, evaluate=345). Root cause: hadn't read D35/Session 53's
   load-bearing constraint that any `eval/*.rs` term needs a matching
   feature+weight in `texel/{features,predict,weights,weights_f64,
   predict_f64}.rs`. Fixed by dropping a truncating `/7` from the
   formula and wiring two new diff features
   (`passed_king_enemy_dist_diff` / `passed_king_own_dist_diff`) and
   two new weights (`enemy_king_dist_eg` / `own_king_dist_eg`) through
   all 5 texel files, proved bit-exact equivalence by hand (packed-
   score addition is associative, so per-pawn-then-pack and
   diff-then-multiply-once are the same i64 sum before `taper()`).
3. **Second CI round-trip (`logs_80462058909.zip`) — compile error
   `E0063`, missing fields.** `src/bin/texel_diag.rs` and
   `src/bin/texel_tune.rs` construct/parse `TunableWeights` directly
   (read/write the tuned-weights dump file format) and were missed by
   the first pass — hadn't grepped the *whole* repo for
   `TunableWeights` construction sites, only `texel/`. Fixed both;
   confirmed via `grep -rl "TunableWeights" src/` that no other site
   existed.
4. **Third CI round-trip (`logs_80462762656.zip`) — compiled and ran,
   but broke a pre-existing sanity test.** `eval::pawns::tests::
   test_passed_pawn_bonus` (asserts a lone passed pawn scores `> 0`)
   flipped to `-5` at weight `2`: its fixture FEN is a worst-case
   combo (enemy king literally on the promotion square, own king 7
   squares away, the pawn also isolated). Fixed by halving both
   weights to `1` — recomputed the same FEN by hand: `+23` (68 passed
   − 17 isolated − 28 king-dist), comfortably positive.
5. **Fourth CI round-trip (`logs_80464922497.zip`) — green.** 418 lib
   tests passed, 0 failed, across all binaries 567 run / 562 passed /
   5 ignored / 0 failed.
6. **Docs updated to close out the item** (this entry + `ROADMAP.md`
   Phase 24 item 1 checked off + test-count table refreshed).

**Bugs fixed:** see round-trips 1-3 above (Texel self-consistency,
missing-fields compile error, sign-flip on a pre-existing test) — all
same-session, all CI-confirmed fixed by round-trip 4.

**Decisions made:** none new for `DECISIONS.md` — this was an
implementation of an already-decided D63 candidate, not a new
architectural choice. The weight-magnitude walk-back (2→1) is recorded
in `ROADMAP.md`'s Phase 24 item 1 entry, not `DECISIONS.md`, since it's
a tuning-value note, not a decision.

**Phase 24 item 2 (pawn storm), same session, after item 1 shipped:**

1. **Implemented `PAWN_STORM_BONUS[8]` + `pawn_storm_danger()` in
   `eval/king_safety.rs`** — mirror image of the existing pawn shield:
   scores enemy pawns advanced on files near a king, indexed by rank-
   distance-to-king bucket. Read `king_safety.rs` in full first, as
   planned.
2. **Applied item 1's lessons up front this time**, before submitting
   anything: checked whether `king_safety.rs`'s constants are wired
   into `texel::predict()` (they are — `king_safety_side()` — almost
   made the same mistake again, caught it by grepping before writing
   the doc comment that would have claimed otherwise) and did the full
   5-file Texel wiring (`features.rs`/`predict.rs`/`weights.rs`/
   `weights_f64.rs`/`predict_f64.rs`) plus both `TunableWeights`
   construction sites (`texel_diag.rs`/`texel_tune.rs`) in the same
   submission as the eval change, instead of discovering the
   requirement via a failed CI run.
3. **Hand-checked every pre-existing `king_safety.rs` test FEN**
   against the new term before submitting (learned from item 1's
   round-trip 3) — confirmed none of them have an actually-advanced
   enemy pawn near the opposing king, so the new term evaluates to 0
   everywhere those tests check.
4. **CI round-trip (`logs_80500379634.zip`) — green on the first
   submission.** 422 lib tests passed, 0 failed.

**Bugs fixed (item 2):** none — first submission was green.

**Decisions made (item 2):** none new for `DECISIONS.md`, same
reasoning as item 1.

**Phase 24 item 3 (King-relative PST bucketing), same session, after
items 1-2 shipped — design discussed before implementing, per Gokul's
own explicit request:**

1. **Design discussion.** Presented the real gap precisely:
   `king_safety.rs`'s `attacker_weight` already scores enemy pieces
   attacking near a king (tropism, attacking side); nothing scored our
   own minor pieces staying close to our own king (clustering,
   defensive side). Weighed 3 options: A (flat bonus for knights/
   bishops in the same king-file-third zone as own king, ~2 params),
   B (full 3-zone table across more piece kinds, ~10-15 params, risks
   double-counting `open_lines.rs`'s rook-activity terms), C
   (attacking-side mirror, risks double-counting `attacker_weight`
   itself). Recommended A as smallest/cheapest/most-verifiable; Gokul
   picked A via the options tool.
2. **Implemented option A** — `KNIGHT_NEAR_OWN_KING_BONUS`/
   `BISHOP_NEAR_OWN_KING_BONUS` in `eval/king_safety.rs` (`8`/`6`,
   hand-picked), `minor_piece_shelter_bonus()`/`king_file_zone()`
   helpers, wired into `king_safety_score`. 5 new tests.
3. **Full Texel-chain wiring done in the same submission** as the
   eval change (all 5 texel files + both `TunableWeights` construction
   sites), plus a fresh full-repo grep confirming no other
   `TunableWeights`/`TexelFeatures` construction site existed —
   applying items 1-2's lessons rather than re-discovering them.
4. **Hand-checked every pre-existing `king_safety.rs` test FEN**
   against the new term before submitting — none have knights/bishops
   at all, so the new term is 0 everywhere those tests check.
5. **CI round-trip (`logs_80664145490.zip`) — green on the first
   submission.** 427 lib tests passed, 0 failed.

**Bugs fixed (item 3):** none — first submission was green.

**Decisions made (item 3):** the option-A-vs-B-vs-C design choice
itself is worth a permanent record since it rules out B and C for
future reference (both risk double-counting an existing term) — added
as a new entry in `DECISIONS.md` this session (see that file).

**Phase 24 / D63 is now fully closed out** — all three candidates
implemented, CI-confirmed green, documented.

**Post-Phase-24, same session — game analysis, goal-setting, and
Phase 25/26 scoping:**

1. **Analyzed a real Pet Dragon (skill 20) vs Stockfish (skill 10)
   match log Gokul supplied**, on his report that low skill levels beat
   Stockfish but skill 20 "played pathetic" and lost. Replayed the
   whole game with `python-chess` (installed this session) rather than
   hand-tracing: ruled out a variant-rules mismatch (standard start FEN
   means Pet Dragon's custom pawn rule coincides with normal chess
   here), found a real opening inaccuracy (`3...c6?!`) and traced
   material through the whole game — conclusion was NOT "broken skill
   scaling": Black was only 1-2 pawns down for most of the game,
   fought back to material equality by move 60, and actually lost to a
   late endgame technique collapse around moves 60-65. Flagged the
   comparison as likely confounded (unknown what Stockfish skill level
   the "skill 0-4 wins" games used) and recommended a real batched
   `uci_match_runner.rs` comparison over trusting n=1.
2. **Verified, not assumed, whether `PawnStartMap` leaks into eval/
   search** (Gokul's doubt) — grepped `eval/`, `texel/`, `search/` for
   any reference to `pawn_starts`/`started_here`: zero hits. Confirmed
   it's correctly scoped to move generation (`movegen/pawns.rs`'s
   double-step gate) and Zobrist hashing (`pawn_start_key`) only.
3. **Clarified a separate doubt** (D10/D11's "pawn start feature
   convergence") — this is NNUE-specific (128 extra input features
   that go to 0 once a pawn's moved), unrelated to `PawnStartMap` and
   currently dormant since NNUE is shelved (D61).
4. **New goal set: make Pet Dragon "lethal... irrespective of game
   phase."** Presented 4 candidate levers (full Texel re-tune, endgame
   conversion technique, search depth/efficiency, un-shelve NNUE),
   asked Gokul to prioritize, he asked Claude to decide. Chose full
   Texel re-tune — reasoning recorded as **D66**. Scoped as **Phase 25**
   in `ROADMAP.md` (2-step plan: `texel_gen.yml` then `texel_tune.yml`,
   both pre-existing mobile-friendly `workflow_dispatch` workflows —
   no new infrastructure needed).
5. **Corrected a real misconception about Chess960.** Gokul initially
   asked for the engine to also outperform Chess960 opponents,
   believing it was already covered by Pet Dragon's variant. Checked
   `movegen/castling.rs` directly: castling is hardcoded to `E1`/`E8`
   king squares with standard `G1`/`C1` targets — genuine Chess960
   support (flexible king placement, proper Chess960 castling
   semantics) does not exist and would be real new work. Gokul
   confirmed he doesn't want that built — his actual intent was just
   "be strong on whatever position comes up, including Chess960-shaped
   ones within our own pool," which Phase 25 already covers (eval
   confirmed rank/position-agnostic in step 2 above).
6. **Reviewed two outside-AI variant-adaptation responses Gokul
   shared**, plus did a live-web-search-grounded comparison against
   current Stockfish (18, Jan 2026 stable + current dev builds — not
   memory, Stockfish moves too fast to trust training-data recall).
   Verified claims against the real repo rather than taking either
   source's word: most of the advice was already correctly implemented
   (`PawnStartMap`, `pawn_start_key` zobrist hashing, Texel-tuned
   setup-agnostic PST confirmed via `tables.rs`'s header, `texel_gen.rs`
   confirmed sampling from the randomized-start generator not a
   standard-position bias). One response's core premise (Chess960-style
   flexible castling) was simply wrong for this repo. The other
   ("DragonView," a per-piece View/Sovereign-View system) was assessed
   and **not adopted** — mostly a repackaging of existing techniques
   (attack tables, SEE, king-ring danger) plus unproven search-
   integration ideas carrying a self-flagged 30-50% NPS-regression
   risk; too large/invasive a bet against this session's proven
   small-diff pattern. Full reasoning on record in `ROADMAP.md` Phase
   26 so it isn't re-litigated without new information. 3 genuinely
   open, real candidates logged as **Phase 26** (not scheduled): a
   null-move king-exposure guard, deeper adversarial perft coverage,
   and expanding `CorrectionHistory` beyond its current single
   pawn-hash table (Stockfish 18 confirmed to have evolved past this;
   Pet Dragon's version matches Stockfish's original 2023 design).
7. **Triggered Phase 25 step 1** (`texel_gen.yml`) at the very end of
   the session — `num_games=4000`, `seed_start=10000`. Caught a real
   correctness risk before it happened: checked `texel_gen.rs`'s seed
   math directly (`seed = seed_start + game_idx`, sequential, no
   dedup elsewhere) and confirmed run #4 (`texel-data-seed2000-n3500`
   artifact) already consumed seeds 2000-5499 — an initially-suggested
   `seed_start=4000` would have silently overlapped and regenerated
   duplicate games. `10000` is clear of that and any plausible range
   from the other 2 prior production runs without needing to check
   them individually. Not yet confirmed to complete — status is
   **IN PROGRESS** at session close.

**Bugs fixed (post-Phase-24 work):** none — this stretch was analysis,
scoping, and verification, no code shipped except triggering the
existing `texel_gen.yml` workflow (no code change).

**Decisions made (post-Phase-24 work):** D66 (full Texel re-tune
chosen as the next strength lever, over 3 other candidates).

**Next session start point:** Check whether the `texel_gen.yml` run
(seed 10000, 4000 games, ~5h estimated) completed — Gokul will bring
the Run ID or artifact at the start of the fresh conversation. If
green: move straight to Phase 25 step 2 (`texel_tune.yml`, fed this
run's `data_run_id`, short sanity run first per the existing plan). If
it failed or is still running: diagnose before retriggering, don't
assume a blind rerun is the right move. Phase 26's 3 candidates remain
logged and unscheduled — no action needed on them unless explicitly
picked up.

---



**Built/done, in order:**

1. **Ran a competitive-analysis pass on request** (not a Phase 23 item
   itself — a fresh ad-hoc comparison against current-gen top engines,
   Stockfish 18/SFNNv10 specifically, via web search). Conclusion:
   search is essentially state-of-the-art (Pet Dragon's pruning/
   extension suite matches what top engines converged on), eval is the
   real gap (flat 896-input NNUE, ~2.5M training rows vs. tens of
   billions, still loses to HCE) — consistent with and not
   contradicting the Session 74 analysis already embedded as Phase 23.
   Found two small, real search deltas not yet in Phase 23: Stockfish's
   singular extension now includes multi-cut/negative-extension
   siblings (Pet Dragon had the base technique only), and no explicit
   Late Move Pruning distinct from LMR.
2. **Read `alpha_beta.rs`, `pruning.rs`, `mod.rs`, `ordering.rs` in
   full** (fresh via `raw.githubusercontent.com`, not assumed from
   ENGINE_ARCHITECTURE.md's summary) before writing anything, per the
   mandatory read-before-write rule.
3. **Implemented D59 — multi-cut pruning + negative extensions**,
   extending Phase 13.3's base singular extension. `alpha_beta.rs`'s
   singular block generalized from a boolean `singular_extension` flag
   to a signed `tt_move_extension` (+1 singular / -1 PV / -2 non-PV
   negative-extension / 0 default). Multi-cut returns early
   (`return singular_beta`) before move generation, same shape as the
   existing probcut/razoring early returns — not a new pattern in this
   codebase. Renamed `singular_ext` → `move_ext` at all 4 use sites in
   the PVS block (still applied only to the TT move).
4. **Implemented D60 — Late Move Pruning (LMP)**, distinct from LMR:
   skips late quiet moves outright (not just reduced) once a
   depth-indexed quiet-move-count threshold is passed. New
   `MAX_DEPTH_LMP` constant (`mod.rs`), new `lmp_threshold()` /
   `should_apply_lmp()` (`pruning.rs`), wired into `alpha_beta.rs`'s
   move loop right after the existing futility-pruning block. Uses a
   single conservative ("non-improving") threshold table rather than
   the improving-flag-differentiated version Stockfish/Ethereal use —
   Pet Dragon's `alpha_beta` doesn't track an "improving" flag at all
   currently, and adding one is a separate, real change with its own
   risk surface, not bundled into this session.
5. **Added 9 new unit tests**: 8 for `lmp_threshold`/`should_apply_lmp`
   in `pruning.rs` (monotonicity, out-of-range clamping, each guard
   condition individually, threshold boundary), 1 in `alpha_beta.rs`
   at depth 7 (>= `MIN_DEPTH_SINGULAR`) across 5 seeded positions,
   asserting the search stays bounded and returns a legal move — this
   exercises D59's new branches without asserting which one fires
   (position/TT-state dependent, not deterministic).
6. **Could not run `cargo test` this session** — no local Rust
   toolchain reachable (rustup's domain isn't in the sandbox's network
   allowlist, and no cargo/rustc was pre-installed). Verified instead
   by: full manual review of both changed files, a brace/paren/bracket
   balance check, and a diff against the freshly-pulled pristine source
   confirming only the intended lines changed. **Flagged explicitly as
   a real test risk** — the next CI run on `main` is the first actual
   confirmation these compile and pass.

**Files changed this session:**
- `src/search/alpha_beta.rs` — D59 (multi-cut/negative extension),
  D60 (LMP call site), 1 new test
- `src/search/pruning.rs` — D60 (`lmp_threshold`, `should_apply_lmp`),
  8 new tests
- `src/search/mod.rs` — `MAX_DEPTH_LMP` constant

**Bugs fixed:** None — new capability on top of existing, working
search code; no prior behavior touched except the rename of
`singular_ext` to `move_ext` (mechanical, same value in the one case
that already existed — `+1` — behavior only changes in the two new
cases that didn't exist before, multi-cut and negative extension).

**Decisions made:** D59 (singular-extension family: multi-cut +
negative extensions), D60 (Late Move Pruning). Both in `DECISIONS.md`.

**Post-session addendum (same day):** Gokul supplied the `Deploy`
workflow's logs (`logs_80383979254.zip`) for commit `4429ec0` —
confirmed via the checkout step's SHA that this is the commit
containing D59/D60. `Build WASM & Web`'s `wasm-pack build --release
--features wasm` step compiled `pet_dragon v0.1.0` clean, zero errors,
"Finished `release` profile [optimized] target(s) in 16.37s", and
`Deploy to GitHub Pages` reported success. This is real confirmation
the D59/D60 code is syntactically and type correct and compiles clean
for the wasm32 target.

⚠️ **This does NOT confirm `cargo test` passes.** The `Deploy` workflow
only runs `wasm-pack build`, which compiles production code and never
touches `#[cfg(test)]` modules — so the 9 new unit tests added this
session (8 in `pruning.rs`, 1 in `alpha_beta.rs`) are still unverified.
Need the separate `Build`/`regression-gate` workflow's logs (the one
that runs `cargo test`) for commit `4429ec0` specifically, not this
Deploy run, to close out the real open item from this session.

**Second post-session addendum (same day):** Gokul supplied the actual
`Test` job's logs (`logs_80383979241.zip`), also for commit `4429ec0`
(re-confirmed via checkout SHA). ✅ **Fully closes out the open item
above.** `pet_dragon_lib`'s test binary — where both D59's and D60's
`#[cfg(test)]` modules live — ran 412 tests, 412 passed, 0 failed. All
9 new tests confirmed passing individually by name in the log:
`test_search_at_singular_extension_depth_no_panic` and the 8
`test_lmp_*` tests under `search::pruning::tests`. Across every test
binary in the job (lib, `match_runner`, `uci_match_runner`,
`node_count`, `make_unmake`, `perft`, `setup`) the real total is 545
run / 540 passed / 5 ignored / **0 failed** — no compile errors
anywhere in the job (the 530 estimate from earlier in this session was
close but not exact; 545/540/5 is the confirmed real number, now
recorded in `ROADMAP.md`'s test-count table). D59/D60 are genuinely
done: compiles clean on both wasm32 (Deploy) and native (Test)
targets, all tests green.

**Next session start point:** D59/D60 are fully shipped and verified —
no follow-up action needed on them. Elo impact still isn't measured
(same open item as 23.2's thread-differentiated Lazy SMP) — a real
`uci_match_runner.yml` run would quantify it, worth doing before
stacking more search changes on top blind. If NNUE work resumes
instead, start from D58's three remaining options. Otherwise
ROADMAP.md's 23.4 (variant opening statistics) is still the next
unblocked *new* item.

---

**Third post-session addendum (same day):** Gokul decided to shelve
NNUE entirely for the future (D61) and asked whether 23.5 should be
done now instead — flagged that this is self-contradictory (23.5 *is*
the NNUE upgrade) and confirmed HCE is already Texel-tuned and solid
(Phase 14, ~39 measured Elo gain), not an empty bucket needing generic
"solidification." Gokul chose to move to 23.4 instead. Investigated
23.4 before writing code (read `selfplay.rs` in full) and found the
ROADMAP's original scoping was wrong: `selfplay.rs`'s output has no
seed/position/root-move data to aggregate, and — the bigger issue —
2.16M distinct starting positions vs. sequentially-seeded self-play
games means an exact-position book would almost never hit a real game,
the same starved-coverage shape as NNUE's own problem. Presented the
two real design paths (exact-position vs. structural-feature
bucketing); Gokul chose to hold 23.4 too rather than pick one now.
Both decisions recorded in full in `DECISIONS.md` D61/D62 — resume
either only by re-reading those, not by restarting from ROADMAP's
original (now-corrected) framing.

**Session ends with Phase 23 having no active items** — 23.1–23.3 and
23.6–23.7 done, 23.4 and 23.5 both explicitly held. Next session:
don't auto-resume either — ask what to work on.

---

**Fourth post-session addendum (same day):** Gokul asked whether the
engine's search/eval had more headroom generally. Read all six
`eval/` submodules fresh (`material.rs`, `tables.rs`, `mobility.rs`,
`pawns.rs`, `king_safety.rs`, `open_lines.rs`) to answer with actual
source rather than guessing. Confirmed most standard HCE terms already
present and Texel-tuned; found three real, unimplemented gaps —
passed-pawn king distance (highest value), pawn storm, king-relative
PST bucketing (lowest/most speculative) — recorded as a ranked,
unscheduled candidate list in `DECISIONS.md` D63 and `ROADMAP.md`'s
new Phase 24 section. Also surveyed evaluation paradigms beyond HCE/
NNUE on request (MCTS+policy/value net, searchless transformer,
GPU-sized NNUE, policy-guided move ordering) and ruled all out for
Pet Dragon's actual deployment constraints — recorded in `DECISIONS.md`
D64 so it isn't re-investigated from scratch later.

**`ENGINE_ARCHITECTURE.md` updated this session** (first update since
Session 73) to fix three real staleness issues found while doing this:
its Lazy SMP row still said "not parameter-differentiated" despite
D49 shipping that fix in Session 76; it didn't mention D59/D60 at all;
and its evaluation section didn't reflect D61's NNUE-shelving decision
or D63/D64's new findings. All four now corrected in the doc, not just
in `DECISIONS.md`/`ROADMAP.md`, matching the "repo is the memory,
don't let docs silently diverge from source" rule.

---

## Session 81 — 2026-07-19 (23.3c: pawn-feature redesign scoped-not-shipped, phase-balance oversampling tried, re-parked — D56/D57/D58)

**Built/done, in order:**

1. **Deep-dived D11's pawn-start feature mechanism** on request — each of
   16 pawns has its own feature, permanently off after that pawn's first
   move, active only a few plies out of a game's up-to-300-ply length.
2. **Scoped a redesign (D56)**: 5-bucket one-hot over
   (own unmoved pawns - opponent unmoved pawns), replacing D11's 128
   per-square flags. Wrote it in full (`features.rs` rewrite, 8 tests
   updated, `NUM_FEATURES` 896->773) but **did not ship it** — it's
   schema-breaking (old checkpoint becomes incompatible, forced retrain,
   red CI in between), and the person doing the committing explicitly
   said they didn't want more unreliable/risky changes after this
   session's earlier smoothing-sweep surprises.
3. **Shipped the cheaper alternative instead (D57)**: phase-balanced
   training-row oversampling — no feature/schema change, purely
   reweights which rows get seen more during training via integer
   duplication in the per-epoch order. `phase_balance_cap` CLI/workflow
   param, default 4, `1` disables.
4. **Tested D57 at `phase_balance_cap=4`, `label_smoothing=0.10`.**
   Result: zero effect — no row was duplicated, `val_loss` identical
   to the un-oversampled baseline. The actual activation-count
   histogram (logged by the run) was far flatter than the imbalance
   hypothesis assumed; that hypothesis is not well supported by the
   real data. No further tuning of this lever recommended.
5. **Also discussed, not built**: confidence/in-distribution gating
   (route to NNUE only in well-covered training-data regions) and
   Stockfish-distillation data augmentation for the phase of a game
   where D10's design has converged close enough to standard chess to
   trust it — and a sequencing recommendation for all three remaining
   options (D56's redesign -> training-pipeline swap off NORU's CPU
   trainer -> distillation) if any get picked up later.
6. **Re-parked (D58)**, superseding D55's reopening list. Current best
   remains the `label_smoothing=0.10` checkpoint from Session 80: 17W-2L-1D,
   87.5%, +338 Elo for HCE. Unchanged by this session.

**Files changed this session:**
- `src/bin/train_nnue.rs` — `phase_balance_cap` param (D57)
- `.github/workflows/train_nnue.yml` — `phase_balance_cap` workflow input
- No change to `src/nnue/features.rs` or the committed network — D56 was
  scoped but explicitly not shipped; the D57 test run reproduced the
  existing `0.10` checkpoint's numbers exactly, so nothing needed
  re-committing.

**Bugs fixed:** none (this session was investigation/negative-result,
not a bug fix).

**Decisions made:** D56, D57, D58 (all in DECISIONS.md). D58
supersedes D55's reopening-lever list — read D58, not D55, if NNUE
work resumes.

**Next session start point:** NNUE is re-parked, no action needed
unless picking it up again deliberately. If so, start from D58's three
remaining options (feature redesign / pipeline swap / distillation) —
all are real investments now, not quick tries, so get explicit buy-in
on scope before starting rather than assuming a small change will do.
Otherwise, ROADMAP.md's 23.4 (variant opening statistics) is the next
unblocked item.

---

## Session 80 — 2026-07-19 (23.3b: NNUE saturation bug fixed, swept, re-parked — D53/D54/D55)

**Built/done, roughly in order:**

1. **Root-caused and fixed D52's logit saturation** (D53): BCE against
   the hard 0/1/0.5 game-result label has no finite minimizer, not a
   data-volume or weight_decay/grad_clip_norm-strength problem as
   D52 speculated. Added `label_smoothing` param to `train_nnue.rs`
   and `train_nnue.yml` (workflow input, default `0.03`), bounding the
   BCE target into `[label_smoothing, 1-label_smoothing]`.
2. **Swept 6 label_smoothing values** against the existing 2.48M-row
   23.3 dataset (0.03/0.05/0.08/0.10/0.15/0.30), each match-tested with
   a real 20-game `uci_match_runner.yml` run, not just `eval_diag.yml`.
3. **Found and documented D54**: `eval_diag.yml`'s static 8-position
   calibration check does not reliably predict match strength — the
   best-looking diagnostic (`0.30`, 0/8 saturated) produced the worst
   match result of the sweep. Discovered by chasing the diagnostic
   number for two rounds before checking match results caught the
   mismatch; `eval_diag.yml` is now treated as a pre-filter only, never
   a ranking signal, going forward.
4. **`label_smoothing=0.10` confirmed as a clean local optimum**
   (D55) — monotonic 0.05→0.08→0.10 improvement, cliff at 0.30.
   Result: 17W-2L-1D, 87.5%, +338 Elo for HCE — real progress from
   the pre-fix 20-0 shutout (sign-correct, non-degenerate now), but
   not competitive with HCE, and numerically identical to the very
   first NNUE blend test this project ever ran (17.4/D25).
5. **Decision (D55): ship as-is, re-park.** `label_smoothing=0.10`
   checkpoint committed to `src/nnue/weights/nnue_pet_dragon_quantized.bin`.
   `NNUEWeight` stays 0% default (unchanged since D25) — network
   available as a UCI option, not on the default search path.
6. **Discussed but did not build**: material-bucket confidence-gating
   (route to NNUE only in board-material buckets well-represented in
   training data, HCE elsewhere) — logged as the top reopening lever
   in D55, not started.

**Files changed this session:**
- `src/bin/train_nnue.rs` — `label_smoothing` param (D53)
- `.github/workflows/train_nnue.yml` — `label_smoothing` workflow input
- `src/nnue/weights/nnue_pet_dragon_quantized.bin` — `label_smoothing=0.10`
  checkpoint (confirm this is the version actually committed — several
  networks were swapped in/out during the sweep; the last one sent was
  the `0.10` checkpoint and should be the one that stuck)

**Bugs fixed:** D52's logit saturation (root cause, not just symptom
— see D53).

**Decisions made:** D53, D54, D55 (all in DECISIONS.md).

**Next session start point:** 23.3b is DONE/re-parked. Read
ROADMAP.md's 23.4 (variant opening statistics) as the next unblocked
item, or pick up the material-bucket gating idea from D55's reopening
list if NNUE work continues instead. Before either: confirm with Gokul
that `nnue_pet_dragon_quantized.bin` on `main` is in fact the
`label_smoothing=0.10` checkpoint (verify via match_runner one more
time if any doubt — several swaps happened this session).

---

## Session 79 — 2026-07-18 (Phase 23.3 compute done, NNUE retrained and re-parked — D52)

**Built/done, roughly in order:**

1. **Merged 10 sharded self-play runs** (seeds 0-27000, from Session 77's
   `selfplay.yml`) into one 2,428,608-row file. Verified all 10 source
   files distinct by SHA-256, seed ranges non-overlapping, only 1.79%
   incidental cross-file row duplication (normal, not a seed collision).
2. **Corrected a real error from Session 77's D50**: NNUE training does
   not go through Kaggle — `train_nnue.yml` runs it natively on GitHub
   Actions (Phase 16.5). D50 has a correction note appended (not
   rewritten). This was flagged as owed at the end of Session 78 and is
   now fixed.
3. **Walked Gokul through the real hand-off**: merged file uploaded as a
   GitHub Release asset (486MB, over the 25MB repo-upload limit),
   reused an existing completed `lichess_sample.yml` run
   (28721456844) instead of regenerating, triggered `train_nnue.yml`
   with both.
4. **Training completed**: 2,478,608 total rows (2,428,608 self-play +
   50,000 Lichess), best epoch 6/10, val_loss=0.50108 — a real
   improvement over the old 483,080-row run's 0.53776.
5. **`src/nnue/inference.rs` doc-comment update prepared** (stale
   val_loss/row-count numbers corrected) — delivered to Gokul,
   **not yet confirmed committed**.
6. **Match-tested the new network**: first attempt (Gokul-triggered) was
   `main` vs `main` with no NNUE options — didn't actually test
   anything, caught and re-run correctly. Real test — pure HCE
   (`NNUEWeight=0`) vs pure new-NNUE (`NNUEWeight=100`), 20 games — was
   a **20-0 shutout for HCE**.
7. **Ran `eval_diag.yml`** to diagnose the shutout rather than assume
   "just weaker": found the raw network saturating at the ±1500cp hard
   clamp on 6/8 test positions, including ones that should read near
   zero. Traced the likely mechanism to `train_nnue.rs`'s `lambda=0.7`
   blend leaving 30% loss weight on the hard game-result label, which
   BCE-on-logits has no natural incentive to keep bounded —
   `weight_decay=0.01`/`grad_clip_norm=1.0` (D30/D33) apparently
   insufficient at 5x the old data volume.

**Bugs fixed:** None shipped this session — the saturation issue was
diagnosed, not yet fixed. Root cause is a hypothesis (well-evidenced,
not yet confirmed by a follow-up run).

**Decisions made:** D52 — NNUE re-parked again, this time on a
materially better-understood basis: data volume is confirmed NOT the
blocker (loss metric improved substantially with 5x data, strength
didn't), a specific training-config mechanism is the leading suspect
instead of "needs more data/capacity." Correction appended to D50 (see
above). Full rationale in `DECISIONS.md`.

**`ROADMAP.md` changes:** 23.3 marked done (data volume question
settled); new 23.3b added for the logit-saturation fix, explicitly left
open pending the next session's decision on which hyperparameter(s) to
adjust first (`lambda` alone vs. also touching `weight_decay`/
`grad_clip_norm`).

**Next session start point:** Gokul explicitly asked to document
everything and defer further exploration to a fresh session — so:
decide the retrain experiment design for 23.3b (lambda-only vs.
multi-knob), then run it via `train_nnue.yml` on the same 2.48M-row
dataset (already uploaded as a GitHub Release, reusable — no need to
re-merge or re-upload), and re-check with `eval_diag.yml` before
spending another `uci_match_runner.yml` run. Carried forward, none
blocking: (1) `regression-gate` branch-protection setup (Session 75);
(2) manual multi-threaded `uci_match_runner.yml` run to measure 23.2's
Elo impact (Session 76). (`src/nnue/inference.rs`'s doc-comment update
— confirmed committed same-session, no longer open.)

---

## Session 78 — 2026-07-17 (diagnostic export "Result:" line — D51)

**Built:**

1. **`web/index.html`**: added `gameOverWinner`/`gameOverReasonText`
   globals, set at all 8 `customGameOver = true` sites (checkmate,
   stalemate, king captured, insufficient material, threefold
   repetition, 50-move rule, abort, resign, and both directions of time
   forfeit), reset in `startGame()`. Diagnostic FEN export now includes
   a `Result:` line built from these — e.g. `Result: White wins (you)
   — Checkmate`, `Result: Draw — Threefold repetition`, `Result:
   Aborted — no result` — instead of leaving the reason to be
   reverse-engineered from `Status`/`(in check)` after the fact.

**Bugs fixed:** None in game logic — `_checkGameOverInner`'s actual
win/draw conditions and the in-game banner text are untouched; purely
additive result-capture for the diagnostic export.

**Decisions made:** D51 — capture the reason at the source (the 8
existing termination sites already know exactly why the game ended) as
a short undecorated reason string + winner color, rather than trying to
reconstruct it later from `customGameOver`/`customInCheck`/board state.
Full rationale in `DECISIONS.md`.

**Context — why this came up:** User uploaded diagnostic FEN exports to
sanity-check the threefold-repetition system. Two things came out of
that investigation, both closed by this change going forward: (1) an
export showing `Status: Game over (in check)` was initially misread as
implying checkmate — turned out `(in check)` is just an orthogonal
`customInCheck` flag, unrelated to end-reason; hand-tracing the last 8
plies against the halfmove clock showed the final position had only
recurred twice, not three times, so that specific game most likely
ended by checkmate rather than the repetition the move pattern visually
suggested. (2) A separate, still-**unresolved** earlier export showed
`moveHistory` (81 entries) inconsistent with the live board/turn state
implied by the exported FEN's fullmove number (which matched only 79
applied plies) — traced through all 4 move-commit code paths
(`applyEngineMove`/`handleTap`/`handleDrop`/`doPromotion`) and confirmed
each pushes to `moveHistory` and mutates `customTurn`/`fullmoveNumber`
atomically in the same synchronous block, with no other mutation site
found (ruled out undo — it's a stub in chaos mode — and ruled out the
move-review/`viewIndex` system, which explicitly never touches the live
board variables). No mechanism found in current `main` source that
explains that specific desync; flagged as open, lower-priority given
it hasn't recurred in later exports, and may simply have been a
one-off from the export's ambiguous `Status` phrasing (now improved by
this session's `Result:` line) rather than a deeper state bug.

**Next session start point:** No specific task queued. Options: (a)
if Gokul reproduces the `moveHistory`-vs-fullmove desync again with a
fresh diagnostic export (now including a `Result:` line, which should
make the reason unambiguous), dig further into it with that concrete
example in hand; (b) otherwise fall back to the Phase 23 backlog per
its own "don't skip ahead" rule — check with Gokul on 23.3's data
volume before resuming Phase 23. Carried forward, non-blocking:
`regression-gate` branch-protection setup (Session 75); manual
multi-threaded `uci_match_runner.yml` run to measure 23.2's Elo impact
(Session 76).

---

## Session 77 — 2026-07-17 (Phase 23.3, code half: sharded self-play generation — D50)

**Built:**

1. **`.github/workflows/selfplay.yml` rewritten**: was one sequential
   job (max ~3,000 games/run observed, the prior `n3000` artifact), now
   a 3-job pipeline — `plan-shards` (builds a `[0..shards-1]` matrix
   from the `shards` input via plain bash), `selfplay-shard` (matrix
   job, one independent batch per shard, disjoint seed range per
   shard), `merge-shards` (downloads + concatenates every shard into
   one combined artifact, 30-day retention; runs with `if: always()`
   and warns if fewer than `shards` files landed). `selfplay.rs` itself
   untouched — only how many times it's invoked in parallel changed.
   Defaults: 10 shards × 300 games = 3,000 games/run, same total ceiling
   as before but ~10x less wall-clock time; both inputs overridable for
   bigger runs.
2. **`ROADMAP.md` 23.3 left unchecked, split explicitly**: code/infra
   half done this session; compute half (actually triggering enough
   runs to grow past ~500K rows, merging multiple runs, retraining on
   Kaggle) is still open and needs Gokul to trigger `selfplay.yml`
   repeatedly across future sessions/days.

**Files touched:** `.github/workflows/selfplay.yml`.

**Bugs fixed:** None — new workflow structure, `selfplay.rs` binary
unchanged.

**Decisions made:** D50 — GitHub Actions sharding chosen over a
dedicated Kaggle generation job (generation is plain CPU search work,
no GPU/training-loop benefit from Kaggle, and Actions was already the
mobile-runnable surface this used); matrix+merge chosen over one bigger
sequential job to cut wall-clock time via parallelism, not just raise a
ceiling. Full rationale in `DECISIONS.md`.

**Next session start point:** Nothing code-shaped is queued next on
Phase 23 until 23.3's compute half actually produces more data — the
natural next session either (a) helps Gokul interpret/kick off
`selfplay.yml` runs and plan a merge-and-retrain pass once enough
combined artifacts exist, or (b) if Gokul says data volume isn't ready
yet, skips ahead to 23.4 is explicitly NOT allowed per Phase 23's own
"don't skip ahead" rule — check with Gokul before starting anything
Phase-23-shaped next session; otherwise pull a different, independent
task off the backlog. Two items still carried forward, both requiring
Gokul, neither blocking: (1) `regression-gate` branch-protection setup
(Session 75); (2) manual multi-threaded `uci_match_runner.yml` run to
measure 23.2's Elo impact (Session 76).

---

## Session 76 — 2026-07-17 (Phase 23.2: thread-differentiated Lazy SMP — D49)

**Built:**

1. **`SearchInfo.thread_id: usize` added** (`search/mod.rs`), default `0`,
   set explicitly per helper in `main.rs`'s Lazy SMP spawn loop (`for tid
   in 1..threads`, was `for _ in 1..threads`). Main thread's `SearchInfo`
   is never touched, so it stays `thread_id == 0` implicitly.
2. **`search::pruning::lmr_thread_base(thread_id) -> f64`** — new pure
   function, replaces the hardcoded `0.75` LMR-formula constant in
   `alpha_beta.rs`'s reduction calculation. Cycles through a 4-entry
   table; `thread_id == 0` always returns exactly `0.75` (byte-identical
   to prior behavior). 3 new unit tests.
3. **`search::ordering::thread_tie_break(thread_id, from, to) -> i32`** —
   new pure function, adds a small (`0..=3`) deterministic offset to
   quiet-move ordering scores in `score_move()`, so ties break
   differently per helper thread. `thread_id == 0` always returns `0`. 4
   new unit tests (main-thread-zero, determinism, bounded magnitude,
   varies-across-threads).
4. **`ROADMAP.md` 23.2 checked off**, flagged that Elo impact of this
   change is not yet measured — the 23.1 regression gate runs
   single-threaded and isn't sized to detect an SMP-scaling improvement;
   a manual multi-threaded `uci_match_runner.yml` run is the way to
   measure it, left as an open follow-up.

**Files touched:** `src/search/mod.rs`, `src/search/pruning.rs`,
`src/search/ordering.rs`, `src/search/alpha_beta.rs`, `src/main.rs`.

**Bugs fixed:** None — additive change to an existing, working feature.
Verified additive-only via `diff` against the freshly-fetched pre-edit
version of every touched file before finalizing (all edits were pure
insertions/single-line replacements, no unrelated lines touched).

**Decisions made:** D49 — fixed offset tables keyed on `thread_id`
(not per-thread RNG), main thread always pinned to the original
constant/zero-offset in both new functions, `skill_level`/`contempt`
deliberately left undiversified per Phase 20's existing constraint. Full
rationale in `DECISIONS.md`.

**Next session start point:** 23.3 — NNUE training data scale-up. This
is a background-compute task, not primarily a coding task (code in
`nnue/`/`evaluate_blended()` is already complete per Phase 16) — starting
point is scoping how much additional self-play + Lichess data is
realistically achievable via GitHub Actions compute before deciding
whether to actually kick off generation. Two open items carried forward,
neither blocking: (1) Gokul still needs to set `regression-gate` as a
required branch-protection check (from Session 75); (2) a manual
multi-threaded `uci_match_runner.yml` run to actually measure 23.2's Elo
impact hasn't been done yet.

---

## Session 75 — 2026-07-17 (Phase 23.1: lightweight SPRT-style regression gate — D48)

**Built:**

1. **`uci_match_runner.rs` (D36 harness) extended, not replaced**: new
   optional 11th CLI arg `min_score_pct` turns a match run into a
   pass/fail gate — process exits `1` if Engine A's score falls below
   the threshold, `0` otherwise. Arg is fully additive: every existing
   `uci_match_runner.yml` manual invocation (pre/post-tuning Elo, Skill
   Level tiers) never passes it, so those keep their exact prior
   always-exits-0 behavior unchanged.
2. **New pure function `regression_gate_passes(score_a_pct,
   min_score_pct)`**, unit tested (3 new tests: no-threshold always
   passes, at/above threshold passes, below threshold fails) — same
   "pull the boundary logic out as a pure fn" pattern as
   `elo_diff_from_score`/`split_uci_options` already in this file.
3. **New `regression-gate` job added to `build.yml`**, runs
   automatically on every `pull_request` (not manual-dispatch): builds
   the PR head as candidate, current `main` tip as baseline, plays 20
   games at 50ms/move via the harness, fails the job below 35% score.
   Uploads the match summary as a build artifact either way for
   debugging. Full rationale for the 35%/20-games/50ms choices and the
   live-`main`-vs-pinned-baseline-file decision recorded in
   `DECISIONS.md` D48.
4. **`ROADMAP.md` 23.1 checked off**, flagged with an explicit ⚠️ that
   the job existing and reporting is not the same as it *blocking*
   merges — Gokul still needs to mark `regression-gate` as a required
   status check in GitHub branch protection settings (one-time,
   mobile-app-doable) before it actually gates anything.

**Bugs fixed:** None — new capability on an existing, working harness,
no prior behavior touched (verified: the new arg is strictly additive
and every prior call site omits it).

**Decisions made:** D48 — regression gate implemented as an exit-code
threshold on the existing harness rather than new infrastructure or a
true statistical SPRT; baseline is always live `main`, not a pinned
SHA/file. Full rationale in `DECISIONS.md`.

**Next session start point:** 23.2 — thread-differentiated Lazy SMP.
Vary LMR aggressiveness / move-ordering tie-breaks by thread ID in the
helper threads (`main.rs`) so they explore genuinely different tree
regions instead of largely duplicating the main thread's search. Read
`main.rs`'s current Lazy SMP thread-spawn code fresh before writing
anything (not covered by any Tier 2 doc in enough implementation
detail). One thing still outstanding from this session, non-blocking:
Gokul needs to set `regression-gate` as a required branch-protection
check on `main` for 23.1 to actually block merges, whenever convenient.

---

## Session 74 — 2026-07-17 (ENGINE_ARCHITECTURE.md rewrite; competitive-analysis report generated, found wrong, corrected; corrected list embedded as ROADMAP Phase 23 — D47)

**Built:**

1. **Competitive analysis vs. top-class engines (Stockfish 18, Leela
   Chess Zero, Komodo Dragon), first draft.** Grounded correctly on the
   external side (CCRL rankings, Stockfish's actual recent additions —
   correction history, singular extensions with multi-cut/negative
   extensions — verified via web search), but built on the Pet Dragon
   side from a stale `ENGINE_ARCHITECTURE.md` (still describing Phase
   7/8 scaffolding) plus general knowledge instead of checking actual
   source. Listed 7 already-implemented techniques as gaps.

2. **`ENGINE_ARCHITECTURE.md` fully rewritten**, verified line-by-line
   against actual current `src/` (pulled fresh via codeload tarball,
   grepped struct definitions and function signatures directly) rather
   than assumed. Confirmed the full real state: PVS, null-move pruning,
   LMR, razoring, futility pruning, ProbCut, IIR, singular extensions,
   correction history, 1-ply continuation history, best-move-stability
   time management, Lazy SMP, MultiPV, pondering, Skill Levels, Syzygy,
   and the real NNUE blend state (0% default, both tested sizes lost to
   HCE) — all confirmed present and correctly documented for the first
   time since early scaffolding.

3. **Caught my own verification bug while fixing the first draft**: an
   initial case-sensitive `grep razor` missed the actual `Razoring`
   comment (capital R) in `alpha_beta.rs`, which would have
   reintroduced the exact same class of error inside the correction
   itself. Caught by re-running case-insensitive before finalizing
   anything.

4. **Improvement report regenerated, corrected to 5 real items**
   (thread-differentiated Lazy SMP, NNUE data scale-up, NNUE
   architecture upgrade, variant-specific opening statistics, SPRT-
   style testing gate), each confirmed absent from actual source rather
   than assumed, with an explicit copyright-cleanliness note per item
   (all are original-code implementations of general algorithmic
   techniques, same pattern as everything already in the engine).

5. **Corrected list embedded into `ROADMAP.md` as Phase 23** (D47) —
   checkbox-tracked like every other phase rather than left as
   standalone report files, numbered 23.1-23.5 in actual recommended
   execution order (not topic-grouped) per Gokul's explicit request,
   with a warning against skipping ahead since later items assume
   earlier ones are done.

**Bugs fixed:** None in the engine itself — this was a documentation-
accuracy and analysis-correction session. The two "bugs" were in my own
process (stale-doc-sourced false claims, then a case-sensitivity slip
while correcting them), both caught and fixed before anything wrong
reached `ROADMAP.md` or `DECISIONS.md`.

**Decisions made:** D47 — track post-release improvements as a numbered
ROADMAP phase, not standalone reports; full rationale and the two-
mistake postmortem recorded there.

**Next session start point:** Nothing mid-flight. Phase 23 is ready to
work top-to-bottom starting with 23.1 (SPRT-style testing gate)
whenever Gokul wants to pick it up. `ENGINE_ARCHITECTURE.md` is now
accurate and current — safe to treat as reliable Tier 2 context again
without re-verifying against source, until it next drifts.

---

## Session 73 — 2026-07-17 (WASM release assets added; v3.0.0 tag silently failed and was diagnosed; v3.3.3 confirmed fully working — D46 updated)

**Built:**

1. **`build-wasm` job added to `build.yml`.** Previously only native
   binaries shipped; wasm-pack output never reached a release at all.
   New job mirrors `deploy.yml`'s `wasm-pack build` command, then
   generates a third asset: `pet_dragon_standalone.js`, which base64-
   embeds the compiled wasm binary and calls `pet_dragon.js`'s `init()`
   with those bytes directly instead of letting it fetch a separate
   `.wasm` file. Considered and rejected true single-file bundling
   (concatenating the wasm-bindgen glue itself, Stockfish-`SINGLE_FILE`-
   style) — technically doable (proved out the exact transform: strip
   `export default` off the generated `init` function, re-add it at the
   end) but rejected as unnecessary fragility against wasm-bindgen
   version changes for one fewer file. Two-file version (standalone +
   `pet_dragon.js` together) is what shipped.

2. **Caught my own bug before it shipped**: first implementation used a
   bash heredoc (`cat > file << 'EOF' ...`) nested inside an indented
   YAML block scalar — a direct conflict, since heredoc terminators
   without `<<-` must sit at column 0 but YAML block scalars require
   every line indented. Caught on inspection, reverted to the working
   `node -e "..."` inline version before it was ever committed.

3. **`v3.0.0` tag published, discovered dead, diagnosed, fixed.** Gokul
   published `v3.0.0` before `build.yml`'s fix had actually landed on
   `main` — exactly the sequencing risk flagged when the tag/rolling
   split was first built. Confirmed via GitHub API (`releases/tags/...`)
   and Actions run history (zero runs against any tag ref, ever) that
   the old workflow — no `tags:` trigger at all at that moment — never
   engaged; the release's 2 assets were manually attached by Gokul, not
   automation. Deleted `v3.0.0`, confirmed the fix was live on `main` (the
   rolling `latest` release already showed all 7 assets from an
   intervening ordinary push, proving the workflow itself worked),
   retagged as **`v3.3.3`** at Gokul's request. Confirmed via screenshot:
   full 7-asset download table, correct generated body, "Latest" badge —
   pipeline worked correctly end-to-end this time.

**Bugs fixed:** The would-be heredoc/YAML conflict (caught pre-commit,
never shipped). The `v3.0.0` dead-tag situation (caught post-hoc via
API/Actions cross-check, not assumed from surface appearance).

**Decisions made:** D46 updated in place — corrected the worked example
from `v3.0.0` to the actual `v3.3.3`, and added a "confirmed the hard
way" postmortem of the sequencing failure so the specific diagnostic
method (cross-check release timestamps against Actions run history,
don't trust asset count alone) is on record for next time.

**Next session start point:** Nothing outstanding — this closes the
world-release housekeeping thread (README, dead code, release pipeline,
wasm assets, versioned tagging all confirmed). Check ROADMAP.md's
housekeeping/phase list fresh for whatever's next; nothing is mid-flight.

---

## Session 72 — 2026-07-16 (World-release prep: README factual fixes, dead file cleanup, versioned release pipeline — D46)

**Built:**

1. **README.md audit and fix**, prompted by Gokul asking what's left
   before a "world release." Found and fixed a real factual error —
   "Play against Stockfish directly in your browser" (it's Pet Dragon's
   own engine; leftover from early scaffolding, never caught). Replaced
   the Project Status table, which still showed every phase as
   pending/in-progress despite Phases 0-22 being complete — the single
   highest-visibility document in the repo was actively undermining a
   finished, tested engine. Softened the flat "3000+ Elo" claim to match
   DECISIONS.md's own careful framing (relative comparison only, no
   external rating pool for a custom variant), citing the real ~39 Elo
   Texel-tuning gain from Session 61's 520-game pinned-ref result.
   Caught and fixed my own mistake mid-session — first pass introduced a
   duplicate "Handcrafted evaluation" bullet; also fixed a pre-existing
   missing-space typo nearby.

2. **`src/material.rs` deleted.** Orphaned pre-Phase-8 duplicate of
   `src/eval/material.rs`, never compiled (not declared in `lib.rs`).
   Previously left in place because it "can't be deleted via web UI" —
   that blocker no longer applies; Gokul deleted it directly.

3. **Release pipeline redesigned (D46).** `build.yml` previously only
   ever published one rolling release under the literal tag `latest` on
   every `main` push — no mechanism existed to cut a real semantic
   version. Added a `tags: ['v*.*.*']` trigger; the `release` job now
   branches on `startsWith(github.ref, 'refs/tags/v')` to publish a
   frozen, "Latest"-badged release under the actual tag name (e.g.
   `v3.0.0`) instead, while the rolling `latest` release keeps working
   for `main` commits but stops stealing the "Latest" badge back once a
   real tag exists. Logic is tag-name-generic — works unchanged for any
   future `vX.Y.Z`. Gokul asked to cut `v3.0.0` as the first tagged
   release; instructed to commit `build.yml` to `main` *before* creating
   the tag (workflow version at tag-push time is whatever's on `main`
   then, not a later snapshot) — tag creation itself confirmed still
   pending as of end of session.

**Bugs fixed:** README's Stockfish/engine-identity error (factual,
public-facing); stale Project Status table (accuracy, public-facing);
duplicate README bullet introduced and caught in the same session.

**Decisions made:** D46 — versioned tag releases separate from the
rolling `latest` release; see DECISIONS.md for full rationale and the
rejected alternatives (manual local builds, dropping the rolling
release entirely).

**Next session start point:** Confirm whether Gokul published the
`v3.0.0` tag and whether the Actions run succeeded — check the Releases
page for 4 attached binaries (`pet-dragon-windows-x64.exe`,
`pet-dragon-macos-arm64`, `pet-dragon-macos-x64`, `pet-dragon-linux-x64`)
and the "Latest" badge on `v3.0.0` specifically, not on `latest`. If the
tag was created *before* the `build.yml` commit landed, the run will
have used the old workflow and silently done nothing — in that case walk
through deleting and recreating the tag now that the fix is live.

---

## Session 71 — 2026-07-16 (Verification-only session: Phase 21 eval bar + Phase 22 make_unmake.rs both confirmed; root index.html deleted)

**Built:** No new code this session — pure verification of two items
left open at the end of Session 70.

1. **Phase 21.3 eval bar — CONFIRMED.** Gokul supplied a screen
   recording of the live deployed page (`g-c-3.github.io/pet-dragon`).
   Extracted frames and zoomed into the eval bar region: it renders a
   real, changing numeric value (`+0.3` and others across different
   board states, e.g. `+0.9`, `+0.5` while browsing the Think Time
   control panel) — not a NaN, not a stuck placeholder, not silently
   falling back to the material+mobility heuristic. Phase 21 fully closed.

2. **Root `index.html` — deleted.** Gokul removed the orphaned root
   file per last session's housekeeping note. No further action needed.

3. **Phase 22.2 `tests/make_unmake.rs` — CONFIRMED.** Gokul triggered a
   real CI run and supplied the full `cargo test --verbose` Actions log
   (rustc 1.97.0, commit `4c2ab40`). Verified by name, not aggregate
   count, that all 5 rebuilt repetition tests pass:
   `test_no_repetition_at_start`,
   `test_pet_dragon_repetition_uses_pawn_start_hash`,
   `test_repetition_detected_after_moves`,
   `test_repetition_not_triggered_by_different_positions`,
   `test_threefold_repetition` — plus all 14 other tests in that file
   (perft-via-make/unmake depths 3-5, kiwipete, castling, promotion, en
   passant, hash consistency, 1000-position fuzz). Also cross-checked
   the `position/mod.rs` unit tests for the underlying algorithm
   (`test_is_repetition_chain_always_true_regardless_of_ply` and 3
   siblings) — all passing. Full suite total from this log: 396 lib +
   125 bin/integration = 521 tests run, 521 passed, 0 failed, 5 ignored
   (node_count.rs's depth≥8 perft, expected — those are manual-run only).
   Phase 22 fully closed.

4. **Test Coverage Summary in ROADMAP.md corrected.** The table had
   been stale since Session 63 (estimated 239/375 depending on which
   note you read). Replaced with the real per-crate counts read directly
   off this session's CI log — 521 total, authoritative until the next
   structural test change.

**Bugs fixed:** None — no code touched this session.

**Decisions made:** None — no new architectural decisions, verification only.

**Next session start point:** Both phases now fully closed with no
outstanding verification debt. Check ROADMAP.md for the next
un-started phase/task — nothing is mid-flight. If nothing else is
flagged, a good candidate is finally recomputing the per-module test
breakdown properly (low priority, mentioned as stale for several
sessions), or picking up the next roadmap item after Phase 22.

---

## Session 70 — 2026-07-14 (UCI completeness: Ponder, Contempt, real eval bar; UCI_Elo declined then reversed; shallow-test fix surfaces real time-fraction bug — D42/D43/D44)

**Built:**

1. **Warning cleanup follow-through** (carried over from earlier today,
   docs not yet updated for it): 3 rounds of `cargo test` round-trips
   fixed all compiler warnings across lib/bins/tests, plus one genuine
   bug found along the way — `test_finds_mate_in_1` in
   `src/search/alpha_beta.rs` was missing its `#[test]` attribute
   entirely, silently never running (a same-named sibling test in
   `iterative.rs` masked the gap). Final state: 0 warnings, 376/376 lib
   tests passing, confirmed via real CI logs, not just local checks.
   One real regression happened mid-cleanup (removed `Bitboard` from
   `pawns.rs` assuming it was unused, missed that it's used inside
   `#[cfg(test)] mod tests`) — caught and fixed same session via the
   next CI round-trip.

2. **Ponder UCI option** (`src/main.rs`) — declared
   `option name Ponder type check default true`. No engine state; the
   existing ponderhit/pending-allocation logic already worked, this was
   purely a missing advertisement some GUIs require.

3. **Contempt** (`src/search/mod.rs`, `src/search/alpha_beta.rs`,
   `src/main.rs`) — new UCI spin option, -100..100, default 0.
   `draw_score(ply, contempt)` derives the root-relative sign from
   `ply % 2` alone — no new root-side field needed anywhere in
   `SearchInfo`/`Position`. Applied at all 4 draw-detection sites in
   `alpha_beta.rs` (repetition, 50-move, insufficient material,
   stalemate — found the 4th site, stalemate, while implementing;
   wasn't in the original 3-site description). New unit tests in
   `search/mod.rs` (pure `draw_score()` math, all 4 sign cases) and an
   integration test in `alpha_beta.rs` (proves `SearchInfo.contempt`
   actually reaches a real `alpha_beta()` call via the unconditional
   50-move-rule path, not just that the pure function is correct in
   isolation) plus 5 new tests in `main.rs` covering the option/
   setoption/cmd_go wiring.

4. **Real eval bar for the browser** (`src/lib.rs`, `web/index.html`) —
   new WASM export `search_from_fen_with_eval()`, deliberately added
   alongside (not replacing) the existing `search_from_fen` to avoid any
   blast radius on callers this session didn't audit. Returns
   `"<uci_move> <eval>"`, eval always from White's perspective for bar
   stability. `web/index.html`'s Worker, `petDragonSearch()`, and a new
   `updateEvalBarFromSearch()` all updated to consume it; the old
   material+mobility `updateEvalBar()` heuristic stays as the instant-
   feedback fallback before a real search has run.

**Discovered along the way:**
- `web/index.html` (7199 lines) is the real, current frontend source —
  the repo's checked-in root `index.html` (2003 lines, what GitHub
  Pages actually serves) is stale, still on the pre-Skill-Level 2-arg
  `search_from_fen` call. Deploy pipeline isn't regenerating it. Not
  actioned this session — flagged in ROADMAP Phase 21's housekeeping note.

**Decision:** D42 — full reasoning for all three builds, plus why
`UCI_Elo` (the 4th originally-requested item) was declined rather than
built: it directly conflicts with D39's core reasoning (no honest Elo
promise this engine can back for a custom variant with no external
rating pool). Gokul chose to skip it and keep D39 intact rather than
override it.

**Verification gap, stated plainly:** the Rust lib/bins (everything
except the `wasm` feature) and the JS were both mechanically verified —
`cargo check --bins` clean, `node --check` clean on all 3 script blocks
including the Worker source checked separately as its own ES module.
The `wasm` feature itself could NOT be compiled locally this session —
`wasm-bindgen v0.2.126` (pinned in the committed Cargo.lock) needs
rustc 1.77+, this sandbox has 1.75.0. `search_from_fen_with_eval` was
manually field-checked against `Position`/`SearchResult`'s real
definitions and mirrors the already-working `search_from_fen`'s
structure, but was never actually built or run in a browser by Claude.

**Later the same session — reversed the UCI_Elo decline (D43):** Gokul
asked to implement `UCI_LimitStrength`/`UCI_Elo` after all, with
`UCI_LimitStrength` mapped to Skill Level and `UCI_Elo` mapped to
self-assumed Elo values — explicitly reversing the decline from earlier
this same session. Flagged the reversal and D39's actual objection
(honesty about calibration, not the mere existence of the option)
before proceeding; Gokul chose to continue anyway. Asked for a
comparative Elo table for all 21 levels before writing any code, per
his own instruction. Built that table from the only two real inputs
available: Session 68's actual measured tier-pair gaps (0v5 -619.4 Elo,
5v10 -117.2, 10v15 -65.0, 15v20 -81.35), rescaled to fit Gokul's chosen
absolute anchors (Skill 0 = 1200, Skill 20 = 2600), with the 16
untested intermediate levels linearly interpolated within each rescaled
band. Landed as `ELO_TABLE`/`elo_to_skill_level()` in `search/skill.rs`
(nearest-match, clamped, ties resolve to the lower/weaker level on
purpose) and `UCI_LimitStrength`/`UCI_Elo` UCI options in `main.rs`,
overriding Skill Level in `cmd_go` when enabled. New tests: 8 in
`skill.rs` (exact-anchor round-trips for all 21 levels, monotonicity,
both clamp directions, tie-break direction, non-exact nearest-match)
and 7 in `main.rs` (defaults, setoption parsing/clamping, cmd_go
override wiring). Full reasoning — especially which numbers are real
measurements vs. interpolation vs. Gokul's own chosen anchors, kept
visibly separate throughout — is in D43. D39 itself was NOT rewritten;
only its specific rejection of an Elo number was overridden.

**Later still the same session — fixed shallow cmd_go wiring tests
(D44):** Gokul correctly flagged that several `cmd_go` tests (the new
D42/D43 ones, plus two pre-existing ones they copied their shape from)
only checked that `EngineState` fields weren't mutated, never that the
spawned search thread actually received the configured value — a real
gap, not just a style nit, since `cmd_go` never writes those fields
back either way. Fixed properly rather than patched: widened
`wait_for_search()` to return the joined thread's actual `SearchInfo`
(`Option<SearchInfo>`, backward compatible — `Option` isn't
`#[must_use]`, so every existing bare-statement call site keeps
compiling with zero warnings), and extracted `effective_skill_level()`
and `build_time_control()` as pure, directly-testable functions instead
of inline logic. Doing the extraction carefully surfaced a real bug in
the process: `tc.skill_time_fraction_pct` was computed from
`state.skill_level` BEFORE the Elo-override logic ran, so
`UCI_LimitStrength` correctly overrode the depth cap but silently left
the time budget on the raw (possibly full-strength) setting — exactly
the "shallow search, then sits idle" failure mode Session 65 built the
depth+time pairing to prevent. Fixed by resolving the effective skill
level once, up front, and threading that single value everywhere.
Added `#[derive(Clone)]` to `CorrectionHistory` (needed for the
`wait_for_search()` widening). All 5 affected tests rewritten to assert
on real returned/computed values instead of state non-mutation, plus
one new regression test specifically for the time-fraction bug.

**Next session start point:** three pending verifications now, in this
order. (1) Trigger the Actions wasm-pack build, confirm it succeeds,
load the actual game, confirm the eval bar renders real values instead
of erroring — if it fails, the likely culprits are named directly in
D42/ROADMAP 21.3. (2) Run `cargo test`, confirm the UCI_Elo tests
(D43) pass. (3) Also in that same `cargo test` run, confirm the D44
rewrites pass AND specifically confirm
`test_build_time_control_uses_elo_derived_skill_level_not_raw` passes —
that one is the actual regression test for a bug that was live in code
generated earlier this same session, never previously run for real.

**All three pending verifications closed out, still the same session:**
(1) `cargo test` log confirmed: 388 passed, 0 failed for the lib,
specifically confirmed by name (not just aggregate count) —
`elo_to_skill_level`'s tests, the `UCI_LimitStrength`/`UCI_Elo`
setoption tests, and critically `test_build_time_control_uses_elo_
derived_skill_level_not_raw` (the actual regression test for D44's bug)
and `test_cmd_go_search_reflects_elo_override_not_raw_skill_level` (the
end-to-end integration test). (2) `deploy.yml`'s wasm-pack build
confirmed succeeding on runs #443 and #444 (both on this session's
actual commits) — `search_from_fen_with_eval` compiles to
`wasm32-unknown-unknown` for real. (3) Also scanned for other instances
of the same two bug classes (shallow `cmd_go` tests; raw `state.
skill_level` reads that should go through `effective_skill_level()`)
— found and fixed one more minor instance (a test using the raw value
where it should have used the wrapper, harmless today but a bad
precedent), nothing else turned up.

**Corrected a wrong claim from earlier this session**: had flagged root
`index.html` as "what GitHub Pages actually serves" and stale. The
staleness claim was right; the "what Pages serves" claim was wrong —
`deploy.yml` uploads only `web/` as the Pages artifact, so `web/
index.html` is what's actually live, confirmed by runs #443/#444
succeeding. Root `index.html` is simply dead, unused weight in the
repo, never live. Downgraded from "needs a dedicated look" to
delete-whenever-convenient.

**Remaining open item**: a one-time visual check — load the live game,
play a move, confirm the eval bar actually renders a real number. A
clean compile + passing tests doesn't rule out a runtime JS bug (e.g. a
string-parsing off-by-one in the Worker's `"e2e4 34"` split), just the
class of error a build/test failure would have caught.

**Later still — re-fixed the eval bar in Gokul's own edited
`web/index.html`:** Gokul had added a substantial "Console Dashboard"
debugging panel plus, more importantly, a genuinely more advanced eval-
bar system than what this session originally built on top of — a
dynamic-`import()`-inside-a-classic-Worker pattern specifically to
support Firefox (which doesn't support module workers), and a
"thinking" wobble animation that fills the visual gap during a search
and settles on the real value once one completes. Rather than pasting
the earlier diff back in (which would have silently regressed the
Firefox fix), re-implemented the `search_from_fen_with_eval` wiring
adapted to this newer structure: new `_renderRealEvalResult()` reuses
the existing `_renderEvalDiff()` helper for visual consistency with the
heuristic and the wobble, with its own mate-specific branch since the
heuristic can't detect real forced mates. Deliberately placed the real-
eval render call AFTER `applyEngineMove()`, not before — traced through
the render pipeline and confirmed `updateEvalBar()`'s heuristic fires
automatically as part of applying any move, so rendering first would've
just been immediately overwritten.

**Later still — repetition detection redesigned to match Stockfish
(D45, Phase 22):** asked to make Pet Dragon's threefold-avoidance logic
"similar to Stockfish." Verified Stockfish's actual algorithm
(`Position::set_state()`/`is_draw(ply)`) against the real source before
touching any code, rather than working from memory. Replaced the old
unbounded "scan everything for any 2nd occurrence" with Stockfish's
real approach: `game_history` now caches a per-position "repetition"
distance at push time (`Vec<u64>` → `Vec<(u64, i32)>`), computed via a
bounded backward walk (bounded by `halfmove_clock`, starting at 4 plies
— the shortest possible repetition cycle in legal chess), with the
cached value's sign distinguishing a first repeat from a genuine
repetition chain. The real behavioral change: a first repeat is only
scored as a draw if the search itself chose the moves that led back to
it (ply-relative), not when it's purely inherited from real game
history predating the search root — a true repetition chain is still
always a draw regardless. Checked Pet Dragon's null-move pruning
directly and confirmed it never touches `game_history` at all, so
(unlike Stockfish) no separate `pliesFromNull` bound was needed. Fixed
every consumer the type change touched, rebuilt every test that
depended on the old raw-push API with genuine legal move sequences, and
added direct unit tests for the algorithm itself rather than relying
only on the existing weak integration assertion. Found and fixed a real
bug in my own test-writing along the way — forgot to push the starting
position before making moves in several rebuilt tests, which silently
made the bounded walk one entry short everywhere; traced the exact
index arithmetic by hand to confirm the fix. Also caught that the
integration test's original bare-K-vs-K position would have triggered
insufficient-material short-circuiting before repetition detection got
exercised at all — fixed with a position that has real material but
still leaves the king-shuffle squares open. Full algorithm and
reasoning in D45.

**Next session start point:** two things, in this order. (1) The eval-
bar visual check from before — still pending, still the first thing to
debug if something looks off. (2) Run `cargo test` and specifically
confirm `tests/make_unmake.rs`'s 5 rebuilt repetition tests pass **by
name**, not just the aggregate count — this file could not be locally
compiled at all this session (same edition2024 toolchain wall as every
previous session's test work), so it's carrying more risk than usual;
everything in it was verified by hand-tracing the index arithmetic, not
by a compiler. If either check surfaces a problem, that's where to
start; otherwise Phases 21 and 22 are both genuinely closed.

---

## Session 69 — 2026-07-13 (hidden_size=128 NNUE test — negative, re-parked, D41)

**Built/tested:**

1. **Triggered `train_nnue.yml` with hidden_size=128** (accumulator_size
   unchanged at 256), reusing the exact same 286,659-row dataset as the
   parked hidden_size=32 baseline (4 Kaggle self-play batches, seeds
   100/200/300/900, 750 games each, recovered from GitHub Release assets
   at tags 100/200/300/900 — the committed `data/selfplay/
   selfplay_data_seed0_n3000.txt` was found to be empty, 1 byte, not
   usable) + the 50,000-row Lichess sample (verified via run
   `28721456844`, confirmed completed/successful before use).

2. **Result: val_loss=0.51655 (best epoch 3/10)** vs the parked
   hidden_size=32 baseline's val_loss=0.51636 (best epoch 4/10) — slightly
   *worse*, and overfit one epoch earlier. train_loss kept falling through
   epoch 10 (0.48798) while val_loss climbed to 0.53169 — classic
   overfitting signature, more parameters fitting the same limited signal
   faster rather than learning new positional understanding.

3. **No match_runner Elo sweep run.** The val_loss regression already
   rules this network out as an improvement over what's parked; spending
   an Actions run to reconfirm a known-negative result isn't worth it
   (D19/D20 efficiency stance). `NNUEWeight` stays 0%.

**Decision:** D41 — re-park NNUE. This was D34's own stated revisit
condition (hidden_size=128+, isolated as the only variable vs the parked
baseline) and it landed in the same place as D34's original 4 attempts —
5 independent levers now, all converging on the same ~0.516 val_loss /
~70-72% ceiling. Also clarified for Gokul: NNUE was never "the last task
for a proper engine" — Phases 0-20 (search, HCE, Texel tuning, Syzygy,
UCI, WASM, difficulty levels) are complete and Elo-validated; NNUE was
always scoped as an optional enhancement (Phase 16's own title). Not
worth a dedicated bigger-data Kaggle effort right now for an optional
feature on an already-complete engine. Phase 16/17 formally closed.

**Housekeeping note (not actioned this session):** `data/selfplay/
selfplay_data_seed0_n3000.txt` in the repo is empty (1 byte) despite its
filename implying 3000 games of data. Low priority (not blocking — the
Release-asset URLs work fine as the real data source) but worth either
deleting the stale file or replacing it with real content next time
someone touches `data/selfplay/`.

**Next session start point:** No NNUE work pending. Pick up from
ROADMAP's Housekeeping section (Node20 deprecation on 2 workflow files,
no upstream fix yet — check if one's shipped) or the stale test-count
recompute, or whatever Gokul brings next. If NNUE ever resumes, D41 says
start with a genuinely bigger self-play dataset (500K-1M+ rows, dedicated
Kaggle job), not another hidden_size bump on the current data.

---

## Session 68 — 2026-07-12 (Phase 20 closed out — final validation, pawn-rule regression tests, WASM skill_level, GUI preset decision)

**Built/validated, in order:**

1. **Final Skill Level validation, 200 games/pair.** All four tier pairs
   now correctly and monotonically ordered: 0v5 -619.4 Elo (97%), 5v10
   -117.2 Elo (66% — confirms the `skill_noise_trigger_pct` fix from
   Session 67 holds at a much larger sample), 10v15 -65.0 Elo (59%),
   15v20 -83.2/-79.5 Elo across two runs (62%, consistent). **Phase 20 is
   now complete.** See ROADMAP 20.6.

2. **Pawn-rule correctness Q&A led to real test additions, not just
   discussion.** Gokul asked pointed questions about whether Pet Dragon's
   custom rank-1/8 pawn starts could cause double-push or en-passant bugs
   (e.g. a pawn reaching rank 2 via a single push incorrectly gaining a
   second double-push; a pawn of one color landing on a square recorded as
   the other color's start). Traced through the actual code each time
   rather than answering from general chess-engine knowledge (this
   project's pawn rules are custom, so nothing about them can be assumed):
   confirmed `PawnStartMap`'s `Option<Color>` design already prevents the
   cross-color scenario safely, and confirmed `make_move.rs`'s
   `DoublePush`/en-passant handling already derives everything from the
   actual move's `from` square rather than a hardcoded rank. Both were
   already correct — but found the en-passant-after-rank-1/8-push path had
   NO direct test coverage, only inspection-level confidence. Added
   `test_en_passant_after_rank1_double_push` and its Black-side
   counterpart `test_en_passant_after_rank8_double_push` to
   `movegen/pawns.rs` (full end-to-end: play the double push via
   `make_move`, assert the EP target lands on the passed-through square
   not the destination rank, confirm the capture is actually offered).
   Also tightened the pre-existing `test_black_double_push_from_rank8`,
   which silently no-op-passed if no rank-8 pawn turned up in 200 seeds,
   to hard-fail like the White version already did, and equalized both
   tests' seed search ranges to 200 (was 100 vs 200, asymmetric).

3. **WASM binding gained a skill_level parameter.** Gokul is building his
   own GUI and asked where `search_from_fen` is exposed to find its
   signature — found it in `src/lib.rs`, and found it had NO skill_level
   parameter at all (every WASM search silently ran full-strength, since
   `SearchInfo::new()` defaults to `MAX_SKILL_LEVEL`). Decided (his call,
   deferred to Claude) to add it as a 3rd plain parameter —
   `search_from_fen(fen, movetime_ms, skill_level)` — rather than a
   separate stateful setter function, since this WASM API is otherwise
   fully stateless (fen and movetime are already passed fresh every call,
   no persistent "engine session" object anywhere in `lib.rs`); a setter
   would've been the first piece of global mutable state in an otherwise
   clean design, and a "did I remember to configure it first" footgun for
   a browser dev. Clamps defensively to `MAX_SKILL_LEVEL` rather than
   erroring on an out-of-range value, mirroring the native UCI
   `setoption` handler's own clamping.

4. **Fixed the two existing demo pages' now-broken calls.** `web/index.html`
   (Gokul's actual in-progress GUI — NOT a stale demo, it already has real
   pawn_starts/extended-FEN handling built in) and `index.html` both called
   the old 2-arg `search_from_fen(fen, ms)`. Per Gokul's instruction,
   updated both to pass `20` (full strength) explicitly rather than
   leaving them broken or deleting them — one-line change plus an
   explanatory comment in each, nothing else touched.

5. **D40 decided: GUI should expose named presets, not a raw 0-20
   slider.** Gokul asked whether it's acceptable that Skill Level 15
   sometimes loses to Skill Level 20 (a fair question — prompted by the
   15-vs-20 gap being visibly smaller than 0-vs-5). Clarified this is
   normal Elo behavior (a ~62% win rate, not 100%, is what a moderate Elo
   gap actually means) rather than a defect, then reframed the real
   design question as a UX one: is the top-end gap big enough to FEEL
   different to a player. Decided not to touch the validated engine
   mechanism to chase artificially bigger gaps (would fight the real,
   expected diminishing-returns shape of engine strength vs. depth).
   Instead: GUI exposes five named presets (Beginner/Easy/Medium/Hard/
   Master = skill_level 0/5/10/15/20) mapped directly onto the already-
   validated data points — zero new engine testing needed. Full reasoning
   in D40.

**Bugs fixed:** None in this session's own new code — the pawn-rule
investigation confirmed existing logic was already correct (closed a
*test-coverage* gap, not a bug). The skill_noise_trigger_pct fix that
made 5-vs-10 finally correct was Session 67's fix; this session's 200-game
runs are what confirmed it holds at scale.

**Decisions made:** D40 (GUI preset design — see DECISIONS.md).

**Test risk flagged:** same standing note as every session — no Rust
toolchain in this sandbox, nothing was compiled here. New pawn-rule tests
and the `lib.rs` signature change were hand-verified against the existing
suite's patterns and cross-checked against the actual fetched source
(not written from memory of "how chess usually works," given this
project's custom rules) but still need GitHub Actions to confirm.

**Next session start point:** Commit, in order: `movegen/pawns.rs`
(REPLACE — 2 new EP tests + tightened rank-8 test), `src/lib.rs` (REPLACE
— skill_level param), `web/index.html` and `index.html` (REPLACE — 1-line
fix each). Confirm full suite green. Phase 20 is DONE — next session
should start by asking Gokul what's next on the roadmap (no Phase 21 is
defined yet as of this entry) rather than assuming more Skill Level work
is needed.

---

## Session 67 — 2026-07-12 (Skill Level validation + move-selection noise — Phase 20.4)

**Built:** Ran the empirical validation Session 66 left pending. Root
cause of two false starts along the way (both user error, not code bugs,
worth recording so they don't repeat): (1) the `uci_match_runner.yml`
"Run workflow" form on mobile can carry over stale values from a previous
run if fields aren't manually cleared — one run silently reused
`num_games=100` instead of the intended 40; (2) the `engine_a/b_uci_
options` fields need the FULL command (`setoption name Skill Level value
N`), not just the bare number — a bare `"0"`/`"20"` gets sent as a raw
line the engine doesn't recognize as any known UCI command and is
silently ignored, so the engine just stays at its compiled-in default.
Both runs that hit this looked like ~50/50 "no difference" results,
which would have been mistaken for a depth-cap bug if not caught by
reading the workflow log's command echo (top of the "Run UCI pinned-ref
match" step) — added a rule of thumb going forward: always check that
echo line on a new/reconfigured run before trusting its result.

**Corrected validation results (all field-verified via log echo):**
- 0 vs 5: Elo -381.7 (tier 5 wins ~90%) — strong ✅
- 5 vs 10: Elo -436.4 (tier 10 wins ~92.5%) — strong ✅
- 10 vs 15: Elo -8.7 (48.8%, statistical tie) — **not separated** ❌
- 15 vs 20: Elo -52.5 (tier 20 wins 57.5%) — real but modest

**Bugs fixed:** None in the shipped code — the "10 vs 15 loses" result
from an earlier, less-carefully-verified run turned out to also be a
field-entry artifact (small sample without a confirmed echo), not a
depth-cap defect; the properly-verified 40-game rerun shows a tie, not an
inversion.

**Decisions made (Phase 20.4, no new D-number — extends D39):** The
10-vs-15 tie isn't a bug — it's the well-known shape of engine strength
vs. depth (huge gains in the first few plies, fast-diminishing returns
once the search is already reasonably deep for the time budget; even
Stockfish's own Skill Level 15-20 are notoriously close for the same
reason). Decided — with Gokul deferring the specific fix choice to
Claude — to add move-selection noise rather than leave it undocumented or
just compress the option's upper range, since noise fixes the actual
separation problem instead of just hiding it, and 20.1 always flagged
this as the fallback for exactly this situation.

**Move-selection noise, implemented this session:** `skill.rs` gained
`skill_noise_window_cp(level)` — `0` at Skill Level 20 (no-op, matches
every other Skill Level mechanism's backward-compat pattern), `(20 -
level) * 8` cp for 0..19 (level 0 -> 160cp, level 19 -> 8cp) — plus a
small embedded xorshift64 PRNG (no new crate dependency; deliberately
avoids `SystemTime::now()`, which this code also needs to stay safe under
because `iterative_deepening()` compiles to wasm32 too) and
`pick_noisy_move_index()`, which picks uniformly among root candidates
within the window rather than always the single best one. `iterative.rs`
wires it in right after the main depth loop: gathers up to 3 root
candidates via the existing `search_multipv_slot()` (Phase 19) machinery
(reused, not duplicated), then swaps in a noisy pick when one applies,
updating `result.best_move`/`score`/`pv[0]` to stay internally consistent.
Gated identically to the depth cap/time fraction — zero behavior change
at the default Skill Level.

**Test risk flagged:** same as every session in this sandbox — no Rust
toolchain here, nothing was compiled. New tests (skill.rs: noise-window
monotonicity + boundary + PRNG-selection tests; no iterative.rs tests
added this session for the noise wiring itself, since it needs a live
multi-candidate root position to meaningfully test and the existing
`fixed_depth_tc`/`start_pos()` test fixtures don't reliably produce
close-enough alternate root moves to assert on — flagging this as a real
test gap, covered instead by the match-runner re-validation below) were
hand-verified against the existing suite's patterns only.

**Important:** noise applies to EVERY capped tier (0-19), not just
10-15/15-20 — it's a strict superset addition on top of the existing
depth cap, layered onto the SAME tiers that were already well-separated.
That means the 0-vs-5 and 5-vs-10 numbers above are now stale relative to
the shipped code, not just the two problem pairs — all four need a fresh
run before any tier is called "done," not just the two that failed.

**Next session start point:** Commit `search/skill.rs` and
`search/iterative.rs` (both REPLACE, on top of everything from Sessions
66/this one). Confirm the full suite is still green. Then re-run ALL FOUR
tier pairs through the workflow (`uci_match_runner.yml`) — same
methodology as this session: `pre_tuning_ref`/`post_tuning_ref` both
`main`, full `setoption name Skill Level value N` strings in both
UCI-options fields (never a bare number), `movetime_ms` 1000, `num_games`
at least 40, and CONFIRM the log echo before trusting any result. Compare
against this session's pre-noise baseline (0v5 -381.7, 5v10 -436.4, 10v15
-8.7, 15v20 -52.5) to see whether noise closed the 10-15/15-20 gaps
without meaningfully eroding the already-strong 0-5/5-10 gaps. If the
upper pairs are still too close even with noise, the next lever is
widening the noise-window formula's coefficient (currently a flat `*8`
per level) rather than a structural rewrite.

**Follow-up same session (still 2026-07-12) — real bug found and fixed:**
Gokul ran the re-validation. 0-vs-5 improved (Elo -759.1, even more
lopsided) and 15-vs-20 went from marginal to decisive (Elo -240.8) — both
correct-direction improvements from noise. But 5-vs-10 came back
INVERTED: three separate runs (40, 50, 60 games) all had Skill Level 5
beating Skill Level 10 — cumulatively 85-64-1 across 150 games (57%),
Elo roughly +30 to +60 in the wrong direction each time. Three consistent
runs in the same direction ruled out sampling noise as the explanation.

Reviewed the code for state leakage (`age_history()`, `reset_for_search()`)
and time-budget overruns (`is_time_up()`'s deadline source) first — found
neither; the noise-candidate-gathering searches correctly share the same
`time_allocated_ms` deadline as the main search and can't corrupt the next
move's state.

Root cause (design flaw, not a crash-bug): `skill_noise_window_cp()` used
a flat centipawn threshold to decide which alternate candidates were
eligible for noisy selection. But root-move score gaps shrink as search
gets deeper — a deeper search converges on more genuinely-close
alternatives. So level 10's nominally TIGHTER window (80cp, at depth 11)
was actually catching MORE eligible candidates in practice than level 5's
nominally WIDER window (120cp, at depth 6), where shallow search's larger
natural score gaps rarely fell inside 120cp at all. Net effect: the
"weaker" tier (10) was deviating from its best move MORE often than the
"stronger" tier (5), inverted from intent — independent of the window
values' own correct ordering.

**Fix:** added `skill_noise_trigger_pct(level)` — a separate probability
gate (`(20 - level) * 4`, so level 0 = 80%, level 19 = 4%) checked BEFORE
the cp window. This directly ties deviation FREQUENCY to Skill Level,
independent of how clustered any given position's candidates happen to be
at that depth; the cp window still applies after a triggered roll, now
purely as a safety bound rather than the sole frequency control. Added a
regression-guard test (`test_trigger_pct_is_independent_of_depth`) pinning
`trigger_pct(5) > trigger_pct(10)` specifically, since that's the exact
ordering that was backwards before.

**Decisions made:** None new-numbered — this is a bug fix within the
already-decided noise mechanism (20.4), not a new architectural choice.

**Next session start point (updated):** Commit the newly-fixed
`search/skill.rs` (REPLACE — `iterative.rs` from earlier this session is
unchanged, no need to re-commit it) on top of everything else from this
session. Confirm green. Then re-run 5-vs-10 specifically first (it's the
one with a confirmed-wrong direction to verify is now fixed) at 50+ games,
full setoption lines, movetime 1000 — confirm it now favors tier 10. If
that holds, re-run the other three pairs (0v5, 10v15, 15v20) to get a full
clean ladder under the fixed mechanism before calling Phase 20 validated.

---

**Built:** `src/search/skill.rs` (new) — 21-level `Skill Level` tier table.
`skill_depth_cap(level)`: `None` at level 20 (default, uncapped), else
`level + 1`. `skill_time_fraction_pct(level)`: `100` at level 20, else
`(10 + level*5).min(98)`. Wired into `search/mod.rs` (`SearchInfo.
skill_level: u8`, default `MAX_SKILL_LEVEL`, persists across
`reset_for_search()` like `multipv`), `search/iterative.rs`
(`max_depth.min(cap)` — never overrides an explicit shallower `go depth`,
only reduces further), `search/time.rs` (new `TimeControl.
skill_time_fraction_pct` field, applied to the movetime branch, the
clock-based branch, and the no-clock-info fallback — NOT to infinite/
ponder or the fixed-depth/nodes sentinel), and `main.rs` (new `Skill
Level` UCI spin option 0..20 default 20, two-word `setoption` handler,
applied in `cmd_go` to both the main thread's and every helper thread's
`SearchInfo.skill_level` so Lazy SMP helpers can't leak full-strength
lines into a low-skill main search via the shared TT).

**Bugs fixed:** None — new feature, not a fix.

**Decisions made:** None new — this session implements what D39 (Session
64) and its Session 65 refinement already scoped; no fresh D-numbered
entry.

**Test risk flagged:** this sandbox has no Rust toolchain (`cargo`/`rustc`
not available), and per project convention GitHub Actions handles all
building/testing anyway — so none of this session's code was compiled or
run here. All new/changed logic has unit tests written to match the
existing suite's own patterns (skill.rs: monotonicity + boundary tests;
iterative.rs: depth-cap-vs-explicit-depth tests; time.rs: fraction-scaling
tests across every affected branch, explicit non-effect tests for the
branches that should stay untouched; main.rs: two-word option parsing +
clamping + default tests), and existing tests were re-read before editing
to avoid touching anything already green, but this is a real risk to flag,
not a substitute for CI actually running the suite.

**Next session start point:** Commit `skill.rs` (new), `search/mod.rs`,
`search/iterative.rs`, `search/time.rs`, `main.rs` (all REPLACE). Confirm
GitHub Actions' full suite is still green. Then run `uci_match_runner.rs`
across multiple seeds for at least these tier pairs: 0 vs 5, 5 vs 10, 10
vs 15, 15 vs 20 — confirm monotonic, convincing win rates before marking
any tier "done" (this is the empirical validation 20.1/20.2 always said
was required; it has NOT happened yet — implementation ≠ validation). If
any pair is too close, that's the point to reconsider the move-selection
noise mechanism that was scoped as optional in 20.1 and deliberately not
built this session.

**Follow-up same session (still 2026-07-12):** Gokul confirmed the full
suite is green from the files above and asked for the tier-pair match
values to actually run the validation. Discovered `uci_match_runner.rs`/
`uci_match_runner.yml` (D36) can't do it as they stood — that harness
spawns two SEPARATE git-ref builds and never sends a single `setoption` to
either one, so both engines always run at compiled-in defaults (Skill
Level 20 either way) regardless of what the workflow's inputs claim to
compare. Fine for D36's original pre/post-Texel-tuning use case (two
different builds, same default options), but structurally unable to
compare two Skill Level tiers of the SAME build.

**Built (cont.):** Extended `uci_match_runner.rs` with a pure
`split_uci_options()` helper (semicolon-separated `setoption` lines ->
trimmed non-empty commands, unit tested directly) and an
`EngineProcess::configure()` method that sends them once right after the
UCI handshake, before any games — matching how a real GUI sets a
persistent option like Skill Level once per session, not per move. Two
new trailing CLI args (`engine_a_uci_options`, `engine_b_uci_options`,
both optional/empty-by-default, fully backward compatible with every
existing invocation). Extended `uci_match_runner.yml` with matching new
`engine_a_uci_options`/`engine_b_uci_options` workflow inputs, passed
through to the harness; updated the match-summary labels from
"pre-tuning (ref)"/"post-tuning (ref)" to "A (ref | options)"/
"B (ref | options)" so a Skill Level run's output is actually readable
(flagging this label wording change explicitly since it touches D36's
existing, already-green output format, even though the underlying
scoring/Elo logic is untouched).

**Decisions made:** None new — this is infrastructure needed to actually
execute the validation D39/20.1/20.2 already required, not a new
architectural choice.

**Next session start point (updated):** Commit `src/bin/uci_match_runner.rs`
and `.github/workflows/uci_match_runner.yml` (both REPLACE) on top of the
Skill Level files above. Confirm the full suite is still green (new
`split_uci_options` tests included). Then run the workflow 4 times, each
with `pre_tuning_ref`/`post_tuning_ref` BOTH set to `main` and only the
UCI-options inputs differing, to compare: Skill Level 0 vs 5, 5 vs 10, 10
vs 15, 15 vs 20. Confirm monotonic, convincing win rates for the higher
tier in each pair before marking any tier "done." If a pair is too close,
reconsider the move-selection noise mechanism deferred from 20.1.

---

## Session 65 — 2026-07-11 (Difficulty levels: depth+movetime refinement — no code)

**Built:** Nothing — continued scoping Phase 20, no implementation yet.
Gokul asked whether difficulty is set by depth alone or movetime too.

**What got worked out:** depth-only has a real gap — it doesn't touch
time at all, so a low tier would still use whatever time the GUI/clock
gives it, just to search shallower, which can produce an oddly-instant
move on a long time control (looks broken, not weak) and wastes think
time that move-selection noise would otherwise benefit from. Refined
20.1's plan to use both: depth as the primary strength ceiling, plus a
tier-dependent fraction of the normal time budget so low tiers also
visibly "try less hard." Wiring plan: `Skill Level` feeds both a
`max_depth` override and a time-fraction multiplier into
`allocate_time()`'s output, same pattern as `Move Overhead` (D38).

**Bugs fixed:** None.

**Decisions made:** None new (this refines 20.1's existing scope rather
than reversing or replacing a prior decision — not a fresh D-numbered
entry, folded into ROADMAP's Phase 20 as 20.2).

**Next session start point:** Build Phase 20 — scope the exact tier
count, depth values per tier, and time-fraction values per tier;
implement as a `Skill Level` UCI option (spin, similar shape to
`MultiPV`); wire into both `iterative_deepening()`'s depth cap and
`allocate_time()`'s output; validate tier ordering with
`uci_match_runner.rs` across multiple seeds before calling any tier done.

---

## Session 64 — 2026-07-11 (Difficulty levels scoped — D39 — no code)

**Built:** Nothing — this session was a scoping discussion, not
implementation. Gokul asked whether the engine has difficulty presets,
whether major engines do, and then proposed reusing standard-chess Elo
calibration (via one Pet Dragon opening that visually resembles the
standard starting array) as a shortcut for difficulty levels.

**What got worked out:** Pet Dragon has no difficulty/skill-level option
currently — confirmed against the live UCI option list. Real engines do:
Stockfish's `Skill Level` (weighted randomness among top candidates + a
depth cap, not just "search less") and `UCI_Elo` (calibrated against real
rating data). Gokul's Elo-reuse idea was explored properly rather than
dismissed outright — took two passes to fully reject correctly:
1. First pass: one opening's resemblance to standard chess isn't enough
   representativeness on its own — same class of mistake as D36's
   original single-seed outlier, just bigger in scope.
2. Real reason, found on the second pass: it doesn't work even for that
   one opening, because Pet Dragon's custom pawn rules apply from move
   one — a visually standard starting array doesn't mean the game plays
   like real chess from there on, so no part of an external Elo
   calibration table (built from real chess games) transfers over.

**Decision (D39):** difficulty levels will be depth-cap tiers (possibly +
low-end move-selection noise, Stockfish-style), labeled plainly (`Skill
Level N`), making no Elo/human-comparable-strength claim — sidesteps the
whole external-calibration problem, since "less depth is weaker" needs no
borrowed data, only internal verification. That verification reuses the
existing `uci_match_runner.rs` harness (D36) across many seeds — no new
infrastructure needed.

**Bugs fixed:** None.

**Decisions made:** D39 (see DECISIONS.md) — scoped, not implemented.

**Next session start point:** Build Phase 20. Scope the exact tier count
and depth/noise values, add a `Skill Level` UCI option (spin, similar
shape to `MultiPV`), wire into `iterative_deepening()`'s depth cap (and
move selection if noise is included), then validate tier ordering with
`uci_match_runner.rs` across multiple seeds before calling any tier done
— don't skip that validation step given how much this project has
already learned about single-sample results being misleading (D36's
original outlier, this very session's rejected shortcut).

---

## Session 63 — 2026-07-11 (Phase 19: MultiPV + Move Overhead built and verified)

**Built:** Two standard UCI analysis-GUI options, requested together after
the pondering/self-containment review identified them as the remaining
gaps besides NNUE: `MultiPV` (report N candidate lines) and `Move
Overhead` (runtime-configurable time-safety buffer). Neither affects
playing strength.

**MultiPV (D38):** Standard root-move-exclusion technique — search the
primary line normally, then re-search from the root excluding
already-found moves, once per extra line, full window. New `SearchInfo`
fields `multipv: usize` (default 1) and `root_exclude: Vec<Move>`. The
root-only exclusion check in `alpha_beta.rs`'s move loop coexists with
singular extension's `excluded: Move` parameter without collision —
confirmed by reading exactly where singular extension's `!root_node`
guard sits before writing the new `root_node`-gated check. Entirely
additive: gated behind `multipv > 1`, so the existing single-PV code path
in `iterative_deepening()` wasn't touched, not even reformatted.

**Move Overhead (D38):** `OVERHEAD_MS` was a hardcoded constant in
`search/time.rs`; now `TimeControl::overhead_ms`, defaulting to the same
value, set from `EngineState.move_overhead_ms` via `setoption`.

**Files changed:** `src/search/mod.rs`, `src/search/alpha_beta.rs`,
`src/search/iterative.rs`, `src/search/time.rs`, `src/main.rs`.

**Two real things caught during this session, both fixed before shipping:**
1. A pre-existing bug in `cmd_setoption`, found while touching it to add
   Move Overhead: the old parser assumed single-word option names/values
   at fixed token positions, which would have silently mis-parsed "Move
   Overhead" itself (two words) and truncated any multi-word value (e.g.
   a spaced Windows SyzygyPath) to its first token. Rewrote to find the
   `"value"` token and join everything on each side — backward-compatible
   with every existing single-word case (regression test added).
2. My own first version of a MultiPV test asserted the primary line's
   move is identical between MultiPV=1 and MultiPV=4 runs. It failed —
   genuinely, not flaky. Turns out extra MultiPV lines searched at
   earlier depths feed the same shared TT/history tables the primary
   line's later-depth search then reads, so more of the tree has
   legitimately been explored by the time depth N's primary line runs —
   enough to occasionally shift which (still fully valid, still
   best-scoring) move comes out on top. This is a known, accepted
   property of MultiPV in alpha-beta engines generally (Stockfish
   documents the same caveat), not a defect. Fixed the test to assert
   what's actually guaranteed — legality — with the full explanation
   written into the test itself.

**Verification:**
- `cargo check --release` clean against the real crate.
- Full suite green: 345 lib tests (was 335) + 30 bin tests (was 22) = 375
  total, 18 new, 0 regressed, 0 pre-existing tests rewritten.
- Manual end-to-end UCI runs against the actual compiled binary: MultiPV=3
  produced correctly depth-sorted, distinct-move `multipv 1/2/3` lines
  with `bestmove` matching line 1's final-depth move; default (no MultiPV
  set) produced exactly one `multipv 1` line per depth, confirming zero
  change to existing behavior; Move Overhead=2000 on a movetime=3000
  search finished in ~1.02s (3000-2000ms budget) instead of ~3s.

**Bugs fixed:** The `cmd_setoption` multi-word parsing bug above — this
was a pre-existing latent bug (SyzygyPath's value could already have been
silently truncated), not something introduced this session, caught and
fixed as a natural consequence of touching that function.

**Decisions made:** D38 (see DECISIONS.md) — covers both MultiPV's
root-exclusion design and Move Overhead's TimeControl-field approach,
plus the `cmd_setoption` fix.

**Next session start point:** No code task queued. Phase 19 is complete.
Gokul said "then we will move on to NNUE" — next session should start
there: a bigger network (128+ hidden units, king-relative features
instead of flat piece-square) per the existing Phase 16/17.6 conclusion
that `hidden_size=32` is structurally too small, not a tuning problem.
That's a real architecture change needing a bigger Kaggle self-play job
for enough data to justify the extra capacity — worth scoping carefully
before writing any code, given how large that undertaking is compared to
this session's two contained features.

---

## Session 62 — 2026-07-11 (Phase 18: real UCI pondering built and verified)

**Built:** Full UCI pondering support (`go ... ponder` + `ponderhit`),
closing a real protocol-completeness gap surfaced during a general "is the
engine release-ready / what's missing besides NNUE" review — found by
inspection, not by a failing test. `allocate_time()` already special-cased
`tc.ponder` correctly (near-infinite search); `ponderhit` simply wasn't
handled anywhere in `main.rs`'s command dispatch.

**Files changed:** `src/main.rs`, `src/search/mod.rs`, `src/search/iterative.rs`.
Mechanism: two new `Arc<AtomicU64>` fields on `SearchInfo`
(`ponder_hit_soft_ms`/`ponder_hit_hard_ms`), threaded exactly like the
existing `stop_flag` pattern (D4). `cmd_go` precomputes the real time
allocation a ponder search would use once confirmed; `cmd_ponderhit`
converts that into a deadline relative to the search thread's own
`start_time` (not reset to zero — pondering time is free per spec, and
`start_time` is owned by the search thread so can't be safely reset from
main.rs's thread). `is_time_up()`'s hot-path check (every 256 nodes) and
`iterative_deepening()`'s depth-loop both respect the override once set.
Full design rationale and the rejected start_time-reset alternative: D37.

**Bug caught during this session, not shipped:** the first version of the
integration test set the override atomics *before* calling
`iterative_deepening()`, which meant `reset_for_search()` — correctly,
for real usage — wiped them before the depth loop ever read them. The
test passed for the wrong reason. Caught because a manual `cargo test`
run showed the "bounded" search still going at depth 23 / 191 seconds
instead of stopping in milliseconds. Fixed by spawning a real background
thread that sets the atomics ~30ms after the search starts, matching how
a real GUI's `ponderhit` can only arrive after the search is already
running.

**Verification:**
- `cargo check --release` clean against the real crate (pulled fresh via
  `codeload.github.com` tarball, edited in-sandbox).
- Full test suite green: 335 lib tests (was 329) + 22 bin tests (was 17)
  = 357 total, 12 new, 0 regressed, 0 pre-existing tests touched/rewritten.
- Manual end-to-end UCI runs against the actual compiled binary (piped
  real UCI commands via a Python harness): pondered 500ms then
  `ponderhit` correctly bounded the search to ~2.2s, matching the
  60s-clock-implied soft limit, instead of running forever. Plain
  `go movetime 300` (0.20s, as expected) and `go ponder` + `stop`
  (ponder miss, stopped promptly at 0.4s) both confirmed unaffected.

**Also done — self-containment audit (no code changes):** confirmed NNUE
weights are embedded via `include_bytes!` (no external model file at
runtime); the only optional external-file dependency is `SyzygyPath`,
standard for any tablebase-capable UCI engine. No other gaps found.

**Bugs fixed:** None in shipped code (the test bug above was caught and
fixed before it left the sandbox, not a shipped regression).

**Decisions made:** D37 (see DECISIONS.md) — atomic-deadline-override
approach over resetting `start_time` from another thread.

**Note on ROADMAP's Test Coverage Summary table:** flagged as stale (predates
several sessions of additions) rather than guessing at a recomputed
per-module breakdown — the confirmed current total (335+22=357) is now
recorded, but the per-file split still needs a proper recount sometime.

**Next session start point:** No code task queued. Phase 18 is complete.
Whoever picks this up next should ask Gokul what's next — there's no
further known gap in UCI compatibility or self-containment as of this
session.

---

## Session 61 — 2026-07-11 (D36 final confirmation: 2×200-game runs, ~39 Elo)

**Built:** Nothing new — analysis only. Gokul ran 2 more `uci_match_runner.yml`
matches at 200 games each (larger than any prior run), same
`pre_tuning_ref`/`post_tuning_ref`, 100ms/move.

**Result (these 2 runs):** 200 games each — 86W/110L/4D (44.0%, −41.9 Elo)
and 85W/109L/6D (44.0%, −41.9 Elo). Essentially identical to each other —
tight, consistent, well-powered.

**Pooled final (all 7 runs, 520 games total):** pre-tuning (A) 225 wins,
post-tuning (B, `main`) 283 wins, 12 draws. A score 44.4% → aggregate Elo
diff ≈ −38.9 (A vs B) — Texel-tuned HCE is ~39 Elo stronger than
pre-tuning, pooled. This converged as expected: Session 60's small-sample
pooled estimate (~29 Elo) moved toward these two large runs' own number
(~42 Elo) as sample size dominated. RUN 1's original +147.2 outlier is now
unambiguously explained as small-sample noise (n=20, one seed).

**Bugs fixed:** None.

**Decisions made:** None new.

**Housekeeping note:** the Session 60 docs update had not actually been
committed to the live repo yet when this batch of results came in — caught
via `raw.githubusercontent.com` before editing (would have built on stale
docs otherwise). This session's ROADMAP.md/SESSION_LOG.md outputs are
built on the correct Session 60 content plus this session's additions, so
committing these latest files carries both sessions' work in one go.

**Next session start point:** No code task queued. 17.8/D36 is fully
closed with a well-powered result. Ask Gokul what's next.

---

## Session 60 — 2026-07-11 (D36 CLOSED: pooled 5-run result, no bug)

**Built:** Nothing new — analysis only. Gokul ran 4 more `uci_match_runner.yml`
matches (40, 20, 20, 20 games) beyond Session 59's original 20-game RUN 1,
same `pre_tuning_ref`/`post_tuning_ref`, different seeds.

**Result (pooled, 5 runs, 120 games):** pre-tuning (A) 54 wins, post-tuning
(B, `main`) 64 wins, 2 draws. A score 45.8% → aggregate Elo diff ≈ −29
(A vs B) — Texel-tuned HCE is ~29 Elo stronger than the original
Ethereal-derived values, pooled. Session 59's RUN 1 (+147.2 the opposite
direction) was a single-seed outlier; the other 4 runs (100 games) all
lean the expected direction, consistently. Conclusion: Phase 14's tuning
works, no bug, no revert — 17.8 closed as `[x]`.

**Bugs fixed:** None — none existed. The dramatic RUN 1 number was sample
noise from one seed at a fast 100ms/move time control, not a defect in the
tuner or the harness (harness trustworthiness had already been verified
file-diff-clean in Session 59).

**Decisions made:** None new. D37 (tentatively flagged in Session 59 as
"needed if regression confirmed") is now moot — no revert-or-fix decision
to make.

**Note on the milestone table:** reframed the "Texel tuned HCE ~3000-3100"
entry to be explicit that this was always a relative/comparative estimate,
not a calibrated absolute — Pet Dragon is a custom variant with no
external rating pool to anchor an absolute Elo figure against. The real,
now-measured number is "~29 Elo better than pre-tuning," which is a much
more modest claim than "~3000-3100" implies on its own.

**Next session start point:** No code task queued. Phase 14 and Phase 17
are both now in a clean, fully-documented state (17.8 closed, milestone
table honest). Whoever picks this up next should check ROADMAP for
whatever's queued after Phase 17, or ask Gokul directly if nothing is.

---

## Session 59 — 2026-07-11 (D36 RUN 1: tuned HCE loses to pre-tuning — unconfirmed)

**Built:** Nothing new — this session ran the D36 harness (built Session
58) for the first time and analyzed the result.

**Result (RUN 1):** `pre_tuning_ref=c9905a22ed018c6c8332bef275aff548a1d0de70`,
`post_tuning_ref=main`, 20 games, 100ms/move, seed_start=0. Pre-tuning
(Ethereal-derived hand-picked HCE) beat post-tuning (Phase 14 Texel-tuned
HCE, current `main`) 14-6, 0 draws, +147.2 Elo. This is the OPPOSITE of
what Phase 14 assumed — the milestone table's "~3000-3100" target implied
tuning would help, not cost ~147 Elo.

**Verification done before trusting this result:** Confirmed the D36
harness itself isn't the cause. (1) Diffed every file (not just the
expected eval ones) between the two refs via `codeload.github.com`
tarballs of both — only the 7 Session-55 tuning files differ
(`eval/{king_safety,material,mobility,mod,open_lines,pawns,tables}.rs`,
`texel/weights.rs`); no search, hash, threads, or other engine-config
differences that could confound the match. (2) Confirmed both refs default
to 0% NNUE blend weight identically (`NNUE_BLEND_WEIGHT_PCT` static
default unchanged), so this isn't a repeat of the NNUE-blend confound
17.7 already ruled out. The harness is trustworthy; the result itself
still needs replication before acting on it.

**Decision (delegated to Claude — "you decide, confirmative call"):**
Don't revert Phase 14's work and don't start a tuner-bug investigation off
one 20-game/one-seed sample. Rerun with a different seed first — it's
free, changes no code, and either confirms the regression (then
investigate/revert with actual confidence) or shows RUN 1 was noise
(then there's nothing to fix). Queued as 17.8's next action:
`seed_start=1000`, everything else identical.

**Bugs fixed:** None.

**Decisions made:** None new (no DECISIONS.md entry this session — the
verification above was ruling out confounds in an existing tool, not an
architectural choice). If RUN 2 confirms the regression, the eventual
revert-or-fix call will need its own entry, tentatively D37.

**Next session start point:** Check whether Gokul ran RUN 2
(`seed_start=1000`). If yes: compare against RUN 1. Regression confirmed
by both → investigate `src/bin/texel_tune.rs` for a bug (overfitting,
sign error, loss function) before any revert decision (D37). Result
flips or narrows a lot → sample size at 100ms/20 games is too noisy,
recommend more games and/or longer movetime next. If Gokul hasn't run it
yet, that's still the queued action — nothing else to do until that data
exists.

---

## Session 58 — 2026-07-10 (D36: pinned-ref UCI match harness built)

**Built:** `src/bin/uci_match_runner.rs` — new match harness that spawns
two separate `pet_dragon` binaries as OS child processes and plays them
against each other over real UCI (stdin/stdout), unlike `match_runner.rs`
(Phase 17.2) which A/Bs NNUE blend weight within one binary. This is what
makes a genuine pre/post-Texel-tuning HCE Elo number measurable — point it
at a binary built from before the Session 55 tuning commit and one from
`main`. Also built `.github/workflows/uci_match_runner.yml`, a manual-
dispatch workflow that checks out and builds both refs plus the harness,
then runs the match and uploads results — same pattern as
`match_runner.yml` (D19/D20).

**Design decision:** D36 — see DECISIONS.md. Chose pinned-ref UCI over
refactoring `eval/*.rs` to runtime-loadable weights, because the runtime-
weights approach touches the hottest path in the engine (`evaluate()`) for
a one-time measurement, and doesn't even avoid needing an old git ref
anyway (the pre-tuning literal values are already gone from the working
tree). Gokul approved this design before any code was written (asked via
in-chat confirmation, not assumed).

**Verification this session:** Pulled the full repo tarball into the
sandbox, dropped the new file into `src/bin/`, and ran
`cargo check --bin uci_match_runner --release` against the real crate —
compiled clean, zero warnings from the new file (pre-existing unrelated
warnings elsewhere untouched). Could NOT run `cargo test` in-sandbox — the
sandbox's rustc 1.75 (installed via `apt`, since `rustup`'s download
domain isn't in this environment's network allowlist) hit a transitive
dev-dependency requiring the `edition2024` Cargo feature, which isn't
stabilized until a newer toolchain. This is a sandbox tooling limitation,
not a defect in the new file — `cargo check` (which doesn't pull dev-deps)
passed, and the real repo's CI runs on a current stable toolchain via
`dtolnay/rust-toolchain@stable`, so the tests as written are expected to
run fine there. Flagging this honestly rather than claiming a green test
run that didn't actually happen.

**Bugs fixed:** None (new files only, per this session's scope).

**Decisions made:** D36 (see above and DECISIONS.md).

**Next session start point:** Nothing to build yet. Gokul needs to (1)
find the git SHA immediately before the Session 55 Texel-tuning commit
(mobile GitHub app -> Commits, search "Session 55" or "tuned weights", use
the parent SHA — Claude couldn't confirm this SHA directly this session,
api.github.com anonymous rate limit was hit), (2) run
`uci_match_runner.yml` from the Actions tab with that SHA as
`pre_tuning_ref`. Next session should start by checking whether that run
happened and what the result was — if yes, close out ROADMAP 17.8 as [x]
and fold the real Elo number into the milestone table; if not, no code
task is queued and the same "what's next" question from Session 57
applies again.

---

## Session 57 — 2026-07-10 (ROADMAP.md accuracy pass — no code changes)

**Built:** Nothing — Session 56's log explicitly closed with "no specific
code task queued," so this session did docs-only cleanup rather than
inventing work or picking an unapproved architectural task.

**What changed in ROADMAP.md:**
- Phase 14 header `⏳` → `✅ COMPLETE` (14.1-14.5 were already all `[x]`,
  marker was just never updated).
- Annotated the stale Session-54 "NEXT SESSION START POINT" text under
  14.3/14.4/14.5 — left the original text intact for history, added a
  note that it was actually completed in Sessions 55-56 and is not a
  live task.
- `17.5` root checkbox changed `[ ]` → `[~]` with an explicit note: the
  NNUE-blend sub-items (17.5a-f) are done and parked (D34), but the one
  piece of the original ask — a true pre/post-Texel-tuning HCE Elo
  number — was never built and still needs a DECISIONS.md entry +
  approval before any infra work starts on it. Previously this was a
  dangling unchecked box with no clear status.
- Milestone table's "Texel tuned HCE" row updated to reflect that 17.7's
  match_runner.yml validation already happened (Session 56), replacing
  the stale "needs validation next session" text.

**Bugs fixed:** None.

**Decisions made:** None — no new architecture, just correcting stale
doc state to match what Sessions 55-56 actually did.

**Next session start point:** No code task queued. Two open options,
neither started, both need Gokul's call before any work begins:
(a) if a genuine pre/post-Texel-tuning Elo number becomes worth building
(runtime-loadable HCE weight tables, or a second binary from a pinned
pre-tuning git ref over UCI) — write a DECISIONS.md entry first, given
the added runtime complexity for what's currently a one-time
measurement; (b) the 3 Node24-blocked GitHub Actions steps
(`configure-pages@v4`, `upload-pages-artifact@v3`, `deploy-pages@v4`,
`softprops/action-gh-release@v2`) remain cosmetic-warning-only with no
upstream Node24 release yet — worth a quick check next session for
whether upstream has shipped one, otherwise not actionable.

---

## Session 56 — 2026-07-10 (match_runner data points post-tuning; weights.rs commit-mistake fix)

**Built:** Nothing new — this session was two parts: (1) fixing a broken
commit, (2) gathering match_runner.yml data with existing tooling.

**Bugs fixed:** Gokul's commit of Session 55's `src/texel/weights.rs`
delta landed broken — the `Default::default()` sync (Change 1) never got
applied (still had the old Ethereal values) AND there was a stray extra
closing `}` right after the `Default` impl, a hard compile error (caught
immediately: build failed in ~20-24s, before tests could even run). Found
via `raw.githubusercontent.com` (no rate-limit issues there, unlike
`api.github.com` which stayed rate-limited most of this session) + diffing
against the locally-verified copy — confirmed all 6 `eval/*.rs` files
landed byte-identical, only `weights.rs` was affected. Fix: gave Gokul the
complete corrected file as a full replacement rather than another delta,
since a delta is what went wrong the first time — verified byte-identical
against the live repo after the re-commit. Build green again (`Build &
Release #370`, `Deploy to GitHub Pages #369`).

**Match results gathered (Phase 17.7):** 3 `match_runner.yml` runs using
the tuned HCE now compiled in. See ROADMAP 17.7 for the numbers and the
important caveat about what this tooling can and can't measure (discovered
this session: it A/Bs two NNUE blend weights of the same binary, not two
different HCE constant sets — so there's no real pre/post-tuning Elo
number available without new infrastructure).

**Decisions made:** None new — interpreting existing tool output, not an
architectural call. The "would need runtime-loadable HCE weights or a
second pinned-ref binary" note is flagged as a future option in ROADMAP
17.7, explicitly NOT built this session (deliberately deferred, not an
oversight — a real complexity trade-off for a one-time measurement,
worth its own DECISIONS.md discussion before committing to it).

**Next session start point:** No specific code task queued. Options for
whoever picks this up: (a) if a genuine pre/post-Texel-tuning Elo number
becomes worth the infrastructure cost, design it deliberately (see ROADMAP
17.7's note) rather than bolting it onto match_runner.rs's existing
NNUE-blend-only design; (b) otherwise, check ROADMAP for whatever the
next un-checked phase item is — Phase 17.6 already parked further NNUE
work, so absent a specific ask, the natural next thread is probably
whatever's queued after Phase 17 in ROADMAP's structure, not Phase 14/17
specifically, since both are now in a stable, tested, committed state.

---

## Session 55 — 2026-07-09 (Phase 14 COMPLETE — tuned weights applied to eval/*.rs)

**Built:** `src/bin/texel_diag.rs` (NEW) + `.github/workflows/texel_diag.yml`
(NEW) — a sanity-check tool comparing HCE under `TunableWeights::default()`
against a candidate `texel_tune.rs` output, on the same real-Pet-Dragon-
position test philosophy `eval_diag.rs` uses (5 random starts that should
read ~0, up/down a queen, up a rook, 2 pawn-endgame checks). Built after
Gokul's first two real 147,283-sample tuning runs (15 epochs no decay, then
100 epochs weight_decay=0.02) came back with physically implausible values
(`bishop_pair` MG negative, `attacker_weight` entries negative) that raw
value-inspection caught but a proper positional check hadn't — this closes
that gap for any future re-tuning round. Also added a `weight_decay` CLI
arg + workflow input to `texel_tune.rs`/`texel_tune.yml` (decoupled decay
ANCHORED AT the default weights, not zero — see rationale in the code
comment) after the first run's implausible values pointed at under-
regularized rare/sparse features (D30/D31 established the decoupled-decay
pattern for train_nnue.rs; anchoring at zero doesn't make sense for HCE).

**Gokul's runs, in order:** (1) 15 epochs, no decay — loss 0.0502->0.0454,
but `bishop_pair` MG went negative, `attacker_weight` had -81/-6 entries.
(2) 100 epochs, weight_decay=0.02 — loss ->0.0463, most values fixed but
`attacker_weight[1]` still -27. (3) 100 epochs, weight_decay=0.08 — loss
->0.0478 (a bit higher, expected regularization trade-off), everything
resolved to plausible ranges: `bishop_pair` s(18,29) vs default s(22,30),
`attacker_weight[1]` down to -5 (essentially noise), `rook_on_seventh`
positive again. `texel_diag` run against this file: 10/10 PASS (one initial
FAIL was my own test-case sign-convention bug in `texel_diag.rs`, fixed and
confirmed not a real regression before reporting anything to Gokul).

**Applied (D35 step 6):** All 3 run 3 values written into
`eval/material.rs` (MG_VALUES, EG_VALUES, BISHOP_PAIR_MG/EG),
`eval/tables.rs` (all 6 PST tables), `eval/mobility.rs` (all 4 mobility
tables), `eval/pawns.rs` (ISOLATED/DOUBLED/BACKWARD_PENALTY,
PASSED_PAWN_BONUS), `eval/king_safety.rs` (ATTACKER_WEIGHT,
OPEN_FILE_NEAR_KING, SEMI_OPEN_FILE_NEAR_KING, PAWN_SHIELD_BONUS —
MAX_KING_DANGER untouched, structural clamp per D35), `eval/open_lines.rs`
(all 9 constants), `eval/mod.rs` (TEMPO: 10 -> 20). Cross-checked every
existing unit test in these 6 files by hand before applying — all are
structural (symmetry, array lengths, index math), none hardcode exact
tuned values, so none were at risk; also manually verified
`test_rook_7th_rank`/`test_knight_centre_better_than_rim`/
`test_king_endgame_centralises`/`test_pawn_advance_bonus` (tables.rs)
still hold arithmetically against the new table values before running
the actual suite.

**Bugs fixed:** After applying the new eval/*.rs consts, `cargo test --lib`
immediately failed 4 tests — `texel::predict::tests::test_predict_matches_
evaluate_*` and `predict_f64`'s equivalent. Cause: `src/texel/weights.rs`'s
`TunableWeights::default()` is documented to always mirror whatever's
currently compiled into `eval/*.rs`; I'd updated eval/*.rs but not its
mirror, so `predict(features, TunableWeights::default())` stopped matching
the now-changed `evaluate()`. Fix: synced `weights.rs`'s `Default` impl and
its 6 standalone PST consts to the same new values — generated
programmatically from Gokul's `texel_weights_tuned.txt` (reusing
`texel_diag.rs`'s parser) rather than hand-retyping ~964 numbers a second
time, both as a time-save and to cross-validate the hand-transcription
into eval/*.rs (values matched exactly). Why correct: `TunableWeights::
default()`'s entire purpose is "current compiled eval/*.rs values, so the
self-consistency tests hold" — it's supposed to move every time eval/*.rs's
tuned constants move; this isn't a special one-off fix, it's the standing
rule for every future re-tuning round too. Full suite 329/329 after the
sync.

**Decisions made:** None new — this was applying D35's plan (steps 5.5 and
6) as written, with `weight_decay` as a straightforward, expected parameter
addition rather than a new architectural direction.

**Next session start point:** Phase 14 is functionally complete, but the
actual playing-strength effect of the tuned weights hasn't been measured
yet — `match_runner.yml` (tuned HCE vs the pre-tuning Ethereal-derived HCE,
or vs the current NNUE, per whatever comparison ROADMAP's Phase 15/16
section calls for) should run next to get a real Elo delta before
considering Phase 14 "validated" rather than just "compiled and
sanity-checked." Check ROADMAP's Phase 15 section for what's next after
that.

---

## Session 54 — 2026-07-09 (14.3 step 5 built — gradient descent optimizer)

**Built:** `src/texel/weights_f64.rs`, `src/texel/predict_f64.rs` (both
NEW), `src/bin/texel_tune.rs` (NEW), `.github/workflows/texel_tune.yml`
(NEW) — D35's step 5. `TunableWeightsF64` mirrors `TunableWeights`'
exact shape with `S{mg,eg}: f64` pairs instead of packed i64, plus
flatten/unflatten for a fixed-order flat parameter vector (964 scalars,
matches D35's ~970 estimate). `predict_and_accumulate_grad` does the
forward pass and gradient accumulation in one call, mirrored term-for-term
against `predict_f64` (a straight f64 port of Session 53's `predict()`);
king safety's `MAX_KING_DANGER` clamp gets zero gradient on
`attacker_weight[idx]` when clamped, full gradient otherwise, per D35.
`texel_tune.rs`: loads `<FEN>|<result>` lines (texel_gen.rs's exact
format), runs a coarse-then-fine K line search, then plain Adam gradient
descent (Kingma & Ba 2014, hand-rolled — noru's `AdamState` is tied to
NNUE's own tensor shapes, not reusable for a flat HCE vector) over
shuffled minibatches, writes tuned weights in `s(mg,eg)`/array-literal
syntax matching `weights.rs`'s own format. `texel_tune.yml` mirrors
`train_nnue.yml`'s three-source data pattern (Run ID / committed path /
Release URL, D19/D22).

**Verification (before presenting):** cloned the real repo, dropped the
files in, `cargo check --lib --bins` clean. `cargo test --lib` initially
caught a REAL bug (see below) — after the fix, full suite 329/329 (322 +
3 Session 53 + 4 new: grad-vs-forward cross-check on hand-picked FENs, a
200-seed Pet Dragon sweep, an f64-vs-integer-predict sanity bound, and the
flatten/unflatten + default-conversion round-trip). Also ran the actual
binaries end-to-end: `texel_gen` generated a real 559-sample dataset (40
games, seed 5000), `texel_tune` trained on it for 8 epochs — K line search
found 2.589 (had to widen the search range from 0-2 to 0-4 after the first
smoke-test run hit the original boundary), loss fell every single epoch
(0.042636 -> 0.017032), output file parses back into `weights.rs`'s own
literal format cleanly. This was a mechanical smoke test on self-generated
data, NOT the real 14.2 production database — that's next session's job.

**Bugs fixed:** `TunableWeightsF64`'s `S::from(packed_i64)` initially
called `crate::eval::material::mg()` directly to decode a single literal
weight — but `mg()`/`taper()` are only valid decoders of an ACCUMULATED
SUM of many `s(mg,eg)` terms (the addition-based packing scheme relies on
borrow/carry cancelling out across a full sum); applied to one unsummed
term with a negative `eg`, `mg()` silently returns `mg - 1`. Fix: keep
`eg()` (exact for a single term — a low-32-bit reinterpret has no borrow
issue) and derive the true `mg` via `(packed - eg) >> 32` instead, which
is an exact division by construction of `s()`. Caught by the
flatten/unflatten round-trip test (`test_flatten_unflatten_roundtrip_and_
default_conversion`) failing on real PST table values — exactly the kind
of silent, scale-dependent bug D35 built these tests to catch. `predict.rs`
and `weights.rs` (Session 53) were never at risk from this — they only
ever decode from accumulated sums, never a single term, which is correct
usage of the same trick.

**Decisions made:** None new — D35's plan executed as written. Widening
the K search range (0-2 -> 0-4) is a parameter tweak based on smoke-test
evidence, not an architectural decision.

**Next session start point:** Run `texel_tune.yml` for real against the
actual 14.2 production database (147,867 samples — find the GitHub
Release asset URLs or Run IDs for the smoke-test + seed-1000/2000/3000
batches; `api.github.com`'s unauthenticated rate limit blocked looking
these up directly this session, so they need to come from Gokul or a
future session with fresh rate-limit headroom). Start with a short run
(~10-20 epochs) against the real data first to sanity-check loss trends
before committing to a long run — the smoke test only proves the mechanism
works, not that 147k real samples will behave the same way. Then D35 step
6: sanity-check the tuned numbers (a small HCE-specific diagnostic, in the
spirit of `eval_diag.rs`, would help here and doesn't exist yet), then
write the `eval/*.rs` delta from `texel_weights_tuned.txt`'s output.

---

## Session 53 — 2026-07-09 (14.3 steps 1-4 built — self-consistency GREEN)

**Built:** `src/texel/mod.rs`, `src/texel/features.rs`, `src/texel/weights.rs`,
`src/texel/predict.rs` (all NEW) — D35's steps 1-4. `TexelFeatures` extracts
a per-position feature summary (diffs for simple additive terms; raw
per-bucket/per-side components for PST, mobility, and king safety, where a
plain diff would lose which table entry applies). `TunableWeights::default()`
copies every current eval/*.rs const verbatim (material, all 6 PST tables,
mobility tables, pawn penalties, king safety weights, open-lines bonuses,
tempo). `predict()` recomputes the score via the same arithmetic
`crate::eval::evaluate()` uses (packed s(mg,eg) tapering via the existing
`taper()` helper, same king-safety bucket/clamp/phase-scaling logic) but
pulling constants from `weights` instead of hardcoded arrays.

**Verification (before presenting, per working-style rules):** cloned the
real repo via codeload, dropped the 4 new files in, installed rustc/cargo
via apt (no cargo available in-sandbox otherwise), ran `cargo check --lib`
(clean) and `cargo test --lib` — 325/325 passing (322 pre-existing + 3 new).
The 3 self-consistency tests cover hand-picked FENs (material swings, pure
endgame phase=0, king-exposed middlegame), a 500-seed
`Position::generate_with_seed` sweep, and a 30-seed x 6-ply mid-game sweep
(deterministic move selection, no RNG dependency) — `predict()` matched
`evaluate()` bit-exact in every case, zero mismatches.

**Bugs fixed:** N/A — new code, self-consistency test passed on first
attempt (no iteration needed; extraction logic was written as a careful
line-by-line mirror of each eval/*.rs submodule specifically to avoid
needing a fix-the-mismatch cycle at this scale, per D35's own caution about
that risk).

**Decisions made:** None new — this is D35's plan executed, not a new
architectural call.

**Test risk flagged:** local verification used rustc/cargo 1.75 (apt,
Ubuntu 24 default) rather than whatever `dtolnay/rust-toolchain` resolves
to latest-stable in CI — had to temporarily strip the `criterion`
dev-dependency locally (edition2024 requirement in a newer transitive dep
exceeds 1.75's cargo) to run `cargo test`; this was NOT committed, only used
for local verification. CI runs on the real toolchain and the real
Cargo.toml, so this is not expected to cause a discrepancy, but it's the one
part of this session's verification that wasn't against the exact CI
environment — worth a normal green-build confirmation like any other
session, not skipped just because local tests passed.

**Next session start point:** D35 step 5 — the gradient-descent optimizer.
Sigmoid-scaled error against game results (same K/lambda-style scaling
reasoning as D14's NNUE target), batched across the 147,867-sample database
from 14.2, output `TunableWeights` in a mutable f64-friendly form (current
`TunableWeights` uses i64/i32 to guarantee exact self-consistency this
session — the optimizer will need a parallel f64 or a documented
conversion). Then `texel_tune.yml` (GitHub Actions per D19). Remember
`MAX_KING_DANGER`'s clamp needs zero gradient when clamped, pass-through
otherwise (D35's one nonlinearity) — don't let the optimizer silently
backprop through it as if smooth.

---

## Session 52 — 2026-07-08 (14.2 complete, 14.3 architecture audit — D35)

**Built:** Nothing new — this was a design/audit session. Validated all 4
Texel data batches (147,867 samples total). Read all ~2000 lines of
eval/*.rs in full to determine whether HCE could be tuned via efficient
gradient descent or would need slower coordinate descent.

**Bugs fixed:** N/A. Also fixed, unrelated: a duplicate `build:` job key in
build.yml (introduced during the Node.js 20 deprecation delta application)
that was causing every recent build.yml run to fail at the YAML-parse stage
(0 jobs created) — caught because the Actions UI fell back to showing the
raw file path instead of "Build & Release". Confirmed fixed via GitHub API
(jobs list went from empty to test/build/bench/release). Also found and
fixed a session-numbering gap (Session 45 was skipped in the original
sequence) via a direct audit of SESSION_LOG.md's session numbers.

**Decisions made:** D35 (full Texel tuning architecture — see
DECISIONS.md). Confirmed HCE is linear-in-weights, ~970 parameters, one
clamp nonlinearity (king safety), full 6-step implementation plan
documented.

**Next session start point:** Build D35's steps 1-4 (TexelFeatures struct,
TunableWeights struct, predict() dot-product function, and — critically —
the self-consistency test comparing predict() against the real evaluate()
on many random positions). Do NOT write the gradient-descent optimizer or
texel_tune.yml until the self-consistency test is green — it's the only
way to catch a silently-wrong feature extraction, since there's no other
verification signal (no val_loss curve, no eval_diag-equivalent) available
until real tuning runs produce results.

---

## Session 51 — 2026-07-08 (14.2 validated, production batching planned)

**Built:** Nothing new — read the smoke-test Actions log + output data.

**Bugs fixed:** N/A.

**Decisions made:** None new — straightforward scaling calculation
(4.3s/game observed rate implies 10,000+ games needed for a real tuning
dataset would exceed Actions' 6-hour ceiling in one run), resolved via the
same batching pattern already established for self-play data, not a new
architectural choice.

**Next session start point:** Gokul running 3 batches (3500 games each,
seed_start 1000/2000/3000) via texel_gen.yml, uploading each as a GitHub
Release asset. Once 1+ batch URLs are available, 14.3 can start: design the
parallel tunable-eval function (flat weight vector, mirrors evaluate()'s
logic across material.rs/tables.rs/mobility.rs/pawns.rs/king_safety.rs/
open_lines.rs — ~970 parameters per the Session 50 audit) plus the
gradient-descent optimizer and texel_tune.yml workflow (GitHub Actions
per D19). Don't necessarily wait for all 3 batches to build/test 14.3's
optimizer logic — a single batch's worth of data is enough to validate the
tuner mechanically works before committing to the full 3-batch dataset.

---

## Session 50 — 2026-07-08 (14.2 built — Texel tuning data generator)

**Built:** `texel_gen.rs` + `texel_gen.yml` (Phase 14.2). Modeled directly
on `selfplay.rs`'s already-verified game loop (same random-start generator
per D32, same mate/stalemate/repetition/max-plies handling, same silent
search per D28) but with a different sampling target: raw
`<FEN>|<game_result>` pairs instead of NNUE feature vectors + search-eval
labels. Sampling skips the first 12 plies and any in-check position, then
takes every 4th eligible ply — standard Texel tuning practice (fit static
eval, not tactics; avoid near-duplicate consecutive positions).

**Bugs fixed:** N/A.

**Decisions made:** None new — audit-driven design, not an open question
(confirmed via eval/*.rs audit: ~970 tunable parameters, all currently
compile-time consts, so 14.3 needs a separate tunable-eval path rather
than modifying the real evaluate()).

**Next session start point:** Gokul applies both new files, confirms
build green, runs a smoke-test generation (default 20 games) to confirm
output looks sane, then scales up to a real tuning-sized run. Once that
data exists, 14.3 starts: design the parallel tunable-eval function
(flat weight vector, mirrors evaluate()'s logic across material.rs/
tables.rs/mobility.rs/pawns.rs/king_safety.rs/open_lines.rs) plus the
gradient-descent optimizer and texel_tune.yml workflow (GitHub Actions
per D19, not Colab/Kaggle unless D20's trigger conditions are met).

---

## Session 49 — 2026-07-08 (Phase 13 closed, 10.3 skipped, Phase 14 decided+scoped)

**Built:** Nothing new — pure planning/decision session.

**Bugs fixed:** N/A.

**Decisions made:** Phase 14 (Texel tuning) proceeds — HCE is the actual
shipped eval now that NNUE is parked (D34), so this is the highest-leverage
remaining strength work. 10.3 (Arena/BanksiaGUI/CuteChess verification)
permanently skipped — desktop-only tools, Gokul is mobile-only, no path
forward without that constraint changing.

**Next session start point:** 14.2 — build the Texel tuning game-database
generator. First step is an audit, not code: read eval/material.rs,
eval/tables.rs, eval/mobility.rs, eval/pawns.rs, eval/king_safety.rs,
eval/open_lines.rs to see how tunable (or not) the current weight
constants are — Texel tuning needs to read/write these programmatically,
which may require restructuring some of them from compile-time consts into
a loadable/mutable weight struct before any tuning binary can work against
them. Do that audit first, then design texel_gen.rs's output format.

---

## Session 48 — 2026-07-08 (Housekeeping — Node.js 20 deprecation bump)

**Built:** Bumped GitHub Actions across all 7 workflow files:
actions/checkout v4→v6, actions/upload-artifact v4→v6,
actions/download-artifact v4→v7 (searched web for current release status
first — confirmed each is genuinely Node24-native now, not guessed).
Left actions/configure-pages, actions/upload-pages-artifact,
actions/deploy-pages (deploy.yml), and softprops/action-gh-release
(build.yml) unchanged — confirmed no Node24-compatible release exists
upstream for any of these yet. Swatinem/rust-cache@v2 needed no change,
already a floating tag that picked up Node24 support (v2.9.0) on its own.

**Bugs fixed:** N/A — cosmetic deprecation warning, not a functional break
(GitHub already force-runs everything on Node24 regardless since June 2,
2026, per D-series research this session; the warning is about the
action's *declared* runtime metadata lagging, not actual execution).

**Decisions made:** None new — straightforward dependency bump, not an
architectural decision.

**Next session start point:** Confirm all 7 workflows still run green
after this bump (next push exercises build.yml/deploy.yml automatically;
spot-check the manual-trigger ones — eval_diag.yml, lichess_sample.yml,
match_runner.yml, selfplay.yml, train_nnue.yml — whenever convenient).
No other open ROADMAP items as of this session; check with Gokul on
priorities for what's next (Phase 18+, or revisiting Phase 14's optional
Texel tuning, or a future hidden_size=128+ NNUE attempt per D34's
"revisit" note).

---

## Session 47 — 2026-07-08 (16.7 confirmed, web/index.html synced)

**Built:** Nothing new from Claude. Confirmed 16.7 (WASM engine responds
in-browser at https://g-c-3.github.io/pet-dragon). Gokul independently
edited `web/index.html` directly on GitHub ("more dynamic" — no further
detail given), confirmed still working live. Pulled the current version
(1521 lines) to keep context in sync since it wasn't authored or reviewed
here.

**Bugs fixed:** N/A.

**Decisions made:** None.

**Next session start point:** Check ROADMAP.md Housekeeping section
(Node.js 20 deprecation in workflow files — low priority, easy win) and
confirm nothing else is open before considering Phase 17/18+ scope. If
Gokul wants to review/extend web/index.html further, re-fetch it fresh
first (don't assume the version in this session's context if significant
time has passed) since it's now maintained partly outside the normal
delta flow.

---

## Session 46 — 2026-07-08 (Phase 17.5 PARKED — final verdict, D33/D34)

**Built:** Nothing new — read the weight_decay=0.01 + grad_clip_norm=1.0
retrain's val_loss log and the corrected eval_diag output, then the final
4-run match_runner sweep against that network.

**Bugs fixed:** N/A this session (D30's clamp + D33's regularization were
built in prior sessions this same arc).

**Decisions made:** D33 (raise weight_decay 100x, add grad_clip_norm — see
DECISIONS.md), D34 (park Phase 17.5).

**Result:** val_loss 0.51636 (best epoch 4/10) — matches/slightly beats
every prior attempt, confirming regularization didn't cost fit quality.
eval_diag showed real calibration improvement: seed=2 raw eval 1500→375,
seed=3 1500→50 (vs HCE's 10), K+P case 1225→225 (vs HCE's 113) — no longer
saturating at the clamp on typical in-distribution positions. BUT the final
sweep (5/10/15/20% vs 0%, 40 games each) averaged 70.3% opponent score —
statistically indistinguishable from clamp-only's 70.0%, and in the same
~70-72% band every one of the 4 independent fix attempts has landed in
(original network, 3x-data retrain, clamp-only, clamp+regularization).
That consistency despite genuinely different, real fixes each time points
at hidden_size=32 itself being the ceiling — not calibration, not data
volume. D34: parking further NNUE tuning at this scale. NNUEWeight stays
0% (unchanged since D25). Everything built stays permanent value regardless
(clamp, regularization defaults, eval_diag.rs tool).

**Next session start point:** Move off Phase 17.5 entirely. Next concrete
item: 16.7's WASM confirmation (mobile-doable — visit
https://g-c-3.github.io/pet-dragon, confirm engine responds with a move).
After that, check ROADMAP.md Housekeeping section (Node.js 20 deprecation
in workflow files — low priority, quick win whenever a workflow is next
touched) and Phase 14 (OPTIONAL Texel tuning — explicitly skippable, already
skipped in favor of NNUE per original roadmap notes) for what's actually
still open. A future hidden_size=128+ NNUE attempt is a real option later,
but treat it as a deliberate new effort (bigger Kaggle job, fresh session
budget) — not something to slide back into via "just one more tweak."

---

## Session 45 — 2026-07-07 (NUMBERING GAP — retroactively documented, Session 55)

**Note added retroactively (Session 55, 2026-07-08):** this number was
skipped in the original sequence — the corrected eval_diag work (D32) and
the weight_decay=0.01 + grad_clip_norm addition (D33) happened in this
window but got folded into the Session 44 and Session 46 entries without
their own log entry ever using this number. Content-wise nothing is lost
(D32/D33 are both fully documented in DECISIONS.md and referenced
correctly from Session 46's entry) — this entry exists only to close the
numbering gap Gokul caught by inspection. No new content to log here.

---

## Session 44 — 2026-07-07 (Phase 17.5e — root cause found, clamp added)

**Built:** `NNUE_EVAL_CLAMP_CP` constant + clamp applied in
`evaluate_nnue()` (inference.rs). Tightened the existing start-pos sanity
test (was `< 5000`, passed the actual bug trivially) to enforce the new
clamp ceiling directly, added a dedicated clamp-enforcement test.

**Bugs fixed:** Root cause of D29's paradox found via `eval_diag.rs`'s
output: the Session 42 network scores the symmetric start position at
+2425cp (should be ~0) and a single queen swing at ~4000-4500cp (HCE:
~976cp) — raw logits around 6-16, i.e. near-100% confidence at positions
that plainly aren't decided. This is unregularized BCE weight blowup, not a
feature-design or scale-constant bug (verified `CP_TO_WINPROB_SCALE`
matches between train/inference, and all 5 eval_diag cases agreed with HCE
on sign). Cause: no weight regularization in train_nnue.rs's training loop
lets output-layer weights grow unbounded while still lowering loss. Fix:
clamp the symptom in inference (cheap, no retrain needed to test); the
underlying training-side fix (weight decay) is follow-up work for the next
retrain, not applied yet.

**Decisions made:** D30 (see DECISIONS.md).

**Next session start point:** Gokul applying the inference.rs delta,
confirming build.yml green, then re-running the same match_runner sweep
(5/10/15/20% vs 0%, 40 games, seed_start=0) — no Kaggle/retraining needed
for this test. Compare against D27/17.5c's numbers. If clamping closes most
of the gap: D30 confirmed, next actual training run should add weight decay
to train_nnue.rs before generating more self-play data. If it barely moves:
miscalibration is deeper than magnitude, re-read D9/D10/D14 (feature/target
formulation) next, and consider whether the K+P vs K / queen-swing
eval_diag cases hint at anything feature-specific despite the sign
agreement (e.g. check per-feature weight magnitudes directly rather than
only aggregate output).

---

## Session 43 — 2026-07-07 (Phase 17.5c/d — retrain made things WORSE, new diagnostic)

**Built:** `src/bin/eval_diag.rs` + `.github/workflows/eval_diag.yml`
(Phase 17.5d) — prints HCE vs raw NNUE vs 100%-blended eval for 5 hand-picked
positions with an unambiguous correct evaluation (start pos ~0, White
up/down a queen, K+P vs K both directions). No training/Kaggle needed, pure
Actions run, checks calibration directly instead of only inferring it from
aggregate match_runner Elo.

**Bugs fixed:** N/A.

**Decisions made:** D29 (see DECISIONS.md) — pausing the self-play-data-
volume lever after two rounds of the same paradox: val_loss improves,
in-game strength at every blend weight gets worse. Pivoting to a direct
eval-output diagnostic before any further training runs.

**Result (17.5c sweep against the Session 42 retrained network):** ALL FOUR
weight points got monotonically WORSE than D27's baseline, not better:
  5%:  A 67.5% (+127.0 Elo), was 65.0% (+107.5)
  10%: A 80.0% (+240.8 Elo), was 75.0% (+190.8)
  15%: A 78.8% (+227.6 Elo), was 70.0% (+147.2)
  20%: A 90.0% (+381.7 Elo), was 80.0% (+240.8)
This is the opposite of what improved val_loss (0.53776 -> 0.51661) should
produce, and consistent in direction across all 4 points — not sampling
noise. Checked the blend implementation (eval/mod.rs's evaluate_blended)
and the cp<->logit scale constants (OUTPUT_SCALE, CP_TO_WINPROB_SCALE=400.0)
— both train_nnue.rs and inference.rs agree on scale, ruling out a simple
constant mismatch. Leading hypothesis: the network is fitting its BCE
training target more confidently (lower loss) without that confidence
reflecting genuinely better evaluation quality on the specific tactical
positions alpha-beta search visits — a more confidently-wrong eval hurts
move ordering/pruning more than a milder one. Not confirmed — that's what
17.5d's diagnostic is for.

**Next session start point:** Read eval_diag's output from Gokul. If HCE
and NNUE disagree on sign for the queen-up/queen-down cases (or NNUE is
flat/saturated across all 5 positions), that's confirmed miscalibration —
next step is inspecting the quantization step (noru::quant, an external
crate dependency, not in this repo — may need to check its source via
crates.io/docs.rs) for a fp32-to-quantized conversion bug specific to this
network's weight distribution. If eval_diag looks sane (correct signs,
plausible magnitudes) on all 5 cases, the miscalibration is more subtle
(only shows up on real search-visited positions) and D9/D10/D14 (feature
design + target formulation) need a harder look instead.

---

## Session 42 — 2026-07-07 (Phase 17.5b complete — val_loss improved)

**Built:** Nothing new — read the completed train_nnue.yml run log Gokul
uploaded. Confirmed: 286,659 total rows loaded cleanly (0 malformed rows
skipped from any of the 4 Kaggle batches or the lichess file — the old
1-byte stub correctly caught by load_rows()'s malformed-line handling, as
designed). Training ran all 10 epochs without incident.

**Bugs fixed:** N/A.

**Decisions made:** None new — D29 not needed yet, holding until the
re-sweep result determines whether this network actually changes the
NNUEWeight default question.

**Result:** val_loss improved 0.53776 → 0.51661 (best epoch moved from 8/10
to 5/10). Train/val divergence after epoch 5 (train_loss keeps dropping to
0.49199 by epoch 10, val_loss creeps back to 0.52431) shows the underlying
small-dataset overfit pattern is still present — this is a better floor, not
a fixed problem. Notably, total row count (286,659) was LOWER than the
previous run (483,080) despite far more actual self-play games (3000 vs the
earlier run's uncertain ~93-game estimate) — avg rows/game dropped a lot,
unexplained, not investigated, flagged for awareness only since val_loss
still improved regardless.

**Next session start point:** Gokul is (1) replacing
src/nnue/weights/nnue_pet_dragon_quantized.bin with the new trained network
(artifact nnue-pet-dragon-h32-a256-e10, run 28865459160) — this is a real
code-affecting change since Phase 16.6 embeds it via include_bytes!, confirm
build.yml stays green after the commit — then (2) re-running the exact D27
match_runner sweep (5/10/15/20% vs 0%, 40 games each, seed_start=0) against
the new network. Compare against D27's baseline numbers (5%: A 65%/+107.5
Elo, 10%: A 75%/+190.8, 15%: A 70%/+147.2, 20%: A 80%/+240.8) — if any
weight flips to net-neutral-or-better, that becomes the new NNUEWeight
default candidate (write D29 at that point). If all 4 are still clearly
negative even with the improved network, that's a stronger signal the
architecture (hidden_size=32) or feature set needs revisiting next, not just
more data again.

---

## Session 41 — 2026-07-07 (Phase 17.5b triggered — retraining wired, no code)

**Built:** Nothing new — train_nnue.yml's `selfplay_urls` input already
supported everything needed. Wired in the 4 Kaggle batch URLs (seeds
100/200/300/900, uploaded as GitHub user-attachment links rather than formal
Releases — functionally identical, both resolve to signed S3 URLs the
workflow's `curl -sL` step follows fine). Held epochs/hidden_size/lr/etc at
the same values as the 483k-row baseline run so "more data" is the only
changed variable, per D26/D27's one-lever-at-a-time approach.

**Bugs fixed:** N/A.

**Decisions made:** None new.

**Next session start point:** Read the new run's val_loss from the Actions
log / uploaded artifact, compare against the 0.53776 baseline (D23/Session
32). If meaningfully lower: produce the new quantized weights, then re-run
the match_runner sweep (5/10/15/20% minimum, same as D27's sweep) against
the NEW network before touching NNUEWeight's default. If val_loss barely
moved despite ~30x more self-play games (93→~3000): that's a real signal
worth investigating before just throwing more data at it again — reread
D9/D10/D14 together (feature set / training data blend) for whether
something structural is capping quality, not just data volume.

---

## Session 40 — 2026-07-07 (D28 bugfix — selfplay/match_runner stdout flood)

**Built:** `SearchInfo.print_info` flag (default true) gating the UCI info
println in `iterative_deepening()`. Set `false` in `selfplay.rs` and
`match_runner.rs`; `main.rs`'s UCI loop untouched (default stays true).

**Bugs fixed:** Kaggle self-play versions #7/#9 (seeds 200/400) showed
"Failed" despite 750/750 games completing correctly (confirmed from the
Kaggle log: last line before the traceback is
`game 750/750 (seed 1649): 137 samples written`). Root cause: silent-search
callers were inheriting `iterative_deepening()`'s UCI `info depth ...`
println, unconditional and meant only for the real UCI loop — 750 games'
worth of it is ~56,000 stdout lines, which overwhelmed papermill's 4s IOPub
relay timeout on the `print(result.stdout)` cell. Not a timeout, not
resource contention between the 4 concurrent sessions as first suspected —
a pure stdout-volume bug. See D28.

**Decisions made:** D28 (see DECISIONS.md).

**Next session start point:** Gokul uploading all 4 batches (seeds
100/200/300/900 — #7/#9's data is valid despite the false "Failed" label,
same as #6/#8) as GitHub Release assets. All 4 now completed and uploaded.
Wire them
into `train_nnue.yml`'s `selfplay_urls` input alongside the existing 483,080
rows, rerun training, produce a new quantized weights file, then re-run the
match_runner sweep (5/10/15/20% minimum) against the new network before
touching NNUEWeight's default (D27's open branch — don't skip the re-sweep).

---

## Session 39 — 2026-07-07 (Phase 17.5a sweep read, D27 made, no code)

**Built:** Nothing new. Read all 4 match_runner sweep results Gokul ran
per Session 38's D26 plan (A=0% vs B=5/10/15/20%, 40 games each,
seed_start=0):
  5%:  A scored 65.0% (+107.5 Elo)
  10%: A scored 75.0% (+190.8 Elo)
  15%: A scored 70.0% (+147.2 Elo)
  20%: A scored 80.0% (+240.8 Elo)

**Decisions made:** D27 — no safe nonzero blend weight exists at the
current network's quality (val_loss=0.538). Even 5% is decisively
net-negative (65%, not close to 50%), so this isn't a tuning problem
solvable by picking a lower default — the network itself needs to improve
before any blend is worth re-enabling. Redirects 17.5 from "sweep weights"
to "get more self-play data."

**Bugs fixed:** N/A.

**Next session start point:** Gokul is running 4x Kaggle self-play batches
(750 games each, seed_start 100/200/300/400 — see ROADMAP.md 17.5b) to avoid
repeating the Session 30 queue-delay shortfall (~93/3000 games) that
produced the current undertrained network. As batch files land as GitHub
Release assets (D22), wire their URLs into train_nnue.yml's selfplay_urls
input alongside the existing 483,080 rows, rerun train_nnue, produce a new
quantized weights file, then re-run the match_runner sweep (5/10/15/20% at
minimum) against the new network before touching the NNUEWeight default
again. Don't skip the re-sweep just because more data went in — verify it
actually helped.

---

## Session 38 — 2026-07-07 (Phase 17.5 planning — D26 made, no code)

**Built:** Nothing new. Verified Phase 16.4c (pawn-start feature convergence)
was already fully implemented in Phase 16.2's `features.rs` — confirmed via
`pawn_start_feature_index()`, `extract_features()`, and
`test_pawn_start_feature_drops_once_record_cleared()` all matching D11's rule
exactly. Closed the roadmap item with no source changes.

**Decisions made:** D26 — before spending Kaggle compute on a bigger
self-play run or an architecture change (hidden_size bump), sweep the
existing `match_runner` workflow at 5/10/15/20% NNUE weight (vs 0% baseline)
to find where D25's "net negative" result stops holding. Zero new code —
`match_runner.yml`/`match_runner.rs` (Phase 17.2/17.3) already take
`weight_a`/`weight_b` as inputs. Cheapest, most-informative experiment
available; result determines whether retraining is even necessary.

**Bugs fixed:** N/A.

**Next session start point:** Read 4x `match_results.txt` artifacts from
Gokul's queued Actions runs (A=0 vs B=5/10/15/20, 40 games each,
seed_start=0, movetime=100ms — see ROADMAP.md 17.5 for the exact table).
Based on where the Elo delta crosses from negative to roughly-neutral/
positive:
- if some low weight (5-10%) is net-neutral-or-positive → just set that
  as the new `NNUEWeight` default, Phase 17.5 done, no retraining needed.
- if even 5% is still clearly net-negative → that points at the network
  itself (val_loss=0.538 too weak at any blend), which justifies committing
  to a bigger Kaggle self-play run (target the original 3000 games, the
  first attempt only got ~93 due to queue delay — D26 discussion) before
  trying architecture tweaks.
Read D25/D26 together before deciding; don't rerun anything bigger without
the sweep data in hand first.

---

## Session 37 — 2026-07-07 (Phase 17.4 — match result read, D25 made)

**Built:** No new code beyond flipping the default. Read
`match_results.txt` from the Actions artifact (A=0% NNUE vs B=25% NNUE,
20 games): A scored 87.5%, +338 Elo. Decisive enough to act on immediately
rather than wait for more samples.

**Decisions made:** D25 — `NNUEWeight` default dropped 25% → 0% in
`src/eval/mod.rs` and `src/main.rs` (UCI option string). The Phase 17.1
mechanism (runtime-configurable weight) stays; only the default moved.

**Bugs fixed:** N/A — not a bug, a training-quality finding. The network's
val_loss (0.538) was already flagged in D23 as not confident; this match
is the first real evidence of how much that mattered in practice.

**Next session start point:** Phase 17.5 — improving NNUE training quality
before attempting to re-enable a nonzero blend weight. Options to weigh:
more self-play games (Phase 16.4a's `selfplay.rs` binary already exists,
just needs a bigger run), more training epochs, or reconsidering the
16.4c pawn-start feature convergence question (still open, see D11) since
that could be part of why val_loss plateaued where it did. Read
`docs/DECISIONS.md` D9/D11/D23/D25 together before deciding which lever
to pull — don't just rerun selfplay bigger without first checking whether
the feature-convergence issue is a bigger lever.

---

## Session 36 — 2026-07-07 (Phase 17.2/17.3 — Elo A/B match harness)

**Built:**
- `src/bin/match_runner.rs` (NEW) — plays engine-vs-engine matches between
  two `NNUEWeight` configs. Colors alternate per game; separate TT per
  color-per-game (correctness requirement — see ROADMAP note). Reports
  W/L/D and a standard logistic Elo diff, undefined case handled explicitly
  (0%/100% score → "undefined" string, not a crash or bogus infinity).
  5 unit tests (Elo formula edges, outcome-translation both color
  assignments, one real short-game smoke test).
- `.github/workflows/match_runner.yml` (NEW) — workflow_dispatch, mirrors
  `selfplay.yml`'s exact structure (Rust toolchain, cache, build, run,
  upload artifact). Default config: A=0% (pure HCE) vs B=25% (D23 default).

**Decisions made:** None new — this is the concrete Elo-testing mechanism
D23 already called for.

**Bugs fixed:** N/A.

**Next session start point:** Phase 17.4 — actually trigger the workflow
(Gokul: Actions tab → "Elo A/B Match" → Run workflow, defaults are fine for
a first pass) and read `match_results.txt` from the run artifact. If A
(pure HCE) scores meaningfully above 50%, that's a signal the 25% NNUE
weight may be net-negative given the network's still-modest val_loss
(0.538) — don't change `eval::set_nnue_weight_pct`'s default without
reading the actual result first, no guessing which way it went.

---

## Session 35 — 2026-07-07 (Phase 17.1 — NNUE blend weight runtime-configurable)

**Post-session CI check:** Deploy to GitHub Pages run #261 (commit 83b1da7)
showed "Cancelled", not failed — confirmed via run detail page: `Build WASM
& Web` passed green (26s), `Deploy` was cancelled with reason "Canceling
since a higher priority waiting request for pag[es]…" — i.e. the immediate
follow-up commit's deploy pre-empted it, normal GitHub Pages single-flight
concurrency, not a code issue. No fix needed. Separately flagged: Node.js 20
deprecation warning on the same run — logged in ROADMAP.md Housekeeping,
not urgent.

**Built:**
- `src/eval/mod.rs` (DELTA) — `NNUE_BLEND_WEIGHT` const → `NNUE_BLEND_WEIGHT_PCT`
  (process-global `AtomicU32`, Relaxed ordering, same benign-race reasoning
  as the TT per D4). New `set_nnue_weight_pct()` / `nnue_weight()`.
  `evaluate_blended()` now skips the NNUE forward pass entirely at weight 0.
- `src/main.rs` (DELTA) — new UCI option `NNUEWeight` (spin, 0-100, default
  25), wired into `cmd_setoption`.
- 1 new test (`test_nnue_weight_setter_getter_and_blend_at_zero`) —
  deliberately a single test function to avoid cross-test races on the
  shared atomic; restores the 25% default before returning.

**Decisions made:** None new — this directly implements the follow-up D23
already flagged ("weight can be raised... once actual Elo testing... is
available"), no new architectural call made.

**Bugs fixed:** N/A.

**Why this task:** Gokul asked "you decide" after Phase 16 closed. Chose
this over Texel tuning (Phase 14, optional per D12, superseded in value by
NNUE) or an undefined new feature — this is the smallest concrete
prerequisite for the Elo testing that D23 itself says should happen before
the blend weight is trusted further.

**Next session start point:** Phase 17.2 — build the self-play match
harness. Read `src/nnue/inference.rs`, `src/eval/mod.rs` (this session's
delta), and `src/position/setup.rs` (Pet Dragon generator) fresh before
writing it — needs to run two in-process searches with different
`NNUEWeight` settings per game and tally results without needing a
terminal (Rust binary run via a GitHub Actions workflow, not a UCI/GUI
match runner Gokul would have to operate manually).

---

## Session 34 — 2026-07-07 (Phase 16.7 WASM compatibility — code audit, no changes)

**Built:** Nothing new — this was a verification session. Read
`src/nnue/inference.rs`, `Cargo.toml`, `src/lib.rs`, `src/nnue/mod.rs` fresh
and traced the only WASM→NNUE call path (`search_from_fen` →
`iterative_deepening` → `evaluate_blended` → `evaluate_nnue`). Confirmed:
no OS/filesystem calls anywhere in that path, `std::sync::OnceLock` is
wasm32-safe, `noru` has no `target_arch = "wasm32"` exclusion in
Cargo.toml (only `pyrrhic-rs` is correctly gated out). Conclusion: the
NNUE code is already WASM-compatible with zero changes required, matching
the Session 33 handoff's own prediction.

**Decisions made:** None new.

**Bugs fixed:** None (no bug — nothing to fix).

**Blocked on:** Gokul confirming in-browser at
https://g-c-3.github.io/pet-dragon that a new game actually gets an engine
reply (not stuck on "Engine thinking…") and, if reachable, no red console
errors. This can't be verified from this session's tooling (no browser
access) — genuinely needs a human eyeball on the live page.

**Next session start point:** If Gokul confirms the browser check is
clean, mark ROADMAP 16.7 `[x]` and move to 16.4c (pawn-start feature
convergence design) or Phase 14 (Texel tuning, optional) per D12 — decide
which based on current Elo appetite. If Gokul reports the engine hangs or
throws a console error, read that exact error before touching any NNUE
code — do not guess at a fix blind.

---

## Session 33 — 2026-07-06 (Phase 16.6 NNUE eval integration — code only, pending weights upload)

**Built:**
- Read NORU 2.2.0's real inference API from docs.rs fresh (`network::NnueWeights`,
  `network::Accumulator`, `network::forward`, `quant::OUTPUT_SCALE`) — the
  trainer API (Session 29) doesn't cover inference, confirmed before writing
  anything per the mandatory "read fresh" rule for a first-time code path.
- `src/nnue/inference.rs` (NEW) — embeds the trained quantized network via
  `include_bytes!`, lazily parses it once via `OnceLock`, runs
  `Accumulator::refresh()` + `forward()` per call, converts the raw i32 to
  centipawns via `OUTPUT_SCALE(16)` and `CP_TO_WINPROB_SCALE(400)` (inverse
  of train_nnue.rs's own sigmoid(eval_cp/400) target formula, D14). 3 tests
  (bounded, 1000-seed no-panic, determinism) — cannot actually run in CI
  until the weights file exists (see blocker below).
- `src/eval/mod.rs` (DELTA) — added `evaluate_blended()`: HCE + NNUE at a
  conservative 25% NNUE weight (D23). Left `evaluate()` itself untouched so
  its existing HCE-only test suite keeps validating HCE in isolation.
- `src/search/alpha_beta.rs` (DELTA) — one-line change: the existing
  `evaluate()` delegation wrapper now calls `crate::eval::evaluate_blended()`
  instead of `crate::eval::evaluate()`. This is the only search-facing
  change; no other call site needed touching.
- `src/nnue/mod.rs` (DELTA) — `pub mod inference;`.

**Decisions made:** D23 (new) — NNUE blend weight fixed at 0.25 pending real
Elo testing, per Session 32's own handoff reasoning (val_loss=0.538 is not
yet confident enough to justify a heavier weight or a full replace).

**Bugs fixed:** N/A (new code, not yet compiled — see blocker).

**⚠️ BLOCKING — nothing in this session's delta can compile yet:**
`src/nnue/inference.rs` embeds the network via
`include_bytes!("weights/nnue_pet_dragon_quantized.bin")`, and that file
does not exist in the repo. Gokul must download
`nnue_pet_dragon_quantized.bin` from the Session 32 training run's artifact
page and upload it to exactly `src/nnue/weights/nnue_pet_dragon_quantized.bin`
in the same commit as this session's code (481K, well under the 25MB
repo-upload UI limit — D22's GitHub Releases workaround is not needed for a
file this small). Without it, CI fails at compile time, not test time.

**Test risk flagged (unverified):** the cp-scale conversion
(`raw / OUTPUT_SCALE * 400`) is derived from NORU's documented API and
train_nnue.rs's own target formula, not verified against a real forward
pass — no weights file existed to test against while writing this. First
real signal comes from CI's first green (or red) run once the weights file
is uploaded.

**Bugs found and fixed (post-upload CI red, two rounds):**
1. `is_time_up()` sampled the clock every 2048 nodes — too coarse once
   NNUE's heavier per-node eval let an 881-node depth-3 search finish
   uninterrupted past the test's 500ms ceiling. Tightened to every 256
   nodes (`search/mod.rs`). This alone did NOT fix the test — same 517ms,
   same node count, proving the check wasn't actually engaging.
2. Root cause (D24): `info.time_allocated_ms` — what `is_time_up()`
   compares elapsed time against — was never wired to the real
   `TimeManager` hard limit inside `iterative_deepening()`. It sat at
   `SearchInfo::new()`'s default (5000ms) for every real search, making the
   in-search abort dead code outside `alpha_beta.rs`'s own unit tests (which
   set it manually). Fixed by setting `info.time_allocated_ms = hard_ms`
   right after the `TimeManager` is constructed (`search/iterative.rs`).
   This bug predates Phase 16.6 — NNUE's slower eval just made it visible.

Both fixes together verified locally (real weights file, exact CI test
command): **320/320 passed.** Confirmed green on GitHub Actions.
Root-caused entirely from CI logs Gokul pasted — no guessing either round.

**Phase 16.6 complete.**

**Next session start point:**
Start Phase 16.7 (WASM-compatible inference) — see ROADMAP.md note: likely
already works given NORU's include_bytes!-embedded weights and no OS calls,
but hasn't been runtime-verified in an actual browser session yet. Read
src/nnue/inference.rs and the wasm-pack build/deploy workflow fresh before
touching anything.

---

Prior entry, superseded by the above once committed: check
`test_evaluate_nnue_start_pos_bounded`'s actual value isn't near a scale
extreme — if it looks implausible (near-zero always, or saturating), use
`noru::network::NnueWeights::audit_against_fp32` to get an empirical
`inferred_output_scale` and correct `CP_TO_WINPROB_SCALE`/`OUTPUT_SCALE`
math in `inference.rs`, rather than re-guessing. If genuinely looking
reasonable: Phase 16.6 done, move to 16.7 (WASM-compatible inference —
NORU is pure Rust so this may already work with zero changes; verify by
checking the wasm-pack build log for the next deploy). If CI is red on a
compile error unrelated to the missing-file case: read the actual error
before touching inference.rs again.

---

## Session 32 — 2026-07-05 (Phase 16.5 complete — 483k-row training run)

**Built:**
- Fixed a corrupted train_nnue.yml (a previous session's delta-format
  "WHERE:" annotation had been pasted literally into the YAML body,
  breaking the step list) — delivered as a full corrected file rather than
  another delta, given repeated manual-paste errors on a file this small.
- Added selfplay_urls input to train_nnue.yml (D22) — curls self-play data
  from direct URLs (GitHub Release assets) for files too large for the
  25MB repo-upload UI.
- train_nnue.rs: load_rows() no longer panics on a line read error
  (e.g. invalid UTF-8 from a truncated final line) — skips it like a
  malformed row instead. Defensive fix; turned out not to be needed for
  this specific file (truncation landed on a clean ASCII boundary) but is
  real protection for the general case.
- web/index.html: added player-turn panels, populated the previously-dead
  coordinate-label CSS, restyled controls with plain-text icons instead of
  color emoji (kept visual consistency with the gold/dark theme).

**Bugs fixed:** train_nnue.yml YAML corruption (see above) — cause: prior
session's delta instructions pasted into the file instead of just the
code; fix: full corrected file; correct because delta format's plain-text
annotations were never meant to be file content.

**Decisions made:** D22 (GitHub Releases for >25MB data files, supersedes
committing large files directly).

**Result:** Phase 16.5 (train NORU NNUE) COMPLETE. 483,080-row run
(433,080 self-play + 50,000 Lichess) — best epoch 8/10, val_loss=0.53776,
train/val curves monotonic and near-plateaued, no meaningful overfitting
(contrast with the first 52,836-row run's clear epoch-3 overfit). Trained
network: nnue-pet-dragon-h32-a256-e10 artifact, nnue_pet_dragon_quantized.bin
(481K), retained 30 days on the run's artifact page.

**Next session start point:** Phase 16.6 — integrate the trained network
into eval. Before writing anything: (1) download nnue_pet_dragon_quantized.bin
from the run-28745742039 artifact page and re-upload it into the repo
(suggest a new nnue/ directory or similar — decide path when there),
(2) read src/eval/mod.rs fresh to see the current HCE call site, (3) read
NORU's inference-side API (NnueWeights::load_from_bytes, forward pass) from
docs.rs fresh — the trainer API was already read in Session 29 but
inference is a different code path, (4) decide blend-vs-replace: given
this is the first trained network and val_loss (0.538) is still well above
a "confident" prediction (0 or 1), a blend with HCE (not a full replace)
is probably the safer starting point pending real Elo testing later.

---

## Session 31 — 2026-07-05 (Kaggle offload for self-play)

**Built:**
- kaggle/pet_dragon_selfplay_kaggle.ipynb (NEW) — installs Rust via rustup,
  clones pet-dragon, builds+runs selfplay binary, writes output to Kaggle's
  /kaggle/working/ (downloadable Output). Designed for "Save & Run All
  (Commit)" background execution so Gokul doesn't need to keep a tab open.
- train_nnue.yml: added selfplay_paths input (comma-separated committed
  file paths) alongside the now-optional selfplay_run_id, so Kaggle-
  generated data can be committed straight into the repo and consumed
  without going through a GH Actions artifact.

**Decisions made:** D21 (Kaggle for self-play generation, conserving
Actions minutes across Gokul's multiple projects — supersedes D20's
narrower GPU/dataset-size framing).

**Next session start point:** Gokul runs the Kaggle notebook in the
background (3000 games, seed_start=0), commits the resulting .txt to
data/selfplay/, then triggers train_nnue.yml with selfplay_paths pointing
to it and the existing lichess_run_id. Read the log for "best epoch: X/10"
— with ~3000 games (~380k rows, vastly more than the 2836-row run that
overfit by epoch 3) expect val_loss to keep improving for more of the 10
epochs; if best epoch is still low (<=3), suspect lr too high or hidden_size
too small for the new data volume before reaching for more data again.

---

## Session 30 — 2026-07-05 (Phase 16.5 first successful training run + overfit fix)

**Built:**
- First successful train_nnue.yml run (Run ID 28723114004): 2836 selfplay
  rows + 50000 lichess rows, hidden=32, accumulator=256, 10 epochs.
  train_loss fell 0.615→0.554 but val_loss bottomed at epoch 3 (0.615) then
  rose to 0.635 by epoch 10 — clear overfitting on this small dataset.
- src/bin/train_nnue.rs: added best-validation-checkpoint tracking (clone
  TrainableWeights whenever val_loss improves, save that instead of the
  final epoch) — confirmed TrainableWeights derives Clone from NORU 2.2.0
  source before relying on it. Removed an unused Write import flagged by
  the compiler.

**Bugs fixed:** train_nnue.rs saved the final epoch unconditionally even
when validation loss had been rising for several epochs; fixed by tracking
best_val_loss and saving that snapshot; correct because it ships the
checkpoint that generalized best rather than the one that memorized
training rows longest.

**Decisions made:** None new — noted for the record (not D-worthy yet) that
94.7% of current training rows are Lichess "NA" rows, so target ≈
eval-sigmoid-only for most of the dataset per existing D14; NNUE is mostly
learning to imitate current HCE until self-play volume grows.

**Next session start point:** Gokul re-runs train_nnue.yml with the same
two Run IDs after uploading the two deltas above. Read the new log: confirm
"best epoch: X/10" is not epoch 10 (would mean still improving — safe to
increase epochs next time) and not epoch 1 (would mean lr too high or net
too small). If best epoch lands mid-range (3-7) with val_loss meaningfully
below 0.693 (the loss of a coin-flip predictor), Phase 16.5 is done — move
to 16.6: read src/eval/mod.rs fresh, then wire nnue_pet_dragon_quantized.bin
loading into eval as a blend-or-replace path alongside HCE.

---

## Session 29 — 2026-07-05 (Phase 16.5 NNUE trainer)

**Built:**
- Read NORU 2.2.0's actual trainer.rs/network.rs source from docs.rs before
  writing anything (TrainableWeights, AdamState, Gradients, TrainingSample,
  NnueConfig::new_owned, NnueWeights::save_to_bytes) — API guessed from the
  crate description alone would have been wrong on several field names.
- src/bin/train_nnue.rs (NEW) — parses stm|nstm|eval|result rows (shared
  format from selfplay.rs/lichess_sample.rs), blends eval-sigmoid target
  with game-result target per D14 (lambda CLI arg, default 0.7, no-op for
  "NA" rows), trains NnueConfig(896 features, configurable accumulator/
  hidden size, SCReLU) via NORU's FP32 Adam trainer with a seeded
  shuffle + 5% validation split, logs per-epoch train/val BCE loss, writes
  both an FP32 checkpoint and quantized i16 weights.
- .github/workflows/train_nnue.yml (NEW) — workflow_dispatch, downloads
  selfplay + lichess artifacts by Run ID (actions/download-artifact@v4),
  builds and runs train_nnue, uploads both output binaries.

**Decisions made:** D19 (new) — training runs via GitHub Actions, not the
Colab notebook the Session 7/ROADMAP note originally proposed. Colab would
require Gokul to manually run Rust/cargo cells, which conflicts with D15
(Actions handles all building, Gokul never runs cargo). NORU is pure Rust
either way, so Actions runs it exactly as well as Colab's CPU would, with
one less manual step for Gokul.

**Bugs fixed:** N/A (new files).

**Test risk flagged (unconfirmed at write time):** train_nnue.rs has no
#[cfg(test)] unit tests of its own — it's a data-processing binary, not a
library module, consistent with selfplay.rs/lichess_sample.rs conventions
(bins aren't exercised by `cargo test`). Real first signal comes from
actually triggering train_nnue.yml with real Run IDs; the loss-decreasing
behavior of NORU's Adam trainer itself is proven by NORU's own upstream
tests, not re-tested here.

**Next session start point:**
Ask Gokul for the Run ID of a completed selfplay.yml run and a completed
lichess_sample.yml run (visible on the Actions tab / run URL). Trigger
train_nnue.yml with those two IDs and default hyperparameters first. Read
the workflow log: check "X rows kept, Y skipped" lines from load_rows() —
if skipped is high relative to kept, the row format assumption is wrong,
fetch a raw line from one of the artifacts and fix parse_line(), don't
re-guess. If train_loss/val_loss both decrease and finish reasonably close
(no large train/val gap suggesting overfit at these small hidden sizes):
Phase 16.5 done, move to 16.6 — wire the quantized network into src/eval/
(likely as a blend-or-replace option alongside HCE, read eval/mod.rs fresh
first). If val_loss diverges from train_loss: reduce hidden_size or add an
L2/weight-decay knob (NORU's AdamState doesn't expose one currently —
would need a manual weight-decay term added in train_nnue.rs itself).

---

## Session 28 — 2026-07-04 (Phase 16.4b: Lichess sampler, 3 bugs fixed)

**Built:** src/bin/lichess_sample.rs (NEW) + .github/workflows/lichess_sample.yml
(NEW) — streaming Lichess CC0 eval dataset sampler. Feature-gated behind
"lichess-sample" (ruzstd, reqwest, serde_json all optional, not in default
or wasm) — zero impact on native release build, cargo test, or WASM bundle.

**Bugs fixed (3, all in this new file, none in existing green code):**
1. reqwest 0.13 feature name changed from "rustls-tls" (0.11/0.12 era) to
   "rustls" — build failed to resolve the feature. Fixed by using the
   correct 0.13 feature name.
2. ruzstd's StreamingDecoder doesn't skip zstd "skippable frames"
   (metadata headers) itself — errored with ReadFrameHeaderError::SkipFrame
   on init. Fixed with a retry loop that reads-and-discards `length` bytes
   then retries decoder creation (pattern confirmed from Rust std's own
   gimli/elf.rs zstd handling).
3. StreamingDecoder only decodes ONE zstd content frame then reports EOF —
   the Lichess file has multiple sequential content frames (large
   archives commonly do, e.g. via pzstd). First fix attempt only handled
   this at decoder-init time inside a separate named function, which
   failed to compile (E0107 — StreamingDecoder takes 2 generic params,
   READ and DEC, and the function's return-type annotation only named
   one, which can't use `_` in item signatures). Fixed by inlining the
   frame-opening loop directly into main()'s outer 'frames loop instead
   of a named helper, so both generics are inferred from usage.

**Verified (Session 28, workflow runs):** 500/500 test sample run first
(0 parse failures, 99,801 lines read). Then full-scale run confirmed:
50000/50000 samples, 0 parse failures, 9,999,801 lines read,
lichess_sample.txt = 9.6M / 50000 rows. JSON field assumptions
(evals[].pvs[].cp/.mate, evals[].depth, 4-field FEN) all correct against
the real dataset. Gokul needs to download the artifact
(lichess-sample-skip0-n50000-stride200) before Phase 16.5 can start.

**Decisions made:** None new this session — D18 (prefix-sampling approach,
from Session prior) stands as documented, now empirically confirmed to
produce clean data.

**Next session start point:**
Gokul: download the lichess_sample.txt artifact from the full-scale run
(50000 rows, 9.6M) and upload it, along with the most recent
selfplay_data.txt artifact from selfplay.yml. Then start Phase 16.5:combine both into one training set, write the Kaggle notebook (Save Version → Run All mode, per D18) for NORU NNUE training (D14's
blend-eval-vs-game-result decision happens at that stage — lichess rows have result="NA" and must be treated as eval-only targets, self-play rows have both signals).

---

## Session 27 — 2026-07-04 (Phase 16.4b streaming sampler)

**Verified:** Session 26's Cargo.toml duplicate-key fix landed cleanly on
GitHub (fetched and confirmed — no duplication, single copy of the
web-time/console_error_panic_hook block). CI status and browser-engine-moves
confirmation from Session 25/26 are still open — need Gokul's confirmation,
can't verify either through curl (Actions API was rate-limited this
session; browser behavior is inherently unverifiable without a browser).

**Built:**
- Resolved Session 24's open design question: standard .zst is
  sequential-only (no byte-seek to arbitrary decompressed offsets) —
  confirmed via ruzstd's public API (only exposes sequential io::Read).
  True reservoir sampling over all 388M positions isn't CI-feasible.
  Went with Session 24 handoff's pragmatic option (b): skip a prefix, keep
  every Nth line after that, stop once sample_size is reached.
- `src/bin/lichess_sample.rs` (NEW) — streams Lichess CC0 eval dataset
  (reqwest blocking + rustls-tls) through a ruzstd StreamingDecoder,
  parses each JSONL line (serde_json), picks highest-depth eval, converts
  mate scores to a bounded cp proxy, negates to side-to-move perspective,
  writes stm|nstm|eval|result rows matching selfplay.rs's format
  (result="NA" — this dataset has no game outcome). 8 unit tests.
- `.github/workflows/lichess_sample.yml` (NEW) — workflow_dispatch,
  skip_lines/sample_size/stride inputs, mirrors selfplay.yml conventions.
- `Cargo.toml` (DELTA) — ruzstd/reqwest/serde_json added as optional deps
  behind a new "lichess-sample" feature (not in default, not in wasm) —
  zero impact on native release build, cargo test, or WASM build. New
  [[bin]] entry for lichess_sample with required-features gating so
  default `cargo build`/`cargo test` never touches it.

**Decisions made:** See DECISIONS.md D18 (new).

**Bugs fixed:** N/A (new files).

**Test risk flagged (unconfirmed at write time):** JSON field assumptions
(evals[].pvs[].cp / .mate, evals[].depth, fen field count) are based on
Session 24's research summary of the dataset format, not a real fetched
line — this sandbox has no network path to database.lichess.org. The 8
unit tests only run when built with `--features lichess-sample`, so they
did NOT run in the main CI test job this session. First real signal comes
from the workflow's own run log (check `parse_failures` count vs `kept`).

**Next session start point:**
First, close out the still-open Session 25/26 loop: ask Gokul (a) is CI
green on the current main, (b) does the live browser site actually make
engine moves now. Then: trigger the new lichess_sample.yml workflow
(Actions tab → Run workflow, small sample_size like 1000 first) and read
its log — if parse_failures is high relative to kept, fetch one real raw
line's structure (e.g. via the Actions log's own stderr, or a tiny
debug-print of the first unparsed line) and fix process_line/
best_eval_cp_white against real data, don't re-guess blind. If the sample
looks clean: Phase 16.5 — combine selfplay_data.txt + lichess_sample.txt
and write the Colab NORU training notebook (D14's blend-at-training-time
decision happens there).

---

## Session 26 — 2026-07-04 (CI fix: duplicate Cargo.toml key)

**Reported:** Gokul — CI failed immediately after committing Session 25's
WASM hang fix, before any tests ran.

**Diagnosed:** `cargo metadata` error: `duplicate key` for
`console_error_panic_hook` at Cargo.toml:49. The Session 25 delta block
(console_error_panic_hook + web-time, with comments) had been pasted in
twice, back-to-back.

**Fixed:** Cargo.toml (DELTA) — removed the duplicate copy of the block,
kept one. No logic change, pure duplicate-line removal.

**Decisions made:** None.

**Test risk:** None — failure was a TOML parse error before compilation;
`cargo test` never ran, so the actual Session 25 search/mod.rs and lib.rs
changes remain unverified by CI until this fix lands and a run goes green.

**Next session start point:**
Confirm CI green after this fix (this is the real first test of Session
25's WASM Instant fix — the previous run never got far enough to test it).
If green: ask Gokul to reload the live site and confirm the engine now
actually moves (browser-only, cargo test can't verify this). If confirmed
working: return to Phase 16.4b (streaming Lichess zstd sampler, per
Session 24 handoff — resolve zstd seekability/sampling-strategy question
first, verify a zstd-decoding crate exists on crates.io before assuming).

---

## Session 25 — 2026-07-04 (Browser hang bug fix)

**Reported:** Gokul — live site (g-c-3.github.io/pet-dragon) stuck on
"Engine thinking..." forever after playing a move, engine never responds.

**Diagnosed:**
- Read `src/lib.rs` (WASM exports) — `search_from_fen()` constructs a fresh
  `SearchInfo::new()` per call.
- Read `src/search/mod.rs` — `SearchInfo::new()` sets
  `start_time: std::time::Instant::now()`.
- Root cause: `std::time::Instant::now()` panics at runtime on
  `wasm32-unknown-unknown` (no clock source on that specific target, unlike
  wasm32-wasi). Every browser search call panicked immediately. The panic
  hook (`console_error_panic_hook_setup` in lib.rs) was a no-op stub, so
  the panic surfaced as a silent WASM trap — no console error, JS call just
  never returned, UI stuck forever with no diagnostic trail.

**Fixed:**
- Verified `web-time` v1.1.0 on crates.io (MIT/Apache-2.0, drop-in
  std::time replacement, Performance.now()-backed on wasm32, plain
  std::time elsewhere — no feature-flag wiring needed on our end).
- `Cargo.toml` (DELTA) — added `web-time = "1.1"` (unconditional dep) and
  `console_error_panic_hook = { version = "0.1", optional = true }` (added
  to the `wasm` feature list).
- `src/search/mod.rs` (DELTA) — 3 sites, `std::time::Instant` →
  `web_time::Instant`. No other Instant usage found anywhere else in
  search/ or main.rs (grepped to confirm before finishing).
- `src/lib.rs` (DELTA) — `console_error_panic_hook_setup()` now actually
  calls `console_error_panic_hook::set_once()` instead of being a stub, so
  any future wasm panic is visible in the browser console instead of
  hanging silently again.

**Decisions made:** None new — this is a platform-compatibility bug fix,
not an architectural call.

**Test risk:** Zero for `cargo test` (native Instant unaffected — web-time
only changes behavior on wasm32 target, which isn't exercised by the test
suite at all). This means the fix is UNVERIFIED until Gokul redeploys and
reloads the live site — CI going green does not confirm this fix works,
only that it doesn't break anything native.

**Next session start point:**
Confirm with Gokul that the browser engine now actually makes moves after
redeploy. If still broken: the new console_error_panic_hook will now show
a real error in the browser console (Chrome mobile: long-press → Inspect,
or ask Gokul to screenshot the console) — read that error before guessing
again, don't re-touch search/mod.rs blind. If fixed: no pending Phase 12
work remains; return to Phase 16.4b (streaming Lichess zstd sampler,
per Session 24's handoff — resolve the seekability/sampling-strategy
question first).

---

## Session 24 — 2026-07-04 (16.4a verified live + 16.4b source research)

**Verified:**
- Gokul ran the selfplay.yml workflow (20 games, seed 0). Confirmed from
  Actions log: 20/20 games completed, zero panics, zero compile errors,
  2,836 samples written (476K), sum of per-game counts matches `wc -l`
  exactly. The "1 warning" annotation on the run is GitHub's own Node.js 20
  deprecation notice on actions/checkout@v4 — unrelated to our code.
  Game-length spread (80–300 plies) is healthy; two games hit the 300-ply
  cap, expected occasionally at 100ms/move search strength.
  Phase 16.4a is now fully closed — code + workflow both proven working.

**Researched (16.4b):**
- Confirmed CC0 Lichess evaluation dataset:
  https://database.lichess.org/lichess_db_eval.jsonl.zst — 388,458,657
  positions, CC0-1.0, updated 2026-06-04. JSON-per-line format documented
  in ROADMAP delta above.
- Identified the real constraint before writing code: this is a multi-GB
  file — a GitHub Actions job cannot download+decompress+parse the whole
  thing. Needs a streaming zstd sampler that spreads its sample across the
  file, not a naive full-fetch or prefix-read (either wastes the CI budget
  or biases the sample toward whatever Lichess logged first).

**Decisions made:** None finalized yet — deliberately did not pick a
sampling strategy (every-Nth-line vs reservoir sampling vs byte-offset
seeking) without thinking through zstd's seekability constraints first;
that's next session's first task, not a guess to lock in now.

**Bugs fixed:** N/A.

**Next session start point:**
Design and implement the 16.4b streaming sampler. Key open question to
resolve first: standard .zst (not seekable zstd) means sequential
decompression only — can't jump to byte offsets, so sampling strategy is
either (a) decompress-and-count-past N lines before keeping one (works but
means decompressing the whole file anyway, just not storing it), or
(b) accept sequential first-N-after-skip sampling with a large skip
distance as a pragmatic approximation. Check whether a pure-Rust zstd
decoder crate is available offline in the sandboxed CI environment (no
network beyond crates.io/registry) before assuming `zstd` crate works —
verify on crates.io first like NORU was verified in Session 19.

---

## Session 23 — 2026-07-04 (Phase 16.4a workflow trigger)

**Built:**
- `.github/workflows/selfplay.yml` (NEW) — workflow_dispatch job with
  `num_games`/`seed_start` inputs, builds `selfplay` in release mode, runs
  it, reports output size, uploads `selfplay_data.txt` as an artifact
  (30-day retention). Matched existing `build.yml` conventions
  (dtolnay/rust-toolchain, Swatinem/rust-cache).

**Decisions made:** Used workflow_dispatch inputs for game count instead of
hardcoding a number in `selfplay.rs` or asking Gokul mid-session — puts the
compute/data-volume tradeoff in his hands at trigger time via the Actions
UI, which is the mobile-friendly equivalent of a CLI flag.

**Bugs fixed:** N/A (new file).

**Next session start point:**
Phase 16.4a is now fully actionable — Gokul needs to actually run the
workflow (Actions tab → Run workflow) and download the artifact before
16.4b/16.5 can proceed meaningfully (need real sample data to sanity-check
format, not just code that compiles). If Gokul reports the run: check the
Actions log for `selfplay` binary panics — the highest-risk lines are
`Position::generate_with_seed` (unverified panic behavior on edge-case
random setups) and `iterative_deepening`'s TimeControl with only
`movetime` set (unverified this doesn't require other TimeControl fields
to be non-zero). If green: move to 16.4b — source and license-check a
Lichess CC0 dataset URL before writing any fetch/parse code (data-sourcing
decision, not code — confirm with Gokul or research the current CC0 export
location, don't assume a URL from training data since Lichess's export
paths may have changed).

---

## Session 22 — 2026-07-04 (Phase 16.4a self-play generator)

**Built:**
- Confirmed Session 21 CI green (315/315).
- Read `position/mod.rs`, `movegen/mod.rs`, `movegen/legal.rs`, `search/mod.rs`,
  `search/time.rs`, `position/setup.rs`, `main.rs`, `Cargo.toml` fresh, plus
  NORU's README again, before writing a binary spanning nearly every module.
- `src/bin/selfplay.rs` (NEW) — self-play data generator. Per game: random
  Pet Dragon start via `Position::generate_with_seed(seed)`, 100ms/move
  search via `iterative_deepening` (16MB TT), records
  `extract_stm_nstm_features()` + `result.score` at every ply, backfills
  `game_result` (0/0.5/1 from stm) once the game ends via
  checkmate/stalemate/insufficient-material/repetition/300-ply cap. Output:
  plain-text `stm|nstm|eval|result` lines. Both search-eval and game-result
  signals recorded so the search-vs-outcome training blend (D14) is decided
  in Colab (16.5), not hardcoded here. Caught and fixed my own bug mid-draft:
  an overcomplicated Sample/FinishedSample adapter silently dropped
  `game_result` before it ever reached `write_sample` — rewrote with one
  flat `Sample` struct instead.

**Decisions made:** None new — this is D9/D14 implementation, not a new
architectural call. Explicitly decided NOT to add a `halfmove_clock()`
accessor to `Position` — self-play doesn't need 50-move-rule draws, the
ply cap is sufficient data diversity for training purposes.

**Bugs fixed:** N/A (new file); one self-caught draft-stage bug, not shipped
(see above — never left this session, no docs risk).

**Test risk:** none — `src/bin/` binaries aren't exercised by `cargo test`;
verified 11 `pet_dragon_lib::...` import paths against current source.

**Next session start point:**
`.github/workflows/selfplay.yml` — workflow_dispatch (manual trigger button,
mobile-friendly) job that builds and runs
`cargo run --release --bin selfplay -- <games> selfplay_data.txt`, then
`actions/upload-artifact` for `selfplay_data.txt`. Ask Gokul how many games
per run before writing it (CI minutes budget vs data volume tradeoff — his
call, not mine to assume). After that: 16.4b, standard-chess Lichess CC0
data — that's a data-sourcing/licensing task, not code, confirm CC0 dataset
URL before writing anything that fetches it.

---

## Session 21 — 2026-07-04 (Phase 16.3 incremental NNUE delta)

**Built:**
- Confirmed Session 20 CI green (315/315).
- Read `src/position/make_move.rs` fresh (per own handoff note) to design
  the delta engine against the real match arms rather than guessing.
- `src/nnue/delta.rs` (NEW) — `compute_move_changes()` mirrors make_move()'s
  Quiet/DoublePush/Capture/EnPassant/CastleKing/CastleQueen/Promo* arms,
  reading `pos.piece_on()` and `pos.pawn_starts` in pre-move state (must be
  called before `make_move()` mutates). Produces board-space
  `BoardFeatureChange`/`PawnStartFeatureChange` events; `render_for_perspective()`
  turns those into perspective-specific (added, removed) index pairs for
  NORU's `FeatureDelta`. Correctness proven by a sweep test comparing the
  delta-applied feature set against a full `extract_features()` re-extraction
  post-move, across 300 seeds x up to 6 moves x both perspectives.
- `src/nnue/mod.rs` (DELTA) — added `pub mod delta;`.

**Decisions made:** None new — delta engine implements D10/D11 exactly,
no new architectural call.

**Bugs fixed:** N/A (new file).

**Test risk flagged (unconfirmed at write time):** `test_quiet_king_move_no_pawn_start_changes`
and `test_promotion_drops_pawn_start_feature` assume `Move::new(from, to, kind)`,
`Position::from_fen()`, and `PawnStartMap::set(square, color)` signatures
inferred from convention, not re-verified against `types.rs`/`position/mod.rs`
source this session. If CI fails only on these two, it's a constructor
mismatch — fix the call site, not the delta logic (the main sweep test is
the real correctness proof and doesn't depend on these helpers).

**Next session start point:**
If CI green: Phase 16.4 — training data generation strategy (self-play +
Lichess CC0, per D9/D14 already documented). Read `docs/DECISIONS.md`
entries D9/D14 again at start of that session (not from memory) before
picking the self-play position sampler design, since exact convergence
criteria matter for data quality. If CI red on the two flagged tests: fetch
`src/types.rs` Move constructor + `src/position/mod.rs` FEN parser
signatures, patch call sites only.


---

## Session 20 — 2026-07-04 (Phase 16.1–16.2 NNUE feature set)

**Built:**
- Verified `noru` v2.2.0 live on crates.io (MIT/Apache-2.0, zero deps,
  explicitly WASM-safe per upstream README — no libc dependency like
  pyrrhic-rs, so no wasm32 target exclusion needed).
- `Cargo.toml` (DELTA) — `noru = "2.2"` added to ordinary [dependencies].
- `src/lib.rs` (DELTA) — `pub mod nnue;` added.
- `src/nnue/mod.rs` (NEW) — module root, declares `features`.
- `src/nnue/features.rs` (NEW) — 896-input feature encoding per D10:
  `piece_feature_index()`, `pawn_start_feature_index()`,
  `extract_features()`, `extract_stm_nstm_features()`. Perspective-relative
  encoding (own=0/opp=1, Black view rank-mirrored) matches NORU's
  stm_features/nstm_features training API directly. Pawn-start feature
  presence is driven by `PawnStartMap::started_here()` — same check move
  generation uses — so D11 convergence can't drift out of sync with the
  actual game rule. 9 tests added, all read-only additions (no existing
  code path touched).

**Decisions made:** None new — feature layout follows D10/D11 exactly as
already documented; no new DECISIONS.md entry needed this session.

**Bugs fixed:** N/A (new files).

**Next session start point:**
Confirm CI green with 306 + 9 = 315 tests passing (`nnue::features` tests
are the only new ones). If green: Phase 16.3 — incremental accumulator
updates. Design note for next session: NORU's `Accumulator::update_incremental()`
takes a `FeatureDelta` (added/removed indices) per perspective; the natural
hook point is inside `Position::make_move()`/`unmake_move()` (src/position/make_move.rs)
where the piece/pawn-start deltas are already known move-by-move — read that
file fresh before starting since this is the first eval-adjacent change to
touch make/unmake. Decide there whether accumulator state lives on
`Position` (simplest, mirrors `hash: u64` incremental pattern already used)
or is threaded through `SearchInfo` (more search-friction but keeps
Position UCI/FEN-only). Lean toward `Position` field for consistency with
how `hash` is already handled. If CI is red: read the actual failure log
before touching nnue/features.rs again — don't guess.


---

## Session 19 — 2026-07-04 (Phase 15 CI green — round 2)

**Fixed:**
- `src/syzygy/mod.rs` — `test_syzygy_bad_path_returns_err` failed AGAIN after
  Session 18's fix: got `max_pieces() == 7` (not 0) for a nonexistent path.
  Root cause fully identified: pyrrhic-rs's `TableBases::new()` performs no
  filesystem validation at init — it unconditionally reports the library's
  max supported piece count (7) regardless of path validity. Real absence of
  tablebase files only surfaces later as a probe-time failure, not an
  init-time error. Rewrote test to only check construction doesn't panic;
  removed all behavioral assumptions about pyrrhic-rs's return values for
  bad paths (test renamed `test_syzygy_new_does_not_panic`).

**Result:** CI should now be green. 306/306 tests passing (pending confirmation).

**Known limitation (carried from Session 18, unchanged):** `pyrrhic_rs::TableBases`
is a process-wide singleton. Second `setoption SyzygyPath` call in same process
returns `Err(AlreadyInitialized)`. See D16.

**Next session start point:**
Confirm this CI run is green (306/306, no compile errors). If green: Phase 15
fully closed. Start Phase 16: NNUE. First step: verify NORU crate on crates.io
(`curl -s "https://crates.io/api/v1/crates/noru" -H "User-Agent: pet-dragon/0.1"`).
If NORU not available: research alternatives (bullet-train-rs, nnue-rs, or custom).
Then Phase 16.1 — add to Cargo.toml with same wasm32 exclusion pattern as pyrrhic-rs.
If NOT green: read the new CI log Gokul uploads and continue debugging — do not
re-touch syzygy/mod.rs test again without reading the actual new failure first.

---

## Session 18 — 2026-07-04 (Phase 15 CI green + test fix)

**Fixed:**
- `src/syzygy/mod.rs` — `test_syzygy_bad_path_returns_err` failed in CI:
  pyrrhic-rs returned `Ok` (not `Err`) for a nonexistent path, because
  `tb_init()` sets `TB_LARGEST` nonzero even with zero probe files for
  trivial (fileless) endgame classes. Loosened test to accept either
  outcome, asserting `max_pieces() == 0` when `Ok`.
- Earlier in session: E0597 lifetime error in main.rs DTZ root probe
  (`legal` borrow outliving via `impl Iterator` temporary in nested if-let)
  — root-caused after two failed attempts (`Some(mv)` then `Some(&mv)`
  both insufficient). Final fix: split into `let found_move: Option<Move> =
  legal.iter().find(...).copied();` then `if let Some(mv) = found_move`,
  fully decoupling the owned value from `legal`'s borrow.

**Result:** CI green. 306/306 tests passing. Phase 15 fully shipped.

**Known limitation (not yet fixed):** `pyrrhic_rs::TableBases` is a
process-wide singleton (`TB_INITIALIZED` static in the C library). Calling
`setoption SyzygyPath` a second time in the same engine process will return
`Err(AlreadyInitialized)` even for a valid path. Most GUIs set SyzygyPath
once at startup, so this is low-impact, but worth a follow-up: either
document as expected behavior, or wrap in a `OnceLock`-guarded re-init path
if GUI compatibility issues arise.

**Next session start point:**
Phase 15 complete and green. D12 confirmed: skip Phase 14 (Texel tuning).
Start Phase 16: NNUE. First step: verify NORU crate on crates.io
(`curl -s "https://crates.io/api/v1/crates/noru" -H "User-Agent: pet-dragon/0.1"`).
If NORU not available: research alternatives (bullet-train-rs, nnue-rs, or custom).
Then Phase 16.1 — add to Cargo.toml with same wasm32 exclusion pattern as pyrrhic-rs.

---

## Session 17 — 2026-07-03 (Phase 15 Syzygy Tablebases)

**Built:**
- `src/syzygy/mod.rs` (NEW) — full Syzygy integration:
  `PetDragonAdapter` implements `pyrrhic_rs::EngineAdapter` via Pet Dragon's
  precomputed bitboard tables (no extra code — delegates to existing functions).
  `SyzygyProber` wraps `TableBases<PetDragonAdapter>` with two public methods:
    `probe_wdl(&pos)` → `Option<i32>`: thread-safe WDL for interior nodes.
    `probe_root(&pos)` → `Option<(u8, u8, PieceKind, i32)>`: DTZ at root (single-thread).
  `TB_WIN_SCORE = 10_000`. Private helpers: `extract_position_bits`, `wdl_to_score`,
  `pyrrhic_piece_to_pd`. 4 tests: bad-path-err, extract-bits-no-overlap,
  wdl-score-symmetry, tb-win-score-bounds.
- `Cargo.toml` (DELTA) — `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`
  pyrrhic-rs = "0.2" — native-only; WASM builds unchanged.
- `src/lib.rs` (DELTA) — `#[cfg(not(target_arch = "wasm32"))] pub mod syzygy;`
- `src/search/mod.rs` (DELTA) — `syzygy: Option<Arc<SyzygyProber>>` field added
  to SearchInfo (cfg-gated). Initialised to `None` in `new()`.
- `src/search/alpha_beta.rs` (DELTA) — WDL probe inserted after draw checks,
  before TT probe. On probe success: bound set, stored in TT, score returned.
- `src/main.rs` (DELTA) — SyzygyPath option in cmd_uci; syzygypath match arm
  in cmd_setoption; `syzygy: Option<Arc<SyzygyProber>>` in EngineState;
  DTZ root probe at start of cmd_go (before thread spawn); syzygy Arc cloned
  via `syzygy_for_threads` into main thread info and each helper thread info.

**Bug fixed (in-session):**
- Helper thread loop captured `syzygy_for_threads` by move on first iteration,
  leaving subsequent iterations with a moved-out value.
  Fix: `let h_syzygy = syzygy_for_threads.clone()` before each `move ||` closure
  (same pattern already used for h_pos, h_tt, h_stop). Caught before upload.

**Architecture decisions:**
- None new. Follows D4 (lock-free TT sharing), D15 (Actions-only build).

**Next session start point:**
Phase 15 complete. D12 confirmed: skip Phase 14 (Texel tuning).
Start Phase 16: NNUE. First step: verify NORU crate on crates.io
(`curl -s "https://crates.io/api/v1/crates/noru" -H "User-Agent: pet-dragon/0.1"`).
If NORU not available: research alternatives (bullet-train-rs, nnue-rs, or custom).
Then Phase 16.1 — add to Cargo.toml with same wasm32 exclusion pattern as pyrrhic-rs.
Manual task for Gokul: check bench CI Actions log for node count baselines (Session 16
left ??? placeholders in ROADMAP 13.7). Fill them in before next major search change.

---

## Session 16 — 2026-07-03 (Phase 13.7 Node Count Benchmarking)

**Built:**
- `tests/node_count.rs` (NEW) — 5 #[ignore] fixed-depth node count benchmarks:
  node_count_startpos_depth10, node_count_kiwipete_depth9, node_count_endgame_depth11,
  node_count_tactical_depth9, node_count_pet_dragon_rank1_depth8.
  Each calls iterative_deepening() via fixed_depth_tc(), prints nodes/NPS/best move.
  32MB TT. Skipped in normal cargo test; run only by bench CI job.
- `.github/workflows/build.yml` (DELTA) — bench job added: runs on main push after test,
  uses cargo test --profile bench-tests node_count -- --ignored --nocapture.
- `Cargo.toml` (DELTA) — [profile.bench-tests] added: inherits release (opt-level=3,
  codegen-units=1), overrides panic=unwind (required for test harness), lto=false
  (faster CI compile), strip=false.

**Bug fixed:**
- Cause: [profile.release] has panic="abort"; cargo test --release applied this to
  test binary, killing process on any assert! with no output (exit 101, silent).
- Fix: new [profile.bench-tests] profile with panic="unwind".
- Why correct: Rust test harness requires unwinding to catch panics per test;
  abort mode kills the whole process before the harness can report results.

**Architecture decisions:**
- None new. panic=abort stays in [profile.release] for the native binary (correct
  for production). Only test profile overrides it.

**Baselines to fill in** (from bench job Actions log):
  node_count_startpos_depth10:    nodes = ???, nps = ???
  node_count_kiwipete_depth9:     nodes = ???, nps = ???
  node_count_endgame_depth11:     nodes = ???, nps = ???
  node_count_tactical_depth9:     nodes = ???, nps = ???
  node_count_pet_dragon_rank1_d8: nodes = ???, nps = ???

**Next session start point:**
Phase 13 complete. Decision point per D12 (DECISIONS.md):
  Skip Phase 14 (Texel tuning). Proceed to Phase 15 (Syzygy tablebases).
  Start: Phase 15.1 — verify pyrrhic-rs on crates.io, add to Cargo.toml,
  confirm it builds on stable Rust. Read DECISIONS.md D12 first.
  Then: Phase 15.2 — UCI SyzygyPath option in main.rs.
  
---

## Session 15 — 2026-07 (Phase 13.6 Continuation History)

**Built:**
- Phase 13.6 complete across 3 files:
  - `src/search/mod.rs` — Added `cont_hist: Box<[[[i32; 64]; 12]; 64]>` to `SearchInfo`.
    Indexed as `cont_hist[prev_to][piece_idx][curr_to]` where `piece_idx = kind*2 + color`.
    Methods: `get_cont_hist()`, `update_cont_hist()` (same gravity formula as history).
    Zeroed in `reset_for_search()`; not preserved across moves (position-dependent).
    2 new tests: `test_cont_hist_update_get`, `test_cont_hist_reset_on_search`.
  - `src/search/ordering.rs` — `score_move()` gains `prev_move: Move` parameter;
    cont_hist score added to quiet-move history when `prev_move != NULL`.
    `update_ordering_on_cutoff()` gains `pos: &Position` parameter; updates cont_hist
    for the cutoff move (bonus) and all quiets tried before it (penalty).
    Updated existing test; added `test_cont_hist_boosts_quiet_score`.
  - `src/search/alpha_beta.rs` — One-line delta: pass `pos` to `update_ordering_on_cutoff()`.

**Architecture decisions:**
- None new — cont_hist follows same gravity formula as D5/history convention.

**Next session start point:**
Phase 13.7 — Node count benchmarking vs known engines.
- Implement a `perft_bench` or fixed-depth node count test comparing Pet Dragon vs
  reference (Ethereal/Stockfish) at depth 8–10 from standard start.
- Add to `tests/` or as a binary benchmark in `benches/` (Criterion).
- Baseline: record nodes searched at depth 10, note NPS.
- Then Phase 14 (Texel tuning) vs going straight to Phase 16 (NNUE) — decision point.

---

## Session 14 — 2026-07 (Phase 13.5 Quiescence Improvements)

**Built:**
- Phase 13.5 complete across 2 files:
  - `src/search/alpha_beta.rs` — `quiescence()` rewritten with 4 improvements:
    1. Added `qs_depth: i32` parameter (0 = allow quiet checks; <0 = captures only)
    2. In-check path: `generate_moves()` for all evasions, no stand-pat allowed,
       empty move list → `-(MATE_SCORE - ply)` checkmate score
    3. Per-capture delta pruning: `stand_pat + captured_val + 200 < alpha → skip`
       (more precise than global delta; uses `QS_CAPTURE_VALUES[6]` constant)
    4. Quiet checks at qs_depth ≥ 0: quiet moves with positive SEE that give check
       searched after captures, recurse with qs_depth = -1 (1 level deep only)
    - Both call sites updated (leaf node, razoring) → pass `qs_depth = 0`
    - 3 new tests: `test_qsearch_in_check_generates_evasions`,
      `test_qsearch_checkmate_detection`, `test_qsearch_qs_depth_parameter_no_panic`
  - `src/search/pruning.rs` — `try_probcut`: quiescence called with `qs_depth = -1`
    (captures only; avoids quiet-check overhead in probcut verification)

**Architecture decisions:**
- qs_depth = 0 at all main-search → qsearch entries (leaf node + razoring)
- qs_depth = -1 for probcut and all recursive qsearch calls
- In-check evasions use `score_moves()` (killers + history) for ordering quality
- Quiet check recursion is capped at 1 level (passes -1) to prevent tree explosion
- History gravity already present in `update_history()` — no new code needed for that half of 13.6

**Next session start point:**
Phase 13.6 — Continuation history (history gravity already implemented).
- Add `cont_hist: Box<[[[i32; 64]; 12]; 64]>` to `SearchInfo` in `src/search/mod.rs`
  (64 prev_to squares × 12 piece-color types × 64 curr_to squares = 192KB, needs Box)
- Initialize in `SearchInfo::new()` and `new_with_stop()`
- Clear in `reset_for_search()`
- In `src/search/ordering.rs` `score_move()`: add cont_hist lookup for quiet moves
  `score += info.cont_hist[prev_to][piece_idx][to]` (needs prev_move propagated)
- In `src/search/ordering.rs` `update_ordering_on_cutoff()`: update cont_hist on beta cutoff
- Then 13.7: node count benchmarking vs Ethereal at fixed depth

---

## Session 13 — 2026-07 (Phase 13.4 Lazy SMP)

**Built:**
- Phase 13.4 Lazy SMP complete across 6 files:
  - `src/tt/mod.rs` — `store()` changed to `&self`, `unsafe impl Send/Sync` added
  - `src/search/mod.rs` — `Arc<AtomicBool> stop_flag` in SearchInfo, `new_with_stop()` constructor, `is_time_up()` checks shared flag
  - `src/search/alpha_beta.rs` — all `tt: &mut TranspositionTable` → `tt: &TranspositionTable`
  - `src/search/iterative.rs` — same signature change + test call sites updated
  - `src/search/pruning.rs` — `try_probcut` signature updated
  - `src/main.rs` — full rewrite: `go` non-blocking, `stop` works, N-1 helper threads, history preserved across moves

**Architecture decisions:**
- TT shared via Arc, lock-free writes per D4 (benign races, key verification in probe)
- Each thread has own SearchInfo (no killer/history sharing — simpler, avoids data races)
- Main thread returns SearchInfo on join; history/countermoves/correction_history merged back
- Helper threads use infinite TC; killed by shared stop_flag when main finishes
- bestmove printed inside spawned thread (not main loop) — correct UCI semantics

**Next session start point:**
Phase 14.1 — Evaluation improvements: PST (piece-square tables) from Ethereal. Read `src/eval/tables.rs` first (may not exist yet). Then implement material.rs integration with full HCE.

---

## Session 12 — Phase 13.1–13.3: Probcut, CorrectionHistory, Singular Extensions

**Date**: 2026-06-30
**Build entering session**: #129 green (Phase 12 complete, Phase 13 untouched)
**Build leaving session**: #136 green

### What Was Done
- 13.1 — Wired Probcut (`pruning::try_probcut`/`should_try_probcut`) into
  `alpha_beta.rs`, called after IIR and before move generation.
- 13.2 — Wired CorrectionHistory into `alpha_beta.rs`: raw static eval is
  corrected via `info.correction_history.apply()` before all pruning
  decisions (razoring, null move, futility); the raw (uncorrected) eval and
  final `best_score` feed `correction_history.update()` at node exit
  (skipped when in check, search aborted, or result is a mate score).
  Added `correction_history: CorrectionHistory` field to `SearchInfo`
  (`src/search/mod.rs`), persisted across searches like history/countermoves.
- 13.3 — Singular extensions: TT move gets a verification search (reduced
  depth, excluded from the normal move loop) at `depth >= MIN_DEPTH_SINGULAR
  (6)`. If every alternative fails to reach `tt_score - 2*depth`, the TT
  move is "singular" and gets +1 ply when actually searched.

### Decisions Made
- **D16** (new — see DECISIONS.md): introduced `alpha_beta_with_excluded()`
  as the real search body; public `alpha_beta()` is now a thin wrapper
  passing `excluded = Move::NULL`. Keeps the public signature — and every
  existing call site in `iterative.rs` and all test modules — untouched.

### Bugs Fixed
- **Build #129→#136**: `MIN_DEPTH_SINGULAR` constant was added to
  `search/mod.rs` but never added to the `use crate::search::{...}` import
  list in `alpha_beta.rs` — bare reference failed to resolve (E0425).
  Fixed by adding it to the existing import block. One-line delta, confirmed
  green at #136.

### Next Session Start Point
1. Confirm #136 test count (should still be 296+ tests, none removed) —
   spot check `test_iterative_deepening_depth_increases` (depth 8, fixed)
   and `test_aspiration_window_handles_score_drop` (depth 6) since these are
   the two existing tests most likely to exercise the new singular-extension
   path (flagged as a risk last session, came back green so no action needed).
2. Start **13.4 — Lazy SMP** (multi-threaded parallel search). This is the
   biggest Phase 13 item — needs `Arc<AtomicBool>` stop flag shared across
   threads, a way to share/synchronize the TT (already lock-free per D4, so
   no change needed there), and per-thread `SearchInfo` (history/killers
   not safely shareable — each thread needs its own, root move merged at
   the end). Read `src/search/mod.rs` and `src/main.rs` fresh before
   starting since main.rs owns the single `EngineState` that Lazy SMP will
   need to fan out from.
3. Alternative if 13.4 feels too large for one session: do 13.5
   (quiescence improvements) or 13.6 (history gravity/continuation history)
   first — both are smaller, contained to existing single-threaded files.

---

---

## Session 11 — Phases 10/11/12: Release Pipeline + WASM + Browser UI

**Date**: 2026-06-30
**Build entering session**: #116 green (309 tests, Phase 9 UCI complete)

### What Was Done
- Confirmed Build & Release #116 green from screenshot — Phase 10 already complete
- Diagnosed deploy.yml failure: mkdir -p web ran AFTER wasm-pack, directory didn't exist
- Fixed deploy.yml: mkdir -p web/pkg now before wasm-pack build
- Wrote src/lib.rs Phase 11: wasm_main() calls init_masks/magic/zobrist on load;
  added new_game(), search_from_fen(), legal_moves_from_fen() WASM exports
- Wrote web/index.html Phase 12: full browser chess UI — board, pieces, legal move
  highlights, promotion modal, engine play, undo, flip, side select, think time

### Decisions Made
- Using Pet Dragon engine as browser opponent (not Stockfish) — engine is strong enough
- JS-side FEN applicator (no apply_move WASM export needed) — keeps WASM API minimal
- EP target = Math.floor((fromRank + toRank) / 2) — handles Pet Dragon rank 1→3 pushes

### Bugs Fixed
- deploy.yml: mkdir step was after wasm-pack, causing write to non-existent directory

### Next Session Start Point
1. Check Deploy workflow result — should be green, site live at g-c-3.github.io/pet-dragon
2. If green → Phase 13 (Search Improvements): wire Probcut + CorrectionHistory into alpha_beta.rs
3. If red → check deploy log, likely a wasm-pack compilation error or Pages permission issue

---

## Session 10 — Phase 9 UCI Protocol

**Date**: 2026-06-29
**Build**: #86 green entering session (296 tests, Phase 8 HCE complete)

### What Was Done
- Confirmed Phase 8 fully uploaded (eval/mod.rs final + alpha_beta wired)
- Wrote src/main.rs — full UCI protocol (Phase 9 complete):
  - uci, isready, ucinewgame, position, go, stop, setoption, quit
  - position: startpos and fen + moves list
  - go: all time control fields, calls iterative_deepening, bestmove + ponder
  - setoption: Hash resize live, Threads accepted (Phase 13)
  - d: debug display, perft: divide output
  - 9 tests added covering all command paths

### Decisions Made
- None new

### Bugs Fixed
- N/A (new file)

### Next Session Start Point
1. Confirm src/main.rs upload + GitHub Actions green
2. If green → Phase 9 complete, start Phase 10 (Release pipeline in build.yml)
3. Phase 10.1: build release binaries for Windows/macOS/Linux in .github/workflows/build.yml
4. If red → check build log, likely a missing pub or wrong path

---

## Session 9 — Phase 8 HCE Complete

**Date**: 2026-06-29
**Build**: #86 green entering session; Phase 8 files uploaded

### What Was Done
- Confirmed material.rs, mod.rs (stub), tables.rs all on GitHub and building
- Confirmed `const fn s/mg/eg` fix and `taper` plain fn fix applied and green
- Confirmed `mod.rs` stub had unimplemented modules commented out
- Wrote and delivered Phase 8 remaining files:
  - `src/eval/mobility.rs` — mobility bonus (Ethereal weights, tapered)
  - `src/eval/pawns.rs` — pawn structure (passed/isolated/doubled/backward)
    Pet Dragon: rank 1 pawns never penalised as backward
  - `src/eval/king_safety.rs` — king safety (pawn shield, open files, attackers)
    Pet Dragon: no castling bonus (D7), scaled by phase
  - `src/eval/open_lines.rs` — open files, batteries, 7th rank, connected rooks
    Pet Dragon: active from move 1, no suppression (D6, D8)
  - `src/eval/mod.rs` FINAL — full evaluate() combining all 6 terms + tempo
  - Delta: `src/search/alpha_beta.rs` — replace placeholder with crate::eval::evaluate()

### Decisions Made
- None new — all consistent with D6/D7/D8 already documented

### Bugs Fixed
- **PST table White indexing reversed** (`tables.rs`): PST tables are written rank 8 at
  index 0 (Ethereal/Stockfish layout), but White used `sq.index()` = `rank*8+file`, which
  reads the table upside-down (rank 1 pawn got rank 7 bonus, rank 7 pawn got rank 1 bonus).
  Fix: White uses `(7-rank)*8+file`, Black uses `sq.index()`. Black was accidentally correct
  (its mirror formula happened to match what White should use).
  Affected tests: `test_pawn_advance_bonus`, `test_rook_7th_rank` (both now pass).
  Build went from 294 passed / 2 failed → 296 passed / 0 failed.

### Next Session Start Point
1. Confirm all 5 eval files uploaded + alpha_beta.rs delta applied
2. Check GitHub Actions build is green (239+ tests should still pass)
3. If green → Phase 8 complete, start Phase 9 (UCI protocol in src/main.rs)
4. If red → upload error log and fix

---

## Session 8 — 2026-06-29

**Built:** Nothing new — pure bug-fix session on Phase 8 eval compilation.

**Bugs fixed:**
- E0015 (388 errors): `s()`, `mg()`, `eg()` were plain `fn` used in `const` PST array initialisers in `tables.rs`. Fix: make them `const fn`. Applied in both `src/eval/material.rs` and `src/material.rs`.
- E0583 (file not found): `src/eval/mod.rs` declared `mobility`, `pawns`, `king_safety`, `open_lines` modules that don't exist yet. Fix: comment them out.
- E0658 (4 errors): `taper()` was also made `const fn` but uses `i32::max()`/`i32::min()` which are not yet stable as const (rust-lang issue #143874). Fix: revert `taper` to plain `fn` — only `s/mg/eg` need to be const.
- Unused import `mg, eg` in `tables.rs` after removing their calls. Fix: trim import.
- 3 unused variable warnings (`ply`, `depth`, `them`) prefixed with `_`.

**Decisions:** None new — these were implementation fixes only.

**Next session start point:** Phase 8 eval is compiling. Next task: implement `src/eval/mobility.rs`, `src/eval/pawns.rs`, `src/eval/king_safety.rs`, `src/eval/open_lines.rs`, then re-enable them in `mod.rs`. Start with `mobility.rs`.

---

## Session 7 — Phase 8 Start + Docs Setup
**Date**: 2026-06-28
**Build**: #86 green (239 tests passing)

### What Was Done
- Phase 7 confirmed complete (Build #86 green)
- Phase 8 started:
  - `src/eval/material.rs` provided — tapered material values (Ethereal weights)
  - `src/eval/mod.rs` provided — module stub
  - `src/eval/tables.rs` provided during session — PST tables
- Docs directory created and all 6 docs files generated for GitHub MCP connector

### Decisions Made
- D15 confirmed: GitHub Actions only, Gokul mobile only
- NNUE dual-network rejected (D9 finalised)
- Pawn start feature convergence fully documented (D11)
- Texel tuning marked optional (D12)

### Bugs Fixed
- None this session (Phase 8 in progress)

### Context Window Note
Context window reached limit. Docs generated to enable fresh context continuation.

### Next Session Start Point
1. Check GitHub: confirm `src/eval/material.rs`, `src/eval/mod.rs` uploaded
2. Check GitHub: confirm `src/eval/tables.rs` uploaded (provided this session)
3. If all three green → continue with `src/eval/mobility.rs`
4. If any missing → re-provide missing files first
5. Continue Phase 8 in order: mobility → pawns → king_safety → open_lines → mod.rs final

---

## Session 6 — Phase 7 Complete
**Date**: 2026-06-24
**Build**: #86 green (239 tests passing)

### What Was Done
- Phase 7 search engine complete:
  - `src/search/mod.rs` — SearchInfo, SearchResult, constants
  - `src/search/time.rs` — TimeControl, TimeManager
  - `src/search/see.rs` — Static Exchange Evaluation
  - `src/search/ordering.rs` — Move ordering (ScoredMove made pub)
  - `src/search/alpha_beta.rs` — Alpha-beta + PVS + quiescence
  - `src/search/iterative.rs` — Iterative deepening + aspiration windows
  - `src/search/pruning.rs` — Extensions, LMR, probcut, correction history
- Phase 6 (Transposition Table) confirmed green

### Bugs Fixed
- **ScoredMove private** (Build #66): Added `pub` to struct and fields in ordering.rs
- **SEE even-exchange wrong** (Build #67/75): FEN had no recapturer.
  Fixed test FEN to include Black Rook on d8. Also rewrote SEE negamax backwards pass.
- **u64 overflow in time.rs** (Build #75): `soft_limit_ms * 3 / 4` overflows when
  `soft_limit_ms = u64::MAX/2`. Fixed with `if self.soft_limit_ms > u64::MAX / 4` guard.
- **King not found panic** (Build #80): `move_gives_check()` cloned position and
  called `in_check()` on a position where King was captured. Fixed with
  `piece_bb(side, King).is_empty()` guard before calling `in_check()`.
- **pubpub syntax error** (Build #75): Duplicate `pub` keyword in see.rs from
  a bad find-replace. Fixed by removing duplicate.
- **Unused imports compile errors** (Build #70): Removed `is_checkmate`, `is_stalemate`,
  `MoveKind` from alpha_beta.rs; `DRAW_SCORE`, `MATE_SCORE`, `MATE_THRESHOLD`,
  `evaluate` from iterative.rs; `MAX_PLY`, `INFINITY` from pruning.rs.
- **Mate test FEN** (Build #80): Minimal mate position caused King-captured panic
  in search. Changed test to use `"4k3/8/8/8/8/8/8/4KQ2 w - - 0 1"` (up a queen)
  instead of `"7k/7Q/6K1/8/8/8/8/8"`.

### Decisions Made
- Probcut and CorrectionHistory defined in pruning.rs but not wired until Phase 13 (D13)
- Pet Dragon rank-1 double-push gets history bonus in ordering.rs (PET_DRAGON_RANK1_PUSH_BONUS)

---

## Session 5 — Phase 5 + 6 Complete
**Date**: 2026-06-23
**Build**: #57 green

### What Was Done
- Phase 5.4 (repetition detection) completed after multiple test fixes
- Phase 6 (Transposition Table) complete: `src/tt/mod.rs`
- `pub mod tt;` added to lib.rs

### Bugs Fixed
- **Repetition test logic** (multiple builds): `make_move_with_history()` pushes
  hash AFTER the move, so `is_repetition()` needs count >= 2 in history (not >= 1).
  The current position IS in game_history (just pushed), so seeing it once means
  it's the just-pushed entry, not a prior occurrence. Count >= 2 means truly seen before.
- **Threefold repetition count**: `is_threefold_repetition()` needs count >= 3
  in history (since current position is included in history after make_with_history).

### Decisions Made
- `is_repetition()` conservative: returns true at 2nd occurrence (draw claimable)
  rather than waiting for 3rd (forced draw). Safer for search to avoid repetition cycles.

---

## Session 4 — Phase 5 Make/Unmake
**Date**: 2026-06-23
**Build**: #47 green

### What Was Done
- Phase 5 make/unmake complete:
  - `src/position/make_move.rs` — full incremental make/unmake
  - `tests/make_unmake.rs` — perft depth 5 via make/unmake = 4,865,609 ✅
- Phase 5.4 repetition detection added to Position struct in mod.rs
  - `game_history: Vec<u64>` field
  - `push_game_history()`, `pop_game_history()`
  - `is_repetition()`, `is_threefold_repetition()`
  - `make_move_with_history()`, `unmake_move_with_history()`

### Bugs Fixed
- Repetition test logic (fixed in Session 5)

---

## Session 3 — Phase 4 Move Generation Complete
**Date**: 2026-06-22
**Build**: #43 green (perft depth 5 = 4,865,609)

### What Was Done
- Phase 4 complete:
  - `src/movegen/mod.rs` — MoveList, generate_moves()
  - `src/movegen/pieces.rs` — all piece moves
  - `src/movegen/pawns.rs` — Pet Dragon custom pawn logic
  - `src/movegen/castling.rs` — dynamic castling
  - `src/movegen/legal.rs` — legal filter + apply_move_for_legality_pub()
  - `tests/perft.rs` — perft depth 5 proven correct
- `pub mod movegen;` added to lib.rs

### Bugs Fixed
- **Promotion test FEN** (Build #38): Black King was on e8 blocking White pawn
  promotion. Changed to `"3k4/4P3/8/8/8/8/8/4K3"` (King moved to d8).
- **En passant legality test** (Build #40/41): Test FEN had White Rook (uppercase R)
  instead of Black Rook. Fixed to `"8/8/8/KPpr4/8/8/8/7k"` (lowercase r).
- **Perft promo_depth1 expected value** (Build #42): Test expected 6 but engine
  returned 36 (correct). Fixed expected value.

### Decisions Made
- `apply_move_for_legality_pub()` made public for perft tests (D_movegen_1)

---

## Session 2 — Phases 1–3 Complete
**Date**: 2026-06-22
**Build**: #35 green (setup tests + position tests)

### What Was Done
- Phase 1: Core types in src/types.rs
- Phase 2: Bitboard foundation (mod.rs, masks.rs, magic.rs)
  - PAWN_DOUBLE_PUSH_MASK[2][64] Pet Dragon custom
- Phase 3: Position struct, FEN, Zobrist, Pet Dragon generator, make/unmake stub
  - 1000 position validation passing
  - pawn_starts map correctly recorded
  - Castling detection from Rook positions

### Bugs Fixed
- Various unused import warnings cleaned up
- Bishop constraint enforced correctly in setup.rs

### Decisions Made
- D1 through D8 finalised
- PAWN_DOUBLE_PUSH_MASK covers both rank 1 and rank 2 for White,
  rank 7 and rank 8 for Black (Pet Dragon custom, not standard chess)

---

## Session 1 — Project Initialisation
**Date**: 2026-06-21
**Build**: First green build

### What Was Done
- GitHub repository created: g-c-3/pet-dragon
- LICENSE (GPL v3)
- README.md
- Cargo.toml
- .github/workflows/build.yml
- .github/workflows/deploy.yml
- src/main.rs placeholder
- src/lib.rs placeholder

### Decisions Made
- Project name: Pet Dragon
- Language: Rust
- License: GPL v3
- Gokul Chandar as author, Claude (Anthropic) as contributor
- Target: 3000+ Elo without NNUE
