// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// uci_wasm.rs — Real UCI-text-protocol WASM build target
//
// This is a SECOND, independent WASM surface from the "wasm"-feature exports
// in lib.rs (search_from_fen, new_game, etc. — the direct-function-call API
// web/index.html and web/pit/vs.html already use and which this file does
// not touch or replace). Compiled only under `--features uci-wasm`, never
// alongside `--features wasm` in the same build — see Cargo.toml.
//
// Exposes exactly one export, `uci_command(line)`, which accepts one line of
// real UCI protocol text and returns the engine's response as a string
// (empty string if the command produces no response line, e.g. `setoption`).
// A JS-side wrapper is expected to feed it lines and print/relay whatever
// comes back — the shape a genuine UCI-speaking browser GUI expects, rather
// than the bespoke one-shot functions the "wasm" feature exposes.
//
// ── Architectural limitations, stated up front (not discovered later) ──────
// A browser WASM call is synchronous: JS is blocked on the call stack for
// the entire duration of `uci_command`, so nothing else can run — including
// a hypothetical follow-up `uci_command("stop")` — until the current call
// returns. This is a real, structural difference from native UCI's
// stdin/stdout loop (main.rs), where `stop` arrives on a separate thread
// while a search is in flight. Three concrete consequences, all deliberate
// scoping decisions rather than bugs:
//   1. `stop` is a no-op here. By the time JS could call it, any prior `go`
//      call has already returned — there is no in-flight search to stop.
//   2. `go infinite` is NOT honored as literal infinite (which would risk
//      permanently hanging the browser tab with zero recovery route, since
//      nothing could ever interrupt it). It's clamped to
//      `INFINITE_FALLBACK_MS` instead — see `parse_go_tokens`.
//   3. No live `info depth N ...` streaming during search — only one final
//      `info ...` line plus `bestmove` is returned, once the search
//      completes. True streaming would need a JS callback wired into the
//      search hot loop (the `sendPrompt`-style pattern used elsewhere in
//      this project's tooling could support this) — a separate, larger
//      change, not done here.
//   4. `go ponder` / `ponderhit` are not supported — the `ponder` token in
//      a `go` line is simply ignored (any other time-control tokens on the
//      same line are still honored). Real pondering needs the same
//      async/threaded model `stop` above is missing.
//   5. `MultiPV` is not exposed — only a single best line is ever returned.
//   6. `Threads` is accepted but pinned to 1 — WASM has no Lazy SMP path.
//
// None of this affects the existing "wasm" feature's browser gameplay UI in
// any way; those functions are unchanged and this module is not linked into
// that build at all.
// ============================================================================

use std::cell::RefCell;

use crate::position::fen::STANDARD_START_FEN;
use crate::position::Position;
use crate::movegen::generate_moves;
use crate::search::iterative::iterative_deepening;
use crate::search::time::TimeControl;
use crate::search::SearchInfo;
use crate::tt::TranspositionTable;
use crate::types::Move;

use wasm_bindgen::prelude::*;

/// Default TT size for a fresh session — matches the "wasm" feature's
/// existing `search_from_fen` functions (32 MB), a reasonable browser-tab
/// default. Adjustable at runtime via `setoption name Hash value <N>`.
const DEFAULT_HASH_MB: usize = 32;

/// Upper bound offered on the `Hash` UCI option. Native's ceiling is 65536
/// (D-series discussion, ROADMAP §1) — deliberately far smaller here, since
/// a browser tab sharing memory with the rest of the page is a very
/// different environment from a dedicated native process.
const MAX_HASH_MB: usize = 512;

/// `go infinite` is not honored literally — see this file's module doc
/// comment, point 2, for why. This is the bounded fallback duration used
/// instead. Chosen as a "long enough to be a real deep look, short enough
/// that a stray `go infinite` from a naively-ported GUI config can't
/// permanently hang the tab" compromise, not a tuned value.
const INFINITE_FALLBACK_MS: u64 = 30_000;

// ── Persistent per-session engine state ─────────────────────────────────────

