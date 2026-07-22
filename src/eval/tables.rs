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
// Texel-tuned. Re-tuned in Phase 25 (Session 84, D66) against 62,125 fresh
// self-play positions (src/bin/texel_tune.rs, weight_decay=0.03, 75 epochs
// — see SESSION_LOG), superseding the Phase 14 values. The Ethereal tables
// remain the tuner's ORIGINAL starting point historically;
// src/texel/weights.rs's TunableWeights::default() now mirrors these
// Phase-25 values, not the Ethereal ones.
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
    s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0),
    s(  65, 140), s( 121, 145), s(  28, 123), s(  80, 103), s(  44, 113), s( 106, 100), s(   8, 134), s(  -2, 161),
    s( -27,  99), s( -15,  79), s(   0,  51), s(  57,  67), s(  46,  44), s(  47,  34), s(  10,  61), s(   3,  74),
    s( -20,   9), s(  -3,  23), s( -12,   7), s(  16,  19), s(  11,   5), s(  16,  -9), s(  -4,  21), s(   5,  16),
    s( -28,  -7), s(  12,   3), s( -32,   4), s(  22,  -8), s(   1, -25), s(  18,  -5), s(  22,  -1), s(  -7,  19),
    s( -29,  11), s(   6, -16), s(   7,  -9), s( -18,   7), s( -17,  -6), s( -13, -20), s(  56,   3), s( -30,   1),
    s( -21,  11), s(   2,   9), s(  -4,   1), s( -11,   8), s( -10,   2), s(  -6,   6), s(  56,   8), s( -23,   0),
    s(  -1,  32), s(  22,   9), s(  -6,   4), s(  11,  -3), s(   0,   0), s(   5,  20), s(   5,  18), s(  11,  29),
];

// ── Knight table ──────────────────────────────────────────────────────────────
// Knights love the centre, hate the rim ("a knight on the rim is dim")

#[rustfmt::skip]
const KNIGHT_TABLE: [i64; 64] = [
    s(-180, -44), s( -69, -20), s( -59, -32), s( -61, -25), s(  60, -39), s(-102,  -7), s(  -5, -52), s( -84, -92),
    s( -48,  -6), s( -27,  -4), s(  56, -11), s(  14,  13), s( -13, -10), s(  84,   1), s(   2, -20), s( -33, -62),
    s( -66, -12), s(  56,   0), s(  37,  13), s(  46,   6), s(  77,  16), s( 143,   1), s(  65,  -6), s(  33, -12),
    s(  -6,  18), s(  -1,   2), s(  10,  35), s(  44,  24), s(  28,  28), s(  56,   5), s(  35,  21), s(  26,  18),
    s( -35,  -5), s( -14,  -3), s(   9,  34), s(  15,  40), s(  11,  19), s(  28,  20), s(  12, -13), s( -27, -23),
    s( -13, -23), s( -13,  -3), s(   0,   2), s(   7,   6), s(  12,   8), s(  22, -15), s(  34, -14), s( -40, -44),
    s( -46, -76), s( -51, -20), s( -16, -22), s(  13,   3), s(  17,   5), s(   5, -11), s(  -5, -20), s( -42, -68),
    s(-113, -50), s( -26, -47), s( -48,  -1), s( -19, -27), s( -16, -21), s( -17,  -9), s( -12, -38), s( -15, -34),
];

// ── Bishop table ──────────────────────────────────────────────────────────────
// Bishops like long diagonals and open positions
// Penalty for being blocked by own pawns

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s( -65, -41), s(  -8, -40), s( -71,  14), s( -17,  12), s( -38,  -5), s( -74, -10), s(   4, -17), s( -45, -55),
    s( -13, -23), s( -10,   3), s( -13,  19), s(   3,  12), s(  17,  -5), s(  46, -14), s(  15, -17), s( -40, -40),
    s(   1,  23), s(  31,  -6), s(  28,  19), s(  41,  -6), s(  13,   2), s(  20, -13), s(  42, -11), s(   5,   8),
    s(  19,  18), s(  27,  -7), s(  -9,  -3), s(  43,  -3), s(  12, -24), s(  18,   4), s(  19, -16), s(  18,  10),
    s( -15, -21), s(   2,   6), s(  17, -12), s(   7,   0), s(   5,  -9), s(   6,  -4), s(   8,  23), s( -16, -29),
    s(  -2, -16), s(  21,   5), s(  27,  16), s(  22,   7), s(  19, -15), s(  13,   1), s(   8,  -3), s(  12,  19),
    s(   4, -20), s(  10,  21), s(  -4, -15), s(   1, -10), s(  26,  -9), s(  33,  13), s(  31,   0), s( -25, -13),
    s( -22, -34), s(   7, -13), s(   2,  -5), s(  -6, -19), s(   8,  10), s( -33, -21), s( -13,  -8), s( -15, -12),
];

