// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// bitboard/masks.rs — Precomputed attack tables and masks
//
// Everything here is computed ONCE at engine startup and stored in
// static arrays. Move generation then just does an array lookup —
// one memory access instead of calculating attacks from scratch.
//
// Tables provided:
//   KNIGHT_ATTACKS[64]    — squares a knight attacks from each square
//   KING_ATTACKS[64]      — squares a king attacks from each square
//   PAWN_ATTACKS[2][64]   — squares a pawn attacks from each square
//                           index 0 = White, index 1 = Black
//   BETWEEN[64][64]       — squares strictly between two squares
//                           (used for check blocking and pin detection)
//   LINE[64][64]          — all squares on the same rank/file/diagonal
//                           through two squares (used for pin rays)
//   RANK_MASKS[8]         — all squares on each rank
//   FILE_MASKS[8]         — all squares on each file
//   DIAGONAL_MASKS[15]    — all squares on each diagonal (NE direction)
//   ANTI_DIAG_MASKS[15]   — all squares on each anti-diagonal (NW direction)
//
// Pet Dragon specific:
//   PAWN_DOUBLE_PUSH_MASK[2][64] — for each pawn start square, the
//                                   target square of a double push
//                                   (rank 1 → rank 3, rank 2 → rank 4)
// ============================================================================

use crate::bitboard::Bitboard;
use crate::types::{Color, Square};

// ── Attack tables (filled at startup by init_masks()) ─────────────────────────

/// Knight attack table: KNIGHT_ATTACKS[sq] = all squares a knight on sq attacks
pub static mut KNIGHT_ATTACKS: [Bitboard; 64] = [Bitboard(0); 64];

/// King attack table: KING_ATTACKS[sq] = all squares a king on sq attacks
pub static mut KING_ATTACKS: [Bitboard; 64] = [Bitboard(0); 64];

/// Pawn attack table: PAWN_ATTACKS[color][sq] = squares attacked by pawn
/// White (0) attacks diagonally north, Black (1) attacks diagonally south
pub static mut PAWN_ATTACKS: [[Bitboard; 64]; 2] = [[Bitboard(0); 64]; 2];

/// Between table: BETWEEN[sq1][sq2] = squares strictly between sq1 and sq2
/// Only filled for squares on the same rank, file, or diagonal.
/// Empty if squares not aligned.
/// Used for: "can this piece block a check?"
pub static mut BETWEEN: [[Bitboard; 64]; 64] = [[Bitboard(0); 64]; 64];

/// Line table: LINE[sq1][sq2] = all squares on the ray through sq1 and sq2
/// including both squares and beyond.
/// Used for: pin detection
pub static mut LINE: [[Bitboard; 64]; 64] = [[Bitboard(0); 64]; 64];

/// Pet Dragon custom: double push targets
/// PAWN_DOUBLE_PUSH_MASK[color][sq] = target square bitboard for double push
/// White pawn on rank 1 → rank 3, White pawn on rank 2 → rank 4
/// Black pawn on rank 8 → rank 6, Black pawn on rank 7 → rank 5
/// Zero if the pawn is not on a valid double-push start rank
pub static mut PAWN_DOUBLE_PUSH_MASK: [[Bitboard; 64]; 2] =
    [[Bitboard(0); 64]; 2];

// ── Initialisation ────────────────────────────────────────────────────────────

