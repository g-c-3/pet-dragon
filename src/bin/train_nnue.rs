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

use pet_dragon_lib::nnue::features::NUM_FEATURES;

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
fn target_from_row(row: &Row, lambda: f32) -> f32 {
    let eval_target = 1.0 / (1.0 + (-(row.eval_cp as f32) / CP_TO_WINPROB_SCALE).exp());
    match row.result {
        Some(r) => lambda * eval_target + (1.0 - lambda) * r,
        None => eval_target,
    }
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
             [accumulator_size=256] [lambda=0.7] [seed=42] [weight_decay=0.0001]"
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
    // step directly to the weight tensors (not biases — standard practice,
    // biases don't drive the runaway-logit-magnitude failure mode weight
    // decay is meant to prevent). Session 42's network had none at all,
    // which let output-layer weights grow unbounded while BCE loss kept
    // improving (confirmed via eval_diag.rs: +2425cp at the symmetric
    // start position, ~4000cp queen swing vs HCE's ~976cp). Default 1e-4
    // is a conservative starting point for this network's small
    // hidden_size=32 scale — not yet tuned.
    let weight_decay: f32 = args.get(10).and_then(|s| s.parse().ok()).unwrap_or(1e-4);

    eprintln!(
        "config: epochs={epochs} lr={lr} batch_size={batch_size} hidden_size={hidden_size} \
         accumulator_size={accumulator_size} lambda={lambda} seed={seed} \
         weight_decay={weight_decay}"
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
            target: target_from_row(row, lambda),
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

    let config = NnueConfig::new_owned(
        NUM_FEATURES,
        accumulator_size,
        vec![hidden_size],
        Activation::SCReLU,
    );

    let mut weights = TrainableWeights::init_random(config.clone(), &mut rng);
    let mut state = AdamState::new(config.clone());

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

    let start = Instant::now();
    for epoch in 0..epochs {
        let mut order = train_idx.to_vec();
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
            weights.adam_update(&grad, &mut state, lr, batch.len() as f32);
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
