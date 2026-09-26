# Pet Dragon — Engine Architecture

*Reflects the codebase as of Session 139 (through Phase 35, D136).
Previously the header claimed "Session 82" while the body had already
been patched piecemeal past that point (e.g. D114/Session 116) without
the header being bumped — a documentation-debt pattern this revision
corrects rather than repeats. Five real gaps fixed this pass, checked
directly against live source, not memory of this doc: `TTEntry.key`'s
torn-write self-detection (D135), the iterative-deepening blunder
fallback fix (D136), the full correction-history option family
(`NonPawnCorrectionHistory`/`ContinuationCorrectionHistory`/
`CorrectionExtension`, Phase 26 items 3a-3c, D82/D85/D86/D88/D89/D133/
D134), the Threats-term status (`ThreatByRook`/`ThreatByMinor` now
implemented, only `ThreatByKing`/`WeakQueenProtection` still open,
D130), and §6's UCI option table, which was missing roughly half the
engine's actual advertised options.*

---

## 1. High-Level Pipeline

```
UCI input (main.rs)
   │
   ▼
Position (position/mod.rs) ── bitboard state, Zobrist hash, pawn_starts
   │
   ▼
Move generation (movegen/) ── magic-bitboard sliding pieces, Pet Dragon
   │                          pawn double-step rules (pawn_starts-gated)
   ▼
iterative_deepening() (search/iterative.rs)
   │   aspiration windows, MultiPV, best-move-stability time management
   ▼
alpha_beta() (search/alpha_beta.rs) ◄──► Transposition Table (tt/)
   │   full pruning/extension suite — see §3
   ▼
evaluate_blended() (eval/mod.rs) ── HCE ⊕ NNUE, runtime-weighted
   │
   ├── evaluate()         (eval/*.rs)   — handcrafted, Texel-tuned
   └── nnue::inference()  (nnue/*.rs)   — trained network, 0% weight by default
```

Startup sequence is mandatory and unchanged since early phases:
`init_masks() → init_magic() → init_zobrist()`.

---

## 2. Core Data Structures

### `Position` (`position/mod.rs`)

```rust
pub struct Position {
    pub pieces: [[Bitboard; 6]; 2],       // [color][piece_kind]
    pub occupied_by: [Bitboard; 2],
    pub all_occupied: Bitboard,

    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,

    pub hash: u64,                        // Zobrist, incrementally updated

    pub pawn_starts: PawnStartMap,        // Pet Dragon: per-pawn actual
                                           // start square, gates double-step
                                           // eligibility — NOT rank-based

    pub history: Vec<HistoryEntry>,       // for unmake
                                           // repetition detection: checked
                                           // BEFORE the TT, Stockfish-style
                                           // Vec<(u64, i32)> algorithm (D45)
}
```

`pawn_starts` is the one field with no standard-chess equivalent — it's
what makes the 2,162,160-starting-position variant correct. A pawn may
double-step if and only if it still occupies its own recorded start
square, regardless of which rank that square is on.

### `SearchInfo` (`search/mod.rs`)

Carries all per-search mutable state — move-ordering tables, time
control, MultiPV/pondering state, and Syzygy handle:

```rust
pub struct SearchInfo {
    // Time management
    pub time_allocated_ms: u64,
    pub start_time: web_time::Instant,
    pub stop: bool,
    pub stop_flag: Arc<AtomicBool>,        // shared across Lazy SMP threads

    // Move ordering
    pub killers: KillerTable,
    pub history: HistoryTable,
    pub countermoves: CountermoveTable,
    pub cont_hist: Box<[[[i32; 64]; 12]; 64]>,   // [prev_to][piece][to]
    pub correction_history: pruning::CorrectionHistory, // pawn-hash indexed, always on
    pub correction_history_nonpawn: pruning::CorrectionHistory, // gated, default ON (D134)
    pub correction_history_continuation: pruning::CorrectionHistory, // gated, default off
    pub nonpawn_correction_enabled: bool,        // UCI NonPawnCorrectionHistory, default true
    pub continuation_correction_enabled: bool,   // UCI ContinuationCorrectionHistory, default false
    pub correction_extension_enabled: bool,      // UCI CorrectionExtension, default false

    // PV / node accounting
    pub pv_table: [[Move; MAX_PLY]; MAX_PLY],
    pub nodes: u64,
    pub seldepth: usize,

    // Pondering (Phase 18/D37)
    pub ponder_hit_soft_ms: Arc<AtomicU64>,
    pub ponder_hit_hard_ms: Arc<AtomicU64>,

    // MultiPV (Phase 19)
    pub multipv: usize,
    pub root_exclude: Vec<Move>,

    // Tablebase (Phase 15, native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub syzygy: Option<Arc<syzygy::SyzygyProber>>,
}
```

