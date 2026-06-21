// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// lib.rs — Library root
//
// This file is the entry point for the WebAssembly (browser) build.
// It will expose engine functions to JavaScript in Phase 11.
//
// For now it declares all engine modules so the project compiles cleanly.
// ============================================================================

// WASM bindings — only compiled when targeting the browser
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// ── Module declarations ──────────────────────────────────────────────────────
// Each module lives in its own file/folder under src/
// We declare them here so Rust knows they exist.
// They will be filled in phase by phase.

pub mod types;        // Core data types — Square, Piece, Move, etc.

// These modules will be added in later phases:
// pub mod bitboard;  // Phase 2
// pub mod position;  // Phase 3
// pub mod movegen;   // Phase 4
// pub mod search;    // Phase 7
// pub mod eval;      // Phase 8
// pub mod tt;        // Phase 6
// pub mod uci;       // Phase 9

// ── WASM entry point ─────────────────────────────────────────────────────────
// This function runs automatically when the WASM module loads in the browser.
// Right now it just confirms the engine loaded successfully.
// Full WASM bindings are added in Phase 11.

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    // In Phase 11 we will add:
    // - console_error_panic_hook for better browser error messages
    // - Engine initialisation (precompute attack tables, etc.)
    // For now this is intentionally empty — it just needs to compile.
}

// ── Version info exposed to JavaScript ───────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn engine_name() -> String {
    String::from("Pet Dragon")
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn engine_author() -> String {
    String::from("Gokul Chandar")
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn engine_version() -> String {
    String::from(env!("CARGO_PKG_VERSION"))
}
