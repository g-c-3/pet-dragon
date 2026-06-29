// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// main.rs — Full UCI protocol implementation (Phase 9)
//
// UCI (Universal Chess Interface) is the standard protocol for chess GUIs
// to communicate with chess engines. Communication is over stdin/stdout,
// one line per command.
//
// Commands handled:
//   uci              → id name/author + option declarations + uciok
//   isready          → readyok
//   ucinewgame       → reset position + clear TT + age history
//   position         → set board (startpos|fen) + optional move list
//   go               → run search, print info lines, output bestmove
//   stop             → (Phase 13: signal search thread; Phase 9: no-op)
//   setoption        → Hash size, Threads (stored for Phase 13)
//   quit             → exit
//   d                → debug: print current position
//   perft <depth>    → divide output from current position
//
// Single-threaded model (Phase 9):
//   Search runs synchronously on the main thread.
//   Phase 13 (Lazy SMP) will move search to a background thread with
//   an AtomicBool stop flag shared via Arc.
// ============================================================================

use std::io::{self, BufRead, Write};

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::generate_moves;
use pet_dragon_lib::movegen::legal::apply_move_for_legality_pub;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::position::fen::STANDARD_START_FEN;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::search::{
    iterative::iterative_deepening, SearchInfo,
};
use pet_dragon_lib::search::time::{allocate_time, TimeControl};
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::{Color, Move, PieceKind, Square};

// ── Engine metadata ───────────────────────────────────────────────────────────

const ENGINE_NAME:    &str = "Pet Dragon";
const ENGINE_AUTHOR:  &str = "Gokul Chandar";
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_HASH_MB: usize = 64;

// ── Engine state ──────────────────────────────────────────────────────────────

/// All persistent state across UCI commands within one session.
struct EngineState {
    pos:     Position,
    tt:      TranspositionTable,
    info:    SearchInfo,
    hash_mb: usize,
}

impl EngineState {
    fn new() -> Self {
        EngineState {
            pos:     Position::start_pos().unwrap(),
            tt:      TranspositionTable::new(DEFAULT_HASH_MB),
            info:    SearchInfo::new(),
            hash_mb: DEFAULT_HASH_MB,
        }
    }

    /// Reset for a new game — clear TT, age history, reset position.
    fn new_game(&mut self) {
        self.tt.clear();
        self.tt.new_search();
        self.info.age_history();
        self.pos = Position::start_pos().unwrap();
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    // Mandatory startup — must run before any move generation or search.
    init_masks();
    init_magic();
    init_zobrist();

    let mut state = EngineState::new();
    let stdin     = io::stdin();
    let stdout    = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l)  => l,
            Err(_) => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() { continue; }

        if line == "quit" {
            break;
        } else if line == "uci" {
            cmd_uci();
        } else if line == "isready" {
            println!("readyok");
        } else if line == "ucinewgame" {
            state.new_game();
        } else if line.starts_with("position") {
            cmd_position(&mut state, &line);
        } else if line.starts_with("go") {
            cmd_go(&mut state, &line);
        } else if line == "stop" {
            // Phase 9: search is synchronous — stop arrives after it finishes.
            // Phase 13: set state.info.stop = true here.
        } else if line.starts_with("setoption") {
            cmd_setoption(&mut state, &line);
        } else if line == "d" {
            println!("{}", state.pos);
        } else if line.starts_with("perft") {
            cmd_perft(&mut state, &line);
        }
        // Unrecognised commands silently ignored (UCI spec §3.1)

        let _ = stdout.lock().flush();
    }
}

// ── UCI identification ────────────────────────────────────────────────────────

