// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/match_runner.rs — Elo A/B match harness (Phase 17.2)
//
// Plays engine-vs-engine games between two configurations of the SAME
// binary, differing only in NNUE blend weight (Phase 17.1's
// `eval::set_nnue_weight_pct`). Reports win/loss/draw counts from "Engine A"'s
// perspective and a simple Elo delta estimate — this is the concrete Elo
// test D23 flagged as the trigger for ever raising the fixed 25% blend
// weight beyond its current conservative default.
//
// Colors alternate every game (Engine A plays White on even game indices,
// Black on odd) so neither configuration benefits from a first-move
// advantage over the match as a whole. Each engine gets its own
// TranspositionTable per game — sharing a single TT between two configs
// with different blend weights would let one config's stale scores leak
// into the other's search, since a score frozen in a TT entry is only
// valid for the evaluator that produced it.
//
// Usage (no terminal needed — triggered via GitHub Actions workflow_dispatch,
// see .github/workflows/match_runner.yml, Phase 17.3):
//   cargo run --release --bin match_runner -- \
//       <num_games> <weight_a_pct> <weight_b_pct> [movetime_ms] [seed_start] [output_path]
//
// Defaults: 20 games, A=0% (pure HCE), B=25% (D23 default), 100ms/move,
// seed_start=0, output_path="match_results.txt".
// ============================================================================

use std::env;
use std::fs::File;
use std::io::Write;

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::eval::set_nnue_weight_pct;
use pet_dragon_lib::movegen::{is_checkmate, is_stalemate};
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::search::iterative::iterative_deepening;
use pet_dragon_lib::search::time::TimeControl;
use pet_dragon_lib::search::SearchInfo;
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::{Color, Move};

/// Hard cap on plies per game — guarantees CI jobs terminate even if a line
/// never naturally reaches mate/stalemate/repetition/insufficient material.
/// Same value as `selfplay.rs` (Phase 16.4a) for consistency.
const MAX_PLIES: usize = 300;

/// Per-side transposition table size. Small and short-lived (fresh per
/// game per side) — this harness prioritises throughput (many games) over
/// single-game search strength.
const TT_SIZE_MB: usize = 16;

/// Outcome of one game from Engine A's perspective.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameOutcome {
    WinA,
    WinB,
    Draw,
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let num_games: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let weight_a: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let weight_b: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(25);
    let movetime_ms: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(100);
    let seed_start: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let output_path = args
        .get(6)
        .cloned()
        .unwrap_or_else(|| "match_results.txt".to_string());

    let mut wins_a = 0u64;
    let mut wins_b = 0u64;
    let mut draws = 0u64;

    for game_idx in 0..num_games {
        let seed = seed_start + game_idx;
        // Engine A plays White on even game indices, Black on odd — cancels
        // out first-move advantage over the match as a whole.
        let a_plays_white = game_idx % 2 == 0;
        let outcome = play_one_game(seed, weight_a, weight_b, a_plays_white, movetime_ms);

        match outcome {
            GameOutcome::WinA => wins_a += 1,
            GameOutcome::WinB => wins_b += 1,
            GameOutcome::Draw => draws += 1,
        }

        eprintln!(
            "game {}/{} (seed {}, A={}): {}",
            game_idx + 1,
            num_games,
            seed,
            if a_plays_white { "White" } else { "Black" },
            match outcome {
                GameOutcome::WinA => "A wins",
                GameOutcome::WinB => "B wins",
                GameOutcome::Draw => "draw",
            }
        );
    }

    let summary = format_summary(num_games, weight_a, weight_b, wins_a, wins_b, draws);
    println!("{summary}");

    let mut file = File::create(&output_path).expect("failed to create output file");
    file.write_all(summary.as_bytes())
        .expect("failed to write summary file");
}

