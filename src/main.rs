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
//   ponderhit        → convert an active `go ponder` search from effectively
//                      infinite to a real, time-bounded one (Phase 18/D37).
//                      No-op if no ponder search is currently active.
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
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread::JoinHandle;

use web_time::Instant;

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
use pet_dragon_lib::search::time::{allocate_time, TimeControl};
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
/// Declared UCI upper bound for MultiPV (Phase 19). iterative_deepening()
/// separately clamps against the actual number of legal root moves at
/// search time, so this is just a sane ceiling for the `option` declaration
/// — no realistic position has anywhere near this many legal moves, but
/// Pet Dragon's custom pawn rules mean "no realistic position" isn't a
/// guarantee, hence a generous but still bounded number rather than
/// something unbounded.
const MAX_MULTIPV: usize = 64;

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

    // ── Pondering (Phase 18/D37) ────────────────────────────────────────────
    /// Shared ponder-hit soft-limit override, cloned into the active search
    /// thread's SearchInfo in cmd_go. Written by cmd_ponderhit. See
    /// SearchInfo::ponder_hit_soft_ms for the full mechanism explanation.
    ponder_hit_soft_ms: Arc<AtomicU64>,
    /// Shared ponder-hit hard-limit override — see SearchInfo::ponder_hit_hard_ms.
    ponder_hit_hard_ms: Arc<AtomicU64>,
    /// The (soft_ms, hard_ms) the current search WOULD use once a
    /// `ponderhit` arrives, precomputed in cmd_go from the original `go`
    /// command's real time control (with `ponder` forced false). None
    /// when the most recent `go` wasn't a ponder search, or once consumed
    /// by cmd_ponderhit.
    pending_ponder_allocation: Option<(u64, u64)>,
    /// Wall-clock instant the current ponder search was dispatched. Used by
    /// cmd_ponderhit to compute how much "free" pondering time has already
    /// elapsed (pondering time doesn't count against the real clock, per
    /// the UCI spec). None when not currently pondering.
    ponder_started_at: Option<Instant>,

    // ── Analysis GUI options (Phase 19) ─────────────────────────────────────
    /// UCI `MultiPV` setting — how many principal variations to report.
    /// Set from `setoption`; applied to the active search's SearchInfo in
    /// cmd_go. Neither this nor `move_overhead_ms` below affects playing
    /// strength, only what gets reported / how time is budgeted.
    multipv: usize,
    /// UCI `Move Overhead` setting, in ms — replaces the old hardcoded
    /// `OVERHEAD_MS` constant with a runtime-configurable value. Applied
    /// to the TimeControl in cmd_go via `TimeControl::overhead_ms`.
    move_overhead_ms: u64,

    // ── Skill Level (Phase 20 / D39) ────────────────────────────────────────
    /// UCI `Skill Level` setting, 0..=20. UNLIKE `multipv`/`move_overhead_ms`
    /// above, this DOES affect playing strength by design — it's the whole
    /// point (D39). Applied in cmd_go to both `main_info`/`h_info.skill_level`
    /// (depth cap, via `iterative_deepening()`) and `tc.skill_time_fraction_pct`
    /// (time budget, via `allocate_time()`) — see skill.rs for the full
    /// per-tier table and Session 65's reasoning for needing both.
    skill_level: u8,

    // ── Contempt ─────────────────────────────────────────────────────────────
    /// UCI `Contempt` setting, centipawns, -100..=100, default 0. Threaded
    /// into `main_info.contempt`/`h_info.contempt` in cmd_go, same pattern
    /// as `skill_level` above — see `SearchInfo::contempt`'s doc comment
    /// for the full sign convention.
    contempt: i32,

    // ── Elo-limited strength (D43 — overrides D39's original UCI_Elo rejection) ──
    /// UCI `UCI_LimitStrength` setting, default false. When true, `elo`
    /// below overrides `skill_level` in cmd_go via
    /// `search::skill::elo_to_skill_level()` — see that function's doc
    /// comment and DECISIONS.md D43 for what these Elo numbers actually
    /// are (two chosen anchors + this project's own real measured tier
    /// gaps, NOT an externally-calibrated rating).
    limit_strength: bool,
    /// UCI `UCI_Elo` setting. Only used when `limit_strength` is true.
    elo: i32,
}