/// Initialise all precomputed tables.
/// MUST be called once before any move generation.
/// Called from main() and wasm_main() at engine startup.
pub fn init_masks() {
    for sq_idx in 0u8..64 {
        let sq = Square::from_index(sq_idx).unwrap();
        let bb = Bitboard::from_square(sq);

        // ── Knight attacks ───────────────────────────────────────────────────
        // Knights move in an L-shape: 2 squares one way, 1 square the other.
        // We compute all 8 possible jumps using bitboard shifts,
        // masking off wrap-around on a and h files.
        let knight_attacks = {
            let l1 = (bb.0 >> 1) & Bitboard::NOT_FILE_H.0;
            let l2 = (bb.0 >> 2) & Bitboard::NOT_FILE_GH.0;
            let r1 = (bb.0 << 1) & Bitboard::NOT_FILE_A.0;
            let r2 = (bb.0 << 2) & Bitboard::NOT_FILE_AB.0;
            let h1 = l1 | r1;
            let h2 = l2 | r2;
            Bitboard((h1 << 16) | (h1 >> 16) | (h2 << 8) | (h2 >> 8))
        };
        unsafe { KNIGHT_ATTACKS[sq_idx as usize] = knight_attacks; }

        // ── King attacks ─────────────────────────────────────────────────────
        // King moves one square in any of 8 directions.
        let king_attacks = {
            let mut attacks = bb.shift_north()
                | bb.shift_south()
                | bb.shift_east()
                | bb.shift_west()
                | bb.shift_north_east()
                | bb.shift_north_west()
                | bb.shift_south_east()
                | bb.shift_south_west();
            // Remove the king's own square (shouldn't be set, but be safe)
            attacks.clear(sq);
            attacks
        };
        unsafe { KING_ATTACKS[sq_idx as usize] = king_attacks; }

        // ── Pawn attacks ─────────────────────────────────────────────────────
        // White pawns attack diagonally north (toward rank 8)
        // Black pawns attack diagonally south (toward rank 1)
        let white_pawn_attacks =
            bb.shift_north_east() | bb.shift_north_west();
        let black_pawn_attacks =
            bb.shift_south_east() | bb.shift_south_west();

        unsafe {
            PAWN_ATTACKS[Color::White as usize][sq_idx as usize] =
                white_pawn_attacks;
            PAWN_ATTACKS[Color::Black as usize][sq_idx as usize] =
                black_pawn_attacks;
        }

        // ── Pet Dragon: double push masks ────────────────────────────────────
        // White pawn on rank 1 (index 0) → jumps to rank 3 (+2 ranks)
        // White pawn on rank 2 (index 1) → jumps to rank 4 (+2 ranks)
        // Black pawn on rank 8 (index 7) → jumps to rank 6 (-2 ranks)
        // Black pawn on rank 7 (index 6) → jumps to rank 5 (-2 ranks)
        let rank = sq.rank();
        let file = sq.file();

        // White double push
        let white_double = if rank == 0 || rank == 1 {
            // From rank 1 → land on rank 3, from rank 2 → land on rank 4
            let target_rank = rank + 2;
            if let Some(target) = Square::from_file_rank(file, target_rank) {
                Bitboard::from_square(target)
            } else {
                Bitboard::EMPTY
            }
        } else {
            Bitboard::EMPTY
        };

        // Black double push
        let black_double = if rank == 6 || rank == 7 {
            // From rank 7 → land on rank 5, from rank 8 → land on rank 6
            // rank is 0-indexed so rank 7 = rank 8, rank 6 = rank 7
            let target_rank = rank.wrapping_sub(2);
            if target_rank < 8 {
                if let Some(target) =
                    Square::from_file_rank(file, target_rank) {
                    Bitboard::from_square(target)
                } else {
                    Bitboard::EMPTY
                }
            } else {
                Bitboard::EMPTY
            }
        } else {
            Bitboard::EMPTY
        };

        unsafe {
            PAWN_DOUBLE_PUSH_MASK[Color::White as usize][sq_idx as usize] =
                white_double;
            PAWN_DOUBLE_PUSH_MASK[Color::Black as usize][sq_idx as usize] =
                black_double;
        }
    }

    // ── Between and Line tables ───────────────────────────────────────────────
    // For every pair of squares, compute what's between them and
    // what line they share (if any).
    // Used for check detection, pin detection, and move legality.
    init_between_and_line();
}

