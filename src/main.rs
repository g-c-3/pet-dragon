// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// main.rs — Full UCI protocol with Lazy SMP (Phase 9 + 13.4)
//
// Commands handled:
//   uci              → id name/author + option declarations + uciok
//   isready          → readyok
//   ucinewgame       → reset position + clear TT + age history
//   position         → set board (startpos|fen) + optional move list
//   go               → start search in background thread; print bestmove when done
//   stop             → signal background search to stop; wait; print bestmove
//   setoption        → Hash size, Threads count
//   quit             → join any active search, exit
//   d                → debug: print current position
//   perft <depth>    → divide output from current position
//
// Lazy SMP (Phase 13.4):
//   - `go` always spawns the search on a background thread and returns
//     immediately. The UCI loop remains responsive.
//   - With `Threads N`, an additional N-1 helper threads are spawned. Each
//     helper runs iterative deepening on a clone of the position with the
//     same shared TT (Arc<TranspositionTable>, lock-free benign races per D4).
//   - All threads share an `Arc<AtomicBool>` stop flag. The main search thread
//     sets it when time expires. The `stop` UCI command also sets it.
//   - When the main search thread completes, it prints bestmove and exits.
//     Helper threads detect the stop flag and terminate silently.
//   - History/killers/correction_history are NOT shared — each thread has its
//     own SearchInfo. The main thread's SearchInfo is returned on join so its
//     history can be preserved for the next search.
// ============================================================================

use std::io::{self, BufRead, Write};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::JoinHandle;

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::generate_moves;
use pet_dragon_lib::movegen::legal::apply_move_for_legality_pub;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::position::fen::STANDARD_START_FEN;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::eval::set_nnue_weight_pct;
use pet_dragon_lib::search::{
    iterative::iterative_deepening, SearchInfo,
};
use pet_dragon_lib::search::time::TimeControl;
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::{Color, Move, PieceKind, Square};

#[cfg(not(target_arch = "wasm32"))]
use pet_dragon_lib::syzygy::SyzygyProber;

// ── Engine metadata ───────────────────────────────────────────────────────────

const ENGINE_NAME:    &str = "Pet Dragon";
const ENGINE_AUTHOR:  &str = "Gokul Chandar";
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_HASH_MB: usize = 64;
const MAX_THREADS: usize = 64;

// ── Engine state ──────────────────────────────────────────────────────────────

/// All persistent state across UCI commands within one session.
/// The TT is shared via Arc so search threads can hold references.
struct EngineState {
    pos:     Position,
    /// Shared lock-free transposition table (D4: benign races accepted).
    tt:      Arc<TranspositionTable>,
    /// Main thread's SearchInfo — history/countermoves persist across moves.
    info:    SearchInfo,
    hash_mb: usize,
    threads: usize,
    /// Shared stop flag. Set by `stop` command or when time expires.
    stop_flag: Arc<AtomicBool>,
    /// Handle to active search thread. None when engine is idle.
    /// Returns the main SearchInfo so history can be preserved.
    search_handle: Option<JoinHandle<SearchInfo>>,
    /// Syzygy tablebase handle — set on setoption SyzygyPath. None = disabled.
    #[cfg(not(target_arch = "wasm32"))]
    syzygy: Option<std::sync::Arc<SyzygyProber>>,
}

