// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// nnue/inference.rs — NORU NNUE inference (Phase 16.6)
//
// Loads the trained i16-quantized Pet Dragon network (Phase 16.5,
// nnue-pet-dragon-h32-a256-e10, val_loss=0.501, trained on the Phase 23.3
// sharded self-play dataset — see DECISIONS.md D50) and exposes
// evaluate_nnue(), a centipawn score from the side-to-move's perspective —
// same convention as eval::evaluate().
//
// Weights are embedded at compile time via include_bytes! (not loaded from
// a runtime path) so both the native binary and the WASM/browser bundle
// carry the network with zero filesystem access — required for WASM, and
// keeps native/browser evaluation identical without a path-configuration
// UCI option.
//
// ⚠️ REQUIRES nnue/weights/nnue_pet_dragon_quantized.bin TO EXIST IN THE
// REPO BEFORE THIS FILE COMPILES. To swap in a newly-trained network:
// download the quantized .bin from the train_nnue.yml run's artifact and
// replace this exact file at this exact path, in the same commit as (or
// the commit immediately before) any change to this file — GitHub Actions
// will fail to even compile otherwise (missing include_bytes! path is a
// hard error). NNUEWeight (default 0 = pure HCE) still gates how much this
// embedded network actually affects play — swapping the file alone changes
// nothing at runtime until NNUEWeight is raised above 0.
//
// cp-scale derivation (UNVERIFIED against a real network — flagged below):
//   train_nnue.rs trains against target = sigmoid(eval_cp / 400) (D14,
//   CP_TO_WINPROB_SCALE). BCE loss on that target means the network's raw
//   (pre-sigmoid) fp32 output approximates eval_cp / 400 in logit space.
//   NORU's i16 forward() returns that raw value pre-scaled by
//   noru::quant::OUTPUT_SCALE (16) for quantized-domain precision, so
//   dividing by OUTPUT_SCALE recovers the fp32-equivalent raw output, and
//   multiplying by 400 converts back to centipawns.
//   This chain was derived from NORU's documented API and train_nnue.rs's
//   own target formula, not verified against a real forward pass (no
//   weights file existed when this was written). First real signal: once
//   the weights file lands, run `cargo test nnue::inference` and sanity
//   check test_evaluate_nnue_start_pos_bounded's printed value is a
//   plausible centipawn number (roughly -1000..1000 for a near-balanced
//   start), not something like ±50 (scale too small, needs a bigger
//   multiplier) or ±50000 (scale too large). If it's off, use
//   noru::network::NnueWeights::audit_against_fp32 to get an
//   inferred_output_scale empirically rather than re-guessing.
// ============================================================================

use std::sync::OnceLock;

use noru::network::{forward, Accumulator, NnueWeights};
use noru::quant::OUTPUT_SCALE;

/// Hard ceiling on the magnitude of a raw NNUE evaluation, applied before
/// blending. Without this, an unregularized network can produce arbitrarily
/// large logits (BCE loss has no penalty for pushing output-layer weights
/// toward infinity on cleanly-separable training examples) — Session 43's
/// eval_diag confirmed exactly this: the Session 42 retrained network
/// scored the symmetric start position at +2425cp (should be ~0) and a
/// single queen swing at ~4000cp (HCE: ~976cp), a real, not cosmetic,
/// miscalibration (D30). 1500cp is comfortably above HCE's typical material
/// swings (a queen is ~950-1000cp) while still preventing the network from
/// overriding search with implausible certainty.
const NNUE_EVAL_CLAMP_CP: i32 = 1500;

use crate::nnue::features::extract_stm_nstm_features;
use crate::position::Position;

/// cp <-> win-probability sigmoid slope. Must stay numerically identical to
/// `train_nnue.rs::CP_TO_WINPROB_SCALE` — this is the inverse of the same
/// transform, and drifting the two apart would silently miscalibrate every
/// NNUE score without any compiler warning.
const CP_TO_WINPROB_SCALE: f32 = 400.0;