/// Respond to "uci" — declare identity and options, end with "uciok".
fn cmd_uci() {
    println!("id name {} {}", ENGINE_NAME, ENGINE_VERSION);
    println!("id author {}", ENGINE_AUTHOR);
    println!();
    println!(
        "option name Hash type spin default {} min 1 max 65536",
        DEFAULT_HASH_MB
    );
    // Threads accepted but ignored until Phase 13 Lazy SMP
    println!("option name Threads type spin default 1 min 1 max 1");
    // Some GUIs send this; we accept it to avoid complaints
    println!("option name UCI_Chess960 type check default false");
    println!();
    println!("uciok");
}

// ── setoption ─────────────────────────────────────────────────────────────────

/// Handle "setoption name <Name> value <Value>"
fn cmd_setoption(state: &mut EngineState, line: &str) {
    // Expected: setoption name Hash value 128
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 || tokens[1] != "name" { return; }

    let name  = tokens[2].to_lowercase();
    let value = tokens.get(4).copied().unwrap_or("");

    match name.as_str() {
        "hash" => {
            if let Ok(mb) = value.parse::<usize>() {
                let mb = mb.clamp(1, 65536);
                state.hash_mb = mb;
                state.tt.resize(mb);
            }
        }
        "threads" => { /* accepted, Phase 13 will use it */ }
        "uci_chess960" => { /* ignored */ }
        _ => { /* unknown options silently ignored */ }
    }
}

// ── position ──────────────────────────────────────────────────────────────────

/// Handle "position [startpos | fen <fen>] [moves <m1> <m2> ...]"
fn cmd_position(state: &mut EngineState, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut idx = 1usize; // skip "position"

    // Parse base position
    let mut pos = if idx < tokens.len() && tokens[idx] == "startpos" {
        idx += 1;
        match Position::from_fen(STANDARD_START_FEN) {
            Ok(p)  => p,
            Err(e) => {
                eprintln!("info string Error: start_pos failed: {:?}", e);
                return;
            }
        }
    } else if idx < tokens.len() && tokens[idx] == "fen" {
        idx += 1;
        let fen_start = idx;
        while idx < tokens.len() && tokens[idx] != "moves" {
            idx += 1;
        }
        if fen_start == idx {
            eprintln!("info string Error: empty FEN");
            return;
        }
        let fen = tokens[fen_start..idx].join(" ");
        match Position::from_fen(&fen) {
            Ok(p)  => p,
            Err(e) => {
                eprintln!("info string Error parsing FEN '{:?}': {:?}", fen, e);
                return;
            }
        }
    } else {
        eprintln!("info string Error: position requires startpos or fen");
        return;
    };

    // Clear game history and record the starting position hash
    pos.clear_game_history();
    pos.push_game_history();

    // Apply moves list
    if idx < tokens.len() && tokens[idx] == "moves" {
        idx += 1;
        while idx < tokens.len() {
            let mv_str = tokens[idx];
            idx += 1;
            match parse_uci_move(&pos, mv_str) {
                Some(mv) => {
                    pos.make_move(mv);
                    pos.push_game_history();
                }
                None => {
                    eprintln!("info string Warning: illegal or unknown move '{}'", mv_str);
                    break;
                }
            }
        }
    }

    state.pos = pos;
}

// ── go ────────────────────────────────────────────────────────────────────────

/// Handle "go [wtime N] [btime N] [winc N] [binc N] [movestogo N]
///           [movetime N] [depth N] [nodes N] [infinite] [ponder]"
fn cmd_go(state: &mut EngineState, line: &str) {
    let tc = parse_go(line);

    // Allocate time and configure SearchInfo
    let is_white = state.pos.side_to_move == Color::White;
    let (soft_ms, _hard_ms) = allocate_time(&tc, is_white);
    state.info.time_allocated_ms = soft_ms;
    state.info.reset_for_search();
    state.tt.new_search();

    // Run search synchronously (Phase 9 — single-threaded)
    let result = iterative_deepening(
        &mut state.pos,
        &tc,
        &mut state.info,
        &mut state.tt,
    );

    // Output bestmove [ponder <move>]
    let ponder = result.pv.get(1).copied();
    match ponder {
        Some(p) => println!("bestmove {} ponder {}", result.best_move.to_uci(), p.to_uci()),
        None    => println!("bestmove {}", result.best_move.to_uci()),
    }
}