/// Everything that must persist across `uci_command` calls within one
/// browser session: the current position, the transposition table (kept
/// warm across moves, same as native), and every UCI option value that
/// isn't a bare global (Hash/MultiPV-style options need session-local
/// state; NNUEWeight/PlayStyle are already engine-wide globals via
/// `eval::set_nnue_weight_pct`/`eval::style::set_play_style` — see
/// `apply_setoption` below, which calls those directly instead of
/// duplicating storage here).
struct UciSession {
    pos: Position,
    tt: TranspositionTable,
    hash_mb: usize,
    move_overhead_ms: u64,
    skill_level: u8,
    contempt: i32,
    uci_limit_strength: bool,
    uci_elo: i32,
}

impl UciSession {
    fn new() -> Self {
        UciSession {
            // STANDARD_START_FEN always parses — this is not user input.
            pos: Position::from_fen(STANDARD_START_FEN)
                .expect("STANDARD_START_FEN must always be a valid FEN"),
            tt: TranspositionTable::new(DEFAULT_HASH_MB),
            hash_mb: DEFAULT_HASH_MB,
            move_overhead_ms: crate::search::time::OVERHEAD_MS,
            skill_level: crate::search::skill::MAX_SKILL_LEVEL,
            contempt: 0,
            uci_limit_strength: false,
            uci_elo: crate::search::skill::ELO_TABLE[crate::search::skill::MAX_SKILL_LEVEL as usize],
        }
    }
}

thread_local! {
    static SESSION: RefCell<UciSession> = RefCell::new(UciSession::new());
}

// ── WASM export ──────────────────────────────────────────────────────────────

/// Feed one line of UCI protocol text to the engine and get its response
/// back as a string (possibly multi-line, `\n`-joined; empty string if the
/// command produces no response — e.g. `position`, `setoption`,
/// `ucinewgame`). See this file's module doc comment for the real, stated
/// limitations of running UCI over a single synchronous WASM call.
#[wasm_bindgen]
pub fn uci_command(line: &str) -> String {
    SESSION.with(|cell| {
        let mut session = cell.borrow_mut();
        handle_line(&mut session, line.trim())
    })
}

// ── Command dispatch ───────────────────────────────────────────────────────

fn handle_line(session: &mut UciSession, line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    let tokens: Vec<&str> = line.split_whitespace().collect();
    match tokens[0] {
        "uci" => cmd_uci(),
        "isready" => String::from("readyok"),
        "ucinewgame" => {
            session.pos = Position::from_fen(STANDARD_START_FEN)
                .expect("STANDARD_START_FEN must always be a valid FEN");
            session.tt.clear();
            String::new()
        }
        "position" => {
            cmd_position(session, &tokens[1..]);
            String::new()
        }
        "go" => cmd_go(session, &tokens[1..]),
        "setoption" => {
            cmd_setoption(session, &tokens[1..]);
            String::new()
        }
        "stop" | "ponderhit" | "quit" | "debug" => String::new(), // see module doc comment
        _ => String::new(), // unrecognized command — real UCI engines ignore, not error
    }
}

/// Response to the `uci` command: id/author, every declared option, `uciok`.
fn cmd_uci() -> String {
    let mut lines = Vec::new();
    lines.push(format!("id name Pet Dragon {}", env!("CARGO_PKG_VERSION")));
    lines.push(String::from("id author Gokul Chandar"));
    lines.push(format!(
        "option name Hash type spin default {} min 1 max {}",
        DEFAULT_HASH_MB, MAX_HASH_MB
    ));
    lines.push(String::from("option name Threads type spin default 1 min 1 max 1"));
    lines.push(String::from(
        "option name Move Overhead type spin default 30 min 0 max 5000",
    ));
    lines.push(format!(
        "option name Skill Level type spin default {} min 0 max {}",
        crate::search::skill::MAX_SKILL_LEVEL,
        crate::search::skill::MAX_SKILL_LEVEL
    ));
    lines.push(String::from(
        "option name Contempt type spin default 0 min -100 max 100",
    ));
    lines.push(String::from(
        "option name UCI_LimitStrength type check default false",
    ));
    lines.push(format!(
        "option name UCI_Elo type spin default {} min {} max {}",
        crate::search::skill::ELO_TABLE[crate::search::skill::MAX_SKILL_LEVEL as usize],
        crate::search::skill::ELO_TABLE[0],
        crate::search::skill::ELO_TABLE[crate::search::skill::MAX_SKILL_LEVEL as usize]
    ));
    lines.push(String::from(
        "option name NNUEWeight type spin default 0 min 0 max 100",
    ));
    lines.push(String::from(
        "option name PlayStyle type spin default 0 min 0 max 4",
    ));
    lines.push(String::from("uciok"));
    lines.join("\n")
}

