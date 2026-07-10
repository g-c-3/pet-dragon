// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/tables.rs — Piece-square tables (PST)
//
// Each piece gets a positional bonus based on its square.
// Tables are from White's perspective (a1=index 0, h8=index 63).
// Black's tables are mirrored automatically.
//
// Piece-square tables were originally borrowed from Ethereal chess engine
// (GPL v3, Andrew Grant); as of Phase 14 (D35) they are Pet-Dragon-specific,
// Texel-tuned against 147,283 real Pet Dragon self-play positions
// (src/bin/texel_tune.rs, weight_decay=0.08, 100 epochs — see SESSION_LOG).
// The Ethereal tables remain the tuner's starting point
// (src/texel/weights.rs's TunableWeights::default()), not the tables
// compiled here anymore.
//
// All values use the s(mg, eg) tapered score system.
// ============================================================================

use crate::eval::material::{s, taper, eg, mg};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

// ── Pawn table ────────────────────────────────────────────────────────────────
// Pawns want to advance, especially centre pawns
// Penalty for pawns on the rim (a/h files)

#[rustfmt::skip]
const PAWN_TABLE: [i64; 64] = [
    s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0),
    s( 93,169), s(128,167), s( 55,150), s( 91,129), s( 62,140), s(127,128), s( 32,159), s(-12,183),
    s( -2, 94), s(  0, 94), s( 19, 77), s( 34, 60), s( 56, 47), s( 60, 49), s( 28, 84), s(-25, 77),
    s(-12, 29), s( 10, 23), s( 13, 10), s( 30,  0), s( 26, -2), s( 20,  5), s( 20,  9), s(-16, 22),
    s(-30,  7), s( -4,  5), s( -9, -6), s( 19,-14), s(  9,-18), s(  5, -7), s( 14, 12), s(-28,  8),
    s(-31,  3), s(-10, -7), s(  1, -3), s(-14,  5), s(  5,  5), s(  0, -8), s( 38, -5), s(-19,-13),
    s(-35,  4), s(  7,  1), s(-16,  9), s(-26,  3), s( -6, 20), s( 17, -5), s( 37, -4), s(-18,-17),
    s(  2,  9), s(  9, 10), s( -4,  6), s(  4,  6), s(  0,  0), s(  5, -1), s(  8,  3), s(  1,  6),
];

// ── Knight table ──────────────────────────────────────────────────────────────
// Knights love the centre, hate the rim ("a knight on the rim is dim")

#[rustfmt::skip]
const KNIGHT_TABLE: [i64; 64] = [
    s(-167,-58), s(-89,-38), s(-34,-13), s(-49,-28), s( 61,-31), s(-97,-27), s(-15,-63), s(-107,-99),
    s( -73,-25), s(-41, -8), s( 72,-25), s( 36,  6), s( 23,  6), s( 62,-17), s(  7,-24), s( -17,-52),
    s( -47,-24), s( 60,-20), s( 37, 10), s( 65, 18), s( 84, 18), s(129,  8), s( 73, -4), s(  44,-17),
    s(  -9,-10), s( 17,  6), s( 19, 20), s( 53, 34), s( 37, 34), s( 69, 20), s( 18,  6), s(  22, -6),
    s( -13,-10), s(  4,  6), s( 16, 20), s( 13, 34), s( 28, 34), s( 19, 20), s( 21,  6), s(  -8,-10),
    s( -23,-20), s( -9, -8), s( 12, -4), s( 10, 18), s( 22, 18), s( 15, -4), s( 36, -8), s( -21,-20),
    s( -29,-60), s(-53,-20), s(-12,-20), s( -3, -8), s( -1, -8), s( 18,-20), s(-14,-20), s( -19,-60),
    s(-105,-40), s(-21,-60), s(-58,-20), s(-33,-20), s(-17,-20), s(-28,-20), s(-19,-60), s( -23,-40),
];

// ── Bishop table ──────────────────────────────────────────────────────────────
// Bishops like long diagonals and open positions
// Penalty for being blocked by own pawns

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s(-29,-14), s(  4,-21), s(-82,-11), s(-37, -8), s(-25, -7), s(-42, -9), s(  7,-17), s( -8,-24),
    s(-26, -8), s( 16,  6), s(-18,  1), s(-13, -7), s( 30, -3), s( 59, -9), s( 18, -4), s(-47, -21),
    s(-16,  2), s( 37,  0), s( 43,  2), s( 40, -2), s( 35,  6), s( 50,  0), s( 37, -2), s( -2,  4),
    s( -4, -6), s(  5,  0), s( 19,  4), s( 50, -2), s( 37,  4), s( 37, -4), s(  7,  0), s( -2, -6),
    s( -6, -4), s( 13,  0), s( 13,  4), s( 26,  4), s( 34,  0), s(  0,  4), s(  2,  0), s( -6, -6),
    s(  0, -4), s( 15,  0), s( 15,  0), s( 15,  2), s( 14,  4), s( 27,  4), s( 18,  0), s(  4, -8),
    s(  4,-13), s( 15, -6), s(  6, -5), s(  7, -5), s( 10, -5), s( 18, -8), s( 22,-11), s(  1,-13),
    s(-33,-14), s( -3,-21), s( -14,-11),s(-21,-8),  s(-13,-7),  s(-12,-9),  s(-39,-17), s(-21,-24),
];

