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

use crate::eval::material::{s, taper};
#[cfg(test)]
use crate::eval::material::{eg, mg};
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
    s(-169,-55), s(-88,-34), s(-35, -9), s(-46,-25), s( 62,-31), s(-94,-25), s(-20,-69), s(-103,-103),
    s( -66,-22), s(-36, -4), s( 76,-19), s( 32,  5), s( 13, -2), s( 60,-13), s(  1,-28), s( -16,-45),
    s( -54,-29), s( 63,-17), s( 37,  8), s( 64, 13), s( 76, 10), s(125,  1), s( 79,  2), s(  42,-13),
    s(  -6, -7), s( 10,  0), s( 16, 20), s( 48, 33), s( 32, 32), s( 70, 15), s( 18,  8), s(  21, -5),
    s( -14, -3), s(  5,  6), s( 15, 15), s(  7, 29), s( 26, 30), s( 17, 20), s( 21,  4), s(  -9, -4),
    s( -25,-23), s(-12,-16), s(  8,  0), s( 13, 19), s( 27, 20), s( 13, -6), s( 28,-14), s( -27,-22),
    s( -31,-56), s(-50,-12), s(-12,-15), s( -2,-11), s(  5,  0), s( 14,-19), s(-12,-16), s( -23,-56),
    s(-107,-36), s(-25,-57), s(-55,-21), s(-33,-21), s(-11,-16), s(-34,-21), s(-21,-56), s( -29,-38),
];

// ── Bishop table ──────────────────────────────────────────────────────────────
// Bishops like long diagonals and open positions
// Penalty for being blocked by own pawns

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s(-37,-16), s(  1,-16), s(-83, -8), s(-39, -8), s(-28,-11), s(-50,-15), s(  1,-21), s(-18,-34),
    s(-21, -4), s( 11,  8), s(-22, -4), s(-16,-12), s( 23, -8), s( 54,-12), s( 13,-10), s(-49,-21),
    s(-18, -3), s( 31, -2), s( 36, -1), s( 39, -1), s( 27, -1), s( 43,  0), s( 29,-10), s(  2, 10),
    s( -7, -9), s(  6,  3), s( 20, -2), s( 45, -5), s( 28, -3), s( 33, -2), s(  5, -6), s( -1,  1),
    s( -6, -8), s(  5, -8), s(  8,  1), s( 15, -1), s( 26, -6), s(  2,  3), s( -3, -3), s( -5,-10),
    s(  5, -4), s(  9, -4), s( 11, -3), s(  9,  2), s( 15,  1), s( 23,  3), s( 14, -4), s(  6, -5),
    s(  6,-11), s( 21,  2), s(  9, -3), s(  8, -6), s( 15, -4), s( 17, -8), s( 27, -8), s( -3,-14),
    s(-32,-11), s(  2,-13), s( -9, -2), s(-21,-11), s(-15,-11), s( -8, -1), s(-34,-13), s(-14,-14),
];

// ── Rook table ────────────────────────────────────────────────────────────────
// Rooks love open files and the 7th rank
// ⚠️ Pet Dragon: Rooks may start on unusual squares — PST handles this
// naturally by incentivising movement to open files

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s( 30, 12), s( 33,  2), s( 34, 23), s( 50, 17), s( 65, 17), s(  8,  8), s( 29,  3), s( 44, 10),
    s( 26, 11), s( 28, 16), s( 54, 10), s( 54,  4), s( 84,  6), s( 59, -1), s( 17,  0), s( 41,  2),
    s( -2,  9), s( 15,  3), s( 17,  0), s( 32,  4), s( 19,  7), s( 42,  0), s( 58, -7), s( 24,  3),
    s(-18, 10), s(-10,  5), s( -2, -3), s( 24,  0), s( 21,  2), s( 32, -4), s( -2, -3), s( -3,  6),
    s(-22,  8), s(-33, -3), s( -9,  1), s( -1,  4), s( 13, -2), s( -4, -5), s(-14, -5), s(-19,  8),
    s(-24,  7), s( -4,  2), s( -4,  1), s(  4, -4), s( 14,  9), s(  7, -1), s(  1, -6), s(-16, -8),
    s(-37,  3), s(-31,  0), s(-17,  1), s(-13,  1), s(  1,  4), s(  2,  5), s(  1,  5), s(-31,  2),
    s(-56, -7), s(-42, -4), s(-35, -3), s(-17,  7), s(-31, -1), s(-37,  9), s(-19,-10), s(-43,  3),
];

// ── Queen table ───────────────────────────────────────────────────────────────
// Queens are flexible — slight bonus for central control
// Penalty for early queen development (attacked easily)

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s(-24, -8), s( -1, 21), s( 29, 21), s(  5, 20), s( 60, 29), s( 48, 22), s( 43,  9), s( 41, 21),
    s(-20,-12), s(-34,  3), s(-10, -4), s(  4, 14), s(-12, 26), s( 56, 21), s( 32, 24), s( 56,  8),
    s(-18,-24), s(-12,  4), s(  8,  1), s( 11, 11), s( 32, 18), s( 54, 17), s( 48,  8), s( 56,  7),
    s(-20,  5), s(-21, 10), s(-13,  8), s(-15,  8), s( -5, 13), s( 10, 14), s( -1, 19), s( -1,  9),
    s( -9, -4), s(-20,  8), s(-13,  1), s(-12,  4), s(  1, 11), s( -2,  8), s(  5, 12), s(  9,  0),
    s(-14,-10), s( -2, -5), s( -5,  8), s( -1,  2), s( -3,  9), s( -4, -2), s(  9, -4), s(  9,  7),
    s(-40,-12), s(-11,-16), s(  5,-19), s( -3,-11), s( 10, -7), s( 14,-13), s( -4,-13), s(  8,-13),
    s( -9,-27), s(-19,-12), s( -6,-15), s(  3,-22), s(-17,-12), s(-26,-24), s(-34,-18), s(-53, -8),
];

// ── King table ────────────────────────────────────────────────────────────────
// Middlegame: King wants to be safe (castled, behind pawns)
// Endgame: King wants to be active (centralise to support pawns)
// ⚠️ Pet Dragon: No castling bonus in MG — King safety from pawn shield only

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s(-66,-46), s( 22,-29), s( 19,-23), s(-17,-50), s(-57,-48), s(-36,-35), s(  3,-25), s( 18,-42),
    s( 25,-29), s(  1, -6), s(-20, -7), s(-60,-22), s(-25,-35), s(-35,-15), s( -3,-14), s( 29,-25),
    s(-10,-10), s( 30,  9), s(  4,  1), s(-13, -5), s(-26,-13), s(  4,  0), s( 21,  0), s(-26,-10),
    s(-17,-17), s(-20, -6), s( -7,  3), s(-27,  1), s(-31,  0), s(-28, -7), s(-16, -5), s(-40,-17),
    s(-51,-32), s( -8,-17), s(-22, -2), s(-38, -4), s(-49, -3), s(-51, -8), s(-29,-14), s(-54,-28),
    s(-18,-35), s(-14,-16), s(-28, -9), s(-48,-10), s(-43, -8), s(-37, -9), s(-17,-21), s(-34,-31),
    s( -4,-12), s(  0, -5), s(-19, -9), s(-66,-12), s(-47, -7), s(-10, -3), s(  0, -9), s(  0,-15),
    s(-20,-56), s( 34,-35), s( 18,-34), s(-53,-44), s( 14,-24), s(-32,-34), s( 21,-36), s(  8,-50),
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
