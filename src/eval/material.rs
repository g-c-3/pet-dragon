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
// Piece values were originally borrowed from Ethereal chess engine (GPL v3,
// Andrew Grant); as of Phase 14 (D35) they are Pet-Dragon-specific values
// produced by Texel tuning. Re-tuned in Phase 25 (Session 84, D66) against
// 62,125 fresh self-play positions (src/bin/texel_tune.rs, weight_decay=0.03,
// 75 epochs — see SESSION_LOG), superseding the Phase 14 values. The
// Ethereal values remain the tuner's ORIGINAL starting point historically;
// src/texel/weights.rs's TunableWeights::default() now mirrors these
// Phase-25 values, not the Ethereal ones.
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
// Phase 25 Texel-tuned (Session 84, D66)

pub const MG_VALUES: [i32; 6] = [
    97,   // Pawn
    304,  // Knight
    350,  // Bishop
    474,  // Rook
    1037, // Queen
    0,    // King (handled separately)
];

// ── Endgame piece values ──────────────────────────────────────────────────────
// Phase 25 Texel-tuned (Session 84, D66)

pub const EG_VALUES: [i32; 6] = [
    118,  // Pawn
    286,  // Knight
    294,  // Bishop
    540,  // Rook
    930,  // Queen
    0,    // King
];

// ── Bishop pair bonus ─────────────────────────────────────────────────────────
// Having both bishops is worth extra in open positions
pub const BISHOP_PAIR_MG: i32 = 2;
pub const BISHOP_PAIR_EG: i32 = 15;

// ── Tapered score helper ──────────────────────────────────────────────────────

/// Pack middlegame and endgame scores into a single i64
/// High 32 bits = middlegame, Low 32 bits = endgame
/// This allows accumulating scores with a single addition
#[inline]
pub const fn s(mg: i32, eg: i32) -> i64 {
    ((mg as i64) << 32) + (eg as i64)
}

/// Extract middlegame score from packed value
///
/// D120 (Session 122, review finding #5) fix: the pre-fix version
/// (`(score >> 32) as i32`, a plain arithmetic right shift) was wrong
/// whenever `eg` was negative. `s()` above sign-extends `eg as i64`
/// before adding it to the shifted `mg` half — when `eg` is negative,
/// that sign-extension borrows into the mg half's own bits, and a
/// plain arithmetic shift on the way back out doesn't correct for it.
/// Empirically verified (200,000 random `(mg, eg)` pairs, not just
/// reasoned about): the old extraction was wrong on essentially
/// exactly half of them — every case with `eg < 0` — always off by
/// exactly 1 (too low), while the `eg()` extraction below was correct
/// in 100% of the same cases (the bug is specific to `mg()`).
/// Since every tapered eval term in this engine (material, PST,
/// mobility, pawns, king safety, threats, open lines, PlayStyle
/// bonuses — all of it) is packed and unpacked through these two
/// functions, this was quietly under-counting the middlegame
/// contribution of roughly half of all eval terms by 1 centipawn each,
/// for as long as this packing scheme has existed.
///
/// Fixed using the same technique Stockfish's own `Score` packing
/// uses: add half the low half's range before shifting
/// (`wrapping_add` rather than `+`, so this stays correct — and
/// panic-free in debug builds — even in the extreme, never-actually-
/// occurring case of `score` sitting right at `i64`'s own bounds), and
/// shift as unsigned (`as u64 >>`) so no sign-extension happens during
/// the shift itself. The trailing `as i32` then correctly reinterprets
/// the resulting low 32 bits as a signed value.
#[inline]
pub const fn mg(score: i64) -> i32 {
    (score.wrapping_add(0x8000_0000) as u64 >> 32) as i32
}

/// Extract endgame score from packed value
///
/// Unaffected by the D120 fix above — a plain low-32-bits truncation
/// via `as i32` was, and remains, correct for every case (verified
/// alongside `mg()`'s own fix, same 200,000-case sweep).
#[inline]
pub const fn eg(score: i64) -> i32 {
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

    // ── Packed score mg()/eg() round-trip (D120, Session 122, review finding #5) ──

    #[test]
    fn test_mg_eg_round_trip_negative_eg() {
        // The exact bug D120 fixed: before it, mg() returned 99 here
        // instead of the correct 100, because eg's negative
        // sign-extension during s()'s packing borrowed into the mg
        // half's bits and the old plain-arithmetic-shift mg()
        // extraction didn't correct for it.
        let score = s(100, -50);
        assert_eq!(mg(score), 100, "mg() must round-trip correctly when eg is negative");
        assert_eq!(eg(score), -50, "eg() must round-trip correctly when eg is negative");
    }

    #[test]
    fn test_mg_eg_round_trip_negative_mg() {
        let score = s(-30, 20);
        assert_eq!(mg(score), -30);
        assert_eq!(eg(score), 20);
    }

    #[test]
    fn test_mg_eg_round_trip_both_negative() {
        let score = s(-30, -50);
        assert_eq!(mg(score), -30);
        assert_eq!(eg(score), -50);
    }

    #[test]
    fn test_mg_eg_round_trip_sweep() {
        // Broader sweep than the individual cases above — covers the
        // full sign combination space plus zero, at a range
        // representative of real eval term magnitudes (well within
        // i32, nowhere near i64's actual range where s() lives).
        for mg_v in [-500, -100, -1, 0, 1, 100, 500] {
            for eg_v in [-500, -100, -1, 0, 1, 100, 500] {
                let score = s(mg_v, eg_v);
                assert_eq!(mg(score), mg_v,
                    "mg() round-trip failed for s({mg_v}, {eg_v})");
                assert_eq!(eg(score), eg_v,
                    "eg() round-trip failed for s({mg_v}, {eg_v})");
            }
        }
    }

    #[test]
    fn test_taper_with_negative_eg_uses_correct_mg_value() {
        // End-to-end check at the level every eval term actually calls
        // (taper(), not mg()/eg() directly) — confirms the fix reaches
        // real eval output, not just the unit-level extraction
        // functions in isolation.
        let score = s(100, -50);
        assert_eq!(taper(score, 24), 100,
            "Full middlegame taper must use the correct mg value even \
             when eg is negative — this is exactly the scenario D120 \
             fixed");
    }
}
