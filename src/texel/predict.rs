// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/predict.rs — Tunable eval prediction + self-consistency test (D35)
//
// `predict(features, weights)` recomputes the HCE score using the exact
// same arithmetic `crate::eval::evaluate()` uses internally (packed
// s(mg,eg) tapering via `crate::eval::material::taper`, the same king-safety
// bucket/clamp logic, the same phase==0 early-out) — but pulling every
// constant from `weights` instead of a hardcoded array. At
// `TunableWeights::default()` this must equal `crate::eval::evaluate()`
// bit-for-bit; that equivalence is proven by
// `test_predict_matches_evaluate_default_weights` below, NOT assumed.
//
// Per D35 this is the load-bearing safety net for the whole Texel tuning
// effort: no gradient-descent optimizer gets written until this test is
// green, since there is no other way to catch a silently-wrong feature
// extraction at ~970-parameter scale.
// ============================================================================

use crate::eval::material::taper;
use crate::texel::features::TexelFeatures;
use crate::texel::weights::{TunableWeights, MAX_KING_DANGER};

/// Recompute the HCE score for a position from its extracted features and
/// a (possibly tuned) weight vector. Returns centipawns from the
/// side-to-move's perspective, matching `crate::eval::evaluate()`.
pub fn predict(f: &TexelFeatures, w: &TunableWeights) -> i32 {
    let phase = f.phase;

    // ── Material ─────────────────────────────────────────────────────────
    let mut material_score = 0i64;
    for i in 0..5 {
        material_score += w.material_values[i] * f.material_diff[i] as i64;
    }
    material_score += w.bishop_pair * f.bishop_pair_diff as i64;
    let material = taper(material_score, phase);

    // ── PST ───────────────────────────────────────────────────────────────
    let mut pst_score = 0i64;
    for kind in 0..6 {
        for idx in 0..64 {
            let d = f.pst_diff[kind][idx];
            if d != 0 {
                pst_score += w.pst[kind][idx] * d as i64;
            }
        }
    }
    let tables = taper(pst_score, phase);

    // ── Mobility ──────────────────────────────────────────────────────────
    let mut mob = 0i64;
    for i in 0..9 {
        mob += w.knight_mobility[i] * f.knight_mobility_diff[i] as i64;
    }
    for i in 0..14 {
        mob += w.bishop_mobility[i] * f.bishop_mobility_diff[i] as i64;
    }
    for i in 0..15 {
        mob += w.rook_mobility[i] * f.rook_mobility_diff[i] as i64;
    }
    for i in 0..28 {
        mob += w.queen_mobility[i] * f.queen_mobility_diff[i] as i64;
    }
    let mobility = taper(mob, phase);

    // ── Pawns ─────────────────────────────────────────────────────────────
    let mut pawn_score = 0i64;
    pawn_score += w.isolated_penalty * f.pawn_isolated_diff as i64;
    pawn_score += w.doubled_penalty * f.pawn_doubled_diff as i64;
    pawn_score += w.backward_penalty * f.pawn_backward_diff as i64;
    for i in 0..8 {
        pawn_score += w.passed_pawn_bonus[i] * f.pawn_passed_diff[i] as i64;
    }
    let pawns = taper(pawn_score, phase);

    // ── King safety ───────────────────────────────────────────────────────
    // Matches eval::king_safety::evaluate_king_safety's phase==0 early-out
    // and its (our - their) * phase / 24 scaling exactly (this term is
    // NOT run through the generic taper() helper in the original either).
    let king_safety = if phase == 0 {
        0
    } else {
        let our_safety = king_safety_side(
            f.king_us_attacker_count,
            f.king_us_attack_units,
            f.king_us_shield_pawns,
            f.king_us_open_files,
            f.king_us_semi_open_files,
            w,
        );
        let their_safety = king_safety_side(
            f.king_them_attacker_count,
            f.king_them_attack_units,
            f.king_them_shield_pawns,
            f.king_them_open_files,
            f.king_them_semi_open_files,
            w,
        );
        (our_safety - their_safety) * phase / 24
    };

    // ── Open lines ────────────────────────────────────────────────────────
    let mut ol = 0i64;
    ol += w.rook_open_file * f.rook_open_diff as i64;
    ol += w.rook_semi_open_file * f.rook_semi_diff as i64;
    ol += w.rook_on_seventh * f.rook_seventh_diff as i64;
    ol += w.rooks_connected * f.rooks_connected_diff as i64;
    ol += w.battery_rook_queen * f.battery_rook_queen_diff as i64;
    ol += w.contested_file * f.contested_file_diff as i64;
    ol += w.queen_open_file * f.queen_open_diff as i64;
    ol += w.queen_semi_open_file * f.queen_semi_diff as i64;
    ol += w.battery_bishop_queen * f.battery_bishop_queen_diff as i64;
    let open_lines = taper(ol, phase);

    material + tables + mobility + pawns + king_safety + open_lines + w.tempo
}

