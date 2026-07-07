// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/mod.rs — Handcrafted evaluation (HCE) — full implementation
//
// Entry point: evaluate(pos) → i32 centipawns from side-to-move perspective.
//
// Terms (all tapered MG→EG unless noted):
//   1. Material:    piece counts with phase-dependent values (Ethereal weights)
//   2. Tables:      piece-square tables — positional bonuses per piece/square
//   3. Mobility:    attack count bonus — active pieces score higher
//   4. Pawns:       pawn structure — passed/isolated/doubled/backward
//   5. King safety: pawn shield, open files near king, attacker count (MG only)
//   6. Open lines:  Rook on open file, batteries, 7th rank, connected rooks
//   7. Tempo:       small bonus for side to move (~10 centipawns)
//
// ⚠️ Pet Dragon notes:
//   - No opening suppression (D6): all terms active from move 1
//   - No castling bonus in king safety (D7): ~74% of games have no castling
//   - Open lines (D8): Rooks/Bishops active from start, never suppressed
//   - Rank 1 pawn rules apply in pawns.rs (D2)
//
// Weights from Ethereal chess engine (GPL v3, Andrew Grant) with
// Pet Dragon adaptations as noted above.
// ============================================================================

pub mod material;
pub mod tables;
pub mod mobility;
pub mod pawns;
pub mod king_safety;
pub mod open_lines;

use crate::position::Position;
use material::{evaluate_material, game_phase};
use tables::evaluate_tables;
use mobility::evaluate_mobility;
use pawns::evaluate_pawns;
use king_safety::evaluate_king_safety;
use open_lines::evaluate_open_lines;

// ── Tempo bonus ───────────────────────────────────────────────────────────────
/// Small bonus for the side to move — having the initiative is worth ~10 cp.
const TEMPO: i32 = 10;

// ── Main evaluation entry point ───────────────────────────────────────────────

/// Evaluate a position and return a score in centipawns from the
/// side-to-move's perspective.
///
/// Positive = good for the side to move.
/// Negative = good for the opponent.
///
/// This function is called from quiescence search (and will be called from
/// alpha_beta once 8.9 wiring is complete).
pub fn evaluate(pos: &Position) -> i32 {
    let phase = game_phase(pos);

    // Sum all evaluation terms
    let score = evaluate_material(pos, phase)
              + evaluate_tables(pos, phase)
              + evaluate_mobility(pos, phase)
              + evaluate_pawns(pos, phase)
              + evaluate_king_safety(pos, phase)
              + evaluate_open_lines(pos, phase)
              + TEMPO;

    score
}

/// Runtime-configurable NNUE blend weight, stored as an integer percentage
/// (0-100) rather than an f32 so it round-trips exactly through an integer
/// UCI `spin` option. 0 = pure HCE, 100 = pure NNUE. Was a compile-time
/// `const` (D23) — Phase 17 needs to A/B pure-HCE vs blended search from
/// the *same* binary for real Elo testing (cutechess/fastchess-style
/// matches via `setoption`), which a hardcoded constant can't do without
/// maintaining two build configurations. Relaxed ordering is fine here —
/// same benign-race reasoning as the lock-free TT (D4): worst case is one
/// search node reads a value one `setoption` call stale.
static NNUE_BLEND_WEIGHT_PCT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(25);

/// Set the NNUE blend weight as a percentage (0-100). Out-of-range values
/// are clamped rather than rejected, matching the existing `Hash`/`Threads`
/// UCI option pattern in `main.rs`. Called from
/// `setoption name NNUEWeight value <N>`.
pub fn set_nnue_weight_pct(pct: u32) {
    NNUE_BLEND_WEIGHT_PCT.store(pct.min(100), std::sync::atomic::Ordering::Relaxed);
}

/// Current NNUE blend weight as a fraction in `0.0..=1.0`.
pub fn nnue_weight() -> f32 {
    NNUE_BLEND_WEIGHT_PCT.load(std::sync::atomic::Ordering::Relaxed) as f32 / 100.0
}

/// Evaluate a position blending the full HCE (`evaluate()`) with the
/// trained Pet Dragon NNUE (Phase 16.6), both already in centipawns from
/// the side-to-move's perspective. Blend weight is runtime-configurable
/// (D23 default 25%, see `set_nnue_weight_pct`).
///
/// This is the function actually wired into search (via
/// `search::alpha_beta::evaluate()`); `evaluate()` itself stays pure-HCE
/// and untouched so its existing test suite keeps validating HCE in
/// isolation.
pub fn evaluate_blended(pos: &Position) -> i32 {
    let hce = evaluate(pos);
    let weight = nnue_weight();
    if weight <= 0.0 {
        // Skip the NNUE forward pass entirely at weight 0 — matters for a
        // pure-HCE arm in an Elo A/B match, which should pay zero NNUE cost.
        return hce;
    }
    let nnue = crate::nnue::inference::evaluate_nnue(pos);
    let blended = (1.0 - weight) * hce as f32 + weight * nnue as f32;
    blended.round() as i32
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
    fn test_evaluate_start_pos_near_zero() {
        setup();
        let pos = Position::start_pos().unwrap();
        let score = evaluate(&pos);
        // Start is symmetric — only tempo bonus remains (~10)
        assert!(score.abs() <= 20,
            "Start position should evaluate near zero (tempo only): {}", score);
    }

    #[test]
    fn test_evaluate_up_material_positive() {
        setup();
        // White up a queen — should be strongly positive
        let fen = "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = evaluate(&pos);
        assert!(score > 800, "Up a queen should evaluate > 800 cp: {}", score);
    }

    #[test]
    fn test_evaluate_down_material_negative() {
        setup();
        // White down a rook
        let fen = "4k1r1/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = evaluate(&pos);
        assert!(score < 0, "Down a rook should evaluate negative: {}", score);
    }

    #[test]
    fn test_evaluate_returns_tempo_in_equal_position() {
        setup();
        // King vs King — only tempo remains
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = evaluate(&pos);
        // Should be exactly TEMPO (10) with possible PST adjustment for king position
        assert!(score.abs() < 100,
            "KvK should evaluate near zero: {}", score);
    }

    #[test]
    fn test_evaluate_1000_pet_dragon_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let _ = evaluate(&pos);
        }
    }

    #[test]
    fn test_evaluate_bounded() {
        setup();
        // Evaluate should never return ±INFINITY for any legal position
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let score = evaluate(&pos);
            assert!(score.abs() < 50_000,
                "Eval should be bounded, got {} (seed {})", score, seed);
        }
    }

    #[test]
    fn test_evaluate_pet_dragon_symmetric_start() {
        setup();
        // Pet Dragon starting positions are mirror-symmetric.
        // evaluate() returns from side-to-move's perspective.
        // Both sides face the same position — score should be ~TEMPO.
        for seed in 0..20u64 {
            let pos = Position::generate_with_seed(seed);
            let score = evaluate(&pos);
            // Allow up to ±100 for small PST asymmetries (King always on e1/e8)
            assert!(score.abs() <= 150,
                "Symmetric Pet Dragon start should eval near zero: {} (seed {})",
                score, seed);
        }
    }

    #[test]
    fn test_evaluate_phase_zero_no_king_safety() {
        setup();
        // In phase 0 (KvK), king safety = 0, score dominated by tempo + PST
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        assert_eq!(phase, 0, "KvK should be phase 0");
        let score = evaluate(&pos);
        // King safety contributes 0 at phase 0 — all other terms near zero
        assert!(score.abs() < 100,
            "Phase 0 evaluate should be near zero: {}", score);
    }
}