impl EngineState {
    fn new() -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut info = SearchInfo::new_with_stop(Arc::clone(&stop_flag));
        // Ensure info's stop_flag is the same Arc as our stop_flag
        info.stop_flag = Arc::clone(&stop_flag);
        EngineState {
            pos:           Position::start_pos().unwrap(),
            tt:            Arc::new(TranspositionTable::new(DEFAULT_HASH_MB)),
            info,
            hash_mb:       DEFAULT_HASH_MB,
            threads:       1,
            stop_flag,
            search_handle: None,
            #[cfg(not(target_arch = "wasm32"))]
            syzygy: None,
        }
    }

    /// Block until any active search thread finishes.
    /// Recovers the thread's SearchInfo and merges history tables for persistence.
    fn wait_for_search(&mut self) {
        if let Some(handle) = self.search_handle.take() {
            if let Ok(returned_info) = handle.join() {
                // Preserve ordering tables across moves (gives better move ordering)
                self.info.history      = returned_info.history;
                self.info.countermoves = returned_info.countermoves;
                self.info.correction_history = returned_info.correction_history;
            }
        }
    }

    /// Stop any active search and wait for the thread to exit.
    fn stop_search(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        self.wait_for_search();
    }

    /// Reset for a new game — clear TT, age history, reset position.
    fn new_game(&mut self) {
        self.stop_search();
        // Arc::get_mut succeeds because search_handle is now None (joined above)
        if let Some(tt) = Arc::get_mut(&mut self.tt) {
            tt.clear();
            tt.new_search();
        }
        self.info.age_history();
        self.stop_flag.store(false, Ordering::SeqCst);
        self.info.stop_flag = Arc::clone(&self.stop_flag);
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
            state.stop_search();
            break;
        } else if line == "uci" {
            cmd_uci();
        } else if line == "isready" {
            // If a search is running, wait for it to finish before reporting ready.
            // (Usually search is done by the time isready arrives.)
            state.wait_for_search();
            println!("readyok");
        } else if line == "ucinewgame" {
            state.new_game();
        } else if line.starts_with("position") {
            // Complete any in-flight search before changing position.
            state.wait_for_search();
            cmd_position(&mut state, &line);
        } else if line.starts_with("go") {
            // Complete any previous search first (shouldn't normally happen).
            state.wait_for_search();
            cmd_go(&mut state, &line);
        } else if line == "stop" {
            // Signal search thread to stop; it will print bestmove and exit.
            state.stop_search();
        } else if line.starts_with("setoption") {
            state.wait_for_search(); // safe to change options only when idle
            cmd_setoption(&mut state, &line);
        } else if line == "d" {
            println!("{}", state.pos);
        } else if line.starts_with("perft") {
            state.wait_for_search();
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
    println!(
        "option name Threads type spin default 1 min 1 max {}",
        MAX_THREADS
    );
    // Some GUIs send this; we accept it to avoid complaints
    println!("option name UCI_Chess960 type check default false");
    // Phase 15: Syzygy tablebase path (native only; WASM builds ignore this)
    println!("option name SyzygyPath type string default <empty>");
    // Phase 17: NNUE/HCE blend weight as a percentage — 0 = pure HCE,
    // 100 = pure NNUE. Default matches D23's fixed constant (25%), now
    // runtime-adjustable for Elo A/B testing from one binary.
    println!("option name NNUEWeight type spin default 25 min 0 max 100");
    println!();
    println!("uciok");
}

// ── setoption ─────────────────────────────────────────────────────────────────