/// One king's safety contribution — mirrors
/// `eval::king_safety::king_safety_score`'s combine step exactly, including
/// the `MAX_KING_DANGER` clamp (D35's one genuine nonlinearity: zero
/// gradient when clamped, pass-through otherwise, same as any ML clip).
fn king_safety_side(
    attacker_count: usize,
    attack_units: i32,
    shield_pawns: i32,
    open_files: i32,
    semi_open_files: i32,
    w: &TunableWeights,
) -> i32 {
    let shield_score = shield_pawns * w.pawn_shield_bonus;
    let open_penalty =
        open_files * w.open_file_near_king + semi_open_files * w.semi_open_file_near_king;
    let weight_idx = attacker_count.min(7);
    let danger = (attack_units * w.attacker_weight[weight_idx] / 100).min(MAX_KING_DANGER);
    shield_score + open_penalty - danger
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::texel::features::extract_features;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    /// THE load-bearing test (D35 step 4). Must pass before any
    /// gradient-descent optimizer is written — it's the only way to catch a
    /// silently-wrong feature extraction at ~970-parameter scale, since
    /// there's no val_loss curve or eval_diag-equivalent available yet.
    #[test]
    fn test_predict_matches_evaluate_default_weights() {
        setup();
        let weights = TunableWeights::default();

        // Hand-picked positions: symmetric start, material imbalances,
        // pure endgame (phase=0 king-safety early-out), king-exposed
        // middlegame positions (exercises the attacker/clamp path).
        let fens = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1",
            "4k1r1/8/8/8/8/8/8/4K3 w - - 0 1",
            "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/4K3 w kq - 0 1",
            "4k3/8/8/8/8/8/5PPP/6K1 w - - 0 1",
            "4k3/pppppppp/8/8/8/8/8/4K3 w - - 0 1",
        ];
        for fen in &fens {
            let pos = Position::from_fen(fen).unwrap();
            let expected = crate::eval::evaluate(&pos);
            let features = extract_features(&pos);
            let actual = predict(&features, &weights);
            assert_eq!(
                actual, expected,
                "predict() must match evaluate() exactly for {}: predict={} evaluate={}",
                fen, actual, expected
            );
        }
    }

    /// The real proof: sweep many random Pet Dragon starting positions
    /// (Position::generate_with_seed, same generator selfplay/match_runner
    /// actually use per D32) — not just hand-picked standard-chess FENs.
    #[test]
    fn test_predict_matches_evaluate_pet_dragon_positions() {
        setup();
        let weights = TunableWeights::default();
        for seed in 0..500u64 {
            let pos = Position::generate_with_seed(seed);
            let expected = crate::eval::evaluate(&pos);
            let features = extract_features(&pos);
            let actual = predict(&features, &weights);
            assert_eq!(
                actual, expected,
                "predict()/evaluate() mismatch at seed {}: predict={} evaluate={}",
                seed, actual, expected
            );
        }
    }

    /// Also sweep a handful of plies into games from several seeds, so the
    /// self-consistency check covers positions with pawns off their start
    /// squares, captures, and open lines from real play — not just the
    /// initial Pet Dragon setup.
    #[test]
    fn test_predict_matches_evaluate_after_moves() {
        setup();
        let weights = TunableWeights::default();
        for seed in 0..30u64 {
            let mut pos = Position::generate_with_seed(seed);
            for _ in 0..6 {
                let moves = crate::movegen::generate_moves(&pos);
                if moves.len() == 0 {
                    break;
                }
                // Deterministic "random" pick from the seed/ply so this
                // test has no external RNG dependency.
                let idx = (seed as usize).wrapping_mul(2654435761) % moves.len();
                let mv = moves.get(idx);
                pos.make_move_with_history(mv);

                let expected = crate::eval::evaluate(&pos);
                let features = extract_features(&pos);
                let actual = predict(&features, &weights);
                assert_eq!(
                    actual, expected,
                    "predict()/evaluate() mismatch mid-game at seed {}: predict={} evaluate={}",
                    seed, actual, expected
                );
            }
        }
    }
}
