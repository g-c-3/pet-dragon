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
//   search_from_fen(fen, movetime_ms) → run search, return UCI bestmove string
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

/// Set up panic hook only when the optional dep is available.
/// Wrapped so we can easily add console_error_panic_hook later.
#[cfg(feature = "wasm")]
fn console_error_panic_hook_setup() {
    // Phase 12+: add console_error_panic_hook crate here for better debugging
    // For now: default panic behaviour (WASM trap)
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
/// milliseconds. Returns the bestmove in UCI format (e.g. "e2e4").
///
/// # Arguments
/// * `fen`         - FEN string of the position to search (Pet Dragon or standard)
/// * `movetime_ms` - Maximum milliseconds to think
///
/// Returns "0000" if the position is illegal or has no legal moves.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn search_from_fen(fen: &str, movetime_ms: u32) -> String {
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

    // Set up search with movetime
    let tc = TimeControl {
        movetime: movetime_ms as u64,
        ..TimeControl::default()
    };

    let mut info = SearchInfo::new();
    let mut tt   = TranspositionTable::new(32); // 32MB TT for browser

    // Run search
    let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

    if result.best_move == Move::NULL {
        String::from("0000")
    } else {
        result.best_move.to_uci()
    }
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
