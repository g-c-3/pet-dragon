# VARIANT_ARCHITECTURE.md 
# Pet Dragon Variant — Technical Architecture

## The Variant Rules (Canonical)

### Starting Position Generation
1. White King fixed on e1 (always)
2. Remaining 15 White pieces (Q, 2R, 2B, 2N, 8P) randomly placed on ranks 1–2
   - Bishops MUST land on opposite colour squares (enforced during placement)
   - Fisher-Yates shuffle of available squares
3. Black mirrors White exactly:
   - rank 1 → rank 8, rank 2 → rank 7
   - Same file, same piece type
4. Per-pawn starting squares recorded in `PawnStartMap`
5. Castling rights set ONLY if Rooks landed on a1/h1 (White) or a8/h8 (Black)
6. Standard chess starting position is one valid Pet Dragon arrangement

### Pawn Rules (Critical Differences from Standard Chess)
```
Standard chess:
  White pawn on rank 2 → can double-step to rank 4
  Black pawn on rank 7 → can double-step to rank 5

Pet Dragon:
  White pawn on rank 1 → can double-step to rank 3 (FIRST MOVE ONLY)
  White pawn on rank 2 → can double-step to rank 4 (FIRST MOVE ONLY)
  Black pawn on rank 7 → can double-step to rank 5 (FIRST MOVE ONLY)
  Black pawn on rank 8 → can double-step to rank 6 (FIRST MOVE ONLY)

  "First move only" = pawn is still on its ACTUAL STARTING SQUARE
  Tracked via PawnStartMap — never changes during game
```

### Key Edge Case: Rank 1 Pawn → Rank 2
```
White pawn starts rank 1, moves to rank 2 (single step):
  - It is now on rank 2 — looks like standard chess pawn
  - BUT it CANNOT double-step (it has already moved)
  - pawn_starts.started_here(rank2_sq, White) = FALSE
  - → Correctly no double-step available
  - This is different from a pawn that STARTED on rank 2
```

### NNUE Feature Implication
```
Pawn start features active ONLY while pawn on starting square.
Rank 1 pawn → rank 2: start feature = 0 (already moved)
Rank 2 pawn → rank 2: start feature = 1 (still on start)
These have DIFFERENT feature encodings → different evaluation
Network correctly distinguishes "can double-step" vs "cannot"
```

### Castling
```
Available ONLY if Rook started on standard square.
White King is ALWAYS on e1, so only Rook positions matter.

Probability ~26% of games have any castling available.
~74% of games have NO castling at all.

IMPORTANT FOR EVALUATION:
King safety must NOT rely on castling status.
Pawn shield + piece proximity only.
No "has castled" bonus.
```

### Everything Else
Identical to standard FIDE chess:
- Check, checkmate, stalemate
- En passant (follows from double-step rules)
- Promotion (White on rank 8, Black on rank 1)
- 50-move rule
- Threefold repetition
- Insufficient material
- All piece movement rules

---

## PawnStartMap Implementation

```rust
// In src/types.rs
pub struct PawnStartMap(pub [Option<Color>; 64]);

impl PawnStartMap {
    // Set at game start — never modified during play
    pub fn set(&mut self, square: Square, color: Color)

    // Called by move generator to check double-step eligibility
    pub fn started_here(&self, square: Square, color: Color) -> bool
}
```

**Key properties:**
- Never changes during `make_move()` or `unmake_move()`
- Included in Zobrist hash via `PAWN_START_KEYS[color][sq]`
- Serialised in FEN as 7th field: `"e1:w,d2:w,..."` (Pet Dragon extension)
- When loading standard FEN (no 7th field), inferred from current pawn positions

---

## Double-Step Move Generation

```rust
// In src/movegen/pawns.rs - generate_pawn_pushes()

// ONLY check double-step if pawn is on its actual starting square
if pos.pawn_starts.started_here(from, color) {
    let to_double = match color {
        Color::White => Square::from_file_rank(from.file(), from.rank() + 2),
        Color::Black => Square::from_file_rank(from.file(), from.rank() - 2),
    };
    // Also requires intermediate square to be empty (standard chess rule)
}
```

---

## Evaluation Special Cases

### Open Lines from Move 1
Pet Dragon starts with all pieces on ranks 1–2 (White) and 7–8 (Black).
Ranks 3–6 are completely empty. Open file/diagonal detection must work
at depth 0 — there is no "closed opening" phase.

```
Battery detection applies immediately:
  Queen on d1, Rook on d2 → vertical battery on file d
  Bishop on c1 (diagonal) → diagonal tension immediately
  Rook facing Rook across open board → contested file
```

### No Opening Suppression
Standard engines reduce PST/mobility weight in opening to avoid
"played too aggressively before development." Pet Dragon has no such phase —
full eval weight from move 1.

### Rank 1 Pawn Eval
A White pawn on rank 1 mid-game is still on its starting square.
- NEVER penalise as backward
- Pawn forward span correctly handles rank 1 starts
- Passed pawn detection works from rank 1

### King Safety
```
Standard engine: castled King gets safety bonus
Pet Dragon: 74% of games have no castling → no castling bias

King safety = pawn shield (pawns on adjacent squares/files)
            + piece proximity (own pieces nearby)
            + attacker count (enemy pieces near king)
            + open/semi-open files through king
```

---

## Zobrist Hash (Pet Dragon Extension)

Standard Zobrist hash components:
- Piece-square keys (768 values: 6 pieces × 2 colors × 64 squares)
- Side to move (1 value)
- Castling rights (16 values for 4-bit mask)
- En passant file (8 values)

**Pet Dragon addition:**
- Pawn start square keys (128 values: 64 squares × 2 colors)
- XOR'd in for each pawn's actual starting square at position creation
- Ensures two positions with identical piece placement but different
  pawn start configurations get different hashes
- Critical for TT correctness across different Pet Dragon games

---

## NNUE Design (Phase 16)

### Feature Set: 896 inputs
- 768 standard piece-square features (HalfKP-style)
- 128 pawn start square features (64 squares × 2 colors)

### Phase Convergence
```
Opening:   All 896 features active. Network uses Pet Dragon specific
           knowledge (rank 1 pawn dynamics, random piece arrangements).

Middlegame: Pawn start features → 0 as pawns leave starting squares.
            Network evaluates using standard piece-square patterns.

Endgame:   All pawn start features = 0. Evaluation converges to
           standard chess NNUE behaviour for equivalent material.
```

### Training Data Strategy
- Phase 1: Pet Dragon self-play (engine vs engine from all starting positions)
- Phase 2: Supplement with standard chess positions (Lichess CC0 dataset)
- Single network learns both: Pet Dragon specific + standard chess patterns
- No dual-network architecture needed

### Why NOT Stockfish NNUE
Even when all pawns have passed rank 2/7, Pet Dragon piece arrangements
differ from standard chess training data. A Rook that started on b2 and
moved to b4 is alien to Stockfish NNUE. Our Pet Dragon NNUE handles
all arrangements correctly without switching.
