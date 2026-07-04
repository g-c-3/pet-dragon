// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/lichess_sample.rs — Standard-chess training data sampler (Phase 16.4b)
//
// Streams the Lichess CC0-1.0 evaluation dataset
// (https://database.lichess.org/lichess_db_eval.jsonl.zst, 388.4M positions,
// updated 2026-06-04) directly over HTTP, decompresses it on the fly, and
// writes out a bounded sample in the same NORU-shaped row format selfplay.rs
// produces — so Phase 16.5 (Colab training) can concatenate both files
// (D14: self-play + Lichess CC0 standard-chess data in one training set).
//
// ── Why this isn't a uniform random sample of the full file ─────────────────
// Standard .zst frames are NOT byte-seekable to an arbitrary decompressed
// offset — decompression is strictly sequential from the start of the frame
// (per Session 24 handoff; confirmed against ruzstd's docs, which only
// expose a sequential io::Read). True reservoir sampling across all 388M
// positions would require decompressing the *entire* multi-GB file, which a
// GitHub Actions job cannot do in reasonable CI time/bandwidth.
//
// Pragmatic approximation used instead (Session 24 handoff option "b"):
//   1. Skip the first `skip_lines` decompressed lines (unparsed, cheap).
//   2. Keep every `stride`-th line after that, parse it, write a sample.
//   3. Stop — and drop the HTTP connection — as soon as `sample_size`
//      samples have been written.
// This only ever decompresses a *prefix* of the file
// (skip_lines + stride * sample_size lines), not the whole thing, and the
// connection is closed early so bytes past that point are never downloaded.
// It is NOT a uniform sample of the full 388M-position dataset — it samples
// a spread-out prefix. Good enough for bootstrapping NNUE training data
// (D14); revisit if training shows a bias traceable to file ordering.
//
// ── Format written (matches selfplay.rs's write_sample) ─────────────────────
//   <stm feature indices>|<nstm feature indices>|<eval_cp from stm>|<result>
// `result` is always the literal string "NA" here: the Lichess eval dataset
// has no game outcome, only an engine evaluation. Phase 16.5's Colab loader
// must treat "NA" rows as eval-only targets (skip game-result blending for
// them) — this is a training-time decision (D14), not one to bake in here.
//
// Usage (workflow_dispatch, no terminal needed — see
// .github/workflows/lichess_sample.yml):
//   cargo run --release --features lichess-sample --bin lichess_sample -- \
//       [url] [output_path] [skip_lines] [sample_size] [stride]
// All arguments optional; defaults are conservative for a single CI run.
// ============================================================================

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};

use pet_dragon_lib::nnue::features::extract_stm_nstm_features;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::types::Color;

use ruzstd::decoding::StreamingDecoder;

/// Default dataset URL — CC0-1.0, confirmed live in Session 24.
const DEFAULT_URL: &str = "https://database.lichess.org/lichess_db_eval.jsonl.zst";

/// Default: don't skip any prefix (start from the beginning of the file).
const DEFAULT_SKIP_LINES: u64 = 0;

/// Default sample size — conservative for a single CI run's time/bandwidth budget.
const DEFAULT_SAMPLE_SIZE: u64 = 50_000;

/// Default stride — keep 1 line in every 200, so the kept sample spreads
/// across roughly 10M decompressed lines rather than being a dense head-only slice.
const DEFAULT_STRIDE: u64 = 200;

/// Large-magnitude centipawn stand-in for a "mate in N" eval, monotonic in N
/// so closer mates still sort as more extreme than farther ones. Chosen well
/// below our engine's own MATE_SCORE (999_999, see ENGINE_ARCHITECTURE.md)
/// so these rows are never confused with actual search mate scores downstream.
const MATE_CP_BASE: i32 = 100_000;

fn mate_to_cp(mate_in: i32) -> i32 {
    if mate_in >= 0 {
        MATE_CP_BASE - mate_in
    } else {
        -MATE_CP_BASE - mate_in
    }
}

