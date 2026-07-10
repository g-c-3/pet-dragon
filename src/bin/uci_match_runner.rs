// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/uci_match_runner.rs — real engine-vs-engine UCI match harness (D36)
//
// Unlike `match_runner.rs` (Phase 17.2), which A/Bs two configs of the SAME
// compiled binary in-process, this harness spawns TWO SEPARATE binaries as
// OS child processes and drives them with the standard UCI protocol over
// stdin/stdout — exactly how a real GUI/tournament manager talks to an
// engine. This is what makes it possible to measure a genuine pre/post-
// Texel-tuning Elo delta: point it at a binary built from a git ref BEFORE
// the tuning commit and a binary built from a ref AFTER, and the two
// binaries' compiled-in `eval/*.rs` constants are whatever they actually
// were at each ref — no in-process weight-swapping hack required.
//
// This harness does not itself search or evaluate anything. It only:
//   1. Generates a seeded Pet Dragon starting position (same generator
//      match_runner.rs/selfplay.rs use, for a fair/comparable sample).
//   2. Tracks the game locally (for legality + termination detection) using
//      the same movegen/position code as the rest of the engine.
//   3. Relays `position` / `go movetime` to whichever child process is on
//      move, and parses its `bestmove` reply.
//
// Usage (triggered via GitHub Actions workflow_dispatch, see
// .github/workflows/uci_match_runner.yml):
//   cargo run --release --bin uci_match_runner -- \
//       <engine_a_path> <engine_b_path> [num_games] [movetime_ms] \
//       [seed_start] [label_a] [label_b] [output_path]
//
// Defaults: 20 games, 100ms/move, seed_start=0, labels "Engine A"/"Engine B",
// output_path="uci_match_results.txt".
//
// Known limitation: if a child engine process hangs (never prints a
// `bestmove` line), this harness blocks indefinitely on that read — there is
// no per-move timeout. Acceptable for a manually-triggered, one-off
// measurement tool bounded by the CI job's own overall timeout; not
// acceptable if this were ever promoted to a routinely-scheduled job.
// ============================================================================

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::{generate_moves, is_checkmate, is_stalemate};
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::types::{Color, Move, PieceKind, Square};

/// Hard cap on plies per game — same convention as `match_runner.rs`
/// (Phase 17.2) and `selfplay.rs` (Phase 16.4a).
const MAX_PLIES: usize = 300;

/// Outcome of one game from Engine A's perspective.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameOutcome {
    WinA,
    WinB,
    Draw,
}

/// A running engine child process, wired for UCI over stdin/stdout.
struct EngineProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl EngineProcess {
    /// Spawn `path` as a child process with piped stdin/stdout and perform
    /// the standard `uci` -> `uciok`, `isready` -> `readyok` handshake.
    fn spawn(path: &str) -> Self {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn engine at '{path}': {e}"));

        let stdin = child.stdin.take().expect("child stdin was not piped");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout was not piped"));

        let mut engine = EngineProcess {
            child,
            stdin,
            stdout,
        };

        engine.send("uci");
        engine.wait_for_line_starting_with("uciok");
        engine.send("isready");
        engine.wait_for_line_starting_with("readyok");

        engine
    }

    /// Write one line + newline to the child's stdin.
    fn send(&mut self, cmd: &str) {
        writeln!(self.stdin, "{cmd}").expect("failed to write to engine stdin");
    }

    /// Block reading lines from the child's stdout until one starts with
    /// `prefix`, returning that full line.
    fn wait_for_line_starting_with(&mut self, prefix: &str) -> String {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self
                .stdout
                .read_line(&mut line)
                .expect("failed to read from engine stdout");
            if n == 0 {
                panic!("engine process closed stdout while waiting for '{prefix}'");
            }
            if line.trim_start().starts_with(prefix) {
                return line.trim().to_string();
            }
        }
    }

    /// Send `ucinewgame` + `isready`/`readyok`, resetting the engine's
    /// internal state before a new game.
    fn new_game(&mut self) {
        self.send("ucinewgame");
        self.send("isready");
        self.wait_for_line_starting_with("readyok");
    }

    /// Send the current position (base FEN + move list so far) and a
    /// `go movetime` command, then return the `bestmove` UCI string
    /// (e.g. "e2e4", "a7a8q", or "(none)").
    fn go_movetime(&mut self, base_fen: &str, moves_so_far: &[String], movetime_ms: u64) -> String {
        let pos_cmd = if moves_so_far.is_empty() {
            format!("position fen {base_fen}")
        } else {
            format!("position fen {base_fen} moves {}", moves_so_far.join(" "))
        };
        self.send(&pos_cmd);
        self.send(&format!("go movetime {movetime_ms}"));

        let line = self.wait_for_line_starting_with("bestmove");
        // "bestmove e2e4" or "bestmove e2e4 ponder e7e5"
        line.split_whitespace()
            .nth(1)
            .unwrap_or("(none)")
            .to_string()
    }

    /// Ask the engine to quit and wait for the process to exit. Best-effort
    /// — if it doesn't exit cleanly, the process is killed instead so it
    /// can't outlive the harness.
    fn shutdown(mut self) {
        self.send("quit");
        if self.child.wait().is_err() {
            let _ = self.child.kill();
        }
    }
}

