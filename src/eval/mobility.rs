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
use crate::bitboard::masks::knight_attacks;
use crate::eval::material::{s, taper};
use crate::position::Position;
use crate::types::{Color, PieceKind};

// ── Mobility bonus tables (Phase 25, Session 84, D66 Texel-tuned) ────────────
// Originally borrowed from Ethereal (GPL v3, Andrew Grant); as of Phase 14
// these became Pet-Dragon-specific Texel-tuned values, and were re-tuned in
// Phase 25 against 62,125 fresh self-play positions (weight_decay=0.03,
// 75 epochs — see SESSION_LOG), superseding the Phase 14 values. Ethereal's
// values remain the tuner's ORIGINAL starting point historically
// (src/texel/weights.rs's TunableWeights::default() now mirrors these
// Phase-25 values, not the Ethereal ones).
// Each table indexed by mobility count (squares attacked to non-own squares).
// Values are packed i64 scores: high 32 = MG, low 32 = EG.
// Knight: 0-8 mobility, Bishop: 0-13, Rook: 0-14, Queen: 0-27

/// Knight mobility bonus (0–8 squares reachable)
const KNIGHT_MOBILITY: [i64; 9] = [
    s( -72, -94), s( -30, -56), s( -19, -13), s(  -1,  -4),
    s(   8,  10), s(   2,  14), s(  17,  12), s(   9,  11),
    s(  10,  21),
];

/// Bishop mobility bonus (0–13 squares reachable)
const BISHOP_MOBILITY: [i64; 14] = [
    s( -29, -39), s(   1, -21), s(  20,   3), s(  30,  33),
    s(  37,  33), s(  49,  50), s(  44,  49), s(  64,  52),
    s(  59,  55), s(  53,  58), s(  58,  67), s(  65,  80),
    s(  70,  91), s(  79,  91),
];

/// Rook mobility bonus (0–14 squares reachable)
const ROOK_MOBILITY: [i64; 15] = [
    s( -63, -78), s( -14, -14), s( -17,  35), s(   8,  64),
    s( -17,  84), s(  -8,  80), s(   9,  95), s(  19, 101),
    s(  11,  99), s(   8, 100), s(  30, 117), s(  44, 100),
    s(  45, 121), s(  44, 111), s(  55, 109),
];

/// Queen mobility bonus (0–27 squares reachable)
const QUEEN_MOBILITY: [i64; 28] = [
    s( -64, -48), s( -23, -32), s(  14,   6), s(   6,  33),
    s(   0,  23), s(  30,  47), s(  34,  57), s(  49,  85),
    s(  44,  80), s(  39,  79), s(  69, 101), s(  63,  99),
    s(  56, 127), s(  62, 114), s(  71, 123), s(  59, 112),
    s(  86, 152), s(  63, 139), s(  78, 138), s(  79, 159),
    s(  74, 148), s(  90, 151), s( 105, 176), s(  87, 164),
    s(  94, 174), s( 116, 202), s(  97, 194), s( 112, 217),
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
