# ENGINE_ARCHITECTURE.md
# Pet Dragon — Engine Architecture

## Startup Sequence (MANDATORY)
Every binary, test, and benchmark must call these three in order:
```rust
pet_dragon_lib::bitboard::masks::init_masks();   // precompute attack tables
pet_dragon_lib::bitboard::magic::init_magic();   // precompute magic bitboards
pet_dragon_lib::position::zobrist::init_zobrist(); // generate Zobrist keys
```
Calling any move generation or position function before init is undefined behaviour.

---

## Data Flow: Position → Search → Move

```
UCI "go" command
    │
    ▼
TimeControl::from_uci(...)
    │
    ▼
iterative_deepening(pos, tc, info, tt)
    │
    ├── depth 1: alpha_beta(pos, 1, -INF, +INF, 0, true, info, tt, NULL)
    ├── depth 2: alpha_beta(pos, 2, prev-δ, prev+δ, 0, true, info, tt, NULL)
    ├── depth 3: ...
    │
    │   Inside alpha_beta:
    │   ├── is_time_up() → return 0
    │   ├── depth==0 → quiescence(pos, alpha, beta, ply, info, tt)
    │   ├── is_repetition() → return DRAW_SCORE
    │   ├── halfmove_clock >= 100 → return DRAW_SCORE
    │   ├── tt.probe(pos.hash) → TT hit → maybe cutoff
    │   ├── in_check() → check extension (depth += 1)
    │   ├── null_move_pruning()
    │   ├── generate_moves(pos)
    │   ├── score_moves(pos, moves, info, tt_move, ply, prev_move)
    │   └── for each move (best first via next_move()):
    │       ├── futility_pruning → skip
    │       ├── see_pruning → skip
    │       ├── pos.make_move_with_history(mv)
    │       ├── -alpha_beta(pos, depth-1, -beta, -alpha, ply+1, ...)
    │       ├── pos.unmake_move_with_history(mv)
    │       └── update best, alpha, bound
    │
    └── return SearchResult { best_move, score, depth, ... }
```

---

## Core Structs

### Position (src/position/mod.rs)
```rust
pub struct Position {
    pieces:          [[Bitboard; 6]; 2],  // [color][piece_kind]
    occupied_by:     [Bitboard; 2],       // all squares by color
    all_occupied:    Bitboard,            // all occupied squares
    side_to_move:    Color,
    castling:        CastlingRights,
    en_passant:      Option<Square>,
    halfmove_clock:  u32,
    fullmove_number: u32,
    hash:            u64,                 // Zobrist hash (incremental)
    pawn_starts:     PawnStartMap,        // Pet Dragon: never changes during play
    history:         Vec<HistoryEntry>,   // for unmake_move()
    game_history:    Vec<u64>,            // for repetition detection
}
```

### SearchInfo (src/search/mod.rs)
```rust
pub struct SearchInfo {
    time_allocated_ms: u64,
    start_time:        Instant,
    stop:              bool,
    nodes:             u64,
    nps:               u64,
    killers:           [[Move; 2]; 128],     // [ply][slot]
    history:           [[[i32; 64]; 64]; 2], // [color][from][to]
    countermoves:      [[Move; 64]; 64],     // [from][to]
    pv_length:         [usize; 128],
    pv_table:          [[Move; 128]; 128],
    best_move:         Move,
    best_score:        i32,
    seldepth:          usize,
}
```

### TTEntry (src/tt/mod.rs)
```rust
pub struct TTEntry {
    key:   u32,   // upper 32 bits of Zobrist hash (verification)
    depth: i8,    // search depth this result came from
    bound: Bound, // Exact / LowerBound / UpperBound
    age:   u8,    // search generation
    mv:    Move,  // best move found
    score: i32,   // evaluation score (mate-adjusted)
}
```

---

## Score System
```
Scores in centipawns from side-to-move perspective.
Positive = good for side to move. Negative = good for opponent.

INFINITY     = 1_000_000   (initial alpha/beta bounds)
DRAW_SCORE   = 0
MATE_SCORE   = 999_999     (mate in 0 = 999,999 centipawns)
MATE_THRESHOLD = 900_000   (any score above this is a forced mate)

Mate in N: score = MATE_SCORE - (2*N - 1) for mating side
           score = -(MATE_SCORE - 2*N) for being mated

TT mate score adjustment:
  score_to_tt(score, ply)   = score + ply  (if score >= MATE_THRESHOLD)
  score_from_tt(score, ply) = score - ply  (if score >= MATE_THRESHOLD)
  This converts between root-relative and ply-relative mate scores.
```

