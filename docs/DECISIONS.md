# DECISIONS.md
# Pet Dragon — Architectural Decisions

## Format
Each decision records: what was decided, why, and what alternatives were rejected.

---

## D1 — Pure Rust, No Forks
**Decision**: Write the engine from scratch in Rust. No forking Stockfish, Leela, or any other engine.

**Why**: Pet Dragon's pawn rules (double-step from rank 1 OR rank 2) are fundamentally different from standard chess. Patching this onto an existing engine would require modifying move generation, evaluation, and NNUE feature extraction in ways that could break existing correctness guarantees. A clean implementation is safer, more maintainable, and conceptually cleaner.

**Rejected**: Forking Stockfish (GPL v3 compatible) — would work legally but Pet Dragon's custom pawn logic would be fighting the engine's assumptions at every layer.

---

## D2 — PawnStartMap Custom Type
**Decision**: Track each pawn's actual starting square in a `PawnStartMap([Option<Color>; 64])` that is set at game creation and never modified during play.

**Why**: Pet Dragon's double-step rule depends on whether a pawn is still on its original starting square — not on its current rank. A White pawn that has moved from rank 1 to rank 2 cannot double-step, even though it's now on rank 2. A pawn that started on rank 2 and is still there can double-step. The only way to correctly distinguish these cases is to record the original starting square.

**Key insight**: If a pawn is still on its starting square → it hasn't moved → double-step eligible. If it has moved away → it's no longer on its starting square → no double-step. No separate "has this pawn moved" flag needed.

**Rejected**: Tracking by current rank — fails the rank1→rank2 case. Tracking by "has moved" flag in the move struct — adds complexity to make/unmake.

---

## D3 — Dynamic Castling Rights
**Decision**: At game setup, detect whether each Rook landed on its standard square (a1/h1/a8/h8) and set castling rights accordingly. Rights are then managed exactly like standard chess from that point.

**Why**: The White King is always on e1 (and Black King always on e8), so the only variable is Rook positions. If a Rook randomly lands on h1, kingside castling is available; otherwise it never will be. This gives ~26% of games some castling availability.

**Implication**: ~74% of Pet Dragon games have no castling. King safety evaluation must not assume castling has occurred or will occur.

---

## D4 — Lock-Free Transposition Table
**Decision**: Use a single flat array, no mutexes, accept benign data races.

**Why**: This is the standard Stockfish approach. A race condition in the TT causes at most a corrupted entry being read or written — the engine might make a slightly worse move in that rare case, but it never crashes and the performance gain from avoiding locks is substantial at high NPS.

---

## D5 — Borrowed Evaluation Weights
**Decision**: Use piece values and PST tables from Ethereal (GPL v3, Andrew Grant) for the initial HCE.

**Why**: Ethereal's weights are world-class, tuned over millions of self-play games. Building our own weights from scratch would require extensive self-play before they became competitive. Borrowing proven weights lets us reach ~2400-2600 Elo immediately with proper attribution.

**Attribution required**: All borrowed code/weights must credit: "Values borrowed from Ethereal chess engine (GPL v3, Andrew Grant)."

---

## D6 — No Opening Suppression in Evaluation
**Decision**: All evaluation terms apply at full weight from move 1.

**Why**: Standard engines reduce mobility/pawn structure weights in the opening to avoid aggressive early play before development. Pet Dragon has no quiet opening — pieces are already randomly placed on ranks 1-2, open files and diagonals exist immediately. Suppressing eval terms in the "opening" would incorrectly ignore real positional features present from the start.

---

## D7 — King Safety Without Castling Bias
**Decision**: King safety evaluation based purely on pawn shield (pawns near king), piece proximity, attack count, and open files through king. No bonus for having castled.

**Why**: ~74% of Pet Dragon games have no castling at all. A king safety bonus for castling would heavily penalise 74% of all games for something that never happened and couldn't have happened (if the Rook wasn't on the standard square). The pawn shield approach is agnostic to whether castling occurred.

---

## D8 — Open Line Detection from Position 0
**Decision**: Battery detection, open file bonus, and contested file penalties apply from depth 0 (the starting position).

**Why**: Pet Dragon starting positions have ranks 3-6 completely empty. Rooks on rank 1/2 face each other across open files immediately. Batteries (Queen+Rook on same file, Queen+Bishop on same diagonal) exist in starting positions. A standard engine that only detects open lines after "development" would miss these.

---

## D9 — Single Pet Dragon NNUE (Phase 16)
**Decision**: Train one NNUE specifically on Pet Dragon data. Do not implement dual-network architecture (Pet Dragon NNUE + Stockfish NNUE).

**Why**: Even when all pawns have passed their starting ranks, Pet Dragon piece arrangements are alien to Stockfish NNUE training data. A Rook that started on b2 (Pet Dragon) vs a Rook that arrived at b4 from h1 (standard chess) are in identical positions mid-game, but Stockfish NNUE never saw the b2-start arrangement in its training data. Our Pet Dragon NNUE, trained on Pet Dragon self-play, handles all arrangements correctly.

**Additional reason**: No clean switching point exists. There's no game state that definitively signals "now we're in standard chess territory."

---

## D10 — NNUE Feature Set: 896 Inputs
**Decision**: 768 standard piece-square features + 128 pawn start square features.

**Why**: The 768 standard features allow the network to learn piece coordination. The 128 pawn start features allow the network to distinguish "pawn on rank 2 that can still double-step" from "pawn on rank 2 that has already moved." This distinction is critical for Pet Dragon correctness and is the minimum addition to HalfKP-style features.

**Phase convergence**: Pawn start features become 0 as pawns leave starting squares. By middlegame/endgame the network evaluates using only standard 768 features — functionally converging to standard chess NNUE behaviour. No switching logic needed.

---

## D11 — Pawn Start Feature Convergence
**Decision**: The moment a pawn makes its FIRST MOVE — regardless of destination — its start feature becomes 0.

**Precise definition**:
- Rank 1 pawn → rank 2 (single step): start feature = 0 (already moved, cannot double-step even though now on rank 2)
- Rank 1 pawn → rank 3 (double step): start feature = 0
- Rank 2 pawn → rank 3: start feature = 0
- Rank 2 pawn → rank 4: start feature = 0

**Critical distinction the network learns**:
- "Pawn on rank 2, started rank 2" → CAN double-step → start feature active
- "Pawn on rank 2, started rank 1" → CANNOT double-step → start feature = 0

These have DIFFERENT feature encodings → different evaluation → correct behaviour.

---

## D12 — Texel Tuning Is Optional
**Decision**: Phase 14 (Texel tuning) is marked optional. Skip if going directly to NNUE (Phase 16).

**Why**: NNUE will outperform even perfectly Texel-tuned HCE by a large margin (~300-500 Elo). Texel tuning is a stepping stone that improves HCE quality and therefore NNUE training data quality, but borrowed Ethereal weights are sufficient for initial NNUE training. Decide after Phase 13 based on engine strength at that point.

---

## D13 — Probcut and CorrectionHistory Defined but Not Wired in Phase 7
**Decision**: Define and test Probcut + CorrectionHistory in Phase 7 (`pruning.rs`) but do not call them from the search loop until Phase 13.

**Why**: Adding advanced pruning techniques before the evaluation is complete makes debugging impossible. A pruned branch might have the correct evaluation but we can't verify this without Phase 8. Phase 13 wires everything in together and measures Elo gain of each technique in isolation.

---

## D14 — Training Data Bootstrap Strategy (Phase 16)
**Decision**: Include standard chess game positions (Lichess CC0 dataset) in NNUE training data alongside Pet Dragon self-play.

**Why**: Standard chess positions are abundant, well-evaluated, and represent the middlegame/endgame patterns our network will encounter. Including them bootstraps the network with millions of already-evaluated positions before Pet Dragon self-play data becomes sufficient. The single network learns both Pet Dragon specific dynamics AND standard chess patterns simultaneously.

---

## D15 — GitHub Actions Only, No Terminal
**Decision**: All building, testing, and deployment via GitHub Actions. Gokul never runs cargo commands.

**Why**: Gokul has mobile only. GitHub Actions provides the CI/CD pipeline. Every file Claude produces must be complete and ready to upload directly to GitHub via the web UI. This is a hard constraint, never violated.

---

---

## D16 — Singular Extension via Wrapper Function, Not Signature Change
**Decision**: Implement singular extensions by keeping `pub fn alpha_beta(...)` with its original 9-argument signature as a thin wrapper, and moving the actual search body into a private `fn alpha_beta_with_excluded(..., excluded: Move)`. All internal recursive self-calls within the search body call `alpha_beta_with_excluded` directly (with `excluded = Move::NULL` for normal recursion); only the singular-verification search passes `excluded = tt_move`.

**Why**: Singular extension verification needs to search the current position with one specific move (the TT move) excluded from the move loop, to answer "how good is the position without it?" Adding an `excluded: Move` parameter to the existing public `alpha_beta()` would have required touching every call site — `iterative.rs` (2 sites) and every test in `alpha_beta.rs` and `iterative.rs` that constructs an `alpha_beta(...)` call directly. The wrapper approach confines the change entirely to `alpha_beta.rs` with zero edits needed outside that file.

**Rejected**: Adding `excluded: Move` directly to `alpha_beta()`'s public signature — more "standard" looking but would have cascaded into `iterative.rs` and ~10 test call sites for no functional benefit, increasing the size and risk of an already nontrivial delta.

## D17 — pyrrhic-rs SyzygyPath re-set not supported mid-process (2026-07-04)
`pyrrhic_rs::TableBases` is a process-wide singleton internally. Re-calling
`setoption SyzygyPath <new path>` after a successful init will return
`Err(AlreadyInitialized)` regardless of path validity. Decision: accept this
as expected UCI behavior for now (GUIs set SyzygyPath once at startup); do
not add re-init workaround unless a real compatibility issue is reported.

## D18 — Lichess Sampler: Prefix Approximation, Not Uniform Sampling (2026-07-04)
**Decision**: `lichess_sample.rs` samples a skip+stride *prefix* of the
decompressed Lichess CC0 eval stream (skip N lines, then keep every Kth
line until sample_size is reached, then close the connection) rather than
a statistically uniform sample of all 388M positions.

**Why**: Standard `.zst` frames only support sequential decompression from
the start — no seeking to an arbitrary decompressed byte offset. A true
uniform/reservoir sample across the whole file would require decompressing
all 388M positions, which is not feasible in a GitHub Actions job's time
and bandwidth budget. Stopping early and dropping the HTTP connection once
`sample_size` is reached also means bytes past that point are never
downloaded, keeping each run cheap.

**Implication for training data (Phase 16.5)**: this sample may be biased
toward whatever ordering Lichess's export uses (e.g. earlier-logged
positions). Acceptable for bootstrapping per D14 (self-play is the primary
Pet Dragon-specific signal; Lichess data is a supplementary standard-chess
bootstrap). Revisit only if Colab training shows a bias traceable to file
ordering.

**Dependency choices**: `ruzstd` (pure Rust, MIT, no libc) over the `zstd`
crate (C bindings) — avoids adding a system zstd requirement to the CI
runner. `reqwest` with `blocking` + `rustls-tls` features (not default TLS)
— avoids an OpenSSL dependency and an async runtime for a single
sequential CI job. All three (`ruzstd`, `reqwest`, `serde_json`) are
optional, gated behind a `lichess-sample` feature absent from both
`default` and `wasm` — cannot affect the native release binary, `cargo
test`, or the WASM/browser bundle.

## D18.5 — Phase 16.5 Training Platform: Kaggle Notebooks, Not Colab (2026-07-04)
**Decision**: Phase 16.5 (NORU NNUE training on combined selfplay + Lichess
data) will use Kaggle Notebooks instead of Google Colab.

**Why**: Gokul is mobile-only (no terminal, no desktop — see CORE RULES).
Kaggle's "Save Version → Run All" (commit) mode keeps a notebook running
on Kaggle's cloud servers even if the tab is closed, the phone locks, or
the connection drops — confirmed via Kaggle's own community docs, this
survives disconnects by design, unlike Colab's free tier which has no
real equivalent walk-away/background-commit mode. This matters directly
for a phone-only workflow where the notebook can't be babysat.

**Caveat carried forward into the Phase 16.5 notebook design**: this
disconnect-proof behavior applies to commit/"Run All" mode only —
Kaggle's *interactive* editing mode has its own inactivity timeout (~40min
idle, "are you still there?" prompts, reported loss of interactive session
state after ~12h). The Phase 16.5 notebook must therefore be written to
run end-to-end via Save Version → Run All from the start, not built up
interactively cell-by-cell, with periodic checkpoint saves to
/kaggle/working/ along the way.

**Implication**: any file paths in the Phase 16.5 notebook use Kaggle's
conventions (/kaggle/input/, /kaggle/working/), not Colab's Drive-mount
paths.

## D19 — NNUE Training via GitHub Actions, Not Colab (2026-07-05)
**Decision**: Phase 16.5 training runs as a GitHub Actions workflow
(train_nnue.yml) rather than a Google Colab notebook.

**Why**: NORU is pure Rust with no GPU-dependent training path (FP32
backprop is CPU-only), so Colab's main advantage (free GPU) doesn't apply.
Running it in Colab would still require Gokul to paste and run Rust/cargo
commands in notebook cells — functionally the same as a terminal, which
violates D15 (GitHub Actions handles all building, Gokul never runs cargo).
An Actions workflow with workflow_dispatch inputs achieves the same
"trigger training with custom hyperparameters from mobile" goal while
keeping the one-button-tap UX consistent with every other phase.

**Rejected**: Colab notebook (as originally noted in Phase 16 ROADMAP
comments from Session 7) — rejected once it became clear NORU has no
GPU-training benefit to justify breaking the Actions-only convention.

## D20 — Kaggle Backgrounding as Documented Fallback, Not Default (2026-07-05)
**Decision**: GitHub Actions remains the default for all training workloads
(Texel tuning if revisited, NNUE training). Kaggle (background execution) is
documented as the fallback path, triggered only if either condition is met:
(a) NORU or a future eval component gains a GPU-dependent training path, or
(b) self-play/Lichess dataset size grows large enough that a single training
run approaches GitHub Actions' 6-hour job ceiling.

**Why**: Kaggle's main advantage over Actions is free GPU + long background
runs. NORU's training is FP32 CPU backprop with no CUDA path, and current
dataset sizes (tens of thousands of rows) finish in minutes on Actions. Using
Kaggle today would add manual artifact-shuffling steps (download from
Actions → upload to Kaggle → run → download → re-upload to repo) with no
compute benefit, breaking the one-tap Actions workflow convention (D15/D19).

**Trigger conditions for revisiting**: dataset row count exceeds roughly
1-2M rows (self-play + Lichess combined) such that a training run's wall
time starts competing with Actions' free-tier minutes/time limits, or a
future NNUE architecture change requires GPU-accelerated training that
NORU's CPU-only trainer can't provide.

**Rejected (for now)**: Adopting Kaggle as the default training runner —
rejected until one of the trigger conditions above is actually hit; no
benefit today, real UX cost.

## D21 — Kaggle for Self-Play Generation, Actions Minutes Conserved (2026-07-05)
**Decision**: Self-play data generation moves to a Kaggle notebook
(kaggle/pet_dragon_selfplay_kaggle.ipynb) run via "Save & Run All (Commit)"
background execution, rather than GitHub Actions selfplay.yml. train_nnue.yml
gains a selfplay_paths input (comma-separated repo-committed file paths) so
Kaggle output can be committed directly and consumed without an Actions
artifact round-trip; selfplay_run_id becomes optional, kept for occasional
small GH Actions batches.

**Why**: Gokul runs multiple GitHub projects sharing Actions minutes/limits.
Kaggle compute is a separate free quota, supports true background execution
(commit-and-close, ~9-12h ceiling vs Actions' 6h), and needs no terminal —
fits the mobile-only constraint (D15) as well as Actions did. This supersedes
D20's original framing (Kaggle only if GPU or dataset-size forced it) — the
real trigger turned out to be cross-project Actions budget pressure, which
D20 didn't anticipate.

**Rejected**: Keeping self-play exclusively on GH Actions with larger/more
batches — rejected once Actions minutes became a shared, contended resource
across Gokul's other projects.

## D22 — GitHub Releases for Data Files Over 25MB (2026-07-05)
**Decision**: Self-play/training data files too large for the repo's
web-upload UI (25MB cap) are attached as GitHub Release assets instead
(supports far larger files via the same mobile web UI), and train_nnue.yml
gained a selfplay_urls input that curls them directly. selfplay_paths
(committed repo files) remains for smaller files.

**Why**: the first Kaggle-generated file (66MB after ~93 games) hit the
repo-content upload page's 25MB ceiling. Git LFS would need CLI setup
Gokul can't do from mobile; Releases needed no new tooling and reused the
same upload flow he already knows.

**Rejected**: Git LFS — requires CLI/terminal setup, violates D15.

## D23 — NNUE Blend Weight Fixed at 0.25 Pending Elo Testing (2026-07-06)
**Decision**: Phase 16.6 integrates the trained Pet Dragon NNUE (Phase 16.5)
as a 25%-weighted blend with the existing HCE (`eval::evaluate_blended() =
0.75*HCE + 0.25*NNUE`), not a full replacement of HCE, and not a
configurable/tunable weight for now — a fixed constant in `eval/mod.rs`.

**Why**: The first (and only) trained network has val_loss=0.53776 —
meaningfully better than the 0.693 coin-flip baseline, but still far from a
confident predictor. Session 32's own handoff note already flagged this as
the reason to start with a blend rather than a replace. A low, fixed weight
lets the NNUE contribute real signal immediately without risking a
regression from over-trusting an undertrained network; the weight can be
raised (or the whole approach reconsidered) once actual Elo testing against
the current pure-HCE build is available.

**Rejected**: Full HCE replacement — premature given val_loss is still well
above a confident-prediction threshold; borrowed Ethereal HCE weights
(D5) are proven to ~2400-2600 Elo and shouldn't be discarded on a first
training run. Also rejected: making the blend weight a UCI-tunable option
immediately — adds surface area before there's any Elo data to tune
against; revisit once benchmarking exists.

## D24 — Wire info.time_allocated_ms to the Real TimeManager Hard Limit (2026-07-06)
**Decision**: `iterative_deepening()` now sets `info.time_allocated_ms =
hard_ms` immediately after constructing its `TimeManager`, so
`alpha_beta`'s in-search `is_time_up()` check has the actual per-move time
budget to compare against, instead of `SearchInfo::new()`'s hardcoded
5000ms default.

**Why**: `is_time_up()`'s mid-search abort was silently dead code for every
real (UCI/iterative-deepening) search since it was introduced — only
`alpha_beta.rs`'s own unit tests, which set `time_allocated_ms` by hand,
ever exercised it. Real games were relying entirely on
`TimeManager::should_start_next_depth()`'s between-depths check, meaning a
single slow depth could run arbitrarily long past the allocated budget.
This was invisible while HCE-only eval was fast enough that individual
depths rarely overran; Phase 16.6's NNUE blend raised per-node cost enough
to expose it via a failing CI test.

**Impact**: this is a real robustness fix for actual play, not just a test
fix — any future eval slowdown (or a slow position with a large branching
factor) could previously have caused a real time forfeit in UCI play. Now
bounded by the true hard limit, checked every 256 nodes (see 2048→256
change, same session).

**Rejected**: loosening the test's 500ms ceiling instead of finding the
real wiring bug — would have hidden a genuine time-management defect rather
than fixing it.

## D25 — NNUE blend weight dropped to 0% default (2026-07-07)

Phase 17.4 match: Engine A (0% NNUE) vs Engine B (25% NNUE, D23's fixed
default), 20 games, 100ms/move, colors alternated. Result: A scored 87.5%
(17 wins, 2 losses, 1 draw), **+338 Elo** in favor of pure HCE. This is a
large, unambiguous gap — the trained network (val_loss=0.538, already
flagged in D23 as not yet confident) is actively harmful at 25% weight,
not merely under-contributing.

Decision: `NNUEWeight` UCI option default changed 25 → 0 (src/eval/mod.rs,
src/main.rs). The blend mechanism itself (Phase 17.1) stays — `NNUEWeight`
remains a live, settable option, so this isn't a rollback of Phase 16-17
work, just a correction of the default now that real Elo data exists.
Re-enabling any nonzero weight should wait for either (a) a retrained
network with materially lower val_loss, or (b) rerunning match_runner at
several weight values (5/10/15/20%) to find where it stops being net
negative — don't re-raise the default from vibes a second time.


## D26 — Sweep Existing match_runner Before Retraining or Architecture Changes (2026-07-07)

**Decision**: For Phase 17.5 (bringing val_loss down / deciding whether any
nonzero NNUE weight is usable), the first action is a 4-run sweep of the
existing `match_runner` workflow — A=0% vs B={5,10,15,20}%, 40 games each,
seed_start=0 — NOT a bigger Kaggle self-play run and NOT a hidden_size
architecture change.

**Why**: D25 only tested one point (25% weight, decisively bad). We don't
yet know whether *all* nonzero weight is harmful (network problem, points at
retraining) or only high weight is harmful (tuning problem, current network
is fine at a lower default). The sweep uses infrastructure that already
exists (Phase 17.2/17.3, zero new code) and answers this in one Actions
session, before spending Kaggle time/compute on a guess.

**Rejected (for now)**: Immediately rerunning bigger Kaggle self-play (the
original 3000-game target, cut short to ~93 by queue delay in Session 30) —
rejected until the sweep confirms the network itself is the bottleneck, not
just the blend weight. Also rejected: bumping hidden_size before the sweep —
same reasoning, don't change the model before knowing whether the model is
actually the problem.

**Revisit**: once the 4 match_results.txt artifacts are read (see
SESSION_LOG.md Session 38 next-start-point for the exact decision branches).

## D27 — No Safe Nonzero NNUE Weight at Current Network Quality; Retrain Before Re-Testing (2026-07-07)

**Decision**: The D26 sweep (A=0% vs B=5/10/15/20%, 40 games each) showed
every nonzero weight net-negative, including 5% (65.0% score, +107.5 Elo —
not a marginal or noisy result). Conclusion: don't look for a "safe" low
default: commit to retraining the network with substantially more self-play
data before re-enabling any blend weight. NNUEWeight stays at 0% (D25).

**Why**: If the problem were purely "25% overweights a decent-but-imperfect
network," we'd expect the Elo gap to shrink smoothly toward ~0 as weight
drops, with maybe 5% landing close to neutral. Instead 5% is still a
large, clear loss — consistent with D23's original flag that val_loss=0.538
is far from a confident predictor at *any* blend strength, not just at 25%.

**Rejected**: Setting the default to some low nonzero value (e.g. 5%) anyway
on the theory that "some signal is better than none" — rejected because the
sweep shows 5% is actively harmful, not merely weak; a harmful-but-small
signal is still harmful.

**Next step**: Session 39 committed Gokul to running self-play in 4 smaller
Kaggle batches (750 games each, different seeds) instead of one large
attempt, specifically to avoid repeating the queue-delay shortfall that
produced the current undertrained network (Session 30: ~93/3000 games
actually completed). Once retrained, the match_runner sweep must be rerun
before touching NNUEWeight's default — improved training data is not
assumed to fix things until re-measured.

**Revisit**: once the 4 match_results.txt artifacts are read (see
SESSION_LOG.md Session 38 next-start-point for the exact decision branches).

## D28 — SearchInfo.print_info Gate for Silent-Search Callers (2026-07-07)

**Decision**: Added `SearchInfo.print_info: bool` (default `true`). `iterative_deepening()`'s UCI `info depth ...` println is now gated on it. `selfplay.rs` and `match_runner.rs` set it `false`; `main.rs`'s real UCI loop leaves it at the default, unchanged.

**Why**: Kaggle self-play versions #7 and #9 (seeds 200/400) showed "Failed" despite completing all 750/750 games correctly — the actual crash was a `CellTimeoutError` in the *next* notebook cell, caused by `capture_output=True` trying to relay ~56,000 lines of UCI info-string stdout (10 depths × dozens of moves × 750 games) through Jupyter's IOPub channel, which papermill caps at a 4-second per-message timeout. The data generation itself never failed.

**Rejected**: Working around it purely on the notebook side (e.g. not printing `result.stdout`) — rejected because the same unbounded stdout volume would still hit other consumers (GitHub Actions log truncation in `selfplay.yml`/`match_runner.yml`, slower subprocess pipes in general) and the actual defect (silent-search callers inheriting UCI-loop printing they never needed) is a one-line source fix, not a workaround.


## D29 — Pause the Data-Volume Lever; val_loss Improving ≠ Game Strength Improving (2026-07-07)

**Decision**: After retraining with ~3x the self-play games (Session 42:
val_loss 0.53776 -> 0.51661), the re-swept match_runner results got WORSE
at every single blend weight, not better (5/10/15/20% all moved further
net-negative, consistently). Stop feeding more self-play data into training
as the default response to a bad Elo result — it's now failed to help twice
in the same direction. Pivot to a direct eval-calibration diagnostic
(17.5d, eval_diag.rs) before any further training runs.

**Why**: A lower BCE loss on the training/validation distribution is not
the same claim as "better evaluation quality on positions search actually
visits." The consistent, monotonic worsening across all 4 weight points
(not scattered/noisy) suggests the network is becoming more *confident*
without becoming more *correct* in the ways that matter for move selection
— and a confidently-wrong eval poisons alpha-beta pruning/move-ordering
more than a mild one does. Verified this isn't a simple scale bug: the
cp<->logit constant (400.0) and OUTPUT_SCALE are consistent between
train_nnue.rs and inference.rs.

**Rejected**: Immediately queuing an even-bigger self-play run — rejected
because the pattern (better loss, worse Elo) already repeated once; a third
attempt at the same lever without understanding why the first two failed
would be guessing, not debugging.

**Revisit**: once eval_diag.rs's output is read (Session 43 next-start-point
lays out the two branches: quantization bug vs deeper feature/target
formulation issue in D9/D10/D14).

## D30 — NNUE Output Clamp; Root Cause is Unregularized Logit Magnitude, Not Feature Design (2026-07-07)

**Decision**: Added `NNUE_EVAL_CLAMP_CP = 1500` in `inference.rs`, applied
to every `evaluate_nnue()` call before blending. Tightened
`test_evaluate_nnue_start_pos_bounded`'s bound from the old `< 5000` (which
never caught this) to `<= NNUE_EVAL_CLAMP_CP`, and added a dedicated clamp
test.

**Why**: `eval_diag.rs` (17.5d) showed the Session 42 network scoring the
symmetric start position at +2425cp (should be ~0) and a single queen swing
at ~4000-4500cp (HCE: ~976cp) — a raw fp32 logit around 6-16, meaning the
network expresses near-100% certainty at trivially non-decided positions.
All 5 test cases agreed with HCE on *sign*, ruling out a feature-design or
scale-constant bug (checked: `CP_TO_WINPROB_SCALE=400.0` matches between
`train_nnue.rs` and `inference.rs`). The pattern matches classic BCE
overconfidence: with no weight regularization, gradient descent has no
penalty for pushing output-layer weights toward unbounded magnitude on
training examples it can cleanly separate, and loss (val_loss) keeps
improving even as the raw output becomes progressively less sane. This
directly explains D29's paradox (better val_loss, worse Elo at every blend
weight): a more confidently-extreme eval corrupts alpha-beta pruning more
than a mild one does, regardless of whether its *sign* is usually right.

**Rejected**: Assuming this needed another retraining cycle before testing
— rejected because clamping is a one-file, no-Kaggle-needed change that
tests the hypothesis directly; a retrain-first approach would have cost
another hour+ round trip to learn the same thing.

**Revisit**: once the clamp-only re-sweep result comes back. If it
meaningfully improves the 5/10/15/20% numbers, the long-term fix is weight
decay in `train_nnue.rs` (the clamp is a safety net, not a substitute for
training a properly-scaled network). If it doesn't move much, the problem
is deeper than raw magnitude and D9/D10/D14 (feature/target design) need
the harder look Session 43 flagged as the fallback branch.

## D33 — Stronger Weight Decay + Gradient Clipping (2026-07-08)

**Decision**: Raised `train_nnue.rs`'s `weight_decay` default 100x
(1e-4 → 0.01) and added global-norm gradient clipping (`grad_clip_norm`,
default 1.0), applied to the whole gradient including biases, before every
Adam step.

**Why**: The corrected in-distribution `eval_diag` (D32) showed
`weight_decay=0.0001` did essentially nothing — real Pet Dragon random
starts (seed=2, seed=3) were still fully saturating at the clamp ceiling.
Weight decay alone only pulls weights toward zero passively each step;
gradient clipping directly caps the update that causes a blowup in the
first place, a standard complementary pairing.

**Result**: val_loss held steady (0.51636, best epoch 4/10 — matching or
slightly beating every unregularized attempt) while calibration improved
substantially (seed=2: 1500→375, seed=3: 1500→50, K+P: 1225→225). Confirms
the fix worked exactly as intended: better-behaved output without trading
away fit quality.

**Rejected**: Weight decay alone without clipping, or a smaller decay bump
— rejected because 1e-4 was already shown insufficient, and clipping
addresses the mechanism (unbounded per-batch updates) more directly than
decay's passive shrinkage alone.

## D34 — Park Phase 17.5 NNUE Tuning at hidden_size=32 (2026-07-08)

**Decision**: After 4 independent fix attempts (original network, 3x
self-play data retrain, output clamp, clamp+regularization) all landing in
the same ~70-72% average-opponent-score band on the match_runner sweep,
stop iterating on NNUE tuning at the current `hidden_size=32` scale.
NNUEWeight stays 0% (unchanged since D25). The clamp, weight_decay, and
grad_clip_norm defaults all stay — they're correct regardless, just not
sufficient alone to make blending worthwhile yet.

**Why**: D30's clamp and D33's regularization were both real, verified
fixes — eval_diag confirms genuinely better calibration each time, not
placebo changes. Yet the final sweep (5/10/15/20%: 66.2/68.8/66.2/80.0%,
avg 70.3%) is statistically indistinguishable from clamp-only's 70.0%
average, and both are in the same range as the very first D27 sweep
(72.5% average) despite completely different underlying networks. That
consistency across genuinely different interventions is the signal: the
ceiling isn't calibration or data volume anymore (both were real problems,
both got fixed), it's most likely that `hidden_size=32` is too small to
learn positional understanding beyond what HCE's hand-crafted tables
already encode — so blending in a well-calibrated-but-not-more-informed
network still doesn't out-predict HCE where it matters for search.

**Rejected**: Continuing to tune hyperparameters (lr, lambda, epochs) at
the current architecture — rejected because the pattern across 4 attempts
with materially different fixes each landing in the same band is stronger
evidence of a structural ceiling than of an undiscovered hyperparameter.
Also rejected: reverting the clamp/regularization work — rejected because
both are real, measured improvements and correct defaults for whenever
NNUE work resumes, regardless of today's Elo verdict.

**Revisit**: only via a deliberately bigger architecture (hidden_size=128+),
treated as a fresh, separately-scoped effort (bigger Kaggle training job,
new session budget) — not as an incremental extension of this arc.

## D35 — Texel Tuning Architecture: HCE is Linear-in-Weights (2026-07-08)

**Decision**: Full audit of all 6 eval/*.rs submodules + mod.rs's TEMPO
confirmed HCE is entirely linear-in-weights (~970 tunable parameters).
Tuning approach: extract a per-position FEATURE VECTOR once (counts/
indicators derived from board state), then fit via efficient gradient
descent (logistic-regression style dot-product) against the 147,867-sample
game database (14.2), NOT expensive per-parameter coordinate descent or
re-running full evaluate() thousands of times per position.

**Audit findings by module**:
- material.rs, tables.rs (PST), pawns.rs, open_lines.rs: pure sum of
  (board-derived count/boolean feature × tunable const), tapered by a
  per-position phase scalar (phase itself, from PHASE_WEIGHTS, is NOT
  tunable — standard practice, phase weights are structural).
- mobility.rs, king_safety.rs's ATTACKER_WEIGHT: table lookups indexed by
  a clamped count (e.g. `mobility.min(8)`) — one-hot bucket selection,
  still linear-in-weights (exactly one table entry active per position).
- ONE genuine nonlinearity: king_safety.rs's `.min(MAX_KING_DANGER)` value
  clamp on the danger term — handled like any ML clip/ReLU (zero gradient
  when clamped, pass-through otherwise).

**Implementation plan (NOT YET BUILT — next session)**:
1. `TexelFeatures` struct: one field per tunable parameter (or array field
   for table-shaped ones — PST tables, mobility tables, PASSED_PAWN_BONUS,
   ATTACKER_WEIGHT), extracted from a Position in one pass, mirroring each
   submodule's board-scanning logic exactly.
2. `TunableWeights` struct: same shape as TexelFeatures but holding f64
   weight values, initialized FROM the current consts (so default weights
   reproduce current behavior exactly).
3. `predict(&features, &weights) -> f64`: the dot-product "tunable eval",
   replacing the real evaluate()'s hardcoded consts with the weight struct.
4. CRITICAL SAFETY NET: a self-consistency test that runs BOTH evaluate()
   (real, fast, const-based) and predict(extract(pos), &default_weights)
   on many random Pet Dragon positions (Position::generate_with_seed) and
   asserts EXACT agreement. This must pass before any tuning work is
   trusted — it's the only way to verify the feature extraction faithfully
   mirrors 2000+ lines of existing eval logic without being able to
   compile-check by hand at this scale.
5. Only after step 4 passes: gradient descent optimizer (sigmoid-scaled
   error against game results, same K/lambda-style scaling reasoning as
   D14's NNUE target, batched across the 147,867-sample database from
   14.2) + texel_tune.yml (GitHub Actions per D19).
6. Tuned weights get formatted back into the same s(mg,eg)/array literal
   syntax already used in eval/*.rs and applied as a delta.

**Why staged this way**: HCE bugs are much harder to catch than NNUE bugs
were — there's no val_loss curve, no eval_diag-style sanity check readily
available until the tuner exists, and a subtly wrong feature extraction
would silently corrupt every tuning result with no error at any point.
The self-consistency test (step 4) is the load-bearing safety mechanism
and must be built and verified before writing a single line of the actual
optimizer.

**Rejected**: Writing the full ~800+ line feature extractor in the same
session as the audit — rejected due to session length/fatigue risk on a
piece of code with no compiler available to catch mistakes and no
intermediate verification possible until the whole thing (all 6 modules)
is complete enough to run the self-consistency test.

## D36 — Pre/Post-Texel-Tuning Elo: Pinned-Ref UCI Match, Not Runtime Weights (2026-07-10)

**Decision**: To get a genuine pre/post-Texel-tuning Elo number (gap left
open by 17.5/17.7 — `match_runner.rs`/17.2 can only A/B NNUE blend weight
within one compiled binary, not two different HCE weight sets), build a
second, independent match harness (`src/bin/uci_match_runner.rs`) that
spawns TWO SEPARATE `pet_dragon` binaries as OS child processes — one built
from a git ref before the Session 55 tuning commit, one from `main` — and
plays them against each other over real UCI (stdin/stdout), the same way
any two engines are matched by a GUI or tournament manager. A new workflow
(`uci_match_runner.yml`) checks out and builds both refs plus the harness
itself, then runs the match, mirroring `match_runner.yml`'s manual-dispatch
pattern (D19/D20).

**Rejected alternative**: Refactor `eval/*.rs` (material, tables, mobility,
pawns, king_safety, open_lines) to read weights from a runtime struct
instead of compile-time consts, then A/B a pre-tuning and post-tuning
profile within one binary like the NNUE blend already does. Rejected for
two reasons: (1) `evaluate()` runs at every leaf node — it's the hottest
path in the engine, and the added indirection risks a real NPS regression
plus risks breaking D35's exact-agreement self-consistency guarantee, for
what is a one-time measurement, not a permanent feature; (2) the original
pre-tuning Ethereal-derived literal values are already gone from the
working tree (Session 55 overwrote them in place) — recovering them would
require pulling an old git ref anyway, so the "runtime weights" approach
doesn't even avoid needing a pinned ref. The pinned-ref UCI approach gets
the same answer with zero changes to any shipped eval code.

**Design notes**:
- Harness tracks game state locally (movegen/position, same generator as
  `match_runner.rs`/`selfplay.rs`) purely for legality and termination
  checks; each engine process is treated as a black box that only sees
  `position`/`go`/`bestmove` over UCI — exactly what a real GUI sees.
- `parse_uci_move` is duplicated from `main.rs` rather than exported from
  the library — it's a thin UCI-string-to-Move lookup with a single
  caller, not worth growing the public library surface for.
- Known limitation, accepted: no per-move timeout on the child process
  read — a hung engine blocks the harness indefinitely, bounded only by
  the CI job's own overall timeout. Acceptable for a manually-triggered,
  one-off tool; would need revisiting if this were ever promoted to a
  routinely-scheduled job.
- `pre_tuning_ref` is a required workflow input with no default (Gokul
  supplies the SHA via the mobile GitHub app's commit history) rather than
  hardcoded, since Claude could not reliably confirm the exact pre-Session-55
  commit SHA this session (api.github.com anonymous rate limit hit).


## D37 — Real UCI Pondering: Atomic Deadline Override, Not start_time Reset (2026-07-11)

**Decision**: Implement full UCI pondering (`go ... ponder` + `ponderhit`),
closing a real protocol-completeness gap found by inspection (not by a
failing test): `allocate_time()` already special-cased `tc.ponder` to
search near-infinitely (correct half of pondering, pre-existing), but
`ponderhit` wasn't handled anywhere in `main.rs`'s command dispatch at
all — a real GUI that ponders would get an unrequested `bestmove` during
the opponent's think time instead of the engine correctly switching to a
bounded real-time search.

Mechanism: two new `Arc<AtomicU64>` fields on `SearchInfo`
(`ponder_hit_soft_ms`, `ponder_hit_hard_ms`, default `u64::MAX` sentinel =
unset), threaded through exactly like the existing `stop_flag: Arc<AtomicBool>`
pattern (D4). `cmd_go` precomputes what the REAL (non-ponder) time
allocation would be from the same `go` line's clock values, stashes it on
`EngineState`, and records the wall-clock instant pondering began. `cmd_ponderhit`
computes elapsed-since-pondering-began and stores `elapsed + real_soft` /
`elapsed + real_hard` into the atomics — deadlines expressed relative to
the search thread's own `start_time`, not reset to zero, since pondering
time is free per the UCI spec. `is_time_up()` (the hot-path check, every
256 nodes) reads `ponder_hit_hard_ms` first and uses it in place of
`time_allocated_ms` when set — a single extra atomic load, same cost
class as the existing `stop_flag` check right above it. `iterative_deepening()`'s
depth-loop-top additionally consumes `ponder_hit_soft_ms` once (via `swap`)
to rebuild the `TimeManager` so the "should I start the next depth"/early-
stop logic also respects the real budget, not just the hard 256-node kill
switch.

**Rejected alternative**: Reset `SearchInfo::start_time` to `Instant::now()`
at ponderhit, so the existing `elapsed_ms() >= time_allocated_ms` check
needs no new override logic at all. Rejected because `start_time` is a
plain field owned by the (already-running) search thread — `cmd_ponderhit`
runs on the main UCI-loop thread and has no safe way to mutate it without
either a `Mutex` around a currently-hot-path field (D4 precedent is
specifically atomics, not locks, for anything checked per-node) or an
unsafe alias. Expressing the override as a deadline relative to the
existing, unchanged `start_time` avoids ever needing write access to
search-thread-owned state from another thread — the atomics are the only
thing main.rs touches.

**Bug caught during verification, not shipped**: the first version of the
integration test set the override atomics *before* calling
`iterative_deepening()`, which made `reset_for_search()` (correctly, for
real usage — a genuinely new search should never inherit a stale
override) wipe them before the depth loop ever read them, so the test
passed for the wrong reason (it wasn't actually exercising the concurrent
case). Caught because the "bounded" search ran to depth 23 / 191 seconds
in a manual `cargo test` run instead of stopping in milliseconds — fixed
by spawning a real background thread that sets the atomics ~30ms after
the search starts, matching how a real GUI's `ponderhit` can only arrive
after the search is already running, never before.

**Verified**: `cargo check --release` clean on the real crate; full test
suite green (335 lib + 22 bin, up from 329/17 — 12 new tests, all passing,
none touched/regenerated); manual end-to-end UCI runs via a real spawned
`pet_dragon` process — pondered 500ms then `ponderhit` correctly produced
a bounded ~2.2s real search (matching the 60s-clock-implied soft limit)
instead of running forever; a plain `go movetime 300` and a `go ponder`
followed by `stop` (ponder miss) both still behave exactly as before.


## D38 — MultiPV via Root-Move Exclusion, Not Parallel Search Trees (2026-07-11)

**Decision**: Implement UCI `MultiPV` (report N candidate lines instead of
1) using the standard alpha-beta technique: search the primary line
normally, then re-search from the root excluding the move(s) already
found, once per extra line, all at the same depth. Two new `SearchInfo`
fields make this possible: `multipv: usize` (default 1) and
`root_exclude: Vec<Move>`. The move loop in `alpha_beta.rs` skips a move
if it's in `root_exclude` — but ONLY at the root (`root_node &&
!info.root_exclude.is_empty() && ...`), which matters for a specific
reason: singular extension already uses a same-shaped `excluded: Move`
parameter deeper in the tree, gated on `!root_node`. Since singular
verification never runs at the root and MultiPV's exclusion never runs
below the root, the two mechanisms share the move loop without ever
colliding — confirmed by reading where singular extension's guard
actually sits before writing a single line of the new check.

The extra lines are 100% additive: the entire existing single-PV code
path in `iterative_deepening()` is untouched (not even reformatted), and
the new block is gated behind `info.multipv > 1`, which is false for
every existing caller (`SearchInfo::new()` defaults `multipv` to 1). The
returned `SearchResult` — the one that actually determines what gets
played — always comes from the untouched primary-line code above the new
block.

**Rejected alternative**: give each MultiPV line its own aspiration-window
state carried across depths (matching how the primary line already
works), for speed. Rejected as unnecessary complexity for what MultiPV
users already expect to be slower than single-PV — extra lines use a
plain full-window search instead (`search_multipv_slot()`), which is
simpler, shares the TT lock-free, and costs nothing when MultiPV is at
its default of 1 since the function is never called.

**A test I wrote was wrong, and fixing it taught something worth
recording**: the first version of `test_multipv_does_not_affect_primary_
move_choice` asserted the primary line's `best_move` is identical whether
MultiPV is 1 or 4. It failed — genuinely, not a flaky timing issue.
Investigating showed why: extra MultiPV lines searched at depths 1..N-1
feed the same shared TT/history/killer/correction-history tables that
depth N's primary-line search then reads, so a MultiPV=4 run has
legitimately explored more of the tree by depth N than a MultiPV=1 run
did — enough to occasionally shift move ordering and land on a different,
still fully valid, still best-scoring-at-that-depth move. This is a
known, accepted property of MultiPV in alpha-beta engines generally
(Stockfish's own documentation carries the same caveat), not a defect
introduced here. The test now asserts what's actually guaranteed and
worth testing — the primary line is always legal, in both configurations
— documented at length in the test itself so nobody has to rediscover
this the hard way.

**Also this session — Move Overhead (D38 covers both, one small feature
alongside the bigger one)**: `search/time.rs`'s `OVERHEAD_MS` was a
hardcoded constant; added `TimeControl::overhead_ms` (defaults to the same
constant via a manual `Default` impl, replacing the old `#[derive(Default)]`)
and wired it through `EngineState.move_overhead_ms` / `setoption name Move
Overhead`. Caught and fixed a real pre-existing parsing bug while touching
`cmd_setoption` for this: the old parser took `tokens[2]` as the entire
option name and `tokens[4]` as the entire value, which silently mis-parsed
any multi-word option name ("Move Overhead" itself, the very option being
added) and truncated any multi-word value (e.g. a Windows SyzygyPath with
spaces) to its first token. Rewrote to find the `"value"` token and join
everything on each side of it, backward-compatible with every existing
single-word case (verified: `test_setoption_single_word_value_still_works`).

**Verified**: `cargo check --release` clean; full suite green, 345 lib
(was 335) + 30 bin (was 22) = 375 total, 18 new; manual end-to-end UCI
runs against the real compiled binary — `setoption name MultiPV value 3`
produced correctly depth-sorted, distinct-move `multipv 1/2/3` lines with
`bestmove` matching line 1's final-depth move; default (no MultiPV set)
produced exactly one `multipv 1` line per depth, confirming zero change
to existing behavior; `Move Overhead 2000` on a `movetime 3000` search
correctly finished in ~1s (3000-2000ms budget) instead of ~3s.


## D39 — Skill Levels: Depth-Cap Tiers, Not Elo Calibration (2026-07-11)

**Decision (scoping only — not built this session)**: When difficulty
levels get built, they'll be depth-cap tiers (optionally + a little
move-selection noise at the low end, matching Stockfish's actual `Skill
Level` mechanism rather than just "search less"), labeled `Skill Level
0..N` or similar. Each tier's ordering/spacing will be validated
empirically using the existing `uci_match_runner.rs` harness (D36) across
many seeded positions — tier K vs tier K+1 should win convincingly and
consistently, same methodology as the Texel-tuning validation. No new
measurement infrastructure needed.

**Explicitly rejected: reusing standard-chess Elo tables (Stockfish's
`UCI_Elo`/`Skill Level` calibration) for Pet Dragon, even just for
openings that start from the standard array.** The reasoning took two
passes to land correctly:
1. First framing considered: use the *one* Pet Dragon opening that visually
   matches the standard chess starting position, apply standard-chess Elo
   calibration there, generalize to other openings later. Rejected — this
   still repeats D36's original single-seed-outlier lesson (one sample,
   even a resembling one, isn't representative) at a *larger* scale than
   before.
2. Real reason it doesn't work at all, even for that one opening:
   **Pet Dragon's custom pawn rules apply from move one**, so a visually
   standard starting array doesn't mean move one onward plays like real
   chess — legal pawn moves, promotion timing, and en passant can already
   diverge. Stockfish's Elo calibration tables were built from millions of
   games under *real* chess rules against known-strength opponents; that
   calibration measures "how does this engine play chess," not "how does
   it handle these 32 starting squares." Once the rules diverge, there's
   no way to know how much of that calibration still applies — could be
   negligible or could be large, and there's no data either way to check.
   The resemblance between the two starting positions is cosmetic, not
   structural, so no part of an external Elo table transfers.

**Why depth-cap tiers don't have this problem**: "less search depth is
weaker" is true by construction, not by borrowed calibration — it doesn't
need external data to justify, only internal ordering to verify, which
D36's harness already does well. A plain `Level 1-10` label makes no
claim about human-comparable strength, so there's nothing dishonest about
shipping it without a rating pool — unlike `UCI_Elo`, which explicitly
promises "you will play at approximately N Elo," a promise this project
still can't back for the reasons already established in the "is it
release-ready" discussion (no external rating pool exists for this
variant at all, full stop, regardless of the pawn-rules issue above).

**Status**: scoped, not implemented. Queued for a future session — see
ROADMAP Phase 20.

---

## D40 — GUI Exposes Named Skill-Level Presets, Not a Raw 0-20 Slider (2026-07-12)

**Context**: Phase 20's engine-side Skill Level mechanism (D39, depth-cap
+ time-fraction + move-selection-noise) is fully validated end-to-end —
200-game match-runner results confirm all adjacent tiers are correctly
and monotonically ordered (0v5 -619 Elo, 5v10 -117 Elo, 10v15 -65 Elo,
15v20 -80 Elo). Gokul asked whether a "Skill Level 15 loses to Skill
Level 20 sometimes" result was acceptable, prompted by the 15-vs-20 gap
being noticeably smaller than 0-vs-5.

**Decision**: don't change the underlying 0-20 mechanism to chase bigger
separation at the top end — it's correctly validated, and forcing wider
gaps between adjacent numeric levels would fight the real, expected shape
of engine-strength-vs-depth (diminishing returns at greater depth, not a
bug — see ROADMAP Phase 20.4/20.5 for the full investigation). Keeping the
full 0-20 range also matches the standard UCI `Skill Level` spin
convention other tools/GUIs already expect, useful if Pet Dragon is ever
driven by something other than Gokul's own GUI.

Instead: the GUI-facing difficulty control should be a small set of NAMED
presets, not a raw numeric slider exposing all 21 levels. No player can
feel the difference between adjacent numbers like 14 vs 15 — that's not a
UX problem specific to Pet Dragon, it's true of every engine's Skill
Level implementation, including Stockfish's. Mapped directly onto the five
points already validated with real match data, no new engine-side testing
required:

| GUI label  | skill_level | Basis |
|------------|-------------|-------|
| Beginner   | 0           | 97% loss rate to next tier — genuinely weak |
| Easy       | 5           | Clear step up (66% for 10 vs 5) |
| Medium     | 10          | Clear step up (59% for 15 vs 10) |
| Hard       | 15          | Noticeable step up (62% for 20 vs 15) |
| Master     | 20          | Full strength — no cap, no noise |

**Why this resolves the "felt difference" concern without more engine
work**: framing it as five distinct named difficulties, rather than two
adjacent numbers (15 and 20) that imply they should feel similarly close,
sidesteps the issue entirely — a player choosing "Hard" vs "Master" reads
those as different difficulties on their face, which is an honest framing
of a real (if moderate, ~62%) win-rate gap, not an inflated one.

**If bigger separation at the top is wanted later**: the lever is preset
SPACING (e.g. skip from Hard=15 straight to Master=20 with no
intermediate, or define a custom extra tier), not re-tuning the
depth-cap/time-fraction/noise formulas in `skill.rs` — those are working
correctly and shouldn't be treated as the thing to fix for a UX-level
concern.

**Status**: decided, GUI implementation is Gokul's own — no Pet Dragon
engine repo changes from this decision. `src/lib.rs`'s WASM
`search_from_fen(fen, movetime_ms, skill_level)` already accepts a plain
`u8` skill_level per call (added this session), so the GUI can pass
whichever preset's value the player picked on every search — no engine-
side wiring left to do.

## D41 — Re-Park NNUE After hidden_size=128 Test Confirms D34's Ceiling (2026-07-13)

**Decision**: Tested D34's stated revisit condition (`hidden_size=128+`,
deliberately bigger architecture) on the exact same 286,659-row dataset
as the parked `hidden_size=32` baseline, isolating hidden_size as the
only changed variable. Result: val_loss=0.51655 (best epoch 3/10) vs the
parked baseline's val_loss=0.51636 (best epoch 4/10) — 4x the hidden-layer
capacity produced a marginally *worse* val_loss and overfit one epoch
earlier (val_loss rose to 0.53169 by epoch 10, a +0.0151 climb from best).
`NNUEWeight` stays 0% (unchanged since D25). No match_runner Elo sweep
run against this network — the val_loss regression already rules it out
as an improvement, and burning an Actions run to reconfirm a negative
result contradicts D19/D20's efficiency stance. NNUE work is re-parked;
Phase 16/17 stay closed as an optional, not-currently-worthwhile
enhancement on top of an otherwise-complete, Elo-validated engine
(Phases 0-20 all done).

**Why**: This is the 5th independent lever (after D34's data retrain,
clamp, and clamp+regularization) landing in the same ~70-72%
average-opponent-score / ~0.516 val_loss neighborhood, each time via a
genuinely different mechanism. Four different fixes plus one architecture
change all converging on the same ceiling is much stronger evidence of a
structural limit than of an undiscovered hyperparameter or under-sized
network. More parameters on the same data classically overfits faster
rather than learning more — which is exactly what happened (earlier best
epoch, steeper post-peak divergence) — not a coincidence.

**Rejected**: Immediately pursuing D34's other stated lever — a genuinely
bigger self-play dataset (500K-1M+ rows via a dedicated Kaggle job) —
rejected for now, not because it's wrong in principle, but because NNUE
is explicitly an optional strength enhancement (Phase 16's own scoping)
on an engine that's already complete and Elo-validated through Phase 20.
Spending a new, expensive, separately-scoped data-generation effort to
chase an optional feature isn't currently the best use of session budget.
Also rejected: running the match_runner sweep anyway for completeness —
rejected because the val_loss comparison is already conclusive and
directly comparable (identical data, identical everything else but
hidden_size), so the sweep would only spend Actions minutes to reconfirm
a known answer.

**Revisit**: only if/when a genuinely larger, dedicated self-play data
effort (not incremental) is undertaken separately — at that point retest
hidden_size=32 AND hidden_size=128 on the new data to see whether more
data alone breaks the ceiling before re-attempting bigger architectures.
Until then, NNUE stays parked and out of scope for regular sessions.

## D42 — UCI Completeness: Ponder, Contempt, Real Eval Bar; UCI_Elo Stays Rejected (2026-07-14)

**Decision**: Implemented three of four items requested this session:
Ponder option declaration, Contempt (draw-value tuning), and a real
engine-score eval bar for the browser. The fourth — `UCI_Elo` — was
explicitly declined by Gokul after being flagged as a direct conflict
with D39 (no calibrated human-comparable Elo number this engine can
honestly back). D39 stands unmodified; `Skill Level` (0-20, depth-cap
tiers) remains the only difficulty lever.

**Ponder**: added `option name Ponder type check default true` to the
UCI option list. No engine-side state — `setoption name Ponder` is
accepted (`"ponder" => {}` no-op arm) but genuinely does nothing, since
whether pondering happens is entirely determined by whether the GUI
sends `go ... ponder` at all (the underlying pending-allocation/
ponderhit logic already existed and worked; it just wasn't advertised,
so some GUIs would never invoke it).

**Contempt**: `option name Contempt type spin default 0 min -100 max
100`. Design choice worth recording: rather than adding a `root_side:
Color` field threaded through `SearchInfo`/`alpha_beta`, contempt's
sign is derived purely from `ply % 2` — `alpha_beta`'s `ply` parameter
already increments by exactly 1 per node starting at 0 at the root, so
`ply % 2 == 0` means the current node's side-to-move IS the root side,
with no new state to add, reset, or ever get stale. `draw_score(ply,
contempt)` (search/mod.rs) is a pure function: returns `DRAW_SCORE`
unchanged when `contempt == 0` (byte-identical to every pre-Contempt
test), `DRAW_SCORE - contempt` at root-side nodes, `DRAW_SCORE +
contempt` at opponent nodes. Applied at all 4 draw-detection sites in
`alpha_beta.rs` (repetition, 50-move, insufficient material, and
stalemate — the 4th one wasn't in the original 3-site read of the code;
included it too since a draw is a draw regardless of which mechanism
produced it). Threaded into `SearchInfo.contempt` (persistent across
moves, same pattern as `skill_level`) via `cmd_go`, applied to both
`h_info` (helper threads) and `main_info`.

**Real eval bar**: added `search_from_fen_with_eval(fen, movetime_ms,
skill_level) -> String` as a NEW WASM export, deliberately NOT modifying
the existing `search_from_fen`'s signature or return format. Returns
`"<uci_move> <eval>"` where eval is a plain signed centipawn integer or
`"mateN"`/`"mate-N"`, always from **White's perspective** (not the
UCI/negamax side-to-move convention) — a GUI bar needs a stable
reference frame that doesn't flip depending on whose turn it is.
`web/index.html`'s Worker now calls this instead of the old bare
`search_from_fen`; `window.petDragonSearch()` resolves `{bestmove,
evalCp, isMate, mateIn}` instead of a bare move string; a new
`updateEvalBarFromSearch()` renders the real score, called right after
each engine move completes. The old `updateEvalBar()` heuristic
(material + mobility) stays as-is and un-renamed — it's still the right
tool for instant feedback in the gap before the engine's own search has
run (e.g. immediately after a human move), `updateEvalBarFromSearch()`
just overrides it once a real search completes.

**Why the new-function-not-modified-signature approach**: `search_from_fen`
might have callers beyond `web/index.html` that this session didn't
audit (the repo's checked-in root `index.html` is stale — see
Housekeeping below — and there could be others). Changing its return
format would be a silent breaking change for anything still expecting a
bare move string. A new function costs a few duplicated lines
(`Position::from_fen`, `TimeControl` setup) but has zero blast radius
on anything already working.

**Verification gap, stated plainly**: the `wasm` feature could not be
compiled locally this session — `wasm-bindgen v0.2.126` (the version
pinned in the committed `Cargo.lock`) requires rustc 1.77+, and this
sandbox's apt-installed rustc is 1.75.0 (same class of toolchain-age
gap noted in prior sessions, just hitting a different dependency this
time). `search_from_fen_with_eval` was manually verified field-by-field
against `Position`/`SearchResult`'s actual definitions (`side_to_move:
Color`, `Color: PartialEq`, `score`/`is_mate`/`mate_in` on
`SearchResult`) and mirrors the already-working `search_from_fen`'s
structure exactly apart from the new eval-formatting tail, but this is
the one piece in this session that was not mechanically compiled by
Claude before delivery. The Rust lib/bins (everything except the
`wasm` feature) and the JS (`node --check` on all 3 script blocks,
including the Worker source extracted and checked separately as its
own ES module) were both verified. **Next session should confirm the
actual `wasm-pack`/GitHub Actions build succeeds** before treating this
as done.

**Housekeeping note (not actioned this session)**: discovered
`web/index.html` (7199 lines, current, has the 3-arg
`search_from_fen(fen, ms, skill)` call) is the real source, while the
repo's checked-in root `index.html` (2003 lines, what GitHub Pages
actually serves) is stale — still has the old 2-arg call from before
Skill Level (D39) existed, meaning the deployed page has been out of
sync with the actual engine's WASM API for at least one full phase.
Worth a dedicated session to figure out why the deploy workflow isn't
regenerating root `index.html` from `web/index.html` and fix the
pipeline, separate from today's feature work.

**Rejected**: `UCI_Elo` (see above — D39 stands). Also rejected:
touching the existing `search_from_fen`'s signature directly instead of
adding a new function (blast-radius reasoning above).

## D43 — UCI_LimitStrength/UCI_Elo Built, Overriding D39's Rejection (2026-07-14)

**Decision**: D42 (same day) closed with `UCI_Elo` explicitly declined
per D39. Gokul reversed that call later the same session and asked for
it anyway, with `UCI_LimitStrength` mapped onto Skill Level and `UCI_Elo`
mapped onto self-assumed Elo values. Flagged the reversal plainly before
building (D39's actual objection was never "don't build the option," it
was "don't attach a number this engine can't honestly back" — assigning
Elo values without real calibration data runs into exactly that). Gokul
chose to proceed anyway. D39 itself is NOT rewritten or deleted — its
depth-cap-tiers reasoning for Skill Level stands untouched; this decision
only overrides its specific rejection of attaching an Elo number.

**The numbers, and exactly what they are**: two layers, kept visibly
separate throughout implementation and its own doc comments.
1. **Real**: Session 68's actual 200-games/pair `uci_match_runner`
   validation — 0v5 -619.4 Elo, 5v10 -117.2, 10v15 -65.0, 15v20 -81.35
   (avg of two consistent runs). These are the only measured relative
   tier gaps that exist anywhere in this project.
2. **Assumed**: Gokul chose Skill 0 = 1200 and Skill 20 = 2600 as
   absolute anchors (no external rating pool exists to derive these
   from — same limitation D39 already documented). The four real gaps
   above were rescaled by one constant factor so they sum to exactly
   1400 (2600-1200) instead of their original unscaled sum, then
   chained from the two anchors to get 5 grounded points (levels 0, 5,
   10, 15, 20). Levels 1-4, 6-9, 11-14, 16-19 were linearly interpolated
   within each rescaled band — these 16 entries have ZERO game data
   behind them, only the assumption that Elo varies smoothly between
   the two nearest tested points.

Landed in `search/skill.rs`: `ELO_TABLE: [i32; 21]` (the full computed
table) and `elo_to_skill_level(target_elo: i32) -> u8` (nearest-match
against the table, clamped to the table's own min/max first, ties
resolve to the LOWER level — deliberately conservative, since
`UCI_LimitStrength` exists to make the engine weaker on request, and a
tie-break that rounds up would silently give a stronger engine than
what was asked for). `main.rs` gained `UCI_LimitStrength` (check,
default false) and `UCI_Elo` (spin, default = table max = 2600, range
1200-2600) options, `EngineState.limit_strength`/`.elo` fields, and
`cmd_go` now computes `skill_level` from `elo_to_skill_level(state.elo)`
instead of `state.skill_level` directly whenever `limit_strength` is
true — an override relationship, not an additional independent lever
(mirrors how Stockfish's own `UCI_LimitStrength` relates to its `Skill
Level`).

**Why `UCI_Elo` defaults to the table's max (2600), not some lower
"typical" value**: `UCI_LimitStrength` defaults to false, so `elo`'s
default is inert until a GUI explicitly enables limiting — but if a GUI
enabled `UCI_LimitStrength` without ever touching `UCI_Elo` (plausible:
some GUIs set flags before values, or a user toggles a checkbox without
realizing a paired spin box exists), defaulting `elo` low would silently
weaken the engine with no explicit request to do so. Defaulting to max
means that specific failure mode can only ever produce full strength,
never an unrequested handicap.

**Why linear interpolation for the untested 16 levels, not something
fancier**: the real 4-gap data already shows a non-uniform, diminishing-
returns shape (massive at 0-5, compressed at 15-20) — that shape is
real and preserved by rescaling all four gaps by the same factor.
Within each individual band, `skill_depth_cap()` itself increments by
exactly 1 ply per level (confirmed in skill.rs — no unevenness to
model), so there's no principled basis for anything more elaborate than
linear interpolation between the two nearest tested anchors without
inventing structure this project has no data to support.

**Test risk flagged**: `elo_to_skill_level()`'s own tests (exact-anchor
round-trips for all 21 levels, monotonicity, clamping both directions,
tie-break direction, non-exact nearest-match) and the `main.rs`
option/setoption/cmd_go wiring tests were all written and locally
compile-checked (`cargo check --bins` clean, plus the `--cfg test`
workaround for the test-cfg code — see prior sessions' notes on why that
workaround is imperfect but the best available check in this sandbox).
Not yet confirmed via a real `cargo test` CI run as of this decision.

**Revisit**: if real match-tested data ever exists for the currently-
interpolated levels (1-4, 6-9, 11-14, 16-19), replace those specific
`ELO_TABLE` entries with measured values rather than re-deriving the
whole table — the 5 anchor-derived entries (0, 5, 10, 15, 20) would stay
as-is unless Gokul chooses different absolute anchors again.

## D44 — Fix Shallow cmd_go Wiring Tests; Fixes a Real Elo/Time-Fraction Bug (2026-07-14)

**Decision**: Gokul flagged that several `cmd_go` wiring tests (added in
D42/D43, plus two pre-existing ones they copied their shape from —
`test_cmd_go_applies_skill_level_to_search` and
`test_cmd_go_applies_move_overhead_to_time_control`) only verified that
`EngineState` fields weren't unexpectedly mutated, not that the search
thread actually received the configured value. True: `cmd_go` never
writes `skill_level`/`contempt`/etc. back to `EngineState` either way
(it only reads them once into local variables before spawning the
search thread), so those assertions passed unconditionally regardless
of whether the thread closure used the right value — a copy-paste bug
like `main_info.contempt = 0` instead of `= contempt` would have sailed
through every one of them.

**Fix, in two parts:**

1. **`wait_for_search()` now returns `Option<SearchInfo>`** instead of
   `()`. It already joined the search thread and received its actual
   `SearchInfo` back (`returned_info`) — it just discarded everything
   except `history`/`countermoves`/`correction_history` after copying
   those into `self.info`. Now it returns the full joined struct too.
   Backward compatible: `Option<T>` isn't `#[must_use]`, so every
   existing call site (`state.wait_for_search();` as a bare statement)
   keeps compiling unchanged with zero warnings — confirmed via a clean
   `cargo check --bins`. Required adding `#[derive(Clone)]` to
   `CorrectionHistory` (`search/pruning.rs`) — needed so
   `wait_for_search()` can copy `correction_history` into `self.info`
   while also returning the whole `SearchInfo` it came from, without a
   partial-move conflict. Safe: it's just `Vec<[i32;2]>` + `usize`.

2. **Extracted `effective_skill_level(state: &EngineState) -> u8`
   and `build_time_control(state: &EngineState, line: &str, skill_level:
   u8) -> TimeControl`** as pure, standalone functions — the
   UCI_LimitStrength/UCI_Elo override logic and the `go`-line-to-
   `TimeControl` construction, respectively, previously both inlined
   directly in `cmd_go`. Both are directly unit-testable without
   spawning or joining any thread.

**Real bug this surfaced**: `build_time_control`'s extraction required
looking at exactly when `tc.skill_time_fraction_pct` got computed
relative to the Elo-override logic — and they were in the wrong order.
D43's `cmd_go` computed `tc.skill_time_fraction_pct` from
`state.skill_level` directly, then only LATER (right before the thread
spawn) computed the Elo-overridden `skill_level` used for the depth cap.
So with `UCI_LimitStrength` active, the depth cap correctly used the
Elo-derived level, but the TIME BUDGET still used the raw, possibly
irrelevant `Skill Level` setting. Concretely: a `UCI_Elo` request
mapping to a low Skill Level got the correct shallow depth cap paired
with a full, unreduced time budget — precisely the "searches shallow
then sits idle for the rest of its allocation, looking broken rather
than weak" failure mode Session 65 built the depth+time pairing to
prevent in the first place (see D39/skill.rs's own header comment).
Fixed by resolving `effective_skill_level(state)` once, up front in
`cmd_go`, and threading that single value into both
`build_time_control` and the later per-thread `SearchInfo` construction
— they can no longer diverge, by construction, not by convention.

**Tests rewritten** (all now assert on real, thread-verified or
pure-function values instead of state-field non-mutation):
- `test_cmd_go_applies_skill_level_to_search` — now checks
  `wait_for_search()`'s returned `SearchInfo.skill_level` directly.
- `test_cmd_go_applies_contempt_to_search` — same pattern for `contempt`.
- `test_cmd_go_applies_move_overhead_to_time_control` — now calls
  `build_time_control()` directly and checks `overhead_ms`/`movetime`.
- Split the old `test_limit_strength_enabled_overrides_skill_level`
  into `test_effective_skill_level_ignores_elo_when_limit_strength_off`
  and `test_effective_skill_level_uses_elo_when_limit_strength_on`
  (fast, pure, exact) plus one genuine end-to-end integration test,
  `test_cmd_go_search_reflects_elo_override_not_raw_skill_level`, that
  deliberately sets `skill_level` and the Elo-derived level to
  *disagree* (20 vs 3) and confirms the joined `SearchInfo` reflects the
  Elo-derived one — the specific case a state-field check could never
  distinguish.
- New regression test for the bug above,
  `test_build_time_control_uses_elo_derived_skill_level_not_raw`.

**Why extraction over a test-only escape hatch**: considered adding a
`#[cfg(test)]`-gated field to stash the built `SearchInfo` somewhere
`EngineState` exposes, instead of widening `wait_for_search`'s return
type. Rejected — that pattern only helps tests, adds permanent
production-code surface area (a field that exists solely to be read
back out in a different build configuration) for zero production
benefit, and doesn't fix the `TimeControl`/time-fraction ordering bug
at all (that bug lived in inline logic with no return value to widen).
Extracting the two pure functions fixes both the testability gap AND
the real bug simultaneously, and leaves `cmd_go` itself easier to read
as a bonus — the effective-skill-level resolution is now a single,
named, one-line step instead of duplicated inline logic in two places.

## D45 — Repetition Detection Redesigned to Match Stockfish's Algorithm (2026-07-14)

**Decision**: Replaced `is_repetition()`'s unbounded "scan all of
game_history for any 2nd occurrence" with the actual algorithm
Stockfish uses (`Position::set_state()`/`is_draw(ply)` in
`position.cpp`), verified against the real source rather than worked
from memory. `game_history` changed from `Vec<u64>` to `Vec<(u64,
i32)>` — each entry now caches a "repetition" distance at push time,
mirroring Stockfish's `StateInfo::repetition` field exactly.

**The algorithm**: `push_game_history()` walks backward through history
in steps of 2 plies (repetition requires the same side to move),
starting at `i=4` and bounded by `halfmove_clock` (no point checking
further back than the last pawn move/capture — that position is
provably not a repeat). On the first match found:
- caches `+i` if the matched position's OWN cached value was `0` (a
  first repeat), or
- caches `-i` if the matched position's own cached value was already
  nonzero (this is now the second link in a genuine repetition chain —
  functionally a 3-fold, encoded via sign rather than a separate flag).

`is_repetition(ply)` is then an O(1) lookup: `repetition != 0 &&
(repetition as i64) < (ply as i64)`. A negative (chain) value is always
`< ply` for any non-root ply, so it's always a draw regardless of where
in the search tree it's noticed — a real 3-fold on the board is a real
draw no matter what. A positive (first-repeat) value is only a draw if
`ply` exceeds it — meaning the repeated position was reached via moves
the search itself chose along the current branch, not purely inherited
from real game history that predates the search root. This is the part
the old implementation didn't have at all: it treated every repeat as
an immediate draw regardless of whether the search could do anything
about it, which could bias the search against a position it has no way
to actually avoid.

**Why `i=4`, not `2`**: a position cannot repeat after only 2 plies (one
move by each side) in legal chess — every individual move is a real
board change, so the shortest possible repetition cycle is 4 plies
(e.g. each side shuffles a piece out and back). Matches Stockfish's own
loop bounds exactly, confirmed against the real source before
implementing.

**Why no `pliesFromNull` bound (unlike Stockfish)**: checked Pet
Dragon's null-move pruning (`alpha_beta.rs`) directly — it mutates
`pos.hash`/`side_to_move` inline and never calls `push_game_history()`
at all, so a null-move-derived position can never end up in
`game_history` in the first place. Stockfish needs the extra bound
because its null-move handling does touch the same StateInfo chain;
Pet Dragon's design sidesteps the concern structurally rather than
needing an equivalent guard.

**Performance side-effect, not the primary motivation but real**: the
old scan was unbounded O(game-length) on every single draw check
(every node, every ply). The new one is O(halfmove_clock/2) amortized
at push time and O(1) at every subsequent lookup — the same asymptotic
improvement Stockfish gets from caching, for the same reason.

**`is_threefold_repetition()` deliberately left alone**: still a plain
count-of-3-or-more scan, ignoring the cached ply-relative value
entirely. This is intentional — it's used for actual game-end
adjudication (`match_runner`/`uci_match_runner`/`selfplay`/
`texel_gen`'s stopping condition), where the literal rule matters, not
what the search tree can currently see.

**`set_game_history()` — dead code, fixed anyway**: has zero callers
anywhere in the codebase (confirmed via grep before touching it), but
its signature had to change regardless since the field type changed.
Rewrote it to replay each hash through the same bounded-walk-and-cache
logic `push_game_history()` uses, rather than leaving it broken or
naively zeroing every cached value — correct behavior for whenever it
might be used, even though nothing calls it today.

**Real bug caught while writing the tests, not while writing the
algorithm itself**: every rebuilt test initially forgot to push the
*starting* position into `game_history` before making moves — matching
how `iterative_deepening()` actually pushes the search root first in
real usage, but easy to miss in an isolated test. Traced through the
exact index arithmetic by hand for each affected test (`position/
mod.rs`'s new direct unit tests, `alpha_beta.rs`'s integration test,
and `tests/make_unmake.rs`'s five rebuilt tests) and confirmed the fix
correctly reaches the intended backward distance in every case before
trusting the expected values.

**Also caught while rebuilding `alpha_beta.rs`'s integration test**:
its original bare-K-vs-K test position would have triggered
`is_insufficient_material()` before repetition detection ever got
meaningfully exercised, since that test runs a real `alpha_beta`
search (unlike `position/mod.rs`'s direct algorithm unit tests, which
aren't affected by this). Fixed by giving each side a pawn positioned
so it doesn't block the king-shuffle squares, so the test actually
exercises what it claims to.

**Verification gap, stated plainly**: `tests/make_unmake.rs` is a
separate integration test crate that could NOT be locally compiled
this session — same toolchain wall as every previous session's test
work (a dev-dependency needs `edition2024`, this sandbox's rustc is
1.75). Unlike the lib and the `pet_dragon` binary (both verified via
the `--cfg test` workaround), this file has zero local verification
beyond careful manual review — every index computation in its 5
rebuilt tests was traced by hand rather than compiled. `cargo test`
should specifically confirm this file's tests pass, not just the
aggregate count, before treating this as done.

**Rejected**: implementing Stockfish's additional "cuckoo table" O(1)
upcoming-repetition detection (used for search extensions near likely-
draw lines). That's a genuinely separate, more advanced feature — fast
detection of a repetition reachable via a single further reversible
move, used to bias search depth/extensions — not part of what was
asked ("the 3-fold avoidance rule/logic"), and not implemented here.

## D46 — build.yml Split: Versioned Tag Releases Separate from Rolling `latest` (2026-07-16)

**Decision**: `build.yml` now triggers on `v*.*.*` tag pushes in addition
to `main` branch pushes. The `release` job publishes to `tag_name:
github.ref_name` (e.g. `v3.3.3`) with `make_latest: true` when triggered
by a real version tag, and continues publishing to the existing rolling
`tag_name: latest` with `make_latest: false` for ordinary `main` commits.
Also added a `build-wasm` job to the same workflow, producing
`pet_dragon_bg.wasm`, `pet_dragon.js`, and `pet_dragon_standalone.js`
(base64-embeds the wasm binary, still needs `pet_dragon.js` alongside
it — explored full single-file concatenation of the wasm-bindgen glue
itself, rejected as too fragile against wasm-bindgen version changes for
the marginal benefit over the two-file version). First real tag:
`v3.3.3`, cutting Gokul's requested world-release version.

**Why**: The pipeline previously only ever published one release, always
under the literal tag name `latest`, on every push to `main` — there was
no mechanism to produce a real semantic-versioned release at all. Pushing
a version tag through GitHub's Release UI would have created an empty
tag with no attached binaries, since the old `on:` block didn't listen
for tag refs. Now: a tagged release captures a specific, citable, frozen
version (what "world release" actually needs); the rolling `latest`
release keeps working exactly as before for anyone tracking `main`
directly, and — because it publishes with `make_latest: false` once a
real tag exists — no longer steals GitHub's "Latest release" badge back
from the most recent intentional version tag on the next ordinary commit.

**Design notes**:
- `make_latest` is driven by `startsWith(github.ref, 'refs/tags/v')`, so
  this logic is generic and will work unchanged for `v3.3.4`, `v4.0.0`,
  etc. — nothing version-specific is hardcoded into the workflow itself.
- Gokul creates the tag via GitHub's mobile Releases UI ("Draft a new
  release" → tag field → target `main` → Publish) rather than any git
  CLI command — consistent with the mobile-only constraint. Title/body
  typed at that step are irrelevant; `softprops/action-gh-release`
  overwrites both fields once the workflow's `release` job runs against
  that same tag name, so the workflow's generated body (rules summary,
  download table, GPL attribution) is always the source of truth for the
  actual published text.
- **Sequencing requirement, confirmed the hard way**: this `build.yml`
  must be committed to `main` *before* a version tag is created — the
  workflow version active on the tag-push event is whatever's on `main`
  at the moment the tag is pushed, not a snapshot from later. This
  wasn't just a theoretical caveat: the first real attempt, `v3.0.0`,
  hit exactly this — the tag was created about 19 minutes before this
  fix actually landed on `main` (confirmed via GitHub API timestamps and
  Actions history showing zero runs against any tag ref), so the old
  workflow — which had no `tags:` trigger at all — silently never ran.
  The release sat with 2 manually-attached assets and GitHub's default
  auto-generated changelog text, easy to mistake for a partial success
  rather than a pipeline that never engaged. Diagnosed by cross-checking
  release `created_at`/`updated_at` timestamps against the Actions run
  list rather than assuming. Fixed by deleting `v3.0.0` and retagging as
  `v3.3.3` once `build.yml` was confirmed live on `main` (proven by the
  rolling `latest` release already showing all 7 assets from an
  intervening ordinary push) — that retag worked correctly end-to-end.

**Rejected**: A separate one-off manual release process (build locally,
upload by hand) — not viable, Gokul has no desktop/terminal. Also
rejected: making the rolling `latest` release disappear once a version
tag exists — kept it running unconditionally since it's useful for
anyone wanting to track `main` between tagged versions, and costs
nothing to keep alongside proper tags. Also rejected: true single-file
wasm bundling (concatenating wasm-bindgen's generated glue into the
standalone file, Stockfish-style) — technically demonstrated as
feasible, but the two-file version is the officially-documented
wasm-bindgen pattern (pass bytes to `init()`) versus text-munging
generated code that could silently break on a wasm-bindgen upgrade;
not worth the fragility for saving one file.

## D47 — Post-Release Improvements Tracked as ROADMAP Phase 23, Numbered in Execution Order (2026-07-17)

**Decision**: The 5-item post-world-release improvement list (thread-
differentiated Lazy SMP, NNUE data scale-up, NNUE architecture upgrade,
variant-specific opening statistics, SPRT-style testing gate) is tracked
as `ROADMAP.md` Phase 23, checkbox-style like every other phase — not
left as standalone report files. Item numbers (23.1-23.5) are the
recommended *execution* order, not a topic grouping: 23.1 (testing
gate) → 23.2 (thread-differentiated SMP) → 23.3 (NNUE data scale-up) →
23.4 (opening statistics) → 23.5 (NNUE architecture upgrade), with an
explicit warning at the top of the phase against skipping ahead.

**Why**: Two standalone report files
(`pet_dragon_competitive_analysis.md`, superseded by
`pet_dragon_improvement_roadmap.md`) were generated this session as
conversational deliverables — useful for a one-time read, but not
tracked anywhere the project's existing docs-are-the-memory discipline
would catch them again next session. Folding the corrected list into
`ROADMAP.md` means it gets the same treatment as every other piece of
planned work: read every session (Tier 1), checked off as completed,
carried forward if not. Numbering by execution order rather than topic
means a future session (or Gokul, mobile-only, scanning quickly) reads
top-to-bottom correctly without needing to separately consult a
"recommended sequencing" footnote that could drift out of sync with the
list above it.

**Context — this list exists because of a real mistake worth recording
plainly**: an earlier draft of this same analysis (same session)
incorrectly claimed continuation history, correction history, IIR,
razoring, singular extensions, ProbCut, and best-move-stability time
management were all missing. All seven were already implemented as of
Phase 13 — the draft was built from a stale `ENGINE_ARCHITECTURE.md`
(still describing Phase 7/8 scaffolding) plus general chess-programming
knowledge, without cross-checking actual source. A second, smaller
mistake happened during the correction itself: an initial grep for
`razor` (lowercase) missed the actual `Razoring` comment (capital R) in
source, nearly reintroducing the same class of error while fixing it.
Both `ENGINE_ARCHITECTURE.md` (full rewrite, verified line-by-line
against `src/`) and this improvement list were corrected in the same
session before anything wrong was committed to `ROADMAP.md` — but the
lesson generalizes: **cross-check any "what's missing" claim against
actual current source before writing it down, especially when a
project doc that's supposed to describe that source is more than a
session or two old.** `ENGINE_ARCHITECTURE.md` itself had drifted for
many sessions without being caught, since Tier 2 docs are only read
once per fresh session and nothing in the working process previously
required reconciling it against `src/` directly.

**Rejected**: Leaving the improvement list as standalone report files
only, on the theory that "the repo is the memory" already covers
anything worth keeping — report files delivered mid-conversation aren't
docs/ files and aren't part of the Tier 1/Tier 2 reading discipline, so
they'd have silently stopped being read the moment this conversation
ended.


## D48 — Regression Gate: exit-code threshold on uci_match_runner, gated against live `main`, not a pinned baseline file (2026-07-17)

**Decision**: Implement 23.1 (lightweight SPRT-style regression gate) as
a minimal extension of the existing `uci_match_runner.rs` (D36) rather
than new infrastructure. Added one optional 11th CLI arg,
`min_score_pct`: when supplied, the harness exits `1` if Engine A's
score against Engine B falls below that percentage, `0` otherwise; when
omitted (every existing manual `uci_match_runner.yml` invocation), the
harness always exits `0`, exactly as before — the gate mode is strictly
additive. A new `regression-gate` job in `build.yml` runs this
automatically on every `pull_request` (unlike `uci_match_runner.yml`,
which stays manual-dispatch for precision Elo/Skill-Level runs): it
builds the PR head as "candidate" (Engine A) and the current tip of
`main` as "baseline" (Engine B), plays 20 games at 50ms/move, and fails
if candidate scores below 35%.

**Why 35%, 20 games, 50ms/move**: This is explicitly a coarse smoke
check, not a precision measurement — the roadmap item itself is titled
"lightweight". At n=20 games, sampling noise alone is large enough that
a strong regression (tens of Elo) can still occasionally land near 50%;
a tight threshold like 45% would produce frequent false-positive gate
failures on completely healthy PRs. 35% (~-108 Elo via the harness's
own logistic conversion) is chosen to only catch clearly broken changes
(a real bug, not a small tuning-noise fluctuation) while staying cheap
enough to run on every PR — 20 games × up to 300 plies × 50ms worst-case
is minutes, not the tens-of-minutes a precision run (100ms/move, 100+
games) would cost. Precision Elo measurement stays on
`uci_match_runner.yml`'s manual trigger, unchanged by this decision.

**Why baseline = live `main`, not a pinned SHA/file**: Considered
storing a baseline git SHA in a repo file (e.g.
`.github/regression_baseline.txt`), updated by Gokul whenever a new
baseline should be promoted. Rejected in favor of always checking out
`ref: main` fresh for the baseline side: it needs no manual upkeep (a
pinned-file baseline would silently go stale the moment nobody
remembers to bump it, and Gokul is mobile-only — editing a SHA in a
text file by hand is exactly the kind of fiddly manual step the rest of
this workflow is built to avoid), and "does this PR make the engine
weaker than what's already on `main`" is the actually-useful regression
question, which a live comparison answers directly. Trade-off accepted:
the baseline binary gets rebuilt on every PR run rather than cached
across runs (mitigated by `Swatinem/rust-cache` on the baseline
workspace same as everywhere else in `build.yml`).

**Design notes**:
- `regression_gate_passes(score_a_pct, min_score_pct)` pulled out as a
  pure function (same pattern as `elo_diff_from_score`,
  `split_uci_options`) so the pass/fail boundary is unit-tested without
  spawning engine processes.
- Engine A is always the candidate and Engine B always the baseline in
  gate mode specifically so a failure message ("A scored 30% against
  B") reads directly as "the candidate regressed" without needing to
  remember an arbitrary A/B assignment.
- **Not yet a hard merge block.** The `regression-gate` job runs and
  reports on every PR starting this session, but GitHub only treats a
  job as merge-blocking once it's marked a *required* status check in
  branch protection settings — that's a repo-settings change, not a
  code change, and needs to happen once from Gokul's side (Settings →
  Branches → main, doable from the mobile GitHub app). Flagged in
  ROADMAP.md 23.1 rather than silently assumed done.
- Known limitation inherited from D36 unchanged: no per-move timeout on
  the child process read, so a genuinely hung candidate binary blocks
  the gate job until the CI job's own overall timeout, not a fast
  failure. Acceptable for now — same acceptance rationale as D36 — but
  worth revisiting if a hang is ever actually observed in a gate run.

**Rejected alternative**: A true statistical SPRT (sequential
probability ratio test, as used by Stockfish's fishtest) with LLR
bounds and configurable elo0/elo1 hypotheses. Rejected as over-built for
23.1's stated scope ("lightweight") and for a solo mobile-only
maintainer — a true SPRT needs a variable, potentially large number of
games per PR to reach a decision, which doesn't fit a fixed CI job
budget as cleanly as a fixed-N threshold check. The roadmap explicitly
calls this "SPRT-style", not SPRT; a fixed-N score threshold gets most
of the practical benefit (catching real regressions before merge) at a
fraction of the implementation and CI-time cost.


## D49 — Thread-Differentiated Lazy SMP: Small Fixed Offset Tables Keyed on thread_id, Not Per-Thread RNG (2026-07-17)

**Decision**: Implement 23.2 by adding a single new `SearchInfo.thread_id`
field (`usize`, default `0`), set explicitly per helper thread in
`main.rs`'s Lazy SMP spawn loop (`for tid in 1..threads`). Two existing
call sites read it to vary behavior:

1. `search::pruning::lmr_thread_base(thread_id) -> f64` replaces the
   hardcoded `0.75` constant in `alpha_beta.rs`'s LMR reduction formula.
   Cycles through a 4-entry fixed table (`[0.75, 0.45, 1.05, 0.60]`),
   indexed by `thread_id % 4`.
2. `search::ordering::thread_tie_break(thread_id, from, to) -> i32` adds
   a small (`0..=3`) deterministic offset to a quiet move's ordering
   score in `score_move()`, via a cheap multiplicative hash mix of
   `(thread_id, from, to)`.

Both functions return the untouched original value at `thread_id == 0`
(the main thread) by construction — `lmr_thread_base(0) == 0.75` exactly,
`thread_tie_break(0, _, _) == 0` always. This is the load-bearing safety
property: single-threaded search (`Threads` 1, the default) and the main
thread's own search in a multi-threaded run are provably byte-identical
to before this change. Only helper threads' internal exploration
changes, and helpers never report a result to the GUI (Phase 19's
existing MultiPV note already established this: helpers exist purely to
populate the shared TT, never to be authoritative on their own line).

**Why fixed offset tables, not per-thread RNG seeded off thread_id**: A
seeded-RNG approach (e.g. a small PRNG per thread, seeded by thread_id,
sampled once at thread spawn for that thread's LMR base and reused every
node) was considered and rejected for two reasons: (1) it's strictly more
machinery — a PRNG struct threaded through `SearchInfo` — for the exact
same practical effect as a lookup table, since the value only needs to be
distinct-and-stable per thread_id, not actually random or resampled
per-node; (2) a fixed table is trivially unit-testable by exact value
(`lmr_thread_base(1)` always equals the same number), where a seeded-RNG
approach would need either a fixed-seed determinism test anyway (no
stronger a guarantee) or would introduce nondeterminism into the test
suite for no real benefit — nothing about SMP tree diversity requires
true randomness, only decorrelation between threads, which a fixed table
already provides.

**Why `thread_tie_break` uses a hash mix instead of a second fixed
table**: Unlike LMR (one value needed per thread), the tie-break needs a
value per `(thread_id, from, to)` triple — 4096 combinations for 64
squares — so a literal table isn't practical. A cheap multiplicative mix
(same style as a simple hash function, not cryptographic) gives enough
avalanche that adjacent squares don't get visibly correlated offsets,
while staying a pure, allocation-free, branch-light function suitable for
the move-ordering hot path.

**Why magnitude is capped at `0..=3` for the tie-break**: `QUIET_BASE_SCORE`
is `0` and quiet moves are ordered purely by `history_score` (typically
tens to low thousands in magnitude once history/continuation-history
tables warm up) — a `0..=3` offset can only ever resolve a genuine tie or
near-tie between two quiet moves with (near-)identical history scores.
It cannot promote a quiet move ahead of a killer (`300_000`+), a capture,
or a meaningfully-history-favored quiet move, so this cannot introduce a
tactical blind spot — only reorders among moves the engine already
considered roughly equally good.

**Why LMR offsets are a small non-monotonic set rather than scaling
linearly with thread_id**: Recorded in `lmr_thread_base`'s own doc
comment (kept there, not duplicated at length here) — Lazy SMP gets
diminishing/negative returns from ever-more-aggressive reduction across
many threads, so a handful of repeating "personalities" decorrelates
helpers without any of them reducing so hard they stop contributing
useful TT entries.

**Known gap, explicitly not closed this session**: Elo impact of this
change is not yet measured. The new 23.1 `regression-gate` job runs
single-threaded by default (candidate vs. baseline `pet_dragon`, no
`Threads` setoption sent) and is sized as a 20-game/50ms smoke check, not
built to detect a genuine SMP-scaling improvement, which needs `Threads`
set >1 on both sides and meaningfully more games to resolve at this
sample size. Measuring this is a manual `uci_match_runner.yml` run
(`engine_a_uci_options`/`engine_b_uci_options` = `"setoption name
Threads value N"` on a multi-core CI runner) — left as a follow-up, not
folded into this session, since 23.2 itself was scoped as the code
change, not the validation run.

**Rejected alternative**: Vary `skill_level` or `contempt` per helper
thread instead of/in addition to LMR and move ordering, on the theory
that a wider spread of "personalities" (some more tactical, some more
positional) would diversify helpers further. Rejected: `skill_level` and
`contempt` are deliberately kept identical across all threads (see the
existing Phase 20 comment in `main.rs` — helpers already must match the
main thread's Skill Level depth cap, or they populate the shared TT with
full-strength lines that leak into a low-skill main search). Diversifying
those specifically would reintroduce the exact bug Phase 20 closed;
LMR/move-ordering are the correct, narrower place to add this kind of
variation because they affect *which lines get explored* without
changing *how the position is evaluated or how far a given tier is
allowed to look*.


## D50 — Self-Play Scale-Up: Sharded GitHub Actions Matrix + Merge, Not a Kaggle Job (2026-07-17)

**Decision**: Implement the code half of 23.3 by rewriting
`.github/workflows/selfplay.yml` from a single sequential job into a
3-job pipeline: `plan-shards` (builds a `[0..shards-1]` JSON list from
the `shards` input in plain bash, since Actions matrices need a
JSON-shaped source and `workflow_dispatch` inputs are plain strings) →
`selfplay-shard` (matrix job, one independent self-play batch per shard
on a disjoint seed range, `seed_start + shard*games_per_shard ..
+games_per_shard-1`) → `merge-shards` (downloads every shard artifact,
concatenates into one combined file, uploads as a single 30-day-retention
artifact). `selfplay.rs` itself is unchanged — the scale-up is entirely
in how many times it's invoked in parallel, not in the binary's own
logic.

**Why GitHub Actions sharding, not a dedicated Kaggle job**: D41's
"Rejected" section floated "a genuinely bigger self-play dataset
(500K-1M+ rows via a dedicated Kaggle job)" as the eventual lever, and
the roadmap's own Ease/Size note for 23.3 says "background GitHub
Actions self-play generation" — the two docs point in slightly different
directions, and this session's choice is GitHub Actions, not Kaggle,
for a mobile-only-maintainer reason: self-play generation is CPU search
work (no GPU, no training loop, nothing Kaggle's notebook environment
offers over a plain compiled binary), and `selfplay.yml` already existed
as a `workflow_dispatch`-triggered, mobile-app-runnable job — sharding
it is a same-surface-area change (still "tap Run workflow", still
downloads one artifact). A Kaggle job would mean Gokul manually
uploading a Kaggle notebook, managing a Kaggle session from a phone
browser, and downloading results from a different platform than the one
everything else in this repo already runs on. GitHub Actions is strictly
the lower-friction choice for the *generation* half; Kaggle stays exactly
where it already was — the *training* half, unchanged by this decision.

**Why a matrix + merge instead of one bigger sequential job**: The prior
workflow's largest recorded run was `n3000` (3,000 games, one job, one
runner) — whatever job-length constraint let that run complete becomes
the ceiling on total games per invocation. Fanning the same total game
count across N parallel runners doesn't change total CPU-time spent, but
does divide wall-clock time by roughly N (self-play games are
embarrassingly parallel — no shared state between games), which is the
actually-scarce resource for a solo maintainer who has to remember to
come back and start the next run. `fail-fast: false` on the shard matrix
so one flaky shard doesn't discard every other shard's already-generated
data; `merge-shards` runs with `if: always()` and reports (via a
`::warning::`) when fewer than `shards` files landed, rather than
silently producing a smaller combined file with no indication anything
was short.

**Why intermediate shard artifacts get 1-day retention, not 30**: Only
the merged combined file is the actual deliverable Gokul downloads and
feeds to Kaggle; the per-shard files exist solely so `merge-shards` can
assemble them. Keeping them at the default 30-day retention would just
be redundant storage — the same data lives in the combined artifact —
so they're set to expire quickly instead.

**Scope boundary, explicitly not closed this session**: This decision
covers the *generation infrastructure* only. Actually running enough
`selfplay.yml` invocations to meaningfully grow past the current ~500K-
row total, merging multiple runs' combined artifacts into one training
set, and re-running the Kaggle NNUE training job on the larger dataset
are all still-open follow-up actions requiring Gokul to trigger things
by hand — none of that can happen inside a single coding session. 23.3
stays unchecked in `ROADMAP.md` until that compute has actually
happened, not just become possible.

**Rejected alternative**: A single job with a higher `num_games` value
and a longer configured `timeout-minutes`, relying on GitHub's up-to-6-
hour default job ceiling headroom. Rejected because it doesn't actually
solve the calendar-time problem sharding does (10,000 games sequentially
at ~100ms/move average is still 10,000 games sequentially), and pushes
right up against the default job timeout as a fixed ceiling with no
parallelism benefit at all, for no advantage over sharding.

**Correction (added Session 79, 2026-07-18)**: The Scope-boundary
paragraph above says "re-running the Kaggle NNUE training job" —
that's wrong and shouldn't have been written without checking first.
NNUE training does not go through Kaggle at all; `train_nnue.yml` runs
the whole training loop natively on GitHub Actions via `train_nnue.rs`
(Phase 16.5), no Python/Kaggle notebook involved. Kaggle was the
original Phase 16.5 plan (D18.5/D21) but was superseded by this native
Actions workflow before D50 was written; this decision entry just
never got double-checked against the actual current workflow file.
Left the original paragraph as-is above (the record of what was
believed at the time) rather than editing it — this correction is the
fix, per this doc's own convention of appending rather than rewriting.


## D51 — Diagnostic Export "Result:" Line: Capture Reason at the Source, Not Reconstruct From Board State (2026-07-17)

**Decision**: Added two globals to `web/index.html` — `gameOverWinner`
('w'/'b'/null) and `gameOverReasonText` (short string: 'Checkmate',
'Stalemate', 'King captured', 'Insufficient material', 'Threefold
repetition', '50-move rule', 'Aborted', 'Resignation', 'Time forfeit') —
set at each of the 8 sites that set `customGameOver = true`
(`_checkGameOverInner`'s 6 branches, `handleAbort`, `confirmResign`,
`handleClockTimeout`'s 2 directions), reset alongside `customGameOver` in
`startGame()`. The diagnostic FEN export now reads these directly into a
new `Result:` line instead of the export tool (or a human, or me)
inferring the reason after the fact from raw board state.

**Why capture at the source instead of reconstructing later**: This
session's own diagnostic thread is the direct motivating evidence for
why reconstruction is unreliable: a "Game over (in check)" export was
initially assumed to mean checkmate, but "(in check)" is actually
computed from `customInCheck` alone — an orthogonal fact about the final
position, true for checkmate AND for a repetition/50-move draw that
happens to land on a position with the side-to-move in check. Confirming
the real reason required manually replaying the last several plies
against the halfmove clock to count position recurrences by hand — slow,
error-prone, and not something to redo for every future diagnostic
export. Every termination branch already builds a perfectly clear `msg`
string for the on-screen banner (e.g. `'🤝 Draw — Threefold
repetition'`); the fix is simply to also store the *reason* (not the
decorated banner text — see below) in a variable at the same moment,
rather than trying to infer it from `customGameOver`/`customInCheck`/
board state after the fact.

**Why a separate `gameOverReasonText` instead of just reusing the
existing `msg` string verbatim**: `msg` is decorated for the in-game
banner (emoji, exclamation points, "You"/"AI Engine" phrasing tied to
`playerColor`) — reusing it verbatim in the export would either
duplicate that framing oddly in a technical diagnostic file, or require
parsing it back apart to get White/Black + reason if the export ever
needs to present the result generically (e.g. independent of which side
the person happened to be playing that game). A short, undecorated
reason string (`'Threefold repetition'`, `'50-move rule'`, etc.) plus a
separate `gameOverWinner` color is more reusable and is what the export
function combines into its own `Result:` phrasing
(`'Result: White wins (you) — Checkmate'` /
`'Result: Draw — Threefold repetition'` / `'Result: Aborted — no
result'`), independent of the on-screen banner's wording.

**Why `gameOverWinner` is a plain color ('w'/'b'/null), not "player" vs
"AI"**: Keeps the stored fact objective (who actually won, in board
terms) and lets the export derive the subjective "(you)"/"(AI)" framing
from `playerColor` at read time — same reasoning as the existing
`Turn to move:`/`Player is playing:` lines already being reported
separately rather than pre-merged.

**Scope note**: This only feeds the diagnostic export. It does not
change any win/draw/loss detection logic itself (`_checkGameOverInner`'s
actual conditions are untouched) and does not change what's shown on
the in-game banner (`el.textContent`/`msg` still built exactly as
before) — purely additive bookkeeping so the *reason* is captured
once, correctly, at the moment it's known, instead of needing to be
reverse-engineered later.


## D52 — NNUE Re-Parked (Again): 5x More Data Closes Most of the Loss Gap But Uncovers a Logit-Saturation Bug — Data Volume Was Not the Real Blocker (2026-07-18)

**Context**: D34 parked NNUE after it lost to HCE on 483,080 rows
(val_loss 0.53776). D41 re-tested a bigger architecture (hidden_size=128)
on the same small dataset and it also lost, re-confirming the park but
leaving open whether data volume (not architecture) was the actual
bottleneck. Phase 23.3 (D50, this session's earlier half) built the
infrastructure to test that directly: 2,428,608 fresh self-play rows,
5x the old dataset, same hidden_size=32 architecture as D34 for a clean
comparison.

**What happened**: Training on the full 2,478,608-row combined dataset
(2,428,608 self-play + 50,000 Lichess) genuinely improved the loss
metric — best epoch 6/10, val_loss=0.50108, a real and meaningfully
sized drop from 0.53776. That part of the data-starvation theory holds.
But a 20-game pure-NNUE-vs-pure-HCE match (`NNUEWeight=100` vs default 0,
via `uci_match_runner.yml`) was a **20-0 shutout in HCE's favor** — not
a modest edge, a complete rout. Zero draws.

A 20-0 sweep is a different signal than "NNUE is weaker" — a merely
weaker-but-functional eval still flukes occasional wins on tactics. The
`eval_diag.yml` diagnostic (Phase 17.5d) confirmed why: **6 of 8 test
positions came back pinned at exactly the ±1500cp hard clamp**,
including positions that should evaluate near zero (a symmetric random
start) or moderately, not maximally. The network isn't giving a graded,
weaker opinion — it's collapsed into near-binary "very winning / very
losing" outputs with almost no signal in between, which would make any
search using it play close to randomly among any moves that aren't
already obviously winning or losing.

**Leading hypothesis for the saturation, not yet confirmed by a
follow-up run**: `train_nnue.rs`'s loss blends `lambda=0.7` eval-target
(soft, sigmoid-bounded) with 30% weight on the raw game *result* — a
hard 0/1/0.5 label — for self-play rows. BCE loss against a hard target
has no natural ceiling: loss keeps shrinking the more extreme (larger-
magnitude logit) the network's prediction gets, so the hard-label
component actively rewards logit blow-up. `weight_decay=0.01` and
`grad_clip_norm=1.0` (D30/D33) exist specifically to counter this
tendency, tuned against the old 483K-row dataset — they may simply not
be strong enough at 5x the data and (likely) more gradient steps.

**Decision**: Re-park NNUE again, same as D34/D41, but on a materially
different and more useful basis than either prior park: this time the
blocker is identified as a specific, plausibly-fixable training-config
issue (logit saturation from hard-label blending) rather than "needs
more data" (now tested and largely resolved on the loss metric, yet the
strength problem persisted) or "needs more capacity" (D41 already ruled
that out at the old data volume). The next experiment — raising `lambda`
toward 0.9-1.0 to reduce or remove the hard-result component's pull
toward saturation, same data, same architecture — is a single-variable
follow-up test, explicitly deferred to a fresh session per Gokul's
request rather than run immediately at the end of this one.

**Why not immediately retrain with a guessed lambda value in this same
session**: Two reasons. First, Gokul explicitly asked to document
everything now and explore further in a new session — respecting that
directly. Second, `weight_decay`/`grad_clip_norm` are both plausible
co-contributors and haven't been ruled out; jumping straight to a
lambda change without first deciding whether to isolate lambda alone
or adjust multiple knobs at once risks another single, ambiguous data
point rather than a clean test. That decision belongs at the start of
the next session, with full context, not rushed at the tail of this
one.

**What's NOT the takeaway**: This is not evidence that NNUE can't work
for Pet Dragon, and it's not a reason to abandon the data-scale-up
effort — the loss-metric improvement from 5x the data was real. It's
evidence that the *training configuration* (specifically the
result-blend mechanism) needs attention before more data or a bigger
network can be fairly evaluated again.

## D53 — Logit-Saturation Root Cause Fixed: Label Smoothing on the BCE Target (Session 80)

D52's saturation wasn't a data-volume problem — it was the training
objective itself. BCE against a hard 0/1/0.5 game-result label has no
finite minimizer; the loss keeps falling as the pre-sigmoid logit is
pushed toward +/-infinity, and `weight_decay`/`grad_clip_norm`
(D30/D33) only damp the resulting large gradients, they don't remove
the incentive. Added `label_smoothing` to `train_nnue.rs` and
`train_nnue.yml` (default `0.03`, CLI/workflow-configurable, `0`
reproduces exact pre-fix behavior): maps the blended `[0,1]` target
into `[label_smoothing, 1-label_smoothing]` before BCE, so the
objective's own minimizer corresponds to a finite logit instead of an
unbounded one. Math check: at `label_smoothing=0.03` the theoretical
best-case target for a decisive row is ~1390cp — barely below the
1500cp hard clamp in `inference.rs` — which is why `0.03` alone barely
moved the `eval_diag.yml` reading even though the fix was real (see
D55 for the full sweep this motivated).

## D54 — `eval_diag.yml` Does Not Reliably Predict Match Strength (Session 80)

Discovered while sweeping `label_smoothing`: the network with the
*best-looking* static calibration (`0.30`, 0/8 test positions
saturated, magnitudes close to HCE) produced the *worst* actual match
result (18-0-2 vs HCE, +511.5 Elo) of the whole sweep — worse than
`0.03`'s 5/8-saturated network. `eval_diag.yml` checks 8 isolated
positions' absolute magnitude against HCE; it says nothing about how
well the network discriminates between two similar positions a move
apart, which is what actually drives move ordering and search quality
during real play. Leading theory: heavier label smoothing compresses
the target range further, which may flatten the network's sensitivity
to small positional differences even as it looks more "reasonable" on
a handful of cherry-picked static positions.
**Consequence**: `eval_diag.yml` stays useful as a cheap pre-filter for
gross failures (sign errors, full clamp saturation) before spending a
20-game match run, but it must never again be used alone to rank or
select between checkpoints that already pass the sanity check — only
`uci_match_runner.yml` results decide that. Every checkpoint in D55's
sweep was match-tested for exactly this reason once D54 was found.

## D55 — NNUE Re-Parked at `label_smoothing=0.10`, NNUEWeight Stays 0% Default (Session 80)

Full sweep of `label_smoothing` post-D53-fix, all six values trained
on the same 2.48M-row 23.3 dataset (`lambda=0.7`, `hidden_size=32`,
10 epochs, seed=42 fixed throughout — single-variable test), and — per
D54 — every value validated with a real 20-game `uci_match_runner.yml`
HCE(0%)-vs-NNUE(100%) match, not just `eval_diag.yml`:

| label_smoothing | eval_diag saturated | Match (HCE score / Elo) |
|---|---|---|
| 0.03 | 5/8 | not match-tested |
| 0.05 | 3/8 | 95.0% / +511.5 |
| 0.08 | 3/8 | 92.5% / +436.4 |
| **0.10** | **3/8** | **87.5% / +338.0 ← best** |
| 0.15 | 6/8 (bad checkpoint) | not match-tested |
| 0.30 | 0/8 | 95.0% / +511.5 |

`0.10` is a clean, well-bracketed local optimum — monotonic
improvement `0.05→0.08→0.10`, then a cliff at `0.30`, not a single
lucky sample. Notably, `0.10`'s result (17W-2L-1D, 87.5%, +338 Elo) is
numerically identical to the very first NNUE blend test ever run on
this project (17.4/D25, a 25%-blend test against the old pre-fix
network) — after fixing the saturation bug and sweeping six values,
NNUE is back to roughly its original-ever performance level relative
to HCE, not ahead of it. Real progress from the pre-fix 20-0 shutout
(sign-correct now, non-degenerate, genuine but limited magnitude
discrimination), but not competitive.

**Decision**: ship as-is rather than continue chasing parity.
`nnue_pet_dragon_quantized.bin` committed at the `label_smoothing=0.10`
checkpoint. `NNUEWeight` stays at its 0% default (unchanged since D25)
— the network is available as a UCI option for anyone who wants to
enable it, but is not on the default search path. Re-parked.

**Reopening conditions**, if NNUE is revisited again: the `label_smoothing`
axis is exhausted (bracketed local optimum found, further sweeping this
one knob is unlikely to help). Next levers, in order of expected
leverage-to-effort: (1) the material-bucket confidence-gating idea
(use NNUE only in board-material buckets well-represented in training
data, HCE elsewhere — cheap, reuses the existing tapered-eval phase
signal, no new ML infra) discussed but not built this session; (2)
`hidden_size` increase (32→64+) or a genuinely larger dataset, both
higher-effort with no strong signal yet that capacity/data (rather
than the training objective) is the remaining bottleneck; (3) 23.5's
king-relative bucketed-feature architecture upgrade, explicitly
sequenced last since it's the largest effort and depends on the
smaller levers being exhausted first.

## D56 — Pawn-Flexibility Feature Redesign: Scoped, Not Shipped (Session 81)

While discussing why D53-D55's calibration issues persisted, walked
through D11's actual mechanism in detail: each of the 16 pawns has its
own feature, active only until that specific pawn's first move, then
permanently off for the rest of that game's length (up to 300 plies,
every ply recorded as a row). This raised a real design question —
whether 128 independent per-square flags is the right shape for this
signal at all, separate from any imbalance in how often it's active.

Proposed replacement: a single one-hot bucket per perspective over
(own unmoved pawns - opponent unmoved pawns), 5 buckets
(`<=-3 / -2..-1 / 0 / 1..2 / >=3`) instead of 128 per-square flags —
`NUM_FEATURES` 896->773. Full implementation was written (`features.rs`
rewrite, all 8 tests updated, `train_nnue.rs` comment fix) and is
available if this gets picked up again, but was **not shipped**: it's
a schema-breaking change (old checkpoints become permanently
incompatible, forced retrain, a real window where `main`'s tests are
red until that retrain lands) which the person doing the committing
explicitly did not want given how many rounds this session already
took. Superseded by D57's cheaper, non-breaking approach — see D58 for
current status. If NNUE work resumes and a real investment is
approved, this design is ready to implement rather than needing to be
re-derived from scratch.

## D57 — Phase-Balanced Oversampling: Implemented, Empirically Negative Result (Session 81)

Cheaper, non-breaking alternative to D56: instead of changing what any
feature means, oversample training rows with a rare pawn-start-feature
activation count (D11) in the per-epoch training order —
inverse-sqrt-frequency weight expressed as integer row duplication,
`phase_balance_cap` (default 4, `1` disables). `features.rs` untouched,
no schema change, currently-committed network stays loadable
throughout.

Tested at `phase_balance_cap=4` on the same 2.48M-row dataset,
`label_smoothing=0.10`. Result: **the fix had zero effect.** The
balanced row count exactly equalled the original row count (no row
was duplicated), and `val_loss=0.57740` was bit-for-bit identical to
the un-oversampled `0.10` baseline run.

Why: the actual activation-count histogram logged by this run is far
flatter than the hypothesis assumed — every bucket (0 through 16
active features) fell within a ~90K-163K range, under 2x apart, not
the sharp early-game-only spike the theory predicted. Notably, bucket
16 (all 16 pawns still on their start square — literally the position
before any move) had *more* rows (128,394) than several of the
supposedly-common decayed-state buckets, the opposite of what the
"near-game-start rows are rare" theory predicted. With imbalance this
mild, inverse-sqrt weighting produces normalized weights too close to
1.0 to survive integer rounding.

**Conclusion: the row-imbalance hypothesis from D53-D55's diagnostic
work is not well supported by the actual data.** Not worth pursuing
further by tuning the weighting more aggressively — that would be
fixing a problem the data says isn't really there. No repo change
needed as a result (currently-committed `0.10` network already
reflects what this run produced). `phase_balance_cap` stays in
`train_nnue.rs`/`train_nnue.yml` as an available, defaults-to-4,
proven-inert-at-current-data knob — cheap to leave in, not worth
removing.

## D58 — NNUE Re-Parked Again: Cheap Levers Exhausted (Session 81)

Following up on D55's parking, this session tried the two cheapest,
lowest-risk remaining levers before considering bigger investments:
D56 (pawn-feature redesign, scoped but not shipped — schema-breaking)
and D57 (phase-balanced oversampling, shipped, empirically negative).
Also discussed but not started: confidence/in-distribution gating
(route to NNUE only where training data is dense, HCE elsewhere) and
distillation from Stockfish's public eval for the phase of a Pet
Dragon game that's converged close enough to standard chess to trust
it (D10's phase-out design is meant to reach that point) — both real,
scoped ideas, neither attempted this session.

**Current best remains the `label_smoothing=0.10` checkpoint from
D55**: 17W-2L-1D, 87.5%, +338 Elo for HCE. Unchanged by this session's
work. `NNUEWeight` stays at its 0% default.

**Status: re-parked.** Every remaining lever with a plausible path to
closing real ground on HCE — D56's feature redesign, swapping the
training pipeline off NORU's pure-Rust CPU trainer onto a
GPU-accelerated PyTorch/Colab pipeline (keeping NORU for its already-
correct i16 quantized inference), or Stockfish-distillation data
augmentation — is a genuine structural investment (new training
infrastructure, a schema-breaking retrain, or a new data pipeline),
not a quick, low-risk try. D55's original reopening-lever list is
superseded by this one; treat this entry as the current status instead
of D55's for anyone resuming NNUE work.

## D59 — Singular Extension Family: Multi-Cut Pruning + Negative Extensions (Session 82)

Extended Phase 13.3's base singular extension with two Stockfish-family
siblings, both reusing the existing verification search's result rather
than doing any extra search work. Multi-cut: if the reduced-depth
search that excludes the TT move still reaches `singular_beta`, at
least one *other* move also refutes at that margin — the position is
being cut multiple different ways, so the whole node is pruned right
there (`return singular_beta`), before move generation. This is the
same "early return, skip TT store" shape probcut and razoring already
use elsewhere in `alpha_beta.rs` — not a new pattern introduced for
this. Negative extension: if verification did *not* confirm
singularity, but the TT move's own recorded score already meets beta,
the TT entry is telling us this is very likely a cutoff regardless —
reduce (not extend) the TT move's own search rather than spend extra
depth re-confirming what the TT already suggests. Reduces by 1 ply more
in non-PV nodes than PV nodes (`-1` PV / `-2` non-PV), matching
Stockfish's `-2 - !PvNode` shape.

Implementation: `alpha_beta.rs`'s singular block generalized from a
boolean `singular_extension` flag (always `+1` or `0`) to a signed
`tt_move_extension` (`+1` / `-1` / `-2` / `0`), applied only to the TT
move via `move_ext` (renamed from `singular_ext` at all 4 PVS-block use
sites — `depth - 1 + move_ext`, unchanged arithmetic shape, just a
wider range of values now). A negative `move_ext` can push
`depth - 1 + move_ext` below zero for the TT move's own recursive call;
this is safe and already handled — `alpha_beta_with_excluded`'s
`depth <= 0` branch drops straight to `quiescence()`, same path any
other depth-exhausted node takes.

No existing test exercised this logic before (confirmed via grep — no
`singular` test in `alpha_beta.rs` or `pruning.rs`), so this carried no
risk of silently changing a previously-tested behavior. One new test
added (`test_search_at_singular_extension_depth_no_panic`, depth 7,
5 seeds) checks the search stays bounded and legal at a depth where
this code path is live; it does not assert which of the three branches
(extend/multi-cut/negative-extend) fires for any given seed, since
that's position- and TT-state-dependent and not meaningful to pin down
per-seed.

⚠️ Not yet Elo-measured — same open item as D49/23.2's thread-
differentiated Lazy SMP. A real `uci_match_runner.yml` run (not just
`cargo test` passing) is the way to quantify this, not done this
session.

⚠️ Not yet CI-confirmed to compile/pass at all — see D60's note, same
caveat applies to both of this session's changes together.

## D60 — Late Move Pruning (LMP) (Session 82)

New technique, distinct from LMR: LMR still searches a late quiet move,
just at reduced depth, so a re-search can recover a buried tactic. LMP
instead skips the move outright once enough quiet moves have already
been tried at this node without raising alpha — Stockfish/Ethereal-
family technique, accepting that moves ordered this late at this
shallow a depth essentially never turn out to matter.

New `MAX_DEPTH_LMP = 8` constant (`mod.rs`) and
`pruning::lmp_threshold()` / `should_apply_lmp()`, wired into
`alpha_beta.rs`'s move loop immediately after the existing futility-
pruning block (same guard shape: non-PV only, never in check or on a
checking move, never near mate-range alpha/beta scores). Threshold
table: `[0, 3, 4, 6, 9, 12, 16, 20, 25]` indexed by depth 0-8.

Stockfish and Ethereal both differentiate this threshold by an
"improving" flag (is static eval better than it was two plies ago for
this side) — a higher threshold when improving, lower when not. Pet
Dragon's `alpha_beta` doesn't track an improving flag anywhere
currently; adding one is a real, separate change (needs a per-ply
static-eval history thread through the search) with its own risk
surface, not something to bundle silently into this session. This
implementation uses the single, more conservative ("non-improving")
threshold uniformly at every node instead — it can only prune *fewer*
quiet moves than an improving-aware version would, never more, so it's
a safe default pending that follow-up. If revisited: add an
`improving: bool` parameter alongside the existing `pv_node`/`in_check`
signature pattern already used throughout `alpha_beta.rs`, and give
`lmp_threshold` a second (higher) table for the improving case.

⚠️ Same two caveats as D59: not yet Elo-measured, and not yet
CI-confirmed. This session's sandbox had no reachable Rust toolchain
(rustup's domain isn't in the network allowlist, and cargo/rustc
weren't pre-installed) — verification was full manual review of both
changed files, a brace/paren/bracket balance check, and a diff against
freshly-pulled pristine source confirming only the intended lines
changed, not an actual `cargo test` run. **Next session's first action
should be confirming these changes build and pass CI before relying on
anything else about them.**

## D61 — NNUE Shelved for the Future (Session 82)

Gokul's call: shelve NNUE work entirely for the foreseeable future,
rather than continuing to work the data-volume problem (D53/D55/D57/
D58). Pet Dragon ships and runs on Texel-tuned HCE (`NNUEWeight`
already defaults to 0%, per D55/D58 — this decision doesn't change
runtime behavior, it changes what future sessions should spend time
on). Consequence: 23.5 (NNUE architecture upgrade — king-relative
bucketed features) is moot as scoped, since it's specifically an NNUE
change; see ROADMAP.md's 23.5 entry, left in place but marked HELD
rather than deleted so D55/D57/D58's data-volume analysis isn't lost
if this is revisited. If NNUE work resumes later, start from D58's
three reopening options (better inference reuse of NORU's already-
correct i16 quantized path, a training-infra swap off NORU's CPU
trainer, or Stockfish-distillation data augmentation) — those were
already identified as the real levers, not a quick retry of what's
already been tried.

## D62 — Variant Opening Statistics (23.4) Held Pending Design Decision (Session 82)

Investigated implementing 23.4 this session (per the original ROADMAP
scoping, "Ease: Medium, data-aggregation script over existing
`selfplay.rs` output") and found the scoping was wrong on two counts,
surfaced by actually reading `selfplay.rs` before writing anything
(mandatory read-before-write rule doing its job here):

1. **No data to aggregate yet.** `selfplay.rs`'s output format is
   NNUE training rows only — `stm_features | nstm_features |
   search_eval_cp | game_result` — and records neither the starting
   seed, the position's identity, nor which move was played at the
   root. There's currently no file anywhere with a "starting position
   → root move → result" record. 23.4 isn't an aggregation script over
   existing output; `selfplay.rs` would need a new output stream
   first.

2. **Coverage problem, more fundamental.** `Position::generate_with_
   seed` draws from 2.16M distinct starting positions. Self-play games
   run on sequentially incrementing seeds (`seed_start + game_idx`),
   so unless job runs deliberately overlap seed ranges, essentially
   every self-play game visits a starting position that's never
   repeated anywhere else. An exact-position-keyed opening book built
   from that data would have a near-zero hit rate against any real
   game — the same starved-coverage shape as NNUE's problem (D53),
   just in a different keyspace (positions instead of training rows).

Presented Gokul two real design paths — exact-position book (simple,
but hit rate only grows as self-play volume grows, slowly, against a
2.16M-position denominator) vs. bucketing by structural features
(pawn shape / piece pattern) so similar-but-not-identical positions
can share statistics, generalizing instead of requiring an exact
match — and Gokul chose to hold the whole item rather than pick a
path yet. Recorded here rather than half-designed, so a future session
doesn't restart from the original (incorrect) "Ease: Medium" framing
in ROADMAP.md. Resume by re-reading ROADMAP.md's 23.4 entry and this
decision, then actually picking exact-vs-bucketed before writing any
code — the two investigation findings above stand regardless of which
path gets picked.

## D63 — HCE Term-Gap Audit: Three Candidates Identified, None Implemented (Session 82)

Gokul asked whether the two eval improvements discussed so far were
"the best the engine can be tuned" — audited all six `eval/` submodules
fresh (not from memory of ENGINE_ARCHITECTURE.md's summary) against the
standard classical-engine term list to give a grounded answer rather
than a guess.

Confirmed already present and correct: material with bishop-pair bonus
(`material.rs`), four pawn-structure terms — isolated/doubled/backward/
passed with rank-scaling (`pawns.rs`), four king-safety terms —
attacker-count danger/pawn shield/open-and-semi-open-file penalties
(`king_safety.rs`), rook-specific terms — open file/7th rank/batteries/
connected rooks (`open_lines.rs`), mobility (`mobility.rs`), tempo, and
phase tapering threaded through every term via `eval/mod.rs`'s
`evaluate()`. Most of the well-established HCE term list is already
checked off and Texel-tuned (D35).

Three real, confirmed-by-source gaps found, ranked by expected value:

1. **Passed-pawn king distance** — `pawns.rs`'s passed-pawn bonus
   (`PASSED_PAWN_BONUS`, rank-indexed) never checks either king's
   distance to the pawn. This is one of the highest-value terms in any
   strong classical engine (the "square of the pawn" concept) and is
   completely absent. Ranked highest.
2. **Pawn storm** — `king_safety.rs` scores the defensive shield
   (`PAWN_SHIELD_BONUS`) but nothing scores advancing pawns toward the
   *enemy* king as an attacking resource. Confirmed via grep across
   `pawns.rs`/`mobility.rs`/`open_lines.rs` that no king-relative logic
   exists anywhere outside `king_safety.rs`.
3. **King-relative PST bucketing** — `tables.rs`'s `pst_value()` takes
   only `(kind, sq, color)`; no term anywhere makes a piece's
   positional value depend on where either king is. Ranked lowest/most
   speculative — would need new Texel-tunable parameters, and needs to
   stay *coarse* (a handful of buckets: same-side/center/opposite-side
   or similar) rather than full per-square king-relative buckets in the
   NNUE-HalfKP sense. A full NNUE-scale king-bucket PST (~64× the
   current flat-PST parameter count) would hit the exact same
   parameter-count-vs-training-data wall NNUE already has at 896 inputs
   with ~2.5M rows (D53/D55) — HCE has far less training data available
   than NNUE does (Texel tuning's 147,283-sample runs, not NNUE's
   millions), so a coarse handful of buckets is the ceiling for this
   term, not a design choice to revisit later for something bigger.

None of the three are scheduled or sized into a roadmap item — this is
a documented, ranked candidate list for whenever Gokul wants to pick
one up, not a commitment. Explicitly told Gokul the honest ceiling:
HCE has a structural cap no amount of hand-crafted terms closes (that's
the entire reason NNUE exists as a category), term-count has
diminishing returns past this point (more free parameters need more
Texel-tuning data, the same constraint already shaping what's
tunable), and search is already close to state-of-the-art per the
Session 82 competitive analysis (D59/D60 already captured the two
concrete gaps found there) — so what's realistically on the table here
is a handful of incremental items, not a large untapped territory. The
actual large remaining ceiling is the NNUE data-volume problem, which
D61 shelves.

## D64 — Alternative Evaluation Paradigms Surveyed, None Adopted (Session 82)

Gokul asked, beyond HCE and NNUE, what else exists as an evaluation
approach. Answered from general chess-engine-architecture knowledge
(cross-checked via web search, since the field moves) rather than
assuming Pet-Dragon-specific applicability without saying so. Recorded
here so this doesn't get re-investigated from scratch in a future
session — the landscape survey stands even if circumstances (compute,
data) change later, though the applicability conclusion should be
re-checked if so.

- **MCTS + policy/value network** (AlphaZero, Leela Chess Zero) — not
  an eval swap, a different *search* paradigm: Monte Carlo Tree Search
  guided by a network outputting both an evaluation and a move-
  probability distribution, replacing alpha-beta entirely. Needs
  GPU-scale parallel compute for both training and inference to be
  competitive. Adopting this would mean discarding Pet Dragon's entire
  search stack — the part already confirmed near state-of-the-art in
  the Session 82 competitive analysis — not augmenting it. Ruled out:
  wrong deployment target (WASM/browser + CPU-only CI), and would
  discard rather than build on existing, working search work.
- **Searchless transformer evaluation** (DeepMind's "Grandmaster-Level
  Chess Without Search," Ruoss et al.) — a large transformer trained
  via supervised learning on Stockfish-annotated positions, single
  forward pass at inference, no tree search at all. Conceptually the
  opposite end of the spectrum from NNUE (replaces the search loop
  rather than living inside it). Ruled out: needs even more training
  data than NNUE already lacks, since there's no search to lean on for
  signal — inherits and worsens the exact problem D61 just shelved.
- **GPU-sized NNUE variants** — some newer engines use larger nets
  requiring GPU rather than staying in NNUE's original CPU-quantized-
  int8 lane. Ruled out: breaks the CPU/WASM-friendly efficiency that
  was the actual reason NNUE was chosen as Pet Dragon's neural option
  in the first place (D10/D11).
- **Policy-guided move ordering** — a smaller-scale hybrid: keep
  classical alpha-beta and eval (HCE or NNUE), add a small network
  just to improve move ordering at high-value nodes. The only option
  here actually compatible with Pet Dragon's existing architecture
  without a rewrite. Not ruled out on architectural grounds, but it's a
  search technique needing training data, not an eval technique — same
  data-volume dependency NNUE has, so it inherits D61's shelving logic
  rather than being a way around it. Noted as the one item on this
  list that could be revisited independently of NNUE's status, if ever
  training data stops being the constraint.

Conclusion given to Gokul: none of these are a practical third path
alongside HCE/NNUE for Pet Dragon right now — each either needs an
architecture Pet Dragon doesn't have (GPU, MCTS) or training data it
doesn't have (all the neural options), consistent with and not
contradicting D61's NNUE-shelving decision.

## D65 — King-Relative Term Design: Minor-Piece Shelter Over Full PST Buckets (Session 83)

**Context:** D63's item 3 candidate ("King-relative PST bucketing")
was deliberately left as the lowest-confidence entry on that list,
flagged as needing design thought before implementation rather than a
straightforward pick-up. Gokul asked to discuss the design before
implementing.

**The actual gap, once examined closely:** it isn't that `eval/` lacks
any king-relative piece scoring — `king_safety.rs`'s `attacker_weight`
already scores enemy pieces attacking near a king (tropism, from the
attacking side). The real gap is narrower: nothing scores the mirror
case, our own minor pieces staying close to our own king as defenders
(clustering, from the defensive side).

**Three options were weighed:**
- **Option A** — flat MG-only bonus per own knight/bishop sitting in
  the same king-file-third zone (queenside/center/kingside, 3 zones)
  as our own king. ~2 new parameters. Cheapest, easiest to hand-verify
  against pre-existing tests, most clearly non-overlapping with any
  existing term.
- **Option B** — a fuller 3-zone table scored across more piece kinds
  (knights/bishops/rooks/queen), ~10-15 parameters. Richer, but rooks/
  queen wanting the zone opposite our own king already overlaps with
  `open_lines.rs`'s rook-activity terms — risk of two terms both
  nudging the same underlying effect, hard to disentangle without
  actual tuning-data validation.
- **Option C** — mirror of A but scoring proximity to the *enemy*
  king's zone (attacking side) for rooks/queen/knights. Highest
  double-counting risk of the three: this is the closest in spirit to
  what `attacker_weight` already measures (piece activity near the
  enemy king), just from static square proximity instead of actual
  attack-square coverage.

**Decision: Option A.** Smallest parameter count, most defensible as a
genuinely non-overlapping effect, cheapest to Texel-tune, and the
easiest to hand-verify wasn't accidentally double-scoring something
`king_safety.rs` or `open_lines.rs` already captures. B and C are not
ruled out permanently — they're deferred pending real tuning-data
evidence that A alone under-fits, which isn't available from static
analysis alone.

**Implementation:** `KNIGHT_NEAR_OWN_KING_BONUS`/
`BISHOP_NEAR_OWN_KING_BONUS` in `eval/king_safety.rs` (values `8`/`6`,
hand-picked, not yet Texel-tuned), full Texel-chain wiring in the same
submission, CI-confirmed green on the first submission
(`logs_80664145490.zip`). See `ROADMAP.md`'s Phase 24 item 3 entry and
`SESSION_LOG.md`'s Session 83 entry for the full implementation
summary.

## D66 — Full Texel Re-Tuning Chosen as Next Strength Lever (Session 83)

**Context:** Gokul set a new broad goal — make Pet Dragon "lethal,
precisive, scary, brutal" regardless of game phase — and asked Claude
to decide the right first move rather than picking for him.

**Options weighed:** (1) full Texel re-tuning pass, (2) endgame
conversion technique, (3) further search depth/efficiency tuning, (4)
un-shelving NNUE (D61).

**Decision: (1), full Texel re-tuning.** Reasoning:
- A mistuned evaluation constant is wrong in every phase of the game
  equally — this is the one lever that's structurally "irrespective of
  opening/middlegame/endgame" by construction, not just in aspiration.
- The hard infrastructure already exists and is proven
  (`texel_gen.rs`/`texel_tune.rs`/the `.yml` workflows, all built and
  validated in Phase 14) — this is a re-run + apply, not new
  engineering.
- It's genuinely stale in two ways, so this isn't "re-run something
  that's already fine": every eval term added since Session 56's one
  tuning pass (including this session's 3 new D63 terms) ships
  hand-picked and untuned, and Phase 14.2's original dataset has long
  since expired (30-day artifact retention).
- (2) and (3) are narrower and harder to scope without first knowing
  where the current eval/search is actually leaving Elo on the table —
  a full re-tune is a reasonable prerequisite diagnostic for either,
  not a competing alternative to them. (4) remains a real structural
  investment per D58, not a quick lever.

**Not a permanent ranking** — if a re-tuned eval doesn't move the
needle, (2)/(3) become the natural next investigation, informed by
where the re-tune's residual errors concentrate.

**Status:** scoped as Phase 25 in `ROADMAP.md`, not started — next
concrete action is Gokul triggering `texel_gen.yml` to generate a
fresh dataset (mobile-friendly, `workflow_dispatch` only, matching
every other training workflow in this repo).

## D67 — Phase 23.4: Bucketed Structural Opening Statistics — Full Design Chosen (Session 84)

D62 held 23.4 pending an exact-vs-bucketed design pick. Chosen:
**bucketed by structural features (Path B).** Exact-position book
(Path A) rejected — with self-play drawing from a 2.16M-position
keyspace on sequentially incrementing seeds, a real game would need
to draw from a small *reused* seed pool for an exact-position book to
ever hit, and that's a policy change to how real games get their
starting position (currently a fresh random seed every game), not a
23.4-scoped decision. Not taken.

### Bucket key (two-tier, so v1 ships small and v2 extends the same
### aggregation path rather than redesigning it)

**Tier 1 (v1 — build this first):**
```
BucketKey = (rook_files: sorted 2-of-8, knight_files: sorted 2-of-8)
```
Only the file each rook/knight *started* on, independent of whether
it started rank 1 or rank 2. Estimated at the time of this design
(WRONG — corrected empirically in D71, Session 84, once real data
existed — see D71 for the actual bucket count and why this estimate
undercounted) as `C(8,2) × C(6,2)` = 28 × 15 = 420 buckets. Chosen
because rook/knight file placement drives the two things that matter
most for early-game plans in a variant with open lines from move 1
(VARIANT_ARCHITECTURE.md) — open/semi-open file control and knight
outpost potential — while staying (it was believed) small enough that
a few thousand self-play games actually populate most buckets with
more than a handful of samples each, unlike the 2.16M-keyspace problem
Path A had. The reasoning for *why* file-based bucketing is the right
axis still holds — only the specific count was wrong.

**Tier 2 (future extension, not v1):**
```
BucketKey += (bishop_files: sorted 2-of-remaining-4,
              queen_file: 1-of-remaining-2,
              rank_mask: 7-bit, one bit per non-pawn/non-king piece,
                         1 = started rank 2, 0 = started rank 1)
```
Adds ~6 × 2 × 128 ≈ 1500× more buckets. Do not build this until Tier 1
buckets show enough per-bucket sample depth in practice that splitting
further wouldn't just re-create the sparsity problem — this is an
empirical call to make once real data exists, not a decision to make
now.

### Data pipeline

1. **`selfplay.rs` — new output stream.** Current output
   (`stm_features | nstm_features | search_eval_cp | game_result`) has
   no starting-position identity or root move (D62 finding). Add a
   second, opt-in output stream:
   `starting_seed | rook_files | knight_files | root_move_uci |
   game_result`. Computed once per game at position setup — cheap.
   Existing NNUE training-row output is unchanged; this is an
   additive stream, not a replacement, so Phase 25/NNUE data
   generation isn't affected by this work.
2. **Aggregation script** (new, offline — runs where `texel_tune.rs`
   currently runs, same CI pattern). Groups rows by `BucketKey`,
   tallies win/draw/loss per candidate root move within each bucket.
   **Minimum-sample threshold: 30 games per (bucket, root move) pair**
   before that entry is trusted (standard small-sample floor) — below
   threshold, the entry is omitted rather than shipped with a
   misleadingly confident win rate.
3. **Existing self-play data cannot be reused.** Every prior
   self-play run (Phase 14 onward) used the old output format with no
   starting-position identity recorded — this needs a fresh generation
   pass with the new stream, not a re-aggregation of existing
   artifacts. Real cost to budget when this gets picked up.

### Runtime deployment

WASM/browser play needs the bucket-stats table client-side. Chosen:
**bake it into the binary at build time** as a generated
`src/opening_stats.rs` (a static table, same shape as how magic
bitboard constants are generated/committed) produced by the offline
aggregation script and committed to the repo — not fetched at runtime.
420 buckets × a handful of root-move entries each is small; a
runtime fetch would add load-time latency and a network dependency
for something that fits in the binary for free. Re-generate and
re-commit this file whenever a new self-play data pass accumulates
enough volume to matter — same "downloadable, replace the file"
workflow already used for Texel-tuned weights.

### Usage mechanism

**Not** an auto-play opening book (engine blindly plays the top bucket
move). Chosen: **root-only move-ordering bias** — if the current
position's bucket has a trusted entry (past the 30-game threshold)
favoring a specific root move, nudge that move earlier in root move
ordering / apply a small eval bonus at the root, but let full search
still evaluate and can still override it. This degrades gracefully
(empty or thin bucket ⇒ zero effect, falls straight through to normal
search) rather than risking a bad hard-coded book move in a variant
with no established opening theory to validate against. Same "one
more capped signal, not a replacement mechanism" shape already used
for the Phase 23.6 extension family (D59) and Tension Field's proposed
search hook.

### Status

**Designed, not started.** Queued behind Phase 25 (full Texel
re-tune) — no code changes yet. Concrete build order for whichever
session picks this up: (1) `selfplay.rs` new output stream, (2) run a
fresh self-play generation pass with it, (3) aggregation script, (4)
generate + commit `src/opening_stats.rs`, (5) wire root-move-ordering
bias into `ordering.rs`, (6) unit test against a few known buckets by
hand before trusting the table. Resume by re-reading this entry, not
by re-deciding exact-vs-bucketed — that question is closed.

## D68 — Threats Evaluation Term (Hanging/Undefended Pieces): Fourth HCE Gap Identified, Not Implemented (Session 84)

Gokul asked whether the engine has anything scoring "pieces defending
each other" / board-wide support relationships. It does not — checked
against `ENGINE_ARCHITECTURE.md`'s current term list (material, PST,
mobility, pawn structure, king safety, open lines, tempo) plus a grep
of `eval/*.rs`. The only existing piece-supports-piece logic is
`open_lines.rs`'s connected-rooks/battery detection — narrow, rook/
file-specific, not a general defended-vs-hanging signal.

**This is not the same proposal as the uploaded "coordination graph"
idea** (that one was generic and speculative, no prior-art citation).
This is Stockfish's established **Threats** evaluation term
(`threats.cpp` in Stockfish's `evaluate.cpp` pipeline, GPL v3, same
family already credited for Pet Dragon's material/PST baseline per
`PROJECT_CONTEXT.md`) — hanging-piece penalty, weak-queen-protection
penalty, minor/rook threat bonus, restricted-piece penalty. Since it's
a known-working term from an already-credited GPL v3 source rather
than an unvalidated idea, it's lower-risk than either Tension Field or
the coordination-graph pitch, and belongs in the same D63-style
gap-audit lineage rather than as a new speculative feature.

**Scope, matching D63's three items:**
- New `eval/threats.rs`, MG-scoped like `king_safety.rs`'s pattern
  (existing convention: `* phase / 24` in this repo, not the tapered
  `s()`/`taper()` pipeline `pawns.rs` uses — pick per D65/D63's
  precedent, whichever the implementing session confirms is still the
  house style by re-reading a current file, don't assume from this
  note).
- **Reuse existing attack bitboards** — `mobility.rs` and
  `king_safety.rs` already compute per-square attacker sets; this must
  reuse those rather than adding a second full attacker-enumeration
  pass (same reuse discipline the uploaded Tension Field spec called
  out, and the same NPS-regression risk if skipped).
- **Double-counting risk, flagged in advance this time** (Phase 24
  items 1-3 found their overlap risk mid-implementation or via
  hand-check; doing it now instead): hanging-piece detection overlaps
  conceptually with `mobility.rs`'s attack counts and with
  `king_safety.rs`'s attacker-weight tropism term. Needs the same
  hand-verification against existing test FENs Phase 24 items used
  before trusting it's additive signal — don't skip that step just
  because the term itself is well-precedented.
- Full Texel-chain wiring in the same submission as the eval change
  (features/predict/weights/weights_f64/predict_f64/texel_diag/
  texel_tune) — apply Phase 24 item 1's lesson directly, don't
  discover the missing sites via failed CI.

**Status:** documented candidate, not implemented, not scheduled.
Queued behind Phase 25 same as D67 — if picked up before Phase 25
closes, its new eval term would also need folding into whatever
Texel dataset generation happens for Phase 25, same sequencing
concern raised for Tension Field. Logged as a Phase 24 addendum in
`ROADMAP.md` rather than reopening Phase 24's "fully closed out"
status — Phase 24 items 1-3 stay closed; this is a new, separate
candidate found by a different question, not an unfinished Phase 24
item.

## D69 — Phase 25 Complete: Full Texel Re-Tune Applied (Session 84)

Applied the Phase 25 re-tune to production code. Source: `texel_tune.yml`
run against 62,125 fresh self-play positions (`texel_gen.yml`,
seed_start=15000, n=3500), `epochs=75`, `weight_decay=0.03`. Sanity-
checked two ways before applying: (1) a 15-epoch/decay=0 run first to
confirm the pipeline itself was healthy (loss monotonically decreasing,
no NaN/divergence) before spending the longer run; (2) `texel_diag.yml`
run against the final 75-epoch result — all 10 sanity cases (5 random
Pet Dragon starts symmetric, up/down material swings correctly signed,
K+P vs K endgames correctly signed) passed clean.

**Files touched:** `eval/material.rs`, `eval/tables.rs`,
`eval/mobility.rs`, `eval/pawns.rs`, `eval/king_safety.rs`,
`eval/open_lines.rs`, `eval/mod.rs`, `texel/weights.rs`. Every field
cross-verified programmatically (not just by eye) to match exactly
between each eval file and `TunableWeights::default()` — dual-sync
confirmed, not assumed.

**One tuned result rejected outright, not applied — this is the
important part of this decision.** The tuner returned
`KNIGHT_NEAR_OWN_KING_BONUS = -1` and `BISHOP_NEAR_OWN_KING_BONUS = -3`
(Phase 24 item 3, its first-ever tune). Both existing
`king_safety.rs` tests encode a validated invariant — sheltering your
own minors near your own king should help, not hurt —
(`test_minor_piece_shelter_knight_same_zone_vs_different` hard-asserts
the constant is positive; `test_king_safety_rewards_minor_piece_shelter`
checks the shelter ordering directly) and a negative value breaks both.
Unlike `bishop_pair`/`rook_on_seventh` in material.rs and open_lines.rs
(also first/early real tunes, also initially sign-flipped in the
15-epoch/no-decay sanity run), this pair did NOT recover under
`weight_decay=0.03` across the full 75-epoch run — treated as this
term's first tune landing on noise rather than signal, most likely
because "friendly minor in the exact same king-file-zone as the king"
is a narrow, rare condition even across 62,125 samples. **Kept at the
Phase 24 hand-picked defaults (`8`/`6`) in both `king_safety.rs` and
`weights.rs`.** This is the correct call per house rules — a tuned
result that inverts a validated sanity check gets rejected, not
shipped, and the tests are not the thing that gets changed to
accommodate it.

**Watch items — not blocking, but worth re-checking once more
self-play data accumulates (next natural re-tune, not a scheduled
task):**
- `rook_on_seventh` MG flipped negative (`s(8,47)→s(-14,39)`). Verified
  this doesn't break `test_rook_on_seventh` — that test's position has
  phase=2 (near-pure endgame taper weighting), so the still-strongly-
  positive EG value dominates — but the MG sign flip itself is
  unconfirmed as signal vs. noise from one tune.
- `pawn_storm_bonus` (Phase 24 item 2, its first tune) came out
  non-monotonic (`[14,10,46,13,14,6,1,-5]` vs. the smooth hand-picked
  `[40,32,24,16,8,0,0,0]`) — index 2 sits well above its neighbours.
  Existing tests only check the distance-3-vs-6 endpoints (still
  correctly ordered, 13 > 1), so this shipped, but the middle bump is
  unexplained.
- `bishop_pair` (`s(18,29)→s(2,15)`) and `battery_bishop_queen`
  (`s(15,5)→s(33,19)`) both moved substantially from their Phase 14/
  hand-picked origins. Both stayed positive and passed `texel_diag`, so
  applied, but flagged as larger-than-typical swings worth a second
  look with more data.

**Tempo bumped `20→24`** — required widening
`test_evaluate_start_pos_near_zero`'s bound (`≤20→≤30`) since that
bound was tightly coupled to the old exact tempo value; the widened
bound still enforces the same "near zero, tempo only" intent.

**Next natural trigger for re-tuning:** whenever self-play data volume
meaningfully increases again (e.g. picking up D67's opening-stats data
collection, or any future Phase pass that generates fresh self-play
data) — not a scheduled task on its own.

## D70 — CI Caught a Real Test Break in D69's Application: ENEMY_KING_DIST_EG/OWN_KING_DIST_EG Reverted (Session 84)

Gokul ran CI after committing D69's application. `test_passed_pawn_bonus`
failed: `-24` instead of the expected `>0`. Root cause traced exactly —
not guessed: that test's position (White pawn e5, White king e1, Black
king e8) has Black's king sitting exactly on the pawn's promotion
square while White's king is 7 squares away. D69 had applied Phase 25's
tuned `ENEMY_KING_DIST_EG`/`OWN_KING_DIST_EG` (`1→3`, a 3x jump, D63
item 1's first-ever tune), which cubes through `own_dist × advancement`
in `passed_pawn_king_distance_bonus()` and swings this specific
position to `-84` for that term alone, overwhelming the base passed-pawn
bonus (`77 - 17 - 84 = -24`, matching the CI output exactly).

**This should have been caught before shipping, not after.** D69's
per-field verification checked that eval and `weights.rs` matched each
other correctly (dual-sync), and checked several individual test cases
by hand — but `ENEMY_KING_DIST_EG`/`OWN_KING_DIST_EG` specifically was
reasoned about too quickly, on the assumption that a positive-staying
weight on a term that was already accounted for in the passed-pawn
bonus couldn't flip the total sign. That reasoning didn't actually
compute the specific test FEN's numbers, and turned out wrong. No
Rust toolchain is available in this environment to compile and run
`cargo test` directly, which is exactly why this kind of arithmetic
needs to be walked through explicitly per-test rather than pattern-
matched from a "this term stayed positive, should be fine" impression
— the lesson for future sessions applying a Texel result: every
existing test whose FEN is even plausibly close to an edge case for a
changed term needs the actual number worked out, not just a
directional sanity check.

**Fix — same rejection category as D69's knight/bishop-near-king
call:** `ENEMY_KING_DIST_EG`/`OWN_KING_DIST_EG` reverted to the Phase 24
hand-picked default (`1`/`1`) in both `eval/pawns.rs` and
`texel/weights.rs`. This is D63 item 1's first-ever tune, on a
compound/product feature (`king_distance × rank_advancement`) —
inherently sparser and more overfit-prone than a simple per-rank
bucket, the same risk profile that made the knight/bishop-near-king
result untrustworthy. Recomputed `test_passed_pawn_bonus` by hand with
the reverted value: `77 - 17 - 28 = 32 > 0` — passes, and matches the
term's pre-Phase-25 (already-known-passing) behavior exactly, so this
isn't a new risk, it's a restoration.

**Updated watch-item list (supersedes D69's):** `rook_on_seventh` MG
sign, `pawn_storm_bonus` non-monotonicity, `bishop_pair`/
`battery_bishop_queen` large swings — unchanged from D69 — **plus**
`enemy_king_dist_eg`/`own_king_dist_eg` now also held at hand-picked
defaults alongside `knight_near_own_king`/`bishop_near_own_king`, for
the same reason. All four "held back" terms are first-tune, sparse/
compound features — a reasonable pattern to watch for specifically
next time: first-time tunes of compound or narrow-condition features
deserve extra scrutiny before shipping, not just a sign check.

## D71 — Phase 23.4 Step 2/3: Bucket Count Corrected Empirically; Aggregator Built (Session 84)

**D67's "420 buckets" estimate was wrong — corrected here with real
data.** That estimate assumed rook files and knight files were drawn
from disjoint pools (rooks "use up" 2 of 8 files, knights pick from
the remaining 6), i.e. that a *file*, once touched by a rook, couldn't
also host a knight. False: only *squares* are exclusive in Pet
Dragon's setup, not files — a rook and a knight (or both rooks) can
share a file across its two ranks. First real data run (Session 84,
12,000 games, `seed_start=100000`, `15×800`) found **1,054 distinct
(rook_files, knight_files) buckets already hit in just 12,000 games**,
2.5x the wrong estimate, with no sign of plateauing yet. The
file-based-bucketing *reasoning* itself (D67) still holds — file
placement is still the right axis for open-line/outpost dynamics —
only the count was wrong. No design change needed, just corrected
expectations: this key space is meaningfully sparser than planned, so
useful table entries will take longer to accumulate than D67 assumed.

**Consequence, checked directly, not assumed:** with the same 12,000
games, 8,130 distinct (bucket, root-move) pairs exist, and **zero**
clear D67's 30-sample threshold (max observed: 16). Expected given the
corrected bucket count, not a bug — flagged to Gokul before building
the aggregator so the first real run's near-empty output table isn't
mistaken for something broken.

**Aggregator built** (D67 step 3): `src/bin/aggregate_opening_stats.rs`
+ `.github/workflows/aggregate_opening_stats.yml`, following the same
pattern as `texel_tune.rs`/`.yml` — accepts one or more opening-stats
data files (local paths or URLs, same `data_paths`/`data_urls` dual
input style), groups by `(sorted rook_files, sorted knight_files)`,
then by root move within each bucket, and for each bucket keeps only
the single best-win-rate move that clears the 30-sample threshold
(matching D67's usage design — root-only move-ordering bias needs one
favored move per bucket, not a full ranking). Buckets with no
qualifying move are omitted, not zero-filled — an absent bucket and a
"we checked and nothing stood out" bucket need to stay distinguishable
so the ordering hook (D67 step 5) can tell "no data" from "data says
no edge here." Outputs `src/opening_stats.rs`: a static, sorted-by-key
array (12-bit packed key: 3 bits each for rook_file_0, rook_file_1,
knight_file_0, knight_file_1) with a `lookup(key) -> Option<&Entry>`
binary search — chosen over a `phf` compile-time map to avoid adding a
new dependency with unconfirmed WASM compatibility, matching this
project's existing zero-added-dependency pattern for generated tables.

**Status:** aggregator built and ready, not yet run against real data
(nothing clears the threshold yet with only 12,000 games — running it
now would just generate an empty `src/opening_stats.rs`, technically
correct but not worth committing). Next: Gokul runs more
`selfplay.yml` batches with fresh `seed_start` values, concatenating
each new `opening_data_combined.txt` with prior ones (12,000 games
already in hand from `seed100000-15x800`) until a meaningful number of
(bucket, move) pairs clear 30 samples, then run the aggregator. No
fixed target game count set — this is genuinely open-ended
accumulation, same as the NNUE self-play data's growth pattern; check
back periodically rather than committing to one large number now.

## D72 — Phase 23.4 Steps 2 (cont.)/5: Real Aggregator Run + ordering.rs Wired, One Panic Bug Caught Pre-Ship (Session 84)

**Accumulated a second data batch and ran the aggregator for real.**
`seed120000-15×1200` (18,000 games) added to the existing
`seed100000-15×800` (12,000 games) — 30,000 games total, no seed
overlap, no data loss. Bucket count barely grew (1,054 → 1,068,
effectively plateaued — D71's corrected estimate holds up), but
growth in *qualifying* (bucket, move) pairs was far slower than bucket
growth: 2.5x the games produced only 2 pairs clearing the 30-sample
threshold (up from 0). The real sparsity driver isn't the bucket count
— it's that each bucket splits across many plausible root moves, so
per-(bucket, move) sample depth grows much slower than per-bucket
depth. Getting broad table coverage will need on the order of hundreds
of thousands of games, not tens of thousands — flagged to Gokul, who
chose to run the aggregator now on the thin real result and iterate
later rather than wait.

Ran `aggregate_opening_stats.yml` against both batches (`data_run_id`
of both `selfplay.yml` runs). Output — 2 entries, both mapping to the
same move:
```
bucket (rook a,d / knight b,h) -> a2a7, win_rate 0.9677, n=31
bucket (rook a,g / knight b,h) -> a2a7, win_rate 0.9000, n=30
```
Internally consistent, not noise-shaped: both qualifying buckets have
a rook on the a-file, which is exactly the precondition for `a2a7`
(an open-file rook lift) to be legal at all. First real evidence the
full pipeline — parse, bucket, tally, threshold, pack, binary search —
is correct end to end, not just structurally plausible.

**Wired into `search/ordering.rs` (D67 step 5).** Additive
`OPENING_STATS_BONUS = 150_000` on top of a move's normal score —
below `COUNTERMOVE_SCORE` (200,000) so a real countermove signal still
wins ties, and negligible next to `TT_MOVE_SCORE`/
`WINNING_CAPTURE_BASE` (millions), so it can only meaningfully move an
otherwise-ordinary quiet move, never override a real tactical signal —
matches D67's "nudge, don't force" usage design exactly.

**Gate: `ply == 0 && pos.fullmove_number == 1 && side_to_move ==
White`, both conditions required, not just one.** `ply == 0` alone
would fire on *every* `go` command regardless of how far into a game
the position already is (`score_moves` runs at every node, and `ply`
is search depth from wherever `go` was called, not moves-since-game-
start) — the bucket key is defined by the game's ORIGINAL rook/knight
files, so applying it to a position whose pieces have already moved
would silently apply the wrong signal, or worse, could coincidentally
collide with an unrelated real bucket. The `fullmove_number ==
1 && White to move` check is what actually pins this to "zero moves,
real or hypothetical, played since the random setup."

**Caught a real panic bug before shipping, not after.** The
file-extraction helper (`sorted_files`, mirrors `selfplay.rs`'s helper
of the same name) originally hard-panicked on anything other than
exactly 2 bits set — correct for `selfplay.rs`, which only ever sees
`Position::generate_with_seed`'s guaranteed-fresh output (exactly 2
rooks/2 knights by construction). Wrong for `ordering.rs`: it sees
arbitrary UCI-supplied positions, and an *existing* test FEN in the
same file (`test_capture_before_quiet`,
`"4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1"` — zero rooks, zero knights)
matches the gate's `fullmove_number==1, White to move` condition
exactly. Checked this against the actual test suite before shipping
rather than assuming the gate was narrow enough — it wasn't, and this
would have crashed the engine on any real analysis/custom position
with fullmove 1 and White to move, not just this one test. Fixed:
`sorted_files` returns `Option<[u8; 2]>`, `None` on a mismatch, treated
identically to a genuine table miss (graceful fallthrough to normal
scoring) rather than a program-invariant violation. Re-verified every
existing `ordering.rs` test by hand against the fixed version — none
change behavior (`Position::start_pos()`'s standard-chess rook/knight
files pack to key 462, not in the 2-entry table, so every test using
`start_pos()` at ply 0 gets a harmless miss; `test_capture_before_quiet`
now gets `None` instead of panicking, matching its pre-existing,
already-passing expectation).

**Known, accepted duplication:** `sorted_files` now exists in three
places (`selfplay.rs`, `aggregate_opening_stats.rs`'s `parse_file_pair`,
`ordering.rs`) with slightly different panic-vs-Option behavior for
good reason in each (explained above). Not deduplicated into a shared
lib function this session — would mean reopening the already-committed,
CI-green `selfplay.rs` for a small refactor, not worth the churn/risk
right now. Flagged here so a future session doesn't "fix" the
duplication without first re-reading why the panic behavior
legitimately differs between call sites.

**Status:** step 5 confirmed committed and CI-green (Gokul confirmed,
same session). Step 6 done —
`test_opening_stats_bias_applies_to_known_bucket` added to
`search/ordering.rs`, hand-traced against table entry 207 before
writing the test (confirmed `a2a7` is always a rook-takes-rook capture
in that bucket, and that it's provably the single highest-scored move
in the hand-constructed position — every other White piece is boxed in
or has nothing to capture). Skips gracefully rather than failing if a
future table regeneration changes or drops entry 207. **All of D67's
6-step build order is now done or ongoing-by-design** — step 4
(accumulation) continues in the background whenever convenient, no
fixed target; the table grows automatically on future aggregator
re-runs as more data clears the threshold, no further code changes
needed for that.

## D73 — Phase 24 Item 4: Threats Term Implemented (Session 84)

Implemented D68's 4th HCE gap candidate. Scoped down from D68's
original 4-part sketch (hanging pieces, weak queen protection, minor/
rook threat, restricted pieces) to **two** sub-terms, decided during
implementation rather than assumed from the design note:

1. **`UNDEFENDED_PENALTY[kind]`** — one of our pieces (knight/bishop/
   rook/queen; pawns and king excluded, see `threats.rs`'s module doc)
   is attacked by more enemy pieces than it has defenders. This
   generalizes and subsumes both "hanging" (0 defenders) and "weakly
   defended" (defended, but still outnumbered) into one continuous
   signal, rather than two separate constants for the same underlying
   pattern — simpler than D68's original 4-term sketch without losing
   anything Stockfish's version captures for this scope.
2. **`THREAT_BY_MINOR_BONUS`** — one of our knights/bishops currently
   attacks an enemy rook or queen. A live tactical threat, distinct
   from `mobility.rs`'s plain square-count.

**"Restricted piece" (Stockfish's low-safe-mobility penalty)
deliberately dropped**, not merely deferred — it's the most direct
overlap risk with `mobility.rs`'s existing per-count bonus table,
which already implicitly scores a piece with few safe squares lower
than one with many. Included would have meant scoring the same
underlying signal from two angles; dropped rather than risk it, per
D68's explicit double-counting warning.

**Double-counting checked against every existing term before writing
code, not after** (D68's requirement, same discipline Phase 24 items
1-3 used): `mobility.rs` counts attacked squares regardless of
occupant; `king_safety.rs`'s `ATTACKER_WEIGHT` only counts attackers
in the king zone specifically. Neither measures "is this specific
piece, anywhere on the board, under-defended" or "is this minor
threatening a bigger piece" — both are genuinely new signal.

**Reuses existing attack primitives** (`knight_attacks`,
`bishop_attacks`, `rook_attacks`, `pawn_attacks`, `king_attacks` — the
same low-level functions `mobility.rs`/`king_safety.rs` already use)
via the standard "reverse attack generation" trick for
attacker/defender counting (a queen's contribution falls out for free
from the bishop/rook checks — it attacks a given square via either a
diagonal or straight ray, never both, so no double-count risk
combining the two checks). No new move-generation machinery, per D68.

**Full Texel-chain wiring in the same submission** (Phase 24 item 1's
lesson, applied directly rather than discovered via failed CI this
time) — touched all of: `eval/threats.rs` (new), `eval/mod.rs`
(dispatch), `texel/features.rs` (extraction, mirrors `threats_for_color`
exactly per this repo's established dual-sync duplication pattern —
not a shared function, matching every other term), `texel/predict.rs`
+ `predict_f64.rs` (forward pass and gradient), `texel/weights.rs` +
`weights_f64.rs` (struct fields, defaults, `PARAM_COUNT`, flatten/
unflatten round-trip — including the round-trip test's exhaustive
per-field assertion list, not just the mechanical plumbing), and two
files not touched by any Phase 24 item before this one:
`src/bin/texel_tune.rs`'s output writer and `src/bin/texel_diag.rs`'s
parser, both of which read/write tuned weights via a fixed positional
`s(mg,eg)` sequence — found this dependency by tracing where the
existing `EXPECTED_PAIR_COUNT` constant came from rather than assuming
`weights.rs`/`weights_f64.rs` were the whole chain.

**Hand-verified against existing tests before considering this done**
(the exact discipline D69/D70 established this session, applied
proactively rather than reactively this time): confirmed
`eval/mod.rs`'s existing tests are unaffected — the new term is
provably 0 at both the standard chess start and Pet Dragon's mirrored
start (fully symmetric, no piece attacks anything across the empty
middle at move 0 in either), and small/bounded everywhere else,
nowhere near any existing test's threshold.

**Weights: hand-picked starting values, not yet Texel-tuned** — same
status Phase 8's original Ethereal-derived terms had before Phase 14,
and the status Phase 24 items 1-3 had before Phase 25. Whenever the
next natural Texel re-tune happens (triggered by the same "meaningful
data increase" condition as D66/D69's — no fixed schedule), this term
gets picked up automatically since the Texel-chain wiring is already
in place; no separate follow-up task needed to make that happen.

**Status:** implemented, own test module in `threats.rs` (5 tests:
symmetric-start, undefended-penalized, defended-not-penalized,
fork-bonus, 1000-seed no-panic, bounded), not yet confirmed committed.

## D74 — CI Caught a Real Self-Consistency Bug in D73: Side-to-Move Convention Fixed (Session 84)

Gokul ran CI after committing D73. `test_predict_matches_evaluate_after_moves`
failed: `predict=73 evaluate=-118` mid-game at seed 0 — `predict()` and
`evaluate()` must be bit-for-bit identical (D35's load-bearing
self-consistency guarantee for the entire Texel pipeline); a 191cp
mismatch is a real bug, not noise.

**Root cause, traced exactly:** every `evaluate_*` function in
`eval/*.rs` computes `us = pos.side_to_move; them = us.flip();` and
scores from `us`'s perspective — confirmed directly in
`evaluate_material` and `evaluate_open_lines` before concluding this,
not assumed from memory (memory was wrong here — see below).
`eval/threats.rs`'s `evaluate_threats` hardcoded
`Color::White`/`Color::Black` instead. This happens to be correct at
the game's start (White to move by default, so hardcoded White
coincides with side-to-move) and silently wrong the moment Black is to
move — exactly matching "works at move 0, breaks mid-game."
`texel/features.rs`'s threats extraction was never affected; it
already correctly used the caller-supplied `us`/`them`.

**Process gap, named plainly:** D73 claimed the double-counting check
was done against every existing term "before writing code, not
after" — true for the double-counting question, but the side-to-move
convention itself was asserted from memory rather than verified
against a sibling file at write time. Checked and confirmed after the
fact this session for `evaluate_material`/`evaluate_open_lines`
specifically, which is what should have happened before writing
`evaluate_threats` in the first place. Also worth naming: none of
D73's own hand-written tests in `threats.rs` caught this, because
every one of them happened to construct a White-to-move position —
a real gap in that test suite's design, not just an implementation
bug.

**Fix:** `evaluate_threats` now derives `us`/`them` from
`pos.side_to_move`, matching every sibling function exactly. Added
`test_evaluate_threats_flips_sign_with_side_to_move` — same board,
only the side-to-move FEN flag differs, asserts the two results are
exact negatives of each other. Closes the actual coverage gap, not
just the bug.

**Status:** fix applied, not yet confirmed committed/CI-green as of
this entry.


## D75 — Null-Move King-Exposure Guard Implemented as a Runtime UCI Option, Not a Compile-Time Flag (Session 85)

**Decision**: Implemented ROADMAP Phase 26 item 1 (null-move
king-exposure guard) as a new `SearchInfo::null_move_king_guard: bool`
field, default `false`, exposed as a UCI option `NullMoveKingGuard`
(`type check default false`) — plumbed through `EngineState` →
`cmd_uci`/`cmd_setoption`/`cmd_go` in exactly the same shape as the
existing `Contempt` option (persistent session setting, threaded into
both `main_info` and every helper thread's `h_info` in `cmd_go`).

**The guard itself**: a new `king_safe_square_count(pos, color) -> u32`
helper in `alpha_beta.rs`, next to the existing `has_non_pawn_material`
zugzwang guard. Counts squares in the king's ring (`king_attacks`) that
are neither occupied by the king's own side nor attacked by the enemy
(reusing `Position::is_attacked`, already used for check detection).
When `null_move_king_guard` is `false` (default), this is never called —
`king_safe_squares` is `None` and `can_null_move`'s extra condition
short-circuits via `Option::map_or(true, ...)`, so default engine
behavior is byte-identical to before this option existed, same
"strictly additive" contract D48's regression gate already established
for `Contempt`/`Skill Level`. When `true`: ≤1 safe square skips
null-move for that node entirely; ≤2 reduces the adaptive reduction `r`
by 1, floored at 1 (never fully cancels the reduction to 0, which would
make the null-move search redundant with a normal search at the same
effective depth).

**Why a runtime UCI option, not a Cargo feature flag or a hardcoded
threshold**: ROADMAP Phase 26 explicitly flags this item as "real and
specific, but unproven — would need its own isolated SPRT-style test
(fixed games, same time control, with/without the guard) before
trusting it, not a blind add." D48 already built exactly the
infrastructure this needs: `uci_match_runner.rs` accepts independent
`engine_a_uci_options`/`engine_b_uci_options` strings, sent as
`setoption` lines once per engine before the match. A runtime option
means the SAME compiled binary serves as both sides of the A/B test —
Engine A with `setoption name NullMoveKingGuard value true`, Engine B
with it left at default `false` — via one `uci_match_runner.yml` manual
dispatch, no second build variant, no feature-flag matrix in `build.yml`
to maintain. A Cargo feature flag would have required building and
distributing two separate binaries just to compare them, for a change
whose entire premise is "unproven, needs isolated testing" — the
runtime-option path gets that isolated test for free from existing CI
plumbing.

**Why the thresholds are ≤1 (skip) / ≤2 (reduce), not something else**:
Chosen as a reasonable starting point consistent with the roadmap's own
framing ("more conservative... or disable... when the king has few safe
squares") — 8 is the max ring size for a non-edge king, so ≤2 is a
meaningfully constrained king (a quarter or less of maximum mobility)
while ≤1 is a near-worst-case (the king is boxed in with at most one
escape). **Not tuned, not validated against real games** — this is
exactly the kind of specific-but-unproven number Phase 26 flagged as
needing SPRT-style verification before being trusted. If the eventual
A/B test shows no effect or a negative one, the fix is either
re-threshold or revert `null_move_king_guard`'s default posture (it
already defaults off, so "revert" here just means never flipping the
option on for real games) — not a structural rework, since the whole
mechanism sits behind one already-off-by-default flag.

**Rejected alternative — folding this into the existing zugzwang guard's
condition unconditionally (no option at all)**: rejected for the same
reason a feature flag was rejected — an SPRT-unverified change to a core
pruning heuristic that runs on nearly every node would directly
contradict Phase 26's own stated bar for trusting this idea, and D48's
regression gate (20 games, 35% threshold) is explicitly a coarse smoke
check, not the kind of precision comparison this specific,
sign-uncertain heuristic change needs before being unconditionally live.

**Verification status**: all three touched files
(`src/search/mod.rs`, `src/search/alpha_beta.rs`, `src/main.rs`) have
new unit tests (default-off state, `king_safe_square_count` correctness
on three constructed FENs, `cmd_go` wiring proof mirroring
`test_cmd_go_applies_contempt_to_search`'s pattern) but were reasoned
through by hand against the same sibling-pattern (`Contempt`) rather
than compiled locally — same toolchain wall (sandbox rustc 1.75 vs. an
edition2024 dev-dependency) noted in every prior session's decisions
that touched test code. **Not yet SPRT-style A/B tested** — that's the
explicit next step before this guard is trusted for anything beyond
"compiles and doesn't regress existing tests," per Phase 26's own bar.


## D76 — NullMoveKingGuard First A/B Result: Promising but n=20 Is Not Enough to Promote to Default (Session 85, follow-up)

**Result**: Gokul ran the `uci_match_runner.yml` A/B exactly as
recommended in D75 — Engine A (`main`, `NullMoveKingGuard value true`)
vs. Engine B (`main`, default `false`), 20 games, 100ms/move, seed
21000. **A scored 12-5-3 (67.5%), +127 Elo over B.**

**Why this is NOT being treated as confirmation, despite the size of the
number**: D48's own stated reasoning for why the CI regression gate uses
a 35% floor rather than a tight one applies here directly and even more
strongly — "at n=20 games, sampling noise alone is large enough that a
strong regression (tens of Elo) can still occasionally land near 50%."
The same logic runs in reverse: at n=20, a genuinely neutral change can
land at 67.5% by variance alone. +127 Elo from 20 games has a
wide-enough confidence interval that this result is consistent with
anything from a real, meaningful improvement down to noise. This is
qualitatively different from D48's regression gate (which only needs to
catch clearly broken changes, tolerating false negatives) — promoting a
pruning heuristic to default-on for real games is the opposite
asymmetry: a false positive here means shipping an unproven,
str­ength-affecting change based on noise.

**Decision: do not flip `NullMoveKingGuard`'s default. Recommend a
larger confirmation run** — same workflow, `pre_tuning_ref`/
`post_tuning_ref` both `main`, same `engine_a_uci_options`/
`engine_b_uci_options` split, but `num_games` raised to 100+ (matching
the "precision Elo measurement" scale D48 already distinguishes from
its own lightweight 20-game gate) and a fresh `seed_start` (game set
must differ from seed 21000, or the second run just replays the same 20
games and adds no new information). If a 100+-game run still shows a
clearly positive score (meaningfully above the noise band a null result
would produce), promote the option's default to `true` in a follow-up
diff and note it as a genuine strength gain in ROADMAP.md Phase 26. If
it regresses toward ~50%, the honest conclusion is that the first result
was noise, and the option stays off-by-default indefinitely (or the
≤1/≤2 thresholds get revisited before a second attempt) — either
outcome is a real answer, which is the entire point of not skipping this
step.

**Status**: `NullMoveKingGuard` remains default `false` in `main`. No
code change from this entry — decision-only, pending the larger run.


## D77 — NullMoveKingGuard Confirmation Run: Flat at Scale, First Result Was Noise. Parked Off (Session 85, follow-up)

**Result**: 200-game confirmation run (fresh seed 30000, same option
split as D76's 20-game run) — A (guard on) 74 wins, B (guard off) 72
wins, 54 draws. **50.5%, +3.5 Elo.** Flat, well inside noise for a
200-game sample.

**Conclusion**: D76's own predicted outcome. The 20-game +127 Elo result
was sampling variance, not a real effect. At the current thresholds
(skip null-move at ≤1 safe king-ring square, reduce `r` by 1 at ≤2),
`NullMoveKingGuard` has no measurable strength impact — positive or
negative — in self-play.

**Decision: park it. `NullMoveKingGuard` stays default `false`,
mechanism stays in the code (harmless, zero-cost when off, already
merged and tested).** Not reverting/removing the option — it's a real,
specific idea from external-advice review (Phase 26) that turned out
not to matter *at these thresholds*, which is a legitimate, useful
result, not a failed implementation. Re-litigating with different
thresholds (e.g. only triggering at 0 safe squares, or scaling the
reduction cut by exactly how few squares remain rather than a flat ≤1/≤2
cutoff) is possible future work, but not scheduled — Pet Dragon's
zugzwang guard already covers the cases that matter most, and this
result suggests king-ring exposure isn't an independent enough signal
from what `has_non_pawn_material` and normal search already handle to
be worth further tuning budget right now.

**Phase 26 item 1: closed as "implemented, tested, no measurable effect,
parked off."** Items 2 (deeper perft coverage) and 3 (expanded
correction history) remain open on Phase 26.


## D78 — Phase 26 Item 2: Adversarial Perft Tests Added; Corrected a Wrong Premise in the Original Item Wording (Session 85, cont.)

**What was added**: 4 new hand-verified `tests/perft.rs` tests closing
the specific gap Phase 26 item 2 flagged — existing perft coverage was
either standard-chess positions (external known-correct values apply)
or loose depth-1/2 range checks on random Pet Dragon seeds (no exact
values, since no independent oracle exists for arbitrary random
starts). Neither carried Pet Dragon's two genuinely custom mechanics —
rank-1 double-push/en-passant interaction, and castling-path blocking —
through multiple plies of real make/unmake + zobrist + legality round
tripping; `movegen/pawns.rs` and `movegen/castling.rs` already
unit-test both mechanics, but only as single-ply move-list checks.

- `test_perft_rank1_double_push_en_passant_depth1/2`: reuses the exact
  position from `pawns.rs::test_en_passant_after_rank1_double_push`
  (White pawn recorded-start e1, Black pawn d3). Hand-counted
  perft(1)=5, perft(2)=32, with the branch-by-branch count in the test
  comment. This specifically discriminates a hardcoded-rank en-passant
  bug: if the target were wrongly computed as e3 (rank-2-to-4
  assumption) instead of e2 (the square actually passed through), d3
  couldn't diagonally reach e3 at all, the capture would silently
  vanish, and the total would read 31, not 32.
- `test_perft_castling_blocked_by_intervening_piece_depth1/2`: reuses
  the exact position from
  `castling.rs::test_castling_blocked_by_piece` (King e1, Knight f1,
  Rook h1). Hand-counted perft(1)=16, perft(2)=78 — the depth-2 count
  includes a rook move to h8 that opens a check along the now-clear 8th
  rank, which was the trickiest branch to hand-verify and is called out
  explicitly in the test comment.

**Correction to the original Phase 26 item 2 wording**: the roadmap
entry described the castling case as "blocked by an intervening piece
on an unusual rook file." Re-checked `castling.rs` directly before
writing anything (mandatory read-before-write) — Pet Dragon's king is
hardcoded to e1/e8, and castling rights only exist at all if the rook
also happens to land on its standard a1/h1/a8/h8 square (confirmed both
in the module's own doc comment and in `CastlingRights` detection at
setup). There is no "unusual rook file" case — that premise appears to
be inherited from a Chess960-style assumption already rejected
elsewhere in this project's history. The real adversarial case, and the
one actually worth testing, is a *standard*-square rook with some other
randomly-placed piece sitting in the path between it and the king —
which is what the new tests cover, using the already-existing
`castling.rs` test position rather than a novel one.

**What's intentionally NOT done, and why**: did not attempt to convert
the existing "reasonable range" tests on the 20 random Pet Dragon seeds
(`test_pet_dragon_perft_depth1_reasonable`,
`test_pet_dragon_perft_depth2_reasonable`) into exact-value assertions.
Doing so would require either (a) an independent perft oracle for
arbitrary random Pet Dragon arrangements, which doesn't exist, or (b)
hand-enumerating each of 20 essentially-full-board random positions,
which is not tractable by hand without unacceptable risk of the
hand-count itself being wrong (unlike the two sparse, few-piece
positions above, which were small enough to fully and carefully
enumerate). This is a genuine, structural limitation of testing a
custom variant with no external reference implementation, not something
this session could close — flagging it here rather than silently
leaving it implicit.

**Phase 26 item 2: closed** on the scope that's actually achievable
(deep, hand-verified adversarial-position coverage for both flagged
custom mechanics). The random-seed range-check limitation above is
inherent, not a follow-up task.


## D79 — Bug Fix: Wrong Expected Value in D78's Castling-Blocked Perft Test (Session 85, cont.)

**Cause**: D78's `test_perft_castling_blocked_by_intervening_piece_depth2`
asserted `perft(2) == 78`, based on a hand count that correctly worked
out the Rh8 branch (rook attacks the fully open 8th rank, giving
check, Black's king has exactly 3 legal replies) but wrongly assumed
the Rh7 branch was unaffecting — the hand count didn't consider that a
rook doesn't need to reach the king's actual square/rank to restrict
its mobility, only the ranks the king could move *to*. A rook on h7
attacks along the fully open 7th rank (nothing between h7 and a7),
hitting d7, e7, and f7 — three of Black's five candidate king squares —
without itself giving check (e8 is on rank 8, untouched by a rook on
h7). CI caught it immediately: `cargo test` reported `left: 75, right:
78`.

**Fix**: rebuilt the actual crate standalone (Ubuntu's packaged
`rustc`/`cargo` 1.75, avoiding the known dev-dependency/edition2024
wall by depending on `pet_dragon` as an ordinary path dependency from a
throwaway probe crate — dev-dependencies of a dependency are never
pulled in by a downstream crate, so `criterion` never enters the
build), and ran the file's own already-existing `perft_divide` helper
directly against the exact failing FEN. Output showed `h1h7: 2` (not
5), `h1h8: 3` (matches the original hand count), everything else 5 —
summing to 75. Corrected the test's expected value to 75 and rewrote
the comment to include the Rh7 branch's rank-7 restriction.

**Why this is correct**: verified against the actual engine's own move
generator and legality filter running the real position, not a second
round of hand-guessing — the same class of error (an incomplete manual
branch enumeration) can't repeat itself when the number comes from
executing the code being tested rather than reasoning about it a
second time by hand.

**Process note for future sessions**: this is a useful precedent —
when a hand-verified perft test's expected value is in doubt (either
because CI disagrees, or before ever committing a new one), building
the crate standalone via `apt-get install rustc cargo` (Ubuntu's
packaged 1.75, already known to work for `cargo build --lib`) and
running `perft_divide` from a throwaway path-dependency probe crate is
faster and more reliable than a second hand-enumeration attempt, and
sidesteps the `cargo test`/criterion edition2024 wall entirely since
`cargo build`/`cargo run` never touch a dependency's dev-dependencies.
Worth using this method for the *next* new hand-verified perft test
too, rather than shipping one straight from hand-arithmetic.


## D80 — Phase 26 Item 3a: Non-Pawn-Material Correction History Added, Verified by Standalone Probe Before Shipping (Session 85, cont.)

**What was added**: a second, independent `CorrectionHistory` table
(`SearchInfo::correction_history_nonpawn`) indexed by a new
`pruning::nonpawn_hash(pos)` — zobrist-style hash over every knight,
bishop, rook, and queen (both colors), deliberately excluding pawns
(already the existing table's signal) and kings (too volatile move-to-
move to usefully index a systematic-error signal by). Applied
additively alongside the pawn table in `alpha_beta.rs`: both read
against the same `raw_static_eval` and their corrections combined via
chained `.apply()` calls (mathematically identical to summing
directly), each updated independently at node exit against that same
raw baseline — not cascaded, so neither source's learned correction
feeds into what the other treats as its own error signal. Threaded
through `SearchInfo` → `EngineState` → `wait_for_search`/`cmd_go`
exactly mirroring `correction_history`'s existing plumbing (persists
across moves within a game).

**This is item 3a of 3** — Gokul asked for all three Phase 26 item-3
candidates (non-pawn-material, continuation-based, extension-margin
use of the signal). Building all three as one bundled diff would
undercut the item's own stated methodology ("each addition should be
its own isolated CI-verified diff + SPRT-style validation, not
bundled") — implementing them as sequential, separately-committed
diffs instead. This is the first.

**Verification methodology — followed through on D79's own
recommendation**: rather than trust hand-reasoning about whether the
new update()/apply() call sites are actually reached, built the crate
standalone (`apt-get`-installed rustc/cargo 1.75, sidestepping the
known dev-dependency/edition2024 wall) and exercised the new code from
a throwaway path-dependency probe crate — same method D79 established.
Caught and fixed two real issues before they ever reached CI:

1. The first draft of the search-integration test used the start
   position — its naturally-occurring search-vs-eval error at depth 6
   was small enough that the weighted-average update formula's integer
   division (`(entry*(256-w) + error*w) / 256`) rounded it back to
   exactly 0, which would have made the test flaky/misleading rather
   than a real proof the wiring works. Switched to a constructed
   position with a large, search-only-discoverable error (an
   undefended queen on an open file, opponent to move, no recapture
   possible) — verified via the probe to reliably produce a non-zero
   correction.

2. The first version of that same test position
   ("3r3k/8/8/8/3Q4/8/8/K7 w - - 0 1") crashed the engine entirely —
   see D81 below for the full diagnosis. Not a bug in this diff; an
   illegal input position (the queen on d4 attacks the black king on
   h8 diagonally, which means Black is already in check while it's
   White's move — an unreachable state in real play). Rebuilt the test
   position with the king on g8 instead of h8 (off that diagonal),
   confirmed via the probe to be a fully legal position with no crash
   and the same large, reliably-nonzero correction.

**Tests added**: `pruning.rs` — `nonpawn_hash` basic non-zero checks,
plus two discriminating tests (`test_nonpawn_hash_ignores_pawn_structure`
confirms two positions differing only in pawn structure hash
identically; `test_nonpawn_hash_ignores_king_position` confirms moving
only a king doesn't change the hash). `alpha_beta.rs` —
`test_nonpawn_correction_history_wired_into_search` (the corrected
hanging-queen-position test above) and
`test_nonpawn_and_pawn_corrections_are_independent_sources` (proves the
two tables don't alias/leak into each other via explicit seeded
updates at different hashes).

**Not yet done**: items 3b (continuation-based correction) and 3c
(extension-margin use of the correction signal) — next, as separate
diffs. No SPRT-style A/B test run yet for item 3a itself — needed
before this is trusted as more than "compiles and passes unit tests,"
same bar as D75-D78.


## D81 — Found (Not Fixed): Engine Crashes on Illegal Input FEN Where the Side Not to Move Is Already in Check (Session 85, cont.)

**What was found**: while probe-verifying D80's test position, an
early draft FEN ("3r3k/8/8/8/3Q4/8/8/K7 w - - 0 1") crashed
`alpha_beta` at *any* depth, including depth 1, with `panicked at
src/position/mod.rs:250:14: King must always be on the board` (inside
`Position::king_sq`, called from `Position::in_check`, called from
`quiescence`).

**Root cause, fully traced** (instrumented `make_move` locally in a
scratch build to log every capture — removed before this diff, not
part of anything shipped): the white queen on d4 has an open diagonal
to h8 (d4-e5-f6-g7-h8, nothing in between) — geometrically a completely
normal queen move. Black's king happens to be on h8. Since move
generation doesn't special-case "the target square holds the enemy
king" as ungenerateable, and (this is the actual root cause) **the FEN
itself is illegal** — the side NOT to move (Black) is already in check
while it's White's turn, a game state that can never arise from legal
play, since Black's own prior move could never legally leave Black's
own king in check — the search takes the pseudo-legal "capture" at
face value, removes the king from the board, and the next call to
`in_check` (which needs to locate a king that no longer exists) panics.

**This is a garbage-in-garbage-out input-validation gap, not a search
or move-generation bug reachable through real play.** `Position::from_fen`
doesn't validate that the side not to move isn't in check, and nothing
downstream treats "the opponent's king is on a square I could
geometrically reach" as disallowed — both individually reasonable
simplifications given the invariant "search only ever explores
positions reached via `generate_moves`'s own legal-move filter, which
guarantees this can't happen" holds for any position that actually
originates from `Position::start_pos()`/`generate_with_seed()` and
legal search from there. It only breaks when an illegal FEN is handed
in directly via `position fen ...`.

**Why this still matters enough to record, even though it's not a
live-play bug**: UCI is an external-facing protocol — a GUI bug, a
corrupted saved position, hand-testing (exactly how this was found),
or a future engine feature that constructs FENs programmatically could
all feed the engine something illegal, and the current behavior is a
hard crash (panics the whole process, including mid-game) rather than
a graceful UCI-level error. A crash mid-tournament-game from a bad
`position fen` is a worse failure mode than almost anything else on the
roadmap right now.

**Deliberately NOT fixed in this diff** — out of scope for Phase 26
item 3a, and a real fix deserves its own scoping rather than a rushed
patch under an unrelated diff. Two candidate directions, not yet
evaluated against each other: (a) validate at `from_fen` parse time
that the side not to move isn't in check, rejecting the FEN outright
with a UCI error rather than silently accepting it; (b) defensively
guard move generation/application against ever treating the enemy
king's square as a legal capture target, so even a genuinely malformed
position degrades to "no capture available there" instead of crashing.
(a) matches how most engines actually behave (garbage FEN → reported
error, not a crash, not silently "handled"); (b) is more defensive but
masks bad input rather than surfacing it. **Recommend (a)** as the
likely right call once scoped, but this needs its own session.

**Action for ROADMAP.md**: added as a new item, flagged for attention
but explicitly NOT as urgent as a real search-correctness bug would be
(it cannot occur via legal search from any of `start_pos()`/
`generate_with_seed()`, so it doesn't affect self-play, tournament play
from a legal start, or any existing test in the suite) — worth fixing
before it's ever hit by a real GUI/tool, but not blocking anything else
in flight.


## D82 — Corrected Item 3a to Ship Gated Off, Matching Item 1's Own Established Discipline (Session 85, cont.)

**Problem**: D80 shipped `correction_history_nonpawn` always-on, with no
kill switch — inconsistent with D75's own discipline for item 1
(`NullMoveKingGuard`): an unvalidated, strength-affecting change ships
gated off by default, gets its own isolated SPRT-style A/B via a
runtime UCI option, and only then is a default flip even considered.
Surfaced when scoping how item 3a's own SPRT test would actually run —
there was no toggle to A/B against, only a choice between adding one
now or comparing against a pinned pre-3a commit ref.

**Decision: add the toggle now, retroactively bringing item 3a in line
with item 1's pattern**, rather than use a pinned-ref comparison for
this one test and leave item 3a permanently switch-less. New
`SearchInfo::nonpawn_correction_enabled: bool` (default `false`), UCI
`NonPawnCorrectionHistory` (`type check default false`), threaded
through `EngineState`/`cmd_setoption`/`cmd_go` exactly mirroring
`NullMoveKingGuard`. Both the `.apply()` and the `.update()` call sites
in `alpha_beta.rs` are now gated on this flag — when off, neither is
reached, and `correction_history_nonpawn` stays completely untouched
(verified directly: `off -> corr = 0`, `on -> corr = 21` against the
same D80 hanging-queen test position, via the same probe-crate method).

**Why this is the right general pattern going forward, not just a
one-off fix**: every Phase 26 item so far that affects search/eval
strength (items 1, 3a, and presumably 3b/3c to come) benefits from a
runtime kill switch for the same two reasons D75 established — it lets
`uci_match_runner.yml` A/B the exact same binary with one `setoption`
difference (no rebuild, no feature-flag matrix), and it means "ships
but turns out not to help" degrades to "leave the default off
forever" rather than requiring a revert diff. Recording this as the
default expectation for future Phase 26-style additions, not just
something to remember case by case.

**Tests updated/added**: `alpha_beta.rs` — the D80 wiring test now
explicitly sets `nonpawn_correction_enabled = true` (it would otherwise
silently test nothing, since the table is untouched by default);
new `test_nonpawn_correction_defaults_to_false` and
`test_nonpawn_correction_off_leaves_table_untouched`. `main.rs` — new
`NonPawnCorrectionHistory` option trio mirroring
`NullMoveKingGuard`'s three tests exactly (default-false, parses
true/false, `cmd_go` wiring reaches the actual search thread's
`SearchInfo`).

**Verification**: full standalone build + probe run before shipping,
same method as D79/D80 — confirmed the default-off case leaves the
table at exactly 0 and the explicit-on case reproduces D80's original
result (`corr = 21`) on the identical test position.


## D83 — Phase 26 Item 4 Fixed: from_fen Now Rejects Illegal Positions (Session 85, cont.)

**Fix**: `Position::from_fen` now validates, before returning `Ok`, that
(a) each color has exactly one king, and (b) the side NOT to move is
not in check. Both return a `FenError` instead of building a `Position`
that would later panic. `FenError::KingNotFound(Color)` already existed
as a declared variant but had never actually been constructed anywhere
— dead validation, now wired up. New `FenError::OpponentInCheck(Color)`
covers the actual D81 crash class.

**Why this is the right fix** (per D81's own two candidate directions):
chose (a) — reject at parse time — over (b) — defensively guard move
generation against ever capturing a king. Parse-time rejection matches
how engines normally handle malformed input (a UCI-level error, not a
crash, not silent acceptance) and surfaces the actual problem (bad
input) rather than papering over it inside search.

**Verified two ways before shipping, not just by construction:**
1. Directly confirmed both original D81 crash FENs now return
   `Err(OpponentInCheck(Black))` instead of parsing successfully and
   later panicking — via the same standalone probe-crate method as
   D79/D80/D82.
2. **Swept the entire existing test suite for collateral damage.**
   This validation applies to every `from_fen` call in the codebase,
   not just the UCI-facing path — extracted all 98 FEN-shaped string
   literals from `src/` and `tests/` via grep and ran every one through
   the new validation. Found 5 pre-existing test FENs that were
   themselves illegal positions of the exact same class D81 flagged
   (opponent already in check when not to move) — the same mistake I'd
   made myself in D80's own first draft, now caught systematically
   instead of one at a time:
   - `movegen/pawns.rs::test_promotion` and
     `make_unmake.rs::test_make_unmake_promotion_position` (same FEN,
     two files): black king on d8, sitting on the promoting pawn's own
     capture diagonal — moved to h8.
   - `eval/open_lines.rs`, three tests (open file, 7th rank, connected
     rooks): each had the black king sitting directly on the open
     file/rank being tested, which is exactly what put it in check —
     moved off that file/rank in each case, preserving what each test
     actually measures.
   - `nnue/inference.rs::test_evaluate_nnue_clamp_enforced`: a 16-queen
     maximal-material-imbalance stress position where literally every
     square on ranks 1-6 sits under an unobstructed file attack from
     the queen wall directly above — the black king couldn't be placed
     anywhere without a blocker. Relocated to d1 (the only file/
     diagonal combination that exits the board before reaching rank
     7/8) plus one blocking pawn on d2 for the remaining file threat.
   All 5 verified fixed via the same probe sweep — 98/98 FENs now parse
   successfully (the only other 3 "rejections" in the sweep were
   grep-regex artifacts: a UCI command-line string and two lichess-JSON
   test fixtures, never actually passed to `from_fen` directly —
   confirmed by inspecting each call site).

**New tests** in `position/mod.rs`: side-to-move-in-check still
accepted (this is normal and must not be rejected); the exact D81 FEN
now rejected with the correct error variant; a missing-king FEN
rejected; a duplicate-king FEN rejected.

**Phase 26 item 4: closed.**


## D84 — NonPawnCorrectionHistory First A/B Result: Promising but n=20 Is Not Enough to Promote to Default (Session 85, cont.)

**Result**: `uci_match_runner` A/B — Engine A (`main`,
`NonPawnCorrectionHistory value true`) vs. Engine B (`main`, default
`false`), 20 games, 100ms/move, seed 40000. **A scored 10-6-4 (60.0%),
+70.4 Elo over B.**

**Same reasoning as D76 (item 1's first look), applied again**: at
n=20, sampling noise alone is large enough that a genuinely neutral
change can land at 60% by variance, just as a real +70 Elo change could
easily land near 50% in a different 20-game sample. This result is
consistent with anything from a real, modest improvement down to pure
noise — not distinguishable at this sample size. D77 already
demonstrated this isn't hypothetical for this exact project: item 1's
20-game +127 Elo first look fully evaporated to +3.5 Elo at 200 games.

**Decision: do not flip `NonPawnCorrectionHistory`'s default. Recommend
a 100+-game confirmation run**, fresh seed (not 40000), same option
split, before trusting this one way or the other — identical protocol
to D76→D77. `NonPawnCorrectionHistory` remains default `false` in
`main`.

**Status**: no code change. Decision-only, pending the larger run.


## D85 — NonPawnCorrectionHistory Confirmation Run: No Benefit, Slight Negative Lean. Parked Off (Session 85, cont.)

**Result**: 200-game confirmation run (fresh seed, same option split as
D84) — A (non-pawn correction on) 62 wins, B (off) 73 wins, 65 draws.
**47.2%, -19.1 Elo.**

**Conclusion**: D84's own predicted outcome — the 20-game +70 Elo first
look was noise, same pattern as item 1 (D76→D77). Worth being precise
about what this result actually says: -19.1 Elo at 200 games is not
strongly distinguishable from zero (well within the noise band D48's
own reasoning already established for samples this size), so this is
NOT good evidence the feature actively hurts. But it is clearly not
evidence it helps either — the point estimate leans negative rather
than flat, which is a slightly different shape than item 1's dead-flat
+3.5 Elo result, though not different enough to draw a stronger
conclusion from a single 200-game run.

**Decision: park it, same treatment as item 1.**
`NonPawnCorrectionHistory` stays default `false`. Not worth a third,
larger confirmation run right now — the pattern across both item 1 and
item 3a this session (promising 20-game look → flat-to-negative
200-game result) is consistent enough with a project already at a
skill level where the *current, simplest form* of these ideas doesn't
move the needle, rather than either idea being wrong in principle. A
plausible explanation specific to 3a: non-pawn material placement may
already be well-captured by existing HCE terms (piece-square tables,
mobility, threats — Phase 24's own additions), leaving less genuine
error for a correction table indexed on piece placement to catch,
unlike pawn structure (which HCE handles more coarsely and where the
original correction table has an established, real effect).

**Phase 26 item 3a: closed as "implemented, tested, no measurable
benefit (mild negative lean, not statistically distinct from zero),
parked off."** Option stays in the code (harmless, off by default).
Given this and item 1's identical outcome, **items 3b and 3c
(continuation-based correction; extension-margin use of the signal)
are now a genuinely open question** rather than an assumed next step —
worth deciding explicitly whether to keep pursuing this family of
ideas or move attention elsewhere (Phase 16 NNUE is still the
larger-scale option on the table), rather than continuing on momentum
alone.


## D86 — Phase 26 Item 3b: Continuation-Based Correction History Added, Shipped Gated-Off From the Start (Session 85, cont.)

**What was added**: a third, independent `CorrectionHistory` table
(`SearchInfo::correction_history_continuation`) indexed by a new
`pruning::continuation_hash(pos, prev_move)` — a hash of the last two
real moves' squares (from/to for each), deliberately position- and
piece-INDEPENDENT (unlike move ordering's own `cont_hist`, which is
piece-conditioned and exists to score individual candidate moves, not
to correct a node's static eval). Gated behind UCI
`ContinuationCorrectionHistory` (default `false`) from the start this
time — no retrofit needed, unlike item 3a (D82).

**Design choice — no new parameter threading**: the roadmap's "recent
move pairs" phrasing implies needing the last TWO moves, but
`alpha_beta`'s existing `prev_move` parameter only carries one ply of
history, and `alpha_beta_with_excluded` already has 8 internal
recursive call sites plus every external caller (`main.rs`, every test)
depending on the current public signature. Rather than add a second
parameter and touch all of that, `continuation_hash` reads the
second-to-last move directly from `pos.history` (`Vec<HistoryEntry>`,
already maintained by `make_move_with_history`/`unmake_move_with_history`
for undo purposes) — `pos.history.last().mv` is always exactly the
existing `prev_move` parameter whenever it's non-null (every call site
passing a real `prev_move` does so immediately after pushing that same
move; null-move pruning's synthetic side-flip is never pushed to
`pos.history` and always passes `Move::NULL`, so it can't be confused
for real history). Net result: one new pure function, zero signature
changes anywhere.

**Why square-only, not piece-conditioned**: keeping `continuation_hash`
a pure function of `pos.history`'s move squares (no board lookups)
means it works identically regardless of what's currently on those
squares — simpler, cheaper, and avoids the piece-tracking complexity a
richer signal would need (the piece that made the *older* of the two
moves may since have moved again or been captured, making "what piece
made that move" unreliable to reconstruct from the current board
alone, unlike the *newer* move where the piece is still findable via
`pos.piece_on`). This is a real scope simplification versus mirroring
`cont_hist`'s full piece-conditioning, made explicit rather than
silently narrowed.

**Verified via the standard probe method** (D79 onward): confirmed
`continuation_hash` returns `None` at the root (no history) and `Some`
once two real moves exist; confirmed the wiring test's exact scenario
(hanging-queen position with two synthetically-attached history
entries, matching real `make_move_with_history` output exactly since
the hash never reads board state) produces a non-zero table entry
after search, before shipping the test.

**Tests added**: `pruning.rs` — 5 tests on `continuation_hash` itself
(None at root, None with only one move, Some with two, different pairs
hash differently, hash depends only on move-pair squares not board
state — the last one deliberately documents the square-only design
choice so a future change to make it piece-aware doesn't silently
drift). `alpha_beta.rs` / `main.rs` — the same default/gating/wiring/
independence test shapes as items 1 and 3a.

**Not yet done**: no SPRT-style A/B test yet — needed before this is
trusted as more than "compiles and passes unit tests," same bar as
every other Phase 26 item. Item 3c (extension-margin use of the
correction signal) not started.


## D87 — CI Caught 3 Real Bugs the Probe Method Structurally Could Not; Found a Way to Run the Real Test Suite Locally (Session 85, cont.)

**Cause**: item 3b's `pruning.rs`/`alpha_beta.rs` shipped with 3 real
bugs, all inside `#[cfg(test)]` code:
1. `MoveKind::DoublePawnPush` — not a real variant (correct name is
   `MoveKind::DoublePush`), used 10 times across 5 new tests in
   `pruning.rs`.
2. `alpha_beta.rs`'s test module used `MoveKind::Quiet` without
   `MoveKind` ever being imported at the top of the file.
3. A genuine test-logic bug in
   `test_continuation_hash_matches_history_regardless_of_current_position`:
   pushed only one of the two required synthetic `HistoryEntry` values
   onto the comparison position, violating `continuation_hash`'s own
   documented contract that `prev_move` must equal
   `pos.history.last().mv` — the test's own setup was inconsistent with
   the function it was testing.

**Why the probe method (established D79, used continuously since)
could not have caught any of these**: `cargo build --lib` — the basis
of every "verified via probe" claim this session — does not compile
`#[cfg(test)]` code at all, by design. The probe crate exercises real
library code through its public API, which proves the actual
production logic works, but it can never touch code that only exists
inside a test module. Every one of these 3 bugs was purely inside test
code, so probe verification was structurally blind to all of them
despite genuinely proving the production logic correct in every prior
diff this session.

**Fix, and a real methodology upgrade**: fixed all 3 bugs directly.
More importantly, found a way to actually compile and run the real
test suite locally, closing this gap going forward — `cargo test`
itself still hits the known wall (Cargo resolves the *entire*
dependency graph, including dev-dependencies, before building
anything, so `criterion`'s edition2024 requirement blocks even
`cargo test --lib`). Worked around it by inspecting `Cargo.toml`'s own
`[profile.bench-tests]` comment (`"required because cargo test needs
unwinding to catch test panics"`) — a `cargo build --lib` (plain debug
profile, not `--release`) produces dependency `.rlib` files with the
default `panic=unwind` strategy, since only `[profile.release]`
overrides it to `abort`. Those debug-profile rlibs can then be handed
directly to `rustc --test` against `src/lib.rs` (and separately against
each `tests/*.rs` file), with explicit `--extern` flags mirroring
Cargo's own invocation — this **entirely bypasses Cargo's dependency
resolution**, so `criterion` and its `edition2024` requirement are
never touched, since we're never asking Cargo to resolve the dev-
dependency graph at all. The result is a real, runnable test binary —
not a hand-written replica of test logic, the actual compiled artifact
`cargo test` would produce.

Ran this against the full lib test suite and all 4 `tests/*.rs`
integration files: **464 lib tests + 19 (`make_unmake.rs`) + 22
(`perft.rs`) + 21 (`setup.rs`) — all passing** after the 3 fixes
(`node_count.rs`'s 5 tests are `#[ignore]`d benchmarks, expected).

**Process change for all future sessions**: this replaces, not
supplements, the D79-established probe method as the standard local
verification step before shipping any diff that includes new
`#[cfg(test)]` code (which is nearly every diff). The probe method
remains useful for exploring/confirming specific behavior interactively,
but "verified via probe" alone is no longer sufficient grounds to claim
a diff is tested — the direct-`rustc --test` method is now the bar,
since it is the actual test suite, not an approximation of it.


## D88 — ContinuationCorrectionHistory: Consistent Negative Result Across Both Sample Sizes. Parked Off (Session 85, cont.)

**Results**: 20-game first look (seed 48000) — A (continuation
correction on) 7-9-4, 45.0%, -34.9 Elo. 200-game confirmation (seed
50000) — 66-79-55, 46.8%, -22.6 Elo.

**Different shape than items 1 and 3a**: those both showed a
promising positive result at n=20 that evaporated (or went slightly
negative) at n=200 — a classic small-sample noise pattern. This one is
different: **both samples agree**, landing within a few Elo of each
other and on the same (negative) side. That's more informative than
either result alone — still not enough to call this a confirmed,
significant regression (the 200-game point estimate is still within
a plausible noise band for this sample size), but there's no ambiguity
here the way there was for items 1/3a: nothing about this result
suggests "wait for a bigger sample, it might flip."

**Decision: park it, same treatment as items 1 and 3a.**
`ContinuationCorrectionHistory` stays default `false`. Not running a
larger confirmation — the consistent-direction result across both
samples already answers the question this session's SPRT protocol was
built to answer, more cleanly than either prior item did.

**Phase 26 item 3b: closed as "implemented, tested, consistent mild
negative signal across two independent samples, parked off."**

**Pattern worth naming directly, now three items deep**: items 1, 3a,
and 3b — every strength-affecting idea from the Session 83
external-advice review that's actually been SPRT-tested this session —
have all landed at flat-to-negative. Item 3c (extension margins from
the correction signal) is the last untested idea from that same
review. Worth an explicit conversation before building it: is there
a specific reason to expect 3c to behave differently (it's a genuinely
different mechanism — search-depth allocation, not eval correction),
or is this a natural point to stop pushing on this particular idea
source and redirect toward something with a different profile (Phase
16 NNUE, or a fresh look at what's actually driving Pet Dragon's
current playing strength)?


## D89 — Phase 26 Item 3c: Correction-Scaled Singular Extension Margin, Verified via the D87 Method Before Shipping (Session 85, cont.)

**What was added**: the singular-extension margin in `alpha_beta.rs`
(base value 2, unconditional since Phase 13.3/D59) can now be reduced
by up to 1 when `info.correction_extension_enabled` is true (UCI
`CorrectionExtension`, default `false`) and the current position's
*base pawn-hash* correction-history magnitude exceeds 300 — a smaller
margin raises `singular_beta` closer to `tt_score`, making the
verification search more likely to confirm singularity and extend.
Rationale: search depth compensates for eval unreliability, so be more
willing to extend the TT move specifically at position types where
eval has a real history of needing correction. Genuinely different
mechanism than items 3a/3b — this affects search-depth allocation via
the existing singular-extension machinery, not eval correction itself.

**Deliberately scoped to only the base pawn-hash table**, not the two
parked-off sources (`correction_history_nonpawn`/
`correction_history_continuation`, D85/D88) — using a signal already
known to correlate with real eval error (the original Phase 13.2
table, established positive), not ones SPRT-tested this session and
shown to have no effect.

**Extracted the margin-reduction arithmetic into a pure, directly-
testable function** (`pruning::singular_margin_reduction(corr_mag) ->
i32`) rather than inlining it in `alpha_beta.rs` — same rationale as
`king_safe_square_count`/`continuation_hash` living in `pruning.rs`:
isolates the actual decision logic from the search-loop plumbing
around it, so it can be unit-tested directly (threshold boundary,
cap behavior) without needing a full search to exercise every case.

**Verification — first diff built under the new D87 standard, not the
probe method**: compiled and ran the actual `cargo test` equivalent —
`cargo build --lib` (plain debug profile, unwind panics) then `rustc
--test` directly against `src/lib.rs`, `src/main.rs` (needed
`CARGO_PKG_VERSION` etc. supplied manually via env vars, since we're
bypassing Cargo's own build-script variable injection along with its
dependency resolution), and all 4 `tests/*.rs` integration files.
**All green: 470 lib tests, 61 `main.rs`-binary tests, 62 integration
tests (19+22+21, `node_count.rs`'s 5 remain `#[ignore]`d benchmarks) —
633 real tests total, zero probe-replica approximation.** This is the
first Phase 26 diff shipped without relying on the probe method as the
final check, closing the gap D87 identified.

**Not yet done**: no SPRT-style A/B test — needed before this is
trusted as more than "compiles and passes the real test suite," same
bar as every other Phase 26 item. Given items 1, 3a, and 3b all landed
flat-to-negative, this one's result (whichever direction) is worth
paying close attention to as a data point on whether the "correction
signal" family of ideas has any real signal left in it for Pet Dragon
at its current strength.


## D90 — CorrectionExtension: Strongest Negative Result of the Session, Consistent Across Both Samples. Phase 26 Closed. (Session 85, cont.)

**Results**: 20-game first look (seed 51000) — A (correction
extension on) 5-7-8, 45.0%, -34.9 Elo. 200-game confirmation (seed
52000) — 62-85-53, 44.2%, -40.1 Elo.

**The clearest negative result of any Phase 26 item this session.**
Not only does the 200-game sample agree in direction with the 20-game
look (same pattern as item 3b, D88) — the magnitude actually grew
slightly rather than regressing toward zero, which is the opposite of
what pure sampling noise would typically produce. Combined with the
mechanism itself (shrinking the singular-extension margin at
high-correction positions extends the TT move more often at exactly
the positions where the engine's own historical data says static eval
is least trustworthy) — extending a possibly-less-reliable move more
often, on reflection, is at least as plausible a way to search
*worse* moves for longer as it is to catch tactics eval would miss.
The idea wasn't unreasonable to try, but the result is unambiguous.

**Decision: park it. `CorrectionExtension` stays default `false`.** No
further confirmation needed — this is the least ambiguous of the four
results this session.

**Phase 26 item 3c: closed as "implemented, tested, consistent and
meaningful negative result across two independent samples, parked
off."**

## Phase 26 — Closed

All four SPRT-testable ideas from the Session 83 external-advice
review are now resolved:
- Item 1 (null-move king-exposure guard): flat, parked off (D77).
- Item 2 (adversarial perft coverage): closed, no strength claim
  involved (test-suite hardening).
- Item 3a (non-pawn-material correction): flat-to-negative, parked
  off (D85).
- Item 3b (continuation-based correction): negative, parked off
  (D88).
- Item 3c (correction-scaled extension margin): clearly negative,
  parked off (D90).
- Item 4 (illegal-FEN crash): fixed (D83).

**Four independent strength-affecting ideas, four flat-or-negative
results.** This is a real, useful outcome, not a wasted session — it
answers a genuine question (does this whole idea family have anything
left to offer Pet Dragon at its current strength) with a clear no,
across a properly isolated, SPRT-verified process for each one. All
four options remain in the code, harmless and off by default, in case
different thresholds or a differently-tuned engine make any of them
worth revisiting later — nothing was reverted, everything was proven
one way and left in a known state.

**Recommendation for next direction**: Phase 16 (NNUE) is the
standing larger-scale option — a substantially bigger jump in ceiling
than anything in the Phase 26 family, but also a much larger
undertaking (training data, NORU integration, its own validation
pipeline). Given this session's results, it's a reasonable point to
either start scoping that, or take a step back and look at what
*other* signal sources (not just correction-history variants) might
actually move HCE-level strength before committing to the NNUE
project's scale.

## D91 — Phase 27: LMPEnabled / SingularMultiCutEnabled Diagnostic Toggles (Session 86)

Gokul supplied two external bench logs (100ms/move, then 1000ms/move)
of Pet Dragon (Skill 20, uncapped) vs. real Stockfish (Skill 10, then
Skill 9) from the standard classical FEN — 6 games, 6 losses, all
checkmates against Pet Dragon. Claude-side `python-chess` replay of the
raw move lists (read-only, no repo changes) found the same shape in
every game: material stays roughly level through the opening/early
middlegame, then collapses hard and fast late-game (Match examples:
even until ~ply130 then -9 by ply150; even until ~ply60 then -15 by
ply90) — a tactical-blindness pattern that gets worse as material
thins, not a slow eval drift. The pattern held at both 100ms and
1000ms/move, ruling out plain time pressure as the primary cause: this
is the first real evidence any Elo-affecting regression exists,
gathered from outside this repo's internal-only validation loop for
the first time (see ROADMAP.md Phase 27 for the full writeup).

Two techniques are the leading (still unconfirmed) suspects: D59
(singular extension family: multi-cut + negative extension) and D60
(Late Move Pruning), both from Session 82, both explicitly flagged
`⚠️ not yet Elo-measured against a real match` at the time and never
followed up on. Both are on by default. D49/23.2 (thread-differentiated
Lazy SMP, also flagged unmeasured) is ruled out for *this* bench data
specifically — `EngineState::new()`'s default `threads: 1` means no
helper threads spawn at all unless Gokul's bench tool explicitly raises
`Threads`, so 23.2's thread-identity code never executes in a
single-thread run.

**Implementation:** two new runtime UCI options,
`LMPEnabled`/`SingularMultiCutEnabled`, both **default `true`** —
unlike every prior Phase 26 diagnostic option (`NullMoveKingGuard` etc.,
which default `false` because those are new/unproven techniques shipped
off), D59/D60 are *already* default-on production behavior, so `true`
is the byte-identical-to-current default. Setting either to `false` via
`setoption` reverts that one technique only:
- `LMPEnabled=false`: `alpha_beta.rs`'s move loop never calls
  `pruning::should_apply_lmp` — D60 never fires.
- `SingularMultiCutEnabled=false`: only the two D59 additions on top of
  Phase 13.3/D16's original base singular extension are skipped (the
  multi-cut early-return and the negative-extension branch). The base
  `score < singular_beta → tt_move_extension = 1` branch is untouched —
  that's Phase 13.3's separately-validated behavior, not part of what
  D59/this investigation covers.

Same threading pattern end-to-end as every prior option in this family
(`SearchInfo` → `EngineState` → `cmd_uci`/`cmd_setoption`/`cmd_go`,
including both the main thread's and every helper thread's `SearchInfo`
in the Lazy SMP spawn loop) — no new pattern introduced. Chose runtime
options over a pinned-ref rebuild specifically so the *same* WASM binary
already deployed can be A/B'd against Stockfish via the existing Engine
Bench tool with a `setoption` call, no rebuild/redeploy needed before
the next bench run.

Six new tests: `SearchInfo`-level default-true assertions and
off-still-searches-safely checks for both toggles in `alpha_beta.rs`
(mirroring `null_move_king_guard`'s and `correction_extension_enabled`'s
existing test pattern), plus `EngineState`-level default/parses/
cmd_go-applies tests for both in `main.rs` (mirroring
`test_correction_extension_option_*`). No existing test asserts on
LMP/singular-multicut firing at all (confirmed via grep before writing),
so this carries no risk of changing previously-tested behavior when both
toggles are left at their default `true`.

⚠️ Not yet CI-confirmed to compile/pass (no local `cargo` in this
session's sandbox — reviewed by hand: brace/paren balance checked
programmatically, and every edited call site double-checked against the
existing `info: &mut SearchInfo` binding already in scope at that point
in the function). First CI run on this commit is the real check.

⚠️ This is diagnostic infrastructure only — it does not fix anything by
itself. Next step (not done this session): have Gokul re-run the same
Engine Bench matchup with each toggle flipped off in turn (three
configs: both on/default, LMP off, SingularMultiCut off) to see which
one — if either — makes the late-game collapse disappear, before any
further code change is made against this regression.

## D92 — Phase 27: WASM-Side Diagnostic Toggles for the Browser Pit Tool (Session 87)

Gokul ran D91's diagnostic-toggle config (a) — both `LMPEnabled`/
`SingularMultiCutEnabled` left at default `true`, i.e. unchanged
production behavior — via `web/pit/vs.html`, 1000ms/move, Skill 20 vs.
Stockfish Skill 10: **2 draws (threefold repetition) + 1 loss**, a real
improvement over the previous 6-for-6 losses, though replaying the loss
(`python-chess`, material tracked ply-by-ply, same method as prior
sessions) shows the same late-game collapse shape in that one game
(level to ~ply60, then -10 by ply80, checkmate) — the underlying issue
Phase 27 opened to investigate is still present, just not deciding
every game outright at this combination of settings/opponent skill.

Gokul then reported not knowing how to actually run configs (b)/(c) —
`vs.html` has no UCI console, just direct engine-vs-engine play. Reading
`web/pit/vs.html` in full explained why: it calls Pet Dragon via
`search_from_fen_with_eval(fen, movetime, skill)`, a single-shot WASM
function exported straight from `lib.rs` — there is no UCI stdin/stdout
loop in the browser at all. D91's `LMPEnabled`/`SingularMultiCutEnabled`
options only exist on the native UCI path (`main.rs`'s
`cmd_setoption`/`cmd_go`), which this page never touches. **D91's
toggles were unreachable from the only tool Gokul actually uses for
this investigation** — a real gap, not a Gokul-side usability question.

**Fix:** added the WASM-side equivalent, following the same shape
`vs.html`'s own Stockfish wrapper already uses for its `Skill Level`
option (`StockfishEngine.setSkill()` — a small stateful setter called
before each move that only sends the UCI command when the value
actually changed):
- Two new module-global `AtomicBool`s in `lib.rs`
  (`LMP_ENABLED`/`SINGULAR_MULTICUT_ENABLED`, both `::new(true)`) plus
  two new `#[wasm_bindgen]` exports, `set_lmp_enabled(bool)` /
  `set_singular_multicut_enabled(bool)`, that store into them.
- Both `search_from_fen` and `search_from_fen_with_eval` now read these
  atomics into each fresh `SearchInfo`'s `lmp_enabled`/
  `singular_multicut_enabled` fields (previously left at `SearchInfo::
  new()`'s own default `true`, which is why nothing needed to change
  for anyone who never calls the new setters — `web/index.html`, the
  real-user-facing play page, doesn't and isn't affected).
- `web/pit/vs.html`: new "Pet Dragon diagnostics (Phase 27)" card, two
  checkboxes (`LMP enabled (D60)`, `Singular multi-cut enabled (D59)`),
  both checked by default. Applied once at boot (right after
  `wasm_main()`) and live on every `change` event (guarded by
  `dragonReady` so a click during the boot race is a no-op rather than
  throwing into an uninitialized WASM instance — `boot()`'s own
  post-ready application already covers that case). Frozen while
  `match.running`, exactly the same rule `sideInputs[side].engine/
  skill/movetime` already follow, to stop a single game from silently
  mixing moves searched with the toggle on and off.
- The exported bench log now records `Pet Dragon diagnostics:
  LMPEnabled=..., SingularMultiCutEnabled=...` per match (captured into
  `currentGameMeta` at the same point white/black configs are snapshot,
  same "settings in effect when the game's first move was requested"
  timing) — a downloaded log is now self-describing about which A/B
  config produced it, so a future session (or Claude) doesn't have to
  ask which config a given log came from.

Chose module-global atomics over adding parameters to
`search_from_fen_with_eval` itself: the latter would have forced
updating every call site across both `web/index.html` and
`web/pit/vs.html` in lockstep with the exact same new parameter count
and order, and wasm-bindgen's JS↔Rust `bool` marshalling turns a missing
JS argument into `false`, not the type's "default" — silently flipping
`web/index.html`'s real-user-facing play page to both techniques
*disabled* the moment its call site fell out of sync with the new
signature, with no compiler error to catch it (JS, not Rust, on that
side of the boundary). A separate setter function makes the default
path (never call it) provably identical to before these functions
existed, and only `vs.html` opts in.

Numbering note: this session's earlier draft mislabeled the toggle-
mechanism entry above as "D90," which collided with Session 85's
already-existing `D90 — CorrectionExtension` entry. Corrected to D91
(the toggle mechanism itself, `main.rs`/`search/mod.rs`/
`search/alpha_beta.rs`) and D92 (this entry, the WASM/`vs.html` half)
before anything was committed — no duplicate D90 exists in the
committed history.

⚠️ Same caveat as D91 — no local `cargo`/`wasm-pack` in this session's
sandbox, so this is hand-reviewed (brace/paren/div-tag balance checked
programmatically across `lib.rs` and `vs.html`, every new element ID
confirmed unique, `dragonReady` declaration order checked against the
new listener registration's position in the file) rather than compiled.
First real check is CI + a manual page load, not `cargo test`.

⚠️ Still diagnostic infrastructure only. Next step unchanged from D91:
run the three configs (both default, LMP off, SingularMultiCut off) via
`vs.html` now that it's actually possible, and see which one — if
either — removes the late-game collapse.

## D93 — Phase 27: D59/D60 Ablation Results (Negative/Inconclusive) + Per-Move Eval Now Logged (Session 88)

Gokul ran the two remaining configs from D91/D92's plan via `vs.html`,
1000ms/move, Skill 20 vs. Stockfish Skill 10, 3 games each:

- **(b) `LMPEnabled=false`, `SingularMultiCutEnabled=true`:** 1 draw
  (repetition, but only after collapsing to -5 material first) + 2
  losses (checkmate). Replay (`python-chess`, material tracked
  ply-by-ply, same method as every prior session) shows the same shape
  in both losses: level to ~ply50-60, then a fast collapse (-12 by
  ply90 in one game, -17 by ply78 in the other, checkmate shortly
  after).
- **(c) `LMPEnabled=true`, `SingularMultiCutEnabled=false`:** 2 losses +
  1 draw. One loss is the most striking result of any bench so far —
  Pet Dragon reached **+1 material** by ply90 and still walked into
  checkmate 8 plies later, which looks like a pure king-safety/mate-
  blindness failure rather than a material-losing blunder. The other
  loss shows the familiar shape (level to ~ply50, -10 by ply70,
  checkmate). The draw in this config is the cleanest game of any bench
  yet — stays +1/+2 material the whole way, no collapse at all.

**Assessment: this data does not support the D59/D60 hypothesis.**
Compared to Session 87's control (a) (2 draws + 1 loss, both toggles at
default), *both* single-technique ablations scored numerically worse
(1 draw + 2 losses each), and the late-game collapse shape still
appears in losses under both ablations. If either D59's multi-cut/
negative-extension or D60's LMP were the primary cause, disabling it
should have measurably reduced or removed the pattern; instead it
persisted in every configuration tested so far, control included. This
is n=3-per-config — nowhere near enough to statistically rule the
techniques in or out on its own — but there is no positive signal here
either, and continuing to guess at more single-technique ablations
without a better source of evidence isn't a good use of Gokul's time
running these by hand.

**Also shipped this session (no hypothesis attached, purely
diagnostic):** both `search_from_fen`/`search_from_fen_with_eval` have
returned a White-relative eval alongside the move since before Phase 27
started (that's what feeds `vs.html`'s eval bar,
`toWhiteRelativeEval`/`parsePetDragonResult`) — `match.history` already
carried it per move, but the **exported bench log discarded it**,
printing only the bare UCI move list. Fixed: `recordMatchLogEntry` now
carries `{uci, eval}` pairs instead of bare strings, and `buildLogText`
prints each move as `e2e4(+35)` / `f2f4(mate-3)` etc. This is a
strictly additive change to the log *format* (still one line per game,
still starts with the same `Moves (N plies` prefix) — no other code
path reads `matchLog[i].moves` as bare strings (checked via grep before
editing), so nothing else needed updating.

**Why this matters for next steps:** every session so far has had to
locate the collapse externally, after the fact, via `python-chess`
material tracking on the raw move list — with zero visibility into
what Pet Dragon's own search *thought* was happening at the time. With
eval now logged per move, the next bench run will show whether Pet
Dragon's own eval already predicted the losing outcome well in advance
(a evaluation-function problem — it "knows" the position is bad but
has no better move, or doesn't realize how bad) or stayed confidently
positive right up to the blunder (a search-blindness problem — a real
tactic it never saw coming, single/handful-of-ply horizon effect). That
distinction points to a completely different part of the codebase to
investigate next (`eval/` vs. `search/`) and is a much stronger signal
than another round of single-technique ablation guesses.

**Not yet done:** no new hypothesis proposed this session. Next step is
simply to get one more bench run (any config — control is fine, no
need to keep varying D91's toggles right now) with the new eval-logging
build, then read the eval trajectory directly against the material
trajectory already established, before deciding where to look next.

## D94 — Phase 27: Exact-Zero Eval Signature Found — Pet Dragon-Specific, Config-Independent — Depth Now Logged Too (Session 89)

Gokul ran 9 more games via `vs.html` with the D93 eval-logging build:
3 control (both toggles default), 3 with `LMPEnabled=false`, 3 with
`SingularMultiCutEnabled=false`, all 1000ms/move vs. Stockfish Skill
10. Results: 6 losses, 2 draws, 1 draw — consistent with every prior
session, no config stands out as better or worse in any way that would
implicate D59/D60.

**The eval data (first time available) surfaces a much sharper, fully
quantitative finding.** Parsed all 9 games' per-move eval tokens
(`python`, regex extraction, no repo changes) and compared how often
each side's own reported eval is **exactly** `0`:

| Game | Pet Dragon (White) exact-zero rate | Stockfish (Black) exact-zero rate |
|---|---|---|
| Match 1 (control) | 49% (17/35), longest run 6 in a row | 0% (0/35) |
| Match 2 (control) | 38% (10/26) | 0% (0/26) |
| Match 3 (control) | 16% (10/64) | 0% (0/64) |
| Match 4 (LMP off) | 19% (10/52) | 0% (0/51) |
| Match 7 (multicut off) | 27% (21/77) | 1% (1/77) |

**This is a Pet Dragon-specific signature, not a property of these
positions.** Stockfish, playing the exact same games, essentially never
reports its own move as dead-even (0-1%) — real chess positions are
almost never *exactly* balanced to the centipawn, especially not 16-49%
of the time in the same game. Pet Dragon reporting exact `0` this often,
including immediately after captures and in positions Black's own eval
(one ply later) shows as decisively lost for White (e.g. Match 1: `...
c7b6(-947) g1g2(0) b6h6(mate-3)...` — Pet Dragon's own last non-mate
eval before getting mated is `0`, while the position was actually ~-9
pawns), is not "the position happens to be equal" — it looks like the
search is failing to produce (or losing) a real score on a large
fraction of moves and reporting a fallback/sentinel value instead. This
pattern is present in **every config tested across all three sessions**
(control, LMP off, SingularMultiCut off) — further evidence against the
D59/D60 hypothesis, and pointing at something generic in the
score-reporting path rather than either individual search technique.

Also striking, independent of the zero-eval pattern: Match 2 shows Pet
Dragon's own eval at `+423` (a supposedly large winning advantage) two
of its own moves before getting checkmated — a completely different
failure signature (confident, wrong, and not merely "stuck at 0"),
suggesting there may be more than one thing wrong, or a shared root
cause that manifests as both "stuck at 0" and "wildly overconfident"
depending on exactly what state triggers it.

**Investigated, not confirmed:** read `search/iterative.rs` in full.
`SearchResult::score` is initialized to `0` at the top of
`iterative_deepening()` (line ~95) before any iteration runs, and
`iterative_deepening()`'s own `score_for_result` selection logic
(`if info.best_score.abs() > 0 && info.best_score != -INFINITY {
info.best_score } else { best_score }`) treats a genuinely-computed `0`
identically to "not yet set" — a real 0/sentinel-confusion bug in that
one expression. Traced both `info.best_score` and the local `best_score`
back through `search_at_depth`/`search_with_aspiration` and could **not**
confirm from static reading alone that this specific expression is what
produces the observed pattern — in the ordinary synchronous, non-aborted
case both variables appear to already converge to the same value before
that check runs, which would make the fallback a no-op. Not ruling it
out — multi-line reasoning about aspiration-window fail-high/fail-low
loops and the `info.stop` early-break path is exactly the kind of thing
that's easy to get subtly wrong by inspection — but not claiming it as
the confirmed cause either. This needs to be settled with real data, not
another round of code-reading speculation.

**Shipped this session — the WASM search functions now report search
depth too.** `search_from_fen_with_eval` returns a third token: deepest
fully-completed iterative-deepening iteration for that move
(`SearchResult::depth`). Strictly additive — checked both existing
callers (`web/index.html`'s `raw.indexOf(' ')`+`parseInt` truncation,
`vs.html`'s `split(/\s+/)` which already ignored extra tokens) before
adding it; neither needed changes to keep working, verified by reading
both files' parsing logic, not assumed. `vs.html`'s `parsePetDragonResult`
now captures it, threaded through `match.history` → `matchLog` → the
exported log text as a `/dN` suffix, e.g. `g1g2(0)/d6`.

**Why this is the right next step over more code-reading:** depth tells
us immediately which failure mode we're in without more guessing — a
`0`-eval move reported at `/d1` or `/d2` means the search barely
started (time-management/abort issue, worth reading `time.rs`/
`TimeManager` next), while the same `0` at `/d6`-`/d8` (a normal depth
for 1000ms) means a real search completed and the *reported* score is
wrong even though real work happened (worth reading the
`iterative.rs`/`alpha_beta.rs` score-plumbing next, starting from the
`score_for_result` expression flagged above). Either answer points at a
specific, different next file to actually fix, instead of another
distributed A/B guess.

**Not yet done:** no fix — this session is diagnostic only, and the code
suspicion above is explicitly unconfirmed. Next step: one more bench run
with the depth-logging build (any config), then check whether the
`/dN` values on `0`-eval moves cluster low or normal, before touching
`iterative.rs`.

## D95 — Phase 27: Root Cause Found and Fixed — is_time_up() Never Actually Set info.stop (Session 90)

Gokul ran one more bench (3 games, control config, 1000ms/move) with
the Session 89 depth-logging build. The depth data answers last
session's open question decisively: **`0`-eval moves cluster at
completely normal depths (d10-d14), not low/aborted ones.** Example
from Match 1: `f4e5(0)/d14`, `e1g1(0)/d12`, `e2c3(0)/d13`,
`e4f4(0)/d11`, `d4d8(0)/d11` — every one of these is a nominally
deep, fully-iterated search reporting the exact-zero sentinel found in
D94. This rules out a time-management/depth-starvation explanation and
confirmed D94's other lead was the right one to chase.

**Root cause, found by reading `search/alpha_beta.rs` and
`search/mod.rs::is_time_up()` together, not by guessing:**

`SearchInfo::is_time_up(&self)` takes `&self` — it only ever *reads*
`self.stop`/`self.stop_flag`; nothing in it, or anywhere else in
production code, ever *writes* `self.stop = true` on a genuine
elapsed-time timeout (the only place that assignment exists at all was
a unit test manually setting it to verify the read side). Both real
call sites of `is_time_up()` — `alpha_beta()`'s own time check and
`quiescence()`'s — did:
```rust
if info.is_time_up() {
    return 0;
}
```
On a real timeout this returns a hardcoded `0` — a genuine, meaningful
"dead equal" evaluation, not a distinguishable "aborted, discard this"
marker — straight into the caller's alpha-beta comparison, **with no
signal recorded anywhere that anything was cut short.** Every
downstream `if info.stop { ... }` check already present in this
codebase (four more `return 0` propagation sites in `alpha_beta.rs`,
the TT-store guard, the correction-history-update guard, and — most
importantly — `iterative_deepening()`'s `if info.stop { break; }`
discard-this-depth logic) was written assuming `info.stop` would
already be `true` by the time it ran. It never was, for the single most
common reason a search aborts. The codebase already had the right
architecture for handling this correctly; the two sites that were
supposed to trigger it just never did.

**Mechanism, matching every symptom gathered across Sessions 85-90:**
once wall-clock time is exceeded mid-iteration, *every* node visited
after that point — not just the first one — independently calls
`is_time_up()`, gets `true` again (it's a live elapsed-time check, not
a one-shot latch), and returns `0` for its own subtree, all without
that depth's overall iteration ever being marked as aborted. The search
keeps running (still consuming real wall-clock time visiting more
nodes, since nothing tells it to stop), contaminating an increasing
fraction of the tree with `0`s as it goes, and when the iteration
finally does finish naturally, `iterative_deepening()`'s
`if info.stop { break; }` never fires (stop was never set), so the
contaminated result is accepted as this depth's legitimate output —
exactly matching D94's "normal depth, wrong score" data. This also
explains why the pattern is:
- **Pet Dragon-specific, not Stockfish** (D94) — it's this engine's own
  time-check bug, unrelated to the opponent.
- **Config-independent across D91's LMP/SingularMultiCut toggles**
  (D93/D94) — the bug is in shared time-check plumbing every config
  path goes through identically; no wonder ablating either technique
  never moved the needle.
- **Worse in longer, more complex positions** — more nodes per ply
  means a higher chance of still being mid-tree when the 1000ms budget
  expires, so more of the tree gets contaminated.
- **Manifests as sudden late-game material collapse** — once
  contamination starts, any real move scoring worse than `0` can lose
  to a corrupted `0` from a sibling/reply subtree that happened to get
  cut short, causing the search to prefer objectively bad moves or miss
  that a move is a blunder because the opponent's punishing reply's
  subtree returned a falsely comfortable `0` instead of its real score.

**Fix (both files' single production call sites, `search/alpha_beta.rs`
lines ~91 and ~295):**
```rust
if info.is_time_up() {
    info.stop = true;
    return 0;
}
```
Two-line change at each site. This activates the discard/skip logic
that was already written and already correct — it doesn't add any new
propagation logic, because none was needed; the codebase already knew
what to do once `info.stop` was true, it just never became true.
`reset_for_search()` already resets `self.stop = false` at the start of
every search (confirmed by reading it before relying on this), so this
can't leak across moves. `info.stop` and the separate `stop_flag`
`Arc<AtomicBool>` (the UCI `stop` command / external-abort mechanism)
remain cleanly independent — this fix only ever writes to the plain
`bool`, never touches `stop_flag` — so there's no interaction with
mid-game `stop`/ponder handling.

**Tests added** (`search/alpha_beta.rs`): two new tests, both giving a
`0ms` time budget so `is_time_up()`'s elapsed-time branch fires on the
very first check (`nodes` starts at `0`, `0 & 255 == 0` is true, and
any elapsed time is `>= 0`) —
`test_alpha_beta_sets_stop_on_real_timeout` (the main search function's
call site) and `test_quiescence_in_check_sets_stop_on_real_timeout`
(the quiescence in-check-evasion call site, using a constructed
genuinely-in-check FEN to actually exercise that specific branch). Both
assert `info.stop` is `true` afterward — this is exactly the write-side
behavior that was missing; `is_time_up()`'s existing test coverage in
`search/mod.rs` already covered the read side (given `info.stop` already
true, does it report time-up correctly) but nothing previously tested
that a real elapsed-time timeout causes that flag to actually get set.

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as every session since Phase 27 began. This is the
highest-confidence fix of the investigation by a wide margin (a
specific, readable mechanism, not an ablation guess), but it still
needs a real bench run after CI/Pages redeploy to confirm the exact-zero
eval pattern actually goes away and win rate actually improves — Phase
27 stays open until that's confirmed, not closed on code-review
confidence alone.

⚠️ This may not be the *only* thing wrong — D94 separately flagged
Match 2's `+423`-then-mated-in-a-few-moves anomaly as a differently-
shaped pattern that this fix doesn't obviously explain (a search that
completes normally but is simply badly wrong about the position is a
different bug class than "returns a corrupted sentinel mid-search").
Worth specifically checking whether that pattern also disappears once
this fix is validated, or whether it's still there and needs its own
investigation.

## D96 — CI Caught a Real Regression in D95's Own Edit: quiescence()'s `info.nodes += 1` Was Accidentally Deleted (Session 91)

Gokul ran CI on the D95 commit and it failed:
`search::alpha_beta::tests::test_qsearch_in_check_generates_evasions`
panicked on `assert!(info.nodes > 0, "Must search nodes when in
check")` — 475 passed, 1 failed.

**Cause:** D95's edit to `quiescence()`'s time check used a `str_replace`
whose `old_str` included the pre-existing `info.nodes += 1;` line (to
keep the match unique against the second, near-identical time-check site
in `alpha_beta_with_excluded()`), but the `new_str` — the new comment
block plus the `info.stop = true;` line — didn't include it back. The
replacement silently dropped a real, load-bearing line that had nothing
to do with the fix. `alpha_beta_with_excluded()`'s own time check (the
second D95 site) wasn't affected — different edit, `info.nodes += 1;`
sits far enough below it (past an intervening `if depth <= 0 {...}`
block) that it was never part of that `old_str` in the first place.

**Impact of the dropped line, beyond the one failing test:** every
`quiescence()` call would have left `info.nodes` completely
un-incremented — meaning the total node counts reported in NPS
calculations and `seldepth`-adjacent stats were undercounting by
whatever fraction of total search nodes actually run through
`quiescence()` (typically a large fraction, since quiescence dominates
node counts in most positions). It would also have made
`is_time_up()`'s own 256-node sampling gate (`self.nodes & 255 == 0`)
evaluate `true` on *every single* quiescence call instead of every
256th — a real performance regression (an `Instant::now()` read on
every node instead of one every 256) that this specific CI failure
didn't directly test for but would have shipped alongside the D95 fix
if it had merged as-is.

**Fix:** restored `info.nodes += 1;` immediately after the new
D95 time-check block in `quiescence()`, in the same position it
occupied before D95 touched this function. One line, no other change.

**Process note, stated plainly rather than glossed over:** this was
caught by CI exactly the way it's supposed to work — Gokul ran the
build, a real test failed, and the failure pointed straight at the
actual defect. This is also a concrete argument for the project's own
stated caution about `str_replace`-based edits on this repo: an
`old_str` chosen only to disambiguate a match, rather than to bound
exactly the lines that should change, can silently swallow adjacent
code that has nothing to do with the edit. Re-reading the *diff* being
produced (not just confirming brace/paren balance, which this passed
even with the bug present) before presenting a file would have caught
this without needing CI to.

⚠️ Same caveat as every session in this investigation: no local `cargo`
to compile/test-check this specific one-line fix before handing it
back. This needs a green CI run to actually confirm — not assumed fixed
from reasoning alone, especially not after this exact class of mistake
just happened once already this investigation.

## D97 — Phase 27 RESOLVED: D95 Fix Confirmed by Real Bench Data (Session 92)

Gokul confirmed CI fully green (all 476 tests, including the two new
D95 tests and the D96 correction), then ran a 3-game bench with the
fixed build: 1000ms/move, Skill 20 vs. Stockfish Skill 10, control
config. **Result: 1 draw, 1 loss, 1 win** — the first win Pet Dragon has
recorded against real Stockfish anywhere in this investigation
(Sessions 85-92, dozens of games).

**Quantitative confirmation the fix worked** (same `python`
regex-extraction method used for every prior session's analysis, still
read-only):

| Game | White exact-zero rate |
|---|---|
| Match 1 (draw) | 8% (4/53) |
| Match 2 (loss) | **0%** (0/59) |
| Match 3 (win) | **0%** (0/40) |

D94's baseline across Sessions 87-89 was 16-49%. This bench: 0-8%, and
the 4 remaining zeros in Match 1 are not the bug — they're the final 4
plies of a 106-ply game that ends in a genuine threefold-repetition
draw, reported at **depth 19-20**, which is exactly what a real,
correctly-evaluated repetition-drawn position should show. That's the
fix working correctly, not a leftover instance of the bug (which
produced `0` at normal-but-not-exceptional depths in clearly *decisive*
positions, not in an actual dead-drawn shuffle at very high depth).

**The other D94 anomaly is also gone from this sample.** Match 2 (the
loss) has no `+400`-ish spike-then-immediately-mated pattern — the eval
trends smoothly and monotonically from roughly even down through
`-100`s, `-500`s, `-1000`s to `mate-2`, fully coherent with a real,
gradually-deteriorating position, nothing like Session 89's Match 2
(`a8a5(+423)` two moves before getting mated). Not proof this pattern
can never recur (n=1 on this specific anomaly), but it's the expected
shape if D95 was the shared root cause behind both symptoms, as D94
speculated it might be.

**Assessment: Phase 27 is resolved.** The specific, confirmed bug
(`is_time_up()` never setting `info.stop`, D95, correctly landed after
D96's fix to D95's own accidental node-counter deletion) is what this
investigation set out to find, and the before/after data is about as
clean a confirmation as three games can give: the signature metric
(exact-zero eval rate) collapsed to near-zero, the second anomaly
disappeared, and Pet Dragon won a game for the first time. Match 2's
loss is not concerning on its own — losing *some* games to Stockfish
Skill 10 at equal time controls is normal, expected variance for an
engine at this strength, not evidence of a remaining bug; the point of
Phase 27 was never "Pet Dragon should never lose," it was "Pet Dragon
should not be losing *because its own search is corrupting its
scores*," and that specific problem is gone from this sample.

**Not fully proven at n=3** — genuinely closing the loop on real
playing-strength impact (as opposed to confirming the specific bug
signature is gone) would need a much larger sample, the kind of
SPRT-style A/B this project already has tooling for
(`uci_match_runner.rs`) for internal Elo questions. That's a reasonable
follow-up if Gokul wants a numeric Elo-impact estimate for the
changelog, but it's normal ongoing strength-tracking work at this
point, not an open regression investigation — Phase 27 closes here.

## D98 — Phase 28: TDSE First Diff — Verified an External Design Doc Against Real Code, Found One Real Error, Implemented Legality-Only Signal (Session 93)

Gokul supplied `tdse-pet-dragon-adaptation.md` — a design proposal for a
Threat-Defusal Search Extension (TDSE): when the root search ends with
several near-tied best-move candidates, prefer whichever one best
defuses the opponent's strongest reply, instead of an arbitrary tiebreak
among moves already judged equivalent. The doc explicitly claimed to
have verified its API assumptions ("verified against the actual file")
at roughly ten separate points across `alpha_beta.rs`, `ordering.rs`,
`iterative.rs`, `mobility.rs`, `see.rs`, `pruning.rs`, and the bitboard
module, including corrections to an implied first draft (e.g.
`mobility_for_color()` turning out to be private,
`alpha_beta_with_excluded()` returning only a score rather than a move
and PV).

**Did not take those claims at face value.** Pulled a fresh copy of the
repo and independently re-checked every load-bearing claim before
writing anything — the fact that a document *says* it was verified
against real code is not itself verification; I have no way to know who
wrote it or whether they actually read the current source. Checked:
the null-move block's exact shape (`can_null_move`, `king_safe_squares`,
`has_non_pawn_material`) — matched near-verbatim; `score_moves`/
`next_move` signatures in `ordering.rs` — matched exactly; the Skill
Level noise block's real shape in `iterative.rs` (the doc's cited
precedent for "gather MultiPV candidates, then override the result") —
matched near-verbatim, including `search_multipv_slot`'s exact
signature; `mobility_for_color()`'s privacy — confirmed; `see()`/
`see_value_of()` signatures — matched exactly; `CorrectionHistory`'s
real shape and the `pawn_hash`/`nonpawn_hash` free functions — matched;
the raw bitboard attack primitives (`knight_attacks`, `bishop_attacks`,
`rook_attacks`, `queen_attacks`) — all present with the claimed
signatures; `make_move_with_history`/`unmake_move_with_history`,
`TranspositionTable::probe` returning `Option<TTEntry>` with a `.mv`
field, plain `make_move`/`unmake_move` — all confirmed.

**One real error found that the document itself missed:**
`control_delta_on_threat_squares` (not implemented this session, but
checked anyway since the same accessor pattern appears in the
legality-only code) called `threat_move.to_square()`. That method
doesn't exist anywhere in this codebase. `Move` has plain public struct
fields `from: Square` and `to: Square` (`#[derive(... PartialEq ...)]`,
confirmed `Copy`) — the correct access is `threat_move.to`, no method
call. This is exactly the kind of subtle, plausible-looking-but-wrong
detail that a document merely *asserting* verification can still get
wrong, and it's why the read-before-write discipline applies even to
content that arrives already claiming to have done that reading.

**Implemented, first diff only, per the proposal's own staged rollout
(§5) — legality-only signal, no SEE, no control-delta yet:**

- `SearchInfo::threat_defusal: bool`, default `false` — new/unproven
  technique, same rollout shape as D75's `null_move_king_guard` (default
  `false`), *not* D91/D92's already-shipped-default-on pattern.
  Threaded through `EngineState` → `cmd_uci`/`cmd_setoption`/`cmd_go`
  (main thread and every Lazy SMP helper thread), identical plumbing to
  every prior option in this family.
- `extract_threat_move()` (`alpha_beta.rs`, `pub` — this codebase has no
  `pub(crate)` precedent anywhere in `search/`, so cross-module items
  use full `pub`, matching `alpha_beta()` itself): a self-contained
  one-ply-ahead scan of the opponent's replies, reusing the same
  null-move side-flip and `has_non_pawn_material` zugzwang guard the
  real null-move probe uses, via the corrected design the proposal
  settled on. **Deliberately does not try to recover the null-move
  probe's own PV from `info.pv`/`info.update_pv`** — that table updates
  on any node whose local alpha improves, not just true PV nodes, so
  it's not safe to read mid-search as "what this one probe concluded"
  without much more bookkeeping than the signal is worth. A fresh,
  independent one-ply scan is simpler and can't be corrupted by
  unrelated search activity.
- `defuses_threat()` (`alpha_beta.rs`, `pub`): legality-only — does the
  candidate move make the opponent's probed threat move no longer an
  exactly-legal move afterward (same `from`/`to`/`kind`/`captured`
  shape)? `Move` derives `PartialEq` on all four fields, so this
  correctly distinguishes "the threat piece moved away, so capturing it
  isn't possible anymore" from "a differently-flavored move to the same
  square now exists" — a quiet move to a square that used to hold a
  capturable piece is a *different* `Move` value (different `kind`/
  `captured`), so it correctly reports `true` (defused) rather than
  matching the old capture.
- New sibling block at the end of `iterative_deepening()`, immediately
  after the existing Skill Level noise block, not a hook into
  `alpha_beta()`'s move loop — reuses Phase 19's `search_multipv_slot`/
  `info.root_exclude` exactly the way the noise block already does.
  **Mutually exclusive with the noise block** (skipped whenever
  `skill_noise_window_cp(info.skill_level) > 0`) — the proposal itself
  flagged the real risk of both blocks racing to override
  `result.best_move` through the same `info.root_exclude` state
  sequentially; combining them is left as a later, separately-validated
  question rather than assumed safe.
- Implementation deviates from the proposal in one place: rewrote the
  proposal's `.find()`-with-closure candidate search as a plain `for`
  loop instead. The proposal's own code used a manual loop specifically
  to avoid a closure capturing `&mut Position` through an iterator
  adapter — a pattern that's very likely fine in modern Rust, but
  without a local compiler to verify it, the already-reasoned-through
  manual loop shape is the lower-risk choice, not a shortcut.

**Deliberately not implemented this session**: SEE-degradation and
square-control signals (the proposal's §3) — genuinely new code, not a
free reuse of anything in `mobility.rs` as an unverified first draft
might have assumed, and each needs its own isolated diff and A/B
validation before being combined with the others, per the same
discipline Phase 26's correction-history sub-items (3a/3b/3c) already
established for this project.

Ten new tests: `SearchInfo` default-false (`alpha_beta.rs`'s test
module); `extract_threat_move` finding a hanging-rook capture on a
constructed FEN (`3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1` — Black queen d8 can
freely capture White's undefended rook on d5); `defuses_threat`
returning `true` when the rook moves away (the exact capture is no
longer legal) and `false` for an unrelated king move (the capture is
still fully legal); `EngineState`-level default/parse/cmd_go-wiring
tests in `main.rs`, mirroring `test_cmd_go_applies_
singular_multicut_enabled_to_search` exactly; and two `iterative.rs`
tests — default-off produces byte-identical `best_move`/`score` to a
run without the block, and enabling it on a real search from the start
position doesn't panic and still returns a legal move.

⚠️ Not yet CI-confirmed — no local `cargo` in this session's sandbox,
same caveat as every session in this repo. This diff is meaningfully
larger than D95's one-line fix; watch CI closely rather than assume
green, especially given D96 already showed once this session-history
that a seemingly-careful edit can still slip in a real mistake.

⚠️ Zero Elo validation has happened yet. `ThreatDefusal` ships `false` by
default and stays that way until the proposal's own rollout plan
(20-game first look, not trusted alone → 100-200 game confirmation) is
actually run — see ROADMAP.md Phase 28 for the exact next steps.

## D99 — Phase 28: TDSE 20-Game First Look — Mild Negative Point Estimate, Not Statistically Distinct From Zero (Session 94)

Gokul ran the `uci_match_runner` A/B via the mobile Actions workflow,
exactly per D98's plan: same commit (`main`) built twice, Engine A with
compiled-in defaults (`ThreatDefusal=false`, the control), Engine B with
`setoption name ThreatDefusal value true`. 20 games, 1000ms/move, seed
60000.

**Raw result:**
```
A wins: 3   B wins: 1   Draws: 16
A score: 55.0%
Elo diff (A vs B): +34.9
```

`ThreatDefusal=true` (Engine B) scored worse than the control — a
point-estimate Elo loss of ~35. **Following this project's own D76/D84
discipline exactly: a 20-game first look is not trusted alone in either
direction, and this one needs the same "not statistically distinct from
zero" scrutiny those entries always give a small-sample result before
reacting to it.**

Checked: with only 4 decisive games out of 20 (3 A wins, 1 B win) and 16
draws, a rough 95% confidence interval on A's score
(mean 55.0%, empirical SD over per-game scores, `SE = SD/√20`) comes out
to roughly **(45%, 65%)** — comfortably straddling 50%. This result is
**not statistically distinguishable from a coin flip** at this sample
size. The point estimate is real and mildly discouraging, but the
uncertainty band is wide enough that "TDSE is worse," "TDSE is neutral,"
and even "TDSE is mildly better" are all still consistent with this
data.

**Recommendation, not a unilateral decision — this is genuinely a
judgment call at this significance level, Gokul's to make:**
- The ROADMAP Phase 28 plan called for a 100-200 game confirmation run
  "if the first look isn't clearly negative." This result doesn't meet
  a "clearly negative" bar in a statistically rigorous sense (the CI
  includes zero), so the plan's own criterion technically says proceed
  to confirmation.
- That said, the point estimate is negative, not positive or flat — this
  isn't a case like D91/D92's LMP/SingularMultiCut ablations where the
  first look gave genuinely no signal either way. A 100-200 game
  confirmation run costs real CI time for a technique whose first look,
  while inconclusive, leans the wrong direction rather than a
  neutral/promising one.
- Both are legitimate paths: (a) run the 100-200 game confirmation
  anyway, since the plan already committed to that threshold and a
  mildly-negative-but-inconclusive result is exactly the ambiguous case
  that threshold exists to resolve, or (b) treat this as enough signal to
  deprioritize TDSE for now without spending the larger CI budget,
  revisitable later if a different signal set (SEE-degradation,
  control-delta) shows more promise once implemented. Left open for
  Gokul to decide — not resolved by this entry.

**No code changes this session** — diagnostic/decision-recording only,
`ThreatDefusal` stays `false` by default either way pending that choice.

## D100 — Phase 28: 200-Game Confirmation Run CRASHED — Real Bug Found in Harness's Own Diagnostics, Fixed; TDSE's Own Bug Still Unconfirmed (Session 95)

Gokul chose to proceed with the 100-200 game confirmation run from
D99's open decision — 200 games, 100ms/move, seed 61000, same
`uci_match_runner` A/B shape (Engine A = control, Engine B =
`ThreatDefusal=true`). **The run crashed after 14 completed games**,
mid-game 15:
```
thread 'main' (3098) panicked at src/bin/uci_match_runner.rs:146:17:
engine process closed stdout while waiting for 'bestmove'
... Aborted (core dumped) ... Process completed with exit code 134.
```
**This is a more serious finding than D99's Elo question.** A crash
means forfeiting/hanging in real play regardless of playing strength —
more important to resolve than whether TDSE is +/- some Elo.

**What this message actually tells us, and what it doesn't:** the
panic shown is the *harness's own* secondary panic (`uci_match_
runner.rs`'s `wait_for_line_starting_with`, detecting that a child
engine process's stdout closed) — not the crashed engine's own panic
message. `RUST_BACKTRACE: 1` is set for the job, but that only helps if
the *engine's own* panic gets a chance to print, and checking
`EngineProcess::spawn()` found it does not: the child was spawned with
`.stderr(Stdio::null())`, silently discarding exactly the one stream
that would have shown which engine crashed, where, and why. Confirmed
by grepping every log file this run produced for "panic" — only the
harness's own message appears anywhere.

**Fixed the actual gap before attempting to diagnose the real crash
further** — same "get better data before guessing" discipline this
whole investigation (Phase 27) already used, not a first attempt to
patch TDSE blind:
- `EngineProcess::spawn()`'s `.stderr(Stdio::null())` → `.stderr(Stdio::
  inherit())`. The child's stderr now flows into the harness's own
  stderr, which GitHub Actions already captures combined with stdout in
  the step log by default (confirmed: the harness's own stderr panic
  message from *this* run already appeared in the captured log despite
  going to stderr, proving the step-level capture already covers it —
  `inherit()` just stops throwing away the child's copy of that same
  already-captured stream). No workflow YAML change needed.
- Added a `name: String` field to `EngineProcess`, threaded from
  `label_a`/`label_b` (already computed at the top of `main()`) through
  `spawn()`, and used in the `wait_for_line_starting_with` panic message
  so a future crash says *which* of the two engines died, not just that
  one did — previously only inferable from the last successfully
  printed `game N/200` line.

**Static review of the new Phase 28 TDSE code (`extract_threat_move`,
`defuses_threat`, the `iterative_deepening()` sibling block) for an
obvious panic source before concluding anything:** checked every
`unwrap`/`expect`/indexing operation in code touched this phase.
`tt.probe(...).map(...).unwrap_or(...)` is panic-safe. `candidates[0]`
is always valid (the vec is seeded with one element before any
possibility of being empty). Confirmed `Cargo.toml`'s `[profile.
release]` sets `panic = "abort"` (explains the exact "Aborted (core
dumped)" symptom) and does **not** set `overflow-checks` or `debug-
assertions` true, ruling out an arithmetic-overflow panic as the cause
(`INFINITY = 1_000_000`, nowhere near `i32::MIN`/`MAX`, so even an
unguarded negation wouldn't overflow regardless). Confirmed
`alpha_beta_with_excluded()` — called directly from `extract_threat_
move` at `ply=1` rather than via the normal root entry — is exactly
what the public `alpha_beta()` wrapper itself calls with `excluded =
Move::NULL`, so this isn't skipping any hidden root-only setup the
wrapper would otherwise do. **Did not find the specific panicking line
by static reading alone** — this needs the actual captured panic
message from a re-run, not another round of guessing.

**Not yet fixed**: the actual TDSE crash. Leading, unconfirmed
hypothesis given it's brand-new code exercised for the first time at
real match scale (14 games / ~a few hundred positions, vs. the four
constructed-FEN unit tests from D98): something in `extract_threat_
move`'s one-ply probe or the candidate-gathering loop in `iterative.rs`
hits an edge case current unit tests don't cover (a position with very
few legal moves, an in-check root position interacting with the probe's
side-flip, or a state left slightly inconsistent by an aborted
`search_multipv_slot` call under D95's now-correctly-propagating
`info.stop`). Explicitly a hypothesis, not a diagnosis — do not act on
it without the real panic message.

**⚠️ TDSE must not be considered for default-on under any
circumstances — Elo question or not — until this crash is root-caused
and fixed.** This supersedes D99's open Elo question; a technique that
can crash the engine is disqualified regardless of its Elo impact.

**Not yet CI-confirmed** — no local `cargo` in this session's sandbox,
same caveat as always, for the one-file harness fix.

## D101 — Phase 28: TDSE Crash Root-Caused and Fixed — Missing `!in_check` Guard (Session 96)

Gokul re-ran the 200-game confirmation (same seed, 61000) with D100's
harness fix in place. This time the real crash was captured:
```
thread '<unnamed>' (4354) panicked at src/position/mod.rs:270:14:
King must always be on the board
```
— `Position::king_sq()`'s own panic, meaning some color's king bitboard
had become empty. The harness's engine-naming fix (also D100) confirmed
it was **Engine B specifically** (`ThreatDefusal=true`), 14 games into
the run — consistent with this being TDSE's own bug, not a pre-existing
one (Engine A, same commit, same binary, ran the same 14+ games with
default options and never crashed).

**Root cause, found by comparing `extract_threat_move` against the real
null-move code it explicitly claims to mirror, not by guessing from the
panic message alone:** `alpha_beta_with_excluded()`'s own null-move
block gates its side-to-move flip behind `can_null_move = !pv_node &&
!in_check && depth >= MIN_DEPTH_NULL_MOVE && static_eval >= beta &&
has_non_pawn_material(...) && prev_move != Move::NULL &&
king_safe_squares.map_or(true, |n| n > 1)`. D98's `extract_threat_move`
only carried over `has_non_pawn_material` — every other condition in
that list is a pruning-*effectiveness* heuristic specific to null-move
pruning's own purpose (not relevant to a threat probe, correctly left
out), except **`!in_check`, which is a correctness guard, not a
heuristic one.** Flipping side-to-move while the current side is in
check produces a position where "whose king is under attack" and
"whose turn it is" no longer agree — undefined territory for the normal
move-generation/check-evasion machinery this probe then hands the
position to. This is exactly the kind of state `alpha_beta_with_excluded`
was never designed to receive, and searching it can select/return
something built on invalid assumptions that later manifests as
board-state corruption once real moves derived from it get applied.

At the root of `iterative_deepening()` (where TDSE's block runs, after
the real search already completed), the side to move being in check is
completely ordinary — it happens whenever the last move made was a
check. D98 never covered this case with any test, since none of the
four constructed-FEN unit tests happened to start from an in-check
position — the gap only showed up once TDSE ran across ~14 real games'
worth of real positions, exactly the scale D100 already flagged unit
tests can't substitute for.

**Fix:** one guard, `if pos.in_check(pos.side_to_move) { return None;
}`, added as the very first check in `extract_threat_move` — before
`has_non_pawn_material`, matching the real null-move block's own
ordering isn't required for correctness here but keeps the two
side-by-side comparisons easy to eyeball. TDSE simply doesn't apply its
tiebreak logic when the position is already in check; the near-tied
candidates still get returned via the normal search result untouched.

**New regression test**: constructs a FEN with White's king in check
from a rook down a clear file, asserts `extract_threat_move` returns
`None`, and — specifically because this was a real crash involving
board corruption, not just a wrong-answer bug — asserts both kings are
still exactly where they started and `side_to_move` is unchanged
afterward, not just that the function returned early.

⚠️ **This fix is well-reasoned and directly targets a confirmed,
real deviation from working, precedent code — but it has not been
confirmed to be the *complete* fix by an actual crash-free re-run.**
It's possible (though not evidenced by anything found this session)
that a second, independent issue also exists. Per D100's own standing
rule: **TDSE remains disqualified from any default-on consideration
until a full 200-game run completes with zero crashes** — this fix
gets to attempt that confirmation, it doesn't skip it.

Not yet CI-confirmed — no local `cargo` in this session's sandbox, same
caveat as always.

## D102 — CI Caught a Bug in D101's Own Regression Test — Missing Black King in the Test FEN (Session 97)

Gokul ran `cargo test` on the D101 commit: 482 passed, 1 failed —
`search::alpha_beta::tests::test_extract_threat_move_returns_none_when_in_check`
panicked on `Position::from_fen(...).unwrap()`:
`KingNotFound(Black)`.

**Cause:** the test's FEN (`"4r3/8/8/8/8/8/8/4K3 w - - 0 1"`) placed a
White king and a Black rook, but never placed a Black king anywhere —
an oversight when hand-constructing the position, not a defect in the
D101 guard the test exists to check. `from_fen()` correctly rejects any
position missing a king for either color; this is exactly the kind of
mistake that check exists to catch, and it caught it.

**Fix:** added a Black king on h8 (out of the way of the check itself),
FEN now `"4r2k/8/8/8/8/8/8/4K3 w - - 0 1"` — White's king on e1 is still
in check from the same rook on e8 down the same clear e-file, the
scenario the test is actually meant to exercise. Updated the
`king_sq(Color::Black)` assertion from the no-longer-applicable `E8` to
`H8` to match.

D101's actual fix (`extract_threat_move`'s `!in_check` guard) is
unaffected by this — the failure was entirely in the test's own setup,
never reached the code under test. `482 passed; 1 failed` before this
fix, all other tests including the rest of D98-D101's suite passed
clean.

## D103 — Phase 28: 200-Game Re-Confirmation — Crash-Free, TDSE Clears D100's Safety Bar; Elo Picture Reverses to Mildly Positive, Still Not Conclusive (Session 98)

Gokul re-ran the 200-game confirmation, same seed (61000), with D101/
D102's fix in place. **All 200 games completed — zero crashes.** TDSE
clears D100's crash-safety bar for the first time since Phase 28 opened.
This is the headline result: the `!in_check` guard fix is confirmed
correct against the exact scenario (same seed, same 200 real games,
including whatever position at game 15 crashed the previous attempt)
that previously crashed at game 15.

**Elo result, same run:**
```
A wins: 34   B wins: 42   Draws: 124
A score: 48.0%
Elo diff (A vs B): -13.9
```
Engine A (control, `ThreatDefusal=false`) scored 48.0%, Engine B
(`ThreatDefusal=true`) scored 52.0% — a **point-estimate Elo gain of
~14 for TDSE**, reversing the direction of D99's n=20 first look
(which had leaned ~35 Elo the other way, and was itself already flagged
as not statistically distinct from zero at that sample size).

**Statistical check, same method as D99 and every small-sample result
in this project:** empirical per-game variance over the 200 scores
(34×1.0, 124×0.5, 42×0.0), `SE = SD/√200` → 95% CI on A's score is
roughly **(43.7%, 52.3%)**. This interval is *much* tighter than D99's
(45%,65%) at n=20, and now sits almost entirely on the "B is better"
side — but it still narrowly includes 50%, so this is **not yet a
statistically airtight positive result**, just a substantially more
reliable and now favorably-directed one. D99's n=20 result should be
read as exactly the kind of noisy small-sample outcome its own entry
already warned it might be, not as a real signal that got contradicted.

**Where this leaves Phase 28:**
- Crash-safety: resolved (D101, confirmed by this run).
- Elo: promising, not proven. A larger confirmatory run (the project's
  usual next step for anything this close to the noise floor — see
  D76/D84/D91-D93's own precedents for not trusting an inconclusive CI)
  is the honest next step before any consideration of flipping
  `ThreatDefusal`'s default. This entry does **not** recommend flipping
  the default — the CI including 50% is disqualifying for that on its
  own, independent of the crash question now being resolved.
- Given crash-safety is now established, implementing the SEE-degradation
  signal (the proposal's §3, still not started — see D98) is reasonable
  to pursue in parallel with gathering more Elo confidence on the
  legality-only signal alone, rather than blocking one on the other.

**Not yet done:** a larger (400+ game, fresh seed) confirmatory run if
Gokul wants to resolve the remaining statistical ambiguity before
deciding whether to keep investing in TDSE; otherwise, proceeding to the
SEE-degradation signal as the next isolated diff is a reasonable
parallel path now that the technique is confirmed safe.

## D104 — Phase 28: SEE-Degradation Signal Implemented — Two More Real Bugs Found in the Proposal, Both Fixed (Session 99)

Gokul chose the SEE-degradation path from D103's two open options.
Implemented the proposal's §3 as its own isolated diff, on top of the
now crash-confirmed legality-only signal (D98/D101). Verified every
claim against the real repo before writing anything, same discipline
as D98 — and found **two more real bugs in the proposal's own §3 code**,
beyond the `to_square()` issue D98 already caught.

**Bug 1 — `threat_see_before` computed at the wrong point in
`extract_threat_move`.** The proposal computes it *after* restoring
`pos.side_to_move` back to the original (our own) side:
```rust
pos.side_to_move = pos.side_to_move.flip();  // restore
...
let threat_see_before = see_value_of(pos, best_move);  // proposal's order
```
Read `see_value_of`'s actual implementation
(`search/see.rs`) before trusting this: it does
`let color = pos.side_to_move; ... pos.piece_on(from, color)` — it looks
up "our" piece on `mv.from` using whatever `pos.side_to_move` currently
is. `best_move` is the *opponent's* move (found during the flipped
probe). Calling `see_value_of` after flipping back means `color` = our
own side, `pos.piece_on(best_move.from, our_color)` finds nothing (the
piece there belongs to the opponent), and the function's `None => return
0` branch fires — **silently returning 0 every single time**, not a
loud failure. Fixed: compute `threat_see_before` *before* the restore,
while `pos.side_to_move` still correctly equals the opponent (`best_
move`'s actual mover). Caught by a dedicated test
(`test_extract_threat_move_computes_correct_see_before`) that asserts
the exact expected SEE value (500, an undefended rook) rather than just
checking non-panic — with the original ordering this test would have
seen `0`, not 500, and failed loudly instead of the bug hiding silently
in production.

**Bug 2 — `mover_color` computed backwards in
`control_delta_on_threat_squares`.** The proposal:
```rust
let mover_color = pos.side_to_move.flip(); // "threat move was made by
                                            // the opponent"
```
This function is called from `defusal_score` *after* `pos.make_move
(candidate)` has already been applied — at that point `pos.side_to_move`
already correctly equals the threat's owner (the opponent, since it's
genuinely their turn next after our candidate move). The proposal's
`.flip()` here points `mover_color` at *our own* side instead — silently
inverting the entire signal (reporting our own defensive strength as if
it were the threat's attacking strength, and vice versa) rather than
failing loudly or panicking. Fixed: `let mover_color = pos.side_to_move;`
— no flip.

**Both bugs share the same shape**: neither would have panicked or
produced an obviously-wrong result during casual testing — both fail
silently, returning a plausible-looking but backwards or zeroed number.
This is exactly the failure mode independent verification exists to
catch and static "does it compile" checking cannot — confirmed both by
reading the actual callee's implementation (`see_value_of`) and by
tracing the actual call-time value of `pos.side_to_move` at each call
site, not by re-reading the proposal's own comments about what it
intended.

**Implemented, matching the proposal's design otherwise:**
- `ThreatInfo` gains a `threat_see_before: i32` field.
- `attacker_count_on(pos, sq, by_color) -> u32` — same raw bitboard
  primitives `king_safe_square_count` (D75) already uses, same file,
  matching that established precedent over the proposal's suggestion to
  place things in `pruning.rs` (deliberate deviation — see below).
- `control_delta_on_threat_squares(pos, threat_move) -> i32` — fixed
  per Bug 2 and the `to_square()` fix D98 already made.
- `pub fn defusal_score(pos, candidate, threat) -> i32` — combines
  illegality bonus (reuses the same legality check `defuses_threat`
  does, but inline rather than calling it, to avoid a second redundant
  make/unmake pair), SEE-drop, and control-delta into one weighted
  score. `WEIGHT_ILLEGAL=1000`, `WEIGHT_SEE=4`, `WEIGHT_CONTROL=15` —
  starting-point constants, explicitly not yet Texel-tuned, same order
  D63/D68 already established (validate mechanism, tune after).
- `iterative.rs`'s TDSE block now calls `defusal_score` and picks the
  **highest-scoring** near-tied candidate, replacing D98's "first
  candidate that merely makes the threat illegal." Considered gating
  the override on `score > 0` (only act on a clearly positive signal)
  but reverted that after reasoning it through: among candidates the
  real search already judged near-equal, the highest-scoring one by
  this heuristic is still the best available tiebreak even if the
  absolute score is small or negative — gating on `> 0` would silently
  disable the technique in exactly the close-call cases it exists for.

**Deliberate deviation from the proposal's file placement**: the
proposal suggested putting `defusal_score`/the new helpers in
`pruning.rs` ("alongside `CorrectionHistory` for consistency"). Checked
where the proposal's *own* cited precedent (`king_safe_square_count`,
D75 — "new helper reusing existing attack-table primitives, gated by a
runtime option") actually lives: `alpha_beta.rs`, not `pruning.rs`.
Followed the actual precedent over the proposal's stated preference —
also avoids needing to make `ThreatInfo`'s private fields `pub` for
cross-module access, which moving to `pruning.rs` would have required.

`defuses_threat` (D98) is kept, unchanged, still independently tested —
just no longer called from `iterative.rs`'s production path, superseded
there by `defusal_score`. Four new tests: exact SEE-value correctness
(the one that would have caught Bug 1 directly), `defusal_score` ranking
an illegality-inducing move above an unrelated one, and
`attacker_count_on`'s basic per-color correctness.

⚠️ Not yet CI-confirmed — no local `cargo` in this session's sandbox,
same caveat as always, and this diff touches the same functions D96
already showed once this investigation that a careless edit can corrupt
without balance-checking catching it. Reviewed the actual diff text
this time, not just brace/paren counts, learning from D96.

⚠️ Zero Elo/crash validation for this specific signal yet. Same rollout
discipline as D98→D101→D103: CI green → 20-game first look (not trusted
alone) → 100-200 game confirmation, watching specifically for any new
crash this signal's extra `pos.make_move`/`unmake_move` pair in
`defusal_score` could introduce, before any consideration of combining
with control-delta as a third signal or touching any default.

## D105 — CI Caught a Genuine Compile Error in D104's Own Diff — Missing `Square` Import (Session 100)

Gokul ran CI on the D104 commit — it failed to even compile:
```
error[E0425]: cannot find type `Square` in this scope
  --> src/search/alpha_beta.rs:1019:42
   |
1019 | fn attacker_count_on(pos: &Position, sq: Square, by_color: Color) -> u32 {
   |                                          ^^^^^^ not found in this scope
```

**Cause, plainly: my mistake, not a static-analysis gap this time.**
`attacker_count_on`'s signature uses `Square` directly, but D104 never
added `Square` to `alpha_beta.rs`'s top-level imports
(`use crate::types::{Color, Move, MoveKind, PieceKind};`) — only the
test module has its own explicit `use crate::types::Square;`, which
D98's earlier tests already relied on. I'd verified `Square::D8` etc.
worked in *test* code during D98/D101 and carried that unchecked
assumption into new *non-test* code this session without re-checking
what's actually imported at module level outside `#[cfg(test)]`. This
is exactly the class of mistake balance-checking (which passed cleanly
on this file, same as it did on D96's dropped node-counter) cannot
catch — it's not a structural error, it's a missing name in scope.

**Fix**: one line —
`use crate::types::{Color, Move, MoveKind, PieceKind, Square};`.

The compiler also flagged (as a non-blocking warning, not the cause of
the build failure) that `MoveKind` is unused at module level outside
tests — left as-is; not this fix's concern and not something worth
guessing about without more investigation into whether it's related to
this session's changes or pre-existing.

No design or logic changes from D104 — this is purely the missing
import. Everything D104 documented (the two silent-failure bugs found
and fixed in the original proposal, the `defusal_score`/
`attacker_count_on`/`control_delta_on_threat_squares` design) stands
unchanged.

⚠️ Still not CI-confirmed after this fix — no local `cargo` in this
session's sandbox to verify the fix actually resolves it, same caveat
as always. First real confirmation is the next CI run.

## D106 — Phase 28: SEE-Degradation Elo Results — Two Runs Land Near the Noise Floor, No Clear Effect Either Way; Crash-Safety Holds (Session 101)

Gokul ran both the 20-game first look (seed 62000) and, without waiting
for a checkpoint, the 100-200 game confirmation (seed 62500) for the
SEE-degradation signal (D104, post-D105 compile fix). **Both completed
with zero crashes** — `defusal_score`'s extra `pos.make_move`/
`unmake_move` pair didn't reintroduce anything like D100/D101's earlier
crash. Crash-safety continues to hold.

**Raw results:**
```
n=20  (seed 62000): A wins 1,  B wins 3,  draws 16  — A score 45.0%, Elo diff -34.9
n=200 (seed 62500): A wins 38, B wins 36, draws 126 — A score 50.5%, Elo diff +3.5
```
(A = control/`ThreatDefusal=false`, B = SEE-degradation signal on.
Positive Elo diff = A/control ahead; negative = B/TDSE ahead — same
sign convention as D99/D103.)

**95% CI on each, same method as every prior result:**
```
n=20,  SEE-degradation:  A score 45.0%  →  (35.4%, 54.6%)
n=200, SEE-degradation:  A score 50.5%  →  (46.3%, 54.7%)
```
The n=200 result is about as close to a genuine null result as a
200-game sample gets — nearly symmetric around 50%.

**Comparing against D99/D103's legality-only results, all four side by
side:**
```
n=20,  legality-only:     A score 55.0%  →  (45.4%, 64.6%)
n=200, legality-only:     A score 48.0%  →  (43.7%, 52.3%)
n=20,  SEE-degradation:   A score 45.0%  →  (35.4%, 54.6%)
n=200, SEE-degradation:   A score 50.5%  →  (46.3%, 54.7%)
```
Every one of these four intervals overlaps every other one. Nothing
here is statistically distinguishable from anything else, including
zero. The two n=20 results even point in *opposite* directions from
each other (legality-only leaned against TDSE at n=20, SEE-degradation
leaned for it at n=20) — exactly the kind of noise this project's own
"don't trust n=20" rule (D76/D84) exists to guard against, now
demonstrated twice with opposite signs from the same underlying
technique family.

**Assessment: across two independent 200-game samples (D103's
legality-only signal, this session's SEE-degradation-augmented signal),
TDSE has not demonstrated a clear, replicated Elo effect in either
direction.** D103's legality-only run leaned mildly positive (+13.9
favoring TDSE, CI 43.7-52.3%); this session's larger-signal run landed
almost exactly on zero (+3.5 favoring the control, CI 46.3-54.7%).
Combined, the most honest reading is: if TDSE has a real effect at its
current weights, it's small — plausibly within roughly ±15 Elo either
way — not the kind of result that would justify flipping a default on
its own merits.

**This is not "TDSE failed" — it's "TDSE hasn't earned promotion,"** the
same distinction D90 drew for Phase 26's correction-history results
before closing that phase. Two live possibilities, not mutually
exclusive:
1. **The technique itself has little to no real effect** at this
   engine's current strength — near-tied root candidates may already be
   genuinely close enough in practice that a threat-aware tiebreak
   rarely matters.
2. **The weights are unvalidated guesses** — `WEIGHT_ILLEGAL=1000`,
   `WEIGHT_SEE=4`, `WEIGHT_CONTROL=15` and `TDSE_MARGIN_CP=20` were
   never tuned, explicitly flagged as "starting-point constants" in
   D104. A real signal could exist but be getting diluted or
   overridden by poorly-calibrated relative weights between the three
   terms, or by a near-tie margin that's too wide or too narrow to
   catch the cases where this actually matters.

**Recommendation, Gokul's call, not resolved here:** (a) treat this as
enough evidence to deprioritize TDSE for now, same as D99 originally
offered as an option — two 200-game runs without a clear win is a
reasonable stopping point; or (b) if there's appetite to keep going,
Texel-tune the three weight constants and `TDSE_MARGIN_CP` before
running another confirmation, since "validate mechanism, then tune"
(D104's own stated order) has now had its mechanism-validation step
done twice without a clear result, and tuning is the one variable in
this technique that's never actually been touched.

---

## D107 — Fix TT Sizing: Floor Directly to Power-of-Two Instead of `next_power_of_two() / 2` (Session 102)
**Decision**: `TranspositionTable::new()`'s entry-count calculation changed
from `(bytes / entry_size).next_power_of_two() / 2` to a direct
floor-to-largest-power-of-two-<=-raw_entries computation
(`1usize << (usize::BITS - 1 - raw_entries.leading_zeros())`, guarded
for `raw_entries == 0`), with the existing `.max(1024)` floor kept
unchanged after that step.

**Why**: External review (Engineering Review & Remediation Proposal,
F-1) found the old formula silently discards half the requested TT
capacity whenever `bytes / entry_size` is *already* an exact power of
two — which is true at every standard Hash size a UCI GUI would ever
send (16/32/64/128/256 MB), including the engine's own 64 MB default.
`next_power_of_two()` is a no-op on an input that's already a power of
two, so the subsequent `/ 2` just throws away half of it. Confirmed
independently by this session (not just re-trusted from the proposal):
`TTEntry` is 16 bytes (matches its own doc comment, "Packed to 16 bytes
for cache efficiency"), so 64 MB / 16 B = 4,194,304 = 2^22 exactly —
the pre-fix engine was running its default Hash setting at 32 MB of
actual capacity, not 64 MB, silently. This directly costs search
strength (more TT misses, more re-searched nodes) at zero benefit and
was not caught by the existing `test_tt_size` test, which only asserted
`size_mb() <= 64` — an inequality the buggy half-size value also
satisfies.

**Not found in decision/session history before this fix** — grepped
`DECISIONS.md`/`SESSION_LOG.md` for TT sizing arithmetic prior to this
entry; nothing discusses it. This was a genuine, undocumented bug, not
a known trade-off (confirmed separately in the Documentation Review,
28 July 2026).

**Regression coverage added**: `test_tt_size_exact_power_of_two_not_halved`
asserts the *exact* expected entry count (not just an inequality) at
16/32/64/128/256 MB, computed independently from `entry_size` at test
run time rather than hardcoded, so it can't silently drift if
`TTEntry`'s layout ever changes size. `test_tt_size_non_power_of_two_floors_correctly`
covers the non-exact-power-of-two case (100 MB) to confirm the fix
didn't change behavior there — the old and new formulas were already
equivalent whenever `raw_entries` wasn't itself an exact power of two,
so this is a non-regression check, not a bug-catching one.

**Alternatives rejected**: Keeping `next_power_of_two()/2` and special-
casing "if already a power of two, don't halve" was considered and
rejected as more code for the same result — a direct floor computation
is both correct unconditionally and simpler than a round-trip through
`next_power_of_two()` plus a conditional.

---

## D108 — Fix En-Passant/Hash Desync in Null-Move Pruning; Factor the Flip into a Shared Helper (Session 103)
**Decision**: In both the real null-move probe (`alpha_beta_with_excluded`)
and the duplicated flip inside `extract_threat_move()`, XOR the
en-passant key out of `pos.hash` before clearing `pos.en_passant`, and
XOR it back in when restoring — mirroring `make_move.rs`'s own "remove
en passant from hash" step exactly. Additionally factored the whole
flip (side-to-move + en-passant + hash, make and unmake) into two
shared functions, `make_null_move()`/`unmake_null_move()`, and switched
both call sites to use them instead of inline-duplicated logic.

**Why**: External review (Engineering Review & Remediation Proposal,
F-2) found that clearing `en_passant` without removing its Zobrist key
left `pos.hash` and `pos.en_passant` disagreeing for the rest of the
null-move subtree — the hash kept encoding the old en-passant file
while the field said `None`. Every TT probe/store inside that subtree
therefore used a hash that didn't correspond to the actual logical
position: a structural, deterministic mismatch (correlated with a
specific related position), not the generic 64-bit collision risk the
TT already accepts elsewhere (D4). Confirmed present in both copies by
reading the current source fresh, not re-trusted from the review.

**Not found in decision/session history before this fix** — the
Documentation Review (28 July 2026) grepped `DECISIONS.md`/
`SESSION_LOG.md` for `ep_key`, en-passant hashing, or any hash/state
mismatch discussion and found nothing; this was a genuine, undocumented
gap, not a known trade-off. Notably, D98 already documents reusing this
exact null-move flip for `extract_threat_move()` and cross-checking
that reused block carefully — but only for API correctness (right
function, right signature), not for this specific hash-consistency
defect, so the bug propagated into the new code undetected. This
matches `VARIANT_ARCHITECTURE.md`'s own stated Zobrist principle
("[pawn-start keys are] XOR'd in for each pawn's actual starting
square... critical for TT correctness") — the same standard this bug
violated for en-passant keys specifically.

**Shared-helper factoring**: the Engineering Review's fix recommendation
included factoring the null-move flip into one shared helper "so this
class of bug can only be fixed — or reintroduced — in one place." Done
as `make_null_move(pos) -> Option<Square>` / `unmake_null_move(pos,
old_ep)`, both `#[inline]`, in `search/alpha_beta.rs` near the top of
the file. Named deliberately *not* `make_move`/`unmake_move` — those
already mean a real move elsewhere in this codebase. `extract_threat_move`'s
`see_value_of(pos, best_move)` call (D104 — must run while
`pos.side_to_move` still equals the threat's actual mover) still runs
*before* the `unmake_null_move()` call at that site, preserving D104's
ordering requirement; the helper wasn't extended to cover that call
since it's specific to the threat-defusal signal, not to the null-move
flip itself.

**Regression coverage added**: `test_null_move_helper_keeps_hash_
consistent_with_recompute` builds a position with an active en-passant
square, calls `make_null_move`/`unmake_null_move` directly, and checks
the incremental `pos.hash` against `Position::compute_hash()` (a full
from-scratch recompute) at every stage — mid-flip and after the full
round-trip. This is the general "recompute and compare" invariant
check the Documentation Review recommended (§7, rec. 5), applied
directly to this bug; it fails under the pre-fix code and passes under
the fix. `test_null_move_helper_hash_consistent_without_en_passant`
covers the no-active-ep case to confirm the `if let Some(ep) = ...`
guard is a true no-op, not just untested.

**Alternatives rejected**: Leaving the two call sites independently
patched (matching the review's minimum recommendation) was considered
but rejected in favor of the shared-helper version — the review's own
extended recommendation, and the whole reason D98's reused-block gap
existed in the first place was duplication without a shared source of
truth.

---

## D109 — Move::NULL Prints as UCI Standard "0000", Not Its Literal a1a1 Squares (Session 104)
**Decision**: Special-case `Move::to_uci()` to return `"0000"` when
`self.is_null()` is true, instead of falling through to the normal
from/to formatting (which produced "a1a1", since `Move::NULL` is
defined as `from: A1, to: A1`).

**Why**: Performance Review §4.4 spot-checked the already-checkmated
position case and found the engine prints `bestmove a1a1` instead of
the UCI-standard `bestmove 0000` sentinel most GUIs/tooling expect when
there's no legal move to report. Root cause: `to_uci()` had no special
case for `Move::NULL`, so it printed the literal A1→A1 squares like any
other move. No real, legal move can ever have `from == to` (`is_null()`
already relies on exactly this to distinguish the sentinel from real
moves), so special-casing on it can't misfire on legitimate move
output — confirmed by grepping the repo for existing `"a1a1"`
references first: the only hit was `uci_match_runner.rs` asserting that
`parse_uci_move` rejects `"a1a1"` as an illegal *inbound* move, which
is unrelated (inbound parsing, not outbound formatting) and unaffected
by this change.

**Impact**: Low in practice — GUIs essentially never call `go` on an
already-game-over position — but cheap and precise to fix, per the
Performance Review's own characterization.

**Regression coverage added**: `test_move_null_uci_is_standard_sentinel`
in `types.rs` asserts `Move::NULL.to_uci() == "0000"`.

---

## D110 — Fix Stale NNUE Blend-Weight Doc Comment in eval/mod.rs (Session 105)
**Decision**: Updated `evaluate_blended()`'s doc comment from "D23
default 25%" to accurately state the current default (0%, pure HCE),
citing both D25 (weight dropped to 0%) and D61 (NNUE shelved for the
future) rather than the original, now-superseded D23 figure.

**Why**: Engineering Review F-4 found the doc comment still cited D23's
original 0.25 constant, while the actual runtime default
(`NNUE_BLEND_WEIGHT_PCT` initialized to `0`) has been 0% since D25 —
confirmed by reading the current initializer directly, not just
trusting the review's claim. No functional impact (the code itself was
already correct; only the comment was stale), but worth fixing to
avoid misleading a future contributor who reads only the doc comment.

**Not a symptom of broader confusion**: the Documentation Review
already confirmed `ENGINE_ARCHITECTURE.md` and the UCI option table
both correctly state the 0% default — this was an isolated stale
in-source comment, not a project-wide inconsistency.

**No regression test needed** — this is a comment-only change with no
behavioral surface to test.

---

## D111 — Five-Mode PlayStyle Additive Eval Bonus (Session 106)
**Decision**: Implemented `playstyle-proposal.md`'s Option B in full: a
new `eval/style.rs` module computing a small, independently-computed
bonus for one of five runtime-selectable modes (Balanced/Killer/
Tactical/Positional/Endgame, UCI `PlayStyle` spin 0-4), added on top of
`evaluate_blended()` via a new `evaluate_styled()` wrapper. `PlayStyle`
follows the exact same pattern as `NNUEWeight`: a bare `static
AtomicU32`, set directly from `main.rs`'s `"playstyle"` setoption arm,
`Relaxed` ordering, no `EngineState` field. Search itself (alpha-beta,
PVS, pruning, TT, move ordering) is completely untouched — only the
leaf evaluation changes, via one call-site swap in `search/
alpha_beta.rs`'s `evaluate()` (now delegates to `evaluate_styled()`
instead of `evaluate_blended()`).

**Why Option B over Option A** (editing the tuned tables directly): the
core HCE constants in `king_safety.rs`/`mobility.rs`/`pawns.rs`/
`open_lines.rs` were fit by Texel tuning against 62,125 real positions
(Phase 25, Session 84) — overwriting them per-mode would need a
separate tuning run per mode before any mode is trustworthy, and would
re-open tested-core files with existing regression tests (including
the deliberately hand-overridden `KNIGHT_NEAR_OWN_KING_BONUS`/
`BISHOP_NEAR_OWN_KING_BONUS` sign in `king_safety.rs`) for zero
necessary reason. Option B's `style.rs` reads only public bitboard/
position primitives already used elsewhere and imports nothing private
from another eval module, so it's fully decoupled — Balanced (mode 0,
default) is a byte-identical no-op, confirmed by a new regression test
(`test_evaluate_styled_matches_blended_at_balanced_default`) rather
than just asserted.

**Verification before implementation**: per the proposal's own note
that `alpha_beta.rs` had shifted since it was drafted, re-fetched
`eval/mod.rs`, `search/alpha_beta.rs`, and `main.rs` fresh rather than
trusting the proposal's cited line numbers. The `evaluate()` wrapper
had moved again since the proposal's last check (1133 → **1169**,
after Session 103's F-2 fix added the `make_null_move`/
`unmake_null_move` helper earlier in the same file) — confirmed and
used the current line, not the stale citation. `king_safety.rs:145`'s
king-zone one-liner (`king_attacks(king_sq) | Bitboard::from_square
(king_sq)`) was re-confirmed unchanged and duplicated (not imported)
into `style.rs`, exactly as the proposal specified.

**Bonus function design** (mirrors playstyle-proposal.md §4 closely,
one deliberate refinement): Tactical mode's "net of the opponent"
squares-controlled metric was implemented as a genuinely symmetric
comparison — squares WE control in THEIR half, minus squares THEY
control in OUR half (not squares they control in their own half, which
would be a less meaningful mirror). Killer mode is deliberately
one-sided (attacks against THEIR king only, no subtraction for danger
to OUR OWN king) since that mirror signal already exists via
`king_safety.rs::evaluate_king_safety()` elsewhere in `evaluate()` —
adding it here would double-count.

**All four mode constants are hand-picked starting points, not yet
Texel-tuned** — `KILLER_ATTACKER_BONUS`, `KILLER_STORM_BONUS_PER_PAWN`,
`TACTICAL_BONUS_PER_SQUARE`, `POSITIONAL_BONUS_PER_SQUARE`,
`ENDGAME_BONUS_PER_UNIT` — flagged explicitly in `style.rs`'s module
doc comment, matching how the project already tags provisional values
(the `KNIGHT_NEAR_OWN_KING_BONUS` note in `king_safety.rs`). Per the
proposal's rollout plan (§7), self-play validation per mode against
Balanced is the next step before any Elo claims are made about the
non-Balanced modes.

**Bonus fix while this block was open**: `main.rs`'s `NNUEWeight` UCI
option comment had the same staleness bug F-4/D110 fixed in
`eval/mod.rs` ("Default matches D23's fixed constant (25%)" — actually
0% since D25). Fixed in the same commit since the PlayStyle option
declaration was added immediately next to it.

**Regression coverage added**: 10 new tests total —
`eval/style.rs` (8): Balanced no-op across 3 position types,
out-of-range defensive fallback, `set_play_style` clamping, and one
sign-correctness test per non-Balanced mode (Killer/Tactical/
Positional/Endgame each on a hand-built attacking-vs-quiet position
pair, per the proposal's own testing plan §6.2).
`eval/mod.rs` (1): `evaluate_styled() == evaluate_blended()` at
Balanced default, across 3 positions.
`main.rs` (1): `PlayStyle` setoption sets and clamps correctly,
mirroring existing `Hash`/`NNUEWeight` option test conventions.

**Alternatives rejected**: exposing `PlayStyle` as five UCI `combo`
labels instead of a 0-4 `spin` (open question in the proposal) — kept
as `spin` for consistency with the engine's two other existing options
in this category (`Contempt`, `NNUEWeight`), both spins; easy to switch
later if a GUI-legibility need arises. Not resolved as part of this
session — flagged as still open in the roadmap for Gokul's call.

**Not yet done** (explicitly out of scope for this session, per the
proposal's own rollout plan): self-play Elo validation per mode
(§6.3) and the eventual Texel-tuning pass on the four modes' constants
(§7.3) — both require actual games, not something this session can
produce.

---

## D112 — PlayStyle Self-Play First-Look Results: All Four Modes Land Within Noise of Balanced at n=20 (Session 107)
**Decision**: Recorded first-look Elo A/B results for all four
non-Balanced PlayStyle modes vs. Balanced, run via
`uci_match_runner.yml` (same-ref, different-`setoption` pattern —
Phase 20's Skill Level precedent). No code or default changed as a
result of this data; `PlayStyle` stays default-0 (Balanced) regardless.

**Results** (n=20 games each, movetime 100ms, Engine A = Balanced
control, Engine B = the named mode):

| Mode | A (Balanced) score | Elo diff (A vs B) | Reading |
|------|--------------------|--------------------|---------|
| Killer (1) | 47.5% (5W-6L-9D) | -17.4 | B (Killer) nominally ahead |
| Tactical (2) | 50.0% (4W-4L-12D) | -0.0 | dead even |
| Positional (3) | 45.0% (2W-4L-14D) | -34.9 | B (Positional) nominally ahead |
| Endgame (4) | 52.5% (5W-4L-11D) | +17.4 | A (Balanced) nominally ahead |

**Why this isn't a real signal yet**: at n=20, the standard error on
match score is roughly ±11 percentage points (≈ ±80 Elo at p≈0.5),
so every result above sits within about 0.5 standard errors of zero —
indistinguishable from pure noise, exactly the same "near the noise
floor" situation D106 documented for TDSE's own n=20/n=200 runs. None
of the four modes crossed ROADMAP Phase 29.6's flag threshold (a mode
losing more than ~100 Elo), so nothing here indicates a broken or
actively harmful mode — but nothing here validates a mode's constants
as good, either. The Killer/Positional modes nominally *winning*
against Balanced at n=20 is notable but not something to read into:
these are hand-picked, untuned constants (D111), and an untuned
additive bonus outperforming the fully Texel-tuned baseline at n=20 is
far more consistent with sampling noise than with a genuine
improvement.

**Not yet done — Gokul's call, genuinely open (mirrors D106's framing
for TDSE)**:
1. **Run n=200 confirmation for all four modes** before drawing any
   conclusion — the same escalation path D99→D103 used for TDSE. Most
   expensive option (4×200 = 800 games) but the only way to actually
   resolve whether any mode has a real effect in either direction.
2. **Skip straight to Texel-tuning the four modes' constants**
   (ROADMAP 29.7) instead of spending compute confirming untuned
   values — since none of the four constants have been tuned yet
   anyway, a confirmation run on the current hand-picked numbers has
   limited value regardless of its outcome; tuning first, then
   confirming the tuned result, may be the more efficient order.
3. **Leave as-is for now** — nothing here is blocking (PlayStyle
   defaults off, exactly like ThreatDefusal did at this stage), so
   there's no urgency either way.
Either 1 or 2 is reasonable; this entry doesn't pick one, same as
D106's stance on TDSE.

---

## D113 — PlayStyle n=200 Confirmation: All Four Modes Flatten to Noise, D112's Leans Confirmed as Sampling Noise (Session 115)

**Decision**: Ran the n=200 confirmation D112 left open (option 1 —
Gokul's call, made this session), same `uci_match_runner.yml` same-ref/
different-`setoption` pattern as every prior PlayStyle/TDSE/Skill Level
Elo measurement in this repo. 4 runs, 200 games each, movetime 100ms,
seeds 70000/70200/70400/70600 (Killer/Tactical/Positional/Endgame vs.
Balanced respectively), no code changed to produce this data — pure
measurement.

**Results** (Engine A = Balanced control, Engine B = the named mode,
sign is A vs B — positive means Balanced ahead):

| Mode | A wins | B wins | Draws | A score | Elo diff (A vs B) |
|------|--------|--------|-------|---------|--------------------|
| Killer (1) | 36 | 35 | 129 | 50.2% | +1.7 |
| Tactical (2) | 36 | 36 | 128 | 50.0% | -0.0 |
| Positional (3) | 40 | 32 | 128 | 52.0% | +13.9 |
| Endgame (4) | 43 | 43 | 114 | 50.0% | -0.0 |

**Reading against D112's n=20 first look:**

| Mode | n=20 Elo diff | n=200 Elo diff | Direction held? |
|------|---------------|-----------------|------------------|
| Killer | -17.4 | +1.7 | No — flipped and flattened |
| Tactical | -0.0 | -0.0 | Flat both times |
| Positional | -34.9 | +13.9 | No — flipped and flattened |
| Endgame | +17.4 | -0.0 | No — flattened toward zero |

Every mode's n=20 lean (as large as ±34.9 Elo) either flattened or
reversed at n=200 — the exact same pattern D103 established for TDSE
(n=20 leans washing out at n=200, not compounding). Positional's +13.9
is the largest remaining gap, well inside the ~±50 Elo 95% CI at this
sample size (draw-heavy match, ~64% draw rate across all four runs,
consistent with 100ms/move Pet Dragon-vs-itself games generally) — not
distinguishable from zero.

**Conclusion**: at their current hand-picked, untuned constants
(D111), none of the four PlayStyle modes show a measurable Elo effect
in either direction against Balanced. This isn't "PlayStyle failed" —
it's the same "hasn't earned promotion" framing D90/D106 used for
Phase 26/TDSE: the mechanism is confirmed crash-safe and Elo-neutral
at n=200×4, which is itself useful information (no mode is quietly
sabotaging play), but says nothing about whether Texel-tuning the
constants (ROADMAP 29.7h) would reveal a real effect the untuned
values are currently diluting or masking. `PlayStyle` stays default-0
(Balanced) — unaffected either way, exactly as D112 already noted.

**Not yet done — Gokul's call, unchanged from before this run:**
1. **29.7h** — generate real bulk self-play data per mode and run the
   actual Texel-tuning pass (the pipeline itself has been built and
   compile/test-verified since Session 110 — this is the compute step,
   not more scaffolding). This is the only remaining lever that could
   turn a currently-neutral mode into a measurably distinct one.
2. **Leave PlayStyle as-is for now** — nothing here is blocking
   (defaults off), so there's no urgency either way, same standing
   status as always.

---

## D114 — Improving Flag Added for LMP + Futility Pruning, Off by Default (Session 116)

**Decision**: Implemented the "improving" flag D60 (Session 82) originally
flagged as a real-but-untaken change — Gokul asked to fix it after a
4-game external Stockfish bench (uploaded log + report) surfaced search-
depth volatility as its most consistent cross-game signal, and asked for
LMP + futility pruning both (broader than LMP alone).

**What it is**: `alpha_beta.rs` now computes, per node (only when
`SearchInfo::improving_enabled` is true): `false` if in check; otherwise
compares this node's (corrected) `static_eval` to the value stored two
plies back for the same side to move (`SearchInfo::static_eval_stack`,
new `[i32; MAX_PLY]` field, `i32::MIN` sentinel for "no usable value").
Unknown two-plies-back data (too shallow, or that node was in check)
defaults to `true` — same "assume improving when unsure" convention
Stockfish uses, since under-pruning on missing information is safer
than over-pruning on it.

**Where it's used**:
1. **LMP** (`pruning.rs`) — `LMP_THRESHOLDS` (the single table that
   existed from D60 through D113) is now `LMP_THRESHOLDS_IMPROVING`,
   values unchanged. New `LMP_THRESHOLDS_NON_IMPROVING` is roughly half
   of each entry (`[0,1,2,3,4,6,8,10,12]` vs.
   `[0,3,4,6,9,12,16,20,25]`), matching the halving relationship
   Stockfish uses between its own improving/non-improving move-count
   thresholds. `lmp_threshold()`/`should_apply_lmp()` both gained an
   `improving: bool` parameter selecting which table applies.
2. **Futility pruning** (`pruning.rs`, new `futility_margin()`) — base
   formula `100 * depth + 200` unchanged when improving; drops to
   `100 * depth + 100` when not (smaller margin ⇒ easier to satisfy the
   skip condition ⇒ prunes more aggressively).

**Rollout discipline**: `improving_enabled` defaults `false` — same
shape as `null_move_king_guard` (D75) and `threat_defusal` (D98), not
`lmp_enabled`/`singular_multicut_enabled`'s already-shipped-default-on
pattern. When `false`: `alpha_beta.rs` never writes
`static_eval_stack`, `improving` is unconditionally `true`, and both
pruning sites reduce to their exact pre-D114 formulas — this is a real,
tested invariant (`test_lmp_threshold_improving_true_matches_pre_d114_table`,
`test_futility_margin_improving_matches_pre_d114_formula`,
`test_static_eval_stack_untouched_when_improving_disabled`), not just an
assertion in a doc comment. New UCI option `ImprovingHeuristic` (check,
default false), same `EngineState` → `cmd_go` → `h_info`/`main_info`
threading pattern as every other Phase 26-family toggle.

**Not done, deliberately out of scope for this change**: no Texel
tuning of the new thresholds/margin (these are search pruning constants,
not eval weights — same status every other LMP/futility constant has
always had); no SPRT-style Elo validation yet (needs a real
`uci_match_runner.yml` A/B, same open item every other unproven Phase
26/27/28 toggle already has); the non-improving table's exact halving
ratio is a reasonable starting guess following Stockfish's own
convention, not independently derived or tuned for Pet Dragon's specific
eval/search balance.

**Context**: prompted by a 4-game external bench (Stockfish skill 10 vs.
Pet Dragon skill 20, both 100ms and 1000ms/move) that Pet Dragon lost
4-0 regardless of color or think time. The accompanying report's own
retune recommendations pointed at node-count instrumentation and
SEE-gating LMP/singular margins on capture-adjacent nodes — neither of
which is what D114 does. D114 addresses the report's separately-noted
"search depth is volatile within a game... more than any single blunder,
the most consistent signal" observation, not the specific one clean
tactical blunder the report found (`e2b5` in its Match 3) — that stays
open as a possible future SEE-gating investigation, not resolved by
this change.

---

## D115 — ImprovingHeuristic A/B: Flat at n=200, No Measurable Effect at Current Thresholds. Parked Off (Session 117)

**Result**: Gokul ran the `uci_match_runner.yml` A/B directly at
200 games (skipped the recommended 20-game first look, went straight to
confirmation scale) — Engine A (`main`, `ImprovingHeuristic value true`)
vs. Engine B (`main`, default `false`), seed 80000, 100ms/move. **A
40 wins, B 39 wins, 121 draws — 50.2%, +1.7 Elo.** Flat, well inside
noise for a 200-game sample — same conclusive shape D77 reached for
`NullMoveKingGuard` (50.5%, +3.5 Elo), reached in one run instead of two
this time since Gokul ran at confirmation scale from the start.

**Conclusion**: at the current thresholds (`LMP_THRESHOLDS_NON_IMPROVING`
roughly half of `LMP_THRESHOLDS_IMPROVING`; futility margin's constant
term dropping from 200 to 100 when non-improving), the improving flag
has no measurable strength impact — positive or negative — in self-play.
This isn't evidence the underlying idea is wrong; both the halving ratio
and the futility-margin delta were hand-picked starting guesses (D114)
following Stockfish's own convention, not independently derived or
tuned for Pet Dragon's specific eval/search balance. It's equally
possible the *mechanism* (static eval two plies back) doesn't carry much
independent signal for Pet Dragon specifically, or that the *thresholds*
just aren't scaled right yet — this result can't distinguish the two.

**Decision: park it. `ImprovingHeuristic` stays default `false`,
mechanism stays in the code (real, tested, zero-cost when off, already
merged and CI-confirmed green — Session 116).** Not reverting/removing
the option — same reasoning D77 gave for `NullMoveKingGuard`: a real,
specific idea, implemented and validated, that turned out not to matter
*at these particular thresholds*. Re-tuning the threshold/margin deltas
(rather than the flat "half" ratio D114 hand-picked) is possible future
work if this gets revisited, but not scheduled.

**ROADMAP Phase 31.2: closed as "implemented, tested, no measurable
effect at current thresholds, parked off."** 31.3 (SEE-gating LMP/
singular margins on capture-adjacent nodes, from the bench report's
`e2b5` finding) remains open and untouched — different mechanism,
unaffected by this result either way.

---

## D116 — Fifty-Move-Rule Draw No Longer Overrides Checkmate (Session 118, external review finding #1)

**Decision**: Fixed a real, current correctness bug surfaced by an
externally-authored code review Gokul uploaded and asked to have
verified before acting on ("double verified"). Directly re-checked
against live `main` source before trusting the report (this session's
own earlier mistake with a stale `ENGINE_ARCHITECTURE.md` claim made
that discipline non-negotiable this time) — 5 of the report's 8
findings were spot-verified line-for-line against current source;
this is the first, and highest-severity, one fixed.

**The bug**: `alpha_beta_with_excluded()`'s fifty-move-rule check
(`pos.halfmove_clock >= 100`) fired unconditionally and returned
`draw_score()` immediately — with no check for whether the side to
move was simultaneously checkmate. Checkmate detection only happens
later, after move generation, which this check preceded and
short-circuited. A position that is both `halfmove_clock >= 100` and
checkmate was scored as a dead draw instead of a forced mate, at any
node in the tree (not just the root) — narrow in practice (needs 50
full moves with no capture/pawn move landing exactly on mate) but real
in long technical endgames and hand-constructed/FEN-loaded positions.

**The fix**: hoisted `let in_check = pos.in_check(pos.side_to_move)`
above the draw-detection block (pure reordering — the position is
read-only in that stretch, nothing between the old and new call sites
mutates it) and guarded the fifty-move check exactly like Stockfish's
own `Position::is_draw()`:
`st->rule50 > 99 && (!checkers() || MoveList<LEGAL>(*this).size())`.
Ported directly: only take the draw path when not in check, or in
check but `generate_moves(pos)` isn't empty. `generate_moves()` is
called an extra time only on the rare path where both conditions
(halfmove_clock ≥ 100 AND in check) hold simultaneously — negligible
cost given how rarely that combination arises.

**Why repetition and insufficient-material (the other two draw checks
in the same block) didn't need the same guard**: repetition can't
coincide with checkmate — if this exact position (same hash) were
checkmate now, it would have been checkmate the first time it
occurred too, ending the game before it could repeat. Insufficient
material can't coincide with checkmate by definition — a side with
insufficient mating material cannot deliver mate. The fifty-move count
is the only one of the three fully independent of whether the position
happens to be checkmate.

**Tests added**: `test_fifty_move_rule_does_not_override_checkmate`
(a real R-vs-k back-rank mate position with `halfmove_clock = 100` —
before this fix, returned `DRAW_SCORE`; after, returns a mate-range
score) and `test_fifty_move_rule_still_draws_when_not_checkmate` (same
clock, side to move in check but with a legal escape — confirms the
guard's other branch still reaches the normal draw path, not a false
mate). Both hand-verified square-by-square (including the "king can't
step along the checking ray" x-ray rule for the mate position's f8/h8
squares) before being trusted, not just assumed correct from the FEN
string. The two pre-existing fifty-move tests
(`test_fifty_move_rule`/`test_fifty_move_rule_with_contempt`) are
unaffected — neither position is check, so both take the `!in_check`
branch exactly as before.

**Not done in this pass**: findings #2-#8 from the same review remain
open — Gokul asked to fix all 8 in priority order (#1 → #3 → #4 → #2 →
#5 → #6 → #7, #8 stays parked/inert since it's gated behind a
default-0 setting), this entry covers #1 only. See ROADMAP.md Phase 32
for the tracked list.

---

## D117 — Recapture Extension Fixed AND Wired Into Live Search for the First Time, Gated (Session 119, external review finding #3)

**Context**: while scoping the fix for review finding #3
(`is_recapture()` "extends nearly every capture"), discovered the
finding's own severity framing was wrong in an important way: its only
caller, `pruning::extension()`, is **never called anywhere in the live
search** — verified by grepping the entire repo, the only call site was
`extension()`'s own unit test. `alpha_beta.rs` has a separate, correct,
always-live check-extension (`if in_check { depth += 1; }`); recapture
and passed-pawn-push extension have never run in a real Pet Dragon game
at all. Surfaced this to Gokul before proceeding rather than silently
"fixing" a function whose bug had zero actual play-strength impact —
asked how to handle it now that the ground truth was different from
the report's framing. Gokul chose: fix the bug **and** wire the
mechanism into the live search for the first time, gated.

**The bug** (as the report described, confirmed accurate): the pre-fix
`is_recapture(pos, mv)` only checked "is `mv` a capture, and is there
currently an enemy piece on its destination square" — it never looked
at the previous move at all, so it would have returned `true` for
essentially any capture, not genuine recaptures specifically, had it
ever been reachable.

**The fix**: `is_recapture()` now takes `prev_move: Move` and requires
`mv.kind.is_capture() && prev_move.kind.is_capture() && mv.to ==
prev_move.to` — same simplification Stockfish itself uses for its own
recapture check (doesn't special-case en passant's one-rank offset,
same known/accepted imprecision as the reference implementation, not
unique to this fix). Pulled the recapture + passed-pawn-push logic out
of `extension()` into its own `recapture_and_passed_pawn_extension()`
function so the (still-unused-as-a-whole) `extension()` and the new
live call site share one implementation, deliberately not repeating
the should_apply_lmr-vs-inline-duplicate pattern review finding #4
flagged elsewhere in the same codebase.

**The wiring — deliberately scoped narrower than the whole bundled
`extension()` function**: only the recapture component was activated
in `alpha_beta.rs`'s move loop, not passed-pawn-push extension.
Passed-pawn-push wasn't flagged as buggy by the review and was never
part of what Gokul asked to fix — it stays real, correct, and
unreachable, same status it's always had, pending a separate decision
if it's ever wanted. New `move_ext` computation in the move loop:
applies to **any** move (not just the TT move, unlike the existing
singular/multi-cut extension), gated behind
`SearchInfo::recapture_extension_enabled` (default `false`), combined
with whatever `tt_move_extension` already contributed and capped
together at `MAX_EXTENSION`/`-2` (the same floor/ceiling the singular-
extension logic already respects on its own).

**Rollout discipline**: same shape as every Phase 26+ toggle —
`SearchInfo`/`EngineState` field, `ImprovingHeuristic`-style UCI option
(`RecaptureExtension`, check, default false), full `cmd_go` threading
into `h_info`/`main_info`. When `false` (default): zero extra
computation in the move loop, `move_ext` for any non-TT move stays
exactly `0`, byte-identical to before D117.

**Tests added**: 4 targeted `is_recapture()` unit tests (genuine
recapture, wrong-square capture — the exact pre-fix bug scenario,
previous move wasn't a capture, this move isn't a capture), 3 for
`recapture_and_passed_pawn_extension()` (fires for a recapture,
respects the `depth <= 4` cutoff, zero for an unrelated capture), plus
the standard 3-test toggle template (defaults false, parses true/false,
`cmd_go` actually threads it into the real search `SearchInfo`) and 2
search-level sanity checks (completes safely on the start position;
completes safely on a position specifically constructed to have a
pending exchange on d5, exercising the real code path rather than just
proving the toggle doesn't crash on an empty board).

**Not done**: no SPRT-style A/B yet — needs its own
`uci_match_runner.yml` run before any default-on consideration, same
open item every other unproven Phase 26+ toggle carries. Passed-pawn-
push extension remains unwired, out of scope. ROADMAP Phase 32 next:
32.3 (review finding #4, dead `should_apply_lmr()` vs. the inline LMR
gate missing its killer-move/TT-move guards).

**Addendum (Session 120)**: ran a 20-game `uci_match_runner.yml` check
(`RecaptureExtension value true` vs. `value false`, seed 90000,
100ms/move) — 3-3-14, 50.0%, -0.0 Elo. Confirms the wiring works
functionally: 20 games completed cleanly with no crashes, hangs, or
illegal-move errors, which is itself meaningful given this was the
mechanism's first time ever running in a real search. Dead flat at
n=20, same as every other toggle's first look — not enough sample to
conclude neutral vs. masking a small effect either way. Gokul chose to
defer the n=200 confirmation for now rather than run it immediately —
tracked as open work (a real SPRT-style A/B for `RecaptureExtension`),
not blocking anything else in Phase 32.

---

## D118 — LMR Gate Now Calls should_apply_lmr() Directly, Fixing the Missing Killer/TT-Move Guards — Shipped Live, Not Gated (Session 120, external review finding #4)

**Decision**: Fixed review finding #4 by replacing `alpha_beta.rs`'s
inline LMR gate (`depth >= MIN_DEPTH_LMR && moves_tried >= 3 &&
is_quiet && !in_check && !gives_check`) with a direct call to
`pruning::should_apply_lmr()`, which already existed, was already
fully unit-tested, and already correctly excluded killer moves and the
TT move from reduction — the bug was specifically that nothing ever
called it; the inline duplicate silently omitted those last two checks
rather than the tested function being wrong. Confirmed via full-repo
grep before this fix (as with finding #3) that
`should_apply_lmr()`'s only caller before this session was its own
unit tests.

**Why this shipped directly, not behind a new gated toggle** (unlike
D114's `ImprovingHeuristic` or D117's `RecaptureExtension`): those two
introduced genuinely new mechanisms that had never run in a real Pet
Dragon game before. This is different — excluding the TT move and
killer moves from LMR is standard practice (Stockfish does both), and
`should_apply_lmr()` already existed as the clearly-intended, fully-
tested canonical gate; the bug was a wiring omission, not a new idea
being introduced for the first time. Treated the same way D116's
fifty-move-rule fix was: a correctness/consistency fix, shipped live.

**Given LMR's reach** (fires on nearly every non-first move at
`depth >= MIN_DEPTH_LMR`, far more central than D116's narrow edge
case), this is a broader behavior change than D116 even though it
isn't gated — worth a closer look at CI and, ideally, a quick sanity
A/B via `uci_match_runner.yml` after confirming green, the same way
Gokul just ran a functional check for D117's `RecaptureExtension`, even
though this isn't a new toggle to flip on/off (there's no "off" state
to compare against — the fix is unconditional). A before/after
comparison would need the pinned pre-D118 commit as one side, not a
`setoption`-toggleable A/B.

**Also removed**: the now-unused `MIN_DEPTH_LMR` import in
`alpha_beta.rs` (the depth check now lives entirely inside
`should_apply_lmr()`, referenced via `crate::search::MIN_DEPTH_LMR`
internally — the caller no longer needs its own copy of the constant).

**Tests added**: `test_lmr_not_applied_to_tt_move` (pruning.rs — the
one direct case the pre-fix wiring bug missed; `should_apply_lmr()`
itself was always correct here, this closes a gap in its own test
coverage rather than testing the fix per se) and
`test_search_completes_after_lmr_gate_now_calls_should_apply_lmr`
(alpha_beta.rs — depth-7 search sanity check, deep enough to actually
exercise LMR, killers, and a TT move across iterative deepening, not
just prove the toggle-free code path doesn't crash on an empty board).

**Not done**: no dedicated Elo measurement of this specific fix yet —
see the A/B note above. ROADMAP Phase 32 next: 32.4 (review finding
#2, duplicate `MATE_THRESHOLD`).

---

## D119 — Duplicate MATE_THRESHOLD Constant Removed, TT Now Uses the Shared Value (Session 121, external review finding #2)

**Decision**: Fixed review finding #2. `tt/mod.rs` defined its own
`TranspositionTable::MATE_THRESHOLD = 30_000`, entirely independent of
`search/mod.rs`'s `MATE_THRESHOLD = 900_000` — used by `score_to_tt()`/
`score_from_tt()` to decide whether a score needs ply-relative
adjustment before/after TT storage. Removed the local constant
entirely; both functions now reference `crate::search::MATE_THRESHOLD`
directly (both modules are top-level siblings under the crate root, so
no import-cycle risk — confirmed against `lib.rs`'s module list before
making the change).

**Real-world impact, as assessed when this was first flagged
(Session 118, ROADMAP Phase 32.4)**: probably close to zero in
practice. Pet Dragon's eval essentially never reaches scores anywhere
near 30,000 in real games, so the report's own "ordinary scores in
[30000, 900000) get corrupted" scenario was mostly theoretical.
Fixed anyway on correctness/hygiene grounds — a duplicated magic
number with two very different values for the same concept is a real
landmine for future code even where today's practical blast radius is
small, and the fix itself is small, low-risk, and unambiguous (there's
only one correct value; unlike D118, there's no judgment call about
whether to gate this behind a toggle).

**Also removed**: `TranspositionTable::MATE_THRESHOLD` as a public API
— confirmed via full-repo grep that its only reference outside
`tt/mod.rs` itself didn't exist (only the module's own test used it),
so removing it is not a breaking change for any other file.

**Tests**: updated the existing `test_mate_score_adjustment` to use the
shared constant, and added
`test_score_to_tt_no_longer_uses_stale_local_threshold` — a direct
regression guard using a score (50,000) that sits exactly in the gap
between the old wrong threshold and the real one, confirming it now
passes through `score_to_tt()` unadjusted instead of being
misclassified as a mate score.

**ROADMAP Phase 32 status after this fix**: 32.1 (D116), 32.2 (D117),
32.3 (D118), 32.4 (D119) all done. Remaining: 32.5 (packed mg/eg score
sign-extension bug, finding #5), 32.6 (SEE missing illegal-king-
recapture guard, finding #6), 32.7 (iterative.rs sentinel
misclassifying a real score of 0, finding #7) — none of these three
have been independently re-verified against live source yet, unlike
#1-#4 and #8.

---

## D120 — Packed mg()/eg() Sign-Extension Bug Fixed at the Source — Confirmed Empirically, Affects the Live Eval Pipeline, Not Just Isolated Terms (Session 122, external review finding #5)

**Decision**: Fixed review finding #5, and it turned out to be more
significant than its initial "not independently re-verified" status
(ROADMAP Phase 32) suggested — the most impactful fix in Phase 32 so
far.

**The bug**: `eval/material.rs`'s `s(mg, eg) -> i64` packs a tapered
score as `(mg << 32) + eg`. The old `mg(score) -> i32` extraction
(`(score >> 32) as i32`, a plain arithmetic right shift) doesn't
account for `eg`'s sign-extension borrowing into the mg half's own
bits whenever `eg` is negative. **Verified empirically, not just
reasoned about**: swept 200,000 random `(mg, eg)` pairs — the old
`mg()` was wrong on 99,937 of them (~50%, exactly the cases with
`eg < 0`), always off by exactly 1 (too low). `eg()` itself
(`score as i32`, a plain low-32-bit truncation) was correct in 100% of
the same cases — the bug is specific to `mg()`.

**Scope discovery — this is NOT limited to isolated eval terms**:
while investigating, found `texel/weights_f64.rs` already had a
hand-rolled workaround for a version of this bug in `From<i64> for S`,
with a comment claiming `mg()` "is only correct once summed" — i.e.,
safe to use on an accumulated total across many eval terms, unsafe
only on one un-summed literal weight. **That claim is false.**
Verified by simulating an accumulated sum of 5 terms
(`Σ mg_i = 40, Σ eg_i = -43`): the old `mg()` applied to the *summed*
packed value still returned 39, not 40 — the bug depends purely on the
sign of the final packed value's low 32 bits, whether that value
represents one term or a total. Since `taper()` — which every real
position's eval score passes through — calls `mg()` on exactly this
kind of accumulated total, **this means the bug was reachable from the
live evaluation pipeline itself**, not just from decoding individual
un-summed weights, whenever a position's total accumulated eg score
happened to be negative. Given how common negative eg values are
across dozens of tapered eval terms, this was plausibly live in a
meaningful fraction of real evaluated positions, each time shaving
1 centipawn off the middlegame-blended score.

**The fix**: `mg()` now uses the same technique Stockfish's own
`Score` packing uses — `(score.wrapping_add(0x8000_0000) as u64 >> 32)
as i32` — add half the low half's range before shifting
(`wrapping_add`, not `+`, so this can't panic in debug builds even at
`i64`'s own extreme bounds, which `s()`'s realistic inputs never
actually approach), then shift as **unsigned** so no sign-extension
happens during the shift itself. Re-verified against the same
200,000-case sweep plus an exhaustive `[-500, -100, -1, 0, 1, 100,
500]²` grid in the new unit tests: zero failures. `eg()` is unchanged
— it was already correct.

**`weights_f64.rs` simplified as a direct consequence**: its
hand-rolled `From<i64> for S` workaround (subtract the known-correct
`eg`, then divide) is now unnecessary — confirmed mathematically
identical to the new, fixed `mg()` across 100,000 random cases before
removing it — simplified to call `mg()`/`eg()` directly like every
other caller, and its misleading "only correct once summed" comment
corrected rather than left in place to mislead a future reader.

**Also checked**: full-repo grep for any other packed-score
workarounds or duplicate `mg()`/`eg()`-style bit manipulation —
`tt/mod.rs`'s only `>> 32` usage is unrelated Zobrist hash-key
extraction, not another instance of this bug. `bin/texel_tune.rs`
calls the same `material::mg()`/`eg()` directly (no separate
workaround needed there) and benefits from the fix automatically.

**Tests added**: 5 in `material.rs` (negative-eg round-trip — the
exact pre-fix failure case, negative-mg, both-negative, a 7×7 sign-
combination sweep, and an end-to-end `taper()`-level check using the
function every real eval term actually calls, not just `mg()`/`eg()`
in isolation) and 1 in `weights_f64.rs` (confirms the simplified
`From<i64>` still round-trips the exact scenario its old workaround
existed to handle). None of the pre-existing tests in either file used
a negative `eg` value, so none needed updating — the fix changes no
previously-tested behavior, only previously-*untested* behavior.

**ROADMAP Phase 32 status**: 32.1/32.2/32.3/32.4/32.5 all done.
Remaining: 32.6 (SEE missing illegal-king-recapture guard), 32.7
(iterative.rs sentinel misclassifying a real score of 0) — neither
independently re-verified against live source yet.

---

## D121 — Two Unused-Import Warnings Cleared (release-only, cargo test never showed them) (Session 123)

**Decision**: Gokul uploaded CI logs from the multi-platform release
workflow ("Publish Release" — Windows/aarch64-macOS/x86_64-macOS/WASM
builds), green but with 2 compiler warnings on every native platform
build: `unused import: MoveKind` (`search/alpha_beta.rs:52`) and
`unused import: eg` (`eval/pawns.rs:30`).

**Why these never showed up in the regular `cargo test` CI job**: both
imports are used exclusively inside each file's own `#[cfg(test)] mod
tests { ... }` block (`MoveKind::Quiet` in various test fixture
`Move::new()` calls; `eg()` in a pawn-advancement test assertion). A
release build (`cargo build --release`, run by this separate
multi-platform workflow to produce the actual distributed binaries)
excludes `#[cfg(test)]` code entirely — the whole test module,
including its `use super::*` re-export of the parent module's
imports, doesn't exist in that compilation. So the top-level imports
were genuinely unused from a release build's point of view, while
being genuinely used from `cargo test`'s point of view — two different
compilation configurations, not a contradiction.

**Fix**: moved both imports from each file's top-level `use` statement
into a dedicated `use` line inside the file's own `mod tests` block
(next to that module's other test-only imports), so they're only
present in configurations where they're actually used. No behavior
change in either build configuration — purely a warning cleanup.

**Not investigated further**: whether other files in the codebase have
the same class of test-only-import-at-top-level pattern waiting to
warn the next time a release build runs — this session only fixed the
two the uploaded logs actually surfaced, not a proactive sweep for
more. Worth keeping in mind if a future release build log shows new
warnings.

---

## D122 — SEE King-Legality Guard Added — Confirmed Real, Frequently-Reachable Impact via Empirical Sweep (Session 124, external review finding #6)

**Decision**: Fixed review finding #6. `search/see.rs`'s exchange-chain
simulation (both `see()` and `see_value_of()`, which duplicate the same
loop) never checked whether a simulated king "recapture" would be
legal — a king can't move into a square still attacked by the
opponent. Confirmed real against live source: nowhere in the loop was
`attacker_kind == PieceKind::King` ever special-cased.

**Investigation discipline note**: initially hand-traced one concrete
position and found the bug's corrupted intermediate value got
"self-corrected" by the backward min-max propagation, due to the
King's placeholder SEE value (20,000) being so much larger than any
real piece that a subsequent "capture the king" step always dominates
and erases the corruption at the very next backward step. This could
have been mistaken for a general property (i.e., "the bug never
actually matters for the final result"). **It's not general** —
verified by an independent Python model of the exact algorithm swept
across 20,000 randomized, realistically-ordered attacker/defender
chains (a side's king can only ever be its own least-valuable-attacker
selection *last*, matching `least_valuable_attacker()`'s real
Pawn→Knight→Bishop→Rook→Queen→King search order): **~9% (1821/20,000)
produced a different final result**, several by large margins (e.g.
one case: 170 vs. the correct 500 — a 330cp swing). The one hand-traced
example that happened to cancel out was not representative.

**The fix**: after selecting the next attacker via
`least_valuable_attacker()`, if it's a king, check whether the
opposing side still has any attacker on the target square (via the
same `least_valuable_attacker()` call, occupancy-respecting — using
raw `attackers` instead would have been wrong, since that bitboard
alone doesn't exclude already-used non-sliding pieces the way
`least_valuable_attacker`'s combined `& occupancy` term does). If so,
the king's participation is illegal and the exchange sequence stops
there, same as running out of attackers entirely.

**Concrete verified test case**: White pawn captures a Black knight
(320) on e5; afterward, White's only remaining attacker on e5 is its
own king, Black's only remaining attacker is its own king — neither
can legally recapture, since each king still guards the square against
the other. Pre-fix, `see_value_of()` returned 220 (corrupted by
simulating Black's king illegally "recapturing" the pawn); post-fix,
correctly returns 320. A companion test shows this also flips a real
`see()` boolean result at threshold 300 (false → true). Position and
expected values verified against an independent Python re-
implementation of the exact algorithm before being trusted, not just
hand-derived once.

**Practical significance**: `see()`/`see_value_of()` are used for
capture move-ordering (`ordering.rs`) and various SEE-gated
pruning/extension decisions throughout search — given the fix changes
results in roughly 1 in 11 randomly-sampled realistic king-involved
endgame-ish exchange shapes, this is a real, non-trivial correctness
fix, not a corner case, though (unlike D120) it's specifically confined
to positions where kings end up as SEE-chain participants (later-game,
fewer-piece positions), not something touching every evaluated
position the way the packed-score bug did.

**ROADMAP Phase 32 status**: 32.1-32.6 all done. Remaining: 32.7
(iterative.rs sentinel misclassifying a real score of 0) — not yet
independently re-verified against live source.

---

## D123 — Wrong-Sentinel Score Bug Fixed, Extracted to a Testable Function — Phase 32 Complete, All 8 Findings Resolved (Session 125, external review finding #7)

**Decision**: Fixed review finding #7, the last open item in Phase 32.
Confirmed real against live source at the report's exact cited
location (`iterative.rs:154`, matched verbatim):

```rust
let score_for_result = if info.best_score.abs() > 0
    && info.best_score != -INFINITY {
    info.best_score
} else {
    best_score
};
```

`info.best_score`'s real "unset" sentinel is `-INFINITY` (set in
`SearchInfo::reset_for_search()`) — the extra `.abs() > 0` clause
wrongly treated a genuinely drawn root position (a completely
legitimate score of exactly `0`) as if it were invalid, silently
falling back to the local `best_score` tracker instead. That local
variable only updates when `info.best_move != Move::NULL` for a given
depth (see the loop a few lines above), so on the narrow edge case
where a completed depth left it `Move::NULL`, the fallback would be
stale — from the *previous* depth — while `info.best_score` correctly
held the current depth's real `0`.

**Severity assessment matches the report's own "Low" label** — unlike
findings #3/#4/#5/#6, which all turned out different (better or worse)
than initially framed once independently checked, this one's own
framing held up: harmless in the common path (both variables agree),
narrow trigger condition (`info.best_move == Move::NULL` on a
completed depth is itself rare), and the report's own suggested fix
was exactly correct — didn't need any independent empirical sweep the
way #5 and #6 did to establish real-world reach, since the bug is
purely a value-selection logic error with an unambiguous correct
answer, not something whose downstream impact depends on how a
separate algorithm (like SEE's backward min-max) happens to propagate
it.

**Fix**: extracted the inline expression into its own function,
`choose_result_score(info_best_score, fallback_best_score) -> i32`,
matching this session's established pattern (D114's
`futility_margin()`, D117's `recapture_and_passed_pawn_extension()`,
D118's reuse of `should_apply_lmr()`) rather than leaving fragile
inline logic buried in a large loop — makes the exact fixed condition
directly unit-testable with plain `i32` inputs, no need to construct a
full `alpha_beta` search or a real position to exercise the edge case.
Dropped the `.abs() > 0` clause entirely; the only condition that
disqualifies `info_best_score` is being the real sentinel.

**Tests added**: 3 — the exact pre-fix bug scenario (`info_best_score
= 0` must be trusted, not treated as unset), the real sentinel case
(`-INFINITY` must fall back), and a sanity check for the ordinary
non-zero, non-sentinel path.

**Phase 32 is now complete — all 8 external review findings resolved**:

| # | Finding | Status | Real-world severity once checked |
|---|---------|--------|-----------------------------------|
| 1 | Fifty-move rule overrides checkmate | Fixed (D116) | Real, narrow edge case |
| 2 | Duplicate `MATE_THRESHOLD` | Fixed (D119) | Real, near-zero reachability |
| 3 | `is_recapture()` ignores prev_move | Fixed + wired live (D117) | Was 100% dead code before the fix |
| 4 | Dead `should_apply_lmr()` vs. inline gate | Fixed (D118) | Was 100% dead code before the fix |
| 5 | Packed mg/eg sign-extension bug | Fixed (D120) | **Highest impact — live in real eval, ~50% of terms** |
| 6 | SEE missing king-legality guard | Fixed (D122) | Real, ~9% of realistic exchange shapes |
| 7 | Wrong sentinel misclassifies score 0 | Fixed (D123) | Real, narrow — matches report's own "Low" label |
| 8 | NNUE centipawn conversion unverified | Confirmed inert | Zero — gated behind default-0 blend weight |

Across this whole pass, the review's own severity labels proved
accurate for exactly 2 of 8 findings (#7, #8) — the other 6 all turned
out to be either more or less impactful than first framed once
independently verified against live source and, in two cases (#5, #6),
an empirical sweep rather than a single hand-derived example. This is
the core lesson of Phase 32 as a whole: verify against source and,
where a bug's downstream impact depends on how another algorithm
propagates it, verify empirically too — neither a report's own
confidence nor a single hand-traced example is sufficient on its own.

---

## D124 — D118/D120 Pinned-Ref Elo Check: +5.2 Elo, Within Noise. Accepted as Neutral, No Further Runs (Session 126)

**Decision**: Ran the pinned-ref `uci_match_runner.yml`-adjacent
"UCI Pinned-Ref Match (D36)" workflow to check D118 (LMR gate fix) and
D120 (packed mg/eg score fix) for a real Elo effect — neither had a
`setoption` toggle to A/B, since both shipped live as unconditional
correctness fixes, so this needed the pre-fix commit pinned directly
rather than a `setoption`-based comparison.

**Process note — a real mistake caught and fixed mid-session**: the
first attempt used `edb0e9a20d` (a 10-character abbreviated SHA) as
`pre_tuning_ref`. `actions/checkout` couldn't resolve that as a branch
or tag ref, failed after 3 retries, and — this is the concerning
part — **the workflow silently fell back to using `main` for both
sides** rather than hard-failing the job. The resulting "clean" 200-
game run (50.0%, -0.0 Elo, both engines labeled `(main)`) looked
exactly like a real neutral finding and would have been recorded as
one if Gokul hadn't noticed both engine labels read `main`. Re-ran with
the full 40-character SHA
(`edb0e9a20d8579b3b31a99efdf627acd80140826`) — confirmed correct labels
this time (`Engine A: A (edb0e9a20d8579b3b31a99efdf627acd80140826)`,
`Engine B: B (main)`). **Lesson for future pinned-ref runs in this
repo: always give the full SHA, never an abbreviated one, and always
sanity-check the reported engine labels before trusting the result.**
Separately flagged to Gokul (not fixed this session, his call): the
workflow itself probably should hard-fail on a checkout error instead
of silently substituting `main` — a real CI-robustness gap, since it
makes a broken run indistinguishable from a genuine neutral result at
a glance.

**Real result** (edb0e9a20d8579b3b31a99efdf627acd80140826 = pre-D118/
D120, vs. `main` = has D118+D120 plus the unrelated D119/D121/D122/D123):
36-39-125, A(pre-fix) 49.2%, **+5.2 Elo for main**. Within noise for
n=200 (typical 95% CI at this sample size is roughly ±50 Elo) —
directionally consistent with the fixes being neutral-to-mildly-
positive, not evidence of a real regression, but not a confirmed
strength gain either.

**Decision: accept as neutral, no further runs.** Gokul's call, given
the options (n=800 for a tighter read, or split D118 vs. D120 into
separate pinned-ref runs to isolate which fix drives the small
positive lean) — chose to move on rather than spend more compute
narrowing a result that's already not concerning in either direction.
Both fixes stay as-is (unconditional, no toggle, already merged and
CI-confirmed). This closes out the last open item from the Phase 32
review-response arc.

---

## D125 — RecaptureExtension Default Flipped to true, Skipping the Standard SPRT Step (Session 127)

**Decision**: Gokul directly instructed: flip `RecaptureExtension`'s
default to `true`, explicitly declining a re-run or further
validation ("No rerun. Make RecaptureExtension's default on").

**This departs from this project's own established rollout discipline**
(D75's `NullMoveKingGuard`, D98's `ThreatDefusal`, D114's
`ImprovingHeuristic`, and D117's own original framing all followed the
same pattern: new/unproven technique ships gated off, earns a
default-on flip only after a dedicated SPRT-style A/B). The only
signal available for `RecaptureExtension` at flip time is D117's own
n=20 functional check (Session 120) — 3-3-14, 50.0%, -0.0 Elo. That
confirms the mechanism works (no crashes on its first live run), not
that it improves strength; n=20 is far too small a sample to draw any
Elo conclusion either way, positive or negative. Flagged this once,
plainly, before making the change — Gokul's explicit, repeated
instruction stands, and this is his call to make as the project owner,
not something requiring further pushback from Claude once stated
clearly.

**What changed**: `SearchInfo::recapture_extension_enabled` (default
`false` → `true`), `EngineState::recapture_extension_enabled` (same),
the `RecaptureExtension` UCI option's advertised default (`type check
default false` → `default true`), and every test that asserted the old
default — `test_recapture_extension_enabled_defaults_to_false` →
`_defaults_to_true` (`alpha_beta.rs`), same rename in `main.rs`. The
mechanism's own logic (`pruning::recapture_and_passed_pawn_extension`,
the move-loop wiring in `alpha_beta.rs`) is completely unchanged —
this is purely a default-value flip, not a behavior change to the
extension logic itself.

**Risk carried forward, explicitly, not silently**: `Recapture
Extension` now runs in every game by default with no dedicated Elo
data behind it. If a future bench or self-play run surfaces a strength
regression, this default is the first place to look. Reverting is a
one-line change (flip the three `true`s back to `false` plus the UCI
option string and the two test names) — recorded here specifically so
that's easy to find later.

---

## D126 — 30.5 (Async WASM UCI) Scoped, Deferred — SharedArrayBuffer Ruled Out for This Deployment Target (Session 128)

**Decision**: Gokul asked to start 30.5 (the documented async/threaded
WASM limitation — `stop` can't interrupt `go`, `go infinite` clamped
to 30s, no live `info depth` streaming, no ponder). Before writing any
code, surfaced a hard constraint that determines the viable approach:

**SharedArrayBuffer-backed real threading — the standard fix (same
shared-memory-plus-polled-stop-flag pattern native `main.rs` already
uses, and how newer threaded Stockfish.js builds work) — is not viable
for this project's deployment target.** It requires
`Cross-Origin-Opener-Policy`/`Cross-Origin-Embedder-Policy` response
headers, and **GitHub Pages cannot set custom HTTP headers at all** —
it's a static host with no server-side config surface. There's also a
subtler point worth recording: a plain Web Worker *alone*, without
shared memory, doesn't fix `stop` either — a Worker is still a single-
threaded JS environment, and a long synchronous WASM call blocks its
message queue exactly as completely as it blocks the main thread's.
"Just move it to a Worker" is not sufficient on its own.

**The approach identified as viable without special headers:
cooperative chunking.** Break `go` into repeated short `uci_command`
calls instead of one long synchronous one — search one iterative-
deepening depth at a time, return control to JS after each depth (with
a live `info depth N ...` line), let JS decide whether to request the
next depth or send `stop`. Fits the existing `thread_local! SESSION`
design well, since session state already persists across calls by
design. Would give real (though depth-granularity, not instant) `stop`
responsiveness, genuinely open-ended `go infinite`, and live depth
streaming — without any deployment infrastructure change.

**Not started.** Gokul confirmed the chunked approach as the right
direction in principle but chose not to scope or begin implementation
this session ("too big" / "later") — this is a real redesign of `go`'s
execution model in `uci_wasm.rs`, not a small patch. Recorded here so
the SharedArrayBuffer/GitHub-Pages constraint and the chunking proposal
don't need to be rediscovered from scratch whenever this is picked back
up. ROADMAP 30.5 left open, unchanged in status, with a pointer to this
entry.

## D127 — Second External Review Round: 3 Confirmed Bugs Fixed (`pet-dragon-bug-report.md`), Upgrade-Plan Review Parked as Backlog (Session 129)

**Context**: Gokul uploaded two independent external documents this
session: a focused bug report covering the full ~31,000-line codebase
(`src/bitboards` through `src/bin/` tools and both UCI front ends), and
a separate, much larger search/eval "Full Review & Upgrade Plan"
(NNUE re-validation, LMR formula enrichment, missing threat sub-terms,
razoring, etc. — no bugs, source-review-only, explicitly unverified
against the real API surface, no SPRT testing performed). Asked to
handle "Bugs then upgrade" — decision covers the bug report; the
upgrade plan is parked as an unscheduled backlog item, same treatment
Phase 32's non-bug suggestions got (see the note at the end of Phase
32 in ROADMAP.md).

All three bug-report findings were spot-verified against live `main`
source (grep + direct file reads) before trusting them, matching the
discipline established in D116 for the first external review round.
All three checked out exactly as described — this report's hit rate
was notably higher than the first external review's (D116: 5/8 findings
verified, and D117/D118/D119/D120/D122/D123 later found 6/8 of *those*
were more/less severe than originally framed once independently
checked). No such reframing was needed here.

**Bug 1 — quiescence never considers quiet pawn promotions.**
Confirmed: `movegen::pawns::generate_pawn_captures()` called only
`generate_pawn_diagonal_captures()` and `generate_en_passant()`;
`add_promotions()` (the quiet-promotion helper) was only ever reached
via `generate_pawn_pushes()`, part of full move generation, never the
capture-only path quiescence draws from. Compounded by
`alpha_beta.rs::quiescence()`'s quiet-checks section explicitly
skipping `mv.kind.is_promotion()` on the (false) assumption promotions
were already covered by the captures list. **Fix**: added
`generate_pawn_quiet_promotions()` (single push landing on the
promotion rank only — double pushes structurally can't reach the
promotion rank, guarded by the existing range checks in
`generate_pawn_pushes()`), called from `generate_pawn_captures()`.
Verified no downstream logic needed changes: `see()` already handles a
zero-value "capture" target correctly (no special-casing needed for a
non-capturing move), and `ordering::score_captures()` already branches
on `mv.kind.is_promotion()` independent of whether `mv.captured` is
`Some`. 2 new regression tests in `movegen/pawns.rs`.

**Bug 2 — `push_game_history()` double-called in both match-runner
binaries.** Confirmed at `bin/match_runner.rs:182-183` and
`bin/uci_match_runner.rs:463-464` — both called
`pos.push_game_history()` immediately after
`pos.make_move_with_history()`, which already pushes internally.
Every move after the first double-counted its resulting hash in
`game_history`, tripping `is_threefold_repetition()`'s raw `count >= 3`
check a full occurrence early (2 real occurrences × 2 pushes = 4).
`uci_match_runner.rs` is wired into `build.yml`'s `regression-gate` CI
job — this was pulling every measured Elo delta (regressions and
improvements alike) toward 50% via spurious early repetition draws,
in both that CI gate and any manual A/B run through `match_runner.rs`.
**Fix**: deleted the redundant line in both files; `selfplay.rs` and
`texel_gen.rs` were already correct (verified via grep) and untouched.
The bug report's related lower-severity note (`selfplay.rs` never
pushes the starting position to `game_history`, unlike the two match
runners) was **not** addressed this session — cosmetic-only
consistency issue, not a correctness bug, out of scope for "fix the
bugs" as scoped.

**Bug 3 — Syzygy tablebase probing never checks castling rights.**
Confirmed: zero references to castling anywhere in `syzygy/mod.rs`;
neither `alpha_beta.rs`'s interior WDL probe nor `main.rs`'s root DTZ
probe gated on it either. Syzygy tables don't encode castling rights
at all (fixed assumption of the underlying retrograde-analysis
format) — every mainstream Syzygy-consuming engine gates every probe
on zero remaining rights. Worth calling out explicitly:
**`ENGINE_ARCHITECTURE.md` §5 previously asserted this was safe
("by the time few enough pieces remain for tablebase lookup, castling
rights are gone")** — that claim has no backing decision anywhere in
this file and no test; it's contradicted by the bug report's own
figure (~26% of games retain at least one castling right, no game
rule forcing rights to clear before material thins into TB range).
Docs get corrected alongside the code fix. **Fix**: added
`has_castling_rights(pos)` (checks `pos.castling.has_any()` for both
colors) as a private helper in `syzygy/mod.rs`, called at the top of
both `SyzygyProber::probe_wdl()` and `SyzygyProber::probe_root()` —
centralized in the prober itself (per the report's own suggestion)
rather than duplicated at each of the two call sites, so a future
third call site can't reintroduce the gap. 3 new regression tests.

**Files touched:** `src/movegen/pawns.rs`, `src/bin/match_runner.rs`,
`src/bin/uci_match_runner.rs`, `src/syzygy/mod.rs`.

⚠️ **Not yet CI-confirmed** — no local `cargo test` in this sandbox;
all four files passed manual brace/paren-balance checks.

**Next session start point:** Gokul commits all four files, confirms
`cargo test` green (8 new tests total: 2 in `movegen/pawns.rs`, 3 in
`syzygy/mod.rs`, plus the two match-runner files which have no new
tests of their own — their behavior change is only observable via a
real match run). Once confirmed green, move to the upgrade-plan
backlog per its own priority order (see the review document itself —
headline item is NNUE re-validation).

## D128 — CI Red on First Commit of D127: Flaky Syzygy Test Singleton Race, Fixed by Consolidating to One `new()` Call (Session 130)

**Decision**: Gokul uploaded a failing GitHub Actions log ("Test" job)
from committing the D127 files. `cargo test` reported 1 failure:
`syzygy::tests::test_probe_wdl_refuses_when_castling_rights_present`
panicked with `called Result::unwrap() on an Err value: "Syzygy init
error: AlreadyInitialized"`, while its sibling test
`test_probe_root_refuses_when_castling_rights_present` (added in the
same commit, structurally identical) passed.

**Root cause**: exactly the constraint already recorded in D17 —
`pyrrhic_rs::TableBases` is a process-wide singleton. Only the *first*
`SyzygyProber::new()` call across the whole test binary succeeds;
every subsequent call returns `Err(AlreadyInitialized)` regardless of
path validity. D127 added two new tests that each called
`SyzygyProber::new(...).unwrap()`, on top of the pre-existing
`test_syzygy_new_does_not_panic` (which also calls `new()`, but with
`let _ = ...`, so it never panics either way) — three call sites
total. `cargo test` runs tests concurrently by default, so which call
"wins" the singleton is a race with no fixed outcome; any test that
unwraps its own call is flaky by construction. This should have been
caught before committing — D17 is exactly the kind of Tier-2-adjacent
prior decision that a new test touching `SyzygyProber::new()` needs to
be checked against, and it wasn't.

**Fix**: consolidated all three `SyzygyProber::new()`-dependent tests
into one — `test_syzygy_prober_construction_and_castling_guard()` —
which is now the *only* call site for `SyzygyProber::new()` in the
whole test binary. It performs the original construction-doesn't-panic
check plus both castling-guard assertions (`probe_wdl` and
`probe_root` each return `None` for a position with remaining
castling rights) against the single successfully-constructed
instance. This removes the race outright rather than fixing one
specific failing ordering — a `.unwrap()` on the *only* call site in
the binary cannot lose a singleton race that no longer has a second
competitor. Net test count in `syzygy/mod.rs`: 5 (was 6 after D127;
consolidating three into one nets -2).

**Not yet CI-confirmed** — no local `cargo test` in this sandbox
(pyrrhic-rs/libc don't build in the sandboxed network anyway); fix
verified by direct reasoning about the singleton behavior D17 already
documented, plus a manual brace/paren-balance check. Should be
re-verified against a real CI run before trusting it's fully green.

**Next session start point:** Gokul commits `src/syzygy/mod.rs`
(replacing the version from D127) and re-runs CI. If green, proceed
to the upgrade-plan backlog per D127's note. If still red, get the new
log before making further changes — don't guess a second time.

## D129 — CI Red Again After D128 Fix: Bug 1's Original Fix Double-Generated Quiet Promotions in Full Movegen, Not Just Quiescence — Split Into a Separate Tactical Function (Session 131)

**Decision**: Gokul uploaded a second failing CI log, taken after the
D128 fix was committed. The Syzygy singleton race was gone (533/533
lib tests green, confirming D128 worked) — but a *different* test
failed: `tests/perft.rs::test_perft_kiwipete_depth4` returned the
wrong node count (expected 4,085,603).

**Root cause**: this traces back to the D127 fix for Bug 1, not D128.
`pawns::generate_pawn_moves()` (full pseudo-legal move generation —
used by perft, and by *every* normal search node, not just
quiescence) calls both `generate_pawn_pushes()` (which already adds
quiet promotions via `add_promotions()`) and `generate_pawn_captures()`.
D127's fix added quiet-promotion generation directly inside
`generate_pawn_captures()` so quiescence would see them — but that
function is shared: `generate_pawn_moves()` calls it too. The result
was every quiet promotion getting added *twice* to every ordinary
pseudo-legal move list in the engine — not a quiescence-only bug, a
full-movegen bug, corrupting perft counts and every normal search
node's move list. This is more severe than the original Bug 1 (which
only affected quiescence leaf nodes) and was introduced by the fix for
it. It slipped through because none of D127's own new tests exercised
`generate_pawn_moves()` or `generate_captures()` together with a
promotion in the same check — they tested `generate_pawn_captures()`
in isolation, which looked correct on its own.

**Fix**: split the quiet-promotion-inclusive behavior out of
`generate_pawn_captures()` into a new function,
`pawns::generate_pawn_tactical()` (captures + en passant + promotion
captures + quiet promotions). `generate_pawn_captures()` is reverted
to its pre-D127 behavior (no quiet promotions) and stays the pawn half
of `generate_pawn_moves()`, unchanged from how it always worked.
`movegen::generate_captures()` (the actual entry point
`alpha_beta.rs::quiescence()` calls) now calls
`pawns::generate_pawn_tactical()` instead of
`pawns::generate_pawn_captures()` directly. This isolates the new
quiet-promotion behavior to exactly the one caller that needs it,
leaving `generate_pawn_moves()`'s path completely untouched.

**Tests**: replaced the two D127 tests (which tested
`generate_pawn_captures()` for quiet-promotion inclusion — now the
wrong function to test that against) with tests against
`generate_pawn_tactical()` instead, and added three new regression
guards specifically targeting the double-generation shape of this bug:
`generate_pawn_captures()` must return 0 moves for a quiet-promotion-
only position (`movegen/pawns.rs`), `generate_pawn_moves()` must
return exactly 4 promotion moves not 8 for the same position
(`movegen/pawns.rs`), and `movegen::generate_captures()` /
`generate_moves()` must each independently show the promotion exactly
once (`movegen/mod.rs`, integration-level, the actual entry points
real code calls rather than the internal `pawns::` functions).

**Not yet CI-confirmed** — no local `cargo test` in this sandbox;
fix verified by manual brace/paren-balance checks and by tracing the
exact call graph (`generate_pawn_moves` → `generate_pawn_pushes` +
`generate_pawn_captures`, vs. `movegen::generate_captures` →
`generate_pawn_tactical`) to confirm no function is reachable from two
paths that both add the same quiet promotion.

**Process note**: this is the second CI-red round in a row from the
D127 commit (first D128, now this). Going forward, any fix that adds
behavior to a function shared between two call sites (here:
`generate_pawn_captures()` feeding both quiescence *and* full movegen)
needs the shared-caller graph checked before writing the fix, not
just the target call site — the same class of gap as D128 (checking
D17's singleton constraint) but for shared-function fan-in instead of
process-wide state.

**Next session start point:** Gokul commits `src/movegen/pawns.rs`
and `src/movegen/mod.rs` and re-runs CI, full test suite including
`tests/perft.rs` this time. If green, proceed to the upgrade-plan
backlog per D127's note. If still red, get the new log — don't guess
a third time; read the failing test's exact assertion and trace the
call graph before touching code.

## D130 — Upgrade-Plan Backlog Started: SPRT Infrastructure for 3 Experimental Flags (34.1) + ThreatByRook (34.2) (Session 133)

**Context**: Gokul confirmed CI green (Session 132) closing out Phase
33. Asked to start the upgrade-plan backlog (D127) with items #3 and
#4 from the review's own priority list — explicitly *not* #1 (NNUE
re-validation), which was flagged back to Gokul as conflicting with
already-tested project history (see D131 below) before any work
started, per this session's own recommendation in the prior turn.

**34.1 — SPRT infrastructure for `nonpawn_correction_enabled`,
`continuation_correction_enabled`, `improving_enabled`.** These three
`SearchInfo` bools all default `false` and are gated behind exactly
the kind of A/B validation `match_runner` already does for NNUE
weight (D23/D25/D26) — but `match_runner`/`match_runner.yml` only ever
varied NNUE blend weight, with no path to toggle these flags at all.
Extended both:
- `match_runner.rs`: new `ExperimentalFlag` enum (one of the three
  flags), `parse_flag()`/`parse_bool_arg()` for CLI parsing, and
  `resolve_weights()` — when a flag is under test, both engines' NNUE
  weight is **forced to 0%** regardless of the `weight_a`/`weight_b`
  inputs, so a mobile user who forgets to zero them doesn't
  accidentally run a two-variable (confounded) match. `play_one_game`
  now applies the flag to each mover's own `SearchInfo` via a new
  `ExperimentalFlag::apply()` method.
- `match_runner.yml`: 3 new `workflow_dispatch` inputs — `flag_name`
  (dropdown: none/nonpawn_correction/continuation_correction/
  improving), `flag_a`, `flag_b` (dropdown true/false, default
  false/true — testing the shipped `false` default against flipping
  it to `true`, the natural SPRT direction for an unproven flag).
  Fully mobile-usable, same as the existing weight-only inputs.
- Backward compatible: existing positional CLI args 1-6 unchanged, new
  args 7-9 are optional and default to the original pure-NNUE-weight
  behavior (`flag_name` empty/missing → `None`).
- 9 new unit tests in `match_runner.rs` covering `parse_flag`,
  `parse_bool_arg`, `resolve_weights`, `ExperimentalFlag::apply`, and
  the flag-carrying path through `play_one_game`.
- **Not yet run.** This session built the capability to test; it did
  not trigger a match or make a keep/revert call on any of the three
  flags. Next step is Gokul running the workflow three times (once per
  flag) from the Actions tab and reporting results back.

**34.2 — `ThreatByRook`.** Implemented per the review's §8.1 spec
almost verbatim — near-identical shape to the existing
`THREAT_BY_MINOR_BONUS` term, same double-counting argument already
accepted for it (see updated module doc comment in `threats.rs`).
`THREAT_BY_ROOK_BONUS = s(20, 12)`, scored only for rook-attacks-
*queen* (not rook-attacks-rook — a roughly even trade, weak signal,
matches Stockfish's own restriction). 2 new regression tests plus a
scoping test confirming rook-attacks-rook does NOT trigger the bonus
— both had to be built defending the attacking rook with a pawn to
avoid `UNDEFENDED_PENALTY` (same open-file mutual-attack shape)
confounding the sign of the assertion; this took a first-draft
mistake and a self-caught fix during this session (documented in the
test comments themselves) before landing on the correct FENs.

**Not yet CI-confirmed** — no local `cargo test` in this sandbox; both
changes verified by manual brace/paren-balance checks and, for
`threats.rs`, by hand-tracing the packed `s(mg,eg)` arithmetic through
each test FEN to confirm the expected sign before finalizing.

**Next session start point:** Gokul commits `src/bin/match_runner.rs`,
`.github/workflows/match_runner.yml`, and `src/eval/threats.rs`, and
confirms CI green. Once green, run the three flag-SPRT matches via the
Actions tab (34.1) and report results back — that decides whether any
of the three flags flips to `true` by default. Upgrade-plan backlog
continues from there; #1 (NNUE) still pending Gokul's call per D131.

## D131 — NNUE Re-Validation (Review Item #1) Flagged as Conflicting With Tested Project History, Not Started (Session 133)

**Context**: the external upgrade-plan review's headline recommendation
("NNUE shipped at 0%... re-validating and re-enabling it is very
likely the highest-return single change available") was flagged back
to Gokul, before any implementation work started, as based on reading
the code rather than the project's own Elo history:

- **D25**: 25%-weight NNUE tested head-to-head against pure HCE, 20
  games — **+338 Elo in favor of turning NNUE off**, not merely
  under-contributing.
- **D53/D55/D57/D58**: three separate follow-up attempts to fix the
  network (label smoothing, phase-balanced oversampling, and others)
  — best result unchanged, same ~+338 Elo gap for HCE.
- **D61**: Gokul's explicit call to shelve NNUE entirely — every
  remaining lever (feature redesign, GPU training pipeline,
  Stockfish-distillation data) is a genuine infrastructure investment
  per D58's own conclusion, not a quick retry.

So "re-validate and re-enable" would be repeating an already-negative,
already-repeated result, not recovering a forgotten feature. Gokul's
response: proceed with review items #3 and #4 instead (this session,
D130); no decision made on NNUE. **Recorded here so a future session
doesn't restart NNUE work from the review document's framing without
first reading this entry and D61** — if NNUE work is ever resumed, it
should start from D58's three reopening options, not from the review's
"just flip the weight back up" framing.

## D132 — CI Red From D130 Commit: ThreatByRook Missing From the Texel Tuner Mirror, Fixed Across the Whole Pipeline (Session 134)

**Context**: Gokul uploaded a failing CI log after committing the
D130 files. 537/538 lib tests passed; the one failure was
`texel::predict::tests::test_predict_matches_evaluate_after_moves`
(`predict()`/`evaluate()` mismatch mid-game at seed 4: predict=31
evaluate=51).

**Root cause**: `eval::threats::THREAT_BY_ROOK_BONUS` (added in D130's
34.2) is a real HCE eval term, but the Texel tuner's `predict()`
function computes its own independent, feature-based mirror of the
entire evaluator — every eval term has to be duplicated on the tuner
side (`texel::features::extract_features`, `texel::predict::predict`,
`texel::predict_f64` forward pass + gradient, `texel::weights::
TunableWeights`, `texel::weights_f64::TunableWeightsF64`) or
`predict()` silently drifts out of sync with `evaluate()`. D130 only
touched `eval/threats.rs` itself — none of the five `texel/*.rs`
mirror files. This is exactly the kind of cross-cutting-concern miss
D129's process note called out (checking a change's full caller/mirror
graph, not just its most obvious call site) — should have been caught
before committing, the same class of gap as D129, just in a different
subsystem (Texel mirroring instead of shared-function fan-in).

**Fix**: added `threat_by_rook` end to end, following the exact same
pattern `threat_by_minor` already established at every one of these
touch points:
- `features.rs`: `threat_by_rook_diff` field on `TexelFeatures`;
  `threats_side_raw()` extended to a 6-tuple with a rook-attacks-
  enemy-queen loop mirroring `eval::threats::threats_for_color`'s new
  loop exactly.
- `weights.rs` / `weights_f64.rs`: `threat_by_rook: i64` / `S` field,
  default `s(20, 12)` (must match `THREAT_BY_ROOK_BONUS` exactly),
  wired into `zero()`, `to_packed()`/`to_tunable_weights()`,
  `flatten()`/`unflatten()` (with `PARAM_COUNT` bumped 5→6 threats
  terms), and `From<&TunableWeights>`.
- `predict.rs` / `predict_f64.rs`: scored in both the integer forward
  pass and the f64 forward pass + gradient accumulation.
- `texel_tune.rs` / `texel_diag.rs`: the D35 tuning-pipeline pair that
  writes tuned weights out as a Rust snippet and reads them back in —
  both needed the new field too (`texel_diag.rs`'s
  `parse_tuned_weights()` constructs `TunableWeights { .. }`
  exhaustively, so missing this would have been a **compile error**,
  not just a test failure, on the next `cargo build` covering that
  binary). `EXPECTED_PAIR_COUNT` bumped alongside `PARAM_COUNT`.

**Process note (self-caught, not CI-caught)**: made the identical
mistake twice while hand-editing struct literals during this fix — a
`str_replace` meant to insert `threat_by_rook,` after
`threat_by_minor,` in `weights_f64.rs`'s `unflatten()` and again in
`texel_diag.rs`'s `parse_tuned_weights()` each accidentally deleted
the adjacent `tempo,` field instead of preserving it, because the
old/new string boundaries were drawn too tight around just the two
threat lines. Caught both by re-viewing the file immediately after
each edit rather than trusting the diff blind — worth remembering as
a general lesson: when inserting a field into a struct literal via
`str_replace`, include enough of the following context in `old_str`
to make an accidental adjacent deletion impossible, not just enough to
be unique.

**Not yet CI-confirmed** — no local `cargo test`/`cargo build` in this
sandbox; verified by (1) a full field-list diff of every
`TunableWeights { .. }` / `TunableWeightsF64 { .. }` literal against
the struct definitions (`texel_diag.rs`'s confirmed as an exact
39-field match after the fix), (2) manual brace/paren-balance checks
on all 7 touched files, and (3) tracing `test_predict_matches_evaluate_
after_moves`'s exact assertion back to confirm `threat_by_rook_diff`
is now populated and scored identically on both the `evaluate()` and
`predict()` sides.

**Next session start point:** Gokul commits all 7 files
(`texel/features.rs`, `texel/predict.rs`, `texel/predict_f64.rs`,
`texel/weights.rs`, `texel/weights_f64.rs`, `bin/texel_tune.rs`,
`bin/texel_diag.rs`) and re-runs full CI. If green, 34.1/34.2 are both
closed out — proceed to running the three 34.1 flag-SPRT matches from
the Actions tab. If still red, get the new log and check first whether
it's another Texel-mirror gap (unlikely, this was the last untouched
mirror file) before assuming a new class of bug.

## D133 — First 34.1 SPRT Batch: `improving_enabled` and `continuation_correction_enabled` Confirmed No-Signal, `nonpawn_correction_enabled` Inconclusive Pending More Games (Session 136)

**Context**: Gokul ran all three 34.1 flag-SPRT matches (200 games
each, 100ms/move, NNUE weight forced to 0% both sides per
`resolve_weights()`) and reported results:

| Flag | A (false) score | Elo diff (A vs B) |
|---|---|---|
| `improving_enabled` | 50.7% | +5.2 (favors off) |
| `nonpawn_correction_enabled` | 47.5% | -17.4 (favors on) |
| `continuation_correction_enabled` | 51.2% | +8.7 (favors off) |

**Noise bar used**: this project already has a concrete precedent for
what "within noise" means at this sample size — D124 explicitly called
a **+5.2 Elo** result at n=200 "within noise... accepted as neutral, no
further runs." Applying that same bar (not a new, looser one invented
for this decision):

- **`improving_enabled`**: +5.2 Elo — the *exact* magnitude D124 already
  called noise. **No change** — default stays `false`. No further runs
  planned; re-open only if a future change to the search elsewhere
  makes `improving_enabled` newly relevant to re-test.
- **`continuation_correction_enabled`**: +8.7 Elo — larger than the
  D124 precedent but still well inside a ~±25-35 Elo standard-error band
  at n=200. **No change** — default stays `false`. No further runs
  planned.
- **`nonpawn_correction_enabled`**: +17.4 Elo (favoring `true`/on) — the
  largest of the three, and in the direction of enabling it, but still
  not clearly outside the noise band on a single 200-game batch.
  **Inconclusive, not decided either way.** Requested a follow-up: 500
  more games at `seed_start=200` (fresh games, not a replay), to combine
  into a 700-game total before making a keep/revert call. Default stays
  `false` in the meantime — not flipped on a single small-sample
  result, consistent with this project's general rollout discipline
  (D23 fixed weight pending Elo test, D124's own "no further runs on
  noise" call, etc.), the one documented exception being D125's
  explicit, acknowledged skip of this same step.

**Next session start point:** Gokul runs the requested 500-game
`nonpawn_correction_enabled` follow-up batch and reports results;
combine with this session's 200 games (700 total) for the actual
decision. `improving_enabled` and `continuation_correction_enabled`
are closed — no further action needed on either.

## D134 — `nonpawn_correction_enabled` Default Flipped to `true` (Session 137)

**Decision**: Gokul's explicit call, given D133's combined 700-game
result (Elo diff -11.9, i.e. `true`/on ahead by ~11.9 Elo, direction
consistent across both independent batches: -17.4 then -9.7) — accept
the directional lean and flip the default now, rather than running
further batches chasing a clean 2-standard-error result. Same kind of
judgment call D125 already documented for
`recapture_extension_enabled` — an explicit, acknowledged skip of
waiting for a fully decisive statistical result, not a new precedent.

**Implementation — every default site for this flag, found by tracing
the full call graph rather than only the most obvious one** (this is
exactly the discipline D129's and D132's process notes called for):
- `search/mod.rs`: `SearchInfo::new()`'s actual default flipped
  `false` → `true`; field doc comment rewritten with the full
  validation history (D82 → D133 → D134).
- `main.rs`: `EngineState`'s own separate default (used by every real
  UCI session — `cmd_go` overwrites `SearchInfo`'s value with
  `EngineState`'s copy regardless of what `SearchInfo::new()` set, so
  this site's flip is actually load-bearing for real usage, not
  `search/mod.rs`'s) flipped `false` → `true`; advertised UCI option
  string (`option name NonPawnCorrectionHistory type check default
  ...`) flipped `false` → `true` so GUIs querying the engine see the
  correct default; field doc comment updated.
- `uci_wasm.rs`: no separate default site — confirmed it constructs
  `SearchInfo::new()` directly with no override, so it inherits the
  `search/mod.rs` flip automatically. No changes needed there.
- 5 tests across 3 files had to be fixed because they asserted or
  relied on the old `false` default:
  - `search/alpha_beta.rs`: `test_nonpawn_correction_defaults_to_false`
    renamed/flipped to `test_nonpawn_correction_defaults_to_true`.
    `test_nonpawn_correction_off_leaves_table_untouched` — this test's
    whole purpose is exercising the OFF path, so it now sets
    `info.nonpawn_correction_enabled = false` explicitly rather than
    relying on the default, which would otherwise have silently made
    it test the ON path instead (and fail its own "table must stay
    untouched" assertion). Two stale comments elsewhere in the file
    ("default false — see that field's doc comment") corrected.
  - `main.rs`: `test_nonpawn_correction_history_option_defaults_to_false`
    renamed/flipped to `..._defaults_to_true`.
  - `bin/match_runner.rs`: `test_experimental_flag_apply_sets_correct_field`
    used to assert `SearchInfo::new()`'s default was `false` for all
    three experimental flags as its starting condition — decoupled
    from any default by explicitly setting all three to `false` before
    the assertions, since this test's actual purpose is verifying
    `ExperimentalFlag::apply()`'s mechanics (right field, no
    cross-contamination, can unset as well as set), not asserting
    defaults — and added an explicit apply(false)-unsets-nonpawn-
    correction check at the end, mirroring the existing Improving
    check, now that the default direction differs between the flags.

**Not yet CI-confirmed** — no local `cargo test`/`cargo build` in this
sandbox; verified by grepping every file in the repo for
`nonpawn_correction_enabled` (7 files total: the 4 with real logic/
tests above, plus `uci_wasm.rs` confirmed as inheriting the flip with
no edit needed, plus `selfplay.rs`/`eval_diag.rs`/`texel_gen.rs`/
`train_nnue.rs`/`lichess_sample.rs`/`aggregate_opening_stats.rs`/
`uci_match_runner.rs` confirmed to have zero references and therefore
nothing to update) and manual brace/paren-balance checks on all 4
edited files.

**Next session start point:** Gokul commits `src/search/mod.rs`,
`src/main.rs`, `src/search/alpha_beta.rs`, `src/bin/match_runner.rs`
and confirms CI green. Once green, 34.1 is fully closed (all three
flags have a keep/revert call: 2 no-change, 1 flipped). Remaining
upgrade-plan backlog: 34.4 (LMR enrichment), 34.5 (ThreatByKing), 34.6
(WeakQueenProtection); 34.3 (NNUE) stays parked per D131.

## D135 — TT Torn-Write Self-Detection Fixed: Key Now XORed With a Payload Fingerprint (Session 139)

**Context**: Gokul uploaded two independent external investigation
reports on a reported "tactical blindness" / "blunder" symptom
(missed captures, hanging pieces, missed recaptures in normal play).
`tactical-blindness-report.md` re-checked 5 hypotheses raised in prior
discussion; 4 were refuted on direct source inspection (LMP, SEE,
null-move zugzwang guard, skill-level leakage), but the 5th — TT
torn-write corruption — checked out as a real, verifiable gap,
independent of whether it's proven to be *the* cause of any specific
game blunder (the report itself frames it as "leading suspect," not
conclusively causal — see D136 below for what turned out to be the
actual confirmed cause of the report's own reproduced cases).

**Root cause (spot-verified against live source before trusting)**:
`TTEntry` is 16 bytes — two 8-byte machine words. `store()`'s
`ptr.write(new_entry)` is not atomic across that whole write, so a
concurrent `store()`/`probe()` on the same slot can legally observe a
torn entry — e.g. `key` from one write paired with `mv`/`score`/`bound`
from a different, unrelated write. The pre-existing code comment
claimed `probe()` "detects this via key verification" — but `key` was
stored as a plain, independent field, not coupled to the payload at
all, so that claim wasn't actually true: a torn write could leave a
genuinely matching `key` sitting next to garbage payload fields, and
`entry.key == exp_key` had no way to tell.

**Fix**: applied Stockfish's actual technique (which the file's own
comment already claimed to be doing, but wasn't) — `key` is now stored
XORed with `payload_fingerprint()`, a cheap deterministic 32-bit mix of
the entry's own depth/bound/age/mv/score. `probe()`, `probe_move()`,
and `store()`'s replacement-decision logic all recompute the
fingerprint from whatever payload is CURRENTLY sitting in the slot (not
from what was originally intended) before trusting the key. A torn
write mixing two different writes' fields will not recompute back to
the original key (astronomically unlikely to coincide by chance), so
verification now correctly fails and the slot is treated as a miss
rather than silently trusted — the property the old comment claimed
but the old implementation didn't have. This rests on individual
struct-field-sized writes (a u32, a u8, an i32) not themselves tearing
— true on every architecture this project targets, no stronger an
assumption than the "benign races" design (D4) already rests on.

**Tests**: 2 new regression tests directly simulate a torn write (by
manually corrupting one field of an already-stored entry, mimicking
what a second thread's interleaved write would produce) and confirm
`probe()`/`probe_move()` now correctly reject it instead of returning
the corrupted value.

**Not yet CI-confirmed** — no local `cargo test`/`cargo build` in this
sandbox; verified by manual brace/paren-balance checks and by tracing
every internal `.key` comparison site (`store()`'s replacement
decision, the move-preservation branch) to confirm all of them were
updated consistently, not just `probe()`.

## D136 — Blunder Fallback Fixed: Depth 1 Now Guaranteed to Run, Last-Resort Fallback Made Evaluation-Aware (Session 139)

**Context**: `blunder-bug-report.md` (the second of the two reports
this session) reported a confirmed, deterministically-reproduced bug —
distinct from D135's TT finding — with FEN-level reproduction cases
(3 hand-built positions, each isolating one blunder type: missed free
capture, own piece left hanging, missed recapture; 15/15 trigger trials
reproduced, 15/15 control trials correct). Spot-verified the exact
mechanism against live source before trusting it, and it checked out
precisely as described.

**Root cause**: `iterative_deepening()`'s end-of-function safety net —
reached whenever `result.best_move == Move::NULL`, i.e. the search
never completed depth 1 before being stopped — picked
`generate_moves(pos).get(0)`: an arbitrary legal move in the raw
pseudo-legal generator's fixed iteration order (pawns, knights,
bishops, rooks, queen, king, by square index), zero evaluation applied.
`allocate_time()` subtracts the `Move Overhead` UCI option (default
30ms) from `movetime`; at `movetime <= 30`, the allocated budget is
0ms, and `is_time_up()` was tripping on the very FIRST check
(`nodes==0`, `elapsed_ms() >= 0`) — before `alpha_beta`'s root loop had
generated or evaluated a single move. `result.best_move` therefore
stayed `Move::NULL` and the blind fallback fired on every single move
at `movetime <= 30`, and intermittently (~10-20% per trial, per the
report's own supplementary testing) whenever a `stop` lands before
depth 1 finishes for any other reason (thread-scheduling delay under
load, etc.).

**Fix — the report's own two recommended, complementary changes,
both implemented**:
1. **Guarantee depth 1 gets to run.** `SearchInfo::is_time_up()`'s
   elapsed-time-budget check (not the explicit `stop`/`stop_flag`
   check above it, which still fires instantly regardless of depth) no
   longer fires while `current_depth <= 1`. Depth 1 completes in low
   single-digit milliseconds for real positions (report's own
   measurement: under 1ms in every case tested) — this can only extend
   an already-tiny time budget by a similarly tiny, bounded amount,
   never turn a real time emergency into an unbounded search, and an
   explicit `stop`/`quit` can still cut it short instantly. Deliberately
   scoped to exclude the ponder-hit hard-override path (a distinct,
   deliberate, GUI-driven "you're out of time NOW" signal computed
   fresh at `ponderhit` time — not the passive "movetime happened to be
   tiny" scenario this guard targets) — it keeps its pre-existing
   immediate-abort behavior at every depth.
2. **Make the true last-resort fallback evaluation-aware.** Replaced
   `moves.get(0)` with the move ranked first by `score_moves()` — the
   same SEE-aware, TT-move-aware, killer/history-aware ordering
   heuristic search already computes cheaply for move ordering every
   node, no real search needed. This is now a true last resort (fix #1
   closes the dominant path into it) rather than a routinely-hit path
   at low `movetime`/Skill Level settings.

**Fix-forward needed on 3 existing tests** that constructed
`SearchInfo::new()` directly and called `alpha_beta()`/checked
`is_time_up()` without going through `iterative_deepening()`'s loop
(where `current_depth` is normally set) — `current_depth` defaults to
`0`, which the new guard also treats as "depth 1 or below," so these
tests would have silently stopped exercising what they actually test:
- `search/alpha_beta.rs::test_alpha_beta_sets_stop_on_real_timeout` and
  `test_quiescence_in_check_sets_stop_on_real_timeout` (both D95,
  Session 90) — added `info.current_depth = 5` so they keep testing
  D95's actual concern (does a real timeout set `info.stop` at all) at
  a realistic mid-game depth, unrelated to D136's depth-1-specific
  behavior.
- `search/mod.rs::test_ponder_hit_hard_override_triggers_timeout` —
  no test change needed once `is_time_up()`'s ponder-override path was
  scoped correctly (see fix #1 above) — confirmed by tracing the logic
  rather than by the test failing first.

**Tests**: 3 new end-to-end regression tests reproduce the report's
exact three FEN cases at `movetime 30` through the real
`iterative_deepening()` entry point and assert the correct move is now
found (not the previously-blundered one), plus 1 test pinning down the
`is_time_up()` depth-1 mechanism directly in isolation.

**Not yet CI-confirmed** — no local `cargo test`/`cargo build` in this
sandbox; verified by manually tracing `allocate_time()`'s exact
arithmetic at `movetime=30` (confirms 0ms budget), `is_time_up()`'s
check ordering (confirms the elapsed-time branch is what fires first,
before any move generation), and by grepping the full repo for every
`time_allocated_ms = 0`-style test that could be affected by the new
depth-1 guard before deciding a test needed fixing, rather than
guessing.

**Next session start point:** Gokul commits `src/tt/mod.rs`,
`src/search/mod.rs`, `src/search/alpha_beta.rs`,
`src/search/iterative.rs` together and confirms full CI green
(including the 3 new end-to-end blunder-repro tests, which are the
real proof this works). If still red, get the log before assuming
either fix is correct — this session's own analysis has no live
compiler/test-runner backing it, only manual tracing.