/// Play one game between Engine A (blend weight `weight_a`) and Engine B
/// (blend weight `weight_b`) from a seeded Pet Dragon random starting
/// position. `a_plays_white` fixes which physical color Engine A controls
/// for this game. Returns the outcome from Engine A's perspective.
fn play_one_game(
    seed: u64,
    weight_a: u32,
    weight_b: u32,
    a_plays_white: bool,
    movetime_ms: u64,
) -> GameOutcome {
    let mut pos = Position::generate_with_seed(seed);
    pos.push_game_history();

    // Separate TT per color-for-this-game — never shared between the two
    // differently-weighted evaluators (see module doc comment).
    let mut tt_white = TranspositionTable::new(TT_SIZE_MB);
    let mut tt_black = TranspositionTable::new(TT_SIZE_MB);
    let tc = TimeControl {
        movetime: movetime_ms,
        ..Default::default()
    };

    let mut plies = 0usize;

    loop {
        if is_checkmate(&pos) {
            // Side to move is checkmated -> the other side (last mover) won.
            let white_won = pos.side_to_move == Color::Black;
            return outcome_from_white_result(white_won, a_plays_white, false);
        }
        if is_stalemate(&pos) || pos.is_insufficient_material() || pos.is_threefold_repetition() {
            return GameOutcome::Draw;
        }
        if plies >= MAX_PLIES {
            // Unresolved long games are adjudicated as draws rather than
            // biasing the match toward whichever config runs out the clock.
            return GameOutcome::Draw;
        }

        let mover_is_a = pos.side_to_move == Color::White && a_plays_white
            || pos.side_to_move == Color::Black && !a_plays_white;
        set_nnue_weight_pct(if mover_is_a { weight_a } else { weight_b });

        let tt = if pos.side_to_move == Color::White {
            &mut tt_white
        } else {
            &mut tt_black
        };

        let mut info = SearchInfo::new();
        info.print_info = false; // silent — see SearchInfo doc comment (Session 39)
        let result = iterative_deepening(&mut pos, &tc, &mut info, tt);

        if result.best_move == Move::NULL {
            // No legal move but not flagged above — defensive bail-out,
            // treated as a draw rather than crashing the whole match.
            return GameOutcome::Draw;
        }

        pos.make_move_with_history(result.best_move);
        pos.push_game_history();
        plies += 1;
    }
}

/// Translate a checkmate result (`white_won`) into a `GameOutcome` from
/// Engine A's perspective, given which color A was playing this game.
/// The unused `_draw` parameter keeps the call site self-documenting about
/// what this function does NOT handle (draws are resolved at the call
/// site before reaching here).
fn outcome_from_white_result(white_won: bool, a_plays_white: bool, _draw: bool) -> GameOutcome {
    let a_won = white_won == a_plays_white;
    if a_won {
        GameOutcome::WinA
    } else {
        GameOutcome::WinB
    }
}

/// Convert a match score percentage (0.0-1.0) into an Elo difference
/// estimate using the standard logistic formula. Returns `None` at the
/// boundaries (0% or 100%) where the formula is undefined (infinite Elo
/// gap) — the caller should report those as "no losses/no wins" instead of
/// a number.
fn elo_diff_from_score(score_pct: f64) -> Option<f64> {
    if score_pct <= 0.0 || score_pct >= 1.0 {
        return None;
    }
    Some(-400.0 * ((1.0 / score_pct - 1.0).log10()))
}

/// Build the human-readable match summary written to stdout and the
/// output file.
fn format_summary(
    num_games: u64,
    weight_a: u32,
    weight_b: u32,
    wins_a: u64,
    wins_b: u64,
    draws: u64,
) -> String {
    let score_a = (wins_a as f64 + 0.5 * draws as f64) / num_games as f64;
    let elo_line = match elo_diff_from_score(score_a) {
        Some(diff) => format!("Elo diff (A vs B): {:+.1}", diff),
        None => "Elo diff (A vs B): undefined (a config had zero losses or zero wins)".to_string(),
    };

    format!(
        "Pet Dragon Elo A/B match (Phase 17.2)\n\
         Engine A: NNUEWeight={weight_a}%\n\
         Engine B: NNUEWeight={weight_b}%\n\
         Games: {num_games}\n\
         A wins: {wins_a}  B wins: {wins_b}  Draws: {draws}\n\
         A score: {:.1}%\n\
         {elo_line}\n",
        score_a * 100.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elo_diff_even_score_is_zero() {
        let diff = elo_diff_from_score(0.5).unwrap();
        assert!(diff.abs() < 0.01, "50% score should be ~0 Elo diff, got {diff}");
    }

    #[test]
    fn test_elo_diff_higher_score_is_positive() {
        let diff = elo_diff_from_score(0.75).unwrap();
        assert!(diff > 0.0, "above-50% score should be positive Elo diff, got {diff}");
    }

    #[test]
    fn test_elo_diff_undefined_at_boundaries() {
        assert!(elo_diff_from_score(0.0).is_none());
        assert!(elo_diff_from_score(1.0).is_none());
    }

    #[test]
    fn test_outcome_from_white_result_a_plays_white_and_wins() {
        let outcome = outcome_from_white_result(true, true, false);
        assert!(matches!(outcome, GameOutcome::WinA));
    }

    #[test]
    fn test_outcome_from_white_result_a_plays_black_and_white_wins() {
        // White won, but A was playing Black -> B wins.
        let outcome = outcome_from_white_result(true, false, false);
        assert!(matches!(outcome, GameOutcome::WinB));
    }

    #[test]
    fn test_play_one_game_terminates_and_returns_outcome() {
        init_masks();
        init_magic();
        init_zobrist();
        // Very short movetime keeps this test fast; just checking the game
        // loop terminates and produces a valid outcome, not searching quality.
        let outcome = play_one_game(0, 0, 25, true, 5);
        assert!(matches!(
            outcome,
            GameOutcome::WinA | GameOutcome::WinB | GameOutcome::Draw
        ));
    }
}
