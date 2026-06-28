# PROJECT_CONTEXT.md
# Pet Dragon Chess Engine

## Purpose
Pet Dragon is the world's first original chess variant engine built natively
in Rust from scratch. Not a fork. Not a port. Purpose-built for the Pet Dragon
variant from day one.

**Copyright © Gokul Chandar. All rights reserved.**
Licensed under GPL v3. Contributors: Claude (Anthropic).
GitHub: https://github.com/g-c-3/pet-dragon
Live: https://g-c-3.github.io/pet-dragon

---

## Pet Dragon Variant Rules (Summary)
- White King always starts on e1. Black King always on e8.
- Remaining 15 White pieces randomly placed across ranks 1–2.
  Bishops must land on opposite colour squares.
- Black mirrors White exactly (rank1↔rank8, rank2↔rank7, file preserved).
- **Pawns**: always move toward opponent's back rank regardless of start square.
  Double-step available ONLY from actual starting square (rank 1 OR rank 2
  for White; rank 7 OR rank 8 for Black).
- **Castling**: only if Rook happened to land on a1/h1 (White) or a8/h8 (Black).
- Everything else: standard FIDE chess.

---

## Current Status
**Phase 8 — Handcrafted Evaluation (HCE) — IN PROGRESS**

- Phases 0–7 complete and green (239 tests passing, Build #86 ✅)
- Phase 8 started: `src/eval/material.rs` and `src/eval/mod.rs` provided,
  may or may not be uploaded yet — check GitHub to confirm before continuing.

---

## Tech Stack
- **Language**: Rust stable, edition 2021
- **Build**: GitHub Actions (cargo test + cargo build --release)
- **Deploy**: wasm-pack → GitHub Pages
- **Board**: Bitboards + Magic bitboards
- **Search**: Alpha-beta + PVS, iterative deepening, aspiration windows
- **Eval**: HCE (Ethereal weights) → Texel tuning → NORU NNUE (Phase 16)
- **Protocol**: UCI for GUI compatibility
- **Crate**: `pet_dragon` / `pet_dragon_lib`

---

## Folder Structure
```
pet-dragon/
├── src/
│   ├── lib.rs                  # Library root, WASM entry point
│   ├── main.rs                 # Native binary entry point (Phase 9: full UCI)
│   ├── types.rs                # All core types (Square, Move, Color, etc.)
│   ├── bitboard/
│   │   ├── mod.rs              # Bitboard type, ops, shifts, constants
│   │   ├── masks.rs            # Precomputed attack tables, init_masks()
│   │   └── magic.rs            # Magic bitboards, init_magic()
│   ├── position/
│   │   ├── mod.rs              # Position struct, check detection, repetition
│   │   ├── fen.rs              # FEN parser + generator (7-field Pet Dragon ext)
│   │   ├── zobrist.rs          # Zobrist hashing, init_zobrist()
│   │   ├── setup.rs            # Pet Dragon position generator
│   │   └── make_move.rs        # Full make/unmake, make_move_with_history()
│   ├── movegen/
│   │   ├── mod.rs              # MoveList, generate_moves(), generate_captures()
│   │   ├── pieces.rs           # Knight/Bishop/Rook/Queen/King moves
│   │   ├── pawns.rs            # Pet Dragon custom pawn logic
│   │   ├── castling.rs         # Dynamic castling from setup rights
│   │   └── legal.rs            # Legal move filter
│   ├── tt/
│   │   └── mod.rs              # Transposition table (lock-free, age-based)
│   ├── search/
│   │   ├── mod.rs              # SearchInfo, SearchResult, constants
│   │   ├── alpha_beta.rs       # Alpha-beta + PVS, quiescence, evaluate()
│   │   ├── iterative.rs        # Iterative deepening + aspiration windows
│   │   ├── ordering.rs         # Move ordering (TT, SEE, killers, history)
│   │   ├── time.rs             # Time management, TimeControl, TimeManager
│   │   ├── see.rs              # Static Exchange Evaluation
│   │   └── pruning.rs          # Extensions, LMR guards, probcut, correction history
│   └── eval/                   # ← PHASE 8 IN PROGRESS
│       ├── mod.rs              # evaluate() combining all terms
│       ├── material.rs         # ✅ Tapered material values (Ethereal weights)
│       ├── tables.rs           # 🔄 Piece-square tables (next to upload)
│       ├── mobility.rs         # ❌ Not yet written
│       ├── pawns.rs            # ❌ Not yet written
│       ├── king_safety.rs      # ❌ Not yet written
│       └── open_lines.rs       # ❌ Not yet written (Pet Dragon critical)
├── tests/
│   ├── perft.rs                # Perft depth 5 = 4,865,609 ✅ proven correct
│   ├── setup.rs                # 1000 Pet Dragon position validation
│   └── make_unmake.rs          # Make/unmake + repetition detection
├── docs/                       # ← THIS DIRECTORY
│   ├── PROJECT_CONTEXT.md
│   ├── VARIANT_ARCHITECTURE.md
│   ├── ROADMAP.md
│   ├── DECISIONS.md
│   ├── SESSION_LOG.md
│   └── ENGINE_ARCHITECTURE.md
├── Cargo.toml
├── LICENSE                     # GPL v3
└── .github/workflows/
    ├── build.yml               # Test + release binaries
    └── deploy.yml              # GitHub Pages
```

---

## Key Design Decisions (summary — see DECISIONS.md for full rationale)
- Pure Rust, no forks — Pet Dragon pawn logic can't cleanly patch existing engines
- Per-pawn start map (`PawnStartMap`) — tracks actual starting square per pawn
- Dynamic castling rights — set at game start from Rook positions
- Borrowed Ethereal + Stockfish weights (GPL v3) for HCE — no tuning compute needed
- No opening suppression in eval — Pet Dragon has no quiet opening
- King safety without castling bias — ~74% of games have no castling
- Single Pet Dragon NNUE (Phase 16) — no dual-network architecture
- Lock-free TT with benign races — Stockfish approach

---

## Known Bugs / Edge Cases
- `move_gives_check()` in `alpha_beta.rs` had King-not-found panic in minimal
  positions — fixed with `piece_bb(...).is_empty()` guard (Build #86)
- `should_start_next_depth()` in `time.rs` had u64 overflow when time is
  `u64::MAX/2` — fixed with overflow guard (Build #80)
- SEE even-exchange test needed correct FEN (Black Rook recapturer) — fixed
- Probcut and CorrectionHistory are defined in `pruning.rs` but NOT YET WIRED
  into search — intentional, scheduled for Phase 13

---

## Current Sprint: Phase 8 — HCE
**Files to create (in order):**
1. `src/eval/material.rs` — ✅ provided to Gokul (verify upload)
2. `src/eval/mod.rs` — ✅ provided to Gokul (verify upload)
3. `src/eval/tables.rs` — 🔄 next to provide
4. `src/eval/mobility.rs`
5. `src/eval/pawns.rs`
6. `src/eval/king_safety.rs`
7. `src/eval/open_lines.rs`
8. Final `src/eval/mod.rs` with full `evaluate()` function
9. Wire `evaluate()` into `src/search/alpha_beta.rs`

**After Phase 8:** Phase 9 — Full UCI protocol in `src/main.rs`

---

## Important Constraints
- Gokul uses mobile only — no terminal, no desktop
- All building via GitHub Actions — Gokul only uploads files
- Every file must be complete and ready to copy-paste
- `init_masks()` → `init_magic()` → `init_zobrist()` must be called at startup
- Tests must stay green before any new commit
- src/lib.rs must declare: `pub mod eval;` (check if done)
