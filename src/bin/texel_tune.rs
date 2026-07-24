// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/texel_tune.rs — Texel tuning gradient-descent driver (Phase 14,
// D35 step 5)
//
// Reads `<FEN>|<game_result>` lines (texel_gen.rs's exact format, Phase
// 14.2), fits `TunableWeightsF64` against those results via the classic
// "Texel's Tuning Method": minimize mean-squared error between
// sigmoid(K * eval) and the actual game result, over ALL parameters
// simultaneously via gradient descent — not per-parameter coordinate
// descent, since D35's audit confirmed HCE is linear-in-weights (one
// clamp nonlinearity, handled in predict_f64.rs). K is found once via a
// coarse-then-fine line search before weight tuning starts (also standard
// practice — the classic method article and most open-source Texel
// tuners, e.g. Ethereal's, do this same two-phase K-then-weights split;
// no source code borrowed from any of them, just the well-known
// technique).
//
// Optimizer: plain Adam (Kingma & Ba, 2014 — public algorithm, not
// borrowed from any specific engine's code) over a flattened parameter
// vector (`TunableWeightsF64::flatten`/`unflatten`), matching this
// project's existing NNUE trainer's optimizer choice (train_nnue.rs) for
// consistency, though implemented independently here since noru's
// `AdamState` is tied to NNUE's own tensor shapes and not reusable for a
// flat HCE weight vector.
//
// Usage (GitHub Actions only, per D15 — see .github/workflows/texel_tune.yml):
//   cargo run --release --bin texel_tune -- \
//     <data_files_comma_separated> <output_path> <epochs> <learning_rate> \
//     <k> <seed> <batch_size>
//   <k> may be the literal string "auto" to run the K line search first.
// ============================================================================

use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::texel::features::{extract_features, TexelFeatures};
use pet_dragon_lib::texel::predict_f64::{predict_and_accumulate_grad, predict_f64};
use pet_dragon_lib::texel::weights::TunableWeights;
use pet_dragon_lib::texel::weights_f64::TunableWeightsF64;

/// Adam hyperparameters — standard defaults (Kingma & Ba, 2014), matching
/// train_nnue.rs's convention of using these exact defaults unless a
/// specific tuning problem shows otherwise (none has here yet).
const ADAM_BETA1: f64 = 0.9;
const ADAM_BETA2: f64 = 0.999;
const ADAM_EPS: f64 = 1e-8;

/// One loaded, feature-extracted training sample: cached so every epoch
/// only re-does cheap dot products, not FEN parsing + board scanning.
struct Sample {
    features: TexelFeatures,
    result: f64, // 0.0 loss / 0.5 draw / 1.0 win, stm-perspective (texel_gen.rs)
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let data_files = args.get(1).cloned().unwrap_or_default();
    let output_path = args.get(2).cloned().unwrap_or_else(|| "texel_weights_tuned.txt".to_string());
    let epochs: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(50);
    let learning_rate: f64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let k_arg = args.get(5).cloned().unwrap_or_else(|| "auto".to_string());
    let seed: u64 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(42);
    let batch_size: usize = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(16384);
    let weight_decay: f64 = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(0.0);

    if data_files.is_empty() {
        eprintln!("usage: texel_tune <data_files_comma_separated> <output_path> <epochs> <learning_rate> <k|auto> <seed> <batch_size> [weight_decay]");
        std::process::exit(1);
    }

    eprintln!("Loading samples...");
    let samples = load_samples(&data_files);
    eprintln!("Loaded {} samples from {}", samples.len(), data_files);
    if samples.is_empty() {
        eprintln!("ERROR: no usable samples loaded — check data_files path(s) and format");
        std::process::exit(1);
    }

    let default_weights = TunableWeights::default();
    let default_weights_f64 = TunableWeightsF64::from(&default_weights);
    let mut weights = TunableWeightsF64::from(&default_weights);

    let k = match k_arg.as_str() {
        "auto" => {
            eprintln!("Running K line search...");
            let k = find_optimal_k(&samples, &weights);
            eprintln!("Optimal K = {:.6}", k);
            k
        }
        s => s.parse().unwrap_or_else(|_| panic!("invalid K value: {}", s)),
    };

    let baseline_loss = mean_loss(&samples, &weights, k);
    eprintln!("Baseline loss (default weights) = {:.6}", baseline_loss);

    train(
        &mut weights,
        &default_weights_f64,
        &samples,
        k,
        epochs,
        learning_rate,
        seed,
        batch_size,
        weight_decay,
    );

    let final_loss = mean_loss(&samples, &weights, k);
    eprintln!(
        "Final loss = {:.6} (baseline {:.6}, delta {:+.6})",
        final_loss,
        baseline_loss,
        final_loss - baseline_loss
    );