/// Compute BETWEEN and LINE tables for all square pairs.
fn init_between_and_line() {
    // Direction vectors: [rank_delta, file_delta]
    // 8 directions: N, NE, E, SE, S, SW, W, NW
    const DIRS: [(i32, i32); 8] = [
        (1, 0), (1, 1), (0, 1), (-1, 1),
        (-1, 0), (-1, -1), (0, -1), (1, -1),
    ];

    for sq1_idx in 0u8..64 {
        let sq1 = Square::from_index(sq1_idx).unwrap();
        let r1 = sq1.rank() as i32;
        let f1 = sq1.file() as i32;

        for &(dr, df) in &DIRS {
            // Walk in this direction from sq1
            let mut r = r1 + dr;
            let mut f = f1 + df;
            let mut ray = Bitboard::EMPTY;

            while r >= 0 && r < 8 && f >= 0 && f < 8 {
                let sq2 = Square::from_file_rank(f as u8, r as u8).unwrap();
                let sq2_idx = sq2.index() as usize;

                // LINE: everything on the full ray through sq1 in this dir
                // including sq1 itself and sq2 and beyond
                // We'll OR in sq2 now and sq1 at the end
                unsafe {
                    LINE[sq1_idx as usize][sq2_idx] |=
                        Bitboard::from_square(sq1);
                    LINE[sq1_idx as usize][sq2_idx] |= ray;
                    LINE[sq1_idx as usize][sq2_idx] |=
                        Bitboard::from_square(sq2);
                }

                // BETWEEN: only squares strictly between sq1 and sq2
                // = the ray so far (not including sq2 yet)
                unsafe {
                    BETWEEN[sq1_idx as usize][sq2_idx] = ray;
                }

                // Add sq2 to ray for the next iteration's BETWEEN
                ray |= Bitboard::from_square(sq2);

                r += dr;
                f += df;
            }
        }
    }
}

// ── Safe accessor functions ───────────────────────────────────────────────────
// These wrap the unsafe static access in safe functions.
// The rest of the engine uses these — never accesses statics directly.

/// Get knight attacks from a square
#[inline]
pub fn knight_attacks(sq: Square) -> Bitboard {
    unsafe { KNIGHT_ATTACKS[sq.index() as usize] }
}

/// Get king attacks from a square
#[inline]
pub fn king_attacks(sq: Square) -> Bitboard {
    unsafe { KING_ATTACKS[sq.index() as usize] }
}

/// Get pawn attacks for a color from a square
#[inline]
pub fn pawn_attacks(color: Color, sq: Square) -> Bitboard {
    unsafe { PAWN_ATTACKS[color as usize][sq.index() as usize] }
}

/// Get squares strictly between two squares (empty if not aligned)
#[inline]
pub fn between(sq1: Square, sq2: Square) -> Bitboard {
    unsafe { BETWEEN[sq1.index() as usize][sq2.index() as usize] }
}

/// Get the full line through two squares (empty if not aligned)
#[inline]
pub fn line(sq1: Square, sq2: Square) -> Bitboard {
    unsafe { LINE[sq1.index() as usize][sq2.index() as usize] }
}

/// Pet Dragon: get double push target for a pawn of given color on given square
/// Returns empty bitboard if the pawn is not on a valid double-push start rank
#[inline]
pub fn pawn_double_push_mask(color: Color, sq: Square) -> Bitboard {
    unsafe {
        PAWN_DOUBLE_PUSH_MASK[color as usize][sq.index() as usize]
    }
}

