# ROADMAP.md
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

---

## Phase 8 — Handcrafted Evaluation (HCE) 🔄 IN PROGRESS
- [~] 8.1 — src/eval/material.rs — Tapered material values (Ethereal weights)
            s(mg,eg) packed score, taper(), game_phase()
            ⚠️ PROVIDED TO GOKUL — verify upload before continuing
- [~] 8.2 — src/eval/mod.rs — Module declarations (stub)
            ⚠️ PROVIDED TO GOKUL — verify upload before continuing
- [~] 8.3 — src/eval/tables.rs — Piece-square tables (PST) MG+EG
            Knight centre bonus, King endgame centralisation, Rook 7th rank
            ️⚠️ PROVIDED TO GOKUL — verify upload before continuing
- [ ] 8.4 — src/eval/mobility.rs — Mobility bonus per piece type
            Bonus for each legal move (from Ethereal weights)
- [ ] 8.5 — src/eval/pawns.rs — Pawn structure evaluation
            Passed / isolated / doubled / backward pawns
            ⚠️ Rank 1 pawns NEVER penalised as backward (Pet Dragon rule)
- [ ] 8.6 — src/eval/king_safety.rs — King safety evaluation
            Pawn shield, open files near king, attack count
            ⚠️ NO castling bonus — 74% of games have no castling
- [ ] 8.7 — src/eval/open_lines.rs — Open file/diagonal evaluation
            ⚠️ Pet Dragon CRITICAL — open lines exist from move 1
            Rook on open file, battery detection, contested files,
            Rook on 7th rank, connected rooks, Queen on open file
- [ ] 8.8 — src/eval/mod.rs FINAL — Full evaluate() combining all terms
            Tapered blend, correction history, tempo bonus
            No opening suppression — all terms at full weight from move 1
- [ ] 8.9 — Wire evaluate() into src/search/alpha_beta.rs
            Replace placeholder with crate::eval::evaluate(pos)
- [ ] 8.10 — src/material.rs duplicate — remove if exists (was a stray file)

---

## Phase 9 — UCI Protocol ⏳
- [ ] 9.1 — Full UCI command loop in src/main.rs
            uci, isready, ucinewgame, position, go, stop, quit
- [ ] 9.2 — position command: parse startpos / fen + moves
- [ ] 9.3 — go command: parse wtime/btime/winc/binc/movestogo/movetime/depth
- [ ] 9.4 — UCI options: Hash (TT size), Threads (future SMP)
- [ ] 9.5 — bestmove output after search completes
- [ ] 9.6 — info strings during search (already formatted in SearchResult)

---

## Phase 10 — GitHub Actions Release Pipeline ⏳
- [ ] 10.1 — Build release binaries for Windows/macOS/Linux in build.yml
- [ ] 10.2 — GitHub Releases page with download links
- [ ] 10.3 — Verify binaries work with Arena, BanksiaGUI, CuteChess

---

## Phase 11 — WebAssembly Build ⏳
- [ ] 11.1 — wasm-pack build --target web --release
- [ ] 11.2 — wasm-bindgen exports: engine_name(), engine_author(), search()
- [ ] 11.3 — WASM feature flag gates all browser-specific code
- [ ] 11.4 — getrandom/js feature for Pet Dragon position generation in browser

---

## Phase 12 — Browser Game ⏳
- [ ] 12.1 — web/index.html — chessboard UI
- [ ] 12.2 — Integrate Stockfish WASM for opponent
- [ ] 12.3 — Pet Dragon position display (rank 1/2 pieces correctly shown)
- [ ] 12.4 — Game controls: new game, undo, set time
- [ ] 12.5 — Deploy via GitHub Pages (deploy.yml already set up)

---

## Phase 13 — Search Improvements ⏳
- [ ] 13.1 — Wire Probcut into alpha_beta.rs (defined in pruning.rs)
- [ ] 13.2 — Wire CorrectionHistory into eval (defined in pruning.rs)
- [ ] 13.3 — Singular extensions
- [ ] 13.4 — Lazy SMP (multi-threaded parallel search)
- [ ] 13.5 — Improve quiescence search (better move ordering, checks in qsearch)
- [ ] 13.6 — History gravity and continuation history
- [ ] 13.7 — Node count benchmarking vs known engines

---

## Phase 14 — Texel Tuning (Optional) ⏳
- [ ] 14.1 — OPTIONAL PHASE — skip if going straight to NNUE (Phase 16)
            Texel tuning improves HCE quality and therefore NNUE training data
            quality, but borrowed weights are sufficient for initial NNUE training.
            Decide after Phase 13 is complete.
            colab/texel_tuning.ipynb — optimise HCE weights via gradient descent
- [ ] 14.2 — Generate Pet Dragon game database for tuning
- [ ] 14.3 — Run tuning on Google Colab (free GPU)
- [ ] 14.4 — Update weights in eval/ files with tuned values

---

## Phase 15 — Syzygy Tablebases ⏳
- [ ] 15.1 — Integrate Syzygy probe via pyrrhic-rs or own implementation
- [ ] 15.2 — UCI SyzygyPath option
- [ ] 15.3 — Probe in search when <= 7 pieces on board
- [ ] 15.4 — WDL (Win/Draw/Loss) probing for eval
- [ ] 15.5 — DTZ (Distance to Zero) probing for perfect endgame play

---

## Phase 16 — NORU NNUE (Optional Enhancement) ⏳
- [ ] 16.1 — Add NORU crate to Cargo.toml
- [ ] 16.2 — Define 896-input feature set:
            768 standard piece-square + 128 pawn start square features
- [ ] 16.3 — Feature extraction: update incrementally on make/unmake
- [ ] 16.4a — Training data: Pet Dragon self-play (engine vs engine)
- [ ] 16.4b — Training data strategy: include standard chess positions
             (Lichess CC0 dataset) alongside Pet Dragon self-play.
             Single network learns both simultaneously.
- [ ] 16.4c — Pawn start feature convergence design:
             Features become 0 as pawns leave starting squares.
             Network naturally transitions to standard-chess-like eval
             in middlegame/endgame without switching logic.
             See DECISIONS.md for full rationale.
- [ ] 16.5 — Train network using NORU's built-in trainer on Google Colab
- [ ] 16.6 — Integrate trained network into eval (replace HCE or blend)
- [ ] 16.7 — WASM-compatible inference (NORU is pure Rust)

---

## Test Coverage Summary
| Test File          | Count | Status |
|--------------------|-------|--------|
| src/types.rs       | 14    | ✅     |
| src/bitboard/      | 42    | ✅     |
| src/position/      | 60+   | ✅     |
| src/movegen/       | 40+   | ✅     |
| src/tt/            | 14    | ✅     |
| src/search/        | 40+   | ✅     |
| tests/perft.rs     | 18    | ✅     |
| tests/setup.rs     | 18    | ✅     |
| tests/make_unmake.rs | 19  | ✅     |
| **TOTAL**          | **239** | ✅   |

---

## Milestone Targets
| Milestone | Target Elo | Phase |
|-----------|-----------|-------|
| Material only (current) | ~1200 | Phase 7 done |
| HCE complete | ~2400-2600 | Phase 8 done |
| Search improvements | ~2800-2900 | Phase 13 done |
| Texel tuned HCE | ~3000-3100 | Phase 14 done |
| NORU NNUE | ~3400-3600 | Phase 16 done |
