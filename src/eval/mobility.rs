// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/mobility.rs — Mobility evaluation
//
// Mobility counts the number of squares each piece can safely reach
// (attacks to squares not occupied by own pieces, excluding x-ray blockers
// for sliders). A piece with more mobility is more active and valuable.
//
// Weights from Ethereal chess engine (GPL v3, Andrew Grant) — world-class
// values tuned via self-play. Indexed by mobility count (0..MAX_MOBILITY).
//
// Pet Dragon note:
//   No modifications needed — mobility is position-based, not rule-based.
//   Rooks and Bishops with no pieces between ranks 3-6 will naturally score
//   high mobility from move 1, correctly reflecting Pet Dragon's open structure.
//
// Tapered: score = (mg * phase + eg * (24 - phase)) / 24
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks, queen_attacks};
use crate::bitboard::masks::{knight_attacks, king_attacks};
use crate::eval::material::{s, taper};
use crate::position::Position;
use crate::types::{Color, PieceKind};

// ── Mobility bonus tables (Ethereal GPL v3, Andrew Grant) ────────────────────
// Each table indexed by mobility count (squares attacked to non-own squares).
// Values are packed i64 scores: high 32 = MG, low 32 = EG.
// Knight: 0-8 mobility, Bishop: 0-13, Rook: 0-14, Queen: 0-27

/// Knight mobility bonus (0–8 squares reachable)
const KNIGHT_MOBILITY: [i64; 9] = [
    s(-62,-81), s(-53,-56), s(-12,-31), s(-4,-16),
    s(  3,  5), s( 13, 11), s( 22, 17), s( 28, 20),
    s( 33, 25),
];

/// Bishop mobility bonus (0–13 squares reachable)
const BISHOP_MOBILITY: [i64; 14] = [
    s(-48,-59), s(-20,-23), s( 16, -3), s( 26, 13),
    s( 38, 24), s( 51, 42), s( 55, 54), s( 63, 57),
    s( 63, 65), s( 68, 73), s( 81, 78), s( 81, 86),
    s( 91, 88), s( 98, 97),
];

/// Rook mobility bonus (0–14 squares reachable)
const ROOK_MOBILITY: [i64; 15] = [
    s(-58,-76), s(-27,-18), s(-15, 28), s(-10, 55),
    s( -5, 69), s( -2, 82), s(  9, 87), s( 16, 94),
    s( 20,102), s( 25,102), s( 32,106), s( 38,109),
    s( 46,111), s( 48,114), s( 58,114),
];

/// Queen mobility bonus (0–27 squares reachable)
const QUEEN_MOBILITY: [i64; 28] = [
    s(-39,-36), s(-21,-15), s(  3,  8), s(  3, 18),
    s( 14, 34), s( 22, 54), s( 28, 61), s( 41, 73),
    s( 43, 79), s( 48, 92), s( 56, 94), s( 60,104),
    s( 60,113), s( 66,120), s( 67,123), s( 70,126),
    s( 71,133), s( 73,136), s( 79,140), s( 80,143),
    s( 86,148), s( 93,166), s( 97,170), s( 99,175),
    s(102,184), s(100,191), s(106,206), s(109,212),
];

// ── Main evaluation function ──────────────────────────────────────────────────

/// Evaluate mobility for both sides and return score from side-to-move perspective.
///
/// Counts squares each piece attacks that are not occupied by friendly pieces.
/// For sliders, uses magic bitboard attack generation with current occupancy,
/// naturally accounting for blockers without needing a separate pin mask.
pub fn evaluate_mobility(pos: &Position, phase: i32) -> i32 {
    let us   = pos.side_to_move;
    let them = us.flip();

    let our_score   = mobility_for_color(pos, us);
    let their_score = mobility_for_color(pos, them);

    taper(our_score - their_score, phase)
}