/// Pick the best-depth eval entry from a `"evals"` array and return its
/// eval in centipawns from White's perspective (Lichess's own convention).
/// Returns `None` if the array is empty or malformed.
fn best_eval_cp_white(evals: &serde_json::Value) -> Option<i32> {
    let arr = evals.as_array()?;
    let best = arr.iter().max_by_key(|e| e.get("depth").and_then(|d| d.as_i64()).unwrap_or(0))?;
    let pv0 = best.get("pvs")?.as_array()?.first()?;
    if let Some(cp) = pv0.get("cp").and_then(|v| v.as_i64()) {
        Some(cp as i32)
    } else if let Some(mate) = pv0.get("mate").and_then(|v| v.as_i64()) {
        Some(mate_to_cp(mate as i32))
    } else {
        None
    }
}

/// Lichess's exported FEN is missing halfmove/fullmove counters (4 fields,
/// not 6). Pad them so `Position::from_fen` (which expects the full 6-field
/// standard format) accepts it. Returns `None` if the field count is
/// otherwise unrecognised.
fn normalise_fen(fen: &str) -> Option<String> {
    let field_count = fen.split_whitespace().count();
    match field_count {
        4 => Some(format!("{} 0 1", fen)),
        6 => Some(fen.to_string()),
        _ => None,
    }
}

fn main() {
    pet_dragon_lib::bitboard::masks::init_masks();
    pet_dragon_lib::bitboard::magic::init_magic();
    pet_dragon_lib::position::zobrist::init_zobrist();

    let args: Vec<String> = env::args().collect();
    let url = args.get(1).cloned().unwrap_or_else(|| DEFAULT_URL.to_string());
    let output_path = args.get(2).cloned().unwrap_or_else(|| "lichess_sample.txt".to_string());
    let skip_lines: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SKIP_LINES);
    let sample_size: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_SAMPLE_SIZE);
    let stride: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_STRIDE).max(1);

    eprintln!(
        "lichess_sample: url={url} skip_lines={skip_lines} sample_size={sample_size} stride={stride}"
    );

    let response = reqwest::blocking::get(&url).expect("HTTP request failed");
    if !response.status().is_success() {
        panic!("HTTP request returned status {}", response.status());
    }

    let decoder = StreamingDecoder::new(response).expect("failed to init zstd stream");
    let mut reader = BufReader::new(decoder);

    let out_file = File::create(&output_path).expect("failed to create output file");
    let mut writer = BufWriter::new(out_file);

    let mut line = String::new();
    let mut line_index: u64 = 0;
    let mut kept: u64 = 0;
    let mut parse_failures: u64 = 0;

    loop {
        if kept >= sample_size {
            break;
        }
        line.clear();
        let bytes_read = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("stream read error at line {line_index}: {e} — stopping early");
                break;
            }
        };
        if bytes_read == 0 {
            eprintln!("reached end of stream at line {line_index} (dataset exhausted before sample_size reached)");
            break;
        }
        line_index += 1;

        if line_index <= skip_lines {
            continue;
        }
        if (line_index - skip_lines - 1) % stride != 0 {
            continue;
        }

        match process_line(&line) {
            Some(sample_line) => {
                writer.write_all(sample_line.as_bytes()).expect("write failed");
                kept += 1;
            }
            None => parse_failures += 1,
        }

        if kept % 5_000 == 0 && kept > 0 {
            eprintln!("progress: {kept}/{sample_size} samples kept, line {line_index}");
        }
    }

    writer.flush().expect("failed to flush output file");
    // `reader` (and the underlying HTTP response) drops here, closing the
    // connection — bytes beyond this point in the dataset are never fetched.

    eprintln!(
        "done: {kept} samples written to {output_path}, {parse_failures} lines failed to parse, {line_index} lines read total"
    );
}

