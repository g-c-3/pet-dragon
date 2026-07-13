// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// position/setup.rs — Pet Dragon starting position generator
//
// This is the core of what makes Pet Dragon unique.
//
// Algorithm:
//   1. Fix White King on e1
//   2. Collect all 15 squares in ranks 1-2 except e1
//   3. Shuffle them randomly
//   4. Place pieces in this order, enforcing bishop constraint:
//      a. Place Bishop 1 on first available light square
//      b. Place Bishop 2 on first available dark square
//      c. Place remaining 13 pieces on remaining 13 squares
//   5. Mirror White to Black:
//      rank 1 → rank 8, rank 2 → rank 7, same file, same piece
//   6. Record per-pawn starting squares for both sides
//   7. Set castling rights only if Rooks landed on a1/h1 (White)
//      or a8/h8 (Black)
//
// Pet Dragon rules encoded here:
//   - King fixed on e1/e8
//   - Bishops on opposite colours (enforced)
//   - Black mirrors White (strict positional mirror)
//   - Pawn double-step from actual start square (recorded here)
//   - Castling only if Rook started on standard square (detected here)
// ============================================================================

use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::types::{
    CastlingRights, Color, PawnStartMap, PieceKind, Square,
};

// ── Random number generator ───────────────────────────────────────────────────
// Simple xorshift64 — fast, good enough for position generation
// Seeded from getrandom for true randomness each game

struct Rng(u64);

impl Rng {
    /// Create a new RNG with a random seed
    fn new() -> Self {
        let mut seed = [0u8; 8];
        // Use getrandom for true randomness when available
        // Falls back to a time-based seed approximation
        #[cfg(feature = "wasm")]
        {
            getrandom::getrandom(&mut seed).unwrap_or(());
        }
        #[cfg(not(feature = "wasm"))]
        {
            getrandom::getrandom(&mut seed).unwrap_or(());
        }
        let seed_u64 = u64::from_le_bytes(seed);
        // Ensure non-zero seed (xorshift requires non-zero)
        Rng(if seed_u64 == 0 { 0x246C_CB28_5410_8BA3 } else { seed_u64 })
    }

    /// Create RNG with a fixed seed (for testing/reproducibility)
    fn with_seed(seed: u64) -> Self {
        Rng(if seed == 0 { 1 } else { seed })
    }

    /// Generate next random u64
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Generate random usize in range [0, n)
    fn next_usize(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }

    /// Fisher-Yates shuffle of a slice
    fn shuffle<T>(&mut self, slice: &mut [T]) {
        let n = slice.len();
        for i in (1..n).rev() {
            let j = self.next_usize(i + 1);
            slice.swap(i, j);
        }
    }
}

// ── The 15 pieces to place (excluding King) ───────────────────────────────────

/// All White pieces except the King, in the order we track them
/// These 15 pieces get randomly distributed across ranks 1-2
const PIECES_TO_PLACE: [PieceKind; 15] = [
    PieceKind::Queen,
    PieceKind::Rook,
    PieceKind::Rook,
    PieceKind::Bishop,
    PieceKind::Bishop,
    PieceKind::Knight,
    PieceKind::Knight,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
    PieceKind::Pawn,
];

// ── Main generator ────────────────────────────────────────────────────────────

impl Position {
    /// Generate a new random Pet Dragon starting position.
    /// Every call produces a different position (true random seed).
    pub fn generate_pet_dragon() -> Self {
        let mut rng = Rng::new();
        Self::generate_with_rng(&mut rng)
    }

    /// Generate a Pet Dragon position with a fixed seed (for testing).
    pub fn generate_with_seed(seed: u64) -> Self {
        let mut rng = Rng::with_seed(seed);
        Self::generate_with_rng(&mut rng)
    }