---

## Move Representation (src/types.rs)
```rust
pub struct Move {
    pub from:     Square,
    pub to:       Square,
    pub kind:     MoveKind,
    pub captured: Option<PieceKind>,  // for fast unmake
}

pub enum MoveKind {
    Quiet, DoublePush, Capture, EnPassant,
    CastleKing, CastleQueen,
    PromoQueen, PromoRook, PromoBishop, PromoKnight,
    PromoCapQueen, PromoCapRook, PromoCapBishop, PromoCapKnight,
}

// NULL move: from == to == A1, used as sentinel in search
pub const NULL: Move = Move { from: A1, to: A1, kind: Quiet, captured: None };
```

---

## Move Generation Pipeline
```
generate_moves(pos)
    │
    ├── generate_pseudo_legal(pos, &mut list)
    │   ├── pawns::generate_pawn_moves(pos, color, &mut list)
    │   │   ├── generate_pawn_pushes()     ← Pet Dragon: double-step from start sq
    │   │   └── generate_pawn_captures()   ← diagonal captures + en passant
    │   ├── pieces::generate_piece_moves(pos, color, &mut list)
    │   │   ├── knights (KNIGHT_ATTACKS table lookup)
    │   │   ├── bishops (bishop_attacks() magic)
    │   │   ├── rooks   (rook_attacks() magic)
    │   │   ├── queens  (queen_attacks() = rook | bishop)
    │   │   └── king    (KING_ATTACKS table lookup)
    │   └── castling::generate_castling_moves(pos, color, &mut list)
    │       └── only if rights set AND path clear AND not through check
    │
    └── legal::filter_legal(pos, pseudo)
        └── for each move: clone pos, apply_move, !in_check(color) → keep
```

---

## Bitboard Attack Generation

### Knight and King (O(1) table lookup)
```rust
pub fn knight_attacks(sq: Square) -> Bitboard {
    unsafe { KNIGHT_ATTACKS[sq.index() as usize] }
}
```

### Sliding Pieces (magic bitboards)
```rust
pub fn rook_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    unsafe {
        let entry = &ROOK_MAGICS[sq.index() as usize];
        let index = (occupancy.0 & entry.mask)
            .wrapping_mul(entry.magic) >> entry.shift;
        ROOK_ATTACKS[entry.offset + index as usize]
    }
}
```

### Pawn Attacks
```rust
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    unsafe { PAWN_ATTACKS[color as usize][sq.index() as usize] }
}
// White: shift_north_east | shift_north_west
// Black: shift_south_east | shift_south_west
```

---

## Zobrist Hashing

### Keys (src/position/zobrist.rs)
```
PIECE_KEYS[2][6][64]     — piece × color × square
SIDE_KEY                 — XOR when Black to move
CASTLING_KEYS[16]        — one per castling rights bitmask
EN_PASSANT_KEYS[8]       — one per file (only when EP is possible)
PAWN_START_KEYS[2][64]   — Pet Dragon: pawn × color × starting square
```

### Incremental Update in make_move()
```
1. XOR out old castling key
2. XOR out old EP key (if any)
3. XOR out side key (if Black just moved)
4. Apply the move (update pieces)
5. XOR in new castling key
6. XOR in new EP key (if double push just occurred)
7. XOR in new side key (now other side to move)
```
Unmake: restore hash from HistoryEntry.hash (no recalculation needed).

---

## Search Techniques (Phase 7)

### Alpha-Beta with PVS
```
First move: full window [-beta, -alpha]
Remaining:  null window [-alpha-1, -alpha]
  If beats alpha: re-search full window (LMR first, then full if needed)
```

### Null Move Pruning
```
Conditions: !pv_node, !in_check, depth >= 3, static_eval >= beta,
            has non-pawn material (zugzwang guard), prev_move != NULL
Reduction:  R = 3 + depth/6 (adaptive)
```

### Late Move Reductions (LMR)
```
Conditions: depth >= 3, moves_tried >= 3, quiet move, !in_check, !gives_check
Formula:    reduction = floor(0.75 + ln(depth) × ln(moves_tried) / 2.25)
Min/max:    1 to depth-1
```

### Move Ordering (descending priority)
```
1. TT move                    (score: 2,000,000)
2. Winning captures by SEE    (score: 1,000,000 + MVV-LVA)
3. Equal captures             (score: 500,000 + MVV-LVA)
4. Promotions (quiet)         (score: 1,400,000 + promo piece value)
5. En passant                 (score: 500,000)
6. Killer move 1              (score: 400,000)
7. Killer move 2              (score: 300,000)
8. Countermove                (score: 200,000)
9. Quiet moves by history     (score: 0 + history[color][from][to])
   + Pet Dragon rank-1 bonus  (score: +50,000 if double-push from rank 1)
10. Losing captures           (score: -500,000 + SEE value)
```