    let tuned = weights.to_tunable_weights();
    write_tuned_weights(&output_path, &tuned, k, baseline_loss, final_loss, samples.len());
    eprintln!("Wrote tuned weights to {}", output_path);
}

/// Parse every `data_files`-listed file (comma-separated paths) of
/// `<FEN>|<game_result>` lines (texel_gen.rs's format) into feature-
/// extracted samples. Malformed lines are skipped with a warning rather
/// than aborting the whole run — a handful of bad lines in a
/// 147k-sample database shouldn't sink hours of Actions compute.
fn load_samples(data_files: &str) -> Vec<Sample> {
    let mut samples = Vec::new();
    for path in data_files.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("WARNING: could not open {}: {}", path, e);
                continue;
            }
        };
        let reader = BufReader::new(file);
        let mut file_count = 0usize;
        for (line_no, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("WARNING: {}:{}: read error: {}", path, line_no + 1, e);
                    continue;
                }
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((fen, result_str)) = line.rsplit_once('|') else {
                eprintln!("WARNING: {}:{}: missing '|' separator, skipped", path, line_no + 1);
                continue;
            };
            let Ok(result) = result_str.trim().parse::<f64>() else {
                eprintln!("WARNING: {}:{}: bad result value '{}', skipped", path, line_no + 1, result_str);
                continue;
            };
            let pos = match Position::from_fen(fen.trim()) {
                Ok(p) => p,
                Err(_) => {
                    eprintln!("WARNING: {}:{}: bad FEN, skipped", path, line_no + 1);
                    continue;
                }
            };
            let features = extract_features(&pos);
            samples.push(Sample { features, result });
            file_count += 1;
        }
        eprintln!("  {}: {} samples", path, file_count);
    }
    samples
}

/// sigmoid(K * eval / 400) — classic Texel scaling (eval in centipawns).
#[inline]
fn sigmoid(eval: f64, k: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf(-k * eval / 400.0))
}

/// Mean squared error between sigmoid(K * predicted eval) and the actual
/// game result, over the full dataset, at the given weights/K.
fn mean_loss(samples: &[Sample], weights: &TunableWeightsF64, k: f64) -> f64 {
    let sum: f64 = samples
        .iter()
        .map(|s| {
            let eval = predict_f64(&s.features, weights);
            let err = s.result - sigmoid(eval, k);
            err * err
        })
        .sum();
    sum / samples.len() as f64
}

/// Coarse-then-fine 1-D line search for the K that minimizes `mean_loss`
/// at the CURRENT (default) weights — standard Texel tuning practice.
/// Not re-run during weight training: K stays fixed once found, same as
/// every established Texel tuner does (re-searching K every epoch just
/// chases a moving target and slows convergence for no accuracy benefit).
fn find_optimal_k(samples: &[Sample], weights: &TunableWeightsF64) -> f64 {
    let mut best_k = 1.0;
    let mut best_loss = mean_loss(samples, weights, best_k);

    // Coarse pass: 0.0..=4.0 step 0.05 (widened from an initial 0.0..=2.0
    // after the smoke-test run below hit that boundary on a small sample —
    // real Texel K values are usually 0.5-1.5 but there's no reason to risk
    // clipping the true optimum for a cheap wider sweep).
    let mut k = 0.05;
    while k <= 4.0 {
        let loss = mean_loss(samples, weights, k);
        if loss < best_loss {
            best_loss = loss;
            best_k = k;
        }
        k += 0.05;
    }

    // Fine pass: +/-0.05 around the coarse best, step 0.001
    let lo = (best_k - 0.05).max(0.001);
    let hi = best_k + 0.05;
    let mut k = lo;
    while k <= hi {
        let loss = mean_loss(samples, weights, k);
        if loss < best_loss {
            best_loss = loss;
            best_k = k;
        }
        k += 0.001;
    }

    best_k
}

