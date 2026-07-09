// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/mod.rs — Texel tuning module root (Phase 14.3, D35)
//
// This module implements a parallel "tunable" evaluation path used only by
// the Texel tuner (src/bin/texel_tune.rs, not yet built — see ROADMAP 14.3).
// It never runs inside the real search; `crate::eval::evaluate()` remains
// the actual engine evaluation, completely untouched by anything here.
//
// Design (D35): HCE is linear-in-weights (~970 tunable parameters across the
// 6 eval submodules, one clamp nonlinearity in king_safety). `features`
// extracts a per-position feature summary once; `weights` holds the same
// shape as a mutable f64-tunable... (values start as exact copies of the
// current compile-time consts); `predict` recomputes the eval from
// (features, weights) via the same arithmetic `crate::eval` uses internally
// (packed s(mg,eg) tapering, same clamp/bucket logic), so that at the
// default weights it is bit-exact against `crate::eval::evaluate()` — this
// is the self-consistency test's job to prove, not an assumption.
// ============================================================================

pub mod features;
pub mod weights;
pub mod predict;
