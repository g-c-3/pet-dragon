// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/selfplay.rs — NNUE training data generator (Phase 16.4a)
//
// Plays engine-vs-engine games from random Pet Dragon starting positions
// (D9/D14: single network learns Pet Dragon dynamics from self-play, plus
// standard-chess patterns from a separate Lichess CC0 dataset merged in at
// training time — Phase 16.4b covers that half of the data pipeline).
//
// Per position visited, records a NORU-shaped training row:
//   stm_features | nstm_features | search_eval_cp | game_result_from_stm
//
// `game_result_from_stm` is filled in once the game ends (1.0 win / 0.5 draw
// / 0.0 loss, from the perspective of whoever was on move in that position).
// Blending `search_eval_cp` (converted to a [0,1] target) with
// `game_result_from_stm` is a training-time decision (Phase 16.5, Colab) —
// this binary's job is only to produce both signals so that choice isn't
// foreclosed here.
//
// Usage (no terminal needed — triggered via GitHub Actions workflow_dispatch,
// see .github/workflows/selfplay.yml):
//   cargo run --release --bin selfplay -- <num_games> <output_path> [seed_start]
//
// Runtime is bounded per game (MAX_PLIES hard cap) so CI jobs finish
// predictably even if a search line never reaches a natural game end.
// ============================================================================

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::{is_checkmate, is_stalemate};
use pet_dragon_lib::nnue::features::extract_stm_nstm_features;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::search::iterative::iterative_deepening;
use pet_dragon_lib::search::time::TimeControl;
use pet_dragon_lib::search::SearchInfo;
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::{Color, Move};

/// Hard cap on plies per game — guarantees CI jobs terminate even if a line
/// never naturally reaches mate/stalemate/repetition/insufficient material.
const MAX_PLIES: usize = 300;

/// Fixed per-move search budget for self-play games. Kept short so a batch
/// of games finishes inside a single Actions job; search quality only needs
/// to be "good enough to not blunder constantly," not tournament-strength —
/// the network learns evaluation patterns, not move selection.
const MOVETIME_MS: u64 = 100;

/// Transposition table size for self-play search (small — short searches).
const TT_SIZE_MB: usize = 16;

/// One recorded position: features known immediately, `game_result` filled
/// in once the game that produced it has finished.
struct Sample {
    stm_features: Vec<usize>,
    nstm_features: Vec<usize>,
    search_eval_cp: i32,
    stm_color: Color,
    game_result: f32, // placeholder until backfilled after the game ends
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let num_games: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let output_path = args.get(2).cloned().unwrap_or_else(|| "selfplay_data.txt".to_string());
    let seed_start: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);

    let file = File::create(&output_path).expect("failed to create output file");
    let mut writer = BufWriter::new(file);

    for game_idx in 0..num_games {
        let seed = seed_start + game_idx;
        let samples = play_one_game(seed);
        for sample in &samples {
            write_sample(&mut writer, sample);
        }
        eprintln!(
            "game {}/{} (seed {}): {} samples written",
            game_idx + 1,
            num_games,
            seed,
            samples.len()
        );
    }

    writer.flush().expect("failed to flush output file");
}

/// Play one self-play game from a Pet Dragon random starting position
/// (seeded for reproducibility) and return every position visited as a
/// training sample with the final game result filled in.
fn play_one_game(seed: u64) -> Vec<Sample> {
    let mut pos = Position::generate_with_seed(seed);
    let mut pending: Vec<Sample> = Vec::with_capacity(MAX_PLIES);

    let mut tt = TranspositionTable::new(TT_SIZE_MB);
    let tc = TimeControl { movetime: MOVETIME_MS, ..Default::default() };

    let mut plies = 0usize;
    let result_for_white: f32;

    loop {
        if is_checkmate(&pos) {
            // Side to move is checkmated -> the other side (last mover) won.
            result_for_white = if pos.side_to_move == Color::White { 0.0 } else { 1.0 };
            break;
        }
        if is_stalemate(&pos) || pos.is_insufficient_material() || pos.is_threefold_repetition() {
            result_for_white = 0.5;
            break;
        }
        if plies >= MAX_PLIES {
            result_for_white = 0.5; // treat unresolved long games as a draw
            break;
        }

        let (stm_features, nstm_features) = extract_stm_nstm_features(&pos);
        let stm_color = pos.side_to_move;

        let mut info = SearchInfo::new();
        info.print_info = false; // silent — see SearchInfo doc comment (Session 39)
        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        if result.best_move == Move::NULL {
            // No legal move but not flagged checkmate/stalemate above (should
            // not happen given the checks run first) — bail out defensively,
            // discarding this position since it has no valid continuation.
            result_for_white = 0.5;
            break;
        }

        pending.push(Sample {
            stm_features,
            nstm_features,
            search_eval_cp: result.score,
            stm_color,
            game_result: 0.0, // backfilled below once result_for_white is known
        });

        pos.make_move_with_history(result.best_move);
        plies += 1;
    }

    for sample in &mut pending {
        sample.game_result = match sample.stm_color {
            Color::White => result_for_white,
            Color::Black => 1.0 - result_for_white,
        };
    }

    pending
}

/// Write one line: space-separated stm feature indices, `|`, space-separated
/// nstm feature indices, `|`, raw search eval (cp, from stm perspective),
/// `|`, game result from stm perspective (0.0 / 0.5 / 1.0).
fn write_sample(writer: &mut impl Write, sample: &Sample) {
    let stm_str: Vec<String> = sample.stm_features.iter().map(|i| i.to_string()).collect();
    let nstm_str: Vec<String> = sample.nstm_features.iter().map(|i| i.to_string()).collect();
    writeln!(
        writer,
        "{}|{}|{}|{}",
        stm_str.join(" "),
        nstm_str.join(" "),
        sample.search_eval_cp,
        sample.game_result,
    )
    .expect("failed to write sample line");
}
