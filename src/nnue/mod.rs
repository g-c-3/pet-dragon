// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// nnue/mod.rs — NORU-based NNUE evaluation (Phase 16)
//
// Feature set (D10): 896 inputs per perspective
//   - 768 standard piece-square features (6 kinds x 2 relative colors x 64 sq)
//   - 128 Pet Dragon pawn-start features (2 relative colors x 64 sq) — active
//     only while a pawn still occupies its actual starting square (D11)
//
// This module currently defines the feature encoding only (Phase 16.2).
// Incremental accumulator updates (16.3), training data generation (16.4),
// training (16.5), and evaluate() integration (16.6) land in later sessions.
//
// Dependency: `noru` (crates.io, MIT/Apache-2.0, zero-dep, WASM-safe) —
// added to Cargo.toml in this session (Phase 16.1).
// ============================================================================

pub mod features;
