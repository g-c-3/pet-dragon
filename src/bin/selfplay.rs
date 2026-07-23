// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/selfplay.rs — NNUE training data generator (Phase 16.4a),
// plus opening-statistics data collection (Phase 23.4, D67, Session 84)
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
// Additionally, one row per GAME (not per position) is written to a second,
// separate output file for Phase 23.4's bucketed opening statistics (D67):
//   starting_seed | rook_files | knight_files | root_move_uci | game_result
// `rook_files`/`knight_files` are read once, from White's starting setup,
// before any move is made — Pet Dragon's mirrored setup means Black's are
// identical, so recording White's is canonical (D67's v1 bucket key).
// `root_move_uci` is whichever move the engine actually played first;
// `game_result` is from the perspective of whoever moved first, same
// win/draw/loss backfill logic as the NNUE stream's `game_result_from_stm`.
// This is a genuinely separate, additive stream — it does not change the
// existing NNUE row format or its file, so existing consumers
// (texel_gen.rs, NNUE training, Phase 25's texel_tune.rs) are unaffected.
//
// Usage (no terminal needed — triggered via GitHub Actions workflow_dispatch,
// see .github/workflows/selfplay.yml):
//   cargo run --release --bin selfplay -- <num_games> <output_path> [seed_start] [opening_output_path]
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
use pet_dragon_lib::types::{Color, Move, PieceKind};

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

/// One recorded game, for Phase 23.4's bucketed opening statistics (D67).
/// `rook_files`/`knight_files` are 0..8 file indices, always exactly 2 each
/// (Pet Dragon's setup always places exactly 2 rooks and 2 knights).
struct OpeningSample {
    starting_seed: u64,
    rook_files: [u8; 2],
    knight_files: [u8; 2],
    root_move_uci: String,
    root_mover: Color,
    game_result: f32, // from root_mover's perspective, backfilled after the game ends
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let num_games: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let output_path = args.get(2).cloned().unwrap_or_else(|| "selfplay_data.txt".to_string());
    let seed_start: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let opening_output_path = args.get(4).cloned()
        .unwrap_or_else(|| "opening_data.txt".to_string());

    let file = File::create(&output_path).expect("failed to create output file");
    let mut writer = BufWriter::new(file);

    let opening_file = File::create(&opening_output_path)
        .expect("failed to create opening-stats output file");
    let mut opening_writer = BufWriter::new(opening_file);

    for game_idx in 0..num_games {
        let seed = seed_start + game_idx;
        let (samples, opening) = play_one_game(seed);
        for sample in &samples {
            write_sample(&mut writer, sample);
        }
        if let Some(opening) = &opening {
            write_opening_sample(&mut opening_writer, opening);
        }
        eprintln!(
            "game {}/{} (seed {}): {} samples written{}",
            game_idx + 1,
            num_games,
            seed,
            samples.len(),
            if opening.is_some() { "" } else { " (no opening sample — no legal root move)" },
        );
    }

    writer.flush().expect("failed to flush output file");
    opening_writer.flush().expect("failed to flush opening-stats output file");
}

/// Play one self-play game from a Pet Dragon random starting position
/// (seeded for reproducibility) and return every position visited as a
/// training sample with the final game result filled in, plus (D67) one
/// opening-statistics sample for the game as a whole — `None` only in the
/// rare case the starting position has no legal move at all (immediate
/// checkmate/stalemate at ply 0, so there's no root move to record).
fn play_one_game(seed: u64) -> (Vec<Sample>, Option<OpeningSample>) {
    let mut pos = Position::generate_with_seed(seed);
    let mut pending: Vec<Sample> = Vec::with_capacity(MAX_PLIES);

    // D67 — captured once, before any move is made. Pet Dragon's setup is
    // mirrored (VARIANT_ARCHITECTURE.md), so White's rook/knight files are
    // canonical for both colors; no need to also read Black's.
    let rook_files = sorted_files(pos.piece_bb(Color::White, PieceKind::Rook));
    let knight_files = sorted_files(pos.piece_bb(Color::White, PieceKind::Knight));
    let mut opening: Option<OpeningSample> = None;

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

        if plies == 0 {
            // D67 — this is the root move; game_result filled in below once
            // result_for_white is known, same backfill pattern as `Sample`.
            opening = Some(OpeningSample {
                starting_seed: seed,
                rook_files,
                knight_files,
                root_move_uci: result.best_move.to_uci(),
                root_mover: stm_color,
                game_result: 0.0,
            });
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
    if let Some(opening) = &mut opening {
        opening.game_result = match opening.root_mover {
            Color::White => result_for_white,
            Color::Black => 1.0 - result_for_white,
        };
    }

    (pending, opening)
}

/// Sorted (ascending) list of files (0..8) occupied by the given bitboard.
/// Used to extract D67's bucket key — panics if `bb` doesn't have exactly 2
/// bits set, since Pet Dragon's setup always places exactly 2 rooks and 2
/// knights; a mismatch here means `piece_bb` was called wrong, not a
/// legitimate position variation to silently tolerate.
fn sorted_files(bb: pet_dragon_lib::bitboard::Bitboard) -> [u8; 2] {
    let mut files: Vec<u8> = bb.map(|sq| sq.file()).collect();
    files.sort_unstable();
    files.try_into().expect("expected exactly 2 pieces (Pet Dragon setup invariant)")
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

/// Write one line for D67's opening-statistics stream: starting seed, `|`,
/// comma-separated rook files, `|`, comma-separated knight files, `|`, root
/// move in UCI, `|`, game result from the root mover's perspective.
fn write_opening_sample(writer: &mut impl Write, opening: &OpeningSample) {
    writeln!(
        writer,
        "{}|{},{}|{},{}|{}|{}",
        opening.starting_seed,
        opening.rook_files[0], opening.rook_files[1],
        opening.knight_files[0], opening.knight_files[1],
        opening.root_move_uci,
        opening.game_result,
    )
    .expect("failed to write opening-stats sample line");
}