/// Parse a UCI move string ("e2e4", "a7a8q") into a `Move` legal in `pos`.
/// Mirrors `main.rs`'s `parse_uci_move` exactly (duplicated here rather than
/// exported from the library, since it's a thin UCI-string-to-Move lookup
/// with no reason to grow a public library surface for one caller).
/// Returns `None` if the string is malformed or doesn't match any legal move.
fn parse_uci_move(pos: &Position, mv_str: &str) -> Option<Move> {
    if mv_str.len() < 4 {
        return None;
    }

    let from = Square::from_uci(&mv_str[0..2])?;
    let to = Square::from_uci(&mv_str[2..4])?;
    let promo = mv_str.chars().nth(4);

    let legal = generate_moves(pos);

    for &mv in legal.iter() {
        if mv.from != from || mv.to != to {
            continue;
        }

        if mv.kind.is_promotion() {
            let matches = match (promo, mv.kind.promotion_piece()) {
                (Some('q'), Some(PieceKind::Queen)) => true,
                (Some('r'), Some(PieceKind::Rook)) => true,
                (Some('b'), Some(PieceKind::Bishop)) => true,
                (Some('n'), Some(PieceKind::Knight)) => true,
                (None, Some(PieceKind::Queen)) => true,
                _ => false,
            };
            if matches {
                return Some(mv);
            }
        } else {
            return Some(mv);
        }
    }

    None
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let engine_a_path = args
        .get(1)
        .unwrap_or_else(|| panic!("usage: uci_match_runner <engine_a_path> <engine_b_path> [...]"));
    let engine_b_path = args
        .get(2)
        .unwrap_or_else(|| panic!("usage: uci_match_runner <engine_a_path> <engine_b_path> [...]"));
    let num_games: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(20);
    let movetime_ms: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(100);
    let seed_start: u64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
    let label_a = args.get(6).cloned().unwrap_or_else(|| "Engine A".to_string());
    let label_b = args.get(7).cloned().unwrap_or_else(|| "Engine B".to_string());
    let output_path = args
        .get(8)
        .cloned()
        .unwrap_or_else(|| "uci_match_results.txt".to_string());

    let mut engine_a = EngineProcess::spawn(engine_a_path);
    let mut engine_b = EngineProcess::spawn(engine_b_path);

    let mut wins_a = 0u64;
    let mut wins_b = 0u64;
    let mut draws = 0u64;

    for game_idx in 0..num_games {
        let seed = seed_start + game_idx;
        let a_plays_white = game_idx % 2 == 0;

        engine_a.new_game();
        engine_b.new_game();

        let outcome = play_one_game(&mut engine_a, &mut engine_b, seed, a_plays_white, movetime_ms);

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

    engine_a.shutdown();
    engine_b.shutdown();

    let summary = format_summary(num_games, &label_a, &label_b, wins_a, wins_b, draws);
    println!("{summary}");

    let mut file = File::create(&output_path).expect("failed to create output file");
    file.write_all(summary.as_bytes())
        .expect("failed to write summary file");
}

/// Play one game between the two engine processes from a seeded Pet Dragon
/// random starting position. `a_plays_white` fixes which physical color
/// Engine A controls for this game. Returns the outcome from Engine A's
/// perspective.
///
/// Game state (for legality/termination checks) is tracked locally via
/// `pet_dragon_lib`; the engines themselves are treated as black boxes that
/// only see `position` / `go` / `bestmove` over UCI, exactly like a real
/// GUI would drive them.
fn play_one_game(
    engine_a: &mut EngineProcess,
    engine_b: &mut EngineProcess,
    seed: u64,
    a_plays_white: bool,
    movetime_ms: u64,
) -> GameOutcome {
    let mut pos = Position::generate_with_seed(seed);
    let base_fen = pos.to_fen();
    pos.push_game_history();

    let mut moves_so_far: Vec<String> = Vec::new();
    let mut plies = 0usize;

    loop {
        if is_checkmate(&pos) {
            let white_won = pos.side_to_move == Color::Black;
            return outcome_from_white_result(white_won, a_plays_white);
        }
        if is_stalemate(&pos) || pos.is_insufficient_material() || pos.is_threefold_repetition() {
            return GameOutcome::Draw;
        }
        if plies >= MAX_PLIES {
            return GameOutcome::Draw;
        }

        let mover_is_a = pos.side_to_move == Color::White && a_plays_white
            || pos.side_to_move == Color::Black && !a_plays_white;
        let mover = if mover_is_a {
            &mut *engine_a
        } else {
            &mut *engine_b
        };

        let mv_str = mover.go_movetime(&base_fen, &moves_so_far, movetime_ms);

        let mv = match parse_uci_move(&pos, &mv_str) {
            Some(mv) => mv,
            None => {
                // Engine returned an unparseable or illegal move — a bug in
                // that engine, not in this harness. Forfeit the game to the
                // other side rather than corrupt the tracked position, and
                // keep the match running rather than crashing the whole job.
                eprintln!(
                    "warning: engine returned illegal/unparseable move '{mv_str}' \
                     (seed {seed}) — forfeiting game to opponent"
                );
                return if mover_is_a {
                    GameOutcome::WinB
                } else {
                    GameOutcome::WinA
                };
            }
        };

        moves_so_far.push(mv.to_uci());
        pos.make_move_with_history(mv);
        pos.push_game_history();
        plies += 1;
    }
}

/// Translate a checkmate result (`white_won`) into a `GameOutcome` from
/// Engine A's perspective, given which color A was playing this game.
fn outcome_from_white_result(white_won: bool, a_plays_white: bool) -> GameOutcome {
    let a_won = white_won == a_plays_white;
    if a_won {
        GameOutcome::WinA
    } else {
        GameOutcome::WinB
    }
}

/// Convert a match score percentage (0.0-1.0) into an Elo difference
/// estimate using the standard logistic formula. Returns `None` at the
/// boundaries (0% or 100%) where the formula is undefined.
fn elo_diff_from_score(score_pct: f64) -> Option<f64> {
    if score_pct <= 0.0 || score_pct >= 1.0 {
        return None;
    }
    Some(-400.0 * ((1.0 / score_pct - 1.0).log10()))
}

/// Build the human-readable match summary written to stdout and the output
/// file.
fn format_summary(
    num_games: u64,
    label_a: &str,
    label_b: &str,
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
        "Pet Dragon UCI-vs-UCI match (D36)\n\
         Engine A: {label_a}\n\
         Engine B: {label_b}\n\
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
        let outcome = outcome_from_white_result(true, true);
        assert!(matches!(outcome, GameOutcome::WinA));
    }

    #[test]
    fn test_outcome_from_white_result_a_plays_black_and_white_wins() {
        let outcome = outcome_from_white_result(true, false);
        assert!(matches!(outcome, GameOutcome::WinB));
    }

    #[test]
    fn test_parse_uci_move_normal() {
        init_masks();
        init_magic();
        init_zobrist();
        let pos = Position::generate_with_seed(0);
        let legal = generate_moves(&pos);
        // Any legal move round-trips through its own UCI string.
        let mv = legal.iter().next().expect("seed 0 position has legal moves");
        let parsed = parse_uci_move(&pos, &mv.to_uci());
        assert_eq!(parsed, Some(*mv));
    }

    #[test]
    fn test_parse_uci_move_illegal_returns_none() {
        init_masks();
        init_magic();
        init_zobrist();
        let pos = Position::generate_with_seed(0);
        // a1a1 is never a legal move (zero-length move).
        assert_eq!(parse_uci_move(&pos, "a1a1"), None);
    }

    #[test]
    fn test_format_summary_contains_key_fields() {
        let summary = format_summary(20, "pre-tuning", "post-tuning", 15, 3, 2);
        assert!(summary.contains("pre-tuning"));
        assert!(summary.contains("post-tuning"));
        assert!(summary.contains("A wins: 15"));
        assert!(summary.contains("B wins: 3"));
        assert!(summary.contains("Draws: 2"));
    }
}
