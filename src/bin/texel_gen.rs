// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/texel_gen.rs — Texel tuning game database generator (Phase 14.2)
//
// Plays engine-vs-engine games from random Pet Dragon starting positions
// (Position::generate_with_seed — same generator selfplay.rs/match_runner.rs
// use, per D32: real games never start from a classic-chess-style layout,
// so training/tuning data must be drawn from the actual distribution).
//
// Unlike selfplay.rs (which records NNUE feature vectors + search-eval
// targets), this binary records the two things classic Texel tuning needs:
// a FEN string and the FINAL GAME RESULT from that position's side-to-move
// perspective (1.0 win / 0.5 draw / 0.0 loss) — no search-eval label
// involved at all. Tuning fits HCE's own weights against real game
// outcomes, not against another eval's opinion.
//
// Sampling policy (standard Texel tuning practice, not arbitrary):
//   - Skip the first SKIP_OPENING_PLIES of each game — early-opening
//     positions from a random Pet Dragon start are highly noisy/arbitrary
//     and not informative for weight fitting.
//   - Skip any position where the side to move is in check — tactical,
//     forcing positions distort static-eval fitting (the whole point of
//     Texel tuning is fitting the STATIC evaluation, not search).
//   - Sample every SAMPLE_STRIDE plies among the remaining quiet positions,
//     not every single ply — reduces redundancy from near-duplicate
//     consecutive positions in the same game.
//
// Usage (no terminal needed — triggered via GitHub Actions workflow_dispatch,
// see .github/workflows/texel_gen.yml):
//   cargo run --release --bin texel_gen -- <num_games> <output_path> [seed_start]
// ============================================================================

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::{is_checkmate, is_stalemate};
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::search::iterative::iterative_deepening;
use pet_dragon_lib::search::time::TimeControl;
use pet_dragon_lib::search::SearchInfo;
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::{Color, Move};

/// Hard cap on plies per game — same rationale as selfplay.rs (D-series):
/// guarantees CI jobs terminate even if a line never naturally reaches
/// mate/stalemate/repetition/insufficient material.
const MAX_PLIES: usize = 300;

/// Fixed per-move search budget. Kept short for the same reason as
/// selfplay.rs — CI job time budget, not tournament-strength play; the
/// tuner needs quiet-position/result PAIRS, not strong move selection.
const MOVETIME_MS: u64 = 100;

/// Transposition table size for game-generation search (small, short searches).
const TT_SIZE_MB: usize = 16;

/// Plies to skip at the start of every game before any position becomes
/// eligible for sampling — random Pet Dragon starts are far more chaotic
/// than a classic chess opening, so this is deliberately generous.
const SKIP_OPENING_PLIES: usize = 12;

/// Sample every Nth eligible (quiet, post-opening) ply, not every single one.
const SAMPLE_STRIDE: usize = 4;

/// One recorded position: FEN known immediately, `game_result` filled in
/// once the game that produced it has finished.
struct Sample {
    fen: String,
    stm_color: Color,
    game_result: f32, // placeholder until backfilled after the game ends
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let num_games: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let output_path = args.get(2).cloned().unwrap_or_else(|| "texel_data.txt".to_string());
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

/// Play one game from a random Pet Dragon starting position (seeded for
/// reproducibility) and return every ELIGIBLE position visited (post-
/// opening, quiet, on-stride) as a tuning sample with the final game
/// result filled in.
fn play_one_game(seed: u64) -> Vec<Sample> {
    let mut pos = Position::generate_with_seed(seed);
    let mut pending: Vec<Sample> = Vec::new();

    let mut tt = TranspositionTable::new(TT_SIZE_MB);
    let tc = TimeControl { movetime: MOVETIME_MS, ..Default::default() };

    let mut plies = 0usize;
    let mut since_last_sample = 0usize;
    let result_for_white: f32;

    loop {
        if is_checkmate(&pos) {
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

        let eligible = plies >= SKIP_OPENING_PLIES
            && !pos.in_check(pos.side_to_move)
            && since_last_sample >= SAMPLE_STRIDE;

        if eligible {
            pending.push(Sample {
                fen: pos.to_fen(),
                stm_color: pos.side_to_move,
                game_result: 0.0, // backfilled below once result_for_white is known
            });
            since_last_sample = 0;
        } else {
            since_last_sample += 1;
        }

        let mut info = SearchInfo::new();
        info.print_info = false; // silent — see SearchInfo doc comment (D28)
        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        if result.best_move == Move::NULL {
            // Defensive bail-out — should not happen given the mate/stalemate
            // checks above run first, but never emit a position with no
            // valid continuation.
            result_for_white = 0.5;
            break;
        }

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

/// Write one line: `<FEN>|<game_result>` (result already oriented to the
/// FEN's own side-to-move perspective, matching selfplay.rs's convention).
fn write_sample(writer: &mut impl Write, sample: &Sample) {
    writeln!(writer, "{}|{}", sample.fen, sample.game_result)
        .expect("failed to write sample line");
}