### `TTEntry` / `TranspositionTable` (`tt/mod.rs`)

Lock-free, power-of-2 sized, age-based replacement:

```rust
pub struct TTEntry {
    pub key:   u32,   // upper 32 bits of Zobrist hash, XOR'd with a
                       // payload_fingerprint() of this entry's own
                       // depth/bound/age/mv/score (D135) — see below
    pub depth: i8,
    pub bound: Bound, // Exact / LowerBound / UpperBound
    pub age:   u8,    // search generation, drives replacement priority
    pub mv:    Move,
    pub score: i32,
}
```

**Torn-write self-detection (D135, Session 139).** Two Lazy SMP threads
can race a non-atomic read/write of the same slot; the code has always
carried a comment claiming this self-detects, but until D135 that was
false — `key` was stored as a plain, independent field, so a torn write
mixing one write's key with another's payload decoded as a normal
(wrong) hit instead of a mismatch. Fixed by XOR-ing `key` with
`payload_fingerprint()`, computed from the entry's own depth/bound/age/
mv/score at write time. `probe()`, `probe_move()`, and `store()` all
recompute the fingerprint from whatever payload currently sits in the
slot before trusting the key, so a torn write no longer decodes back to
the original key. Two regression tests directly simulate a torn write.
Flagged by an external report as a "leading suspect" for a reported
blunder pattern; real bug, confirmed fixed, but D136 below turned out
to be the actual root cause of the report's own reproduced cases.

---

## 3. Search — Full Pruning & Extension Suite

All of the following are implemented and wired into `alpha_beta()`
(`search/alpha_beta.rs`), not partial or planned:

