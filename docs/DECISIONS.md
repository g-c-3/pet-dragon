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
