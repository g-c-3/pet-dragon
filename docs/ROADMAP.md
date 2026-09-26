''# ROADMAP.md
# Pet Dragon — Development Roadmap

## How to Read This File
- [x] = complete and green (tests passing)
- [~] = in progress
- [ ] = not started
- ⚠️ = has a known issue or special note

---

## Phase 0 — GitHub Repository ✅
- [x] Repository created at https://github.com/g-c-3/pet-dragon
- [x] LICENSE (GPL v3)
- [x] README.md
- [x] .github/workflows/build.yml — auto-build + release binaries
- [x] .github/workflows/deploy.yml — auto-deploy to GitHub Pages

---

## Phase 1 — Project Scaffold & Core Types ✅
- [x] Cargo.toml
- [x] src/lib.rs
- [x] src/main.rs (placeholder UCI loop)
- [x] src/types.rs — Square, File, Rank, Color, PieceKind, Piece,
      Move, MoveKind, CastlingRights, PawnStartMap
- [x] 1.1 — Bug fix (Session 104, D109): `Move::to_uci()` printed
      `Move::NULL` as "a1a1" (its literal from/to squares) instead of
      the standard UCI null-move sentinel "0000", which most GUIs/
      tooling expect. Found by Performance Review §4.4. Special-cased
      `to_uci()` on `is_null()`. See DECISIONS.md D109.

---

## Phase 2 — Bitboard Foundation ✅
- [x] src/bitboard/mod.rs — Bitboard type, all ops, iterator
- [x] src/bitboard/masks.rs — Attack tables, PAWN_DOUBLE_PUSH_MASK, init_masks()
- [x] src/bitboard/magic.rs — Magic bitboards, init_magic()
- [x] 56 tests passing including Pet Dragon double-push tests

---

## Phase 3 — Position & Pet Dragon Generator ✅
- [x] src/position/mod.rs — Position struct, check detection, repetition
- [x] src/position/fen.rs — FEN parser + generator with 7th field extension
- [x] src/position/zobrist.rs — Zobrist hash including PAWN_START_KEYS, init_zobrist()
- [x] src/position/setup.rs — Pet Dragon generator, validate_pet_dragon_setup()
- [x] src/position/make_move.rs — Full make/unmake, make/unmake_with_history()
- [x] tests/setup.rs — 1000 position validation passing

---

## Phase 4 — Move Generation ✅
- [x] src/movegen/mod.rs — MoveList, generate_moves(), generate_captures()
- [x] src/movegen/pieces.rs — All standard piece moves
- [x] src/movegen/pawns.rs — Pet Dragon custom pawn logic (rank 1 double-step)
- [x] src/movegen/castling.rs — Dynamic castling from setup rights
- [x] src/movegen/legal.rs — Legal move filter, apply_move_for_legality_pub()
- [x] tests/perft.rs — Perft depth 5 = 4,865,609 ✅ PROVEN CORRECT

---

## Phase 5 — Make/Unmake + Repetition ✅
- [x] 5.1 — position.make_move() incremental state update
- [x] 5.2 — position.unmake_move() perfect restoration
- [x] 5.3 — 10,000 random make/unmake sequences verified
- [x] 5.4 — Repetition detection with game history stack
      is_repetition() / is_threefold_repetition()
      make_move_with_history() / unmake_move_with_history()
- [x] tests/make_unmake.rs — perft depth 5 via make/unmake = 4,865,609 ✅

---

## Phase 6 — Transposition Table ✅
- [x] src/tt/mod.rs — TTEntry, Bound enum, TranspositionTable
      store(), probe(), probe_move()
      score_to_tt() / score_from_tt() mate score adjustment
      new_search() age increment, fill_permille() stats
- [x] 6.1 — Bug fix (Session 102, D107): `TranspositionTable::new()`
      sizing formula silently halved capacity at every exact
      power-of-two Hash size (16/32/64/128/256 MB — including the
      64 MB default). Found by external review (F-1), fixed by
      flooring directly to the largest power of two <= raw entry
      count instead of `next_power_of_two() / 2`. Two new regression
      tests assert the exact expected entry count. See DECISIONS.md
      D107.

---

## Phase 7 — Search Engine ✅
- [x] 7.1 — src/search/mod.rs — SearchInfo, SearchResult, constants
- [x] 7.2 — src/search/time.rs — TimeControl, allocate_time(), TimeManager
- [x] 7.3 — src/search/see.rs — SEE (see() bool + see_value_of() i32)
- [x] 7.4 — src/search/ordering.rs — Full move ordering, ScoredMove pub
- [x] 7.5 — src/search/alpha_beta.rs — Alpha-beta + PVS + quiescence
- [x] 7.6 — src/search/iterative.rs — Iterative deepening + aspiration windows
- [x] 7.7 — src/search/pruning.rs — Extensions, LMR, probcut, correction history
- [x] 239 tests passing, Build #86 green
- [x] 7.8 — Bug fix (Session 103, D108): null-move pruning cleared
      `en_passant` without removing its Zobrist key from `pos.hash`,
      duplicated in both the real null-move probe and
      `extract_threat_move()`. Found by external review (F-2), fixed
      by XOR-ing the ep key in/out symmetrically (mirrors
      `make_move.rs`), and factored into shared `make_null_move()`/
      `unmake_null_move()` helpers so both call sites share one source
      of truth. Two new regression tests compare the incremental hash
      against `Position::compute_hash()` (from-scratch recompute) at
      every stage. See DECISIONS.md D108.

---

## Phase 8 — Handcrafted Evaluation (HCE) ✅ COMPLETE (296 tests passing)
- [x] 8.1 — src/eval/material.rs — Tapered material values (Ethereal weights)
            s(mg,eg) packed score, taper(), game_phase()
- [x] 8.2 — src/eval/mod.rs — Module declarations (stub)
- [x] 8.3 — src/eval/tables.rs — Piece-square tables (PST) MG+EG
- [x] 8.4 — src/eval/mobility.rs — Mobility bonus per piece type
- [x] 8.5 — src/eval/pawns.rs — Pawn structure evaluation
            ⚠️ Rank 1 pawns NOT penalised as backward (Pet Dragon rule applied)
- [x] 8.6 — src/eval/king_safety.rs — King safety (no castling bonus, D7)
- [x] 8.7 — src/eval/open_lines.rs — Open file/diagonal, batteries, 7th rank
- [x] 8.8 — src/eval/mod.rs FINAL — Full evaluate() combining all terms
- [x] 8.9 — Wire evaluate() into src/search/alpha_beta.rs
- [x] 8.10 — src/material.rs duplicate — orphan file, not declared in lib.rs,
             compiler ignores it. **DELETED (Session 72)** — Gokul confirmed
             removal via GitHub mobile UI; the "can't delete via web UI"
             blocker noted at the time this was discovered no longer applies.
- [x] 8.11 — Bug fix: PST White indexing reversed in tables.rs
             (7-rank)*8+file for White; sq.index() for Black.
             296 tests passing. Phase 8 complete. ✅

---

## Phase 9 — UCI Protocol ✅
- [x] 9.1 — Full UCI command loop in src/main.rs
            uci, isready, ucinewgame, position, go, stop, quit
- [x] 9.2 — position command: parse startpos / fen + moves
- [x] 9.3 — go command: parse wtime/btime/winc/binc/movestogo/movetime/depth
- [x] 9.4 — UCI options: Hash (TT size), Threads (future SMP)
- [x] 9.5 — bestmove output after search completes
- [x] 9.6 — info strings during search (already formatted in SearchResult)

---

## Phase 10 — GitHub Actions Release Pipeline ✅
- [x] 10.1 — Build release binaries for Windows/macOS/Linux in build.yml
- [x] 10.2 — GitHub Releases page with download links
- [SKIPPED] 10.3 — Verify binaries work with Arena, BanksiaGUI, CuteChess.
             Decided permanently blocked (Session 49) — these are desktop
             GUI apps, Gokul is mobile-only (D-series core rule). No path
             to doing this without a desktop machine becoming available.
             Not revisiting unless that constraint changes.

---

## Phase 11 — WebAssembly Build ✅
- [x] 11.1 — wasm-pack build --target web --release
- [x] 11.2 — wasm-bindgen exports: engine_name(), engine_version(), new_game(), search_from_fen(), legal_moves_from_fen()
- [x] 11.3 — WASM feature flag gates all browser-specific code
- [x] 11.4 — getrandom/js feature for Pet Dragon position generation in browser

---

## Phase 12 — Browser Game ✅
- [x] 12.1 — web/index.html — chessboard UI (vanilla JS, no frameworks)
- [x] 12.2 — Pet Dragon engine is the opponent (search_from_fen WASM export)
- [x] 12.3 — Pet Dragon position display (rank 1/2 pieces shown correctly)
- [x] 12.4 — Game controls: new game, undo, flip board, side select, think time
## Phase 12 — Browser Game ✅
## Phase 12 — Browser Game ✅
- [x] 12.5 — Deploy via GitHub Pages (deploy.yml fixed mkdir ordering bug)
- [x] 12.6 — Bug fix: engine never moved in browser ("Engine thinking..."
             hung forever). Root cause: std::time::Instant::now() panics on
             wasm32-unknown-unknown (no clock source); SearchInfo::new()
             called it immediately on every search, silently trapping the
             WASM module (no panic hook was wired up to report it). Fixed
             via web-time crate (drop-in Instant replacement, wasm-safe) +
             wired up console_error_panic_hook for future visibility.

---