impl EngineState {
    fn new() -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let ponder_hit_soft_ms = Arc::new(AtomicU64::new(u64::MAX));
        let ponder_hit_hard_ms = Arc::new(AtomicU64::new(u64::MAX));
        let mut info = SearchInfo::new_with_stop(Arc::clone(&stop_flag));
        // Ensure info's stop_flag is the same Arc as our stop_flag
        info.stop_flag = Arc::clone(&stop_flag);
        info.ponder_hit_soft_ms = Arc::clone(&ponder_hit_soft_ms);
        info.ponder_hit_hard_ms = Arc::clone(&ponder_hit_hard_ms);
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
            ponder_hit_soft_ms,
            ponder_hit_hard_ms,
            pending_ponder_allocation: None,
            ponder_started_at: None,
            multipv: 1,
            move_overhead_ms: pet_dragon_lib::search::time::OVERHEAD_MS,
            skill_level: pet_dragon_lib::search::skill::MAX_SKILL_LEVEL,
            contempt: 0,
            limit_strength: false,
            elo: pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize],
        }
    }

    /// Block until any active search thread finishes.
    /// Recovers the thread's SearchInfo and merges history tables for persistence.
    /// Joins the search thread if one is running, folding its ordering
    /// tables (history/countermoves/correction history) back into
    /// `self.info` for the next move, same as before this signature
    /// widened. Returns the joined thread's actual `SearchInfo` — the
    /// genuine values it searched with (`skill_level`, `contempt`, etc.),
    /// not merely a copy of what `EngineState` was configured with. Existing
    /// non-test callers can keep ignoring the return value exactly as
    /// before (`state.wait_for_search();` still compiles unchanged); tests
    /// that need to verify a config value actually reached the search
    /// thread — rather than just that `EngineState` itself wasn't
    /// unexpectedly mutated — should capture and inspect this instead.
    fn wait_for_search(&mut self) -> Option<SearchInfo> {
        if let Some(handle) = self.search_handle.take() {
            if let Ok(returned_info) = handle.join() {
                // Preserve ordering tables across moves (gives better move ordering)
                self.info.history      = returned_info.history;
                self.info.countermoves = returned_info.countermoves;
                self.info.correction_history = returned_info.correction_history.clone();
                return Some(returned_info);
            }
        }
        None
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
        // A new game should never carry a stale ponder-hit override or
        // pending allocation from whatever the engine was doing before.
        self.ponder_hit_soft_ms.store(u64::MAX, Ordering::Relaxed);
        self.ponder_hit_hard_ms.store(u64::MAX, Ordering::Relaxed);
        self.pending_ponder_allocation = None;
        self.ponder_started_at = None;
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
        } else if line == "ponderhit" {
            // Do NOT wait_for_search() here — the ponder search is actively
            // running and this must not block; we only signal it.
            cmd_ponderhit(&mut state);
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
    println!("option name NNUEWeight type spin default 0 min 0 max 100");
    // Phase 19: standard UCI options for analysis GUIs. Neither affects
    // playing strength — MultiPV reports extra candidate lines, Move
    // Overhead just tunes the safety buffer for slow/laggy connections.
    println!(
        "option name MultiPV type spin default 1 min 1 max {}",
        MAX_MULTIPV
    );
    println!(
        "option name Move Overhead type spin default {} min 0 max 5000",
        pet_dragon_lib::search::time::OVERHEAD_MS
    );
    // Phase 20 / D39: difficulty as depth-cap tiers, NOT UCI_Elo — no
    // calibrated human-comparable number attached (DECISIONS.md D39).
    // Default (max) means full strength, byte-identical to pre-Phase-20
    // behavior for any GUI that never touches this option.
    println!(
        "option name Skill Level type spin default {} min 0 max {}",
        pet_dragon_lib::search::skill::MAX_SKILL_LEVEL,
        pet_dragon_lib::search::skill::MAX_SKILL_LEVEL
    );
    // Standard UCI option some GUIs check for before ever sending
    // "go ... ponder"/"ponderhit" — the underlying logic (cmd_go's
    // pending_ponder_allocation, cmd_ponderhit) already works regardless
    // of this declaration; it was just never advertised. No setoption
    // state needed: whether pondering actually happens is entirely
    // determined by whether the GUI sends "go ... ponder", not by this
    // toggle's value (same as how Stockfish treats it).
    println!("option name Ponder type check default true");
    // Contempt: positive = this engine dislikes draws and steers away from
    // them when a genuinely better alternative exists; negative = actively
    // seeks draws. Applied via draw_score() at every draw-detection site
    // in alpha_beta.rs. Default 0 = byte-identical to pre-Contempt
    // behavior (see SearchInfo::contempt's doc comment for the full
    // root-relative sign convention).
    println!("option name Contempt type spin default 0 min -100 max 100");
    // UCI_LimitStrength/UCI_Elo (D43, overrides D39's original rejection of
    // this specific option). When UCI_LimitStrength is true, UCI_Elo
    // overrides Skill Level via elo_to_skill_level() in cmd_go. Range
    // matches search::skill::ELO_TABLE's own min/max exactly (1200-2600)
    // — see that table's doc comment and DECISIONS.md D43 for what these
    // numbers are and are not.
    println!("option name UCI_LimitStrength type check default false");
    println!(
        "option name UCI_Elo type spin default {} min {} max {}",
        pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize],
        pet_dragon_lib::search::skill::ELO_TABLE[0],
        pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize]
    );
    println!();
    println!("uciok");
}