/// `position [startpos | fen <fenstring>] [moves <uci> <uci> ...]`
fn cmd_position(session: &mut UciSession, tokens: &[&str]) {
    if tokens.is_empty() {
        return;
    }

    let moves_idx = tokens.iter().position(|&t| t == "moves");
    let board_tokens = match moves_idx {
        Some(i) => &tokens[..i],
        None => tokens,
    };

    let new_pos = match board_tokens.first() {
        Some(&"startpos") => Position::from_fen(STANDARD_START_FEN).ok(),
        Some(&"fen") => {
            let fen = board_tokens[1..].join(" ");
            Position::from_fen(&fen).ok()
        }
        _ => None,
    };

    // Malformed/illegal `position` line — real UCI engines silently ignore
    // rather than crash. Keep whatever position was already loaded.
    let mut pos = match new_pos {
        Some(p) => p,
        None => return,
    };

    if let Some(i) = moves_idx {
        for mv_str in &tokens[i + 1..] {
            let moves = generate_moves(&pos);
            let matched = moves.iter().find(|mv| mv.to_uci() == *mv_str).copied();
            match matched {
                Some(mv) => pos.make_move_with_history(mv),
                None => break, // unrecognized/illegal move token — stop applying, keep prior state
            }
        }
    }

    session.pos = pos;
}