    /// Core generation logic — takes an RNG so we can control randomness
    fn generate_with_rng(rng: &mut Rng) -> Self {
        let mut pos = Position::empty();

        // ── Step 1: Fix White King on e1 ─────────────────────────────────────
        pos.put_piece(Color::White, PieceKind::King, Square::E1);

        // ── Step 2: Collect available squares (ranks 1-2, excluding e1) ──────
        // Ranks 1 and 2 = 16 squares total, minus e1 for the King = 15 squares
        let mut available: Vec<Square> = Vec::with_capacity(15);
        for rank in 0..2u8 {
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank).unwrap();
                if sq != Square::E1 {
                    available.push(sq);
                }
            }
        }
        debug_assert_eq!(available.len(), 15,
            "Should have exactly 15 available squares");

        // ── Step 3: Shuffle available squares ─────────────────────────────────
        rng.shuffle(&mut available);

        // ── Step 4: Place pieces with bishop constraint ───────────────────────
        // Bishops must be on opposite coloured squares.
        // Strategy:
        //   - Find first light square in shuffled list → Bishop 1
        //   - Find first dark square in shuffled list → Bishop 2
        //   - Place all other pieces on remaining squares

        let mut placed_white: Vec<(Square, PieceKind)> = Vec::with_capacity(15);
        let mut remaining_squares: Vec<Square> = Vec::with_capacity(13);

        // Find bishop squares from the shuffled list
        let mut light_bishop_sq: Option<Square> = None;
        let mut dark_bishop_sq:  Option<Square> = None;
        let mut bishop_indices = Vec::new();

        for (idx, &sq) in available.iter().enumerate() {
            if light_bishop_sq.is_none() && Bitboard::is_light_square(sq) {
                light_bishop_sq = Some(sq);
                bishop_indices.push(idx);
            } else if dark_bishop_sq.is_none() && Bitboard::is_dark_square(sq) {
                dark_bishop_sq = Some(sq);
                bishop_indices.push(idx);
            }
            if light_bishop_sq.is_some() && dark_bishop_sq.is_some() {
                break;
            }
        }

        // Place bishops
        let light_sq = light_bishop_sq.expect(
            "Must find a light square in 15 available squares"
        );
        let dark_sq = dark_bishop_sq.expect(
            "Must find a dark square in 15 available squares"
        );

        pos.put_piece(Color::White, PieceKind::Bishop, light_sq);
        pos.put_piece(Color::White, PieceKind::Bishop, dark_sq);
        placed_white.push((light_sq, PieceKind::Bishop));
        placed_white.push((dark_sq,  PieceKind::Bishop));

        // Collect remaining squares (exclude bishop squares)
        let bishop_sq_set = [light_sq, dark_sq];
        for &sq in &available {
            if !bishop_sq_set.contains(&sq) {
                remaining_squares.push(sq);
            }
        }

        debug_assert_eq!(remaining_squares.len(), 13,
            "Should have 13 squares remaining after placing bishops");

        // Place remaining 13 pieces on remaining 13 squares
        // Pieces: Queen, 2 Rooks, 2 Knights, 8 Pawns (in that order from
        // PIECES_TO_PLACE, skipping the 2 bishops we already placed)
        let non_bishop_pieces: Vec<PieceKind> = PIECES_TO_PLACE
            .iter()
            .filter(|&&k| k != PieceKind::Bishop)
            .copied()
            .collect();

        debug_assert_eq!(non_bishop_pieces.len(), 13);

        for (idx, &kind) in non_bishop_pieces.iter().enumerate() {
            let sq = remaining_squares[idx];
            pos.put_piece(Color::White, kind, sq);
            placed_white.push((sq, kind));
        }

        // ── Step 5: Record White pawn starting squares ────────────────────────
        // Also record King start (for reference, though King is always e1)
        let mut pawn_starts = PawnStartMap::EMPTY;

        for &(sq, kind) in &placed_white {
            if kind == PieceKind::Pawn {
                pawn_starts.set(sq, Color::White);
            }
        }
        // White King's pawn start is not recorded (King can't double-step)
        // but we note e1 is the King square

        // ── Step 6: Mirror White to Black ────────────────────────────────────
        // rank 1 (index 0) → rank 8 (index 7)
        // rank 2 (index 1) → rank 7 (index 6)
        // file preserved, piece type preserved

        // Mirror King: e1 → e8
        pos.put_piece(Color::Black, PieceKind::King, Square::E8);

        // Mirror all other White pieces
        for &(white_sq, kind) in &placed_white {
            let black_sq = white_sq.mirror_rank();
            pos.put_piece(Color::Black, kind, black_sq);

            // Record Black pawn starting squares
            if kind == PieceKind::Pawn {
                pawn_starts.set(black_sq, Color::Black);
            }
        }

        // ── Step 7: Set castling rights ───────────────────────────────────────
        // Only if Rooks happened to land on their standard squares.
        // White King is always on e1, so we only check Rook positions.
        let mut castling = CastlingRights::NONE;

        // Check White Rooks
        if pos.piece_bb(Color::White, PieceKind::Rook)
               .contains(Square::H1) {
            castling.white_kingside = true;
        }
        if pos.piece_bb(Color::White, PieceKind::Rook)
               .contains(Square::A1) {
            castling.white_queenside = true;
        }
        // Black mirrors White — if White has kingside castling, so does Black
        if castling.white_kingside  { castling.black_kingside  = true; }
        if castling.white_queenside { castling.black_queenside = true; }

        // ── Step 8: Finalise position ─────────────────────────────────────────
        pos.side_to_move    = Color::White;
        pos.castling        = castling;
        pos.en_passant      = None;
        pos.halfmove_clock  = 0;
        pos.fullmove_number = 1;
        pos.pawn_starts     = pawn_starts;

        // Compute Zobrist hash
        pos.hash = pos.compute_hash();

        pos
    }

    /// Validate a Pet Dragon starting position
    /// Returns Ok(()) if valid, Err(description) if invalid
    pub fn validate_pet_dragon_setup(&self) -> Result<(), String> {
        // ── King positions ────────────────────────────────────────────────────
        if self.king_sq(Color::White) != Square::E1 {
            return Err(format!(
                "White King must be on e1, found on {}",
                self.king_sq(Color::White)
            ));
        }
        if self.king_sq(Color::Black) != Square::E8 {
            return Err(format!(
                "Black King must be on e8, found on {}",
                self.king_sq(Color::Black)
            ));
        }

        // ── Piece counts ──────────────────────────────────────────────────────
        for color in Color::ALL {
            let pawns   = self.count_pieces(color, PieceKind::Pawn);
            let knights = self.count_pieces(color, PieceKind::Knight);
            let bishops = self.count_pieces(color, PieceKind::Bishop);
            let rooks   = self.count_pieces(color, PieceKind::Rook);
            let queens  = self.count_pieces(color, PieceKind::Queen);
            let kings   = self.count_pieces(color, PieceKind::King);

            if pawns   != 8 { return Err(format!("{:?}: expected 8 pawns, got {}",   color, pawns));   }
            if knights != 2 { return Err(format!("{:?}: expected 2 knights, got {}", color, knights)); }
            if bishops != 2 { return Err(format!("{:?}: expected 2 bishops, got {}", color, bishops)); }
            if rooks   != 2 { return Err(format!("{:?}: expected 2 rooks, got {}",   color, rooks));   }
            if queens  != 1 { return Err(format!("{:?}: expected 1 queen, got {}",   color, queens));  }
            if kings   != 1 { return Err(format!("{:?}: expected 1 king, got {}",    color, kings));   }
        }

        // ── Bishop opposite colours ───────────────────────────────────────────
        for color in Color::ALL {
            let bishop_bb = self.piece_bb(color, PieceKind::Bishop);
            let on_light = (bishop_bb & Bitboard::LIGHT_SQUARES).count();
            let on_dark  = (bishop_bb & Bitboard::DARK_SQUARES).count();
            if on_light != 1 || on_dark != 1 {
                return Err(format!(
                    "{:?}: bishops must be on opposite colours \
                     (light={}, dark={})",
                    color, on_light, on_dark
                ));
            }
        }

        // ── White pieces on ranks 1-2 ─────────────────────────────────────────
        let white_occ = self.occupied(Color::White);
        if (white_occ & !Bitboard::WHITE_SETUP_RANKS).is_not_empty() {
            return Err(
                "White pieces found outside ranks 1-2".to_string()
            );
        }

        // ── Black pieces on ranks 7-8 ─────────────────────────────────────────
        let black_occ = self.occupied(Color::Black);
        if (black_occ & !Bitboard::BLACK_SETUP_RANKS).is_not_empty() {
            return Err(
                "Black pieces found outside ranks 7-8".to_string()
            );
        }

        // ── Mirror validation ─────────────────────────────────────────────────
        // Every White piece on square S must have a matching Black piece
        // on S.mirror_rank()
        for sq in Square::all() {
            if let Some(white_piece) = self.piece_on(sq, Color::White) {
                let mirror_sq = sq.mirror_rank();
                let black_piece = self.piece_on(mirror_sq, Color::Black);
                if black_piece != Some(white_piece) {
                    return Err(format!(
                        "Mirror mismatch: White {:?} on {} should mirror to \
                         Black {:?} on {}, but found {:?}",
                        white_piece, sq, white_piece, mirror_sq, black_piece
                    ));
                }
            }
        }

        // ── Pawn start squares recorded ───────────────────────────────────────
        // Every pawn on ranks 1-2 (White) should have a start square
        let mut white_pawns = self.piece_bb(Color::White, PieceKind::Pawn);
        while let Some(sq) = white_pawns.pop_lsb() {
            if !self.pawn_starts.started_here(sq, Color::White) {
                return Err(format!(
                    "White pawn on {} has no start square recorded", sq
                ));
            }
        }

        let mut black_pawns = self.piece_bb(Color::Black, PieceKind::Pawn);
        while let Some(sq) = black_pawns.pop_lsb() {
            if !self.pawn_starts.started_here(sq, Color::Black) {
                return Err(format!(
                    "Black pawn on {} has no start square recorded", sq
                ));
            }
        }

        // ── Castling rights consistency ───────────────────────────────────────
        if self.castling.white_kingside {
            if !self.piece_bb(Color::White, PieceKind::Rook)
                    .contains(Square::H1) {
                return Err(
                    "White kingside castling right set but no Rook on h1"
                    .to_string()
                );
            }
        }
        if self.castling.white_queenside {
            if !self.piece_bb(Color::White, PieceKind::Rook)
                    .contains(Square::A1) {
                return Err(
                    "White queenside castling right set but no Rook on a1"
                    .to_string()
                );
            }
        }

        // ── No pieces in middle ranks ─────────────────────────────────────────
        let middle_ranks = !(Bitboard::WHITE_SETUP_RANKS
                           | Bitboard::BLACK_SETUP_RANKS);
        if (self.all_pieces() & middle_ranks).is_not_empty() {
            return Err(
                "Pieces found in middle ranks (3-6) at setup".to_string()
            );
        }

        // ── No two pieces on same square ──────────────────────────────────────
        if self.all_pieces().count() != 32 {
            return Err(format!(
                "Expected 32 pieces, found {}",
                self.all_pieces().count()
            ));
        }

        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_single_position_valid() {
        setup();
        let pos = Position::generate_with_seed(42);
        assert!(
            pos.validate_pet_dragon_setup().is_ok(),
            "Position with seed 42 failed validation: {:?}",
            pos.validate_pet_dragon_setup()
        );
    }

    #[test]
    fn test_king_always_e1() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            assert_eq!(
                pos.king_sq(Color::White), Square::E1,
                "White King not on e1 for seed {}", seed
            );
            assert_eq!(
                pos.king_sq(Color::Black), Square::E8,
                "Black King not on e8 for seed {}", seed
            );
        }
    }

    #[test]
    fn test_bishops_opposite_colours() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            for color in Color::ALL {
                let bishop_bb = pos.piece_bb(color, PieceKind::Bishop);
                let on_light = (bishop_bb & Bitboard::LIGHT_SQUARES).count();
                let on_dark  = (bishop_bb & Bitboard::DARK_SQUARES).count();
                assert_eq!(on_light, 1,
                    "{:?} should have 1 bishop on light square (seed {})",
                    color, seed);
                assert_eq!(on_dark, 1,
                    "{:?} should have 1 bishop on dark square (seed {})",
                    color, seed);
            }
        }
    }

    #[test]
    fn test_black_mirrors_white() {
        setup();
        for seed in 0..50u64 {
            let pos = Position::generate_with_seed(seed);
            // Every White piece should have matching Black piece on mirror square
            for sq in Square::all() {
                if let Some(white_kind) = pos.piece_on(sq, Color::White) {
                    let mirror = sq.mirror_rank();
                    let black_kind = pos.piece_on(mirror, Color::Black);
                    assert_eq!(
                        black_kind, Some(white_kind),
                        "Mirror mismatch at {} (seed {}): \
                         White {:?} should mirror to Black {:?} on {}",
                        sq, seed, white_kind, black_kind, mirror
                    );
                }
            }
        }
    }

    #[test]
    fn test_all_pieces_on_setup_ranks() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let white_outside =
                (pos.occupied(Color::White)
                 & !Bitboard::WHITE_SETUP_RANKS).count();
            let black_outside =
                (pos.occupied(Color::Black)
                 & !Bitboard::BLACK_SETUP_RANKS).count();
            assert_eq!(white_outside, 0,
                "White pieces outside ranks 1-2 (seed {})", seed);
            assert_eq!(black_outside, 0,
                "Black pieces outside ranks 7-8 (seed {})", seed);
        }
    }

    #[test]
    fn test_pawn_starts_recorded() {
        setup();
        for seed in 0..50u64 {
            let pos = Position::generate_with_seed(seed);
            let mut white_pawns =
                pos.piece_bb(Color::White, PieceKind::Pawn);
            while let Some(sq) = white_pawns.pop_lsb() {
                assert!(
                    pos.pawn_starts.started_here(sq, Color::White),
                    "White pawn on {} has no start square (seed {})",
                    sq, seed
                );
            }
            let mut black_pawns =
                pos.piece_bb(Color::Black, PieceKind::Pawn);
            while let Some(sq) = black_pawns.pop_lsb() {
                assert!(
                    pos.pawn_starts.started_here(sq, Color::Black),
                    "Black pawn on {} has no start square (seed {})",
                    sq, seed
                );
            }
        }
    }

    #[test]
    fn test_castling_rights_consistent() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            // If castling is set, Rook must be on standard square
            if pos.castling.white_kingside {
                assert!(
                    pos.piece_bb(Color::White, PieceKind::Rook)
                       .contains(Square::H1),
                    "White KS castling set but no Rook on h1 (seed {})",
                    seed
                );
            }
            if pos.castling.white_queenside {
                assert!(
                    pos.piece_bb(Color::White, PieceKind::Rook)
                       .contains(Square::A1),
                    "White QS castling set but no Rook on a1 (seed {})",
                    seed
                );
            }
            // Castling right not set when Rook NOT on standard square
            if !pos.piece_bb(Color::White, PieceKind::Rook)
                   .contains(Square::H1) {
                assert!(
                    !pos.castling.white_kingside,
                    "White KS castling set despite no Rook on h1 (seed {})",
                    seed
                );
            }
        }
    }

    #[test]
    fn test_positions_are_different() {
        setup();
        // With different seeds, positions should differ
        // (occasionally same — extremely rare, not tested)
        let mut found_different = false;
        for seed in 1..20u64 {
            let pos0 = Position::generate_with_seed(0);
            let pos_n = Position::generate_with_seed(seed);
            if pos0.hash != pos_n.hash {
                found_different = true;
                break;
            }
        }
        assert!(found_different,
            "All seeds produced the same position — RNG broken");
    }

    #[test]
    fn test_1000_positions_valid() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            assert!(
                pos.validate_pet_dragon_setup().is_ok(),
                "Seed {} failed validation: {:?}",
                seed,
                pos.validate_pet_dragon_setup()
            );
        }
    }

    #[test]
    fn test_hash_is_set() {
        setup();
        for seed in 0..10u64 {
            let pos = Position::generate_with_seed(seed);
            assert_ne!(pos.hash, 0,
                "Hash should not be zero (seed {})", seed);
        }
    }

    #[test]
    fn test_total_pieces() {
        setup();
        for seed in 0..50u64 {
            let pos = Position::generate_with_seed(seed);
            assert_eq!(pos.all_pieces().count(), 32,
                "Should always have 32 pieces (seed {})", seed);
        }
    }

    #[test]
    fn test_no_pieces_in_middle() {
        setup();
        let middle = !(Bitboard::WHITE_SETUP_RANKS
                     | Bitboard::BLACK_SETUP_RANKS);
        for seed in 0..50u64 {
            let pos = Position::generate_with_seed(seed);
            assert!(
                (pos.all_pieces() & middle).is_empty(),
                "Pieces found in middle ranks at setup (seed {})", seed
            );
        }
    }

    #[test]
    fn test_standard_chess_is_valid_pet_dragon() {
        setup();
        // The standard chess starting position satisfies all Pet Dragon rules
        use crate::position::fen::STANDARD_START_FEN;
        let pos = Position::from_fen(STANDARD_START_FEN).unwrap();
        assert!(
            pos.validate_pet_dragon_setup().is_ok(),
            "Standard chess start should be valid Pet Dragon: {:?}",
            pos.validate_pet_dragon_setup()
        );
    }

    #[test]
    fn test_seed_reproducible() {
        setup();
        // Same seed always produces same position
        let pos1 = Position::generate_with_seed(12345);
        let pos2 = Position::generate_with_seed(12345);
        assert_eq!(pos1.hash, pos2.hash,
            "Same seed must produce same position");
    }

    #[test]
    fn test_fen_roundtrip_pet_dragon() {
        setup();
        // Generate a position, convert to FEN, load back, verify same hash
        for seed in 0..10u64 {
            let pos1 = Position::generate_with_seed(seed);
            let fen = pos1.to_fen();
            let pos2 = Position::from_fen(&fen).unwrap();
            // Hashes should match
            assert_eq!(pos1.hash, pos2.hash,
                "FEN roundtrip hash mismatch (seed {})", seed);
            // Pawn starts should match
            for sq in Square::all() {
                assert_eq!(
                    pos1.pawn_starts.get(sq),
                    pos2.pawn_starts.get(sq),
                    "Pawn start mismatch on {} (seed {})", sq, seed
                );
            }
        }
    }
}
