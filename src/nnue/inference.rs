// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// nnue/inference.rs — NORU NNUE inference (Phase 16.6)
//
// Loads the trained i16-quantized Pet Dragon network (Phase 16.5,
// nnue-pet-dragon-h32-a256-e10, val_loss=0.538) and exposes evaluate_nnue(),
// a centipawn score from the side-to-move's perspective — same convention
// as eval::evaluate().
//
// Weights are embedded at compile time via include_bytes! (not loaded from
// a runtime path) so both the native binary and the WASM/browser bundle
// carry the network with zero filesystem access — required for WASM, and
// keeps native/browser evaluation identical without a path-configuration
// UCI option.
//
// ⚠️ REQUIRES nnue/weights/nnue_pet_dragon_quantized.bin TO EXIST IN THE
// REPO BEFORE THIS FILE COMPILES. Gokul: download the artifact from the
// Session 32 training run and upload it to that exact path (small 481K
// binary, well under the 25MB repo-upload limit — no need for D22's
// GitHub Releases workaround here) in the SAME commit as this file, or the
// very next commit before this one lands — GitHub Actions will fail to
// even compile otherwise (missing include_bytes! path is a hard error).
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

use crate::nnue::features::extract_stm_nstm_features;
use crate::position::Position;

/// cp <-> win-probability sigmoid slope. Must stay numerically identical to
/// `train_nnue.rs::CP_TO_WINPROB_SCALE` — this is the inverse of the same
/// transform, and drifting the two apart would silently miscalibrate every
/// NNUE score without any compiler warning.
const CP_TO_WINPROB_SCALE: f32 = 400.0;

/// Embedded quantized network weights (Phase 16.5 training run,
/// nnue-pet-dragon-h32-a256-e10, 483,080 training rows, best epoch 8/10,
/// val_loss=0.53776). Compiled directly into the binary/WASM bundle.
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
    (fp32_equivalent * CP_TO_WINPROB_SCALE).round() as i32
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
        // Sanity bound only — see the scale-derivation note above. A wildly
        // out-of-range value here (not just "not exactly 0") is the signal
        // that CP_TO_WINPROB_SCALE/OUTPUT_SCALE needs re-deriving via
        // audit_against_fp32 rather than another guess.
        assert!(
            score.abs() < 5000,
            "start pos NNUE eval implausibly large: {score}"
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
