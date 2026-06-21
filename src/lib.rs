// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// lib.rs — Library root
// ============================================================================

// wasm-bindgen is only available when the "wasm" feature is enabled
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// ── Module declarations ───────────────────────────────────────────────────────

pub mod types;
pub mod bitboard;

// Future modules (uncommented phase by phase):
// pub mod bitboard;  // Phase 2
// pub mod position;  // Phase 3
// pub mod movegen;   // Phase 4
// pub mod tt;        // Phase 6
// pub mod search;    // Phase 7
// pub mod eval;      // Phase 8
// pub mod uci;       // Phase 9

// ── WASM entry point ──────────────────────────────────────────────────────────
// Only compiled when building for browser (wasm feature enabled)

#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    // Phase 11: add console_error_panic_hook and engine init here
}

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