// ── setoption ─────────────────────────────────────────────────────────────────

/// Handle "setoption name <Name> value <Value>"
///
/// Both `<Name>` and `<Value>` can contain spaces per the UCI spec (e.g.
/// "Move Overhead" is a two-word option name; a Windows SyzygyPath can
/// contain spaces too) — find the "value" token and split there instead of
/// assuming single-word name/value at fixed positions (Phase 19: this used
/// to just take `tokens[2]`/`tokens[4]`, which silently mis-parsed any
/// multi-word name and truncated any multi-word value to its first token).
fn cmd_setoption(state: &mut EngineState, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 || tokens[1] != "name" { return; }

    let value_idx = tokens.iter().position(|&t| t == "value");
    let name_end  = value_idx.unwrap_or(tokens.len());
    let name: String = tokens[2..name_end].join(" ").to_lowercase();
    let value: String = match value_idx {
        Some(vi) if vi + 1 < tokens.len() => tokens[vi + 1..].join(" "),
        _ => String::new(),
    };
    let value = value.as_str();

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
        "nnueweight" => {
            if let Ok(pct) = value.parse::<u32>() {
                set_nnue_weight_pct(pct);
            }
        }
        "move overhead" => {
            // Phase 19: runtime-configurable safety buffer, replacing the
            // old hardcoded OVERHEAD_MS constant. Applied in cmd_go via
            // TimeControl::overhead_ms, not here — this only records the
            // setting.
            if let Ok(ms) = value.parse::<u64>() {
                state.move_overhead_ms = ms.clamp(0, 5000);
            }
        }
        "multipv" => {
            // Phase 19: how many principal variations to report. Clamped
            // to a sane declared max here; iterative_deepening() further
            // clamps against the actual number of legal root moves at
            // search time, so requesting more than exist is harmless.
            if let Ok(n) = value.parse::<usize>() {
                state.multipv = n.clamp(1, MAX_MULTIPV);
            }
        }
        "skill level" => {
            // Phase 20 / D39: unlike multipv/move overhead above, this DOES
            // change playing strength on purpose. Clamped to the declared
            // 0..=MAX_SKILL_LEVEL range; applied to both the depth cap and
            // the time-fraction budget in cmd_go, not here — this only
            // records the setting.
            if let Ok(n) = value.parse::<u8>() {
                state.skill_level = n.min(pet_dragon_lib::search::skill::MAX_SKILL_LEVEL);
            }
        }
        "ponder" => {
            // Accepted for GUI compatibility, but genuinely a no-op: unlike
            // every other option here, whether pondering happens is decided
            // entirely by whether the GUI sends "go ... ponder", not by
            // this toggle's value — there's no engine-side state to record.
        }
        "contempt" => {
            if let Ok(n) = value.parse::<i32>() {
                state.contempt = n.clamp(-100, 100);
            }
        }
        "uci_limitstrength" => {
            state.limit_strength = value.eq_ignore_ascii_case("true");
        }
        "uci_elo" => {
            if let Ok(n) = value.parse::<i32>() {
                state.elo = n.clamp(
                    pet_dragon_lib::search::skill::ELO_TABLE[0],
                    pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize],
                );
            }
        }
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
/// Resolves the Skill Level actually used for a search: `UCI_Elo` (via
/// `elo_to_skill_level()`) when `UCI_LimitStrength` is enabled, otherwise
/// the manually-set `Skill Level` unchanged (D43's override relationship —
/// same as Stockfish's own UCI_LimitStrength/Skill Level pairing).
///
/// Extracted as a pure function (no search, no threading) so both `cmd_go`
/// and its depth-cap AND time-fraction consumers use one single source of
/// truth for this value — and so it's directly unit-testable without
/// spawning and joining a search thread just to check a config mapping.
fn effective_skill_level(state: &EngineState) -> u8 {
    if state.limit_strength {
        pet_dragon_lib::search::skill::elo_to_skill_level(state.elo)
    } else {
        state.skill_level
    }
}