/// Minimal deterministic PRNG for shuffling — xorshift64*, no external
/// crate dependency, same spirit as the deterministic move selection
/// used elsewhere in this project's test/data-generation code (no
/// reliance on OS entropy, fully reproducible from `seed`).
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15).max(1))
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn next_usize(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

fn shuffle(indices: &mut [usize], rng: &mut Rng) {
    for i in (1..indices.len()).rev() {
        let j = rng.next_usize(i + 1);
        indices.swap(i, j);
    }
}

/// Batched Adam gradient descent over the flattened weight vector, with
/// optional decoupled weight decay ANCHORED AT `default_weights` rather
/// than zero (D30/D31 established the AdamW-style decoupled-decay pattern
/// for train_nnue.rs; anchoring at zero doesn't make sense for HCE weights
/// — "shrink everything toward no evaluation at all" isn't a sensible
/// prior. Anchoring at the current hand-tuned defaults instead means:
/// parameters with a strong, consistent gradient signal from the data
/// move freely away from the default; parameters with a weak/noisy signal
/// (rare king-safety attacker-count buckets, uncommon PST squares) get
/// pulled back toward their known-reasonable starting point instead of
/// drifting on noise — exactly the D35-observed failure mode from the
/// first real run: `bishop_pair` MG went negative and `attacker_weight`
/// picked up negative entries after only 15 epochs / ~135 gradient steps).
///
/// Each epoch: reshuffle sample order, walk `batch_size`-sized batches,
/// accumulate the mean gradient over the batch via
/// `predict_and_accumulate_grad`, take one Adam step, then apply decay
/// toward `default_weights` if `weight_decay > 0.0`. Prints mean loss
/// (over the FULL dataset, not just the last batch) once per epoch so
/// progress is visible in the Actions log.
fn train(
    weights: &mut TunableWeightsF64,
    default_weights: &TunableWeightsF64,
    samples: &[Sample],
    k: f64,
    epochs: usize,
    learning_rate: f64,
    seed: u64,
    batch_size: usize,
    weight_decay: f64,
) {
    let n_params = TunableWeightsF64::PARAM_COUNT;
    let mut m = vec![0.0f64; n_params]; // Adam first moment
    let mut v = vec![0.0f64; n_params]; // Adam second moment
    let mut t = 0i32; // Adam timestep
    let default_flat = default_weights.flatten();

    let mut rng = Rng::new(seed);
    let mut indices: Vec<usize> = (0..samples.len()).collect();

    // ln(10)/400 factor from d/dx[sigmoid(K*x/400)] via the base-10 form.
    let sig_scale = std::f64::consts::LN_10 / 400.0;

    for epoch in 0..epochs {
        shuffle(&mut indices, &mut rng);

        for batch in indices.chunks(batch_size) {
            let mut grad = TunableWeightsF64::zero();

            for &idx in batch {
                let sample = &samples[idx];
                let eval = predict_f64(&sample.features, weights);
                let sig = sigmoid(eval, k);
                // d(error^2)/d(eval) = -2*(result - sig) * sig*(1-sig) * K * ln(10)/400
                let error_signal =
                    -2.0 * (sample.result - sig) * sig * (1.0 - sig) * k * sig_scale;
                predict_and_accumulate_grad(&sample.features, weights, error_signal, &mut grad);
            }

            let batch_n = batch.len() as f64;
            let flat_grad = grad.flatten();
            let mut flat_weights = weights.flatten();

            t += 1;
            let bias_correction1 = 1.0 - ADAM_BETA1.powi(t);
            let bias_correction2 = 1.0 - ADAM_BETA2.powi(t);

            for i in 0..n_params {
                let g = flat_grad[i] / batch_n;
                m[i] = ADAM_BETA1 * m[i] + (1.0 - ADAM_BETA1) * g;
                v[i] = ADAM_BETA2 * v[i] + (1.0 - ADAM_BETA2) * g * g;
                let m_hat = m[i] / bias_correction1;
                let v_hat = v[i] / bias_correction2;
                flat_weights[i] -= learning_rate * m_hat / (v_hat.sqrt() + ADAM_EPS);
                // Decoupled decay toward the default, applied AFTER the
                // Adam step (same ordering train_nnue.rs uses for its own
                // AdamW-style decay), not routed through the gradient/Adam
                // moment estimates.
                if weight_decay > 0.0 {
                    flat_weights[i] -=
                        learning_rate * weight_decay * (flat_weights[i] - default_flat[i]);
                }
            }

            *weights = TunableWeightsF64::unflatten(&flat_weights);
        }

        let loss = mean_loss(samples, weights, k);
        eprintln!("epoch {}/{}: loss = {:.6}", epoch + 1, epochs, loss);
    }
}

/// Write tuned weights as Rust array-literal syntax matching
/// `TunableWeights::default()`'s own format exactly, so the next session
/// can read this file and produce the eval/*.rs delta (D35 step 6)
/// largely by copy-paste rather than hand-transcription.
fn write_tuned_weights(
    path: &str,
    w: &TunableWeights,
    k: f64,
    baseline_loss: f64,
    final_loss: f64,
    n_samples: usize,
) {
    let mut out = String::new();
    out.push_str(&format!(
        "// Texel-tuned weights — K={:.6}, {} samples, baseline_loss={:.6}, final_loss={:.6}\n",
        k, n_samples, baseline_loss, final_loss
    ));
    out.push_str("// Same s(mg,eg)/array-literal syntax as TunableWeights::default() (weights.rs) —\n");
    out.push_str("// see D35 step 6: format into eval/*.rs's own const syntax as a delta.\n\n");

    out.push_str("material_values: [\n");
    for v in &w.material_values {
        out.push_str(&format!("    {},\n", fmt_packed(*v)));
    }
    out.push_str("],\n");
    out.push_str(&format!("bishop_pair: {},\n\n", fmt_packed(w.bishop_pair)));

    let pst_names = ["PAWN", "KNIGHT", "BISHOP", "ROOK", "QUEEN", "KING"];
    for (name, table) in pst_names.iter().zip(w.pst.iter()) {
        out.push_str(&format!("{}_TABLE: [\n", name));
        for rank in 0..8 {
            out.push_str("    ");
            for file in 0..8 {
                out.push_str(&format!("{}, ", fmt_packed(table[rank * 8 + file])));
            }
            out.push('\n');
        }
        out.push_str("],\n\n");
    }

    write_table(&mut out, "knight_mobility", &w.knight_mobility);
    write_table(&mut out, "bishop_mobility", &w.bishop_mobility);
    write_table(&mut out, "rook_mobility", &w.rook_mobility);
    write_table(&mut out, "queen_mobility", &w.queen_mobility);

    out.push_str(&format!("isolated_penalty: {},\n", fmt_packed(w.isolated_penalty)));
    out.push_str(&format!("doubled_penalty: {},\n", fmt_packed(w.doubled_penalty)));
    out.push_str(&format!("backward_penalty: {},\n", fmt_packed(w.backward_penalty)));
    write_table(&mut out, "passed_pawn_bonus", &w.passed_pawn_bonus);

    out.push_str(&format!("enemy_king_dist_eg: {},\n", w.enemy_king_dist_eg));
    out.push_str(&format!("own_king_dist_eg: {},\n\n", w.own_king_dist_eg));

    out.push_str(&format!("attacker_weight: {:?},\n", w.attacker_weight));
    out.push_str(&format!("open_file_near_king: {},\n", w.open_file_near_king));
    out.push_str(&format!("semi_open_file_near_king: {},\n", w.semi_open_file_near_king));
    out.push_str(&format!("pawn_shield_bonus: {},\n\n", w.pawn_shield_bonus));

    out.push_str(&format!("pawn_storm_bonus: {:?},\n\n", w.pawn_storm_bonus));

    out.push_str(&format!("knight_near_own_king: {},\n", w.knight_near_own_king));
    out.push_str(&format!("bishop_near_own_king: {},\n\n", w.bishop_near_own_king));

    out.push_str(&format!("rook_open_file: {},\n", fmt_packed(w.rook_open_file)));
    out.push_str(&format!("rook_semi_open_file: {},\n", fmt_packed(w.rook_semi_open_file)));
    out.push_str(&format!("rook_on_seventh: {},\n", fmt_packed(w.rook_on_seventh)));
    out.push_str(&format!("rooks_connected: {},\n", fmt_packed(w.rooks_connected)));
    out.push_str(&format!("queen_open_file: {},\n", fmt_packed(w.queen_open_file)));
    out.push_str(&format!("queen_semi_open_file: {},\n", fmt_packed(w.queen_semi_open_file)));
    out.push_str(&format!("battery_rook_queen: {},\n", fmt_packed(w.battery_rook_queen)));
    out.push_str(&format!("battery_bishop_queen: {},\n", fmt_packed(w.battery_bishop_queen)));
    out.push_str(&format!("contested_file: {},\n\n", fmt_packed(w.contested_file)));

    out.push_str(&format!("undefended_knight: {},\n", fmt_packed(w.undefended_knight)));
    out.push_str(&format!("undefended_bishop: {},\n", fmt_packed(w.undefended_bishop)));
    out.push_str(&format!("undefended_rook: {},\n", fmt_packed(w.undefended_rook)));
    out.push_str(&format!("undefended_queen: {},\n", fmt_packed(w.undefended_queen)));
    out.push_str(&format!("threat_by_minor: {},\n\n", fmt_packed(w.threat_by_minor)));

    out.push_str(&format!("tempo: {},\n", w.tempo));

    fs::write(path, out).expect("failed to write tuned weights file");
}

fn write_table(out: &mut String, name: &str, table: &[i64]) {
    out.push_str(&format!("{}: [\n    ", name));
    for v in table {
        out.push_str(&format!("{}, ", fmt_packed(*v)));
    }
    out.push_str("\n],\n\n");
}

fn fmt_packed(packed: i64) -> String {
    format!(
        "s({}, {})",
        pet_dragon_lib::eval::material::mg(packed),
        pet_dragon_lib::eval::material::eg(packed)
    )
}