/// Parse one JSONL line into our sample row format, or `None` if the line
/// is malformed, has no usable eval, or fails to parse as a Pet Dragon
/// `Position` (standard chess is always a valid Pet Dragon position — see
/// PROJECT_CONTEXT.md — so a failure here indicates a genuinely bad FEN).
fn process_line(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let fen_raw = value.get("fen")?.as_str()?;
    let fen = normalise_fen(fen_raw)?;
    let eval_cp_white = best_eval_cp_white(value.get("evals")?)?;

    let pos = Position::from_fen(&fen).ok()?;
    let (stm_features, nstm_features) = extract_stm_nstm_features(&pos);

    // Lichess evals are from White's perspective; our sample format (like
    // selfplay.rs) records eval from the side-to-move's perspective.
    let eval_cp_stm = if pos.side_to_move == Color::White {
        eval_cp_white
    } else {
        -eval_cp_white
    };

    let stm_str: Vec<String> = stm_features.iter().map(|i| i.to_string()).collect();
    let nstm_str: Vec<String> = nstm_features.iter().map(|i| i.to_string()).collect();

    Some(format!(
        "{}|{}|{}|NA\n",
        stm_str.join(" "),
        nstm_str.join(" "),
        eval_cp_stm,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mate_to_cp_monotonic_in_distance() {
        assert!(mate_to_cp(1) > mate_to_cp(5));
        assert!(mate_to_cp(-1) < mate_to_cp(-5));
        assert!(mate_to_cp(1) > 0);
        assert!(mate_to_cp(-1) < 0);
    }

    #[test]
    fn test_normalise_fen_pads_four_fields() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";
        let out = normalise_fen(fen).unwrap();
        assert_eq!(out.split_whitespace().count(), 6);
        assert!(out.ends_with("0 1"));
    }

    #[test]
    fn test_normalise_fen_leaves_six_fields() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        assert_eq!(normalise_fen(fen).unwrap(), fen);
    }

    #[test]
    fn test_normalise_fen_rejects_bad_field_count() {
        assert!(normalise_fen("rnbqkbnr w KQkq").is_none());
    }

    #[test]
    fn test_process_line_cp_eval() {
        pet_dragon_lib::bitboard::masks::init_masks();
        pet_dragon_lib::bitboard::magic::init_magic();
        pet_dragon_lib::position::zobrist::init_zobrist();
        let line = r#"{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -","evals":[{"pvs":[{"cp":25}],"knodes":100,"depth":20}]}"#;
        let out = process_line(line).unwrap();
        let parts: Vec<&str> = out.trim().split('|').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[2], "25");
        assert_eq!(parts[3], "NA");
    }

    #[test]
    fn test_process_line_prefers_highest_depth() {
        pet_dragon_lib::bitboard::masks::init_masks();
        pet_dragon_lib::bitboard::magic::init_magic();
        pet_dragon_lib::position::zobrist::init_zobrist();
        let line = r#"{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -","evals":[{"pvs":[{"cp":10}],"depth":10},{"pvs":[{"cp":40}],"depth":30}]}"#;
        let out = process_line(line).unwrap();
        let parts: Vec<&str> = out.trim().split('|').collect();
        assert_eq!(parts[2], "40");
    }

    #[test]
    fn test_process_line_black_to_move_negates_eval() {
        pet_dragon_lib::bitboard::masks::init_masks();
        pet_dragon_lib::bitboard::magic::init_magic();
        pet_dragon_lib::position::zobrist::init_zobrist();
        let line = r#"{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq -","evals":[{"pvs":[{"cp":30}],"depth":20}]}"#;
        let out = process_line(line).unwrap();
        let parts: Vec<&str> = out.trim().split('|').collect();
        assert_eq!(parts[2], "-30");
    }

    #[test]
    fn test_process_line_malformed_json_returns_none() {
        assert!(process_line("not json").is_none());
    }

    #[test]
    fn test_process_line_missing_evals_returns_none() {
        let line = r#"{"fen":"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -"}"#;
        assert!(process_line(line).is_none());
    }
}