/// Builds the `TimeControl` for a `go` command: parses the line, then
/// applies the two persistent session settings that aren't part of the
/// `go` line itself — Move Overhead and the Skill Level time-fraction
/// pairing (Session 65 — depth cap alone isn't enough; a low tier also
/// needs a smaller time budget, or it searches shallow and then just sits
/// idle for the rest of its allocation, looking broken rather than weak).
///
/// Takes `skill_level` as an explicit parameter — the caller's
/// `effective_skill_level(state)`, NOT `state.skill_level` directly —
/// so the time-fraction pairing stays correct even when `UCI_LimitStrength`
/// is active and the two diverge. Extracted as a pure function for the
/// same testability reason as `effective_skill_level` above.
fn build_time_control(state: &EngineState, line: &str, skill_level: u8) -> TimeControl {
    let mut tc = parse_go(line);
    tc.overhead_ms = state.move_overhead_ms;
    tc.skill_time_fraction_pct =
        pet_dragon_lib::search::skill::skill_time_fraction_pct(skill_level);
    tc
}

fn cmd_go(state: &mut EngineState, line: &str) {
    // Resolved once, up front, so the depth cap (applied later, per-thread)
    // and the time-fraction pairing (applied here) can never diverge —
    // see build_time_control's doc comment for why that divergence was a
    // real bug until this extraction.
    let skill_level = effective_skill_level(state);
    let tc = build_time_control(state, line, skill_level);

    // Reset stop flag for the new search
    state.stop_flag.store(false, Ordering::SeqCst);

    // ── Pondering setup (Phase 18/D37) ────────────────────────────────────────
    // Always clear any leftover override from a previous search first.
    state.ponder_hit_soft_ms.store(u64::MAX, Ordering::Relaxed);
    state.ponder_hit_hard_ms.store(u64::MAX, Ordering::Relaxed);
    if tc.ponder {
        // Precompute what the REAL time budget would be if this were an
        // ordinary (non-ponder) go with the same clock values — this is
        // exactly what allocate_time() already computes for a normal
        // search; ponder just tells it to ignore the clock and run near-
        // infinitely instead (see search/time.rs). Stash the real budget
        // now so cmd_ponderhit can apply it later without needing to
        // re-parse the original go line.
        let is_white = state.pos.side_to_move == Color::White;
        let mut real_tc = tc.clone();
        real_tc.ponder = false;
        state.pending_ponder_allocation = Some(allocate_time(&real_tc, is_white));
        state.ponder_started_at = Some(Instant::now());
    } else {
        state.pending_ponder_allocation = None;
        state.ponder_started_at = None;
    }

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
    let ponder_hit_soft_ms = Arc::clone(&state.ponder_hit_soft_ms);
    let ponder_hit_hard_ms = Arc::clone(&state.ponder_hit_hard_ms);
    let multipv   = state.multipv;
    // skill_level was already resolved once at the top of this function
    // (via effective_skill_level) — reused here as-is for both threads'
    // SearchInfo, same value build_time_control used for the time-fraction
    // pairing above.
    let contempt  = state.contempt;

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
                // Phase 20: helpers must respect the same Skill Level depth
                // cap as the main thread — otherwise they'd populate the
                // shared TT with deeper, full-strength lines that leak back
                // into a low-skill main search's move ordering/scores.
                h_info.skill_level = skill_level;
                h_info.contempt = contempt;
                iterative_deepening(&mut h_pos, &h_tc, &mut h_info, &*h_tt)
            }));
        }

        // ── Main search thread ────────────────────────────────────────────────
        let mut main_pos  = pos;
        let mut main_info = SearchInfo::new_with_stop(Arc::clone(&stop_flag));
        main_info.history      = history;
        main_info.countermoves = countermoves;
        main_info.correction_history = correction;
        main_info.ponder_hit_soft_ms = ponder_hit_soft_ms;
        main_info.ponder_hit_hard_ms = ponder_hit_hard_ms;
        // Phase 19: MultiPV reporting comes from the main thread only —
        // helper threads (above) exist purely to populate the shared TT
        // faster and never print `info` lines for the primary line either,
        // so there's no "authoritative main thread" ambiguity to create by
        // leaving their SearchInfo at the single-PV default.
        main_info.multipv = multipv;
        main_info.skill_level = skill_level;
        main_info.contempt = contempt;
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