### SEE (Static Exchange Evaluation)
```
SWAP algorithm:
  1. Record initial gain (value of captured piece)
  2. Find least valuable attacker for each side alternately
  3. Build gain[] array: gain[d] = value(piece_just_captured) - gain[d-1]
  4. Negamax backwards: gain[d-1] = max(-gain[d-1], gain[d])
  5. Return gain[0] >= threshold
```

---

## Evaluation (Phase 8 — IN PROGRESS)

### Current placeholder (src/search/alpha_beta.rs)
```rust
pub fn evaluate(pos: &Position) -> i32 {
    pos.material(pos.side_to_move) - pos.material(pos.side_to_move.flip())
}
```

### Target structure (src/eval/mod.rs — to be built)
```rust
pub fn evaluate(pos: &Position) -> i32 {
    let phase = game_phase(pos);
    let score = 0i64
        + evaluate_material(pos, phase)   // material.rs
        + evaluate_tables(pos, phase)     // tables.rs (PST)
        + evaluate_mobility(pos, phase)   // mobility.rs
        + evaluate_pawns(pos, phase)      // pawns.rs
        + evaluate_king_safety(pos, phase) // king_safety.rs
        + evaluate_open_lines(pos, phase); // open_lines.rs
    // taper() already applied within each component
    // Add tempo bonus
    let tempo = 10; // centipawns for side to move advantage
    score as i32 + tempo
}
```

### Tapered Evaluation
```
phase = sum(piece_count × phase_weight) for all pieces
      = range 0 (pure endgame) to 24 (full middlegame)

score = (mg_score × phase + eg_score × (24 - phase)) / 24

Packed score trick (from Ethereal):
  s(mg, eg) = ((mg as i64) << 32) + (eg as i64)
  mg(score) = (score >> 32) as i32
  eg(score) = score as i32
  taper(score, phase) = (mg(score) × phase + eg(score) × (24-phase)) / 24
```

---

## Key Function Signatures

```rust
// Move generation
pub fn generate_moves(pos: &Position) -> MoveList
pub fn generate_captures(pos: &Position) -> MoveList

// Search entry point
pub fn iterative_deepening(
    pos:  &mut Position,
    tc:   &TimeControl,
    info: &mut SearchInfo,
    tt:   &mut TranspositionTable,
) -> SearchResult

// Core search
pub fn alpha_beta(
    pos:       &mut Position,
    depth:     i32,
    alpha:     i32,
    beta:      i32,
    ply:       usize,
    pv_node:   bool,
    info:      &mut SearchInfo,
    tt:        &mut TranspositionTable,
    prev_move: Move,
) -> i32

// Transposition table
pub fn store(&mut self, hash: u64, depth: i8, score: i32, bound: Bound, mv: Move)
pub fn probe(&self, hash: u64) -> Option<TTEntry>
pub fn score_to_tt(score: i32, ply: i32) -> i32
pub fn score_from_tt(score: i32, ply: i32) -> i32
```

---

## Phase 8 Eval Files — Pet Dragon Notes

### material.rs ✅
- Phase weights: P=0, N=1, B=1, R=2, Q=4, K=0 → max phase = 24
- Bishop pair bonus: +22 MG / +30 EG

### tables.rs (next)
- Mirror rank for Black: `idx = (7 - rank) * 8 + file`
- King MG table: NO castling bonus — small values across all squares
- Rook table: bonus for 7th rank, open file squares

### mobility.rs
- Count pseudo-legal moves per piece type
- Weight by phase and piece type
- Exclude squares attacked by enemy pawns

### pawns.rs
- ⚠️ Rank 1 pawns NEVER backward — they're on start squares
- Passed pawn detection: no enemy pawns on same/adjacent files ahead
- Pet Dragon: "ahead" for rank 1 pawn = ranks 2–8

### king_safety.rs
- ⚠️ NO castling bonus
- Pawn shield: count own pawns within 1-2 squares of king
- Open file penalty: enemy Rooks/Queens on same file as king

### open_lines.rs (Pet Dragon critical)
- ⚠️ Applies from position 0 — ranks 3–6 empty at game start
- Open file: no pawns of either color on file
- Semi-open file: no own pawns on file
- Battery: Queen+Rook same file OR Queen+Bishop same diagonal
- Contested file: both sides have heavy pieces on file