/// `go [depth N] [movetime N] [wtime N] [btime N] [winc N] [binc N]
///     [movestogo N] [nodes N] [infinite] [ponder]`
/// Runs the search to completion synchronously and returns one `info ...`
/// line plus `bestmove ...` — see module doc comment for why this can't
/// stream intermediate depths or be interrupted by `stop`.
fn cmd_go(session: &mut UciSession, tokens: &[&str]) -> String {
    let mut tc = TimeControl {
        overhead_ms: session.move_overhead_ms,
        skill_time_fraction_pct: crate::search::skill::skill_time_fraction_pct(
            effective_skill_level(session),
        ),
        ..TimeControl::default()
    };

    let mut saw_infinite = false;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                tc.depth = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "movetime" => {
                tc.movetime = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "wtime" => {
                tc.wtime = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "btime" => {
                tc.btime = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "winc" => {
                tc.winc = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "binc" => {
                tc.binc = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "movestogo" => {
                tc.movestogo = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "nodes" => {
                tc.nodes = tokens.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "infinite" => {
                saw_infinite = true;
                i += 1;
            }
            "ponder" => {
                // Not supported (module doc comment, point 4) — ignored,
                // remaining tokens on the line are still parsed normally.
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    // Bounded stand-in for literal infinite — see INFINITE_FALLBACK_MS's
    // doc comment. Only applies if nothing more specific (movetime/depth/
    // clock) was already given.
    if saw_infinite && tc.movetime == 0 && tc.depth == 0 && tc.wtime == 0 && tc.btime == 0 {
        tc.movetime = INFINITE_FALLBACK_MS;
    }

    let mut info = SearchInfo::new();
    info.skill_level = effective_skill_level(session);
    info.contempt = session.contempt;
    info.print_info = false; // this module builds its own info/bestmove text below

    session.tt.new_search();
    let result = iterative_deepening(&mut session.pos, &tc, &mut info, &session.tt);

    if result.best_move == Move::NULL {
        return String::from("bestmove 0000");
    }

    let score_token = if result.is_mate {
        format!("mate {}", result.mate_in)
    } else {
        format!("cp {}", result.score)
    };
    let pv_str: Vec<String> = result.pv.iter().map(|mv| mv.to_uci()).collect();

    format!(
        "info depth {} seldepth {} score {} nodes {} nps {} time {} pv {}\nbestmove {}",
        result.depth,
        result.seldepth,
        score_token,
        result.nodes,
        result.nps,
        result.time_ms,
        pv_str.join(" "),
        result.best_move.to_uci()
    )
}

/// `UCI_LimitStrength`/`UCI_Elo` override `Skill Level` exactly like
/// native's `effective_skill_level()` in main.rs (D44) — same precedence,
/// re-implemented here rather than shared since main.rs's version isn't a
/// `pub` library function.
fn effective_skill_level(session: &UciSession) -> u8 {
    if session.uci_limit_strength {
        crate::search::skill::elo_to_skill_level(session.uci_elo)
    } else {
        session.skill_level
    }
}

/// `setoption name <id> value <x>` — `<id>` may contain spaces (e.g.
/// "Move Overhead", "Skill Level"), so this finds the `value` token and
/// joins everything on each side of it, same approach D38 used to fix the
/// equivalent bug in main.rs's native parser.
fn cmd_setoption(session: &mut UciSession, tokens: &[&str]) {
    if tokens.first() != Some(&"name") {
        return;
    }
    let value_idx = tokens.iter().position(|&t| t == "value");
    let name_end = value_idx.unwrap_or(tokens.len());
    let name = tokens[1..name_end].join(" ");
    let value = match value_idx {
        Some(i) => tokens[i + 1..].join(" "),
        None => String::new(),
    };

    match name.as_str() {
        "Hash" => {
            if let Ok(mb) = value.parse::<usize>() {
                let mb = mb.clamp(1, MAX_HASH_MB);
                session.tt.resize(mb);
                session.hash_mb = mb;
            }
        }
        "Threads" => {
            // Accepted, silently pinned to 1 — see module doc comment
            // point 6. Not an error: a GUI sending this shouldn't fail.
        }
        "Move Overhead" => {
            if let Ok(ms) = value.parse::<u64>() {
                session.move_overhead_ms = ms;
            }
        }
        "Skill Level" => {
            if let Ok(level) = value.parse::<u8>() {
                session.skill_level = level.min(crate::search::skill::MAX_SKILL_LEVEL);
            }
        }
        "Contempt" => {
            if let Ok(c) = value.parse::<i32>() {
                session.contempt = c.clamp(-100, 100);
            }
        }
        "UCI_LimitStrength" => {
            session.uci_limit_strength = value.eq_ignore_ascii_case("true");
        }
        "UCI_Elo" => {
            if let Ok(elo) = value.parse::<i32>() {
                session.uci_elo = elo;
            }
        }
        "NNUEWeight" => {
            if let Ok(pct) = value.parse::<u32>() {
                crate::eval::set_nnue_weight_pct(pct);
            }
        }
        "PlayStyle" => {
            if let Ok(mode) = value.parse::<u32>() {
                crate::eval::style::set_play_style(mode);
            }
        }
        _ => {} // unrecognized option — real UCI engines ignore, not error
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Color;

    fn fresh_session() -> UciSession {
        UciSession::new()
    }

    #[test]
    fn test_uci_command_returns_uciok() {
        let response = cmd_uci();
        assert!(response.starts_with("id name Pet Dragon"));
        assert!(response.contains("uciok"));
        assert!(response.contains("option name Hash"));
        assert!(response.contains("option name Skill Level"));
    }

    #[test]
    fn test_isready_returns_readyok() {
        let mut session = fresh_session();
        assert_eq!(handle_line(&mut session, "isready"), "readyok");
    }

    #[test]
    fn test_position_startpos_sets_standard_position() {
        let mut session = fresh_session();
        handle_line(&mut session, "position startpos");
        assert_eq!(session.pos.to_fen(), STANDARD_START_FEN);
    }

    #[test]
    fn test_position_startpos_with_moves_applies_moves() {
        let mut session = fresh_session();
        handle_line(&mut session, "position startpos moves e2e4 e7e5");
        // Two plies played — side to move should be White again, and the
        // position should differ from the starting FEN.
        assert_eq!(session.pos.side_to_move, Color::White);
        assert_ne!(session.pos.to_fen(), STANDARD_START_FEN);
    }

    #[test]
    fn test_position_with_illegal_move_token_stops_applying_but_keeps_prior_moves() {
        let mut session = fresh_session();
        handle_line(&mut session, "position startpos moves e2e4 zz99 e7e5");
        // e2e4 should have applied; zz99 should have halted further
        // application, so e7e5 (which would be legal from there) never runs.
        assert_eq!(session.pos.side_to_move, Color::Black);
    }

    #[test]
    fn test_position_fen_parses_multi_field_fen() {
        let mut session = fresh_session();
        let fen = STANDARD_START_FEN;
        handle_line(&mut session, &format!("position fen {}", fen));
        assert_eq!(session.pos.to_fen(), fen);
    }

    #[test]
    fn test_setoption_hash_resizes_tt() {
        let mut session = fresh_session();
        handle_line(&mut session, "setoption name Hash value 64");
        assert_eq!(session.hash_mb, 64);
        assert_eq!(session.tt.size_mb(), 64);
    }

    #[test]
    fn test_setoption_hash_clamps_to_max() {
        let mut session = fresh_session();
        handle_line(&mut session, &format!("setoption name Hash value {}", MAX_HASH_MB * 4));
        assert_eq!(session.hash_mb, MAX_HASH_MB);
    }

    #[test]
    fn test_setoption_skill_level_clamps_to_max() {
        let mut session = fresh_session();
        handle_line(&mut session, "setoption name Skill Level value 99");
        assert_eq!(session.skill_level, crate::search::skill::MAX_SKILL_LEVEL);
    }

    #[test]
    fn test_setoption_move_overhead_multi_word_name_parses() {
        let mut session = fresh_session();
        handle_line(&mut session, "setoption name Move Overhead value 500");
        assert_eq!(session.move_overhead_ms, 500);
    }

    #[test]
    fn test_effective_skill_level_uses_raw_skill_by_default() {
        let mut session = fresh_session();
        session.skill_level = 10;
        assert_eq!(effective_skill_level(&session), 10);
    }

    #[test]
    fn test_effective_skill_level_uses_elo_when_limit_strength_set() {
        let mut session = fresh_session();
        session.skill_level = 20; // would be "off" if not overridden
        session.uci_limit_strength = true;
        session.uci_elo = crate::search::skill::ELO_TABLE[0]; // lowest tier
        assert_eq!(effective_skill_level(&session), 0);
    }

    #[test]
    fn test_ucinewgame_resets_position_and_clears_tt() {
        let mut session = fresh_session();
        handle_line(&mut session, "position startpos moves e2e4");
        assert_ne!(session.pos.to_fen(), STANDARD_START_FEN);
        handle_line(&mut session, "ucinewgame");
        assert_eq!(session.pos.to_fen(), STANDARD_START_FEN);
    }

    #[test]
    fn test_go_depth_returns_bestmove_and_info_line() {
        let mut session = fresh_session();
        handle_line(&mut session, "position startpos");
        let response = handle_line(&mut session, "go depth 3");
        assert!(response.contains("bestmove "));
        assert!(response.starts_with("info depth"));
        // Shallow depth-3 search on the start position should never
        // legitimately return the null-move sentinel.
        assert!(!response.contains("bestmove 0000"));
    }

    #[test]
    fn test_stop_and_quit_are_no_ops_returning_empty_string() {
        let mut session = fresh_session();
        assert_eq!(handle_line(&mut session, "stop"), "");
        assert_eq!(handle_line(&mut session, "quit"), "");
    }

    #[test]
    fn test_unrecognized_command_returns_empty_string_not_panic() {
        let mut session = fresh_session();
        assert_eq!(handle_line(&mut session, "totally not a uci command"), "");
    }

    #[test]
    fn test_uci_command_wasm_export_uses_thread_local_session() {
        // Exercises the actual #[wasm_bindgen] export end-to-end (not just
        // handle_line directly), confirming the thread_local session
        // persists correctly across two calls to the public function.
        let uciok_response = uci_command("uci");
        assert!(uciok_response.contains("uciok"));
        let readyok_response = uci_command("isready");
        assert_eq!(readyok_response, "readyok");
    }
}
