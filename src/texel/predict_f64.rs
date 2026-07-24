// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/predict_f64.rs — f64 forward pass + gradient accumulation (D35 step 5)
//
// `predict_f64()` is a straight f64 port of `predict.rs`'s `predict()` —
// same term order, same king-safety clamp/phase-scaling logic — used for
// loss reporting and the K line search.
//
// `predict_and_accumulate_grad()` does the SAME forward computation but
// additionally accumulates d(loss)/d(weight) into a `TunableWeightsF64`-
// shaped gradient buffer, given d(loss)/d(predicted_score) as
// `error_signal` (computed by the caller from the sigmoid loss). Since
// every term except king safety's clamp is a plain dot product, its
// gradient w.r.t. each weight is just the matching feature value scaled
// by the tapering factor and `error_signal` — the two functions are kept
// side by side, term for term, specifically so a change to one is
// obviously incomplete if the other isn't updated to match; see the
// cross-check test at the bottom, which asserts they agree.
//
// Per D35: king safety's `.min(MAX_KING_DANGER)` clamp gets ZERO gradient
// on `attacker_weight[idx]` when clamped, pass-through otherwise — the
// one deliberate departure from "gradient = feature value" in this file.
// ============================================================================

use crate::texel::features::TexelFeatures;
use crate::texel::weights::MAX_KING_DANGER;
use crate::texel::weights_f64::{TunableWeightsF64, S};

/// f64 port of `predict.rs::predict()` — no gradient bookkeeping, just the
/// score. Used for loss reporting and the K line search.
pub fn predict_f64(f: &TexelFeatures, w: &TunableWeightsF64) -> f64 {
    let phase = f.phase;

    let mut material = S::zero();
    for i in 0..5 {
        material = accum_only(material, w.material_values[i], f.material_diff[i]);
    }
    material = accum_only(material, w.bishop_pair, f.bishop_pair_diff);
    let material_score = material.taper(phase);

    let mut pst = S::zero();
    for kind in 0..6 {
        for idx in 0..64 {
            let d = f.pst_diff[kind][idx];
            if d != 0 {
                pst = accum_only(pst, w.pst[kind][idx], d);
            }
        }
    }
    let pst_score = pst.taper(phase);

    let mut mob = S::zero();
    for i in 0..9 {
        mob = accum_only(mob, w.knight_mobility[i], f.knight_mobility_diff[i]);
    }
    for i in 0..14 {
        mob = accum_only(mob, w.bishop_mobility[i], f.bishop_mobility_diff[i]);
    }
    for i in 0..15 {
        mob = accum_only(mob, w.rook_mobility[i], f.rook_mobility_diff[i]);
    }
    for i in 0..28 {
        mob = accum_only(mob, w.queen_mobility[i], f.queen_mobility_diff[i]);
    }
    let mobility_score = mob.taper(phase);

    let mut pawns = S::zero();
    pawns = accum_only(pawns, w.isolated_penalty, f.pawn_isolated_diff);
    pawns = accum_only(pawns, w.doubled_penalty, f.pawn_doubled_diff);
    pawns = accum_only(pawns, w.backward_penalty, f.pawn_backward_diff);
    for i in 0..8 {
        pawns = accum_only(pawns, w.passed_pawn_bonus[i], f.pawn_passed_diff[i]);
    }
    // D63 item 1 — EG-only (mg contribution always 0), mirrors
    // predict.rs's `s(0, king_dist_eg)` term exactly.
    pawns.eg += w.enemy_king_dist_eg * f.passed_king_enemy_dist_diff as f64
        - w.own_king_dist_eg * f.passed_king_own_dist_diff as f64;
    let pawns_score = pawns.taper(phase);

    let king_safety_score = if phase == 0 {
        0.0
    } else {
        let our_safety = king_safety_side_f64(
            f.king_us_attacker_count,
            f.king_us_attack_units,
            f.king_us_shield_pawns,
            f.king_us_open_files,
            f.king_us_semi_open_files,
            &f.king_us_storm_buckets,
            f.king_us_knights_near_king,
            f.king_us_bishops_near_king,
            w,
        );
        let their_safety = king_safety_side_f64(
            f.king_them_attacker_count,
            f.king_them_attack_units,
            f.king_them_shield_pawns,
            f.king_them_open_files,
            f.king_them_semi_open_files,
            &f.king_them_storm_buckets,
            f.king_them_knights_near_king,
            f.king_them_bishops_near_king,
            w,
        );
        (our_safety - their_safety) * phase as f64 / 24.0
    };

    let mut ol = S::zero();
    ol = accum_only(ol, w.rook_open_file, f.rook_open_diff);
    ol = accum_only(ol, w.rook_semi_open_file, f.rook_semi_diff);
    ol = accum_only(ol, w.rook_on_seventh, f.rook_seventh_diff);
    ol = accum_only(ol, w.rooks_connected, f.rooks_connected_diff);
    ol = accum_only(ol, w.battery_rook_queen, f.battery_rook_queen_diff);
    ol = accum_only(ol, w.contested_file, f.contested_file_diff);
    ol = accum_only(ol, w.queen_open_file, f.queen_open_diff);
    ol = accum_only(ol, w.queen_semi_open_file, f.queen_semi_diff);
    ol = accum_only(ol, w.battery_bishop_queen, f.battery_bishop_queen_diff);
    let open_lines_score = ol.taper(phase);

    let mut th = S::zero();
    th = accum_only(th, w.undefended_knight, f.undefended_knight_diff);
    th = accum_only(th, w.undefended_bishop, f.undefended_bishop_diff);
    th = accum_only(th, w.undefended_rook, f.undefended_rook_diff);
    th = accum_only(th, w.undefended_queen, f.undefended_queen_diff);
    th = accum_only(th, w.threat_by_minor, f.threat_by_minor_diff);
    let threats_score = th.taper(phase);

    material_score
        + pst_score
        + mobility_score
        + pawns_score
        + king_safety_score
        + open_lines_score
        + threats_score
        + w.tempo
}