/// Handle "setoption name <Name> value <Value>"
fn cmd_setoption(state: &mut EngineState, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 || tokens[1] != "name" { return; }

    let name  = tokens[2].to_lowercase();
    let value = tokens.get(4).copied().unwrap_or("");

    match name.as_str() {
        "hash" => {
            if let Ok(mb) = value.parse::<usize>() {
                let mb = mb.clamp(1, 65536);
                state.hash_mb = mb;
                // Arc::get_mut works because search is idle (wait_for_search called above)
                if let Some(tt) = Arc::get_mut(&mut state.tt) {
                    tt.resize(mb);
                } else {
                    // Fallback: replace Arc entirely
                    state.tt = Arc::new(TranspositionTable::new(mb));
                }
            }
        }
        "threads" => {
            if let Ok(n) = value.parse::<usize>() {
                state.threads = n.clamp(1, MAX_THREADS);
            }
        }
        "uci_chess960" => { /* ignored */ }
        "syzygypath" => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if value.is_empty() || value == "<empty>" {
                    state.syzygy = None;
                    eprintln!("info string Syzygy tablebases disabled");
                } else {
                    match SyzygyProber::new(value) {
                        Ok(prober) => {
                            let max_pc = prober.max_pieces();
                            state.syzygy = Some(std::sync::Arc::new(prober));
                            eprintln!(
                                "info string Syzygy tablebases loaded: {} path={} max_pieces={}",
                                value, value, max_pc
                            );
                        }
                        Err(e) => {
                            state.syzygy = None;
                            eprintln!("info string Syzygy load failed: {}", e);
                        }
                    }
                }
            }
        }
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
///
/// Spawns the search on a background thread (Lazy SMP). The UCI loop
/// remains responsive for `stop`, `isready`, etc.
fn cmd_go(state: &mut EngineState, line: &str) {
    let tc = parse_go(line);

    // Reset stop flag for the new search
    state.stop_flag.store(false, Ordering::SeqCst);

    // Age TT for this new search (only when idle — Arc::get_mut works here)
    if let Some(tt) = Arc::get_mut(&mut state.tt) {
        tt.new_search();
    }

    // ── DTZ root probe (Phase 15.5) ───────────────────────────────────────────
    // Must run BEFORE spawning any threads — probe_root is not thread-safe.
    // If the position is in the tablebases, output bestmove and return early.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(ref tb) = state.syzygy {
        if state.pos.all_occupied.count() <= tb.max_pieces() {
            if let Some((from_idx, to_idx, _promo, wdl)) = tb.probe_root(&state.pos) {
                if let (Some(from_sq), Some(to_sq)) = (
                    Square::from_index(from_idx),
                    Square::from_index(to_idx),
                ) {
                    let legal = generate_moves(&state.pos);
                    let found_move: Option<Move> =
                        legal.iter().find(|m| m.from == from_sq && m.to == to_sq).copied();
                    if let Some(mv) = found_move {
                        let outcome = if wdl > 0 { "win" } else if wdl < 0 { "loss" } else { "draw" };
                        println!(
                            "info depth 0 score cp {} tbhits 1 nodes 0 nps 0 pv {} string TB {}",
                            wdl, mv.to_uci(), outcome
                        );
                        println!("bestmove {}", mv.to_uci());
                        state.search_handle = None;
                        return;
                    }
                }
            }
        }
    }

    // Clone syzygy handle for search threads (WDL probing during search, Phase 15.3/15.4)
    #[cfg(not(target_arch = "wasm32"))]
    let syzygy_for_threads = state.syzygy.clone();

    let pos       = state.pos.clone();
    let threads   = state.threads.max(1);
    let stop_flag = Arc::clone(&state.stop_flag);
    let tt        = Arc::clone(&state.tt);

    // Take snapshots of ordering tables for the main thread's SearchInfo.
    // This preserves history knowledge across moves.
    let history      = state.info.history;
    let countermoves = state.info.countermoves.clone();
    let correction   = std::mem::replace(
        &mut state.info.correction_history,
        pet_dragon_lib::search::pruning::CorrectionHistory::new(),
    );

    let handle: JoinHandle<SearchInfo> = std::thread::spawn(move || {
        // ── Helper threads ────────────────────────────────────────────────────
        // Helpers run the same search with infinite time, sharing the TT.
        // They stop when stop_flag is set (either by main thread or `stop`).
        let mut helper_handles = Vec::new();
        for _ in 1..threads {
            let mut h_pos  = pos.clone();
            let h_tt       = Arc::clone(&tt);
            let h_stop     = Arc::clone(&stop_flag);
            let mut h_tc   = tc.clone();
            h_tc.infinite  = true; // helpers are time-unlimited; stop flag kills them
            // Clone syzygy handle per-thread (same pattern as h_pos/h_tt above)
            #[cfg(not(target_arch = "wasm32"))]
            let h_syzygy = syzygy_for_threads.clone();
            helper_handles.push(std::thread::spawn(move || {
                let mut h_info = SearchInfo::new_with_stop(h_stop);
                #[cfg(not(target_arch = "wasm32"))]
                { h_info.syzygy = h_syzygy; }
                iterative_deepening(&mut h_pos, &h_tc, &mut h_info, &*h_tt)
            }));
        }

        // ── Main search thread ────────────────────────────────────────────────
        let mut main_pos  = pos;
        let mut main_info = SearchInfo::new_with_stop(Arc::clone(&stop_flag));
        main_info.history      = history;
        main_info.countermoves = countermoves;
        main_info.correction_history = correction;
        #[cfg(not(target_arch = "wasm32"))]
        { main_info.syzygy = syzygy_for_threads; }

        let result = iterative_deepening(&mut main_pos, &tc, &mut main_info, &*tt);

        // Signal helpers to stop and wait for them
        stop_flag.store(true, Ordering::SeqCst);
        for h in helper_handles { let _ = h.join(); }

        // Output bestmove — only the main thread prints this
        // (helpers contribute to TT but never output UCI)
        let ponder = result.pv.get(1).copied();
        match ponder {
            Some(p) => println!("bestmove {} ponder {}", result.best_move.to_uci(), p.to_uci()),
            None    => println!("bestmove {}", result.best_move.to_uci()),
        }

        // Age history for next move, then return info for persistence
        main_info.age_history();
        main_info
    });

    state.search_handle = Some(handle);
}