/// Parse a "go" command into a TimeControl.
fn parse_go(line: &str) -> TimeControl {
    let mut tc  = TimeControl::default();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut i = 1usize; // skip "go"

    while i < tokens.len() {
        // Tokens that take a following value
        macro_rules! parse_next_u64 {
            ($field:expr) => {{
                i += 1;
                if i < tokens.len() {
                    $field = tokens[i].parse().unwrap_or(0);
                }
            }};
        }
        match tokens[i] {
            "wtime"     => parse_next_u64!(tc.wtime),
            "btime"     => parse_next_u64!(tc.btime),
            "winc"      => parse_next_u64!(tc.winc),
            "binc"      => parse_next_u64!(tc.binc),
            "movestogo" => parse_next_u64!(tc.movestogo),
            "movetime"  => parse_next_u64!(tc.movetime),
            "nodes"     => parse_next_u64!(tc.nodes),
            "depth"     => {
                i += 1;
                if i < tokens.len() {
                    tc.depth = tokens[i].parse().unwrap_or(0);
                }
            }
            "infinite"  => tc.infinite = true,
            "ponder"    => tc.ponder   = true,
            _           => {} // unknown tokens silently ignored
        }
        i += 1;
    }

    tc
}

// ── Move parsing ──────────────────────────────────────────────────────────────

/// Parse a UCI move string ("e2e4", "a7a8q") and find the matching legal move.
/// Returns None if the move is illegal or the string is malformed.
fn parse_uci_move(pos: &Position, mv_str: &str) -> Option<Move> {
    if mv_str.len() < 4 { return None; }

    let from = Square::from_uci(&mv_str[0..2])?;
    let to   = Square::from_uci(&mv_str[2..4])?;
    let promo = mv_str.chars().nth(4); // optional promotion char

    let legal = generate_moves(pos);

    for &mv in legal.iter() {
        if mv.from != from || mv.to != to { continue; }

        if mv.kind.is_promotion() {
            // Must match the promotion piece (default to queen if omitted)
            let matches = match (promo, mv.kind.promotion_piece()) {
                (Some('q'), Some(PieceKind::Queen))  => true,
                (Some('r'), Some(PieceKind::Rook))   => true,
                (Some('b'), Some(PieceKind::Bishop)) => true,
                (Some('n'), Some(PieceKind::Knight)) => true,
                // No promo char → default queen
                (None,      Some(PieceKind::Queen))  => true,
                _                                    => false,
            };
            if matches { return Some(mv); }
        } else {
            return Some(mv);
        }
    }

    None
}

// ── perft (debug) ─────────────────────────────────────────────────────────────

/// Handle "perft <depth>" — divide output from current position.
/// Useful for verifying move generation against known perft values.
fn cmd_perft(state: &mut EngineState, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let depth: u32 = tokens.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    let legal  = generate_moves(&state.pos);
    let color  = state.pos.side_to_move;
    let mut total = 0u64;

    for &mv in legal.iter() {
        let mut child = state.pos.clone();
        apply_move_for_legality_pub(&mut child, mv, color);
        let count = perft_bulk(&child, depth.saturating_sub(1));
        println!("{}: {}", mv.to_uci(), count);
        total += count;
    }

    println!();
    println!("Nodes searched: {}", total);
}

