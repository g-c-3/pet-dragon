// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// lib.rs — Library root + WASM exports
//
// Native builds use this as a library crate (rlib).
// WASM builds compile this as a cdylib and expose functions via wasm-bindgen.
//
// WASM exports (Phase 11):
//   engine_name()        → "Pet Dragon"
//   engine_author()      → "Gokul Chandar"
//   engine_version()     → crate version string
//   new_game()           → generate a random Pet Dragon starting position,
//                          return FEN string
//   search_from_fen(fen, movetime_ms, skill_level) → run search, return UCI
//                          bestmove string. skill_level 0..=20, 20 = full
//                          strength (Phase 20 / D39 — see the function's own
//                          doc comment for the full explanation).
//
// Startup:
//   wasm_main() runs automatically on WASM module load (wasm_bindgen start).
//   It calls init_masks() → init_magic() → init_zobrist() exactly once.
//   Native builds call these from main() instead.
// ============================================================================

// wasm-bindgen only available when the "wasm" feature is enabled
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// ── Module declarations ───────────────────────────────────────────────────────

pub mod types;
pub mod bitboard;
pub mod position;
pub mod movegen;
pub mod tt;
pub mod search;
pub mod eval;
pub mod texel;
pub mod nnue;

// Syzygy endgame tablebases — native only (pyrrhic-rs needs libc, not wasm32-safe)
#[cfg(not(target_arch = "wasm32"))]
pub mod syzygy;

// ── WASM entry point ──────────────────────────────────────────────────────────

/// Called automatically when the WASM module loads in the browser.
/// Runs the mandatory engine startup sequence once.
#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    // Propagate Rust panics to browser console for debugging
    #[cfg(feature = "wasm")]
    console_error_panic_hook_setup();

    // Mandatory startup — identical to main() on native
    bitboard::masks::init_masks();
    bitboard::magic::init_magic();
    position::zobrist::init_zobrist();
}

/// Set up panic hook so Rust panics print a real message + stack trace to
/// the browser console instead of an unreported WASM trap. This was the
/// reason the original Instant::now() bug hung the UI with zero visible
/// error (Session 25) — any future wasm-side panic will now be diagnosable.
#[cfg(feature = "wasm")]
fn console_error_panic_hook_setup() {
    console_error_panic_hook::set_once();
}

// ── WASM engine identity exports ──────────────────────────────────────────────

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn engine_name() -> String {
    String::from("Pet Dragon")
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn engine_author() -> String {
    String::from("Gokul Chandar")
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn engine_version() -> String {
    String::from(env!("CARGO_PKG_VERSION"))
}

// ── WASM game exports ─────────────────────────────────────────────────────────

/// Generate a new random Pet Dragon starting position.
/// Returns the position as a FEN string (with Pet Dragon pawn-start extension).
/// Called by the browser UI when starting a new game.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn new_game() -> String {
    let pos = position::Position::generate_pet_dragon();
    pos.to_fen()
}

/// Run the engine search from a given FEN position for up to `movetime_ms`
/// milliseconds, at the given Skill Level. Returns the bestmove in UCI
/// format (e.g. "e2e4").
///
/// # Arguments
/// * `fen`         - FEN string of the position to search (Pet Dragon or standard)
/// * `movetime_ms` - Maximum milliseconds to think
/// * `skill_level` - 0..=20 (Phase 20 / D39). 20 (`skill::MAX_SKILL_LEVEL`)
///                   means full strength — no depth cap, no time reduction,
///                   byte-identical to how this function behaved before
///                   Skill Level existed. Values above 20 are clamped down
///                   to 20 rather than treated as an error, since a stray
///                   out-of-range value from a GUI's own slider/state
///                   shouldn't fail a search — it should just mean "as
///                   strong as possible," the same safe fallback the native
///                   UCI `setoption name Skill Level` handler uses.
///                   Unlike the native UCI path (where Skill Level is a
///                   persistent `setoption` that carries across searches
///                   until changed), this is a plain function parameter
///                   like `fen`/`movetime_ms` — pass whatever the GUI's own
///                   difficulty control is currently set to on every call.
///                   There's no hidden engine-side state to remember to
///                   configure first, matching how every other parameter
///                   in this stateless WASM API already works.
///
/// Returns "0000" if the position is illegal or has no legal moves.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn search_from_fen(fen: &str, movetime_ms: u32, skill_level: u8) -> String {
    use search::iterative::iterative_deepening;
    use search::time::TimeControl;
    use search::SearchInfo;
    use tt::TranspositionTable;
    use types::Move;

    // Parse position
    let mut pos = match position::Position::from_fen(fen) {
        Ok(p)  => p,
        Err(_) => return String::from("0000"),
    };

    // Record starting position in game history
    pos.push_game_history();

    // Clamp defensively rather than error — see doc comment above.
    let skill_level = skill_level.min(search::skill::MAX_SKILL_LEVEL);

    // Set up search with movetime, scaled by the Skill Level's time
    // fraction (same mechanism as the native UCI path's cmd_go — see
    // search/skill.rs's skill_time_fraction_pct()).
    let tc = TimeControl {
        movetime: movetime_ms as u64,
        skill_time_fraction_pct: search::skill::skill_time_fraction_pct(skill_level),
        ..TimeControl::default()
    };

    let mut info = SearchInfo::new();
    info.skill_level = skill_level;
    let mut tt   = TranspositionTable::new(32); // 32MB TT for browser

    // Run search
    let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

    if result.best_move == Move::NULL {
        String::from("0000")
    } else {
        result.best_move.to_uci()
    }
}