/// Check if two squares are aligned (same rank, file, or diagonal)
#[inline]
pub fn are_aligned(sq1: Square, sq2: Square, sq3: Square) -> bool {
    line(sq1, sq2).contains(sq3)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Color, Square};

    fn setup() {
        init_masks();
    }

    #[test]
    fn test_knight_attacks_center() {
        setup();
        // Knight on e4 (index 28) should attack 8 squares
        let attacks = knight_attacks(Square::E4);
        assert_eq!(attacks.count(), 8);
        // Should attack d2, f2, c3, g3, c5, g5, d6, f6
        assert!(attacks.contains(Square::D2));
        assert!(attacks.contains(Square::F2));
        assert!(attacks.contains(Square::C3));
        assert!(attacks.contains(Square::G3));
        assert!(attacks.contains(Square::C5));
        assert!(attacks.contains(Square::G5));
        assert!(attacks.contains(Square::D6));
        assert!(attacks.contains(Square::F6));
    }

    #[test]
    fn test_knight_attacks_corner() {
        setup();
        // Knight on a1 can only reach 2 squares
        let attacks = knight_attacks(Square::A1);
        assert_eq!(attacks.count(), 2);
        assert!(attacks.contains(Square::B3));
        assert!(attacks.contains(Square::C2));
    }

    #[test]
    fn test_knight_no_wraparound() {
        setup();
        // Knight on a-file should never attack h-file
        let a1_attacks = knight_attacks(Square::A1);
        assert!((a1_attacks & Bitboard::FILE_H).is_empty());
        let a8_attacks = knight_attacks(Square::A8);
        assert!((a8_attacks & Bitboard::FILE_H).is_empty());
    }

    #[test]
    fn test_king_attacks_center() {
        setup();
        // King on e4 should attack 8 squares
        let attacks = king_attacks(Square::E4);
        assert_eq!(attacks.count(), 8);
    }

    #[test]
    fn test_king_attacks_corner() {
        setup();
        // King on a1 should attack 3 squares
        let attacks = king_attacks(Square::A1);
        assert_eq!(attacks.count(), 3);
        assert!(attacks.contains(Square::A2));
        assert!(attacks.contains(Square::B1));
        assert!(attacks.contains(Square::B2));
    }

    #[test]
    fn test_pawn_attacks_white() {
        setup();
        // White pawn on e2 attacks d3 and f3
        let attacks = pawn_attacks(Color::White, Square::E2);
        assert!(attacks.contains(Square::D3));
        assert!(attacks.contains(Square::F3));
        assert_eq!(attacks.count(), 2);
    }

    #[test]
    fn test_pawn_attacks_black() {
        setup();
        // Black pawn on e7 attacks d6 and f6
        let attacks = pawn_attacks(Color::Black, Square::E7);
        assert!(attacks.contains(Square::D6));
        assert!(attacks.contains(Square::F6));
        assert_eq!(attacks.count(), 2);
    }

    #[test]
    fn test_pawn_attacks_edge() {
        setup();
        // White pawn on a2 only attacks b3 (not off-board)
        let attacks = pawn_attacks(Color::White, Square::A2);
        assert_eq!(attacks.count(), 1);
        assert!(attacks.contains(Square::B3));

        // White pawn on h2 only attacks g3
        let attacks = pawn_attacks(Color::White, Square::H2);
        assert_eq!(attacks.count(), 1);
        assert!(attacks.contains(Square::G3));
    }

    #[test]
    fn test_between_same_rank() {
        setup();
        // Between a1 and h1 should contain b1, c1, d1, e1, f1, g1
        let bb = between(Square::A1, Square::H1);
        assert_eq!(bb.count(), 6);
        assert!(bb.contains(Square::B1));
        assert!(bb.contains(Square::G1));
        assert!(!bb.contains(Square::A1));
        assert!(!bb.contains(Square::H1));
    }

    #[test]
    fn test_between_same_file() {
        setup();
        // Between a1 and a8 should contain a2..a7
        let bb = between(Square::A1, Square::A8);
        assert_eq!(bb.count(), 6);
        assert!(bb.contains(Square::A2));
        assert!(bb.contains(Square::A7));
    }

    #[test]
    fn test_between_diagonal() {
        setup();
        // Between a1 and d4 should contain b2 and c3
        let bb = between(Square::A1, Square::D4);
        assert_eq!(bb.count(), 2);
        assert!(bb.contains(Square::B2));
        assert!(bb.contains(Square::C3));
    }

    #[test]
    fn test_between_adjacent() {
        setup();
        // Between adjacent squares should be empty
        let bb = between(Square::E1, Square::E2);
        assert!(bb.is_empty());
    }

    #[test]
    fn test_between_not_aligned() {
        setup();
        // Between non-aligned squares should be empty
        let bb = between(Square::A1, Square::B3);
        assert!(bb.is_empty());
    }

    // ── Pet Dragon specific tests ─────────────────────────────────────────────

    #[test]
    fn test_white_pawn_double_push_from_rank1() {
        setup();
        // White pawn on e1 (rank 1) should double-push to e3
        // This is the Pet Dragon custom rule — rank 1 start square
        let mask = pawn_double_push_mask(Color::White, Square::E1);
        assert!(!mask.is_empty(), "White pawn on rank 1 should have double push");
        assert!(mask.contains(Square::E3),
            "White pawn on e1 should double-push to e3");
    }

    #[test]
    fn test_white_pawn_double_push_from_rank2() {
        setup();
        // White pawn on e2 (rank 2) should double-push to e4 (standard chess)
        let mask = pawn_double_push_mask(Color::White, Square::E2);
        assert!(mask.contains(Square::E4),
            "White pawn on e2 should double-push to e4");
    }

    #[test]
    fn test_black_pawn_double_push_from_rank8() {
        setup();
        // Black pawn on e8 (rank 8) should double-push to e6
        let mask = pawn_double_push_mask(Color::Black, Square::E8);
        assert!(!mask.is_empty(), "Black pawn on rank 8 should have double push");
        assert!(mask.contains(Square::E6),
            "Black pawn on e8 should double-push to e6");
    }

    #[test]
    fn test_black_pawn_double_push_from_rank7() {
        setup();
        // Black pawn on e7 (rank 7) should double-push to e5 (standard chess)
        let mask = pawn_double_push_mask(Color::Black, Square::E7);
        assert!(mask.contains(Square::E5),
            "Black pawn on e7 should double-push to e5");
    }

    #[test]
    fn test_no_double_push_from_middle_ranks() {
        setup();
        // A pawn on rank 3 or higher (White) has no double push
        let mask = pawn_double_push_mask(Color::White, Square::E3);
        assert!(mask.is_empty(),
            "White pawn on rank 3 should have no double push");
        let mask = pawn_double_push_mask(Color::White, Square::E4);
        assert!(mask.is_empty(),
            "White pawn on rank 4 should have no double push");
    }

    #[test]
    fn test_double_push_all_files_rank1() {
        setup();
        // Every file on rank 1 should have a valid White double push to rank 3
        for file in 0u8..8 {
            let sq = Square::from_file_rank(file, 0).unwrap(); // rank 1
            let mask = pawn_double_push_mask(Color::White, sq);
            assert!(!mask.is_empty(),
                "White pawn on rank 1 file {} should have double push", file);
            let target = Square::from_file_rank(file, 2).unwrap(); // rank 3
            assert!(mask.contains(target),
                "White pawn on rank 1 file {} should reach rank 3", file);
        }
    }

    #[test]
    fn test_double_push_all_files_rank8() {
        setup();
        // Every file on rank 8 should have a valid Black double push to rank 6
        for file in 0u8..8 {
            let sq = Square::from_file_rank(file, 7).unwrap(); // rank 8
            let mask = pawn_double_push_mask(Color::Black, sq);
            assert!(!mask.is_empty(),
                "Black pawn on rank 8 file {} should have double push", file);
            let target = Square::from_file_rank(file, 5).unwrap(); // rank 6
            assert!(mask.contains(target),
                "Black pawn on rank 8 file {} should reach rank 6", file);
        }
    }
}