#[inline]
fn accum_only(acc: S, weight: S, diff: i32) -> S {
    let d = diff as f64;
    S::new(acc.mg + weight.mg * d, acc.eg + weight.eg * d)
}

fn king_safety_side_f64(
    attacker_count: usize,
    attack_units: i32,
    shield_pawns: i32,
    open_files: i32,
    semi_open_files: i32,
    storm_buckets: &[i32; 8],
    knights_near_king: i32,
    bishops_near_king: i32,
    w: &TunableWeightsF64,
) -> f64 {
    let shield_score = shield_pawns as f64 * w.pawn_shield_bonus;
    let open_penalty = open_files as f64 * w.open_file_near_king
        + semi_open_files as f64 * w.semi_open_file_near_king;
    let weight_idx = attacker_count.min(7);
    let danger = (attack_units as f64 * w.attacker_weight[weight_idx] / 100.0)
        .min(MAX_KING_DANGER as f64);
    let storm_danger: f64 = (0..8)
        .map(|i| storm_buckets[i] as f64 * w.pawn_storm_bonus[i])
        .sum();
    let shelter_bonus = knights_near_king as f64 * w.knight_near_own_king
        + bishops_near_king as f64 * w.bishop_near_own_king;
    shield_score + open_penalty - danger - storm_danger + shelter_bonus
}