/// Parse a "go" command into a TimeControl.
fn parse_go(line: &str) -> TimeControl {
    let mut tc  = TimeControl::default();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut i = 1usize; // skip "go"

    while i < tokens.len() {
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
            _           => {}
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

    let from  = Square::from_uci(&mv_str[0..2])?;
    let to    = Square::from_uci(&mv_str[2..4])?;
    let promo = mv_str.chars().nth(4);

    let legal = generate_moves(pos);

    for &mv in legal.iter() {
        if mv.from != from || mv.to != to { continue; }

        if mv.kind.is_promotion() {
            let matches = match (promo, mv.kind.promotion_piece()) {
                (Some('q'), Some(PieceKind::Queen))  => true,
                (Some('r'), Some(PieceKind::Rook))   => true,
                (Some('b'), Some(PieceKind::Bishop)) => true,
                (Some('n'), Some(PieceKind::Knight)) => true,
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
        let mv  = parse_uci_move(&pos, "e7e8");
        assert!(mv.is_some(), "e7e8 with no promo char should default to queen");
        assert_eq!(mv.unwrap().kind.promotion_piece(), Some(PieceKind::Queen));
    }

    #[test]
    fn test_engine_state_new_game_clears_tt() {
        setup();
        let mut state = EngineState::new();
        let fill_before = state.tt.fill_permille();
        state.new_game();
        let fill_after = state.tt.fill_permille();
        assert_eq!(fill_after, 0,
            "TT fill should be 0 after new_game (was {})", fill_before);
    }

    #[test]
    fn test_position_startpos() {
        setup();
        let mut state = EngineState::new();
        cmd_position(&mut state, "position startpos");
        let legal = generate_moves(&state.pos);
        assert_eq!(legal.len(), 20, "startpos should have 20 legal moves");
    }

    #[test]
    fn test_position_fen() {
        setup();
        let mut state  = EngineState::new();
        let test_line  = "position fen rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        cmd_position(&mut state, test_line);
        let legal = generate_moves(&state.pos);
        assert_eq!(legal.len(), 20, "After 1.e4, Black should have 20 legal moves");
    }

    #[test]
    fn test_position_startpos_with_moves() {
        setup();
        let mut state = EngineState::new();
        cmd_position(&mut state, "position startpos moves e2e4 e7e5");
        let legal = generate_moves(&state.pos);
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

    #[test]
    fn test_stop_flag_shared() {
        setup();
        let state = EngineState::new();
        // stop_flag and info.stop_flag should be the same Arc
        state.stop_flag.store(true, Ordering::SeqCst);
        assert!(state.info.stop_flag.load(Ordering::SeqCst),
            "info.stop_flag should see the same store as stop_flag");
        state.stop_flag.store(false, Ordering::SeqCst);
    }

    #[test]
    fn test_threads_option() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Threads value 4");
        assert_eq!(state.threads, 4, "Threads option should update state");
    }

    #[test]
    fn test_hash_option() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Hash value 32");
        assert_eq!(state.hash_mb, 32, "Hash option should update state");
    }

    #[test]
    fn test_wait_for_search_no_handle() {
        setup();
        let mut state = EngineState::new();
        // Should not panic when no search is running
        state.wait_for_search();
    }
}