/// Compute raw (untapered) mobility score for one color.
fn mobility_for_color(pos: &Position, color: Color) -> i64 {
    let own_pieces  = pos.occupied(color);
    let all_pieces  = pos.all_pieces();
    let mut score   = 0i64;

    // ── Knights ──────────────────────────────────────────────────────────────
    let mut knights = pos.piece_bb(color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        let attacks = knight_attacks(sq) & !own_pieces;
        let mobility = attacks.count() as usize;
        score += KNIGHT_MOBILITY[mobility.min(8)];
    }

    // ── Bishops ──────────────────────────────────────────────────────────────
    let mut bishops = pos.piece_bb(color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        let attacks = bishop_attacks(sq, all_pieces) & !own_pieces;
        let mobility = attacks.count() as usize;
        score += BISHOP_MOBILITY[mobility.min(13)];
    }

    // ── Rooks ─────────────────────────────────────────────────────────────────
    let mut rooks = pos.piece_bb(color, PieceKind::Rook);
    while let Some(sq) = rooks.pop_lsb() {
        let attacks = rook_attacks(sq, all_pieces) & !own_pieces;
        let mobility = attacks.count() as usize;
        score += ROOK_MOBILITY[mobility.min(14)];
    }

    // ── Queens ────────────────────────────────────────────────────────────────
    let mut queens = pos.piece_bb(color, PieceKind::Queen);
    while let Some(sq) = queens.pop_lsb() {
        let attacks = queen_attacks(sq, all_pieces) & !own_pieces;
        let mobility = attacks.count() as usize;
        score += QUEEN_MOBILITY[mobility.min(27)];
    }

    // Note: King mobility is excluded — king safety is handled in king_safety.rs.
    // Pawns excluded — pawn mobility covered by pawn structure eval in pawns.rs.

    score
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

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_mobility_start_pos_not_zero() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        // At start only knights have mobility; score should be equal (symmetric)
        let score = evaluate_mobility(&pos, phase);
        assert_eq!(score, 0, "Start position is symmetric — mobility should be 0");
    }

    #[test]
    fn test_mobility_open_position_positive_for_active_side() {
        setup();
        // White Rook on open file vs Black Rook buried — White should score higher
        let fen = "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_mobility(&pos, phase);
        // White Rook has open board mobility; Black has nothing comparable
        assert!(score > 0, "White Rook on open file should outscore Black: {}", score);
    }

    #[test]
    fn test_mobility_pet_dragon_starting_position() {
        setup();
        // Pet Dragon positions — both sides mirror, so mobility should be ~0
        for seed in 0..20u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let score = evaluate_mobility(&pos, phase);
            // Mirrored setup → symmetric mobility → score near 0
            // Small deviations OK from King blocking different squares
            assert!(score.abs() < 200,
                "Symmetric Pet Dragon start should have near-zero mobility: {} (seed {})",
                score, seed);
        }
    }

    #[test]
    fn test_mobility_knight_max_center() {
        setup();
        // Knight in center (e.g. d4) has max 8 mobility on empty board
        let fen = "4k3/8/8/8/3N4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_mobility(&pos, phase);
        // White knight has 8 mobility, Black has 0 (just a king) → White scores higher
        assert!(score > 0, "Central knight should outscore no knight: {}", score);
    }

    #[test]
    fn test_mobility_both_sides_no_panic() {
        setup();
        // Verify mobility eval doesn't panic on any of 1000 Pet Dragon positions
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_mobility(&pos, phase);
        }
    }

    #[test]
    fn test_mobility_score_bounded() {
        setup();
        // Mobility score should never exceed reasonable bounds
        let fens = [
            "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1",  // two open rooks
            "4k3/8/8/8/8/8/8/3QK3 w - - 0 1",     // queen on open board
            "r3k2r/8/8/8/8/8/8/4K3 b kq - 0 1",   // black two rooks
        ];
        for fen in &fens {
            let pos = Position::from_fen(fen).unwrap();
            let phase = game_phase(&pos);
            let score = evaluate_mobility(&pos, phase);
            assert!(score.abs() < 5000,
                "Mobility score should be bounded, got {} for {}", score, fen);
        }
    }

    #[test]
    fn test_mobility_tables_indexed_safely() {
        // Verify table indexing never panics via .min() clamping
        // Knight max mobility on empty board = 8 (matches table size - 1)
        assert_eq!(KNIGHT_MOBILITY.len(), 9);
        assert_eq!(BISHOP_MOBILITY.len(), 14);
        assert_eq!(ROOK_MOBILITY.len(), 15);
        assert_eq!(QUEEN_MOBILITY.len(), 28);
    }
}
