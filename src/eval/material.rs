// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/material.rs — Material values with phase adjustment
//
// Piece values are not static — they change based on game phase.
// In the middlegame, bishops are slightly more valuable (open diagonals).
// In the endgame, rooks become more powerful (open files, king attacks).
// Knights are better in closed positions (middlegame).
//
// Values borrowed from Ethereal chess engine (GPL v3, Andrew Grant)
// with attribution. These are world-class tuned values from years of
// self-play optimization.
//
// Tapered evaluation:
//   score = (mg_score * phase + eg_score * (24 - phase)) / 24
//   phase = 0 (pure endgame) to 24 (pure middlegame)
// ============================================================================

use crate::types::{Color, PieceKind};
use crate::position::Position;

// ── Phase weights ─────────────────────────────────────────────────────────────
// Each piece type contributes to the game phase calculation
// Total at start = 4*1 + 4*1 + 4*2 + 2*4 = 24 (middlegame)
// As pieces are captured, phase decreases toward 0 (endgame)

pub const PHASE_WEIGHTS: [i32; 6] = [
    0, // Pawn
    1, // Knight
    1, // Bishop
    2, // Rook
    4, // Queen
    0, // King
];

// ── Middlegame piece values (centipawns) ──────────────────────────────────────
// From Ethereal's classical evaluation (GPL v3)

pub const MG_VALUES: [i32; 6] = [
    82,   // Pawn
    337,  // Knight
    365,  // Bishop
    477,  // Rook
    1025, // Queen
    0,    // King (handled separately)
];

// ── Endgame piece values ──────────────────────────────────────────────────────
// Rooks and Queens become more powerful in the endgame

pub const EG_VALUES: [i32; 6] = [
    94,   // Pawn
    281,  // Knight
    297,  // Bishop
    512,  // Rook
    936,  // Queen
    0,    // King
];

// ── Bishop pair bonus ─────────────────────────────────────────────────────────
// Having both bishops is worth extra in open positions
pub const BISHOP_PAIR_MG: i32 = 22;
pub const BISHOP_PAIR_EG: i32 = 30;

// ── Tapered score helper ──────────────────────────────────────────────────────

/// Pack middlegame and endgame scores into a single i64
/// High 32 bits = middlegame, Low 32 bits = endgame
/// This allows accumulating scores with a single addition
#[inline]
pub fn s(mg: i32, eg: i32) -> i64 {
    ((mg as i64) << 32) + (eg as i64)
}

/// Extract middlegame score from packed value
#[inline]
pub fn mg(score: i64) -> i32 {
    (score >> 32) as i32
}

/// Extract endgame score from packed value
#[inline]
pub fn eg(score: i64) -> i32 {
    score as i32
}

/// Apply taper: blend MG and EG scores based on game phase
#[inline]
pub fn taper(score: i64, phase: i32) -> i32 {
    let phase = phase.max(0).min(24);
    (mg(score) * phase + eg(score) * (24 - phase)) / 24
}

// ── Material evaluation ───────────────────────────────────────────────────────

/// Evaluate material for both sides, return score from side-to-move perspective
pub fn evaluate_material(pos: &Position, phase: i32) -> i32 {
    let us   = pos.side_to_move;
    let them = us.flip();

    let mut score = 0i64;

    for &kind in &[
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
    ] {
        let our_count   = pos.count_pieces(us,   kind) as i32;
        let their_count = pos.count_pieces(them, kind) as i32;
        let diff        = our_count - their_count;

        score += s(MG_VALUES[kind as usize], EG_VALUES[kind as usize])
               * diff as i64;
    }

    // Bishop pair bonus
    if pos.count_pieces(us,   PieceKind::Bishop) >= 2 {
        score += s(BISHOP_PAIR_MG, BISHOP_PAIR_EG);
    }
    if pos.count_pieces(them, PieceKind::Bishop) >= 2 {
        score -= s(BISHOP_PAIR_MG, BISHOP_PAIR_EG);
    }

    taper(score, phase)
}

/// Calculate game phase (24 = full middlegame, 0 = pure endgame)
pub fn game_phase(pos: &Position) -> i32 {
    let mut phase = 0i32;
    for color in Color::ALL {
        for kind in PieceKind::ALL {
            phase += pos.count_pieces(color, kind) as i32
                   * PHASE_WEIGHTS[kind as usize];
        }
    }
    phase.min(24)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_game_phase_start() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_eq!(game_phase(&pos), 24,
            "Starting position should be full middlegame");
    }

    #[test]
    fn test_game_phase_endgame() {
        setup();
        // Kings only — pure endgame
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(game_phase(&pos), 0,
            "Kings only should be pure endgame");
    }

    #[test]
    fn test_material_equal_at_start() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_material(&pos, phase);
        assert_eq!(score, 0,
            "Equal material at start should score 0");
    }

    #[test]
    fn test_material_up_a_queen() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_material(&pos, phase);
        assert!(score > 900,
            "Up a queen should score > 900: {}", score);
    }

    #[test]
    fn test_material_down_a_rook() {
        setup();
        let fen = "4k1r1/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_material(&pos, phase);
        assert!(score < 0,
            "Down a rook should score negative: {}", score);
    }

    #[test]
    fn test_bishop_pair_bonus() {
        setup();
        // White has both bishops, Black has none
        let fen = "4k3/8/8/8/8/8/8/2B1KB2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_material(&pos, phase);
        // Should include bishop values plus bishop pair bonus
        assert!(score > 0,
            "Having both bishops should be positive: {}", score);
    }

    #[test]
    fn test_taper_full_mg() {
        let score = s(100, 50);
        assert_eq!(taper(score, 24), 100,
            "Full middlegame should use MG value");
    }

    #[test]
    fn test_taper_full_eg() {
        let score = s(100, 50);
        assert_eq!(taper(score, 0), 50,
            "Full endgame should use EG value");
    }

    #[test]
    fn test_taper_midpoint() {
        let score = s(100, 0);
        assert_eq!(taper(score, 12), 50,
            "Midpoint phase should blend equally");
    }
}