/// Recursive perft — bulk-counts leaf nodes.
fn perft_bulk(pos: &Position, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let legal = generate_moves(pos);
    if depth == 1 { return legal.len() as u64; }

    let color = pos.side_to_move;
    let mut nodes = 0u64;
    for &mv in legal.iter() {
        let mut child = pos.clone();
        apply_move_for_legality_pub(&mut child, mv, color);
        nodes += perft_bulk(&child, depth - 1);
    }
    nodes
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_parse_go_movetime() {
        let tc = parse_go("go movetime 5000");
        assert_eq!(tc.movetime, 5000);
    }

    #[test]
    fn test_parse_go_wtime_btime() {
        let tc = parse_go("go wtime 60000 btime 60000 winc 1000 binc 1000");
        assert_eq!(tc.wtime, 60000);
        assert_eq!(tc.btime, 60000);
        assert_eq!(tc.winc,  1000);
        assert_eq!(tc.binc,  1000);
        assert!(!tc.infinite);
    }

    #[test]
    fn test_parse_go_infinite() {
        let tc = parse_go("go infinite");
        assert!(tc.infinite);
    }

    #[test]
    fn test_parse_go_depth() {
        let tc = parse_go("go depth 8");
        assert_eq!(tc.depth, 8);
    }

    #[test]
    fn test_parse_uci_move_normal() {
        setup();
        let pos = Position::from_fen(STANDARD_START_FEN).unwrap();
        let mv  = parse_uci_move(&pos, "e2e4");
        assert!(mv.is_some(), "e2e4 should be legal in starting position");
    }

    #[test]
    fn test_parse_uci_move_illegal() {
        setup();
        let pos = Position::from_fen(STANDARD_START_FEN).unwrap();
        let mv  = parse_uci_move(&pos, "e2e5"); // illegal jump
        assert!(mv.is_none(), "e2e5 should not be legal");
    }

    #[test]
    fn test_parse_uci_move_promotion() {
        setup();
        // White pawn on e7, white king on e1, black king on a8 (e8 clear)
        let fen = "k7/4P3/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = parse_uci_move(&pos, "e7e8q");
        assert!(mv.is_some(), "e7e8q should be a legal promotion");
    }

    #[test]
    fn test_parse_uci_move_default_queen_promo() {
        setup();
        let fen = "k7/4P3/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        // No promo char — should default to queen
        let mv  = parse_uci_move(&pos, "e7e8");
        assert!(mv.is_some(), "e7e8 with no promo char should default to queen");
        assert_eq!(mv.unwrap().kind.promotion_piece(), Some(PieceKind::Queen));
    }

    #[test]
    fn test_engine_state_new_game_clears_tt() {
        setup();
        let mut state = EngineState::new();
        // Store something in TT, then new_game — fill should drop
        let fill_before = state.tt.fill_permille();
        state.new_game();
        let fill_after = state.tt.fill_permille();
        // After clear, fill should be 0
        assert_eq!(fill_after, 0,
            "TT fill should be 0 after new_game (was {})", fill_before);
    }

    #[test]
    fn test_position_startpos() {
        setup();
        let mut state = EngineState::new();
        cmd_position(&mut state, "position startpos");
        // Should be standard starting position — 20 legal moves
        let legal = generate_moves(&state.pos);
        assert_eq!(legal.len(), 20, "startpos should have 20 legal moves");
    }

    #[test]
    fn test_position_fen() {
        setup();
        let mut state  = EngineState::new();
        let test_line  = "position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        cmd_position(&mut state, test_line);
        // After 1.e4, Black has 20 legal responses
        let legal = generate_moves(&state.pos);
        assert_eq!(legal.len(), 20, "After 1.e4, Black should have 20 legal moves");
    }

    #[test]
    fn test_position_startpos_with_moves() {
        setup();
        let mut state = EngineState::new();
        cmd_position(&mut state, "position startpos moves e2e4 e7e5");
        let legal = generate_moves(&state.pos);
        // After 1.e4 e5, White has 29 legal moves
        assert_eq!(legal.len(), 29,
            "After 1.e4 e5, White should have 29 legal moves, got {}", legal.len());
    }

    #[test]
    fn test_perft_depth_1_startpos() {
        setup();
        let pos   = Position::from_fen(STANDARD_START_FEN).unwrap();
        let legal = generate_moves(&pos);
        assert_eq!(legal.len(), 20, "Depth 1 from startpos = 20 nodes");
    }
}