| Technique | Where | Notes |
|---|---|---|
| Principal Variation Search (PVS) | `alpha_beta.rs` | full-width first move, null-window re-search otherwise |
| Iterative deepening + aspiration windows | `iterative.rs` | narrowing window per depth, re-widens on fail-high/low. **D136 (Session 139):** `is_time_up()`'s elapsed-time check no longer fires while `current_depth <= 1` (explicit stop/quit and the ponder-hit hard override are still instant), guaranteeing depth 1 always completes before any stop can land — fixes a confirmed, deterministically-reproduced blunder pattern at `movetime <= 30` where the default Move Overhead reduced the allocated budget to 0ms and the search returned before generating a single evaluated root move |
| Null move pruning | `alpha_beta.rs` | adaptive reduction `3 + depth/6`, zugzwang-guarded (non-pawn material check) |
| Late Move Reductions (LMR) | `alpha_beta.rs`, `pruning.rs` | `MIN_DEPTH_LMR = 3`, reduction scales with depth and move count |
| Razoring | `alpha_beta.rs` | `MIN_DEPTH_RAZORING = 1`, drops to qsearch when static eval far below alpha |
| Futility pruning | `alpha_beta.rs` (`pruning::futility_margin`) | `MIN_DEPTH_FUTILITY = 1`, near-leaf quiet-move skip. Margin improving-aware as of D114 (Session 116, off by default — see below) |
| ProbCut | `pruning.rs` | `MIN_DEPTH_PROBCUT = 5`, shallow-search verified captures beating beta+margin (`PROBCUT_MARGIN = 200`) prune the node — Pet Dragon's first multi-cut-family technique; D59 (below) adds a second, structurally different one inside singular extension |
| Internal Iterative Reduction (IIR) | `alpha_beta.rs` | `MIN_DEPTH_IIR = 4`, reduces PV nodes with no TT move |
| Singular extensions, multi-cut, negative extension | `alpha_beta.rs` | `MIN_DEPTH_SINGULAR = 6`. Base technique (Phase 13.3): reduced exclusion-search verifies TT move is forced before extending by 1. Extended D59 (Session 82) with two siblings reusing the same verification result: **multi-cut** — verification still reaches `singular_beta` ⇒ some other move also refutes ⇒ prune the whole node early (`return singular_beta`, same early-return shape as ProbCut above); **negative extension** — verification doesn't confirm singularity but the TT move's own score already meets beta ⇒ reduce (not extend) the TT move, `-1` ply PV / `-2` ply non-PV |
| Late Move Pruning (LMP) | `pruning.rs` (`should_apply_lmp`, `lmp_threshold`) | D60 (Session 82). Distinct from LMR — skips late quiet moves outright (not just reduced) once a depth-indexed quiet-move-count threshold is passed (`MAX_DEPTH_LMP = 8`). Non-PV only, never in check/giving check, never near mate-range alpha/beta. Threshold table is improving-aware as of D114 (Session 116, off by default — see below) |
| Check extensions | `pruning.rs` (`extension()`) | |
| Quiescence search | `alpha_beta.rs` | in-check evasions, checkmate detection, per-capture delta pruning, quiet checks at `qs_depth = 0` (13.5) |
| Move ordering | `ordering.rs` | SEE-based capture ordering, killers, countermoves, history heuristic with gravity, 1-ply continuation history |
| Correction history (pawn-hash) | `pruning.rs` (`CorrectionHistory`) | pawn-hash-indexed static-eval error tracker, applied before pruning decisions use static eval. Always on — no gating option, unlike the two siblings below |
| Correction history (non-pawn) | `search/mod.rs` (`correction_history_nonpawn`) | Phase 26 item 3a (D82). Second, independent `CorrectionHistory` table indexed by non-pawn material instead of pawn structure. Gated by `SearchInfo::nonpawn_correction_enabled`, UCI `NonPawnCorrectionHistory` — shipped off, SPRT'd at 700 games across two independent batches (D133, Session 136), direction held both times (~+11.9 Elo combined favoring on), **default flipped to `true`** (D134, Session 137) |
| Correction history (continuation) | `search/mod.rs` (`correction_history_continuation`) | Phase 26 item 3b (D86). Third, independent `CorrectionHistory` table indexed by `pruning::continuation_hash` (last two real moves' squares) rather than any board-state property. Gated by `continuation_correction_enabled`, UCI `ContinuationCorrectionHistory`. SPRT'd (D133, Session 136): +8.7 Elo favoring off, within this project's own noise bar — **no change**, default stays `false` |
| Correction-scaled singular margin | `alpha_beta.rs` (`correction_extension_enabled`) | Phase 26 item 3c (D89). When true, scales the singular-extension margin down (by up to 1, from a base of 2) using the pawn-hash correction table's magnitude only — deliberately excludes the two tables above. UCI `CorrectionExtension`, default `false` |
| Last-resort fallback move | `iterative.rs` | **D136 (Session 139).** If depth 1 somehow still doesn't complete, the end-of-function safety net now picks via `score_moves()` instead of blind `moves.get(0)` — the prior blind first-legal-move pick was the confirmed root cause of a separately reported blunder pattern (paired with the depth-1 guard above, which makes this path rare) |
| Repetition detection | `position/mod.rs` | Stockfish-style `Vec<(u64, i32)>` algorithm (D45), checked before TT probe |
| Lazy SMP, thread-differentiated | `main.rs` | N-1 helper threads share TT, time-unlimited, killed via shared `stop_flag`. **Thread-differentiated as of D49 (Session 76)** — small fixed offset tables keyed on `thread_id` vary each helper's search parameters (not per-thread RNG — see D49 for why that was rejected); this corrects the old "helpers are not currently parameter-differentiated" note that used to be here |
| Time management | `search/time.rs` | soft/hard limits plus best-move-stability early stop (`STABILITY_THRESHOLD`) |
| MultiPV | `iterative.rs`, `search/mod.rs` | `root_exclude` prevents re-searching claimed root moves; single-PV path is byte-identical to pre-MultiPV behavior |
| Pondering | `main.rs`, `search/mod.rs` | `ponder_hit_soft_ms` / `ponder_hit_hard_ms`, consumed once via atomic swap on `ponderhit` |
| Skill Levels | `search/skill.rs` | depth cap respected by helper threads too, so low-skill searches don't leak full-strength TT lines (Phase 20) |
| Improving flag | `alpha_beta.rs`, `search/mod.rs` (`SearchInfo::static_eval_stack`) | D114 (Session 116). `SearchInfo::improving_enabled` UCI `ImprovingHeuristic`, default `false` — same unproven-technique rollout shape as `null_move_king_guard`/`threat_defusal`. When on: compares this node's static eval to the value two plies back for the same side to move; feeds improving-aware LMP thresholds and futility margins (both prune more aggressively when not improving). When off (default), `alpha_beta.rs` never writes the eval-history stack and both pruning sites behave byte-identically to pre-D114 |

Deeper (2-ply/4-ply) continuation history beyond the current 1-ply
table, and any variant-specific opening statistics (23.4, explicitly
HELD as of D62 — see ROADMAP.md, not a gap so much as a deferred
decision) remain absent. Elo impact of D49/D59/D60/D114 not yet
measured against a real match — `cargo test` passing confirms
correctness, not strength; see SESSION_LOG Session 82 (D49/D59/D60)
and Session 116 (D114) for the open items.

---

## 4. Evaluation

### Handcrafted Evaluation (`eval/mod.rs` and submodules)

Texel-tuned (Phase 14, D35), Ethereal-derived (GPL v3, Andrew Grant)
starting weights. All terms tapered middlegame→endgame via a `phase`
value threaded into every term from `eval/mod.rs`'s `evaluate()`:

1. **Material** (`eval/material.rs`) — phase-dependent piece values, bishop-pair bonus
2. **Piece-square tables** (`eval/tables.rs`) — absolute-square lookup, no king-relative term (see gaps below)
3. **Mobility** (`eval/mobility.rs`) — attack-count bonus, king mobility excluded (handled separately by king safety)
4. **Pawn structure** (`eval/pawns.rs`) — passed (rank-scaled)/isolated/doubled/backward
5. **King safety** (`eval/king_safety.rs`) — pawn shield, open/semi-open files near king, attacker-count danger (weighted by piece type, nonlinear escalation), MG only — EG king activity handled by tables.rs instead
6. **Open lines** (`eval/open_lines.rs`) — rook on open file, batteries, 7th rank, connected rooks
7. **Tempo** — Pet-Dragon-specific Texel-tuned bonus for side to move

Pet Dragon-specific evaluation notes (see DECISIONS.md for full
rationale): no opening-phase suppression of any term (D6) — all terms
active from move 1, since there's no fixed "opening phase" across
2.16M possible starts; no castling bonus in king safety (D7) — ~74% of
games see no castling at all; open lines never suppressed early (D8).

**HCE term-gap audit (D63, Session 82).** Read all six eval submodules
fresh (not from memory of this doc) to check what's actually missing
against the standard classical-engine term list. Most of the
well-established list is already present (confirmed above). Three real
gaps found, none implemented yet, ranked by expected value:

1. **Passed-pawn king distance** — `pawns.rs`'s passed-pawn bonus is
   rank-only; it doesn't check either king's distance to the pawn (the
   "square of the pawn" idea). Highest-ranked of the three — this is
   one of the highest-value terms in any strong classical engine and
   is currently fully absent.
2. **Pawn storm** — `king_safety.rs` scores the defensive pawn shield
   but nothing scores advancing pawns toward the *enemy* king as an
   attacking resource. Confirmed via grep: no king-relative logic
   exists anywhere outside `king_safety.rs` (checked `pawns.rs`,
   `mobility.rs`, `open_lines.rs`).
3. **King-relative PST bucketing** — `tables.rs`'s `pst_value()` is a
   pure absolute-square lookup; a piece's positional value never
   depends on either king's position. Lowest-ranked/most speculative
   of the three — would need new small Texel-tunable parameter buckets
   (coarse, e.g. same-side/center/opposite-side — NOT per-square
   king-relative NNUE-style buckets, which would blow the HCE
   parameter-count-vs-training-data budget the same way oversized NNUE
   features already have, D53/D55).

None of the three are scheduled — this is a documented candidate list,
not a roadmap commitment. See DECISIONS.md D63 for the full
investigation.

**Fourth gap found separately (D68, Session 84) — partially closed.**
Checking whether the engine scores pieces defending each other
surfaced a gap outside D63's original checklist: no **Threats** term
(Stockfish's `threats.cpp` — hanging-piece penalty, weak-queen-
protection penalty, minor/rook threat bonus, restricted-piece
penalty). Two of these are now implemented: `THREAT_BY_MINOR_BONUS`
and, as of D130 (Phase 34.2, Session 133), `THREAT_BY_ROOK_BONUS =
s(20, 12)` — rook-attacks-queen only (not rook-attacks-rook, matching
Stockfish's own restriction; weak signal otherwise). Both accepted the
same double-counting argument D68 originally flagged against
`mobility.rs`'s attack counts and `king_safety.rs`'s attacker-weight
term — hand-checked before landing, not discovered mid-CI. Still open:
`ThreatByKing` (34.5, review flags this as needing the most care —
MG/EG weighting, avoid rewarding an exposed king, do last) and
`WeakQueenProtection` (34.6). Restricted-piece and hanging-piece
penalties remain unscheduled. Full scope in DECISIONS.md D68/D130.

**Alternative eval paradigms surveyed (D64, Session 82).** For the
record, so this doesn't get re-investigated from scratch later: MCTS +
policy/value network (AlphaZero/Leela-style — different search
paradigm entirely, needs GPU-scale compute Pet Dragon's WASM/CPU
deployment doesn't have), searchless transformer evaluation (single
forward pass, no tree search — needs even more training data than
NNUE already lacks), GPU-sized NNUE variants (breaks the CPU-quantized
efficiency NNUE was chosen for), and policy-guided move ordering
(the one option compatible with Pet Dragon's existing alpha-beta
architecture without a rewrite, but still a search technique needing
training data, not an eval technique). None adopted — see DECISIONS.md
D64 for the full reasoning per option.

### NNUE (`nnue/` — Phase 16, shelved D61)

```
Feature set (D10/D11): 896 inputs per perspective
  - 768 standard piece-square features (6 kinds × 2 colors × 64 squares)
  - 128 Pet Dragon pawn-start features (2 colors × 64 squares) —
    active only while a pawn still occupies its actual start square;
    drops to 0 the instant that pawn moves, regardless of destination
```

`nnue/features.rs` — feature encoding. `nnue/delta.rs` — incremental
accumulator updates for make/unmake, proven equivalent to full
re-extraction. `nnue/inference.rs` — forward pass.

**Current status**: fully implemented and trainable, but disabled by
default (`NNUEWeight` = 0) *and*, as of D61 (Session 82), **shelved for
the future by deliberate decision** — not just runtime-disabled
pending more work, but not an active development target right now.
`evaluate_blended()` (`eval/mod.rs`) still blends HCE and NNUE via a
runtime `NNUEWeight` UCI option (0-100), stored as an integer
percentage rather than a compile-time constant specifically so Phase
17's A/B testing could compare pure-HCE vs. blended search from the
same binary via `setoption` — that machinery stays in place and
functional, just unused by default. Both network sizes tested
(hidden_size=32 and 128, D34/D41) lost to pure HCE — most plausibly a
training-data volume problem (roughly half a million combined
self-play + Lichess rows at the time, versus the hundreds of billions
modern top-class engines train on) rather than an architecture
problem. The inference pipeline itself is production-ready; scaling
the training data was always the open item (D53/D55/D57/D58), and D61
is the decision to stop pursuing that for now rather than a finding
that it's unsolvable. If revisited, start from D58's three reopening
options (better reuse of NORU's already-correct i16 quantized
inference, a training-infra swap off NORU's CPU trainer, or
Stockfish-distillation data augmentation), not from scratch.

---

## 5. Tablebases

Syzygy support (Phase 15), native builds only (`#[cfg(not(target_arch
= "wasm32"))]`) — `syzygy::SyzygyProber`, `Arc`-wrapped for cheap
cloning into Lazy SMP helper threads. Pawn-start features have decayed
to zero by the time few enough pieces remain for tablebase lookup, so
the position is otherwise indistinguishable from a standard-chess
endgame of the same material — **but castling rights are not
guaranteed to be gone** (corrected 2026-08-03, D127/external bug
report: no game rule forces rights to clear before material thins into
TB range, and roughly 26% of games retain at least one right; a king
and its never-moved rook can survive untouched into a 5-7 piece
endgame). Syzygy tables don't encode castling at all, so both
`SyzygyProber::probe_wdl()` and `probe_root()` explicitly refuse to
probe (`has_castling_rights()` guard) whenever either side still has
any right, rather than relying on piece count alone.

---

## 6. UCI Protocol (`main.rs`)

Full option set, confirmed from the actual `option name` output:

| Option | Type | Default | Notes |
|---|---|---|---|
| `Hash` | spin | — | TT size, MB |
| `Threads` | spin | 1 | Lazy SMP helper count |
| `UCI_Chess960` | check | false | |
| `SyzygyPath` | string | empty | |
| `NNUEWeight` | spin | **0** | 0 = pure HCE, 100 = pure NNUE — shelved D61, machinery stays functional |
| `PlayStyle` | spin | 0 | 0-4 |
| `MultiPV` | spin | 1 | |
| `Move Overhead` | spin | — | ms |
| `Skill Level` | spin | — | depth-capped, helper threads respect it too |
| `Ponder` | check | true | |
| `Contempt` | spin | 0 | -100..100 |
| `UCI_LimitStrength` | check | false | |
| `UCI_Elo` | spin | — | |
| `NullMoveKingGuard` | check | false | unproven-technique toggle, same rollout shape as the four below |
| `NonPawnCorrectionHistory` | check | **true** | flipped from `false` — D134 (Session 137), see §3 |
| `ContinuationCorrectionHistory` | check | false | SPRT'd, no change — D133, see §3 |
| `CorrectionExtension` | check | false | see §3 |
| `LMPEnabled` | check | true | kill-switch for Late Move Pruning (§3) |
| `SingularMultiCutEnabled` | check | true | kill-switch for the D59 multi-cut/negative-extension pair (§3) |
| `ThreatDefusal` | check | false | unproven-technique toggle |
| `ImprovingHeuristic` | check | false | D114, see §3 |
| `RecaptureExtension` | check | true | D125 |

---

## 7. WASM / Browser Deployment

`wasm-pack build --target web` → `pet_dragon_bg.wasm` + `pet_dragon.js`
(deploy.yml, GitHub Pages) and, since Session 73, also published as
release assets from `build.yml`'s `build-wasm` job alongside a
`pet_dragon_standalone.js` (base64-embeds the wasm binary; still
imports `pet_dragon.js` for bindings — see DECISIONS.md D46 for why
full single-file bundling was explored and rejected). `getrandom/js`
feature enabled for WASM builds. Live eval bar confirmed rendering real
search output (Session 71).

A second, independent WASM target exists for the `uci-wasm` feature
(`src/uci_wasm.rs`, single export `uci_command` — a real UCI text
protocol over WASM, not the direct-call `wasm` API above). Built via
its own `wasm-pack build --features uci-wasm` invocation — deliberately
never combined with the `wasm` feature in one build (D-series decision
recorded when Phase 30 was scoped: keeps this less-battle-tested
surface from ever risking the production gameplay bundle). Published
to `web/pkg-uci/` for GitHub Pages (deploy.yml) and, since Session 114
(30.7), also as `pet_dragon_uci_bg.wasm` + `pet_dragon_uci.js` release
assets from `build.yml`'s `build-wasm` job, mirroring the `wasm`
bundle's release-asset treatment exactly. No real target UCI-speaking
browser GUI confirmed yet (ROADMAP 30.8, open as of Session 115).

---

## 8. Crate Layout

```
src/
├── bitboard/      magic bitboards, masks
├── position/      Position, FEN, make/unmake, setup (2,162,160 starts), Zobrist
├── movegen/       legal move generation, castling, pawns, pieces
├── search/        alpha_beta, iterative deepening, ordering, pruning, see, skill, time
├── eval/          material, tables, mobility, pawns, king_safety, open_lines
├── nnue/          features, delta, inference
├── tt/            transposition table
├── syzygy/        tablebase probing (native only)
├── texel/         Texel tuning (features, predict, weights)
├── bin/           selfplay, match_runner, uci_match_runner, texel_*, train_nnue, eval_diag, lichess_sample
├── main.rs        UCI loop, Lazy SMP thread orchestration
└── lib.rs         crate root
```