// ── Rook table ────────────────────────────────────────────────────────────────
// Rooks love open files and the 7th rank
// ⚠️ Pet Dragon: Rooks may start on unusual squares — PST handles this
// naturally by incentivising movement to open files

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s( 32, 13), s( 42, 10), s( 32, 18), s( 51, 15), s( 63, 12), s(  9, 12), s( 31,  8), s( 43,  5),
    s( 27, 11), s( 32, 13), s( 58, 13), s( 62, 11), s( 80,  3), s( 67,  3), s( 26,  8), s( 44,  3),
    s( -5,  7), s( 19,  7), s( 26,  7), s( 36,  5), s( 17,  5), s( 45, -3), s( 61, -5), s( 16, -3),
    s(-24,  4), s(-11,  3), s(  7,  5), s( 26,  4), s( 24,  3), s( 35, -2), s(  3, -3), s( -3, -1),
    s(-27,  3), s(-27,  3), s( -4,  3), s(  3,  5), s( 13,  2), s( -2, -3), s(-10, -2), s(-27,  0),
    s(-30,  0), s( -6,  0), s( -1,  1), s(  9,  3), s(  8,  3), s(  6, -3), s(  2, -4), s(-20, -6),
    s(-33, -3), s(-29,  0), s(-13,  0), s(-11,  1), s( -3,  3), s( -1,  3), s( -5,  0), s(-30, -3),
    s(-53, -2), s(-38, -4), s(-31, -2), s(-26, -1), s(-29,  1), s(-44,  3), s(-10, -4), s(-44, -7),
];

// ── Queen table ───────────────────────────────────────────────────────────────
// Queens are flexible — slight bonus for central control
// Penalty for early queen development (attacked easily)

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s(-28, -9), s(  0, 22), s( 29, 22), s( 12, 27), s( 59, 27), s( 44, 19), s( 43, 10), s( 45, 20),
    s(-24,-17), s(-39,  3), s( -5, -3), s(  1, 14), s(-16, 22), s( 57, 22), s( 28, 22), s( 54,  5),
    s(-13,-20), s(-17,  3), s(  7,  3), s(  8,  5), s( 29, 11), s( 56, 16), s( 47, 12), s( 57,  4),
    s(-27,  0), s(-27,  4), s(-16,  5), s(-16,  5), s( -1, 13), s( 17, 16), s( -2, 18), s(  1,  9),
    s( -9, -4), s(-26,  4), s( -9,  5), s(-10,  5), s( -2,  5), s( -4,  8), s(  3,  8), s(  9, -1),
    s(-14, -5), s(  2, -8), s(-11,  3), s( -2,  3), s( -5,  3), s(  2,  6), s( 14,  2), s(  5,  3),
    s(-35, -8), s( -8,-15), s( 11,-14), s(  2, -8), s(  8, -8), s( 15,-14), s( -3,-13), s(  1,-17),
    s( -1,-20), s(-18,-17), s( -9,-12), s( 10,-15), s(-15,-11), s(-25,-20), s(-31,-12), s(-50,-14),
];

// ── King table ────────────────────────────────────────────────────────────────
// Middlegame: King wants to be safe (castled, behind pawns)
// Endgame: King wants to be active (centralise to support pawns)
// ⚠️ Pet Dragon: No castling bonus in MG — King safety from pawn shield only

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s(-65,-50), s( 23,-30), s( 16,-30), s(-15,-50), s(-56,-50), s(-34,-30), s(  2,-30), s( 13,-50),
    s( 29,-30), s( -1,-10), s(-20,-10), s(-63,-30), s(-22,-30), s(-33,-10), s( -1,-10), s( 28,-30),
    s( -9,-10), s( 24,  0), s(  2,  0), s(-16,-10), s(-20,-10), s(  6,  0), s( 22,  0), s(-22,-10),
    s(-17,-20), s(-20,-10), s(-12, -5), s(-27, -5), s(-30, -5), s(-25, -5), s(-14,-10), s(-36,-20),
    s(-49,-30), s(-1,-20),  s(-27,-10), s(-39,-10), s(-46,-10), s(-44,-10), s(-33,-20), s(-51,-30),
    s(-14,-30), s(-14,-20), s(-22,-10), s(-46,-10), s(-44,-10), s(-30,-10), s(-15,-20), s(-27,-30),
    s(  1,-10), s(  7,  0), s( -8,  0), s(-64,-10), s(-43,-10), s(-16,  0), s(  9,  0), s(  8,-10),
    s(-15,-50), s( 36,-30), s( 12,-30), s(-54,-50), s(  8,-30), s(-28,-30), s( 24,-30), s( 14,-50),
];