## Phase 13 — Search Improvements ✅ COMPLETE
- [x] 13.1 — Wire Probcut into alpha_beta.rs (defined in pruning.rs)
- [x] 13.2 — Wire CorrectionHistory into eval (defined in pruning.rs)
- [x] 13.3 — Singular extensions (Build #136 green)
- [x] 13.4 — Lazy SMP (multi-threaded parallel search)
- [x] 13.5 — Improve quiescence search: in-check evasions + checkmate detection + per-capture delta pruning + quiet checks at qs_depth=0
- [x] 13.6 — History gravity and continuation history
            cont_hist[prev_to][piece_idx][to] added to SearchInfo (Box, 192KB heap)
            Used in score_move() for quiet moves; updated in update_ordering_on_cutoff()
            Gravity formula matches regular history; zeroed each search
- [x] 13.7 — Node count benchmarking vs known engines
            tests/node_count.rs: 5 #[ignore] fixed-depth benchmarks (depth 8–11)
            build.yml bench job: cargo test --profile bench-tests node_count -- --ignored --nocapture
            Cargo.toml: [profile.bench-tests] inherits release, panic=unwind, lto=false
            Bug fixed: panic=abort in [profile.release] silently killed test binary (exit 101)
            Positions: startpos/d10, kiwipete/d9, endgame/d11, tactical/d9, rank-1/d8
            Baselines: fill in ??? slots from bench job Actions log after next run

---

## Phase 14 — Texel Tuning ✅ COMPLETE

- [x] 14.1 — DECIDED (Session 49): proceed. Phase 13 is complete and Phase
            17.5's NNUE work is parked at NNUEWeight=0% (D34) — HCE is the
            actual shipped evaluation right now, so tuning it directly
            improves real playing strength, unlike further NNUE work. Per
            D19/D20: GitHub Actions is the default execution path (NOT
            Colab — that was rejected in D19 before this ROADMAP note was
            last updated), Kaggle only as fallback if Actions' 6-hour job
            ceiling becomes a real constraint.
- [x] 14.2 — COMPLETE. All 4 batches run + uploaded (smoke test + 3
            production batches, seeds 1000/2000/3000): 147,867 total
            samples. Result distribution: 62,841 losses / 5,332 draws /
            79,694 wins (stm-perspective) — win/loss imbalance is expected
            (not a bug, see Session 52 chat), draw rarity (~3.6%) reflects
            shallow 100ms searches, also expected. Data validated and ready.
- [x] 14.3/14.4/14.5 — Phase 14 COMPLETE. All of D35's plan built, verified,
            and applied. Steps 1-5 (Sessions 53-54): TexelFeatures,
            TunableWeights, predict()/predict_f64(), Adam optimizer +
            weight decay. Step 5.5 (Session 55, added after first two real
            runs came back with implausible values): src/bin/texel_diag.rs
            sanity-check tool, comparing default-vs-tuned HCE on unambiguous
            positions — now part of the standard workflow for any future
            re-tuning round. Step 6 (Session 56): applied the
            weight_decay=0.08, 100-epoch, 147,283-sample tuned weights to
            eval/material.rs, tables.rs, mobility.rs, pawns.rs,
            king_safety.rs, open_lines.rs, mod.rs (TEMPO) AND synced
            src/texel/weights.rs's TunableWeights::default() to match (this
            second sync is mandatory — it's the tuner's starting point for
            next time, and skipping it silently breaks the Session 53
            self-consistency tests, caught this session by cargo test).
            Full suite 329/329, texel_diag PASS on all 10 cases,
            eval_diag.rs and mobility/pawns/king_safety/open_lines/tables
            unit tests re-verified by hand against the new values (all
            structural — symmetry, array lengths, index math — none
            depend on exact constant values, so none were at risk, but
            worth the explicit check before committing).
            Session 53: TexelFeatures/TunableWeights/predict() +
            self-consistency test (bit-exact vs evaluate()).
            Session 54: step 5 — src/texel/weights_f64.rs (f64 weight
            vector, flatten/unflatten), src/texel/predict_f64.rs (f64
            forward pass + fused gradient accumulation, king-safety clamp
            gets zero gradient per D35), src/bin/texel_tune.rs (K line
            search + Adam gradient descent over the flattened parameter
            vector, writes tuned weights in ready-to-paste s(mg,eg)
            syntax), .github/workflows/texel_tune.yml (mirrors
            train_nnue.yml's run_id/paths/urls three-source pattern).
            Verified end-to-end on a real 559-sample dataset (40 games,
            self-generated via texel_gen this session, NOT the 14.2
            production data): K line search found 2.589, loss fell every
            epoch (0.0426 -> 0.0170 over 8 epochs), output file parses
            back into weights.rs's own literal format cleanly. Full suite
            329/329 (322 + 3 Session 53 + 4 Session 54).
            NEXT SESSION START POINT: run texel_tune.yml FOR REAL against
            the actual 14.2 production database (147,867 samples across
            the smoke-test + 3 seed-1000/2000/3000 batches — find their
            GitHub Release asset URLs or Run IDs and wire them into
            data_run_id/data_paths/data_urls). Start with a short run
            (~10-20 epochs) to sanity-check loss trends on the real data
            before committing to a long run. Once satisfied: step 6 —
            read texel_weights_tuned.txt's output, sanity-check the
            tuned numbers aren't wild outliers (a diagnostic like
            eval_diag.rs's start-pos/material-swing checks, run through
            the tuned weights before touching eval/*.rs, is worth building
            here — no such check exists yet for HCE specifically), then
            write the eval/*.rs delta (material.rs, tables.rs,
            mobility.rs, pawns.rs, king_safety.rs, open_lines.rs, mod.rs's
            TEMPO) replacing the current hand-picked Ethereal-derived
            constants with the tuned ones. Build.yml must stay green.
            [STALE NOTE, superseded Session 56] The above "next session
            start point" text was left over from Session 54 and never
            removed once the work described in it was actually done in
            Sessions 55-56. Real status: DONE. Tuned weights are applied
            and committed (Session 55), texel_diag.rs sanity tool exists
            and passed 10/10, full suite 329/329, and 3 match_runner.yml
            runs (17.7, Session 56) confirm the tuned build is strong in
            absolute terms. No true pre/post-tuning Elo delta exists or
            is queued — see 17.7's caveat.

---

## Phase 15 — Syzygy Tablebases ✅
- [x] 15.1 — pyrrhic-rs v0.2.0 (crates.io). PetDragonAdapter implements
             EngineAdapter via our precomputed bitboard attack tables.
             SyzygyProber wraps TableBases<PetDragonAdapter> with Position helpers.
             TB_WIN_SCORE = 10_000 (above max HCE, below mate threshold 900_000).
             Gate: [target.'cfg(not(target_arch = "wasm32"))'.dependencies].
             WASM builds unaffected — libc excluded on wasm32.
- [x] 15.2 — UCI SyzygyPath string option (cmd_uci). setoption handler in
             main.rs: init SyzygyProber on non-empty path, print info string
             with max_pieces. Disable and log on empty path or bad path.
- [x] 15.3 — WDL probe at ALL interior nodes in alpha_beta_with_excluded(),
             after draw checks, before TT probe.
             Condition: piece_count ≤ tb.max_pieces() && halfmove_clock == 0.
- [x] 15.4 — WDL scores: Win=+10000, CursedWin=+1, Draw=0,
             BlessedLoss=-1, Loss=-10000 cp. Score stored in TT for efficiency.
- [x] 15.5 — DTZ root probe in cmd_go() BEFORE spawning helper threads (not
             thread-safe — must run serially). On DTZ success: print bestmove
             and return early, no search spawned. syzygy_for_threads Arc cloned
             into main + N-1 helper SearchInfos for WDL during search.

---

## Phase 16 — NORU NNUE (Optional Enhancement) ⏳
- [x] 16.0 — Bug fix (Session 105, D110): `eval/mod.rs`'s
      `evaluate_blended()` doc comment still cited D23's original 25%
      default; actual runtime default has been 0% since D25 (NNUE
      further shelved by D61). Comment-only fix, no functional change.
      Found by Engineering Review (F-4). See DECISIONS.md D110.
- [x] 16.1 — Added noru = "2.2" to Cargo.toml (ordinary [dependencies],
            confirmed WASM-safe by upstream, no wasm32 exclusion needed
            unlike pyrrhic-rs).
- [x] 16.2 — src/nnue/features.rs: 896-input feature set defined.
            768 piece-square (kind*128 + relative_color*64 + relative_sq)
            + 128 pawn-start (768 + relative_color*64 + relative_sq).
            Perspective flip via Square::mirror_rank() for Black.
            9 tests: range checks, own/opponent distinction, symmetric
            start-pos parity, pawn-start presence/absence (D11), 1000-seed
            no-panic sweep.
- [x] 16.3 — src/nnue/delta.rs: compute_move_changes() mirrors every
            make_move() match arm (quiet/capture/en passant/castle/promo)
            to produce board-space add/remove events without a full 64-sq
            rescan. render_for_perspective() turns those into NORU-ready
            (added, removed) index lists. Verified equivalent to full
            extract_features() re-extraction across a 300-seed x 6-move x
            2-perspective sweep. Accumulator not yet wired into Position/
            search — that's 16.6, once a trained network exists.
- [x] 16.4a — src/bin/selfplay.rs: self-play data generator (see Session 22).
             .github/workflows/selfplay.yml: workflow_dispatch job, inputs
             for num_games/seed_start, builds + runs the binary, uploads
             selfplay_data.txt as a build artifact. Triggered manually from
             the Actions tab — mobile-friendly, no terminal needed.
             (Gokul is mobile-only, can't `cargo run` locally).
- [x] 16.4b — src/bin/lichess_sample.rs + .github/workflows/lichess_sample.yml
             CONFIRMED WORKING AT FULL SCALE (Session 27: 500/500 test run,
             then 50000/50000 full run, 0 parse failures both times.
             Full run: 9,999,801 lines read, lichess_sample.txt = 9.6M,
             50000 rows). PHASE 16.4b COMPLETE.
- [x] 16.4c — Pawn start feature convergence design: CONFIRMED already
             implemented in Phase 16.2's features.rs, not a separate task.
             pawn_start_feature_index() + extract_features() enforce D11's
             rule exactly (feature drops the instant pawn_starts.started_here()
             is false). test_pawn_start_feature_drops_once_record_cleared()
             covers it directly. No code changes made — verification only
             (Session 38).
- [x] 16.5a — train_nnue.rs first run succeeded; added best-val-checkpoint
             tracking (session 30).
- [x] 16.5b — Kaggle 3000-game self-play run (~93 actual games after queue
             delay, 433,080 rows) hosted as a GitHub Release asset (25MB
             repo-upload UI can't hold 66MB — see D22), pulled via
             train_nnue.yml's new selfplay_urls input. Combined with 50k
             Lichess rows = 483,080 total training rows.
             RESULT: best epoch 8/10, val_loss=0.53776, train/val curves
             both monotonic and near-plateaued (vs. first run's clear
             epoch-3 overfit). PHASE 16.5 COMPLETE.
             Trained network artifact: nnue-pet-dragon-h32-a256-e10
             (nnue_pet_dragon_quantized.bin, 481K).
- [x] 16.6 — Integrate trained network into eval: BLEND chosen (D23), not
             replace. src/nnue/inference.rs loads embedded quantized weights
             via include_bytes!, runs Accumulator::refresh() +
             noru::network::forward(), converts raw i32 to centipawns via
             OUTPUT_SCALE(16)/400 (D14 inverse) — verified correct against
             the real weights file, no scale adjustment needed.
             eval::evaluate_blended() (25% NNUE weight, D23) wired into
             search via the alpha_beta::evaluate() delegation point;
             pure-HCE evaluate() untouched. CI green, 320/320 tests passing.
             Also fixed a pre-existing bug this exposed: info.time_allocated_ms
             was never wired to the real TimeManager hard limit in
             iterative_deepening(), so is_time_up()'s in-search abort was
             dead code for every real search (D24). NNUE's heavier per-node
             cost surfaced it via test_iterative_deepening_respects_time.
- [x] 16.7 — WASM-compatible inference. CONFIRMED (Session 47) — Gokul
             visited https://g-c-3.github.io/pet-dragon, engine replies with
             a move cleanly. Also note: Gokul directly edited web/index.html
             on main ("more dynamic") outside the normal delta workflow —
             confirmed still working live, synced into context (1521 lines
             as of Session 47). Future sessions: treat web/index.html as
             current/live, not stale, and re-fetch before any delta against
             it since it wasn't authored/reviewed by Claude.

---

## Housekeeping ⚠️
- [x] `.github/workflows/*.yml` — Node.js 20 deprecation, PARTIALLY resolved
      (Session 47). Bumped across all 7 workflow files: actions/checkout
      v4→v6, actions/upload-artifact v4→v6, actions/download-artifact
      v4→v7 (all confirmed Node24-native as of this session). Left
      unchanged, no upstream fix available yet as of Session 47:
      actions/configure-pages@v4, actions/upload-pages-artifact@v3,
      actions/deploy-pages@v4 (deploy.yml — GitHub hasn't shipped a Node24
      major yet), softprops/action-gh-release@v2 (build.yml's release job —
      confirmed no Node24-compatible release exists upstream; consider
      replacing with a direct `gh release` CLI step in a future session if
      the warning becomes blocking rather than cosmetic).
      Swatinem/rust-cache@v2 needed no change — it's a floating major tag
      that already picked up Node24 support upstream (v2.9.0).
- [x] `README.md` — stale and factually wrong (Session 72). Fixed: "Play
      against Stockfish directly in your browser" corrected to Pet
      Dragon's own engine (was never Stockfish — copy-paste leftover from
      early scaffolding, never caught before). Project Status table
      replaced — it still showed every phase as 🔄/⏳ pending despite
      Phases 0-22 being complete. Softened the flat "3000+ Elo" claim to
      match DECISIONS.md's own framing (relative internal comparison, no
      external rating pool exists for a custom variant); cited the real
      ~39 Elo Texel-tuning gain from Session 61's 520-game pinned-ref
      result instead of an unqualified target number. Added an honest
      NNUE line (implemented, disabled by default, doesn't yet beat HCE).
- [x] Release pipeline — `build.yml` only ever published one rolling
      release under the literal tag `latest`; there was no way to cut a
      real versioned release at all. Fixed (D46, Session 72): tag pushes
      matching `v*.*.*` now trigger their own frozen, "Latest"-badged
      release; ordinary `main` pushes keep updating the separate rolling
      `latest` release without stealing the badge back. Also added a
      `build-wasm` job producing `pet_dragon_bg.wasm`, `pet_dragon.js`,
      and a base64-embedded `pet_dragon_standalone.js` (needs
      `pet_dragon.js` alongside it — genuinely single-file bundling was
      considered and explicitly rejected, see D46 addendum below) as
      release assets alongside the 4 native binaries.
      **CONFIRMED (Session 73)**: first tagged release attempt `v3.0.0`
      failed silently — tag was created before the `build.yml` fix
      landed on `main`, so the old workflow (no tag trigger at all) never
      ran; Gokul had manually attached 2 files by hand, no automation
      involved. Diagnosed via GitHub API/Actions history cross-check,
      deleted, retagged as `v3.3.3` with the fixed workflow already
      confirmed live on `main` — this time the pipeline ran correctly
      end-to-end: all 7 assets (4 native + `pet_dragon_bg.wasm` +
      `pet_dragon.js` + `pet_dragon_standalone.js`) attached, "Latest"
      badge applied, generated body correct. World-release housekeeping
      fully closed.

---


## Phase 17 — Elo A/B Testing & NNUE Retraining ✅ CLOSED (parked, D34/D41 — optional enhancement, not blocking)
- [x] 17.1 — NNUE blend weight made runtime-configurable via UCI
             `NNUEWeight` option (spin, 0-100, default 25 = D23). Replaces
             the compile-time `NNUE_BLEND_WEIGHT` const. weight=0 skips the
             NNUE forward pass entirely (pure-HCE arm pays zero NNUE cost).
- [x] 17.2 — src/bin/match_runner.rs (auto-discovered by Cargo, no Cargo.toml
             change). Plays N games between Engine A/B (different NNUEWeight
             configs), alternating colors each game to cancel first-move
             advantage. Separate TT per color-per-game (never shared between
             differently-weighted evaluators — a frozen TT score is only
             valid for the evaluator that produced it). Reports W/L/D +
             standard logistic Elo diff from Engine A's perspective.
- [x] 17.3 — .github/workflows/match_runner.yml — workflow_dispatch inputs
             for num_games/weight_a/weight_b/movetime_ms/seed_start, uploads
             match_results.txt as a build artifact. Mirrors selfplay.yml's
             convention exactly.
- [x] 17.4 — Ran 0% vs 25%, 20 games/100ms/move: A (0%) scored 87.5%,
             +338 Elo. Default weight dropped to 0% (D25). NNUE blend
             mechanism stays available as a UCI option for future retests.
- [~] 17.5 — SUPERSEDED, not literally completed as originally scoped.
            The sub-items 17.5a-17.5f (all [x] below) fully executed the
            NNUE side of this investigation and 17.6 formally parked it
            (D34). The one piece of the original 17.5 ask that was never
            built — a genuine pre/post-Texel-tuning HCE Elo number — is
            still open, restated in 17.7's caveat below. Left as [~]
            rather than [x] so this gap isn't silently lost; it needs a
            DECISIONS.md entry and explicit approval before any
            infrastructure (runtime-loadable HCE weights, or a second
            pinned-ref binary) gets built for it.
            Original Session 56 (post Phase-14 Texel tuning) note: discovered
             match_runner.rs can only A/B two NNUE blend weights of the
             SAME compiled binary — it CANNOT measure true before/after
             Elo for a Texel-tuning round, since HCE weights are
             compile-time consts in eval/*.rs, not runtime-swappable, and
             the pre-tuning Ethereal-derived values are gone once a tuning
             delta lands. Recommended immediate substitute: weight_a=0
             (tuned HCE) vs weight_b=100 (pure NNUE) — new, useful signal,
             zero new code, but NOT a real "did tuning help" measurement.
             If a genuine pre/post-tuning Elo number is ever wanted, it
             needs deliberate design (likely: runtime-loadable HCE weight
             tables, or a second binary built from a pinned pre-tuning git
             ref playing over UCI) — worth a DECISIONS.md entry before
             building, given the added runtime complexity trade-off for a
             one-time measurement. Not started.
- [x] 17.5a — D26 sweep complete (Session 39). A=0% vs B={5,10,15,20}%,
            40 games each, seed_start=0. ALL four net-negative for B:
              5%:  A 65.0% (+107.5 Elo)
              10%: A 75.0% (+190.8 Elo)
              15%: A 70.0% (+147.2 Elo)
              20%: A 80.0% (+240.8 Elo)
            No safe low weight exists — even 5% is decisively bad, not
            noise. Conclusion (D27): the network itself is the bottleneck,
            not the blend ratio. Retraining is required before any nonzero
            weight is viable again.
- [x] 17.5b — Retrain with more self-play data. COMPLETE (Session 42).
            286,659 total rows (236,659 self-play across 4 Kaggle batches,
            750 games each seeds 100/200/300/900, + 50,000 lichess).
            Result: val_loss 0.53776 → 0.51661 (best epoch 5/10, was epoch
            8/10 before). Real improvement, not noise — though train/val
            divergence after epoch 5 shows the small-dataset overfit
            pattern persists, just with a better floor. Artifact
            nnue-pet-dragon-h32-a256-e10 (run 28865459160, artifact ID
            8137571554) uploaded successfully.
- [x] 17.5c — Embed + re-sweep COMPLETE (Session 43). Result is the
            OPPOSITE of hoped for — every weight got worse, not better:
              5%:  A 67.5% (+127.0), was 65.0% (+107.5)
              10%: A 80.0% (+240.8), was 75.0% (+190.8)
              15%: A 78.8% (+227.6), was 70.0% (+147.2)
              20%: A 90.0% (+381.7), was 80.0% (+240.8)
            val_loss improved (0.53776 -> 0.51661) but game strength at
            every blend point got monotonically worse. D29: pausing the
            "more data" lever — tried twice now with the same
            better-loss/worse-Elo pattern both times. NNUEWeight stays 0%.
- [x] 17.5d — Direct calibration diagnostic COMPLETE. eval_diag.rs +
            eval_diag.yml built (Session 44), then CORRECTED (Session 45,
            D32) after Gokul caught that the original test cases used a
            classic-chess-layout FEN — astronomically rare under Pet
            Dragon's real random rank-1/2 generator (setup.rs), so it was
            testing an out-of-distribution input. Rewritten to use
            Position::generate_with_seed(N), the same generator selfplay.rs/
            match_runner.rs actually use, giving a trustworthy in-distribution
            read for the first time.
- [x] 17.5e — Root cause found (D30, Session 44): unregularized BCE weight
            blowup, not feature design. Confirmed via corrected eval_diag:
            even real in-distribution positions (seed=2, seed=3) were fully
            saturating at whatever clamp ceiling was in place. Fix: added
            NNUE_EVAL_CLAMP_CP=1500 in inference.rs (safety net) — measured
            Elo improvement at 10%/20% weight even with 0.0001 weight_decay
            (too weak to fix the root cause but clamp alone helped).
- [x] 17.5f — Real regularization (D33, Session 46): weight_decay raised to
            0.01 (1e-4 confirmed insufficient — seed=2/3 still saturated
            at the clamp with it), plus global-norm gradient clipping
            (grad_clip_norm=1.0) added as a second, more direct mechanism.
            Result: val_loss held steady (0.51636, matching/slightly
            beating the unregularized retrain) WHILE calibration improved
            substantially (seed=2: 1500→375, seed=3: 1500→50, K+P: 1225→225
            — see eval_diag.rs output). Confirms regularization fixed the
            calibration problem without trading away fit quality.
- [x] 17.6 — PHASE 17.5 PARKED (D34, Session 46). Final re-sweep against
            the clamped+regularized network: avg opponent score 70.3% (5%:
            66.2%, 10%: 68.8%, 15%: 66.2%, 20%: 80.0%) — statistically
            indistinguishable from clamp-only's 70.0% average. Despite real,
            measured calibration improvement (eval_diag), Elo impact hasn't
            moved beyond the ~70-72% band across 4 independent attempts
            (original network, 3x-data retrain, clamp, clamp+regularization).
            That consistency across otherwise-different fixes points at
            hidden_size=32 being too small to learn anything HCE's hand-
            crafted eval doesn't already capture — a structural ceiling, not
            a magnitude/calibration bug. NNUEWeight stays 0% (default,
            unchanged since D25). Parking further NNUE tuning at this scale;
            revisit only with a meaningfully bigger hidden_size (128+,
            bigger Kaggle job) as a deliberate separate effort, not
            incremental tuning. Everything built in 17.5a-f stays permanent:
            the clamp is a safety net worth keeping regardless of weight,
            weight_decay/grad_clip_norm are now correct defaults for any
            future NNUE training, eval_diag.rs is a reusable diagnostic tool.
- [x] 17.9 — RE-PARKED (D41, Session 69). Tested 17.6's stated revisit
            condition: hidden_size=128 (4x capacity), same 286,659-row
            dataset as the parked hidden_size=32 baseline, everything else
            unchanged — a clean single-variable test. Result: val_loss
            0.51655 (best epoch 3/10) vs baseline's 0.51636 (epoch 4/10) —
            slightly worse, and overfit one epoch earlier (val_loss climbed
            to 0.53169 by epoch 10). More capacity did not break the
            ceiling; it just overfit the same limited signal faster. No
            match_runner sweep run — val_loss regression already rules the
            network out, and re-confirming a negative result costs Actions
            minutes for no new information (D19/D20). This is the 5th
            independent lever (after 17.6's 4) landing in the same place.
            NNUEWeight stays 0%. Phase 16/17 CLOSED as an optional,
            not-currently-worthwhile enhancement — the core engine (Phases
            0-20) is complete and Elo-validated without it. Revisit only
            alongside a genuinely larger, dedicated self-play data effort
            (500K-1M+ rows), not another architecture-size bump alone.
- [x] 17.7 — Session 56: 3 match_runner.yml runs, post Phase-14 Texel
            tuning, using the EXISTING tool (see note below on its real
            limitation). All 3 confirm tuned-HCE (0%) beats every NNUE
            config decisively, same direction as 17.4/17.6, and by a
            comparable-or-larger margin:
              0% vs 25%,  40 games: A 78.8% (+227.6 Elo)
              0% vs 100%, 20 games: A 95.0% (+511.5 Elo)
              0% vs 100%, 40 games: A 97.5% (+636.4 Elo) — agrees with the
                                    20-game run in the same matchup, and
                                    got MORE lopsided at 2x the sample
                                    size, a good sign it's a real effect
                                    not small-sample noise.
            IMPORTANT CAVEAT (discovered this session): match_runner.rs
            can only A/B two NNUE blend weights of the SAME compiled
            binary — it cannot produce a true pre/post-tuning HCE Elo
            number, since HCE weights are compile-time consts and the
            pre-tuning Ethereal-derived values are gone once the tuning
            delta landed (Session 55). These 3 results confirm tuned-HCE
            is strong in absolute terms against NNUE at any blend weight
            (reinforcing D25's 0%-default decision with fresh data), NOT
            "tuning improved HCE by X Elo" — that number doesn't exist
            and would need new infrastructure to ever measure (see note
            left in this file previously — runtime-loadable HCE weights,
            or a second binary from a pinned pre-tuning git ref over UCI
            — worth a DECISIONS.md entry before building, not a quick add).
- [x] 17.8 — D36 (Session 57-60): built and validated the pinned-ref UCI
            match infra flagged in 17.7's caveat. Files:
            `src/bin/uci_match_runner.rs`, `.github/workflows/uci_match_runner.yml`.
            Verified trustworthy (Session 59): diffed every file — not just
            eval — between `pre_tuning_ref=c9905a22ed018c6c8332bef275aff548a1d0de70`
            and `main`; only the 7 expected eval-tuning files differ, both
            binaries default to 0% NNUE blend identically.
            FINAL RESULT — pooled across 7 runs, 520 games total (5 small
            runs of 20-40 games each, then 2 large 200-game confirmatory
            runs; 100ms/move throughout): pre-tuning (A) 225 wins,
            post-tuning (B, current `main`) 283 wins, 12 draws. A score
            44.4% → aggregate Elo diff ≈ −38.9 (A vs B), i.e. Texel-tuned
            HCE is ~39 Elo STRONGER than the original Ethereal-derived
            hand-picked values, pooled. The two 200-game runs alone landed
            at −41.9 each, essentially identical to each other — tight,
            consistent, well-powered. RUN 1's early +147.2 outlier
            (single seed, 20 games) is now clearly explained as small-
            sample noise, not signal. No tuner bug, no revert — Phase 14's
            tuning is working. This is the pre/post-Texel-tuning Elo
            number 17.5/17.7 flagged as missing — closed for real.

---

## Phase 18 — UCI Protocol Completeness (Pondering) ✅ COMPLETE
- [x] 18.1 — D37 (Session 62): found and closed a real UCI-compatibility
            gap during a general "what's missing besides NNUE" review, not
            from a failing test. `search/time.rs`'s `allocate_time()`
            already special-cased `tc.ponder` to search near-infinitely
            (that half was already correct, pre-existing) — but
            `ponderhit` wasn't handled anywhere in `main.rs`'s command
            dispatch at all. A real pondering-capable GUI would get an
            unrequested `bestmove` mid-ponder instead of the engine
            switching to a real, clock-bounded search on `ponderhit`.
            Fix: two new `Arc<AtomicU64>` fields on `SearchInfo`
            (`ponder_hit_soft_ms`/`ponder_hit_hard_ms`, threaded the same
            way `stop_flag` already is, D4-style) let `cmd_ponderhit`
            (main thread) hand the running search thread a real deadline
            — expressed relative to the search's own `start_time` (not
            reset to zero; pondering time is free per spec, and
            `start_time` is owned by the search thread so it can't safely
            be reset from another thread — see D37 for the full mechanism
            and the rejected start_time-reset alternative).
            Files changed: `src/main.rs`, `src/search/mod.rs`,
            `src/search/iterative.rs`.
            Verified: `cargo check --release` clean; full suite green,
            335 lib + 22 bin tests (up from 329/17 — 12 new, 0 regressed,
            0 pre-existing tests touched); manual end-to-end UCI runs
            against the real compiled binary — pondered 500ms then
            `ponderhit` correctly bounded the search to ~2.2s (matching
            the 60s-clock-implied soft limit) instead of running forever;
            plain `go movetime` and `go ponder` + `stop` (ponder miss)
            both unaffected.
            Caught and fixed a bug in the FIRST version of the
            integration test during this same session, before it shipped
            — see D37's verification section for what happened and why.
- [x] 18.2 — Self-containment audit (same session, same review): confirmed
            NNUE weights are embedded via `include_bytes!` (no external
            model file needed at runtime); the only optional external-file
            dependency is `SyzygyPath` for tablebases, which is standard/
            expected for any UCI engine with tablebase support. No other
            runtime dependency gaps found. No code changes needed — this
            was a verification pass, not a build task.

---

## Phase 19 — Analysis GUI UCI Options (MultiPV, Move Overhead) ✅ COMPLETE
- [x] 19.1 — D38 (Session 63): MultiPV — report N candidate lines instead
            of 1. Standard root-move-exclusion technique: search the
            primary line normally (fully unmodified code path), then
            re-search from the root excluding already-found moves once per
            extra line, same depth, full window (no per-line aspiration
            state — simpler, and MultiPV usage already accepts being
            slower than single-PV as the cost of extra lines). New
            `SearchInfo` fields: `multipv: usize` (default 1),
            `root_exclude: Vec<Move>`. The root-only exclusion check in
            `alpha_beta.rs`'s move loop shares space with singular
            extension's differently-scoped `excluded: Move` parameter
            without colliding — singular verification is gated `!root_node`,
            MultiPV's check is gated `root_node`, confirmed by reading the
            existing guard before writing the new one.
            Entirely additive: gated behind `multipv > 1`, false for every
            existing caller by default, so nothing about the single-PV
            path changed — not even reformatted.
            Files changed: `src/search/mod.rs`, `src/search/alpha_beta.rs`,
            `src/search/iterative.rs`, `src/main.rs`.
            A test caught a real (if expected) surprise: MultiPV>1 runs
            can pick a *different* primary-line move than MultiPV=1 at the
            same depth, because extra lines searched at earlier depths
            feed the same shared TT/history tables the primary line then
            reads. Not a bug — matches Stockfish's own documented caveat —
            but the first version of the test wrongly asserted move
            identity; fixed to assert what's actually guaranteed (legality).
            Full writeup: D38.
- [x] 19.2 — D38 (same session): Move Overhead — `search/time.rs`'s
            `OVERHEAD_MS` was a hardcoded constant; now
            `TimeControl::overhead_ms`, runtime-configurable via
            `setoption name Move Overhead`, defaulting to the same value.
            Files changed: `src/search/time.rs`, `src/main.rs`.
            Caught and fixed a real pre-existing bug in `cmd_setoption`
            while touching it for this: the old parser assumed single-word
            option names and values (`tokens[2]`/`tokens[4]` at fixed
            positions), which silently mis-parsed "Move Overhead" itself
            (two words) and would have truncated any multi-word value
            (e.g. a spaced Windows SyzygyPath) to its first token. Rewrote
            to find the `"value"` token and join everything on each side —
            backward-compatible with every existing single-word case.
            Verified: `cargo check --release` clean; full suite green,
            345 lib (was 335) + 30 bin (was 22) = 375 total, 18 new, 0
            regressed; manual end-to-end UCI runs against the real
            compiled binary confirmed both features working (sorted
            distinct-move MultiPV lines matching `bestmove`; default
            behavior unchanged; Move Overhead 2000 on movetime 3000
            correctly finished in ~1s not ~3s).

---

## Phase 20 — Difficulty / Skill Levels ✅ COMPLETE & VALIDATED
- [x] 20.1 — D39 (Session 64, scoping discussion only — no code this
            session): build depth-cap difficulty tiers (`Skill Level
            0..N` or similar), optionally with a little move-selection
            noise at the low end matching Stockfish's actual `Skill
            Level` mechanism (weighted randomness among top candidates,
            not just a lower depth cap) rather than inventing something
            new. Explicitly NOT `UCI_Elo`-style — no calibrated
            human-comparable number attached to any tier, for the same
            reason noted throughout this project: no external rating pool
            exists for this variant.
            Considered and rejected: borrowing standard-chess Elo
            calibration tables for the one Pet Dragon opening that visually
            resembles the standard starting array. Doesn't work even for
            that one opening — Pet Dragon's custom pawn rules apply from
            move one, so a visually standard start doesn't mean the game
            plays like real chess from there on; external Elo tables were
            built against real chess rules and don't transfer. Full
            reasoning: D39.
            Validation plan when built: same methodology as D36 — use the
            existing `uci_match_runner.rs` harness across many seeded
            positions to empirically confirm tiers are correctly ordered
            and reasonably spaced (tier K vs K+1 should win convincingly
            and consistently). No new measurement infrastructure needed,
            reuses what already exists.
            Session 66: IMPLEMENTED — see 20.3 below for what got built.
            Move-selection noise (the "optionally" above) was NOT built —
            depth cap + time fraction (20.2) covers the core requirement;
            noise is deferred, not rejected (see 20.3 notes).
- [x] 20.2 — Session 65 refinement (still scoping, no code): depth cap
            alone has a real rough edge — it doesn't touch time at all, so
            a low tier would still use whatever time the GUI/clock gives
            it, just to search shallower. Concretely: `go movetime 5000`
            at a tier capped to depth 6 could finish in ~50ms and sit idle
            for the rest — a "beginner" bot instaflying moves against a
            human who gave it 5 seconds looks broken, not weak. It also
            wastes think time that the move-selection-noise half of the
            mechanism (20.1) benefits from having — weighted-random choice
            among top candidates works better with at least some real
            search behind it, not an instant return.
            Fix: use BOTH, not depth alone — depth as the primary strength
            ceiling (that's what actually caps how well it can find
            moves), plus a tier-dependent fraction of the normal time
            budget so low tiers also visibly "try less hard," not just
            "see less far." Wiring: a `Skill Level` UCI option feeds both
            a `max_depth` override AND a time-fraction multiplier into
            `allocate_time()`'s output, before `TimeManager` sees it —
            same pattern as `Move Overhead` (D38), just tier-driven
            instead of a flat user-set value.
            Session 66: IMPLEMENTED — see 20.3.
- [x] 20.3 — Session 66: implementation. Built `src/search/skill.rs` —
            21 levels (0..=20, matching the familiar `Skill Level` spin
            shape GUIs expect — option SHAPE only, no borrowed calibration
            data). `skill_depth_cap(level)`: `None` at level 20 (default,
            fully uncapped — byte-identical to pre-Phase-20 behavior),
            `level + 1` for 0..19 (so level 0 = depth 1, level 19 = depth
            20). `skill_time_fraction_pct(level)`: `100` at level 20,
            `(10 + level*5).min(98)` for 0..19 — capped strictly below
            100% so every reduced tier is distinguishable from "off."
            Wired: `iterative_deepening()`'s `max_depth` now takes
            `.min()` against the tier's depth cap (never overrides an
            EXPLICIT shallower `go depth`, only ever reduces further).
            `allocate_time()` in `time.rs` now takes a new
            `TimeControl::skill_time_fraction_pct` field and applies it to
            the movetime branch, the clock-based (movestogo/sudden-death)
            branch, and the no-clock-info default fallback — deliberately
            NOT applied to `infinite`/`ponder` (analysis, not strength) or
            the fixed-depth/fixed-nodes sentinel branches (already governed
            by the depth cap instead). `main.rs`: new `Skill Level` UCI
            spin option (default/max 20, matching `MAX_SKILL_LEVEL`),
            `setoption name Skill Level` handler (two-word name, same
            parsing path as `Move Overhead`), applied in `cmd_go` to both
            `main_info.skill_level` AND every helper thread's
            `h_info.skill_level` (helpers must respect the same cap, or
            they'd populate the shared TT with full-strength lines that
            leak back into a low-skill main search).
            NOT built this session: move-selection noise (weighted-random
            choice among top MultiPV candidates, the Stockfish-style
            mechanism 20.1 flagged as optional). Depth cap + time fraction
            alone should already produce clearly distinguishable tiers;
            noise is a plausible follow-up if match-runner validation
            shows tiers are correctly ordered but too close together in
            practice, not committed to yet.
            STILL PENDING — the actual empirical validation: this session
            could not run `uci_match_runner.rs` (no Rust toolchain in this
            environment; GitHub Actions handles all building/testing per
            project convention). All new logic has unit tests (skill.rs's
            monotonicity/boundary tests, iterative.rs's depth-cap-vs-
            explicit-depth tests, time.rs's fraction-scaling tests,
            main.rs's option-wiring tests) and was reviewed by hand against
            the existing test suite's patterns, but NONE of that is a
            substitute for actual games — tier K beating tier K+1
            convincingly and consistently is an empirical claim 20.1/20.2
            always said needs match-runner confirmation, not code review.
            NEXT SESSION START POINT: commit `skill.rs` (new),
            `search/mod.rs`, `search/iterative.rs`, `search/time.rs`,
            `main.rs` (all REPLACE), let GitHub Actions confirm the full
            suite is still green, then run `uci_match_runner.rs` across
            multiple seeds for several tier pairs (at minimum: level 0 vs
            5, 5 vs 10, 10 vs 15, 15 vs 20) to confirm monotonic, convincing
            win rates before calling any tier "done." If a pair is too
            close, that's the point to reconsider move-selection noise.
            UPDATE (same session, follow-up): `uci_match_runner.rs`/
            `.yml` (D36) could NOT actually run these tier-pair comparisons
            as they stood — that harness builds two separate git-ref
            binaries and never sends a `setoption` to either, so both
            engines always ran at compiled-in defaults regardless of the
            workflow's inputs. Extended both: harness gets a new
            `EngineProcess::configure()` (sends `;`-separated `setoption`
            lines once after the UCI handshake, before any games) plus two
            new trailing CLI args; workflow gets matching
            `engine_a_uci_options`/`engine_b_uci_options` inputs. To run a
            tier comparison: set `pre_tuning_ref`/`post_tuning_ref` BOTH to
            `main` (same build twice) and only vary the two UCI-options
            inputs, e.g. "setoption name Skill Level value 0" vs
            "setoption name Skill Level value 5". Match-summary labels
            changed from "pre-tuning (ref)"/"post-tuning (ref)" to
            "A (ref | options)"/"B (ref | options)" to stay readable for
            this use case — flagged since it touches D36's existing output
            format, though scoring/Elo math is untouched.
            NEXT SESSION START POINT (updated): commit
            `src/bin/uci_match_runner.rs` and
            `.github/workflows/uci_match_runner.yml` (both REPLACE) on top
            of the Phase 20 files, confirm green, then actually run the 4
            tier-pair comparisons above before calling any tier "done."
- [x] 20.4 — Session 67: ran the validation. First two attempts were
            invalidated by test-setup mistakes, not code bugs (see
            SESSION_LOG for the full story — GitHub's mobile "Run
            workflow" form carrying over stale field values, and UCI-
            options fields needing the FULL `setoption name Skill Level
            value N` line rather than a bare number, which the engine
            silently ignores as an unrecognized command). Once corrected
            and verified via the workflow log's command echo:
            0 vs 5: Elo -381.7 (tier 5 wins ~90%) — strong.
            5 vs 10: Elo -436.4 (tier 10 wins ~92.5%) — strong.
            10 vs 15: Elo -8.7 (48.8%) — statistical tie, NOT separated.
            15 vs 20: Elo -52.5 (57.5%) — real but modest.
            Diagnosis: not a bug — a well-known engine-strength-vs-depth
            shape (huge Elo gains in the first few plies, fast-diminishing
            returns once the search is already reasonably deep for the
            time budget; Stockfish's own Skill Level 15-20 have the same
            closeness for the same reason). Depth-cap tiers 10+ landed in
            the region where extra depth barely changes the chosen move
            anymore.
            Fix (Gokul deferred the specific choice to Claude): built the
            move-selection noise mechanism 20.1 always flagged as the
            fallback for this exact situation, rather than just
            compressing the option's upper range or leaving it
            undocumented — noise fixes the actual separation problem
            instead of hiding it. `skill.rs` gained
            `skill_noise_window_cp(level)` (0 at level 20 = no-op, `(20 -
            level) * 8` cp for 0..19) plus a small embedded xorshift64 PRNG
            (no new crate dependency) and `pick_noisy_move_index()` —
            picks uniformly among root candidates within the window rather
            than always the single best move. `iterative.rs` wires it in
            after the main depth loop, reusing the existing Phase 19
            `search_multipv_slot()` machinery to gather alternative root
            candidates rather than duplicating a second root-search path.
            IMPORTANT: noise applies to every capped tier (0-19), not just
            the two problem pairs — the 0-vs-5/5-vs-10 numbers above are
            now stale relative to the shipped code and need re-validating
            alongside 10-vs-15/15-vs-20, not just the pairs that failed.
            NEXT SESSION START POINT: commit `search/skill.rs` and
            `search/iterative.rs` (both REPLACE), confirm green, then
            re-run ALL FOUR tier pairs (same methodology: full setoption
            lines, movetime 1000, confirm the log echo before trusting any
            result) and compare against this session's pre-noise baseline
            above. If 10-15/15-20 are still too close even with noise, the
            next lever is widening the noise-window formula's coefficient
            (currently a flat `*8` per level), not a structural rewrite.
- [x] 20.5 — Session 67 (cont.): re-validation surfaced a real bug in
            20.4's noise mechanism. 0-vs-5 and 15-vs-20 improved with noise
            (Elo -759.1 and -240.8 respectively — both correct direction,
            more decisive than before). But 5-vs-10 came back INVERTED
            across three separate runs (40/50/60 games, 150 total, 57%
            cumulative in the wrong direction) — Skill Level 5 beating
            Skill Level 10, ruling out sampling noise as the explanation.
            Reviewed for state leakage and time-budget overruns first,
            found neither. Root cause: `skill_noise_window_cp()`'s flat
            centipawn threshold doesn't account for root-move score gaps
            shrinking as search gets deeper — level 10's nominally
            TIGHTER window (80cp @ depth 11) was catching MORE eligible
            candidates in practice than level 5's nominally WIDER window
            (120cp @ depth 6, where shallow search's larger natural score
            gaps rarely fell inside 120cp at all). Deviation frequency was
            an accidental side effect of depth-dependent score clustering,
            not actually controlled by Skill Level as intended.
            Fix: added `skill_noise_trigger_pct(level)` — `(20 - level) *
            4`, checked BEFORE the cp window — so deviation FREQUENCY is
            now a direct, depth-independent function of Skill Level; the
            cp window applies only after a triggered roll, as a safety
            bound. Regression-guard test added pinning
            `trigger_pct(5) > trigger_pct(10)` specifically.
            NEXT SESSION START POINT: commit the fixed `search/skill.rs`
            (REPLACE — `iterative.rs` unchanged, no need to re-commit),
            confirm green, re-run 5-vs-10 first (50+ games) to confirm the
            fix, then re-run the other three pairs for a full clean ladder
            before calling Phase 20 validated.
- [x] 20.6 — Final validation, 200 games per pair (large enough sample to
            trust): 0v5 -619.4 Elo (97% for tier 5), 5v10 -117.2 Elo (66%
            for tier 10 — confirms the trigger_pct fix holds at scale,
            this was the pair that was backwards before), 10v15 -65.0 Elo
            (59% for tier 15), 15v20 -83.2/-79.5 Elo across two runs (62%
            for tier 20, consistent). All four pairs now correctly
            ordered — higher tier wins clearly and consistently, though
            never 100% (expected: Elo gaps are win probabilities, not
            guarantees, and the noise mechanism deliberately lets weaker
            tiers occasionally play a good-not-best move by design).
            Gokul asked whether a 5-level gap "should" mean the higher
            level always wins — clarified this is normal Elo behavior, not
            a defect, and that the real design question was whether a ~60%
            win rate (the 15-vs-20 gap specifically) is a big enough FELT
            difference for a player, separate from "is this correctly
            ordered." Decision (D40): don't touch the underlying 0-20
            mechanism — it's validated, monotonic, and matches the
            standard UCI `Skill Level` convention other GUIs expect if
            Pet Dragon is ever connected to a different frontend. Instead,
            the GUI should expose a small set of NAMED presets rather than
            a raw 0-20 slider, since no player can feel the difference
            between adjacent numeric levels anyway: Beginner=0, Easy=5,
            Medium=10, Hard=15, Master=20 — the exact five points already
            validated above. See D40 for the full reasoning.
            PHASE 20 COMPLETE. Skill Level is validated end-to-end: engine
            depth-cap + time-fraction + move-selection-noise mechanism
            (native UCI and WASM), GitHub Actions match-runner tooling
            capable of testing it, and a GUI-facing preset design that
            uses the validated data points directly.

---

## Phase 21 — UCI Completeness: Ponder, Contempt, Real Eval Bar ✅ (wasm build, cargo test, AND live eval-bar visual check all confirmed — fully closed)
- [x] 21.1 — Session 69: added `option name Ponder type check default
            true` to the UCI option list. Pure advertisement — no engine
            state, since pondering is entirely driven by whether the GUI
            sends `go ... ponder` (the underlying ponderhit/pending-
            allocation logic already existed and worked; this was purely
            a missing declaration some GUIs require before ever invoking it).
- [x] 21.2 — Session 69: added `Contempt` (UCI spin, -100..100, default
            0). `draw_score(ply, contempt)` in search/mod.rs derives the
            root-relative sign purely from `ply % 2` — no new root-side
            field needed anywhere. Applied at all 4 draw-detection sites
            in alpha_beta.rs (repetition, 50-move, insufficient material,
            stalemate). Full design reasoning in DECISIONS.md D42.
- [x] 21.3 — Session 69: added `search_from_fen_with_eval()` as a new,
            non-breaking WASM export (existing `search_from_fen` left
            completely untouched) returning the real search score/mate
            alongside the move. `web/index.html`'s eval bar now shows the
            genuine engine evaluation once a search completes, instead of
            only the material+mobility heuristic (heuristic kept as the
            instant-feedback fallback for the gap before a search runs).
            **CONFIRMED (still Session 70)**: `deploy.yml`'s wasm-pack build
            succeeded on runs #443 and #444 — `search_from_fen_with_eval`
            compiles cleanly to `wasm32-unknown-unknown` for real, not
            just Claude's field-check against `Position`/`SearchResult`'s
            definitions. **CONFIRMED (Session 71)**: Gokul supplied a
            screen recording of the live deployed page — the eval bar
            renders a real, changing numeric value (e.g. `+0.3`) across
            multiple engine moves, not a NaN, a stuck placeholder, or the
            heuristic fallback. Phase 21 fully closed.
- [x] 21.4 — Session 70 (later same day, D43): REVERSED from the
            earlier-this-session decline. Gokul explicitly chose to
            proceed with `UCI_LimitStrength`/`UCI_Elo` anyway, using
            self-assumed Elo anchors (0=1200, 20=2600) rescaled onto this
            project's own real measured tier gaps (Session 68's
            200-games/pair validation). `search::skill::ELO_TABLE` +
            `elo_to_skill_level()` built, `main.rs` wired
            `UCI_LimitStrength`/`UCI_Elo` as an override of Skill Level
            in cmd_go. D39 itself is untouched — only its rejection of
            attaching an Elo number was overridden; full reasoning,
            including exactly which of these 21 numbers are real vs.
            interpolated, is in D43. **CONFIRMED (still Session 70)**:
            `cargo test` log showed 388 passed, 0 failed for the lib,
            including `elo_to_skill_level`'s 5 exactness/clamping/
            tie-break tests and the `UCI_LimitStrength`/`UCI_Elo`
            setoption tests, confirmed by name, not just aggregate count.
- [x] 21.5 — Session 70 (still later the same day, D44): Gokul flagged
            that 21.2-21.4's `cmd_go` wiring tests only checked
            `EngineState` field non-mutation, not that the search thread
            actually received the configured values. Fixed by widening
            `wait_for_search()` to return the real joined `SearchInfo`
            and extracting `effective_skill_level()`/
            `build_time_control()` as pure, directly-testable functions.
            The extraction surfaced a real bug: `UCI_LimitStrength` was
            correctly overriding the depth cap but NOT the time-fraction
            budget (wrong computation order — see D44), meaning a low
            `UCI_Elo` request got a shallow depth cap paired with a full
            time budget, the exact "shallow-then-idle" failure Session 65
            built the depth+time pairing to prevent. Fixed. All 5
            affected tests rewritten to assert on real values; 1 new
            regression test added specifically for this bug. **CONFIRMED
            (still Session 70)**: same `cargo test` log —
            `test_build_time_control_uses_elo_derived_skill_level_not_raw`
            (the actual regression test for this bug) and
            `test_cmd_go_search_reflects_elo_override_not_raw_skill_level`
            (the end-to-end integration test) both confirmed passing by
            name, not just swept up in the aggregate 388-passed count.

**Housekeeping discovered this session — CORRECTED (still Session 70)**:
initially flagged the repo's checked-in root `index.html` as "what
GitHub Pages actually serves" and stale relative to `web/index.html`.
The staleness is real (root `index.html` still has the pre-Skill-Level
2-arg `search_from_fen(fen, ms)` call), but the "what GitHub Pages
serves" part was wrong — checked `deploy.yml` directly: it uploads only
the `web/` folder as the Pages artifact (`actions/upload-pages-artifact
@v3` with `path: web/`) and deploys via `actions/deploy-pages`, meaning
Pages serves `web/index.html` exclusively. Confirmed live via runs #443
and #444 both succeeding on this session's actual commits. Root
`index.html` is simply orphaned dead weight — never live, not
out-of-sync with anything that matters. **DONE (Session 71)**: Gokul
deleted the root `index.html`.

---

## Phase 22 — Repetition Detection Redesigned to Match Stockfish ✅ (tests/make_unmake.rs confirmed via real CI log — fully closed)
- [x] 22.1 — Session 70 (still later the same day, D45): replaced the old
            unbounded "scan all of game_history for any 2nd occurrence"
            with the actual Stockfish algorithm (`Position::set_state()`/
            `is_draw(ply)`), verified against the real Stockfish source
            before implementing rather than worked from memory.
            `game_history` changed from `Vec<u64>` to `Vec<(u64, i32)>` —
            each entry now caches a "repetition" distance at push time
            (Stockfish's `StateInfo::repetition` equivalent), making
            `is_repetition(ply)` an O(1) lookup instead of an unbounded
            scan on every draw check. The core behavioral change: a
            first repeat is only scored as a draw if it happened via
            moves the search itself chose (ply-relative), not when it's
            purely inherited from real game history predating the search
            root — a genuine repetition chain (3-fold-equivalent) is
            still always a draw regardless. Full algorithm, sign
            convention, and the `i=4` minimum-cycle-length reasoning are
            in D45.
- [x] 22.2 — Same session: fixed every consumer the type change touched —
            `alpha_beta.rs`'s draw-check call site, `iterative.rs`'s raw
            root-position push/pop (now uses the proper wrapper),
            `is_threefold_repetition()` (deliberately kept as a plain
            count — used for real game-end adjudication, not the search-
            tree-relative heuristic), and the now-dead-but-still-
            type-correct `set_game_history()` (zero callers anywhere,
            confirmed via grep, but rewritten correctly rather than left
            broken). Rebuilt every test that depended on the old raw-push
            API with genuine legal move sequences (a real 4-ply king-
            shuffle or knight-shuffle repetition cycle) instead of
            fabricated hash pushes, which no longer type-check against
            the new cached representation anyway. Added direct, precise
            unit tests for the algorithm itself in `position/mod.rs`
            (positive vs negative caching, the ply-relative distinction,
            the halfmove_clock bound) rather than relying only on the
            existing weak `alpha_beta.rs` integration assertion.
            **CONFIRMED (Session 71)**: Gokul ran a real CI `cargo test`
            (Actions log, rustc 1.97.0) and supplied the full output.
            `tests/make_unmake.rs` — 19 tests total, all passing,
            including the 5 rebuilt repetition tests by exact name:
            `test_no_repetition_at_start`,
            `test_pet_dragon_repetition_uses_pawn_start_hash`,
            `test_repetition_detected_after_moves`,
            `test_repetition_not_triggered_by_different_positions`,
            `test_threefold_repetition`. The `position/mod.rs` unit tests
            for the algorithm itself also confirmed passing:
            `test_is_repetition_chain_always_true_regardless_of_ply`,
            `test_is_repetition_false_when_repeat_predates_search_root`,
            `test_is_repetition_true_when_repeat_is_within_search_tree`,
            `test_is_threefold_repetition_still_uses_plain_count_not_ply`.
            Full suite: 396 lib tests + 125 bin/integration tests (across
            eval_diag, match_runner, pet_dragon, selfplay, texel_diag,
            texel_gen, texel_tune, train_nnue, uci_match_runner,
            make_unmake, node_count [5 ignored — perft depth ≥8, expected],
            perft, setup) = 521 total, 0 failed. Phase 22 fully closed.

---

## Phase 23 — Post-Release Improvement Roadmap ⏳ (Session 74, corrected against verified source — see DECISIONS.md D47)

*Established after a corrected competitive-analysis pass (Session 74).
An earlier draft incorrectly listed continuation history, correction
history, IIR, razoring, singular extensions, ProbCut, and best-move-
stability time management as gaps — all five were already implemented
as of Phase 13. See ENGINE_ARCHITECTURE.md §3 for the full verified
list of what's already in place. Every item below is confirmed absent
from actual source, not assumed. All five are implementable as
original code with no copyright concern — general algorithmic
techniques, not borrowed artifacts (see full rationale in the
improvement roadmap report).*

**⚠️ Complete these in the numbered order below — the numbering IS the
recommended execution order (23.1 → 23.5), not a topic grouping. Do
not skip ahead to 23.4/23.5 before earlier items are done; each later
item's ease/size estimate assumes the earlier ones are already in
place (23.4 assumes healthy self-play volume from 23.2; 23.3 assumes
23.2 has already confirmed the data-volume theory).**

- [x] 23.1 — Lightweight SPRT-style regression testing gate. DONE
             (Session 75, D48). `uci_match_runner.rs` gained an optional
             11th CLI arg (`min_score_pct`) that turns it into a
             pass/fail gate — exits 1 if the candidate's score against
             the baseline falls below the threshold, exits 0 otherwise
             (existing manual `uci_match_runner.yml` invocations are
             unaffected — they never pass this arg). New `regression-gate`
             job in `build.yml`, runs automatically on every PR (not
             manual-dispatch): builds the PR head ("candidate") and
             current `main` tip ("baseline"), plays 20 games at 50ms/move,
             fails the job if candidate scores below 35%. ⚠️ Not yet a
             hard merge block — Gokul still needs to mark
             `regression-gate` as a required status check in GitHub
             branch protection settings (Settings → Branches → main,
             from the mobile GitHub app or web) for it to actually block
             merges; the job runs and reports either way even before
             that's set.
- [x] 23.2 — Thread-differentiated Lazy SMP. DONE (Session 76, D49).
             Helper threads (`main.rs`)
             currently run identical search parameters, differing only
             in start timing and being time-unlimited. Vary LMR
             aggressiveness / move-ordering tie-breaks per thread ID so
             helpers explore genuinely different tree regions instead
             of largely duplicating the main thread. Smallest item with
             no dependencies — do it right after 23.1. Ease:
             Small–Medium (parameterize existing constants by thread
             ID, no new algorithm). Size: Small–Medium, scales with
             core count.
             New `SearchInfo.thread_id` field (default 0, set per-helper
             in `main.rs`'s spawn loop); `search::pruning::lmr_thread_base()`
             varies the LMR formula's base constant per thread (main
             thread pinned to the original 0.75, unchanged);
             `search::ordering`'s `thread_tie_break()` adds a small
             deterministic offset to quiet move scores, also zero for the
             main thread. Full rationale in `DECISIONS.md` D49. ⚠️ Elo
             impact not yet measured — 23.1's `regression-gate` job
             (20 games/50ms, single-threaded by default) isn't built to
             detect an SMP-scaling improvement; a manual
             `uci_match_runner.yml` run with `Threads` > 1 on both sides
             (higher game count) is the way to actually measure this, not
             yet done.
- [x] 23.3 — NNUE training data scale-up. DONE as scoped (data volume
             is no longer the open question — see below for what's now
             blocking NNUE instead). Code was complete and
             production-ready (`nnue/`, `evaluate_blended()`); both
             earlier tested network sizes lost to HCE (D34/D41), most
             plausibly from data starvation — roughly half a million
             combined self-play + Lichess rows vs. the hundreds of
             billions top-class engines train on.
             **Code half DONE (Session 77, D50)**: `selfplay.yml`
             rewritten from one sequential job (max ~3,000 games/run,
             the prior `n3000` artifact) into a sharded fan-out —
             `plan-shards` job builds a `[0..shards-1]` matrix from the
             `shards` input, `selfplay-shard` runs one independent
             self-play batch per shard on a disjoint seed range, then
             `merge-shards` downloads and concatenates every shard's
             output into one combined artifact (30-day retention) so
             Gokul only downloads one file per run, not `shards` of
             them, on mobile.
             **Compute half DONE (Session 79)**: Gokul ran 10 rounds
             of the sharded workflow (seeds 0-27000), producing
             2,428,608 fresh self-play rows — verified distinct by
             SHA-256 across all 10 files, non-overlapping seed ranges,
             only 1.79% incidental cross-file row duplication (normal
             noise, not a seed collision). Combined with 50,000 Lichess
             rows and trained via `train_nnue.yml`: best epoch 6/10,
             val_loss=0.50108, a real improvement over the old
             483,080-row run's 0.53776. **The data-starvation theory is
             now confirmed on the loss metric** — more data measurably
             helped the training objective.
             **But actual play strength still lost badly**: a 20-game
             pure-NNUE-vs-HCE match was a 20-0 shutout. `eval_diag.yml`
             found the raw network saturating at the ±1500cp clamp on
             6/8 test positions (including ones that should read near
             zero) — a calibration/training-config bug (see D52), not
             a data-volume or architecture-size problem. **New,
             better-understood blocker, tracked as its own item now**:
             see 23.3b below. 23.4/23.5 (which assumed 23.3 would
             settle the data-volume question one way or another) are
             now unblocked to proceed independently of whether 23.3b
             resolves NNUE, since they don't depend on NNUE specifically.
- [x] 23.3b — NNUE logit-saturation fix (D52 → D53/D54/D55, Session 80).
             DONE — fixed and re-parked, not left broken. Root cause
             confirmed (D53): BCE against the hard 0/1/0.5 game-result
             label has no finite minimizer, so `train_nnue.rs` rewarded
             pushing raw output toward the ±1500cp clamp regardless of
             true position value. Fix: `label_smoothing` param added to
             `train_nnue.rs` and `train_nnue.yml`, mapping the blended
             `[0,1]` target into `[label_smoothing, 1-label_smoothing]`
             before BCE — bounds the objective's own minimizer instead of
             only reacting to it via `weight_decay`/`grad_clip_norm`.
             Swept 6 values (`0.03/0.05/0.08/0.10/0.15/0.30`) against the
             2.48M-row 23.3 dataset, each validated with a real 20-game
             `uci_match_runner.yml` HCE-vs-NNUE match, not just
             `eval_diag.yml` — D54 found the two disagree, sometimes
             sharply (the best-looking diagnostic, 0.30, produced the
             *worst* match result). `label_smoothing=0.10` is a clean,
             well-bracketed local optimum (17W-2L-1D, 87.5%, +338 Elo for
             HCE) — real progress from the pre-fix 20-0 shutout, network
             is now sign-correct and non-degenerate, but still not
             competitive with HCE. Decision (D55): ship as-is.
             `nnue_pet_dragon_quantized.bin` committed at the
             `label_smoothing=0.10` checkpoint; `NNUEWeight` stays at 0%
             default (unchanged since D25) — network available as a UCI
             option for anyone who wants to enable it, not the default
             search path. Re-parked; see D55 for reopening conditions.

- [x] 23.3c — Cheap-lever follow-up on 23.3b (D56/D57/D58, Session 81).
             DONE, re-parked again. Scoped a pawn-flexibility feature
             redesign (D56, 5-bucket diff encoding replacing D11's 128
             per-square flags) but did not ship it — schema-breaking,
             forces a retrain window with red CI, judged not worth it
             given the person doing the committing explicitly wanted
             low-risk changes only. Shipped a cheaper alternative
             instead (D57, phase-balanced training-row oversampling,
             no schema change) and tested it — empirically negative:
             the row-imbalance hypothesis this whole line of work was
             based on turned out not to hold up against the actual
             training-data histogram, which was far flatter than
             assumed. No further tuning of that lever recommended.
             `label_smoothing=0.10` (D55) remains the best/committed
             checkpoint, unchanged by this round. See D58, not D55,
             for current reopening conditions — D58 supersedes D55's
             lever list. Remaining options (D56's redesign, a
             training-pipeline swap off NORU's CPU trainer, or
             Stockfish-distillation data augmentation for the
             converged-toward-standard-chess phase of a game) are all
             real structural investments now, not quick tries.
- [~] 23.4 — IN PROGRESS (D67 design, Session 84). Variant-specific
             opening statistics, bucketed by structural features. Full
             design in DECISIONS.md D67; bucket-count estimate corrected
             empirically in D71 (true count is several times larger than
             the original "420" — see D71, not a design problem, just a
             wrong estimate). Build order and status:
             1. **DONE (Session 84)** — `selfplay.rs` new output stream,
                `selfplay.yml` uploads/merges it as its own artifact.
             2. **DONE (Session 84)** — two real data collection runs,
                accumulated: `seed100000-15×800` (12,000 games) +
                `seed120000-15×1200` (18,000 games) = 30,000 games, no
                overlap, no data loss. Sparsity worse than even D71's
                corrected estimate suggested — see D72's note on growth
                rate. Still open-ended accumulation; more runs welcome
                whenever convenient, no fixed target.
             3. **DONE (Session 84)** — aggregation script built AND run
                for real against the 30,000-game combined dataset (run
                IDs `29984377150`, `29996467051`). Output: **2 qualifying
                entries** (both bucket→`a2a7`, a full open-file rook
                lift — internally consistent: both qualifying buckets
                have a rook on the a-file, which is what makes that move
                legal). `src/opening_stats.rs` generated and verified —
                see D72.
             4. Ongoing/open-ended — accumulate more `selfplay.yml`
                batches when convenient, re-run the aggregator (it
                naturally grows the table as more data clears the
                threshold — no code changes needed for this, just more
                data + a re-run).
             5. **DONE (Session 84)** — root-move-ordering bias wired into
                `search/ordering.rs` (D72): additive `OPENING_STATS_BONUS`
                on top of normal scoring, gated on `ply==0 &&
                fullmove_number==1 && White to move` (the literal unmoved
                game start, not just any search root — see D72 for why
                both conditions are required). **Caught and fixed a real
                panic bug before shipping** — the file-extraction helper
                assumed exactly 2 rooks/knights (valid for
                `selfplay.rs`'s guaranteed-fresh setups, NOT valid for
                `ordering.rs`'s arbitrary UCI-supplied positions); an
                existing test FEN with zero rooks/knights would have
                crashed the engine on any real analysis position matching
                the gate. Fixed to degrade gracefully (`Option`, not
                panic) — see D72.
             6. **DONE (Session 84)** — hand-verification test added
                (`test_opening_stats_bias_applies_to_known_bucket` in
                `search/ordering.rs`): a hand-constructed FEN matching
                table entry 207 exactly, traced by hand first (confirmed
                `a2a7` is always a rook-takes-rook capture in this
                bucket, not a quiet move — Black's mirrored setup
                guarantees a like-for-like piece on a7), then verified
                through actual `score_moves()` that the bonus applies
                additively and the flagged move ends up the single
                highest-scored move in the position (confirmed correct
                by hand: every other White piece in the constructed FEN
                is boxed in or has nothing to capture). Skips gracefully
                if a future table regeneration drops entry 207, rather
                than becoming a permanently-frozen data-dependent test.
             **All 6 build-order steps are now done or ongoing-by-design.**
             Remaining work is open-ended: step 4 (data accumulation)
             continues in the background whenever convenient, no fixed
             target — the table grows automatically as more data clears
             the threshold on future aggregator re-runs, no further code
             changes needed for that to happen.
             Resume by re-reading D67 + D71 + D72 if picking this back
             up — the design, bucket-count correction, wiring, and
             verification are all closed.
- [~] 23.5 — HELD for the future (D61, Session 82). NNUE architecture
             upgrade: king-relative bucketed features, replacing the
             current flat 768+128=896 input set. Shelving NNUE
             entirely (D61) makes this item moot as written — it's
             specifically an NNUE upgrade, so there's nothing to do
             here while NNUE itself is parked. Left in the roadmap
             rather than deleted so the full technical rationale
             (D55/D57/D58's data-volume analysis) isn't lost if NNUE
             work resumes later.
- [x] 23.6 — Singular extension family: multi-cut pruning + negative
             extensions (D59, Session 82). Extends Phase 13.3's base
             singular extension with two Stockfish-family siblings that
             reuse the same verification search result: multi-cut
             (verification still reaches singular_beta ⇒ at least one
             other move also refutes ⇒ prune the whole node early, same
             shape as probcut's early return) and negative extension
             (verification didn't confirm singularity but the TT move's
             own score already meets beta ⇒ reduce rather than extend
             the TT move, -1 in PV nodes / -2 in non-PV). `alpha_beta.rs`
             singular block generalized from a boolean `singular_extension`
             flag to a signed `tt_move_extension` (was always +1 or 0,
             now +1 / -1 / -2 / 0), applied only to the TT move via
             `move_ext` (renamed from `singular_ext`). Not gated behind
             ⚠️ — new unit tests added, no existing test touched this
             logic before (verified via grep), full rationale in
             `DECISIONS.md` D59. ⚠️ Elo impact not yet measured — same
             caveat as 23.2, needs a real `uci_match_runner.yml` run to
             quantify, not yet done.
- [x] 23.7 — Late Move Pruning (LMP) (D60, Session 82). New technique,
             distinct from LMR: skips late quiet moves outright at
             shallow depth (`MAX_DEPTH_LMP = 8`) once the quiet-move
             count for this node passes a depth-indexed threshold,
             instead of just reducing them. Non-PV only, never in
             check/giving check, never near mate-range alpha/beta —
             same guard shape as the existing futility-pruning block.
             `pruning::lmp_threshold()` / `should_apply_lmp()` (new,
             with unit tests), wired into `alpha_beta.rs`'s move loop
             right after futility pruning. Uses a single conservative
             ("non-improving") threshold table rather than the
             improving-flag-differentiated version Stockfish/Ethereal
             use, since Pet Dragon's `alpha_beta` doesn't track an
             "improving" flag at all yet — adding that is a separate,
             untaken change (see D60 for the full rationale). ⚠️ Elo
             impact not yet measured, same as 23.6.

**Phase 23 status as of Session 84: 23.4 in progress.** 23.1–23.3 and
23.6–23.7 are done; 23.5 is HELD (D61, needs NNUE un-shelved first);
23.4 is IN PROGRESS — step 1 of D67's 6-step build order done
(`selfplay.rs`/`selfplay.yml`), steps 2-6 not started. If no other
instruction is given at the start of a session, ask what to work on
rather than assuming 23.4 continues automatically.

---

## Phase 24 — HCE Term-Gap Candidates (documented, unscheduled — D63, Session 82)

Not a committed phase — a ranked candidate list from Session 82's HCE
audit (DECISIONS.md D63), kept here so a future session can pick one
up without re-auditing `eval/` from scratch. Items 2-3 have no
checkbox because they're still unscheduled; pick one explicitly before
starting, don't default into the list top-down without asking.

1. [x] **Passed-pawn king distance** — DONE (Session 83, CI-confirmed
   green, 418 lib tests passing, 0 failed). Added
   `passed_pawn_king_distance_bonus()` to `eval/pawns.rs`: EG-only bonus
   per passed pawn, scaled by Chebyshev distance from each king to the
   pawn's promotion square and by how advanced the passer already is
   (`rank_idx`). Deliberately pure multiply-add (no division) so the
   term stays linear and was wired fully into the Texel tuning chain:
   two new diff features in `texel/features.rs`
   (`passed_king_enemy_dist_diff` / `passed_king_own_dist_diff`), two
   new weights in `texel/weights.rs` / `weights_f64.rs`
   (`enemy_king_dist_eg` / `own_king_dist_eg`, hand-picked default `1`
   each — see note below), forward+gradient terms in
   `texel/predict.rs` / `predict_f64.rs`, and the two `TunableWeights`
   construction sites in `src/bin/texel_diag.rs` /
   `src/bin/texel_tune.rs` (read/write the tuned-weights dump format).
   Not yet Texel-tuned — same "hand-picked, ready for a future tuning
   run" status Phase 8's original constants had before Phase 14.
   ⚠️ Weight started at `2`/`2`, dropped to `1`/`1` mid-session: at
   weight 2 a worst-case combo (enemy king literally on the promotion
   square, own king 7 squares away, pawn also isolated) flipped the
   sign of the pre-existing `test_passed_pawn_bonus` sanity test
   (+51 → −5). Weight 1 keeps that test comfortably positive (+23) — if
   a future Texel tuning run pushes these weights back up, re-check
   that test by hand rather than assuming it still holds.
2. [x] **Pawn storm** — DONE (Session 83, CI-confirmed green first try,
   422 lib tests passing, 0 failed). Added `PAWN_STORM_BONUS[8]` +
   `pawn_storm_danger()` to `eval/king_safety.rs`: mirror image of the
   existing pawn shield — scores ENEMY pawns advanced on files near a
   king as an attacking resource, indexed by rank-distance-to-king
   bucket (0-7), subtracted into `king_safety_score`. MG-scoped like
   every other term in this file (the whole block is scaled by
   `* phase / 24`, not run through the tapered `s()`/`taper()`
   pipeline `eval/pawns.rs` uses). Wired fully into the Texel chain
   from the start this time (learned from item 1's 3 round-trips):
   `king_us_storm_buckets`/`king_them_storm_buckets` in
   `texel/features.rs`, `pawn_storm_bonus: [i32;8]` in
   `texel/weights.rs`/`weights_f64.rs` (default `[40,32,24,16,8,0,0,0]`,
   hand-picked, not yet Texel-tuned), forward+gradient in
   `texel/predict.rs`/`predict_f64.rs`, and both `TunableWeights`
   construction sites in `src/bin/texel_diag.rs`/`texel_tune.rs`.
   Also hand-verified against every pre-existing `king_safety.rs` test
   FEN before submitting: none of them have an actually-advanced enemy
   pawn near the opposing king (all either have no enemy pawns or
   enemy pawns still on their home rank), so the new term contributes
   exactly 0 everywhere those tests check — no repeat of item 1's
   sign-flip surprise.
3. [x] **King-relative PST bucketing** — DONE (Session 83, CI-confirmed
   green first try, 427 lib tests passing, 0 failed). Design discussed
   before implementing (per this item's own "needs more thought"
   flag): the real non-overlapping gap wasn't a general per-square
   king-relative PST, it was that `king_safety.rs`'s `attacker_weight`
   already scores enemy pieces near *our* king (tropism, attacking
   side) but nothing scored our own minor pieces staying close to our
   own king (clustering, defensive side). Three design options were
   weighed (A: flat bonus for knights/bishops in the same king-file-
   third zone as our king; B: full 3-zone table across more piece
   kinds; C: attacking-side mirror) — went with **option A**, since B
   and C both risked double-counting effects `open_lines.rs`/
   `king_safety.rs`'s existing terms already partially capture, and A
   is the smallest, cheapest-to-tune, easiest-to-hand-verify option.
   Implemented as `KNIGHT_NEAR_OWN_KING_BONUS`/
   `BISHOP_NEAR_OWN_KING_BONUS` in `eval/king_safety.rs` (values `8`/
   `6`, hand-picked, not yet Texel-tuned) — 3 file-third zones
   (queenside a-c / center d-e / kingside f-h), flat MG-only bonus per
   own knight/bishop in the same zone as our own king. Wired fully
   into the Texel chain in the same submission as the eval change
   (features/predict/weights/weights_f64/predict_f64/texel_diag/
   texel_tune — full pattern from items 1-2, applied without any
   discovery-via-failed-CI this time), and hand-verified every
   pre-existing `king_safety.rs` test FEN has no knights/bishops at
   all, so the new term is 0 everywhere those tests check.

**Phase 24 / D63 is now fully closed out** — all three candidates
implemented, CI-confirmed green, and documented. Items 2 and 3 each
went green on the first CI submission by applying item 1's lessons
(full Texel-chain wiring in the same submission as the eval change,
full-repo grep for `TunableWeights`/`TexelFeatures` construction sites,
hand-checking every pre-existing test FEN against the new term before
submitting).

Also on record for context, not actionable: D64 surveyed evaluation
paradigms beyond HCE/NNUE (MCTS+policy/value net, searchless
transformer, GPU-sized NNUE, policy-guided move ordering) and ruled
all of them out given Pet Dragon's CPU/WASM deployment target and
current training-data constraints — don't re-investigate this without
new information changing one of those constraints.

**Phase 24 addendum — 4th candidate identified (D68) and implemented
(D73), Session 84.** A separate question (whether the engine scores
pieces defending each other) surfaced a gap D63's original audit
didn't check for: Stockfish's **Threats** term (`threats.cpp`, GPL v3,
same credited family as material/PST). D68 documented the full
4-part concept; D73 implemented a scoped-down 2-term version
(`UNDEFENDED_PENALTY`, `THREAT_BY_MINOR_BONUS` — "restricted piece"
deliberately dropped as the most direct overlap risk with
`mobility.rs`, not merely deferred). New file `eval/threats.rs`, full
Texel-chain wiring across 9 files including two
(`texel_tune.rs`/`texel_diag.rs`) not touched by any earlier Phase 24
item. Hand-picked starting weights, not yet Texel-tuned — picked up
automatically whenever the next natural re-tune happens. Not yet
confirmed committed. Phase 24 items 1-3 remain independently closed;
this was a new candidate (D68), not a reopened item — now also closed.

---

## Phase 25 — Full Texel Re-Tuning Pass (Session 83)

**Goal, in Gokul's words:** make Pet Dragon "lethal, precisive, scary,
brutal" irrespective of opening/middlegame/endgame. Scoped down from
that broad ambition to the single lever most aligned with it: a
mistuned evaluation constant is wrong in *every* phase of the game
equally, so re-fitting the whole weight set is the most "irrespective
of phase" improvement available, and unlike NNUE (shelved, D61) or
further search tuning (diminishing returns per D40), the hard
infrastructure for this already exists and just needs re-running.

**Why this, not #2/#3/#4** (the other candidates discussed): endgame-
specific work and search tuning are narrower, harder-to-measure levers
without first knowing where the current eval is actually leaving Elo
on the table; NNUE is a real structural investment, not a quick lever.
A full re-tune is the most leveraged next step precisely because nobody
has re-run it since Phase 14 (Session 56) — meaning it's stale in two
concrete ways (see below), not because it's guaranteed to be the
single biggest win.

**Why this hasn't effectively been done yet, despite Phase 14 having
already run a real Texel tuning pass once:**
1. **Stale weight set.** Session 56 tuned and applied weights for the
   term set that existed *then*. Every eval term added since — Phase
   24 / D63's 3 new terms this session, and any mobility/pawn/king-
   safety work landed between Phase 14 and now — ships with hand-
   picked defaults that have never been through Texel fitting, sitting
   alongside otherwise-tuned constants.
2. **Stale data.** The 147,867-sample dataset from Phase 14.2 was
   generated by a much older, weaker version of Pet Dragon's search+
   eval, and — separately — `texel_gen.yml`'s artifacts only retain
   for 30 days, so that dataset is long expired regardless. A fresh
   dataset generated by the *current* engine is both required (old one
   is gone) and higher-quality (reflects current play, not Session-52
   -era play).

**Plan (2 manual GitHub Actions runs, both mobile-friendly, no
terminal — same `workflow_dispatch` pattern as every other training
workflow in this repo):**

1. **Generate fresh Texel data — `texel_gen.yml`.** Phase 14.2 used
   4 batches (smoke test + 3 production, ~37K samples/batch on
   average) to reach 147,867 samples; start with one batch at similar
   scale (`num_games` in the few-hundred range — the original ratio
   isn't recorded precisely enough to give an exact number, so treat
   the first run as a scale-finding run: check `wc -l`/runtime in the
   job log against the 6-hour Actions ceiling per D19/D20, then decide
   whether to run more batches with different `seed_start` values
   before moving to step 2). Bigger dataset than Phase 14.2's if the
   time budget allows — the more samples, the better the fit, and
   Phase 23.3 already proved this repo can generate self-play data at
   real scale (2.48M rows) when needed.
2. **Run the tuner — `texel_tune.yml`.** Feed it the Run ID(s) from
   step 1 via `data_run_id` (comma-separated if multiple batches).
   Start with a short run (~10-20 epochs) to sanity-check the loss
   trend before committing to a long run, exactly as Phase 14's own
   plan did. Once a longer run's `texel_weights_tuned.txt` output looks
   plausible (no wild outliers — this is what `texel_diag.rs` exists to
   check, per Phase 14's Session 55 addition), that's the point to hand
   the output back for the `eval/*.rs` + `texel/weights.rs::default()`
   application step (mandatory dual-sync, per Phase 14.3-14.5's note —
   skipping the `weights.rs` half silently breaks the self-consistency
   test).

**Status: Step 1 COMPLETE, step 2 not yet triggered.** Gokul ran it
with different parameters than originally planned —
`seed_start=15000, num_games=3500` (not `10000`/`4000` as noted at the
end of Session 83; recording actual values here since that's what the
data reflects). Confirmed via the Actions run page: "Texel Tuning Data
Generation #7", commit `afb51f0`, on `main`, **Status: Success**,
duration 5h19m49s. Artifact `texel-data-seed15000-n3500` (1.24 MB,
run ID `29891640736`) — also published as a permanent GitHub Release
asset (won't hit the 90-day artifact retention window):
`https://github.com/g-c-3/pet-dragon/releases/download/texel-data-seed15000-n3500/texel_data.txt`.
62,125 samples (spot-checked head/tail — well-formed, spans opening
through king-and-minor endgames per the tail rows).

**Sequencing note before triggering step 2 — do not skip.** This data
was generated at commit `afb51f0`, which predates the Session 84
rank-battery fix (`open_lines.rs` + `texel/features.rs`, queued for
commit — see Session 84 notes). The dataset itself is fine to reuse
regardless (`texel_gen.rs` writes raw FEN + result, not pre-extracted
features — feature extraction happens at `texel_tune.yml` time against
whatever code is on `main` then). But **`texel_tune.yml` must run
against a `main` that already has the rank-battery fix committed**,
or the new rank-battery term won't be in this tuning pass at all and
will ship untuned again — exactly the staleness problem Phase 25
exists to fix. Confirm the fix is pushed before triggering step 2.

**Step 2 plan, ready to trigger once the above is confirmed:** run
`texel_tune.yml` with `data_urls` set to the Release asset URL above
(simpler than `data_run_id` here since it's already a permanent link,
not subject to artifact expiry). Per the original plan, start with a
short sanity run — override `epochs` to `15` (default is `50`) — and
leave `learning_rate=1.0, k=auto, seed=42, batch_size=16384,
weight_decay=0.0` at their defaults for this sanity pass. Check
`texel_diag.rs`-style output for wild outliers before committing to a
longer run.

**Status: COMPLETE and CI-CONFIRMED GREEN (Session 84, D69, corrected
by D70).** Sanity run (15ep/decay=0) → full run (75ep/decay=0.03) →
`texel_diag.yml` (10/10 pass) → applied to all 8 files
(`eval/material.rs`, `tables.rs`, `mobility.rs`, `pawns.rs`,
`king_safety.rs`, `open_lines.rs`, `mod.rs`, `texel/weights.rs`),
cross-verified field-by-field for dual-sync. Two tuned results
rejected: `knight`/`bishop_near_own_king` (D69) and
`enemy`/`own_king_dist_eg` (D70, CI-caught after D69 shipped). Both
held at Phase 24 hand-picked defaults. Full reasoning and a 4-item
watch list (rook_on_seventh, pawn_storm_bonus, bishop_pair/
battery_bishop_queen large swings) in DECISIONS.md D69/D70. Gokul
confirmed a fresh CI run is fully green with the D70-corrected files
committed — Phase 25 is genuinely done, no open threads.

---

## Phase 26 — Candidates Surfaced from External-Advice Review (Session 83)

**Not scheduled, not started.** Gokul asked two outside AIs for variant-
adaptation advice and shared the responses, and separately asked for a
direct comparison against current Stockfish (verified via live web
search — Stockfish 18, Jan 2026 stable, current dev builds — not
memory, since Stockfish moves fast and training-data recall would be
stale). Most of the external advice turned out to already be covered
(verified against the real repo, not assumed —
`PawnStartMap`, `pawn_start_key` zobrist hashing, Texel-tuned
setup-agnostic PST/eval, `texel_gen.rs`'s randomized-start sampling)
or based on a wrong premise about Pet Dragon (one response assumed
Chess960-style flexible castling with rooks on any file — Pet Dragon's
king is hardcoded to e1/e8, confirmed directly in `movegen/castling.rs`;
Gokul confirmed he doesn't want that built). Two items were genuinely
open and worth keeping on record:

1. ✅ **Null-move king-exposure guard — CLOSED (Session 85, D75/D76/D77):
   implemented, tested, no measurable effect at current thresholds,
   parked off by default.** `NullMoveKingGuard` UCI option exists
   (default `false`, zero cost when off), mechanism stays in the code.
   20-game first look: +127 Elo (D76 — flagged as noise, n too small
   to trust). 200-game confirmation: +3.5 Elo, 50.5% (D77 — flat,
   confirms the first result was noise). Not worth further tuning
   budget right now — Pet Dragon's existing zugzwang guard
   (`has_non_pawn_material`) already covers the cases that matter most.
   Checked
   `search/alpha_beta.rs`'s
   `can_null_move` condition directly — current guards are the
   standard set (not PV, not in check, sufficient depth, `static_eval
   >= beta`, has-non-pawn-material zugzwang guard, no consecutive
   nulls). No king-safety/exposure-specific guard exists. The
   suggestion: be more conservative with the reduction (or disable
   null-move entirely) when the side-to-move's king has few safe
   squares, since a random Pet Dragon start can leave a king with no
   natural pawn shield immediately, unlike a standard opening where
   the king is reliably still safe this early. Real and specific, but
   **unproven** — would need its own isolated SPRT-style test (fixed
   games, same time control, with/without the guard) before trusting
   it, not a blind add.
2. ✅ **Deeper/more adversarial perft coverage — CLOSED (Session 85,
   D78).** `tests/perft.rs` already
   has Pet-Dragon-specific tests (`generate_with_seed` across 20 seeds)
   but only to depth 1-2 with loose "reasonable range" assertions, not
   deep exact-value verification or deliberately constructed
   adversarial setups (rank-1 double-step immediately followed by en
   passant, castling blocked by an intervening piece ~~on an unusual
   rook file~~ — **correction, D78: castling requires the rook on its
   standard a1/h1/a8/h8 square, king is hardcoded e1/e8; there is no
   "unusual rook file" case. The real case is a standard-square rook
   with some other random piece blocking the path, which is what
   D78's tests cover.**). Worth strengthening given the position space size, but
   this is test-suite hardening, not a strength lever — low urgency
   unless a real bug surfaces. **4 new hand-verified perft tests added
   (D78) covering both flagged mechanics through multiple plies of
   real make/unmake + zobrist + legality. NOT closed: converting the
   20-seed random-position range checks to exact values — no
   independent perft oracle exists for arbitrary Pet Dragon
   arrangements, and hand-enumerating full-board random positions
   isn't tractable; this is an inherent limitation of testing a custom
   variant, not a follow-up task.**
3. **Expand correction history beyond a single pawn-hash table.**
   Surfaced by comparing against Stockfish 18 (current stable,
   confirmed via its own release notes and source) — Pet Dragon already
   has `search/pruning.rs::CorrectionHistory`, matching Stockfish's
   original 2023 pawn-structure-indexed design (tested, applied to
   static eval only). Current Stockfish has evolved this further:
   evidence of additional correction signal sources beyond pawn
   structure (continuation-based, non-pawn-material), and — per a
   recent Stockfish PR — using the correction signal to also adjust
   singular/double/triple extension *margins*, not just static eval.
   Real, concrete, and well-scoped (extend an existing, already-tested
   mechanism rather than build something new), but the specific
   additional signal sources worth adding need their own scoping pass
   before implementation, and each addition should get its own
   isolated CI-verified diff + eventual SPRT-style validation, not be
   bundled into one large change. **Gokul opted to do all three
   sub-items, as sequential isolated diffs rather than one bundle:**
   - 3a. ✅ **CLOSED (Session 85, D80/D82/D85) — non-pawn-material
     correction table implemented, tested, gated behind
     `NonPawnCorrectionHistory` (default false). SPRT result: 20-game
     first look +70 Elo (D84, flagged as likely noise); 200-game
     confirmation -19.1 Elo (D85, flat-to-negative, not statistically
     distinct from zero). No measurable benefit — parked off, same
     outcome and treatment as item 1.**
   - 3b. ✅ **CLOSED (Session 85, D86/D87/D88) — continuation-based
     correction implemented, tested, gated behind
     `ContinuationCorrectionHistory` (default false). SPRT result:
     -34.9 Elo at n=20, -22.6 Elo at n=200 (D88) — consistent negative
     direction across both samples, unlike items 1/3a's flip-flopping
     20-game results. Parked off.**
   - 3c. ✅ **CLOSED (Session 85, D89/D90) — correction-scaled
     singular extension margin implemented, tested (633 real tests
     green, D87 method), gated behind `CorrectionExtension` (default
     false). SPRT result: -34.9 Elo at n=20, -40.1 Elo at n=200 (D90)
     — the clearest negative result of any Phase 26 item this
     session, magnitude grew rather than shrank at the larger sample.
     Parked off.**

**Phase 26: CLOSED (Session 85).** All 6 items resolved — 1 (parked
off), 2 (test-hardening, closed), 3a/3b/3c (all parked off — four
independent strength ideas, four flat-or-negative SPRT results), 4
(fixed). See D90 for the closing summary and next-direction
recommendation (Phase 16 NNUE scoping, or a fresh look at other
signal sources before committing to NNUE's scale).
   - 3c. Not started — use the correction signal to adjust
     singular/double/triple extension margins, not just static eval.
4. ✅ **CLOSED (Session 85, D83) — engine crashed on an illegal input
   FEN where the side not to move is already in check.** Found
   (Session 85, D81) while probe-verifying an item 3a test position;
   fixed same session (D83). `Position::from_fen` now rejects (a) any
   FEN missing exactly one king per color (`FenError::KingNotFound` —
   existed as a declared variant since before this session but was
   never actually wired up) and (b) any FEN where the side not to move
   is in check (new `FenError::OpponentInCheck`), instead of building
   a `Position` that would later panic. Verified via a full sweep of
   every FEN literal in the existing codebase (98 found via grep) —
   caught and fixed 5 pre-existing test FENs that were themselves
   illegal positions of the same class, previously accepted only
   because nothing validated it. **Cannot occur via legal search from
   `start_pos()`/`generate_with_seed()`** — never affected self-play or
   tournament games from a legal start — but a malformed external UCI
   `position fen` command now gets a graceful UCI-level error instead
   of crashing the process.

**Explicitly NOT adopted, with reasoning on record so this doesn't get
re-litigated without new information:**
- Chess960-style flexible castling (X-FEN-style per-file rights) —
  wrong premise about Pet Dragon, Gokul confirmed out of scope.
- "DragonView" (per-piece View/Sovereign-View system with
  extend-on-danger-jump / prune-on-zero-view-change / view-delta move
  ordering) — most of the underlying primitives (attack tables, SEE,
  king-ring danger) already exist in some form; the genuinely novel
  search-integration ideas are unproven and the proposal itself flags
  a real NPS-regression risk (30-50%) from over-triggering king
  recompute. A large, invasive architecture change for an unproven
  payoff, on a codebase where the working pattern this session
  (small, individually-verified, CI-confirmed diffs) has been
  succeeding. If any slice of this is ever worth revisiting, it's the
  narrowest one — richer king-safety signal computed relative to
  current king square — through the same discuss-design-first → small
  diff → CI-verify discipline already established, not adopted as a
  named system.

---

**Confirmed via Session 83's real CI `cargo test` log (Actions run,
`logs_80664145490.zip`) — lib test count updated to 427 (was 396 at
Session 71's count; +22 from Session 82's D59/D60 work landing, +4
from Phase 24 item 1's tests, +4 from Phase 24 item 2's tests, +5 from
Phase 24 item 3's tests).** Per-module breakdown below is still not
recomputed — low priority, not blocking anything.
| Test Crate/Binary        | Count | Status |
|---------------------------|-------|--------|
| lib (all of src/, unit tests) | 427 | ✅ |
| tests/make_unmake.rs      | 19    | ✅     |
| tests/perft.rs            | 18    | ✅     |
| tests/setup.rs            | 21    | ✅     |
| tests/node_count.rs       | 5 (all ignored — perft depth ≥8, run manually) | — |
| src/main.rs (pet_dragon bin) | 49 | ✅     |
| src/bin/uci_match_runner.rs | 12  | ✅     |
| src/bin/match_runner.rs   | 6     | ✅     |
| src/bin/eval_diag.rs, texel_diag.rs, texel_gen.rs, texel_tune.rs, train_nnue.rs, selfplay.rs | 0 each (no #[test] fns yet) | — |
| **TOTAL** | **576 run, 571 passed, 0 failed, 5 ignored** (Session 83, CI-confirmed) | ✅ |

✅ **All three Phase 24 / D63 items are now fully CI-confirmed.** Item
1 took 3 CI round-trips (see history below); items 2 and 3 both went
green on the first CI submission — Gokul supplied the `Test` job's
logs (`logs_80500379634.zip` for item 2, `logs_80664145490.zip` for
item 3). Applying item 1's lessons up front (full Texel-chain wiring
done in the same submission as the eval change, and every pre-existing
test FEN hand-checked against the new term before
submitting) avoided repeating any of item 1's 3 round-trips.

**Item 1's CI history, for reference** — Gokul supplied the `Test`
job's logs for the final green run (`logs_80464922497.zip`). Getting
there took 3 CI round-trips, all fixed same-session:
1. First submission compiled but broke `texel::predict()`'s
   self-consistency test (`test_predict_matches_evaluate_default_weights`)
   — the new eval term wasn't mirrored in the Texel predictor. Fixed by
   dropping a truncating `/7` from the formula (pure multiply-add,
   fully linear) and wiring it through `features.rs`/`predict.rs`/
   `weights.rs`/`weights_f64.rs`/`predict_f64.rs`.
2. Second submission failed to compile (`E0063`, missing fields) —
   `src/bin/texel_diag.rs` and `src/bin/texel_tune.rs` construct/parse
   `TunableWeights` outside the `texel/` module and were missed by the
   first pass. Fixed by grepping the full repo for `TunableWeights`
   construction sites rather than trusting the initial file list.
3. Third submission compiled and ran but broke a pre-existing sanity
   test (`test_passed_pawn_bonus`, `score > 0`) — the new term's
   weight (2) was strong enough to flip sign in a worst-case king
   placement. Fixed by halving the weight to 1 (see item 1's note
   above for the exact numbers).

This supersedes Session 82's "540 passed, 0 failed" total.

---

## Phase 27 — External Stockfish Benchmark Regression Investigation ✅ RESOLVED (Session 86 → 92, root cause D95, fix confirmed D97)

**Trigger:** Gokul supplied `engine-bench-log-2026-07-26-13-10-23.txt` —
3 games, Pet Dragon (White, Skill 20 = uncapped) vs. Stockfish (Black,
Skill 10), 100ms/move, both from the standard classical start FEN.
**Result: 0-3, all checkmates against Pet Dragon.** This is the first
recorded data point of Pet Dragon vs. real external Stockfish at any
skill level — every prior strength claim in this repo (Phase 14/17/20/25
Elo numbers) is Pet-Dragon-vs-itself or Pet-Dragon-vs-pinned-ref, never
vs. a real independent engine. Treat this as new, real signal, not
noise to explain away.

**Analysis done this session (Claude, via local `python-chess` replay
of the raw move lists — read-only, no repo changes):**
- All 3 games are legal, end in genuine checkmates (not parser/UCI bugs).
- Material tracked ply-by-ply in all 3 games shows the **same shape**:
  roughly level material through the opening/early-middlegame, then a
  **sudden, large, one-sided material collapse late in the game**
  (Match 1: even until ~ply130, -9 by ply150; Match 2: even until
  ~ply60, -15 by ply90; Match 3: collapses earlier and more repeatedly).
  Three-for-three same shape is a real pattern, not sampling noise.
- This looks like a **tactical blindness / hanging-piece problem that
  gets worse as games go long / material thins**, not a slow eval
  miscalibration (a miscalibrated eval would drift, not suddenly drop a
  rook+ worth of material in ~10-20 plies).
- All 3 games start from the **standard classical FEN**, so whatever's
  wrong is in generic search/eval code, not Pet Dragon variant-specific
  logic (`pawn_starts`, dynamic castling, etc.).

**Leading hypothesis (unconfirmed — needs bisection, not a guess to
ship a fix against):** three search techniques landed in Phase 23 with
their Elo impact explicitly flagged `⚠️ not yet measured against a real
match` — 23.2 (thread-differentiated Lazy SMP, D49), 23.6 (singular
extension multi-cut + negative extension, D59), 23.7 (Late Move Pruning,
D60). All three are **on by default**, unlike the Phase 26 experimental
options (NullMoveKingGuard etc., which are off by default and already
ruled out by real SPRT data). Any of the three could plausibly cause
exactly this failure mode: LMP/multi-cut/negative-extension all prune
or shortcut real lines outright rather than just reducing them, and if
any threshold is miscalibrated it would look like "plays fine until a
long, thinning-material position exposes the over-pruning," not a
crash or an obviously-wrong eval sign. **This has not been verified by
reading source for a concrete bug — it's the most likely suspect class
based on what's documented as unmeasured, and needs to be confirmed or
ruled out empirically, the same way every other strength question in
this repo has been (SPRT-style A/B via `uci_match_runner.rs`), not
patched on suspicion.**

**Investigation plan, staged (mobile/Actions-only, no live debugging):**
1. **Isolate time pressure as a variable.** Re-run the same Pet-Dragon-
   vs-Stockfish bench (or an equivalent) at a slower time control
   (e.g. 1000ms/move) to see if the same late-game collapse shape
   still appears. If it disappears at slower time controls, this is a
   time-management/depth-starvation issue, not the Phase 23 pruning
   hypothesis above — different fix path entirely.
2. **Isolate Lazy SMP.** Re-run with `Threads=1` (no helper threads) to
   rule out/in 23.2's thread-differentiated parameters and any TT
   pollution from helper threads bleeding into the main search under a
   very short time budget.
3. **Bisect 23.6/23.7 via pinned-ref `uci_match_runner.rs`** (same
   tool/methodology as Phase 17.8's D36 pre/post-tuning match and
   Phase 26's SPRT-style validations) — one binary at the commit
   immediately before D59/D60 landed vs. current `main`, same time
   control as whichever step above best reproduces the collapse. This
   directly measures whether 23.6/23.7 net-helped or net-hurt in a way
   the internal-only Phase 23 testing never checked.
4. **Once isolated to a specific technique/commit range**, read that
   specific code path in full against the actual collapse positions
   (extract the FEN a few plies before each game's material collapse
   point from this session's replay) before writing any fix — same
   read-before-write discipline as every other session in this repo.

**Update (Session 86):** Gokul supplied a second bench log — same
matchup, **1000ms/move instead of 100ms**, Stockfish Skill 9. Same
result: 0-3, all checkmates against Pet Dragon, and the exact same
material-collapse shape (level early, then a sudden late-game
one-sided collapse) in all 3 games. **This rules out plain time
pressure as the primary cause of step 1** — the pattern persists at
10x the time budget, so it's a genuine search/eval behavior, not just
depth starvation. Also: `EngineState::new()` defaults `threads: 1`, so
unless Gokul's bench tool explicitly raises `Threads`, 23.2's
thread-differentiated Lazy SMP code never executes at all in these
benches — **23.2 is provisionally ruled out**, narrowing the live
suspects to 23.6 (D59, singular multi-cut/negative extension) and 23.7
(D60, Late Move Pruning).

**D91 — shipped this session:** two runtime UCI diagnostic toggles,
`LMPEnabled` and `SingularMultiCutEnabled`, both default `true`
(byte-identical to current production behavior). Setting either
`false` via `setoption` reverts *only* that one D59/D60 addition,
letting the same already-deployed binary be A/B'd against Stockfish
via the existing Engine Bench tool without a rebuild. Full detail in
DECISIONS.md D91. Six new tests added; not yet CI-confirmed (no local
`cargo` available this session — reviewed by hand, brace/paren-balance
checked programmatically).

**Not yet done — next session should start here:**
1. Gokul commits the 3 changed files (`src/search/mod.rs`,
   `src/search/alpha_beta.rs`, `src/main.rs`) and confirms CI (cargo
   test + build) passes.
2. Once deployed, run the same Engine Bench matchup (Skill 20 vs.
   Stockfish, whatever movetime reproduces the collapse most reliably —
   1000ms/move worked in Session 86) in **three configs**: (a) both
   toggles left at default `true` (control — confirm the collapse still
   reproduces on this exact build), (b) `LMPEnabled=false`, (c)
   `SingularMultiCutEnabled=false`. 2-3 games per config is enough to
   see if the shape changes.
3. Whichever config (if either) removes the late-game collapse
   identifies the responsible technique. Only then read that specific
   code path in full against the actual collapse positions (extract the
   FEN a few plies before the collapse point in one of these games)
   before writing an actual fix — same read-before-write discipline as
   always. If *neither* toggle changes the pattern, the bug is
   elsewhere (generic eval, TT, or something not yet suspected) and
   this session's hypothesis was wrong — don't force-fit a fix onto D59/
   D60 in that case.

---

**Update (Session 87):** Gokul ran config (a) (both toggles default)
via `web/pit/vs.html` — the browser pit tool, not raw UCI — 1000ms/move,
Skill 20 vs. Stockfish Skill 10: **2 draws (repetition) + 1 loss**, a
real improvement over the previous 6-for-6, though the one loss still
shows the same late-game material-collapse shape on replay. Gokul then
reported no way to actually run configs (b)/(c): `vs.html` calls Pet
Dragon via a direct one-shot WASM function
(`search_from_fen_with_eval`), not the native UCI `setoption` path
`main.rs` (D91, see below) was wired through — **D91's toggles were
unreachable from the only tool Gokul actually uses for this
investigation**, a real gap, not a usability question. Fixed via
**D92**: two WASM-side `AtomicBool` toggles + `set_lmp_enabled()`/
`set_singular_multicut_enabled()` exports in `lib.rs`, plus a new "Pet
Dragon diagnostics" checkbox card in `vs.html` that calls them and
records the config into every exported bench log going forward. Full
detail in DECISIONS.md D92.

**Numbering correction:** the diagnostic-toggle mechanism from Session
86 was originally mislabeled `D90` in this file and in `DECISIONS.md`,
colliding with Session 85's existing `D90 — CorrectionExtension` entry.
Corrected throughout to **D91** (the `main.rs`/`SearchInfo` mechanism)
and **D92** (this session's WASM/`vs.html` half) before any of it was
committed.

**Not yet done — next session should start here:**
1. Gokul commits all 5 changed files this time: `src/search/mod.rs`,
   `src/search/alpha_beta.rs`, `src/main.rs` (Session 86/D91),
   `src/lib.rs`, `web/pit/vs.html` (Session 87/D92) — and confirms CI
   passes, including the WASM/Pages deploy workflow (`vs.html` imports
   live from `g-c-3.github.io/pet-dragon/pkg/pet_dragon.js`, so the new
   `set_lmp_enabled`/`set_singular_multicut_enabled` exports only exist
   in the browser once that redeploys).
2. Load `web/pit/vs.html` once manually and confirm the new "Pet Dragon
   diagnostics" card actually renders and the checkboxes are wired
   (this session couldn't compile or load the page — hand-reviewed
   only, see D92's caveat) before trusting any A/B result from it.
3. Run the same three configs from step 2 above, now actually possible
   via the checkboxes: (a) both on/default — control, (b) LMP
   unchecked, (c) SingularMultiCut unchecked. 2-3 games per config.
4. Whichever config removes the late-game collapse identifies the
   responsible technique — then read that code path in full against
   the actual collapse positions before writing a fix, same discipline
   as always. If neither changes the pattern, the bug is elsewhere and
   this hypothesis was wrong.

---

**Update (Session 88):** Gokul ran configs (b) and (c). **Neither
ablation supports the D59/D60 hypothesis** — (b) `LMPEnabled=false`
scored 1 draw + 2 losses, (c) `SingularMultiCutEnabled=false` scored 2
losses + 1 draw, both numerically worse than Session 87's control (2
draws + 1 loss), and the same late-game collapse shape appeared in
losses under both ablations. One loss under config (c) is a striking
data point: Pet Dragon was **+1 material** and still got checkmated 8
plies later — looks like king-safety/mate-blindness, not a
material-losing blunder. n=3-per-config is too small to be conclusive
either way, but there's no positive signal for D59/D60 after two
independent ablations, and burning more of Gokul's time on further
single-technique guesses isn't the efficient next move. Full detail in
DECISIONS.md D93 (this session's entry is D93, not D90 — see the
numbering-correction note below).

**Also shipped (Session 88, no hypothesis attached):** the exported
bench log now includes each move's own White-relative eval —
`e2e4(+35)`, `f2f4(mate-3)`, etc. — pulled from data `vs.html` already
computed for its eval bar but previously discarded before export. Next
bench run will show whether Pet Dragon's own eval already predicted the
losing outcome well before the material collapse (an `eval/` problem)
or stayed confidently positive right up to the blunder (a `search/`
horizon-effect problem) — a much stronger signal than another ablation
round.

**Not yet done — next session should start here:**
1. Gokul commits `web/pit/vs.html` (Session 88's eval-logging addition,
   on top of Session 87's diagnostics card — same file, no conflict) and
   confirms the Pages redeploy.
2. Run one more bench (any config, control is fine) with the new
   eval-logging build.
3. Read the per-move eval trajectory directly against the material
   trajectory already established for these positions. If eval predicts
   the collapse in advance, look at `eval/` (likely `king_safety.rs`
   given the +1-material-but-still-mated data point above); if eval
   stays confidently positive right up to the blunder, look at
   `search/` (quiescence search depth, or a horizon effect not caught by
   the current extensions). Only then read the relevant code in full
   against the actual positions before writing a fix.

---

**Update (Session 89):** Gokul ran 9 games (3 per config, control/LMP
off/SingularMultiCut off) with the eval-logging build. **Sharp,
quantitative finding:** Pet Dragon's own reported eval is *exactly* `0`
on 16-49% of its own moves across every game sampled, while Stockfish
(same games, same positions) reports exact `0` on 0-1% of its moves —
this is a Pet Dragon-specific signature, not a property of these
positions, present in every config tested across Sessions 87-89 (further
evidence against D59/D60). One especially sharp example: Pet Dragon's
last non-mate eval before getting checkmated in Match 1 was `0`, while
the true position (per Black's eval one ply later) was ~-9 pawns. A
separate, differently-shaped anomaly also appears (Match 2: Pet Dragon
self-reports `+423` two of its own moves before getting mated) —
possibly a second symptom of the same root cause, possibly unrelated.

Read `search/iterative.rs` in full looking for the mechanism. Found a
real sentinel-confusion bug in `score_for_result`'s selection logic
(treats a genuinely-computed `0` the same as "not yet set") but could
**not** confirm from static reading alone that it's what's actually
producing the pattern in the ordinary synchronous case — flagged as a
lead, not a confirmed cause. Instead of guessing further, shipped a
data-gathering change: `search_from_fen_with_eval` now returns a third
token, the deepest completed search depth for that move, additive to
both existing callers (checked, not assumed). `vs.html` now logs it as
`/dN` per move. Full detail in DECISIONS.md D94.

**Not yet done — next session should start here:**
1. Gokul commits `src/lib.rs` and `web/pit/vs.html` (Session 89's depth-
   logging addition, layered on Sessions 87-88's changes to the same two
   files) and confirms CI + Pages redeploy.
2. Run one more bench (any config — no need to keep varying D91's
   toggles, that line of investigation is exhausted for now).
3. Check where the `0`-eval moves' `/dN` values cluster: **low
   (`/d1`-`/d2`)** → time-management/search-abort issue, read
   `search/time.rs`'s `TimeManager`/`allocate_time` next; **normal for
   1000ms (`/d6`-`/d8`+)** → the search is doing real work but the
   *reported* score is wrong, read `iterative.rs`'s `score_for_result`
   expression (flagged in D94) and the aspiration-window fail-high/
   fail-low paths in `search_with_aspiration` next. Either result
   replaces guessing with a specific file to actually fix.

---

**Update (Session 90) — ROOT CAUSE FOUND AND FIXED, pending real-bench
validation:** Gokul's next bench confirmed the `0`-eval moves cluster at
completely normal depths (d10-d14), ruling out time-starvation and
pointing squarely at the score-reporting lead from D94. Reading
`search/alpha_beta.rs` and `SearchInfo::is_time_up()` together found the
actual mechanism: `is_time_up()` only ever *read* `self.stop`, and
neither of its two real call sites (`alpha_beta()`'s own time check,
`quiescence()`'s) ever *wrote* `info.stop = true` on a genuine
elapsed-time timeout — they just silently returned a hardcoded `0`
sentinel with no abort signal, so every `if info.stop {...}` discard/
skip check already written elsewhere in the codebase (TT-store guard,
correction-history guard, `iterative_deepening()`'s own
discard-this-depth logic) was dead for the single most common abort
reason there is. **Fixed**: `info.stop = true;` added alongside both
`return 0;` sites — a two-line change per site, activating logic that
was already correctly written and just never triggered. Full mechanism,
data trail, and why this explains every symptom from Sessions 85-90 (Pet
Dragon-specific, config-independent across D91's toggles, worse in
longer games, manifests as sudden material collapse) in DECISIONS.md
D95. Two new deterministic tests added (0ms time budget forces
`is_time_up()`'s elapsed-time branch on the very first check).

⚠️ **Phase 27 stays open — this is not confirmed fixed, only
high-confidence.** No local `cargo` to compile-check this session (same
caveat as every session since Phase 27 began). Next session must get a
real post-fix bench run before this phase can close.

**Not yet done — next session should start here:**
1. Gokul commits `src/search/alpha_beta.rs` (Session 90's D95 fix) and
   confirms CI passes, including the two new tests.
2. Run a bench (any config, control is simplest — no more toggle
   ablation needed) with the fixed build.
3. Check the exported log for two things: (a) has the exact-zero eval
   rate dropped sharply from the 16-49%-of-White's-moves baseline
   established in D94, and (b) has the overall win/loss/draw record
   improved from the ~0-6-3-ish record across Sessions 85-89. If both
   are true, Phase 27 can close as resolved. If the zero-eval rate drops
   but losses continue, or if Match 2's separate `+423`-then-mated
   anomaly (D94) still appears, that's a distinct, second issue worth
   its own investigation — don't assume one fix closes everything found
   in six sessions of data.

---

**Update (Session 91):** Gokul ran CI on the D95 commit and it failed —
one test, `test_qsearch_in_check_generates_evasions`. Cause:
D95's `str_replace` on `quiescence()`'s time check accidentally dropped
the pre-existing `info.nodes += 1;` line (it was only in `old_str` to
disambiguate the match, not meant to be touched, and got silently
swallowed since `new_str` didn't include it back). Beyond the one
failing test, this would also have undercounted total search nodes in
NPS/`seldepth` stats and made `is_time_up()`'s 256-node sampling gate
fire on every quiescence node instead of every 256th — a real perf
regression that CI's single test failure didn't directly cover but
would have shipped alongside D95 if merged as-is. **Fixed**: restored
the one line. `alpha_beta_with_excluded()`'s own D95 edit (the other
call site) wasn't affected — different code shape, that line was never
in its `old_str`. Full writeup, including the process point about
re-reading diffs rather than just checking brace balance, in
DECISIONS.md D96.

**Not yet done — next session should start here:**
1. Gokul commits the corrected `src/search/alpha_beta.rs` and confirms
   CI is actually green this time — all tests, not just the one that
   failed last round.
2. Same as before: run a bench (any config), check whether the
   exact-zero eval rate dropped from D94's 16-49% baseline and whether
   the win/loss/draw record improved from Sessions 85-89. Only close
   Phase 27 if both hold.

---

**Update (Session 92) — PHASE 27 RESOLVED.** Gokul confirmed CI fully
green (476 tests, including D95's two new tests and the D96 fix), then
ran a 3-game bench: **1 draw, 1 loss, 1 win — Pet Dragon's first
recorded win against real Stockfish in this entire investigation.**
Exact-zero eval rate dropped from D94's 16-49% baseline to 0-8%, and the
handful of remaining zeros are a genuine repetition-draw at depth 19-20
(correct behavior), not the bug. The separate `+423`-then-mated anomaly
from D94 also didn't reappear in this sample's loss — that game's eval
trends smoothly to `mate-2`, fully coherent. Full data and reasoning in
DECISIONS.md D97.

**Phase 27 closes here.** The confirmed root cause (D95:
`is_time_up()` never set `info.stop`, correctly landed after D96 fixed
a self-inflicted regression in D95's own edit) is what this
investigation existed to find, and this bench is a clean before/after
confirmation of it. Losing one of three games to Stockfish Skill 10 is
normal expected variance, not a remaining-bug signal — Phase 27's goal
was never "Pet Dragon never loses," it was "Pet Dragon's own search
shouldn't be corrupting its scores," and that specific, confirmed
problem is gone from this sample. n=3 is not a rigorous Elo measurement
— if Gokul wants a real Elo-impact number for the record, that's a
future SPRT-style A/B via `uci_match_runner.rs` (same tooling used for
every internal Elo question in this repo), but that's normal ongoing
strength work, not an open regression to keep chasing.

**Retrospective, six sessions (85-92), for the record:** started from a
single uploaded bench log showing 3-for-3 losses; ruled out time
pressure (100ms vs. 1000ms, same pattern) and Lazy SMP (default
Threads=1); built diagnostic toggles for the two techniques flagged
"Elo impact not yet measured" at ship time (D91/D92) and ran three
separate ablations, all negative — the real bug was never in either of
them; added per-move eval logging (D93) and found the actual empirical
signature (Pet Dragon-specific exact-zero eval, D94); added depth
logging to distinguish a time-starved search from one reporting a wrong
score, which pointed straight at the real mechanism (D94→D95); found
and fixed the root cause by reading code, not guessing (D95); caught
and fixed a self-inflicted regression in that same fix via CI, exactly
as CI is supposed to work (D96); confirmed the fix with real bench data
(D97). Every negative ablation and every diagnostic addition earned its
place in this chain — none of it was wasted, even the results that
didn't pan out narrowed the search.

---

## Phase 28 — Threat-Defusal Search Extension (TDSE) ⏳ IN PROGRESS (Session 93)

**Origin:** Gokul supplied `tdse-pet-dragon-adaptation.md`, a design doc
proposing a new move-selection mechanism: when the root search finds
several near-tied best moves, prefer whichever one best defuses the
opponent's strongest reply (probed via a one-ply lookahead reusing the
existing null-move machinery), rather than picking arbitrarily among
near-equal options. Not a search-strength technique in the usual sense
(it doesn't change what depth/score is reached) — a move-*selection*
tiebreaker among moves the search already considers equivalent.

**Verification before any code was written:** the doc claimed to be
"verified against the actual file" at ~10 separate points across
`alpha_beta.rs`, `ordering.rs`, `iterative.rs`, `mobility.rs`, `see.rs`,
`pruning.rs`, and the bitboard module. Independently re-verified every
one of those claims against a fresh pull of the real repo before
trusting any of them — did not take "verified" in someone else's
document at face value. Result: essentially every claim checked out
exactly, including the doc's own corrections to its first draft (e.g.
`mobility_for_color()` being private, `alpha_beta_with_excluded()`
returning only a score not a move+PV). **One real error found that the
doc missed**: it called `threat_move.to_square()`, which doesn't exist
— `Move` has plain public fields `from`/`to: Square`, not accessor
methods. Fixed during implementation. Full verification trail and the
correction in DECISIONS.md D98.

**Shipped this session — first diff only, per the doc's own staged
rollout plan (§5):** runtime UCI option `ThreatDefusal`, default
`false`, byte-identical behavior when off (same guarantee every prior
D75/D91/D92 toggle makes) — threaded through `SearchInfo` → 
`EngineState` → `cmd_uci`/`cmd_setoption`/`cmd_go` (main thread + every
Lazy SMP helper thread), identical shape to `LMPEnabled`/
`SingularMultiCutEnabled`. New `extract_threat_move()` in
`alpha_beta.rs` (one-ply opponent-reply probe, deliberately *not*
reusing the shared `info.pv`/`info.update_pv` table — see D98 for why)
and a **legality-only** `defuses_threat()` signal — does the candidate
move make the opponent's probed threat move no longer legal, yes/no.
SEE-degradation and square-control signals from the original proposal
are **deliberately not implemented yet** — each is its own isolated
diff, validated independently, before being combined, same discipline
Phase 26's correction-history sub-items (3a/3b/3c) already used. New
sibling block in `iterative_deepening()` (not a hook into `alpha_beta`'s
move loop), reusing Phase 19's `search_multipv_slot`/`info.root_exclude`
exactly the way the existing Skill Level noise block already does —
mutually exclusive with that block (skipped whenever
`skill_noise_window_cp(info.skill_level) > 0`), per a real interaction
risk the proposal itself flagged.

Ten new tests across `search/mod.rs` (implicit via `SearchInfo::new()`
default check in `alpha_beta.rs`'s test module), `search/alpha_beta.rs`
(default-false; `extract_threat_move` correctly finds a hanging-piece
capture on a constructed FEN; `defuses_threat` correctly returns both
`true` and `false` on hand-built before/after positions), `main.rs`
(option default/parse/cmd_go-wiring, mirroring
`test_cmd_go_applies_singular_multicut_enabled_to_search`), and
`search/iterative.rs` (default-off byte-identical to pre-TDSE behavior;
enabling it on a real search doesn't panic and still returns a legal
move).

⚠️ **Not yet CI-confirmed** — no local `cargo` in this session's
sandbox, same caveat as every session in this repo's history. This is a
substantially larger diff than D95's one-line fix was — extra reason to
watch CI closely, not assume green.

⚠️ **Not yet validated for actual Elo impact at all.** This session
shipped the option scaffold and the legality-only signal only. The
proposal's own rollout plan (§5, steps 5-7) — CI green → commit → 20-game
first look (not trusted alone, same D76/D84 discipline) → 100-200 game
confirmation → only then touch the default, and only then add the
SEE-degradation signal as its own separately-validated diff, then
control-delta likewise — has not started. `ThreatDefusal` stays `false`
by default until that full cycle completes.

**Not yet done — next session should start here:**
1. Gokul commits the 4 changed files (`src/search/mod.rs`,
   `src/search/alpha_beta.rs`, `src/search/iterative.rs`, `src/main.rs`)
   and confirms CI is fully green — all tests, including the 10 new
   ones.
2. Run a 20-game `uci_match_runner` A/B (`ThreatDefusal=true` vs.
   `false`, fresh seed) as a first look only — do not promote off this
   result alone, same as every prior first-look in this project's
   history.
3. If the first look isn't clearly negative, a 100-200 game confirmation
   run before considering flipping the default. Record both results as
   D-series entries (next available after D98).
4. Only after the legality-only signal is independently confirmed:
   implement the SEE-degradation signal as its own isolated diff (the
   proposal's §3, corrected `attacker_count_on`/
   `control_delta_on_threat_squares` helpers are new code, not free
   reuse — flag as such), re-run the same A/B → confirm cycle. Then
   control-delta, same pattern. Three signals, three independent
   validations, never bundled.

---

**Update (Session 94):** Gokul ran the 20-game first look via the
`uci_match_runner` mobile Actions workflow, exactly per step 2 above.
**Result: A (control, `ThreatDefusal=false`) 3 wins, B (`ThreatDefusal=
true`) 1 win, 16 draws — A score 55.0%, Elo diff +34.9 favoring the
control.** Checked the statistics before reacting to the direction: with
only 4 decisive games out of 20, the 95% CI on this result is roughly
(45%, 65%) — **not statistically distinguishable from zero** at this
sample size, same "don't trust n=20 alone" standard D76/D84 already
established for this project. Full writeup in DECISIONS.md D99.

**Open decision, explicitly not resolved yet — Gokul's call:** the
point estimate leans negative, not neutral, which is a different
situation than a genuinely flat first look. Two legitimate paths: (a)
run the 100-200 game confirmation the plan already committed to for any
non-clearly-negative first look, since this one's CI does include zero;
or (b) treat the negative lean as enough signal to deprioritize TDSE for
now without spending the larger CI budget, revisitable once the
SEE-degradation/control-delta signals exist and might change the
picture. `ThreatDefusal` stays `false` by default regardless of which
path is chosen.

---

**Update (Session 95) — ⚠️ CRASH FOUND, more urgent than the Elo
question above.** Gokul chose path (a) and ran the 200-game
confirmation (seed 61000, 100ms/move). **It crashed after 14 completed
games** — the harness's own "engine process closed stdout while waiting
for 'bestmove'" panic, meaning one of the two child engine processes
(almost certainly Engine B, `ThreatDefusal=true`, given this is
brand-new code never exercised at this scale) itself panicked and
aborted (`Cargo.toml`'s release profile uses `panic = "abort"`,
matching the observed "Aborted (core dumped)"). **A crash matters more
than Elo — this disqualifies TDSE from any default-on consideration
regardless of what the Elo question resolves to**, until root-caused
and fixed.

The actual engine panic message wasn't captured anywhere in the CI
logs — traced this to `uci_match_runner.rs` spawning child engines with
`.stderr(Stdio::null())`, silently discarding exactly the stream a Rust
panic prints to. **Fixed** (this session, one file): `Stdio::inherit()`
instead, plus the panic message in `wait_for_line_starting_with` now
names which engine (`label_a`/`label_b`) closed its stdout. No workflow
YAML change needed — GitHub Actions already captures the harness's own
stderr combined with stdout, `inherit()` just stops throwing away the
child's copy of that same stream.

Static review of the new TDSE code (`extract_threat_move`,
`defuses_threat`, the `iterative.rs` sibling block) didn't find an
obvious panic source by reading alone — checked every unwrap/index/
overflow possibility, all clean or ruled out (see DECISIONS.md D100 for
the full trail, including confirming `panic="abort"` explains the
crash *symptom* without pointing at a specific line). **The real fix
requires the real panic message from a re-run with the harness fix in
place — not another round of guessing.**

**Not yet done — next session should start here:**
1. Gokul commits `src/bin/uci_match_runner.rs` (this session's
   diagnostic fix) and confirms CI is green (the match-runner binary
   itself has no new tests to add here, but confirm it still builds and
   the existing regression-gate workflows that use it still pass).
2. Re-run the same 200-game confirmation (same seed, 61000, for an
   exact repro) with the fixed harness. This time the crashed engine's
   own panic message and source location should appear directly in the
   CI log.
3. Read that specific line in the specific file it names, in full,
   before writing any fix — same discipline as every other bug in this
   project's history. Do not guess at a fix from D100's hypothesis
   alone; confirm against the real message first.
4. Only once the crash is fixed and re-confirmed crash-free does D99's
   Elo question (100-200 game confirmation vs. deprioritizing TDSE)
   become relevant again — a crashing technique isn't a candidate for
   promotion regardless of its Elo number.

---

**Update (Session 96) — root cause found and fixed, not yet
re-confirmed.** Gokul re-ran the 200-game confirmation with D100's
harness fix in place. Real panic captured this time:
`position::king_sq()`'s "King must always be on the board" — a king
bitboard had gone empty — in Engine B (`ThreatDefusal=true`) specifically,
14 games in, confirming this is TDSE's own bug (Engine A, same commit,
same binary, ran the same games with default options and never
crashed).

Compared `extract_threat_move` against the real null-move block it
explicitly claims to mirror (`alpha_beta_with_excluded`'s
`can_null_move`) rather than guessing from the panic message alone:
found `extract_threat_move` never checks `!in_check` before flipping
side to move, the one guard in the real null-move condition that's
about correctness (not pruning-effectiveness heuristics, which don't
apply to a threat probe and were correctly left out). Flipping sides
while the current side is in check is genuinely undefined territory for
the move-generation/check-evasion machinery this probe hands the
position to. **Fixed**: one guard, `if pos.in_check(pos.side_to_move) {
return None; }`, first line of the function. New regression test
constructs an in-check FEN and asserts both kings stay exactly in place
afterward (not just that the function returns early — this was a
board-corruption bug, so the test checks for that specifically). Full
trail in DECISIONS.md D101.

⚠️ **This is a well-reasoned, precedent-matching fix for a confirmed
real deviation — not yet confirmed to be the complete fix.** D100's
standing rule still applies: TDSE stays disqualified from any
default-on consideration until a full 200-game run completes with zero
crashes.

**Not yet done — next session should start here:**
1. Gokul commits `src/search/alpha_beta.rs` (this session's D101 fix +
   new test) and confirms CI is green, including the new in-check
   regression test.
2. Re-run the 200-game confirmation again, same seed (61000) for
   comparability with the two prior attempts. If it completes all 200
   games with no crash, TDSE clears D100's crash-safety bar for the
   first time — then, and only then, D99's original Elo question
   (100-200 game confirmation vs. deprioritizing TDSE) becomes relevant
   again, now with a real, uncorrupted 200-game Elo result available
   from this same run.
3. If it crashes again (same or different panic), same discipline as
   this session and last: get the real panic message first, read the
   exact line it names, don't guess.

---

**Update (Session 97):** `cargo test` on the D101 commit caught a bug in
D101's own regression test (not the fix) — its FEN never placed a Black
king, so `from_fen()` correctly rejected it (`KingNotFound(Black)`).
Fixed the test FEN (added a Black king on h8), test now correctly
exercises the same in-check scenario. 482/483 passed before this fix,
D101's actual guard was never in question. Full note in DECISIONS.md
D102. Same next step as above still applies: commit, confirm fully
green CI, re-run the 200-game confirmation.

---

**Update (Session 98) — TDSE clears the crash-safety bar; Elo picture
now mildly positive, still not conclusive.** Gokul re-ran the 200-game
confirmation, same seed (61000), with D101/D102 in place. **All 200
games completed — zero crashes.** The `!in_check` fix is confirmed
against the exact scenario that crashed the previous attempt at game 15.

Elo result, same run: **A (control) 48.0%, B (`ThreatDefusal=true`)
52.0% — Elo diff -13.9 favoring TDSE**, reversing D99's n=20 first look
(which leaned the other way and was already flagged as noise-level).
95% CI on this larger sample: roughly (43.7%, 52.3%) — much tighter than
the n=20 result and now mostly on TDSE's favorable side, but still
narrowly includes 50%, so **not yet a statistically conclusive
positive**. Full stats and reasoning in DECISIONS.md D103.

**Where Phase 28 stands:** crash-safety resolved. Elo: promising, not
proven — the CI still touching 50% means `ThreatDefusal`'s default stays
`false` regardless of the encouraging direction; that's not this
session's call to relax.

**Not yet done — two independent, non-blocking paths forward:**
1. **Elo confidence**: a larger confirmatory run (400+ games, fresh
   seed) if Gokul wants to resolve the remaining statistical ambiguity
   before any default-flip discussion — same "don't trust a
   near-the-noise-floor result" discipline as D76/D84.
2. **New signal**: implement the SEE-degradation signal (the proposal's
   §3, still not started since D98) as its own isolated diff, now that
   the legality-only signal is confirmed crash-safe — doesn't need to
   wait on path 1 finishing.
Either is a reasonable next session; Gokul's call which to prioritize.

---

**Update (Session 99):** Gokul chose path 2. Implemented the
SEE-degradation signal — and found **two more real bugs in the
proposal's own §3 code**, beyond the `to_square()` one D98 already
caught, both by reading actual callee implementations rather than
trusting the proposal's comments: (1) `threat_see_before` was computed
*after* restoring `pos.side_to_move`, but `see_value_of` needs it to
still equal the threat's actual mover — the original ordering would
have silently returned 0 every time, no panic, no obvious symptom; (2)
`control_delta_on_threat_squares`'s `mover_color` was computed with an
extra `.flip()` that doesn't belong there given where the function is
actually called from — would have silently inverted the whole signal.
Both fixed; both share the same "fails silently with a plausible-looking
wrong number" shape, exactly what independent verification exists to
catch. New test asserts the *exact* expected SEE value (500, an
undefended rook) rather than just non-panic — would have caught bug (1)
directly. Full trail in DECISIONS.md D104.

`iterative.rs`'s TDSE block now uses `defusal_score` (weighted: illegal
> SEE-drop > control-delta) to pick the best near-tied candidate,
replacing D98's "first candidate that merely makes the threat illegal."
`defuses_threat` (D98) is kept and still tested, just no longer called
from production code.

**Not yet done — next session should start here:**
1. Gokul commits `src/search/alpha_beta.rs` and `src/search/
   iterative.rs` (this session's SEE-degradation diff) and confirms CI
   is fully green, including the 4 new tests.
2. Run a 20-game `uci_match_runner` A/B first look (`ThreatDefusal=true`
   vs. `false`, fresh seed) — not trusted alone, same as every prior
   first look.
3. If not clearly negative, a 100-200 game confirmation, watching
   specifically for any new crash — `defusal_score` adds its own
   `pos.make_move`/`unmake_move` pair beyond what D101 already fixed,
   worth explicit attention given this investigation's history.
4. Only after this signal is independently confirmed: control-delta as
   its own signal was already implemented *as part of* this session's
   diff (it's bundled into `defusal_score` alongside SEE, not held
   back as a separate diff — note this deviates from the original
   proposal's "three signals, three independent validations" plan; flag
   this explicitly if a future session wants to isolate SEE-only vs.
   control-delta-only impact).

---

**Update (Session 100):** CI on the D104 commit failed to compile —
`attacker_count_on`'s `Square` parameter type was never added to
`alpha_beta.rs`'s top-level imports (only the test module had its own).
My mistake, not a gap balance-checking could catch. Fixed: one import
line. No design/logic changes. Full note in DECISIONS.md D105. Same
next steps as above still apply once this is committed and CI is
actually green.

---

**Update (Session 101):** Gokul ran both the 20-game first look (seed
62000) and the 200-game confirmation (seed 62500) for the
SEE-degradation signal. **Zero crashes in either** — crash-safety holds
with the added `defusal_score` machinery. Elo: n=20 leaned toward TDSE
(-34.9, i.e. B/TDSE ahead), n=200 landed almost exactly on zero (+3.5,
mildly favoring the control). 95% CIs: n=20 (35.4%, 54.6%), n=200 (46.3%,
54.7%) — both comfortably straddle 50%.

**Comparing all four Elo measurements from Sessions 94/98/101
together** (legality-only n=20/n=200, SEE-degradation n=20/n=200): every
interval overlaps every other one, including the two n=20 results
pointing in *opposite* directions from each other. **Across two
independent 200-game samples now, TDSE has not demonstrated a clear,
replicated Elo effect in either direction.** D103's legality-only run
leaned mildly positive (+13.9 favoring TDSE); this session's
SEE-augmented run landed near-neutral (+3.5 favoring the control). If
there's a real effect at the current weights, it's small — full data
and reasoning in DECISIONS.md D106.

**This isn't "TDSE failed," it's "TDSE hasn't earned promotion"** — same
distinction D90 drew before closing Phase 26. Two live, non-exclusive
explanations: the technique itself may have little real effect at this
engine's current strength, or the three weight constants
(`WEIGHT_ILLEGAL`/`WEIGHT_SEE`/`WEIGHT_CONTROL`) and `TDSE_MARGIN_CP`
were never tuned — explicitly flagged as guessed starting points in
D104 — and could be diluting a real signal.

**Not yet done — Gokul's call, genuinely open:**
1. **Deprioritize TDSE for now** — two 200-game runs without a clear win
   is a reasonable stopping point. `ThreatDefusal` stays `false` by
   default (already true), nothing to undo, safely revisitable later.
2. **Or Texel-tune the weights** before another confirmation run — "
   validate mechanism, then tune" has had its validation step done
   twice now without resolving the question; tuning is the one lever
   that's never been touched. This would use the existing `texel/`
   pipeline, same as HCE terms.
Either is reasonable; this entry doesn't pick one.

---

## Phase 29 — Five-Mode PlayStyle Additive Eval Bonus ⏳ IN PROGRESS (Session 106, D111)
- [x] 29.1 — `src/eval/style.rs` (NEW): `PlayStyle` UCI spin option
      (0-4: Balanced/Killer/Tactical/Positional/Endgame), additive
      bonus on top of the existing tuned eval, search itself untouched.
      Balanced (default) is a guaranteed no-op — confirmed by
      regression test, not just asserted. All four non-Balanced modes'
      constants are hand-picked starting points, not yet Texel-tuned
      (flagged explicitly in the file's module doc comment).
- [x] 29.2 — `src/eval/mod.rs`: added `pub mod style;` and
      `evaluate_styled()` wrapper (`evaluate_blended(pos) +
      style::evaluate_style(pos, phase)`).
- [x] 29.3 — `src/search/alpha_beta.rs`: `evaluate()` now delegates to
      `evaluate_styled()` instead of `evaluate_blended()` — the one
      call-site change this phase touches. Confirmed current line
      number fresh (1169) rather than trusting the proposal doc's
      stale citation (1133), which had already drifted once before
      this session re-verified it.
- [x] 29.4 — `src/main.rs`: `PlayStyle` UCI option declaration +
      `"playstyle"` setoption arm, mirroring the existing `NNUEWeight`
      direct-global-set pattern exactly. Bonus fix while this block was
      open: `NNUEWeight`'s own doc comment had the same "D23 default
      25%" staleness F-4/D110 already fixed in `eval/mod.rs` — fixed
      here too.
- [x] 29.5 — 10 new regression tests total (8 in `style.rs`, 1 in
      `eval/mod.rs`, 1 in `main.rs`). Full breakdown in DECISIONS.md
      D111.
- [x] 29.6 — Self-play first-look, n=20 per mode vs. Balanced (Session
      107, D112): all four modes land within noise of Balanced —
      Killer -17.4 Elo (A vs B), Tactical -0.0, Positional -34.9,
      Endgame +17.4 (sign is A-Balanced-vs-B-mode; negative means the
      mode nominally scored above Balanced). None crossed the ~100 Elo
      loss flag threshold. At n=20 (~±80 Elo noise floor), none of
      these are distinguishable from zero effect — same "near the
      noise floor" situation D106 already documented for TDSE. See
      DECISIONS.md D112 for the full table and reasoning.
- [x] 29.6b — DONE (Session 115, D113). Ran the n=200 confirmation on
      all four modes' current hand-picked constants (800 games total,
      seeds 70000/70200/70400/70600, movetime 100ms). **All four
      flattened to within noise of Balanced**: Killer +1.7 Elo,
      Tactical -0.0, Positional +13.9, Endgame -0.0 (sign is A/Balanced
      vs B/mode). Confirms every one of D112's n=20 leans (as large as
      -34.9/+17.4) was sampling noise, not signal — same pattern D103
      established for TDSE (n=20 leans washing out at n=200). No mode
      is broken or measurably improved at its current untuned
      constants. Full table and reasoning in DECISIONS.md D113.
- [x] 29.7 — Pipeline built (Session 110): folded each mode's
      `style.rs` constants through the same Texel tuning pipeline used
      for the core eval (proposal §7.3), as scoped in the "Option 1:
      build the full integration" discussion. Confirmed via an actual
      `cargo build`/`cargo test` (not manual review — see below) plus a
      real end-to-end smoke run.
    - [x] 29.7a — `src/texel/features.rs`: new `StyleFeatures` struct +
          `extract_style_features(pos, mode)`, duplicating `style.rs`'s
          4 bonus functions' feature-gathering halves exactly (verified
          line-by-line against the live source, not from memory).
          `TexelFeatures` gets a new `style: Option<StyleFeatures>`
          field, always `None` from `extract_features()` itself — that
          function and its self-consistency test are untouched.
    - [x] 29.7b — `src/texel/weights.rs` / `weights_f64.rs`: 5 new
          tunable fields (`killer_attacker_bonus[8]`,
          `killer_storm_bonus_per_pawn`, `tactical_bonus_per_square`,
          `positional_bonus_per_square`, `endgame_bonus_per_unit`) —
          flat i32/f64, not `S(mg,eg)`, since each already gets its own
          phase-scaling directly (matches `style.rs`'s own math, not
          the taper curve the rest of HCE uses). `PARAM_COUNT`,
          `flatten`/`unflatten`, `to_tunable_weights`,
          `From<&TunableWeights>`, and the round-trip test all extended.
    - [x] 29.7c — `src/texel/predict.rs` / `predict_f64.rs`:
          `style_score`/`style_score_f64` (forward pass, mirrors
          `evaluate_style`'s arithmetic exactly) and
          `style_score_and_grad_f64` (gradient accumulation — every
          PlayStyle term is a plain linear product, no clamps, so every
          gradient is a direct `error_signal * feature * phase_factor`
          term). New tests: bit-exact match against
          `evaluate() + evaluate_style()` across all 4 modes × 100
          seeds; a test proving `predict()` ignores the global
          PlayStyle atomic when `f.style` is `None`; a forward-
          consistency sweep (grad-path score == plain-path score); and
          — the strongest one — a **finite-difference numerical
          gradient check**, which catches a wrong sign or a missing
          phase factor that a pure self-consistency check cannot.
    - [x] 29.7d — `src/bin/texel_gen.rs`: new 4th CLI arg
          (`play_style_mode`, default Balanced), calls
          `eval::style::set_play_style()` once per run so the whole
          game is actually played under that style, and stamps every
          sample it writes with the mode. Output format extended to
          `<FEN>|<result>|<mode>` — purely additive; a bare 2-field
          line is still valid and means "no tag."
    - [x] 29.7e — `src/bin/texel_tune.rs`: `load_samples()` rewritten
          to accept BOTH the 2-field (legacy, implicit Balanced) and
          new 3-field format in the same run, and calls
          `extract_style_features()` for any non-Balanced-tagged
          sample. `write_tuned_weights()` extended to emit the 5
          PlayStyle constants under their real `eval/style.rs` const
          names, ready to copy in directly (same "step 6" workflow
          Phase 25 established for the core eval).
    - [x] 29.7f — `src/bin/texel_diag.rs` (a file this session hadn't
          originally scoped in — found only because an actual compile
          caught it): extended `parse_tuned_weights()` with the same 5
          fields, so the diagnostic round-trip still works on tuned
          output files that include PlayStyle constants.
    - [x] 29.7g — **Actually compiled and tested, not just reviewed.**
          Installed a Rust toolchain in-session (not available by
          default) specifically to verify this rather than trust
          manual inspection on a change this size — caught and fixed 3
          real bugs review alone had missed: a duplicate
          `chebyshev_distance` definition (E0428, `features.rs` already
          had one for an unrelated feature), `texel_diag.rs`'s
          `TunableWeights` construction not knowing about the 5 new
          fields (E0063, a file not originally read/scoped), and two
          bin files (`texel_gen.rs`, `texel_tune.rs`) using `crate::`
          for library paths when bin crates need `pet_dragon_lib::`
          instead (E0433). After fixes: `cargo build --lib`/`--bins`
          clean, `cargo test --lib` **504/504 passing** (finite-
          difference gradient test included), and a real end-to-end
          smoke run — `texel_gen` generated actual KILLER-tagged
          self-play data, `texel_tune` loaded it mixed with hand-
          crafted legacy-format data in the same run (proving
          backward-compatibility, not just claiming it), ran 2 real
          epochs, and **loss decreased monotonically** (0.0571 →
          0.0508) with only the tagged mode's constants moving —
          `texel_diag` then round-trip-parsed that exact output file
          without error.
    - [x] 29.7i — Bug caught by the first real CI run of 29.7's tests
          (Session 111): `texel::predict::tests::test_predict_style_
          matches_evaluate_styled_all_modes` failed on GitHub Actions —
          `mode 1 seed 12: predict=44 evaluate+style=24 (core=24,
          style=0)` — but had passed 3/3 times locally (this sandbox
          only has 1 CPU core; Actions runners have more). Root cause:
          `PLAY_STYLE` is a process-global atomic; this test holds a
          mode fixed across a 100-seed loop — a wide enough window for
          `eval/style.rs`'s own pre-existing tests (a different file,
          also calling `set_play_style()`) to flip the global mid-loop.
          First fix attempt: a `pub(crate) #[cfg(test)] static
          PLAY_STYLE_TEST_LOCK: Mutex<()>`, acquired by all 8
          pre-existing tests in `eval/style.rs` plus both new tests in
          `predict.rs`. **This was necessary but confirmed NOT
          sufficient** — see 29.7j below, found while chasing the
          separately-flagged ThreatDefusal flake: `search::alpha_beta`'s
          leaf eval calls `eval::evaluate_styled()` UNCONDITIONALLY on
          every node of every search, in every test in the whole
          ~500-test suite — not just PlayStyle-aware ones — so a Mutex
          that only PlayStyle-aware tests know to acquire does nothing
          to protect an unrelated test (like ThreatDefusal's) from
          having its own search corrupted by a concurrently-running
          PlayStyle test on another thread. Confirmed empirically, not
          assumed: the Mutex alone measurably reduced but did not
          eliminate the flake (2/20 stress runs still failed after that
          fix alone).
    - [x] 29.7j — **The actual complete fix**, found by investigating
          the "side finding" 29.7i originally flagged and deferred
          (Session 112, same day, immediately after 29.7i — the
          ThreatDefusal test itself has nothing to do with PlayStyle;
          it was corrupted BY the PlayStyle tests via the mechanism
          above, not a second unrelated bug). All 10 PLAY_STYLE-
          mutating tests (8 in `eval/style.rs`, 2 in `predict.rs`)
          marked `#[ignore]`, with a reason string pointing to
          `PLAY_STYLE_TEST_LOCK`'s doc comment, and moved to a new,
          dedicated `build.yml` step — `cargo test --lib -- --ignored
          --test-threads=1 eval::style::tests:: texel::predict::
          tests::` — run right after the existing "Run uci-wasm
          feature tests" step. At `--test-threads=1`, nothing else in
          the binary can execute concurrently, which is the actual
          correctness requirement ("don't race anything," not "don't
          race each other"). Exact same pattern this project already
          used for `node_count`'s benchmark-style tests — not a new
          convention. `PLAY_STYLE_TEST_LOCK` kept as cheap defense-in-
          depth in case these are ever run un-ignored by mistake.
          Along the way, also wrapped `init_masks()`/`init_magic()`/
          `init_zobrist()` (the "mandatory startup sequence," called by
          every one of the ~500 tests' `setup()` helper, writing to
          unsynchronized `static mut` globals) in `std::sync::Once` —
          genuine UB under Rust's rules regardless of whether it was
          THE cause of this specific flake (empirically, per the stress
          testing below, it wasn't the dominant cause — the
          `PLAY_STYLE`/`evaluate_styled()` mechanism was — but it's a
          real, separate correctness issue worth fixing regardless, and
          it's free: identical external signatures, ~500 existing call
          sites need no changes). **Verified by actually trying to
          break it, repeatedly, not by re-running once**: 25 full-suite
          stress runs at `--test-threads=8` (0/25 failed, vs. 2/20
          before this fix), 10 more at `--test-threads=16` (0/10), and
          5 repeats of the new isolated step itself (5/5 clean, 10/10
          tests passing each time) — 35/35 clean total across every
          stress configuration tried.
    - [ ] 29.7h — **Not done this session, the actual remaining work**:
          generate real bulk self-play data for all 4 non-Balanced
          modes (hundreds/thousands of games each, via Actions — the
          smoke test above used 2 toy games, nowhere near enough to
          tune on for real), run the real tuning pass, sanity-check
          the result with `texel_diag`, and copy the tuned constants
          into `eval/style.rs`'s actual consts. This is a compute-cost
          decision (how many games per mode), Gokul's call, same
          "not blocking either way — PlayStyle defaults off" status
          29.6b already has.
- [ ] 29.8 — Open question, Gokul's call (proposal §8): keep
      `PlayStyle` as a 0-4 spin (current implementation, consistent
      with `Contempt`/`NNUEWeight`) or switch to five UCI `combo`
      labels for GUI legibility? Not blocking — easy to change later.

---

## Phase 30 — Second WASM Build Target: Real UCI Protocol (Session 108)

Separate from Phase 29/PlayStyle — new work, requested directly by Gokul
rather than the next open ROADMAP item (29.6b was left open, not
superseded; still available whenever Gokul wants to pick it back up).

Context: `web/index.html`/`web/pit/vs.html` use the existing `wasm`
feature's direct-function-call API (`search_from_fen`, `new_game`,
etc.) — not real UCI text protocol at all (uci-options-comparison.md,
reviewed this session, confirmed this explicitly). The native binary
(`src/main.rs`) already speaks full UCI. This phase adds a **second,
independent** WASM build target that also speaks real UCI text, for any
future UCI-speaking browser GUI to embed — without touching or risking
the existing browser gameplay build at all.

- [x] 30.1 — New `uci-wasm` Cargo feature (`Cargo.toml`), same
      dependencies as `wasm` (wasm-bindgen, js-sys, getrandom/js,
      console_error_panic_hook), but a genuinely separate feature —
      never both active in the same `wasm-pack` invocation.
- [x] 30.2 — New `src/uci_wasm.rs`: single `uci_command(line: &str) ->
      String` export. Persistent per-session `UciSession`
      (`thread_local!` + `RefCell` — WASM is single-threaded) holding
      position, TT (kept warm across moves), and the option values that
      aren't already engine-wide globals (`NNUEWeight`/`PlayStyle` are
      — reused directly via `eval::set_nnue_weight_pct`/
      `eval::style::set_play_style`, not duplicated).
      Commands implemented: `uci`, `isready`, `ucinewgame`, `position
      [startpos|fen ...] [moves ...]`, `go [depth|movetime|wtime/btime/
      winc/binc/movestogo|nodes|infinite]`, `setoption name <id> value
      <x>` (Hash, Threads [pinned 1], Move Overhead, Skill Level,
      Contempt, UCI_LimitStrength, UCI_Elo, NNUEWeight, PlayStyle),
      `stop`/`ponderhit`/`quit`/unrecognized (no-ops). 16 new unit tests.
- [x] 30.3 — `src/lib.rs`: declared the new module
      (`#[cfg(feature = "uci-wasm")]`), widened `wasm_main()`'s
      startup-sequence gate and the `wasm_bindgen::prelude` import from
      `feature = "wasm"` to `any(wasm, uci-wasm)` — both build targets
      need the same mandatory `init_masks()→init_magic()→init_zobrist()`
      + panic-hook setup. Caught and fixed mid-session: the import
      widening was missed on the first pass, which would have broken a
      uci-wasm-only build (wasm_bindgen unresolved) — fixed before
      shipping, not after.
- [x] 30.4 — `.github/workflows/deploy.yml`: second `wasm-pack build`
      step, `--features uci-wasm` → `web/pkg-uci/` (alongside the
      existing `web/pkg/`), both served automatically since the whole
      `web/` folder is already the Pages upload — no separate Pages
      config needed.
- [ ] 30.5 — **Known, documented architectural limitation, not yet
      solved**: because `uci_command` is a single synchronous WASM call,
      (a) `stop` can never interrupt an in-flight `go` — JS is blocked on
      the call stack for the whole search; (b) `go infinite` is clamped
      to a bounded fallback (`INFINITE_FALLBACK_MS` = 30s) rather than
      honored literally, to avoid an unrecoverable hung tab; (c) no live
      `info depth N ...` streaming mid-search, only one final info+
      bestmove line; (d) `ponder`/`ponderhit` unsupported. A real fix
      needs an async/threaded model (Web Worker + `postMessage`, or
      SharedArrayBuffer-backed threads) — meaningfully bigger than this
      session's scope, not started. **Update (Session 128, D126):
      SharedArrayBuffer is ruled out for this specific deployment target
      — GitHub Pages can't set the required COOP/COEP headers. The
      viable path without special headers is cooperative chunking (`go`
      broken into repeated per-depth calls instead of one long
      synchronous one) — see DECISIONS.md D126 for the full analysis.
      Scoped but deliberately deferred, not started; a real redesign of
      `go`'s execution model, not a small patch.**
- [x] 30.6 — `build.yml`'s existing `test` job runs `cargo test
      --verbose` with no features, which would never have touched
      `uci_wasm.rs`'s tests (gated behind `uci-wasm`, not default).
      Added a second step, `cargo test --verbose --features uci-wasm`,
      right after it — same job, native host target, no wasm32/
      wasm-bindgen-test-runner setup needed. **Build-verified**:
      Gokul ran Actions (Session 109) — plain `cargo test` step: 517
      tests, all green, zero regression from Phase 30's `lib.rs`
      changes. `--features uci-wasm` step: compiled clean, 3 of the 16
      new tests failed — real bug, not a CI/environment issue, see
      30.9.
- [x] 30.7 — DONE (Session 114). Mirrored the second-build addition into
      `build.yml`'s `build-wasm` job (D46): a new `wasm-pack build
      --features uci-wasm --out-dir dist-wasm-uci --out-name
      pet_dragon_uci` step, output copied to `dist/pet_dragon_uci_bg.wasm`
      / `dist/pet_dragon_uci.js`, uploaded as its own
      `pet-dragon-uci-wasm` artifact (mirrors the existing `pet-dragon-
      wasm` artifact exactly, separate wasm-pack invocation from the
      `wasm`-feature build directly above it, per Phase 30's own
      "never both active in the same wasm-pack invocation" rule). Both
      new files also added to the `release` job's generated body table
      — `release` already globs `dist/**/*` from all downloaded
      artifacts, so no change needed there beyond the table row itself.
      YAML validated with a real `pyyaml` parse before delivery (Session
      113's process lesson — a colon-related workflow syntax error was
      caught by Actions itself, not review, last time). ⚠️ Not yet
      CI-confirmed — no local `cargo`/`wasm-pack` in this sandbox to
      actually run the build; Gokul needs to commit and confirm the
      `build-wasm` job succeeds and both new release assets appear.
- [ ] 30.8 — Open question, Gokul's call: is there an actual target
      UCI-speaking browser GUI in mind for this, or is it speculative
      infrastructure? Doesn't block anything already built, but worth
      answering before investing in 30.5's async fix.
- [x] 30.9 — Bug caught by the first real CI run of 30.6 (Session 109):
      3 of `uci_wasm.rs`'s 16 tests
      (`test_position_startpos_sets_standard_position`,
      `test_position_fen_parses_multi_field_fen`,
      `test_ucinewgame_resets_position_and_clears_tt`) asserted
      `session.pos.to_fen() == STANDARD_START_FEN`. `Position::to_fen()`
      always appends the Pet Dragon pawn-start extension (its own doc
      comment says so — 7th FEN field, e.g. `a2:w,b2:w,...`), so this
      could never have matched the bare 6-field `STANDARD_START_FEN`
      constant. Not a production bug — `uci_wasm.rs`'s actual
      `position`/`ucinewgame` logic was never wrong, only these three
      test assertions' expected value was. Fixed by switching to
      `Position::to_standard_fen()` — a method the codebase already
      provides specifically for this ("Used for UCI communication with
      external tools" per its own doc comment), not a new one. All 16
      tests now correct.

---

## Phase 31 — Improving Flag for LMP + Futility Pruning (D114, Session 116)

Prompted by a 4-game external Stockfish bench Gokul uploaded (log +
report) — Pet Dragon lost 4-0 regardless of color/skill/think-time. The
report's own cross-game pattern section flagged search-depth volatility
between neighboring moves as its most consistent signal (more so than
the single clean tactical blunder it also found). D60 (Session 82) had
already flagged the missing "improving" flag as a real, separate,
untaken change; Gokul asked to build it now, scoped broader than D60's
own note (LMP + futility, not LMP alone).

- [x] 31.1 — DONE (Session 116, D114). `SearchInfo::static_eval_stack`
      (new `[i32; MAX_PLY]` field, `i32::MIN` sentinel) + `improving`
      computed per-node in `alpha_beta.rs`, gated behind
      `improving_enabled` (default `false`, zero-cost/byte-identical
      when off — verified by test, not just asserted in a comment).
      `pruning::LMP_THRESHOLDS` renamed `LMP_THRESHOLDS_IMPROVING`
      (values unchanged) + new `LMP_THRESHOLDS_NON_IMPROVING` (~half,
      Stockfish-style); `lmp_threshold()`/`should_apply_lmp()` both
      gained an `improving: bool` param. New `pruning::futility_margin()`
      — `100*depth+200` when improving (unchanged), `100*depth+100` when
      not. New UCI option `ImprovingHeuristic` (check, default false),
      threaded through `EngineState`/`cmd_go`/`h_info`/`main_info` same
      pattern as `NullMoveKingGuard`/`ThreatDefusal`. Full test coverage:
      pruning.rs (dual-table selection, byte-identical-when-improving-
      true for both LMP and futility), alpha_beta.rs (defaults false,
      stack untouched when off, stack populated when on, search
      completes safely when on), main.rs (option defaults/parses/
      threads into the real search SearchInfo, not just EngineState).
      ⚠️ Not yet CI-confirmed (no local `cargo test` in this sandbox).
      ⚠️ Not yet Texel-tuned (search constant, not an eval weight — same
      status every other LMP/futility constant has always had) and not
      yet Elo-measured — needs its own `uci_match_runner.yml` SPRT-style
      A/B before any default-on flip is ever considered, same open item
      every other unproven Phase 26/27/28 toggle already carries.
- [x] 31.2 — CLOSED (Session 117, D115). Gokul ran the A/B directly at
      confirmation scale (200 games, skipped the 20-game first look):
      Engine A `ImprovingHeuristic value true` vs Engine B `false`, seed
      80000, 100ms/move. **Flat: 40-39-121, 50.2%, +1.7 Elo** — no
      measurable effect at the current hand-picked thresholds, same
      conclusive shape D77 reached for `NullMoveKingGuard`. Parked off
      (`ImprovingHeuristic` stays default `false`); mechanism stays in
      the code, real and tested, not reverted. Full reasoning in
      DECISIONS.md D115 — this doesn't confirm or rule out the
      underlying idea, just the specific threshold/margin deltas D114
      picked.
- [ ] 31.3 — Open, not started, explicitly out of scope for 31.1: the
      bench report's one clean tactical blunder (`e2b5`, Match 3 — hangs
      a piece to a simple recapture, searched one ply shallower than its
      immediate neighbors) suggested SEE-gating LMP/singular-extension
      margins more tightly on capture-adjacent nodes. Different
      mechanism than the improving flag; not attempted this session.

---

## Phase 32 — External Code Review Follow-Up (Session 118)

Gokul uploaded a detailed external code review (`pet-dragon-detailed-
report.md`, checked against `pet-dragon-main.zip`) covering the full
engine-affecting codebase, and asked "what's your take" before acting
on any of it. 5 of its 8 findings were directly spot-verified against
live `main` source before trusting the report (see DECISIONS.md D116)
— all 5 checked out exactly as described. Gokul asked to fix all 8, in
priority order: #1 → #3 → #4 → #2 → #5 → #6 → #7, with #8 parked
(self-flagged by the report itself as harmless today, gated behind a
default-0 NNUE blend weight).

- [x] 32.1 (review finding #1) — DONE (Session 118, D116). Fifty-move-
      rule draw no longer overrides checkmate. See DECISIONS.md D116
      for the full fix and reasoning. Two new regression tests.
      ⚠️ Not yet CI-confirmed.
- [x] 32.2 (review finding #3) — DONE (Session 119, D117). `is_recapture()`
      bug confirmed real (as reported) but its "extends nearly every
      capture" severity claim was wrong in an important way: the only
      caller, `pruning::extension()`, was never called anywhere in the
      live search — verified by grepping the whole repo, its only call
      site was its own unit test. Zero real play-strength impact before
      this fix. Surfaced that to Gokul before proceeding; he chose fix +
      wire in (gated). `is_recapture()` now correctly checks
      `prev_move`; new `recapture_and_passed_pawn_extension()` is called
      live from `alpha_beta.rs`'s move loop (any move, not just the TT
      move), gated behind new `SearchInfo::recapture_extension_enabled`
      / UCI `RecaptureExtension` (default false at the time, byte-
      identical when off). Deliberately scoped to recapture only —
      passed-pawn-push extension (bundled in the same original
      function) wasn't flagged as buggy and stays unwired, out of
      scope. 12 new tests. Full reasoning in DECISIONS.md D117.
      **Update (Session 127, D125): default flipped to `true` by
      Gokul's explicit instruction, skipping the SPRT-style A/B this
      note originally called for — see DECISIONS.md D125.**
- [x] 32.3 (review finding #4) — DONE (Session 120, D118). Replaced the
      inline LMR gate in `alpha_beta.rs`'s move loop with a direct call
      to `pruning::should_apply_lmr()` (already existed, already fully
      tested, already correctly excluded killer moves and the TT move —
      the bug was that nothing called it). Shipped live, not behind a
      new gated toggle — treated as a correctness/wiring fix (standard
      practice, matches D116's treatment), not a new speculative
      technique like D114/D117. Also removed the now-unused
      `MIN_DEPTH_LMR` import in `alpha_beta.rs`. 2 new tests. Full
      reasoning in DECISIONS.md D118. ⚠️ Not yet CI-confirmed. No
      dedicated Elo measurement yet — this fix has no "off" state to
      A/B against via `setoption` the way D114/D117's toggles do; would
      need the pre-D118 commit pinned as one side of a match instead.
- [x] 32.4 (review finding #2) — DONE (Session 121, D119). Removed the
      duplicate `TranspositionTable::MATE_THRESHOLD = 30_000` entirely;
      `score_to_tt()`/`score_from_tt()` now use the shared
      `crate::search::MATE_THRESHOLD` (900,000) directly. No import-
      cycle risk (both modules are top-level siblings, confirmed against
      `lib.rs`). Confirmed via full-repo grep that nothing outside
      `tt/mod.rs` referenced the removed constant. 2 tests (updated the
      existing round-trip test, added a direct regression guard using a
      score in the old-vs-real threshold gap). Full reasoning in
      DECISIONS.md D119. ⚠️ Not yet CI-confirmed. **All of Phase 32's
      first four findings (32.1-32.4) are now done.**
- [x] 32.5 (review finding #5) — DONE (Session 122, D120). Confirmed
      real and, unexpectedly, more significant than initially assessed:
      empirically verified (200,000 random `(mg, eg)` pairs) the old
      `mg()` was wrong on ~50% of cases, always off by 1. Discovered
      along the way that `texel/weights_f64.rs` already had a partial,
      independently-written workaround with an *incorrect* comment
      claiming the bug "only affects un-summed single terms" — verified
      that claim false (the bug reproduces identically on an
      accumulated sum of terms), meaning the real live `taper()`
      pipeline (not just isolated eval-term decoding) was reachable by
      this bug whenever a position's total accumulated eg was negative.
      Fixed `mg()` at the source using Stockfish's own
      wrapping-add-then-unsigned-shift technique; simplified
      `weights_f64.rs`'s now-unnecessary workaround to call the fixed
      function directly (confirmed mathematically identical to the old
      workaround across 100,000 cases first). 6 new tests across both
      files. Full reasoning in DECISIONS.md D120. ⚠️ Not yet
      CI-confirmed. **This is the highest-impact fix in Phase 32 so
      far** — everything else fixed before it was either dead code
      (32.2/32.3 before their wiring) or a narrow edge case (32.1) or
      near-zero-reachability hygiene (32.4); this one was live in the
      real evaluation function.
- [x] 32.6 (review finding #6) — DONE (Session 124, D122). Confirmed
      real; added the missing king-legality guard to both `see()` and
      `see_value_of()` — a king can't legally recapture into a square
      the opponent still attacks. Empirically swept 20,000 realistic
      randomized exchange chains against an independent Python model:
      ~9% (1821) produced a different final SEE result, several by
      large margins — a genuinely frequent, not corner-case, bug. One
      concrete verified position (both sides down to king-only on the
      target square) added as a regression test: `see_value_of()` was
      220, is now correctly 320; a companion test shows the boolean
      `see()` result also flips at a realistic threshold. Full
      reasoning in DECISIONS.md D122. ⚠️ Not yet CI-confirmed. **Phase
      32.1-32.6 all done.**
- [x] 32.7 (review finding #7) — DONE (Session 125, D123). Confirmed
      real at the report's exact cited location (`iterative.rs:154`,
      matched verbatim). Extracted the buggy inline expression into its
      own testable `choose_result_score()` function; dropped the wrong
      `.abs() > 0` clause, keeping only the real `!= -INFINITY` sentinel
      check. 3 new tests. Matches the report's own "Low" severity label
      — the one finding out of 8 whose framing held up exactly as
      written, unlike #3/#4/#5/#6 which all turned out different once
      independently checked. Full reasoning in DECISIONS.md D123.
      ⚠️ Not yet CI-confirmed.

**Phase 32 complete — all 8 external review findings resolved.** See
DECISIONS.md D123 for the full before/after severity table across all
8: 6 of 8 findings turned out different (better or worse) than their
original framing once independently checked against live source, and
in two cases (#5, #6) an empirical sweep was needed, not just a single
hand-derived example.
- [x] 32.8 (review finding #8) — Confirmed inert, no fix needed.
      `nnue/inference.rs`'s centipawn-scale conversion is
      self-flagged by the report as analytically derived but
      unverified against a real forward pass — verified this session
      that `NNUE_BLEND_WEIGHT_PCT` does default to 0 (pure HCE), so
      this code path has zero effect on any game played with default
      settings. Stays parked; revisit only if NNUE blend weight is
      ever considered for a non-zero default.

Report also included non-bug improvement suggestions (constants
hygiene / centralizing `MATE_THRESHOLD`-style duplication risk, a
`filter_legal()` vs `make_move.rs` parallel-implementation risk, a
packed-`Score` newtype, shared attack-bitboard computation across eval
terms, a central feature-flag registry, docs archiving for the now-
316K/356K `DECISIONS.md`/`SESSION_LOG.md`, and several testing
recommendations). None of these are bugs and none are scheduled —
noted here for reference, not queued as ROADMAP work unless Gokul asks.

---

## Phase 33 — Second External Review Round (Session 129)

Two documents this session: a focused bug report (3 findings, all
confirmed) and a separate, larger search/eval upgrade-plan review
(no bugs — parked as unscheduled backlog, see note below). Gokul's
instruction: "Bugs then upgrade."

- [x] 33.1 (bug report finding #1) — DONE (Session 129, D127; fixed
      forward twice, now CI-confirmed — Session 131, D129). Confirmed
      real: `movegen::pawns::generate_pawn_captures()` never generated
      quiet (non-capturing) pawn promotions — only
      `generate_pawn_pushes()` (full move gen) ever reached
      `add_promotions()`. This is the list `alpha_beta.rs::quiescence()`
      draws its tactical move set from, so a pawn that could walk
      straight to the queening square with no capture involved was
      invisible to qsearch — a real horizon-effect gap, not a corner
      case. D127's first fix wired the quiet-promotion generator
      directly into `generate_pawn_captures()`, which double-generated
      every quiet promotion in full move generation too (shared by
      `generate_pawn_moves()`) — caught by a real CI perft failure
      (Kiwipete depth 4). D129 fixed forward: split into a separate
      `generate_pawn_tactical()`, called only from
      `movegen::generate_captures()`. 5 regression tests total across
      both rounds. ✅ CI green (Session 131).
- [x] 33.2 (bug report finding #2) — DONE (Session 129, D127). ✅ CI
      green (Session 131). Confirmed real: `bin/match_runner.rs` and
      `bin/uci_match_runner.rs` both called `push_game_history()`
      redundantly after `make_move_with_history()` (which already
      pushes internally), double-counting every position and tripping
      `is_threefold_repetition()`'s `count >= 3` check a full
      occurrence early. `uci_match_runner.rs` feeds `build.yml`'s
      `regression-gate` CI job — this was dampening every measured Elo
      delta toward 50% in both that gate and manual A/B runs. Deleted
      the redundant line in both files. `selfplay.rs`/`texel_gen.rs`
      confirmed already correct, untouched. Report's related low-
      severity note (`selfplay.rs` not pushing ply-0 to history) left
      unaddressed — cosmetic, out of scope for "fix the bugs" as
      scoped this session.
- [x] 33.3 (bug report finding #3) — DONE (Session 129, D127; test
      suite fixed forward once — Session 130, D128 — for a flaky
      singleton race, now CI-confirmed — Session 131). Confirmed real:
      `syzygy/mod.rs` never checked castling rights before probing, at
      either the interior WDL call site (`alpha_beta.rs`) or the root
      DTZ call site (`main.rs`). Syzygy tables don't encode castling at
      all. Also corrected a directly-contradicted, unverified claim in
      `ENGINE_ARCHITECTURE.md` §5 that assumed castling rights were
      always gone by the time material reaches TB range. Added
      `has_castling_rights()` guard centralized inside `SyzygyProber`
      itself (both `probe_wdl()` and `probe_root()`), per the report's
      own suggested fix shape, so a future third call site can't
      reintroduce the gap. D127's first pass added 3 tests that raced
      on `pyrrhic_rs::TableBases`'s process-wide singleton (D17); D128
      consolidated to 1 test, net 5. ✅ CI green (Session 131).

**Phase 33 complete — all 3 bug-report findings resolved and
CI-confirmed green (Session 131), after two fix-forward rounds (D128
flaky-test fix, D129 double-generation regression fix).** Full
verification trail and reasoning in DECISIONS.md D127/D128/D129.

Upgrade-plan review (search/eval enhancement backlog: NNUE
re-validation, LMR formula enrichment, missing threat sub-terms,
razoring, and others) intentionally not started this session — parked
as an unscheduled backlog per "Bugs then upgrade." Will need its own
priority pass, and every item requires Texel-tuning/SPRT validation
before landing, per this project's existing rollout discipline — none
of it goes in blind the way a mechanical bug fix does.

---

## Phase 34 — Upgrade-Plan Backlog (from `pet-dragon-full-review.md`, D127)

Started per Gokul's "3 & 4" instruction — review items #3 (SPRT the
three experimental flags) and #4 (`ThreatByRook`), explicitly skipping
#1 (NNUE re-validation) which conflicts with tested project history
(D25/D53/D55/D57/D58/D61) — see D131, not started, needs Gokul's call.

- [x] 34.1 — SPRT infrastructure for `nonpawn_correction_enabled`,
      `continuation_correction_enabled`, `improving_enabled` — DONE,
      infra only (Session 133, D130). `match_runner.rs` extended with
      an `ExperimentalFlag` enum, CLI parsing, and a
      `resolve_weights()` safeguard that forces both engines' NNUE
      weight to 0% whenever a flag is under test, so a mobile user
      can't accidentally run a two-variable confounded match.
      `match_runner.yml` gained 3 new dropdown inputs
      (`flag_name`/`flag_a`/`flag_b`), fully mobile-usable. 9 new
      tests. ✅ CI green (Session 135). **Not yet run** — building the
      capability to test is done; actually running the three matches
      and deciding whether any flag flips its default is the next
      step, from the Actions tab.
- [x] 34.1a — `nonpawn_correction_enabled` SPRT — DONE (Session 136/137,
      D133/D134). Combined 700 games across two batches: Elo diff -11.9
      (true/on ahead), direction consistent across both independent
      batches (-17.4, then -9.7). Not a clean 2-standard-error result,
      but Gokul's explicit call (D134, same kind of judgment call as
      D125) accepted the directional lean. **Default flipped to
      `true`** — implemented across every default site
      (`search/mod.rs`, `main.rs`'s `EngineState` + advertised UCI
      option string; `uci_wasm.rs` confirmed to inherit automatically),
      5 tests fixed across 3 files that assumed the old default. ✅ CI
      green (Session 138).
- [x] 34.1b — `continuation_correction_enabled` SPRT — DONE (Session
      136, D133): 200 games, +8.7 Elo favoring `false` (off) — within
      noise (same bar as D124's own +5.2 Elo precedent). **No change**:
      default stays `false`, no further runs planned for this flag.
- [x] 34.1c — `improving_enabled` SPRT — DONE (Session 136, D133): 200
      games, +5.2 Elo favoring `false` (off) — within noise (identical
      magnitude to the D124 precedent that set this project's own bar
      for "no signal"). **No change**: default stays `false`, no
      further runs planned for this flag.

**Phase 34.1 complete — all three experimental flags resolved**
(`improving_enabled`: no change, `continuation_correction_enabled`: no
change, `nonpawn_correction_enabled`: flipped to `true`), all
CI-confirmed. Phase 34.2 (`ThreatByRook`) also fully closed (Session
135). Remaining upgrade-plan backlog: 34.3 (NNUE, parked per D131),
34.4 (LMR enrichment), 34.5 (ThreatByKing), 34.6
(WeakQueenProtection).
- [x] 34.2 — `ThreatByRook` (review §8.1) — DONE (Session 133, D130;
      fixed forward once — Session 134, D132 — for a missing Texel
      tuner mirror that broke `predict()`/`evaluate()` consistency and
      would have broken compilation of `bin/texel_diag.rs`).
      `THREAT_BY_ROOK_BONUS = s(20, 12)`, rook-attacks-queen only (not
      rook-attacks-rook — weak signal, matches Stockfish's own
      restriction). Same double-counting argument already accepted for
      `THREAT_BY_MINOR_BONUS`. 3 new regression tests. ✅ CI green
      (Session 135).
- [ ] 34.3 (review #1, NNUE re-validation) — NOT STARTED, flagged as
      conflicting with tested history, see D131. Needs Gokul's
      explicit call before any work — this is real infrastructure
      investment (D58), not a flag flip, if it's ever picked back up.
- [ ] 34.4 (review #2) — LMR reduction formula enrichment (cut-node
      status, correction magnitude signals) — not started.
- [ ] 34.5 (review #9) — `ThreatByKing` (review §8.3) — not started;
      review flags this as needing the most care (MG/EG weighting,
      avoid rewarding an exposed king), do last among the eval items.
- [ ] 34.6 (review §8.2) — `WeakQueenProtection` — not started.

---

## Phase 35 — Tactical Blindness / Blunder Investigation (2 External Reports, Session 139)

Two independent external investigation reports uploaded together:
`tactical-blindness-report.md` (5 hypotheses re-checked against source,
4 refuted, 1 confirmed as a real gap) and `blunder-bug-report.md` (a
separately confirmed, deterministically-reproduced root cause with
FEN-level repro cases). Both spot-verified against live source before
trusting, both fixed.

- [x] 35.1 — TT torn-write self-detection fixed (D135). `key` is now
      XORed with a `payload_fingerprint()` of the entry's own
      depth/bound/age/mv/score, so `probe()`/`probe_move()`/`store()`
      actually catch a torn write mixing two different writes' fields
      — the old code's comment claimed this already happened via plain
      key comparison, but `key` was an independent field, so it didn't.
      2 new tests directly simulate a torn write. ⚠️ Not yet
      CI-confirmed. Framed by its own report as a "leading suspect,"
      not conclusively the cause of any specific reported blunder —
      see 35.2, which turned out to be the actual confirmed root cause
      of the report's own reproduced cases.
- [x] 35.2 — Blunder fallback fixed (D136). Root cause:
      `iterative_deepening()`'s end-of-function safety net picked an
      unevaluated, arbitrary legal move whenever depth 1 never
      completed before a stop landed — which happens on every move at
      `movetime <= 30` (the default Move Overhead reduces the
      allocated budget to 0ms). Two complementary fixes: (1)
      `is_time_up()`'s elapsed-time check no longer fires while
      `current_depth <= 1`, guaranteeing depth 1 always gets to run;
      (2) the last-resort fallback itself now uses `score_moves()`
      instead of `moves.get(0)`. 3 fix-forward test corrections needed
      (2 in `alpha_beta.rs`, scoping confirmed sufficient for 1 in
      `search/mod.rs`). 4 new tests, including 3 end-to-end
      reproductions of the report's exact FEN cases through the real
      `iterative_deepening()` entry point. ⚠️ Not yet CI-confirmed.

---

## Milestone Targets

| Milestone | Target Elo | Phase |
|-----------|-----------|-------|
| Material only (current) | ~1200 | Phase 7 done |
| HCE complete | ~2400-2600 | Phase 8 done |
| Search improvements | ~2800-2900 | Phase 13 done |
| Texel tuned HCE | ~3000-3100 (relative estimate — Pet Dragon is a custom variant with no external rating pool to calibrate against, so this figure is comparative, not a calibrated absolute) | Phase 14 done, 17.8/D36 closed (Session 59-60): 520-game pooled pinned-ref UCI match (2 of those runs at 200 games each, tightly consistent) shows tuned HCE ~39 Elo stronger than pre-tuning Ethereal values. Real, well-powered, and modest — nowhere near the scale that "~3000-3100" implies on its own; treat that number as a rough historical target, not a validated one. |
| NORU NNUE | ~3400-3600 | Phase 16 done |