// ── Rook table ────────────────────────────────────────────────────────────────
// Rooks love open files and the 7th rank
// ⚠️ Pet Dragon: Rooks may start on unusual squares — PST handles this
// naturally by incentivising movement to open files

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s(  35,  14), s(   7, -13), s(   9,   6), s(  34,   3), s(  84,  20), s(  14,  25), s(  17,   6), s(  46,  27),
    s(  17,  15), s(  20,   5), s(  36,   5), s(  31,  -7), s(  65,  -8), s(  36, -24), s(  15,   9), s(  55,  17),
    s(   8,  31), s(   5,   7), s(  11,  -3), s(  29,  -2), s(  26,   5), s(  63,  12), s(  48,   7), s(  38,  25),
    s( -14,  19), s( -10,  15), s( -17, -18), s(  -1, -11), s(   4, -19), s(  31,  -2), s( -17,   8), s(  11,  25),
    s( -29,  15), s( -59, -26), s(  -8,  17), s( -17,  -6), s(  28,   2), s(  -8,   0), s( -21,   6), s(  -6,  29),
    s( -21,  16), s(  -4,  -4), s(   8,  -7), s( -20, -20), s(  32,  29), s(  -1,   6), s(   6, -13), s(  11,  12),
    s( -33,   9), s( -38,  -7), s( -18,  -9), s( -19, -17), s(  18,   4), s( -19, -12), s( -18, -16), s( -14, -10),
    s( -49,   7), s( -45,  10), s( -20,   9), s( -21,   6), s(  -8, -13), s( -46,  10), s( -12,   0), s( -32,  14),
];

// ── Queen table ───────────────────────────────────────────────────────────────
// Queens are flexible — slight bonus for central control
// Penalty for early queen development (attacked easily)

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s( -34,  -9), s( -17,  -2), s(  16,   5), s(  27,  34), s(  71,  34), s(  54,  36), s(  64,  26), s(  27,  14),
    s( -13,  -7), s( -40,   1), s(  -8,  -6), s(   8,   6), s(   4,  38), s(  55,  19), s(  48,  38), s(  72,  16),
    s( -20,  -8), s( -16,   7), s(  18,  13), s(  19,  25), s(  16,   8), s(  67,  28), s(  71,  34), s(  36,  -4),
    s( -10,  25), s( -13,  31), s( -34, -15), s(  -6,   0), s(  -8,  18), s(  28,   9), s( -21,   2), s(  -2,  10),
    s(   4,  10), s( -30,   2), s(   2,  18), s( -24, -15), s( -17,   1), s( -22,  -3), s(  28,  37), s(  -1, -15),
    s(   2,  12), s(  -2,  -7), s( -19,  11), s(  -7,  -6), s(  -6,  -7), s(  17,  17), s(  17,   6), s(  -6, -15),
    s( -43, -25), s( -11,  -5), s(  11,  -8), s( -21, -35), s(  21,  -8), s(  33, -30), s(   6,  -6), s(   9,   3),
    s( -20, -21), s( -31, -10), s( -24, -41), s(  -1, -23), s( -40, -31), s( -14, -12), s( -41, -28), s( -63,  -4),
];

// ── King table ────────────────────────────────────────────────────────────────
// Middlegame: King wants to be safe (castled, behind pawns)
// Endgame: King wants to be active (centralise to support pawns)
// ⚠️ Pet Dragon: No castling bonus in MG — King safety from pawn shield only

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s( -57, -32), s(  33, -19), s(  23, -16), s( -29, -70), s( -59, -65), s( -36, -27), s( -10, -46), s(  18, -41),
    s(  14, -41), s( -14,  -7), s( -24,  -9), s( -65, -40), s( -29, -26), s( -51, -38), s(   4,   2), s(  15, -48),
    s( -31, -16), s(  40,   7), s(   4, -18), s( -11, -11), s( -29,  -6), s( -11, -16), s(  27, -10), s( -38, -23),
    s( -32, -22), s( -30, -11), s(  -6,   5), s( -37,  13), s( -51,   2), s( -46, -10), s( -37,  -6), s( -49,  -5),
    s( -38, -35), s( -29, -20), s( -29,   2), s( -49,   7), s( -55,   6), s( -71,   4), s( -48, -20), s( -41,  -4),
    s( -26, -35), s(  -3,  -5), s( -40, -12), s( -48,  -3), s( -28,  -1), s( -24,  -3), s( -41, -14), s( -37, -12),
    s( -21,   1), s( -18, -17), s(   7, -17), s( -50, -17), s( -54,   1), s( -18,  -1), s(  13, -16), s(  10,  -3),
    s( -10, -49), s(  13, -28), s( -11, -55), s( -30, -41), s(   9, -35), s( -26, -28), s(  23, -45), s(   5, -40),
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