// ── PST lookup ────────────────────────────────────────────────────────────────

/// Get the PST value for a piece on a square
/// Automatically mirrors for Black (Black's a8 = White's a1 perspective)
pub fn pst_value(kind: PieceKind, sq: Square, color: Color) -> i64 {
    // Mirror square for Black — Black plays from rank 8 downward
    // PST tables are written rank 8 at index 0, rank 1 at index 56 (standard Ethereal/Stockfish layout).
    // White moves toward rank 8 → use (7-rank)*8+file to index correctly.
    // Black moves toward rank 1 → mirror rank, so use rank*8+file (same as White's sq.index()).
    let idx = match color {
        Color::White => {
            let file = sq.file() as usize;
            let rank = 7 - sq.rank() as usize;
            rank * 8 + file
        }
        Color::Black => sq.index() as usize,
    };

    match kind {
        PieceKind::Pawn   => PAWN_TABLE[idx],
        PieceKind::Knight => KNIGHT_TABLE[idx],
        PieceKind::Bishop => BISHOP_TABLE[idx],
        PieceKind::Rook   => ROOK_TABLE[idx],
        PieceKind::Queen  => QUEEN_TABLE[idx],
        PieceKind::King   => KING_TABLE[idx],
    }
}

// ── PST evaluation ────────────────────────────────────────────────────────────

/// Evaluate piece-square tables for both sides
/// Returns score from side-to-move perspective
pub fn evaluate_tables(pos: &Position, phase: i32) -> i32 {
    let us   = pos.side_to_move;
    let _them = us.flip();

    let mut score = 0i64;

    for color in Color::ALL {
        let sign = if color == us { 1i64 } else { -1i64 };

        for kind in PieceKind::ALL {
            let mut pieces = pos.piece_bb(color, kind);
            while let Some(sq) = pieces.pop_lsb() {
                score += sign * pst_value(kind, sq, color);
            }
        }
    }

    taper(score, phase)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::eval::material::game_phase;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, PieceKind, Square};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_pst_symmetric_at_start() {
        setup();
        let pos   = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_tables(&pos, phase);
        // Starting position is symmetric — PST score should be 0
        assert_eq!(score, 0,
            "PST score should be 0 at symmetric start");
    }

    #[test]
    fn test_knight_centre_better_than_rim() {
        // Knight on e4 (centre) should score higher than knight on a1 (rim)
        let centre = pst_value(PieceKind::Knight, Square::E4, Color::White);
        let rim    = pst_value(PieceKind::Knight, Square::A1, Color::White);
        assert!(mg(centre) > mg(rim),
            "Knight in centre should score higher than on rim");
    }

    #[test]
    fn test_king_endgame_centralises() {
        // King on e4 (centre) should score higher in EG than corner
        let centre = pst_value(PieceKind::King, Square::E4, Color::White);
        let corner = pst_value(PieceKind::King, Square::A1, Color::White);
        assert!(eg(centre) > eg(corner),
            "King should centralise in endgame");
    }

    #[test]
    fn test_pawn_advance_bonus() {
        // White pawn on e5 should score more than e2
        let advanced = pst_value(PieceKind::Pawn, Square::E5, Color::White);
        let start    = pst_value(PieceKind::Pawn, Square::E2, Color::White);
        assert!(mg(advanced) > mg(start),
            "Advanced pawn should score more than start pawn");
    }

    #[test]
    fn test_black_mirror() {
        // Black Knight on e5 should mirror White Knight on e4
        let white_e4 = pst_value(PieceKind::Knight, Square::E4, Color::White);
        let black_e5 = pst_value(PieceKind::Knight, Square::E5, Color::Black);
        assert_eq!(white_e4, black_e5,
            "Black e5 should mirror White e4 in PST");
    }

    #[test]
    fn test_rook_7th_rank() {
        // White Rook on 7th rank (rank index 6) should score well
        let seventh = pst_value(PieceKind::Rook, Square::D7, Color::White);
        let first   = pst_value(PieceKind::Rook, Square::D1, Color::White);
        assert!(mg(seventh) >= mg(first),
            "Rook on 7th should score at least as well as on 1st");
    }

    #[test]
    fn test_pet_dragon_position_tables() {
        setup();
        // PST evaluation should not panic on any Pet Dragon position
        for seed in 0..20u64 {
            let pos   = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _score = evaluate_tables(&pos, phase);
            // Just verify it runs without panicking
        }
    }
}