/// Forward pass + gradient accumulation in one call, mirroring
/// `predict_f64` term for term. `error_signal` is d(loss)/d(predicted
/// score) for this one sample (already includes the sigmoid-derivative
/// and K scaling — see `src/bin/texel_tune.rs`); every weight's gradient
/// contribution gets `+=`'d into `grad`, scaled by that sample's feature
/// value and (for tapered terms) the mg/eg phase-interpolation factor.
///
/// Returns the same score `predict_f64` would return for `(f, w)` — the
/// cross-check test below asserts this equality directly, so a term added
/// to one function and not the other fails a test rather than silently
/// producing wrong gradients.
pub fn predict_and_accumulate_grad(
    f: &TexelFeatures,
    w: &TunableWeightsF64,
    error_signal: f64,
    grad: &mut TunableWeightsF64,
) -> f64 {
    let phase = f.phase;
    let mg_factor = phase as f64 / 24.0;
    let eg_factor = (24 - phase) as f64 / 24.0;

    let mut material = S::zero();
    for i in 0..5 {
        material = accum_grad(
            material,
            w.material_values[i],
            &mut grad.material_values[i],
            f.material_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    material = accum_grad(
        material,
        w.bishop_pair,
        &mut grad.bishop_pair,
        f.bishop_pair_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    let material_score = material.taper(phase);

    let mut pst = S::zero();
    for kind in 0..6 {
        for idx in 0..64 {
            let d = f.pst_diff[kind][idx];
            if d != 0 {
                pst = accum_grad(
                    pst,
                    w.pst[kind][idx],
                    &mut grad.pst[kind][idx],
                    d,
                    error_signal,
                    mg_factor,
                    eg_factor,
                );
            }
        }
    }
    let pst_score = pst.taper(phase);

    let mut mob = S::zero();
    for i in 0..9 {
        mob = accum_grad(
            mob,
            w.knight_mobility[i],
            &mut grad.knight_mobility[i],
            f.knight_mobility_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    for i in 0..14 {
        mob = accum_grad(
            mob,
            w.bishop_mobility[i],
            &mut grad.bishop_mobility[i],
            f.bishop_mobility_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    for i in 0..15 {
        mob = accum_grad(
            mob,
            w.rook_mobility[i],
            &mut grad.rook_mobility[i],
            f.rook_mobility_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    for i in 0..28 {
        mob = accum_grad(
            mob,
            w.queen_mobility[i],
            &mut grad.queen_mobility[i],
            f.queen_mobility_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    let mobility_score = mob.taper(phase);

    let mut pawns = S::zero();
    pawns = accum_grad(
        pawns,
        w.isolated_penalty,
        &mut grad.isolated_penalty,
        f.pawn_isolated_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    pawns = accum_grad(
        pawns,
        w.doubled_penalty,
        &mut grad.doubled_penalty,
        f.pawn_doubled_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    pawns = accum_grad(
        pawns,
        w.backward_penalty,
        &mut grad.backward_penalty,
        f.pawn_backward_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    for i in 0..8 {
        pawns = accum_grad(
            pawns,
            w.passed_pawn_bonus[i],
            &mut grad.passed_pawn_bonus[i],
            f.pawn_passed_diff[i],
            error_signal,
            mg_factor,
            eg_factor,
        );
    }
    // D63 item 1 — EG-only, no mg component, so only `eg_factor` (not
    // `mg_factor`) applies to either weight's gradient. Mirrors the
    // forward term added in `predict_f64` above exactly.
    let enemy_king_dist_diff = f.passed_king_enemy_dist_diff as f64;
    let own_king_dist_diff = f.passed_king_own_dist_diff as f64;
    pawns.eg += w.enemy_king_dist_eg * enemy_king_dist_diff
        - w.own_king_dist_eg * own_king_dist_diff;
    grad.enemy_king_dist_eg += error_signal * eg_factor * enemy_king_dist_diff;
    grad.own_king_dist_eg += error_signal * eg_factor * (-own_king_dist_diff);
    let pawns_score = pawns.taper(phase);

    // King safety — phase/24 scaling applied directly to the per-side
    // error signal (not through S::taper, matching predict_f64/predict()
    // exactly), clamp gets zero gradient on attacker_weight when active.
    let king_safety_score = if phase == 0 {
        0.0
    } else {
        let scaled_error = error_signal * phase as f64 / 24.0;
        let our_safety = king_safety_side_grad(
            f.king_us_attacker_count,
            f.king_us_attack_units,
            f.king_us_shield_pawns,
            f.king_us_open_files,
            f.king_us_semi_open_files,
            &f.king_us_storm_buckets,
            f.king_us_knights_near_king,
            f.king_us_bishops_near_king,
            w,
            scaled_error,
            grad,
        );
        let their_safety = king_safety_side_grad(
            f.king_them_attacker_count,
            f.king_them_attack_units,
            f.king_them_shield_pawns,
            f.king_them_open_files,
            f.king_them_semi_open_files,
            &f.king_them_storm_buckets,
            f.king_them_knights_near_king,
            f.king_them_bishops_near_king,
            w,
            -scaled_error,
            grad,
        );
        (our_safety - their_safety) * phase as f64 / 24.0
    };

    let mut ol = S::zero();
    ol = accum_grad(
        ol,
        w.rook_open_file,
        &mut grad.rook_open_file,
        f.rook_open_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.rook_semi_open_file,
        &mut grad.rook_semi_open_file,
        f.rook_semi_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.rook_on_seventh,
        &mut grad.rook_on_seventh,
        f.rook_seventh_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.rooks_connected,
        &mut grad.rooks_connected,
        f.rooks_connected_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.battery_rook_queen,
        &mut grad.battery_rook_queen,
        f.battery_rook_queen_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.contested_file,
        &mut grad.contested_file,
        f.contested_file_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.queen_open_file,
        &mut grad.queen_open_file,
        f.queen_open_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.queen_semi_open_file,
        &mut grad.queen_semi_open_file,
        f.queen_semi_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    ol = accum_grad(
        ol,
        w.battery_bishop_queen,
        &mut grad.battery_bishop_queen,
        f.battery_bishop_queen_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    let open_lines_score = ol.taper(phase);

    let mut th = S::zero();
    th = accum_grad(
        th,
        w.undefended_knight,
        &mut grad.undefended_knight,
        f.undefended_knight_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    th = accum_grad(
        th,
        w.undefended_bishop,
        &mut grad.undefended_bishop,
        f.undefended_bishop_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    th = accum_grad(
        th,
        w.undefended_rook,
        &mut grad.undefended_rook,
        f.undefended_rook_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    th = accum_grad(
        th,
        w.undefended_queen,
        &mut grad.undefended_queen,
        f.undefended_queen_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    th = accum_grad(
        th,
        w.threat_by_minor,
        &mut grad.threat_by_minor,
        f.threat_by_minor_diff,
        error_signal,
        mg_factor,
        eg_factor,
    );
    let threats_score = th.taper(phase);

    grad.tempo += error_signal;

    material_score
        + pst_score
        + mobility_score
        + pawns_score
        + king_safety_score
        + open_lines_score
        + threats_score
        + w.tempo
}

/// Accumulates one tapered term's forward value AND its gradient
/// contribution in a single call — `weight`'s mg/eg gradient gets
/// `error_signal * mg_factor/eg_factor * diff` added, matching the chain
/// rule through `S::taper`.
#[inline]
fn accum_grad(
    acc: S,
    weight: S,
    grad_slot: &mut S,
    diff: i32,
    error_signal: f64,
    mg_factor: f64,
    eg_factor: f64,
) -> S {
    let d = diff as f64;
    grad_slot.mg += error_signal * mg_factor * d;
    grad_slot.eg += error_signal * eg_factor * d;
    S::new(acc.mg + weight.mg * d, acc.eg + weight.eg * d)
}

/// One king's safety contribution AND its gradient — mirrors
/// `king_safety_side_f64` exactly, plus the D35 clamp rule: zero gradient
/// on `attacker_weight[idx]` when `raw_danger` was clamped by
/// `MAX_KING_DANGER`, full gradient otherwise. `scaled_error` already
/// carries `error_signal * phase/24` and the correct sign for this side
/// (positive for "us", negated for "them" by the caller).
fn king_safety_side_grad(
    attacker_count: usize,
    attack_units: i32,
    shield_pawns: i32,
    open_files: i32,
    semi_open_files: i32,
    storm_buckets: &[i32; 8],
    knights_near_king: i32,
    bishops_near_king: i32,
    w: &TunableWeightsF64,
    scaled_error: f64,
    grad: &mut TunableWeightsF64,
) -> f64 {
    let sp = shield_pawns as f64;
    let of = open_files as f64;
    let sof = semi_open_files as f64;
    let kn = knights_near_king as f64;
    let bp = bishops_near_king as f64;

    let shield_score = sp * w.pawn_shield_bonus;
    let open_penalty = of * w.open_file_near_king + sof * w.semi_open_file_near_king;

    let weight_idx = attacker_count.min(7);
    let raw_danger = attack_units as f64 * w.attacker_weight[weight_idx] / 100.0;
    let clamped = raw_danger >= MAX_KING_DANGER as f64;
    let danger = raw_danger.min(MAX_KING_DANGER as f64);

    let storm_danger: f64 = (0..8)
        .map(|i| storm_buckets[i] as f64 * w.pawn_storm_bonus[i])
        .sum();

    let shelter_bonus = kn * w.knight_near_own_king + bp * w.bishop_near_own_king;

    grad.pawn_shield_bonus += scaled_error * sp;
    grad.open_file_near_king += scaled_error * of;
    grad.semi_open_file_near_king += scaled_error * sof;
    if !clamped {
        grad.attacker_weight[weight_idx] += scaled_error * (-(attack_units as f64) / 100.0);
    }
    for i in 0..8 {
        grad.pawn_storm_bonus[i] += scaled_error * (-(storm_buckets[i] as f64));
    }
    grad.knight_near_own_king += scaled_error * kn;
    grad.bishop_near_own_king += scaled_error * bp;

    shield_score + open_penalty - danger - storm_danger + shelter_bonus
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;
    use crate::position::Position;
    use crate::texel::features::extract_features;
    use crate::texel::weights::TunableWeights;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    /// `predict_and_accumulate_grad`'s returned score must always equal
    /// plain `predict_f64`'s score, for any weights and any error_signal
    /// (gradient bookkeeping must never perturb the forward computation).
    /// This is the load-bearing safety net for THIS session, the same
    /// role the evaluate()-vs-predict() test played in Session 53.
    #[test]
    fn test_grad_forward_matches_predict_f64() {
        setup();
        let default_weights = TunableWeights::default();
        let w = TunableWeightsF64::from(&default_weights);

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
            let features = extract_features(&pos);

            let plain_score = predict_f64(&features, &w);

            let mut grad = TunableWeightsF64::zero();
            let grad_score = predict_and_accumulate_grad(&features, &w, 1.0, &mut grad);

            assert!(
                (plain_score - grad_score).abs() < 1e-6,
                "predict_f64 vs predict_and_accumulate_grad mismatch for {}: {} vs {}",
                fen,
                plain_score,
                grad_score
            );
        }
    }

    /// Sweep many random Pet Dragon positions — same coverage philosophy
    /// as Session 53's self-consistency test, applied to the new f64 path.
    #[test]
    fn test_grad_forward_matches_predict_f64_pet_dragon_positions() {
        setup();
        let default_weights = TunableWeights::default();
        let w = TunableWeightsF64::from(&default_weights);

        for seed in 0..200u64 {
            let pos = Position::generate_with_seed(seed);
            let features = extract_features(&pos);

            let plain_score = predict_f64(&features, &w);
            let mut grad = TunableWeightsF64::zero();
            let grad_score = predict_and_accumulate_grad(&features, &w, 1.0, &mut grad);

            assert!(
                (plain_score - grad_score).abs() < 1e-6,
                "mismatch at seed {}: {} vs {}",
                seed,
                plain_score,
                grad_score
            );
        }
    }

    /// `predict_f64` at the default weights must also agree with the
    /// integer `predict()`/`evaluate()` (within f64 rounding, since
    /// `predict_f64` no longer truncates the tapered division) — a
    /// cheap extra check that the f64 port didn't silently drop a term.
    #[test]
    fn test_predict_f64_close_to_integer_predict_at_default_weights() {
        setup();
        let default_weights = TunableWeights::default();
        let w = TunableWeightsF64::from(&default_weights);

        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let expected = crate::eval::evaluate(&pos) as f64;
            let features = extract_features(&pos);
            let actual = predict_f64(&features, &w);
            // Integer taper() truncates each term separately before
            // summing; f64 taper() doesn't — small (<1cp-per-term-ish)
            // drift is expected and fine here, this is just a sanity
            // bound, not a bit-exactness requirement (that's Session 53's
            // job, on the integer path only).
            assert!(
                (expected - actual).abs() < 6.0,
                "seed {}: evaluate()={} predict_f64()={}",
                seed,
                expected,
                actual
            );
        }
    }
}
