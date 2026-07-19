// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/train_nnue.rs — NORU NNUE trainer (Phase 16.5)
//
// Consumes the row format shared by selfplay.rs and lichess_sample.rs:
//   stm_features | nstm_features | search_eval_cp | game_result_or_NA
// (space-separated feature indices in the first two fields; see those files'
// header comments for the exact encoding — this binary only parses the text,
// it never re-derives features itself.)
//
// D14 (DECISIONS.md): standard-chess Lichess rows have no game outcome
// (result field is the literal string "NA"), so the training target for
// every row is a `lambda`-weighted blend of the search-eval-derived target
// and the game-result target, falling back to eval-only when no result
// exists. Blend ratio is a CLI flag, not hardcoded, so this decision can be
// tuned from a single Colab cell without touching source.
//
// D53 (DECISIONS.md): the `1-lambda` game-result component is a hard 0/1
// (or 0.5 draw) label. Plain BCE-on-logits has no finite minimizer against
// a hard 0/1 target — the loss keeps decreasing as the pre-sigmoid logit is
// pushed toward +/-infinity, which is the actual mechanism behind D52's
// saturation finding (weight_decay/grad_clip_norm are a passive/reactive
// safety net on the resulting large gradients, not a fix for the underlying
// unbounded objective). `label_smoothing` maps the blended target from
// [0,1] into [label_smoothing, 1-label_smoothing] before computing loss, so
// the BCE minimizer corresponds to a finite logit again.
//
// NnueConfig uses NUM_FEATURES (896, D10) from nnue::features — this trainer
// never redefines the feature count, so it can't silently drift out of sync
// with extract_stm_nstm_features().
//
// Usage (from Colab, after installing a Rust toolchain — see
// colab/nnue_training.ipynb):
//   cargo run --release --bin train_nnue -- \
//       <output_prefix> <input_file1[,input_file2,...]> \
//       [epochs] [lr] [batch_size] [hidden_size] [accumulator_size] \
//       [lambda] [seed]
//
// Outputs:
//   <output_prefix>_fp32.bin       — TrainableWeights checkpoint (resumable)
//   <output_prefix>_quantized.bin  — NnueWeights i16 inference format
//                                     (this is what Phase 16.6 loads into eval)
// ============================================================================

use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::time::Instant;

use noru::config::{Activation, NnueConfig};
use noru::trainer::{AdamState, Gradients, SimpleRng, TrainableWeights, TrainingSample};

use pet_dragon_lib::nnue::features::{NUM_FEATURES, NUM_PIECE_SQUARE_FEATURES};

/// cp -> win-probability sigmoid slope. 400cp is the standard NNUE/Stockfish
/// convention for "one pawn of advantage ~= this much win probability slope";
/// kept as a named constant rather than a magic number in `target_from_row`.
const CP_TO_WINPROB_SCALE: f32 = 400.0;

/// One parsed training row before target blending.
struct Row {
    stm_features: Vec<usize>,
    nstm_features: Vec<usize>,
    eval_cp: i32,
    /// `None` for rows with no game outcome (Lichess "NA" rows, D14).
    result: Option<f32>,
}

/// Parse one `stm|nstm|eval|result` line. Returns `None` on any malformed
/// line (never panics on bad input — training data may contain a few dirty
/// rows from upstream generation and one bad line must not kill the run).
fn parse_line(line: &str) -> Option<Row> {
    let parts: Vec<&str> = line.trim().split('|').collect();
    if parts.len() != 4 {
        return None;
    }
    let parse_features = |s: &str| -> Option<Vec<usize>> {
        if s.is_empty() {
            return Some(Vec::new());
        }
        s.split_whitespace().map(|t| t.parse::<usize>().ok()).collect()
    };
    let stm_features = parse_features(parts[0])?;
    let nstm_features = parse_features(parts[1])?;
    let eval_cp: i32 = parts[2].parse().ok()?;
    let result: Option<f32> = if parts[3] == "NA" {
        None
    } else {
        Some(parts[3].parse().ok()?)
    };
    Some(Row { stm_features, nstm_features, eval_cp, result })
}