// ── ponderhit ─────────────────────────────────────────────────────────────────

/// Handle "ponderhit" — the GUI confirms our predicted ponder move was
/// actually played. Converts the currently-running, effectively-infinite
/// `go ponder` search into a properly time-bounded one, using the real time
/// control from the original `go` command (precomputed in cmd_go).
///
/// Pondering time is free per the UCI spec: the real budget is expressed
/// relative to how much time has already elapsed since pondering began, not
/// reset to zero. It can't be reset to zero here even if we wanted to —
/// `SearchInfo::start_time` is owned by the (already-running) search thread,
/// not shared, so this function can only ever hand the search thread a new
/// *deadline* via the shared atomics, never rewind its clock (see D37).
///
/// Silently does nothing if no ponder search is currently active (e.g. a
/// stray or duplicate `ponderhit`, or one that arrives after the search
/// already finished on its own) — matches this file's existing convention
/// of silently ignoring inapplicable commands (UCI spec §3.1).
fn cmd_ponderhit(state: &mut EngineState) {
    let allocation = state.pending_ponder_allocation.take();
    let started_at = state.ponder_started_at.take();

    if let (Some((soft, hard)), Some(started_at)) = (allocation, started_at) {
        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        state.ponder_hit_soft_ms.store(elapsed_ms.saturating_add(soft), Ordering::Relaxed);
        state.ponder_hit_hard_ms.store(elapsed_ms.saturating_add(hard), Ordering::Relaxed);
    }
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
    fn test_skill_level_option_multiword_name() {
        setup();
        let mut state = EngineState::new();
        // "Skill Level" is two words — same parsing hazard as Move Overhead.
        cmd_setoption(&mut state, "setoption name Skill Level value 5");
        assert_eq!(state.skill_level, 5,
            "a two-word option name must parse correctly");
    }

    #[test]
    fn test_skill_level_option_defaults_to_max() {
        setup();
        let state = EngineState::new();
        assert_eq!(
            state.skill_level,
            pet_dragon_lib::search::skill::MAX_SKILL_LEVEL,
            "Skill Level should default to full strength for any GUI that \
             never touches this option"
        );
    }

    #[test]
    fn test_skill_level_option_clamped_to_max() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Skill Level value 255");
        assert_eq!(state.skill_level, pet_dragon_lib::search::skill::MAX_SKILL_LEVEL,
            "Skill Level should be clamped to the declared maximum");
    }

    #[test]
    fn test_effective_skill_level_matches_manual_setting_when_limit_strength_off() {
        setup();
        let mut state = EngineState::new();
        state.skill_level = 5;
        state.limit_strength = false; // default, but explicit here for clarity
        assert_eq!(effective_skill_level(&state), 5,
            "With UCI_LimitStrength off, effective_skill_level must be exactly \
             the manually-set Skill Level, regardless of UCI_Elo's value");
    }

    #[test]
    fn test_cmd_go_applies_skill_level_to_search() {
        setup();
        let mut state = EngineState::new();
        state.skill_level = 0; // weakest tier -> depth cap of 1
        cmd_go(&mut state, "go depth 10");
        let returned = state.wait_for_search();
        // Real proof, not an indirect state check: the SearchInfo the
        // search thread actually built and searched with (joined via the
        // now-widened wait_for_search()) must itself carry skill_level=0
        // — this fails if cmd_go ever stops correctly threading the value
        // into main_info, even if EngineState.skill_level itself is
        // untouched (which it always is — cmd_go only ever reads it).
        let returned = returned.expect("search should have produced a SearchInfo");
        assert_eq!(returned.skill_level, 0,
            "The SearchInfo actually used by the search thread must reflect \
             the configured Skill Level, not just EngineState's own copy of it");
    }

    #[test]
    fn test_contempt_option_defaults_to_zero() {
        setup();
        let state = EngineState::new();
        assert_eq!(state.contempt, 0,
            "Contempt should default to 0 — byte-identical to pre-Contempt \
             behavior for any GUI that never touches this option");
    }

    #[test]
    fn test_contempt_option_clamps_to_declared_range() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Contempt value 500");
        assert_eq!(state.contempt, 100,
            "Contempt should clamp to the declared max of 100");
        cmd_setoption(&mut state, "setoption name Contempt value -500");
        assert_eq!(state.contempt, -100,
            "Contempt should clamp to the declared min of -100");
    }

    #[test]
    fn test_contempt_option_accepts_value_within_range() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Contempt value 30");
        assert_eq!(state.contempt, 30);
    }

    #[test]
    fn test_cmd_go_applies_contempt_to_search() {
        setup();
        let mut state = EngineState::new();
        state.contempt = 40;
        cmd_go(&mut state, "go depth 4");
        let returned = state.wait_for_search()
            .expect("search should have produced a SearchInfo");
        // Real proof: the SearchInfo the search thread actually built and
        // searched with must itself carry contempt=40 — this is exactly
        // the kind of copy-paste bug (e.g. `main_info.contempt = 0`
        // instead of `= contempt`) a state-field-only check can never
        // catch, since cmd_go never writes contempt back to EngineState
        // either way. draw_score()'s own math is covered separately in
        // search/mod.rs's and alpha_beta.rs's tests; this test's job is
        // narrower and different: prove cmd_go's wiring, not the formula.
        assert_eq!(returned.contempt, 40,
            "The SearchInfo actually used by the search thread must reflect \
             the configured Contempt value, not just EngineState's own copy of it");
    }

    #[test]
    fn test_limit_strength_defaults_to_disabled() {
        setup();
        let state = EngineState::new();
        assert!(!state.limit_strength,
            "UCI_LimitStrength should default to false — full Skill Level \
             control, unaffected by UCI_Elo, for any GUI that never touches this option");
    }

    #[test]
    fn test_elo_defaults_to_max_table_value() {
        setup();
        let state = EngineState::new();
        assert_eq!(state.elo,
            pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize],
            "UCI_Elo's default must be the table's max — so enabling \
             UCI_LimitStrength without ever setting UCI_Elo can't silently weaken the engine");
    }

    #[test]
    fn test_uci_limitstrength_setoption_toggles() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name UCI_LimitStrength value true");
        assert!(state.limit_strength);
        cmd_setoption(&mut state, "setoption name UCI_LimitStrength value false");
        assert!(!state.limit_strength);
    }

    #[test]
    fn test_uci_elo_setoption_clamps_to_table_range() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name UCI_Elo value 9999");
        assert_eq!(state.elo,
            pet_dragon_lib::search::skill::ELO_TABLE[pet_dragon_lib::search::skill::MAX_SKILL_LEVEL as usize]);
        cmd_setoption(&mut state, "setoption name UCI_Elo value 0");
        assert_eq!(state.elo, pet_dragon_lib::search::skill::ELO_TABLE[0]);
    }

    #[test]
    fn test_uci_elo_setoption_accepts_value_within_range() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name UCI_Elo value 2000");
        assert_eq!(state.elo, 2000);
    }

    #[test]
    fn test_effective_skill_level_ignores_elo_when_limit_strength_off() {
        setup();
        let mut state = EngineState::new();
        state.skill_level = 7;
        state.limit_strength = false;
        state.elo = pet_dragon_lib::search::skill::ELO_TABLE[0]; // would map to level 0 if active
        assert_eq!(effective_skill_level(&state), 7,
            "UCI_Elo must be completely ignored while UCI_LimitStrength is off, \
             even when it's set to a value that would map to a very different level");
    }

    #[test]
    fn test_effective_skill_level_uses_elo_when_limit_strength_on() {
        setup();
        let mut state = EngineState::new();
        state.skill_level = 20; // must be ignored once limit_strength is active
        state.limit_strength = true;
        state.elo = pet_dragon_lib::search::skill::ELO_TABLE[3]; // exact anchor for level 3
        assert_eq!(effective_skill_level(&state), 3,
            "UCI_LimitStrength=true must make UCI_Elo override the manually-set \
             Skill Level entirely, not blend or average with it");
    }

    #[test]
    fn test_cmd_go_search_reflects_elo_override_not_raw_skill_level() {
        setup();
        let mut state = EngineState::new();
        state.skill_level = 20; // would be full strength if the override didn't apply
        state.limit_strength = true;
        state.elo = pet_dragon_lib::search::skill::ELO_TABLE[3]; // exact anchor for level 3
        cmd_go(&mut state, "go depth 4");
        let returned = state.wait_for_search()
            .expect("search should have produced a SearchInfo");
        // Real end-to-end proof, not just the pure-function checks above:
        // the SearchInfo the search thread actually used must carry the
        // Elo-derived level (3), never the raw stored Skill Level (20) —
        // this is the specific case a state-field-only check could never
        // catch, since it's exactly the two values disagreeing that
        // matters here.
        assert_eq!(returned.skill_level, 3,
            "The SearchInfo actually used by the search thread must reflect \
             the Elo-derived Skill Level when UCI_LimitStrength is active, \
             not EngineState's raw (and here deliberately conflicting) Skill Level");
    }

    #[test]
    fn test_build_time_control_uses_elo_derived_skill_level_not_raw() {
        // Regression test for the bug this refactor fixed: before
        // effective_skill_level() was resolved once up front, cmd_go
        // computed tc.skill_time_fraction_pct from state.skill_level
        // directly — BEFORE the Elo override was applied — so a low
        // UCI_Elo request got the correct (shallow) depth cap but the
        // WRONG (full, unreduced) time budget, defeating the whole point
        // of the Session 65 depth+time pairing for low tiers.
        setup();
        let mut state = EngineState::new();
        state.skill_level = 20; // raw setting says "no time reduction"
        let elo_derived_level = 3; // but the Elo-derived level says otherwise
        let tc = build_time_control(&state, "go movetime 1000", elo_derived_level);
        let expected_pct =
            pet_dragon_lib::search::skill::skill_time_fraction_pct(elo_derived_level);
        assert_eq!(tc.skill_time_fraction_pct, expected_pct,
            "build_time_control must use the skill_level PARAMETER it was given \
             (the Elo-derived effective level), never state.skill_level directly");
        assert_ne!(tc.skill_time_fraction_pct,
            pet_dragon_lib::search::skill::skill_time_fraction_pct(state.skill_level),
            "This assertion only means something because level 20's time \
             fraction (100%, no reduction) genuinely differs from level 3's — \
             confirming the test setup actually exercises the bug scenario");
    }

    #[test]
    fn test_ponder_setoption_accepted_without_error() {
        setup();
        let mut state = EngineState::new();
        // Ponder has no engine-side state to record — this just confirms
        // setoption accepts it cleanly (doesn't fall through to a path
        // that would panic or corrupt unrelated state).
        state.skill_level = 12;
        cmd_setoption(&mut state, "setoption name Ponder value true");
        assert_eq!(state.skill_level, 12,
            "Ponder setoption must not disturb unrelated state");
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
    fn test_multipv_option() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name MultiPV value 3");
        assert_eq!(state.multipv, 3, "MultiPV option should update state");
    }

    #[test]
    fn test_multipv_option_clamped_to_max() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name MultiPV value 99999");
        assert_eq!(state.multipv, MAX_MULTIPV,
            "MultiPV should be clamped to the declared maximum");
    }

    #[test]
    fn test_multipv_option_defaults_to_one() {
        setup();
        let state = EngineState::new();
        assert_eq!(state.multipv, 1);
    }

    #[test]
    fn test_move_overhead_option_multiword_name() {
        setup();
        let mut state = EngineState::new();
        // This is the exact case the old tokens[2]-only parsing broke:
        // "Move Overhead" is two words.
        cmd_setoption(&mut state, "setoption name Move Overhead value 100");
        assert_eq!(state.move_overhead_ms, 100,
            "a two-word option name must parse correctly");
    }

    #[test]
    fn test_move_overhead_option_defaults_to_constant() {
        setup();
        let state = EngineState::new();
        assert_eq!(
            state.move_overhead_ms,
            pet_dragon_lib::search::time::OVERHEAD_MS
        );
    }

    #[test]
    fn test_move_overhead_option_clamped() {
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Move Overhead value 999999");
        assert_eq!(state.move_overhead_ms, 5000,
            "Move Overhead should be clamped to a sane maximum");
    }

    #[test]
    fn test_setoption_single_word_value_still_works() {
        // Regression guard: the rewritten parser must not break the
        // existing single-token-name/single-token-value case.
        setup();
        let mut state = EngineState::new();
        cmd_setoption(&mut state, "setoption name Threads value 4");
        assert_eq!(state.threads, 4);
        cmd_setoption(&mut state, "setoption name Hash value 32");
        assert_eq!(state.hash_mb, 32);
    }

    #[test]
    fn test_cmd_go_applies_move_overhead_to_time_control() {
        setup();
        let mut state = EngineState::new();
        state.move_overhead_ms = 250;
        // Real, direct proof — build_time_control is the exact function
        // cmd_go itself calls, so this isn't "can't reach into the spawned
        // thread" hand-waving anymore, it's the actual value construction.
        let tc = build_time_control(&state, "go movetime 1000", effective_skill_level(&state));
        assert_eq!(tc.overhead_ms, 250,
            "build_time_control must apply state.move_overhead_ms to overhead_ms exactly");
        assert_eq!(tc.movetime, 1000,
            "The go line's own movetime must still parse through unchanged");
        // allocate_time()'s own tests (search/time.rs) cover the actual
        // (movetime - overhead) budget-reduction arithmetic; this test's
        // job is narrower: prove cmd_go's config wiring into TimeControl,
        // not re-derive allocate_time's math.
    }

    #[test]
    fn test_wait_for_search_no_handle() {
        setup();
        let mut state = EngineState::new();
        // Should not panic when no search is running
        state.wait_for_search();
    }

    #[test]
    fn test_ponderhit_without_pending_allocation_is_noop() {
        setup();
        let mut state = EngineState::new();
        // No `go ponder` was ever issued — a stray ponderhit should do nothing.
        cmd_ponderhit(&mut state);
        assert_eq!(state.ponder_hit_soft_ms.load(Ordering::Relaxed), u64::MAX);
        assert_eq!(state.ponder_hit_hard_ms.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn test_ponderhit_computes_and_stores_override() {
        setup();
        let mut state = EngineState::new();
        state.pending_ponder_allocation = Some((100, 200));
        state.ponder_started_at = Some(Instant::now());

        cmd_ponderhit(&mut state);

        let soft = state.ponder_hit_soft_ms.load(Ordering::Relaxed);
        let hard = state.ponder_hit_hard_ms.load(Ordering::Relaxed);
        // Should be roughly elapsed(~0ms) + 100 / elapsed(~0ms) + 200 —
        // generous upper bound to avoid CI timing flakiness.
        assert!(soft >= 100 && soft < 1000,
            "expected soft override near 100ms, got {soft}");
        assert!(hard >= 200 && hard < 1100,
            "expected hard override near 200ms, got {hard}");
        assert!(hard > soft, "hard override should exceed soft override");

        // Consumed exactly once.
        assert!(state.pending_ponder_allocation.is_none());
        assert!(state.ponder_started_at.is_none());
    }

    #[test]
    fn test_ponderhit_is_idempotent_after_consumption() {
        setup();
        let mut state = EngineState::new();
        state.pending_ponder_allocation = Some((50, 100));
        state.ponder_started_at = Some(Instant::now());

        cmd_ponderhit(&mut state);
        let soft_after_first = state.ponder_hit_soft_ms.load(Ordering::Relaxed);

        // A second ponderhit with nothing pending should not change anything.
        cmd_ponderhit(&mut state);
        assert_eq!(state.ponder_hit_soft_ms.load(Ordering::Relaxed), soft_after_first);
    }

    #[test]
    fn test_cmd_go_ponder_sets_pending_allocation() {
        setup();
        let mut state = EngineState::new();
        cmd_go(&mut state, "go ponder wtime 60000 btime 60000");
        assert!(state.pending_ponder_allocation.is_some(),
            "a ponder go should precompute the real allocation");
        assert!(state.ponder_started_at.is_some());
        state.stop_search();
    }

    #[test]
    fn test_cmd_go_non_ponder_clears_pending_allocation() {
        setup();
        let mut state = EngineState::new();
        cmd_go(&mut state, "go ponder wtime 60000 btime 60000");
        state.stop_search();
        cmd_go(&mut state, "go movetime 50");
        assert!(state.pending_ponder_allocation.is_none(),
            "a normal go should clear any leftover ponder allocation");
        assert!(state.ponder_started_at.is_none());
        state.stop_search();
    }
}