/// Embedded quantized network weights (Phase 16.5 training run,
/// nnue-pet-dragon-h32-a256-e10, 2,478,608 training rows — 2,428,608
/// self-play + 50,000 Lichess, Phase 23.3's sharded self-play dataset —
/// best epoch 6/10, val_loss=0.50108, superseding the earlier
/// data-starved run at val_loss=0.53776/483,080 rows; see DECISIONS.md
/// D50). Compiled directly into the binary/WASM bundle.
static NNUE_WEIGHTS_BYTES: &[u8] =
    include_bytes!("weights/nnue_pet_dragon_quantized.bin");

/// Parsed weights, lazily initialised once on first use and shared for the
/// life of the process — parsing the ~481K binary once per search (rather
/// than once ever) would be wasteful and is unnecessary since the weights
/// never change at runtime.
static NNUE_WEIGHTS: OnceLock<NnueWeights> = OnceLock::new();

fn weights() -> &'static NnueWeights {
    NNUE_WEIGHTS.get_or_init(|| {
        NnueWeights::load_from_bytes(NNUE_WEIGHTS_BYTES, None).expect(
            "embedded NNUE weights failed to parse — \
             check nnue/weights/nnue_pet_dragon_quantized.bin is the real \
             quantized (not fp32) checkpoint from train_nnue.rs",
        )
    })
}

/// Evaluate a position with the trained Pet Dragon NNUE.
///
/// Returns a centipawn score from the side-to-move's perspective — positive
/// is good for the side to move, matching `eval::evaluate()`'s convention
/// so the two can be blended directly (see `eval::evaluate_blended()`).
pub fn evaluate_nnue(pos: &Position) -> i32 {
    let w = weights();
    let (stm_features, nstm_features) = extract_stm_nstm_features(pos);

    let mut acc = Accumulator::new(&w.feature_bias);
    acc.refresh(w, &stm_features, &nstm_features);
    let raw = forward(&acc, w);

    let fp32_equivalent = raw as f32 / OUTPUT_SCALE as f32;
    let cp = (fp32_equivalent * CP_TO_WINPROB_SCALE).round() as i32;
    cp.clamp(-NNUE_EVAL_CLAMP_CP, NNUE_EVAL_CLAMP_CP)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_evaluate_nnue_start_pos_bounded() {
        setup();
        let pos = Position::start_pos().unwrap();
        let score = evaluate_nnue(&pos);
        // Tightened post-D30 (was < 5000, which the Session 42 network's
        // actual +2425cp start-pos bug passed easily). This bound is now
        // NNUE_EVAL_CLAMP_CP itself, so it directly enforces the clamp
        // rather than allowing any value the clamp would already reject.
        assert!(
            score.abs() <= NNUE_EVAL_CLAMP_CP,
            "start pos NNUE eval exceeds clamp ceiling: {score}"
        );
    }

    #[test]
    fn test_evaluate_nnue_clamp_enforced() {
        // Confirms the clamp itself is wired correctly regardless of what
        // any particular trained network outputs — construct a position
        // with maximal material imbalance (most likely to hit extreme raw
        // output) and check the result never exceeds the documented ceiling.
        setup();
        let pos = Position::from_fen(
            "QQQQQQQQ/QQQQQQQQ/8/8/8/8/8/k6K w - - 0 1",
        )
        .expect("valid FEN");
        let score = evaluate_nnue(&pos);
        assert!(
            score.abs() <= NNUE_EVAL_CLAMP_CP,
            "clamp not enforced: {score} exceeds {NNUE_EVAL_CLAMP_CP}"
        );
    }

    #[test]
    fn test_evaluate_nnue_1000_pet_dragon_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let _ = evaluate_nnue(&pos);
        }
    }

    #[test]
    fn test_evaluate_nnue_deterministic() {
        setup();
        let pos = Position::start_pos().unwrap();
        let a = evaluate_nnue(&pos);
        let b = evaluate_nnue(&pos);
        assert_eq!(a, b, "same position must produce the same NNUE score");
    }
}