/// D14 target blend: `lambda` weight on the eval-derived target, `1-lambda`
/// on the game-result target. Falls back to eval-only when the row has no
/// result (Lichess rows), so `lambda` only matters for self-play rows.
///
/// D53: the blended target is then label-smoothed — mapped from `[0,1]`
/// into `[label_smoothing, 1-label_smoothing]` — before being handed to
/// BCE. This is applied uniformly (not only to hard-result rows) so
/// eval-only rows and blended rows share one target distribution; for
/// eval-only rows the effect is negligible since `eval_target` is already
/// rarely near the exact 0/1 boundary, but it's the `result` component that
/// actually reaches it (a decisive self-play game scores exactly 1.0/0.0),
/// which is what D52 traced the saturation to.
fn target_from_row(row: &Row, lambda: f32, label_smoothing: f32) -> f32 {
    let eval_target = 1.0 / (1.0 + (-(row.eval_cp as f32) / CP_TO_WINPROB_SCALE).exp());
    let blended = match row.result {
        Some(r) => lambda * eval_target + (1.0 - lambda) * r,
        None => eval_target,
    };
    blended * (1.0 - 2.0 * label_smoothing) + label_smoothing
}

/// Load and parse every row from every comma-separated input file, printing
/// per-file counts and skip counts to stderr so a bad input path or format
/// drift is visible immediately rather than silently producing an empty
/// training set.
fn load_rows(input_files: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for path in input_files.split(',') {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        let file = File::open(path).unwrap_or_else(|e| panic!("failed to open {path}: {e}"));
        let reader = BufReader::new(file);
        let mut kept = 0usize;
        let mut skipped = 0usize;
        for line in reader.lines() {
            // A read error here (most commonly invalid UTF-8 in a truncated
            // final line, e.g. from a process killed mid-write by a Kaggle
            // session time limit) is treated as a single bad row, not a
            // fatal error — an interrupted writer producing one garbled
            // tail line says nothing about the integrity of the rest of
            // the file, and panicking here would discard every good row
            // already read over one recoverable tail issue.
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("{path}: skipping unreadable line ({e})");
                    skipped += 1;
                    continue;
                }
            };
            match parse_line(&line) {
                Some(row) => {
                    kept += 1;
                    rows.push(row);
                }
                None => skipped += 1,
            }
        }
        eprintln!("{path}: {kept} rows kept, {skipped} skipped (malformed)");
    }
    rows
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage: train_nnue <output_prefix> <input_file1[,input_file2,...]> \
             [epochs=10] [lr=0.001] [batch_size=256] [hidden_size=32] \
             [accumulator_size=256] [lambda=0.7] [seed=42] [weight_decay=0.01] \
             [grad_clip_norm=1.0] [label_smoothing=0.03] [phase_balance_cap=4]"
        );
        std::process::exit(1);
    }
    let output_prefix = &args[1];
    let input_files = &args[2];
    let epochs: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let lr: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.001);
    let batch_size: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(256);
    let hidden_size: usize = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(32);
    let accumulator_size: usize = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(256);
    let lambda: f32 = args.get(8).and_then(|s| s.parse().ok()).unwrap_or(0.7);
    let seed: u64 = args.get(9).and_then(|s| s.parse().ok()).unwrap_or(42);
    // D30/D31: decoupled (AdamW-style) weight decay, applied every batch
    // step directly to the weight tensors (not biases). D33: 1e-4 was
    // confirmed insufficient — the corrected in-distribution eval_diag
    // (real Position::generate_with_seed positions, not the flawed
    // classic-chess test) showed seed=2/seed=3 both fully saturated at the
    // 1500cp clamp ceiling even WITH weight_decay=1e-4 applied. Raised
    // default 100x. Still not empirically tuned past this one jump — may
    // need to go higher again.
    let weight_decay: f32 = args.get(10).and_then(|s| s.parse().ok()).unwrap_or(1e-2);
    // D33: global-norm gradient clipping, applied to the raw gradient
    // BEFORE the Adam step (standard combination alongside weight decay —
    // decay bounds steady-state weight magnitude, clipping bounds how far
    // any single batch can push it). Added because weight decay alone,
    // even at a meaningfully large value, may not be enough by itself:
    // decay only pulls weights toward zero passively each step, while
    // clipping directly caps the update that caused the blowup in the
    // first place.
    let grad_clip_norm: f32 = args.get(11).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    // D53: label smoothing on the blended target. 0.03 maps the hard 0/1
    // game-result endpoints to [0.03, 0.97] — chosen as a conventional
    // starting value (Stockfish/Ethereal-style trainers commonly use
    // 0.01-0.05); not yet empirically tuned past this first value. Set to
    // 0.0 to fully disable (reproduces pre-D53 behavior exactly).
    let label_smoothing: f32 = args.get(12).and_then(|s| s.parse().ok()).unwrap_or(0.03);
    // D57: max oversampling multiplier for phase-balanced training order.
    // 1 disables entirely (every row appears exactly once, original
    // behavior) — default 4 caps duplication so a single very-rare
    // activation-count bucket can't blow up epoch time unpredictably.
    let phase_balance_cap: usize = args.get(13).and_then(|s| s.parse().ok()).unwrap_or(4);

    eprintln!(
        "config: epochs={epochs} lr={lr} batch_size={batch_size} hidden_size={hidden_size} \
         accumulator_size={accumulator_size} lambda={lambda} seed={seed} \
         weight_decay={weight_decay} grad_clip_norm={grad_clip_norm} \
         label_smoothing={label_smoothing} phase_balance_cap={phase_balance_cap}"
    );

    let rows = load_rows(input_files);
    assert!(!rows.is_empty(), "no training rows loaded — check input file paths/format");
    eprintln!("total rows loaded: {}", rows.len());

    // Precompute blended targets once (row data itself is immutable after this).
    let samples: Vec<TrainingSample> = rows
        .iter()
        .map(|row| TrainingSample {
            stm_features: row.stm_features.clone(),
            nstm_features: row.nstm_features.clone(),
            target: target_from_row(row, lambda, label_smoothing),
            dense_input: Vec::new(),
        })
        .collect();

    // Held-out validation split: last 5% after a seeded shuffle, so both
    // splits are representative rather than an artifact of file ordering
    // (selfplay games are appended sequentially, so an unshuffled tail split
    // would validate on only the last few games).
    let mut rng = SimpleRng::new(seed);
    let mut indices: Vec<usize> = (0..samples.len()).collect();
    for i in (1..indices.len()).rev() {
        let j = rng.next_usize(i + 1);
        indices.swap(i, j);
    }
    let val_count = (samples.len() / 20).max(1).min(samples.len() - 1);
    let (train_idx, val_idx) = indices.split_at(samples.len() - val_count);
    eprintln!("train rows: {}, validation rows: {}", train_idx.len(), val_idx.len());

    // D57: phase-balanced oversampling. Self-play rows are recorded every
    // ply, but D11's pawn-start features stay active only for the first
    // few plies of a game before permanently decaying, so rows resembling
    // "near the start of a game" are proportionally rare relative to rows
    // where most pawns have already moved — even though every game has
    // exactly one such row. This was found while diagnosing the D53-D55
    // calibration sweep (the near-symmetric random-start eval_diag cases
    // were the most persistently miscalibrated across every checkpoint).
    // Fixes the imbalance by oversampling rare-activation-count rows in
    // the training order — features.rs is untouched, no schema change, the
    // currently-committed network stays loadable throughout. Inverse-sqrt-
    // frequency weight, expressed as integer duplication of a row's index,
    // normalized so the average row still appears ~once (keeps epoch cost
    // predictable). `phase_balance_cap=1` fully disables (original
    // unweighted behavior, every row exactly once).
    fn phase_balanced_train_order(
        samples: &[TrainingSample],
        train_idx: &[usize],
        cap: usize,
    ) -> Vec<usize> {
        if cap <= 1 {
            return train_idx.to_vec();
        }
        let activation_count = |i: usize| -> usize {
            samples[i]
                .stm_features
                .iter()
                .filter(|&&idx| idx >= NUM_PIECE_SQUARE_FEATURES)
                .count()
        };
        // D11 range is 0..=16 active pawn-start features per row.
        let mut histogram = [0usize; 17];
        for &i in train_idx {
            histogram[activation_count(i)] += 1;
        }
        let raw_weights: Vec<f32> = train_idx
            .iter()
            .map(|&i| 1.0 / (histogram[activation_count(i)] as f32).sqrt())
            .collect();
        let mean_weight = raw_weights.iter().sum::<f32>() / raw_weights.len() as f32;

        let mut balanced = Vec::with_capacity(train_idx.len());
        for (pos, &i) in train_idx.iter().enumerate() {
            let normalized = raw_weights[pos] / mean_weight;
            let replicate = (normalized.round() as usize).clamp(1, cap);
            for _ in 0..replicate {
                balanced.push(i);
            }
        }
        eprintln!(
            "phase_balance: activation-count histogram (train split) = {histogram:?}"
        );
        eprintln!(
            "phase_balance: balanced train rows = {} (from {}, cap={cap})",
            balanced.len(),
            train_idx.len()
        );
        balanced
    }

    let config = NnueConfig::new_owned(
        NUM_FEATURES,
        accumulator_size,
        vec![hidden_size],
        Activation::SCReLU,
    );

    let mut weights = TrainableWeights::init_random(config.clone(), &mut rng);
    let mut state = AdamState::new(config.clone());

    // Decoupled weight decay (AdamW-style): shrinks every weight tensor by
    // `(1 - lr * weight_decay)` after each Adam step, applied to weights
    // only, never biases. noru::trainer::TrainableWeights exposes all
    // tensors as `pub` fields (verified against noru 2.2.0 source — no
    // built-in weight_decay param on `adam_update` itself), so this is
    // implemented here rather than assuming crate support that doesn't
    // exist.
    fn apply_weight_decay(w: &mut TrainableWeights, lr: f32, weight_decay: f32) {
        if weight_decay <= 0.0 {
            return;
        }
        let factor = 1.0 - lr * weight_decay;
        for row in w.ft_weight.iter_mut() {
            for v in row.iter_mut() {
                *v *= factor;
            }
        }
        for layer in w.hidden_weights.iter_mut() {
            for row in layer.iter_mut() {
                for v in row.iter_mut() {
                    *v *= factor;
                }
            }
        }
        for v in w.output_weight.iter_mut() {
            *v *= factor;
        }
        for row in w.dense_to_acc.iter_mut() {
            for v in row.iter_mut() {
                *v *= factor;
            }
        }
    }

    // D33: global-norm gradient clipping — computed over every gradient
    // tensor INCLUDING biases (unlike weight decay, clipping conventionally
    // covers the whole gradient, since the goal is bounding the update
    // step itself, not selectively shrinking weights toward zero).
    fn clip_gradients(g: &mut Gradients, max_norm: f32) {
        if max_norm <= 0.0 {
            return;
        }
        let mut sum_sq = 0.0f64;
        for row in &g.ft_weight {
            for &v in row {
                sum_sq += (v as f64) * (v as f64);
            }
        }
        for &v in &g.ft_bias {
            sum_sq += (v as f64) * (v as f64);
        }
        for layer in &g.hidden_weights {
            for row in layer {
                for &v in row {
                    sum_sq += (v as f64) * (v as f64);
                }
            }
        }
        for layer in &g.hidden_biases {
            for &v in layer {
                sum_sq += (v as f64) * (v as f64);
            }
        }
        for &v in &g.output_weight {
            sum_sq += (v as f64) * (v as f64);
        }
        sum_sq += (g.output_bias as f64) * (g.output_bias as f64);
        for row in &g.dense_to_acc {
            for &v in row {
                sum_sq += (v as f64) * (v as f64);
            }
        }

        let norm = sum_sq.sqrt() as f32;
        if norm <= max_norm || norm == 0.0 {
            return;
        }
        let scale = max_norm / norm;
        for row in g.ft_weight.iter_mut() {
            for v in row.iter_mut() {
                *v *= scale;
            }
        }
        for v in g.ft_bias.iter_mut() {
            *v *= scale;
        }
        for layer in g.hidden_weights.iter_mut() {
            for row in layer.iter_mut() {
                for v in row.iter_mut() {
                    *v *= scale;
                }
            }
        }
        for layer in g.hidden_biases.iter_mut() {
            for v in layer.iter_mut() {
                *v *= scale;
            }
        }
        for v in g.output_weight.iter_mut() {
            *v *= scale;
        }
        g.output_bias *= scale;
        for row in g.dense_to_acc.iter_mut() {
            for v in row.iter_mut() {
                *v *= scale;
            }
        }
    }

    let bce_loss = |weights: &TrainableWeights, idx: &[usize]| -> f32 {
        let mut total = 0.0f32;
        for &i in idx {
            let s = &samples[i];
            let fwd = weights.forward(&s.stm_features, &s.nstm_features, &[]);
            let p = fwd.sigmoid.clamp(1e-7, 1.0 - 1e-7);
            total += -(s.target * p.ln() + (1.0 - s.target) * (1.0 - p).ln());
        }
        total / idx.len() as f32
    };

    // Track the best-validation snapshot separately from the live training
    // state — a network trained past its generalization point (val_loss
    // rising while train_loss keeps falling) is the classic small-dataset
    // overfit failure mode, and shipping the *final* epoch unconditionally
    // would ship the most-overfit checkpoint rather than the best one.
    let mut best_val_loss = f32::INFINITY;
    let mut best_epoch = 0usize;
    let mut best_weights: Option<TrainableWeights> = None;

    let balanced_train_idx = phase_balanced_train_order(&samples, train_idx, phase_balance_cap);

    let start = Instant::now();
    for epoch in 0..epochs {
        let mut order = balanced_train_idx.clone();
        for i in (1..order.len()).rev() {
            let j = rng.next_usize(i + 1);
            order.swap(i, j);
        }

        for batch in order.chunks(batch_size) {
            let mut grad = Gradients::new(config.clone());
            for &i in batch {
                let s = &samples[i];
                let fwd = weights.forward(&s.stm_features, &s.nstm_features, &[]);
                weights.backward_bce(s, &fwd, &mut grad);
                }
            clip_gradients(&mut grad, grad_clip_norm);
            weights.adam_update(&grad, &mut state, lr, batch.len() as f32);
            apply_weight_decay(&mut weights, lr, weight_decay);
        }

        let train_loss = bce_loss(&weights, train_idx);
        let val_loss = bce_loss(&weights, val_idx);
        let marker = if val_loss < best_val_loss { " (new best)" } else { "" };
        eprintln!(
            "epoch {}/{epochs}: train_loss={train_loss:.5} val_loss={val_loss:.5} elapsed={:.1}s{marker}",
            epoch + 1,
            start.elapsed().as_secs_f32()
        );
        if val_loss < best_val_loss {
            best_val_loss = val_loss;
            best_epoch = epoch + 1;
            best_weights = Some(weights.clone());
        }
    }

    eprintln!("best epoch: {best_epoch}/{epochs} (val_loss={best_val_loss:.5}) — saving this checkpoint");
    let best_weights = best_weights.expect("at least one epoch must have run");

let fp32_path = format!("{output_prefix}_fp32.bin");
    fs::write(&fp32_path, best_weights.save_to_bytes()).expect("failed to write fp32 checkpoint");
    eprintln!("wrote {fp32_path}");

    let quantized = best_weights.quantize();
    let quant_path = format!("{output_prefix}_quantized.bin");
    fs::write(&quant_path, quantized.save_to_bytes()).expect("failed to write quantized weights");
    eprintln!("wrote {quant_path}");

}