/// Same search as `search_from_fen`, but also returns the evaluation —
/// added so the browser can show a real eval bar instead of the material
/// + mobility heuristic it previously had to fall back to (that heuristic
/// never had access to an actual search score, since this function's
/// sibling above only ever returned the bare move).
///
/// Returns a two-token, space-separated string: `"<uci_move> <eval>"`.
/// `<eval>` is one of:
/// - a plain signed integer, in centipawns, **from White's perspective**
///   (positive = White is better) — deliberately NOT the UCI/negamax
///   convention of "from side-to-move's perspective," since a GUI eval
///   bar needs a stable reference frame that doesn't flip depending on
///   whose turn it is.
/// - `"mateN"` / `"mate-N"` for a forced mate, same White-relative sign:
///   positive = White delivers mate in N, negative = Black does.
///
/// Returns `"0000 0"` if the position is illegal or has no legal moves —
/// callers should check for the `"0000"` move token first, same as
/// `search_from_fen`, before reading the eval token.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn search_from_fen_with_eval(fen: &str, movetime_ms: u32, skill_level: u8) -> String {
    use search::iterative::iterative_deepening;
    use search::time::TimeControl;
    use search::SearchInfo;
    use tt::TranspositionTable;
    use types::{Color, Move};

    // Parse position
    let mut pos = match position::Position::from_fen(fen) {
        Ok(p)  => p,
        Err(_) => return String::from("0000 0"),
    };

    // Captured BEFORE the search mutates `pos` via make/unmake, so this is
    // reliably the side that was actually asked to move here, regardless
    // of internal search bookkeeping.
    let root_side = pos.side_to_move;

    // Record starting position in game history
    pos.push_game_history();

    // Clamp defensively rather than error — see search_from_fen's doc
    // comment above for the full reasoning (same applies here verbatim).
    let skill_level = skill_level.min(search::skill::MAX_SKILL_LEVEL);

    let tc = TimeControl {
        movetime: movetime_ms as u64,
        skill_time_fraction_pct: search::skill::skill_time_fraction_pct(skill_level),
        ..TimeControl::default()
    };

    let mut info = SearchInfo::new();
    info.skill_level = skill_level;
    let mut tt   = TranspositionTable::new(32); // 32MB TT for browser

    // Run search
    let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

    if result.best_move == Move::NULL {
        return String::from("0000 0");
    }

    let eval_token = if result.is_mate {
        let mate_from_white = if root_side == Color::White {
            result.mate_in
        } else {
            -result.mate_in
        };
        format!("mate{}", mate_from_white)
    } else {
        let score_from_white = if root_side == Color::White {
            result.score
        } else {
            -result.score
        };
        score_from_white.to_string()
    };

    format!("{} {}", result.best_move.to_uci(), eval_token)
}

/// Return all legal moves from a FEN position as a space-separated UCI string.
/// Used by the browser UI to highlight legal destinations for a picked piece.
///
/// Returns empty string if position is invalid.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn legal_moves_from_fen(fen: &str) -> String {
    use movegen::generate_moves;

    let pos = match position::Position::from_fen(fen) {
        Ok(p)  => p,
        Err(_) => return String::new(),
    };

    let moves = generate_moves(&pos);
    let uci_strings: Vec<String> = moves.iter()
        .map(|mv| mv.to_uci())
        .collect();

    uci_strings.join(" ")
}
