// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/skill.rs — Skill Level tier table (Phase 20 / D39)
//
// Difficulty is depth-cap tiers, NOT Elo calibration — see DECISIONS.md D39
// for the full reasoning (Pet Dragon's custom pawn rules apply from move
// one, so no external Elo table built from real chess games transfers,
// even for the one opening that visually resembles a standard start).
//
// Session 65 refinement: depth alone left a real gap — a low tier would
// still burn whatever time the GUI/clock gave it just to search shallower,
// which either instaflies (looks broken, not weak) or wastes think time
// that move-selection noise could otherwise use. So each tier sets BOTH:
//   - a depth cap (the actual strength ceiling — how far it can see)
//   - a time-budget fraction (how long it visibly "tries," matching the
//     depth cap so a low tier both sees less and tries less)
//
// 21 levels (0..=20), mirroring the familiar `Skill Level` spin range GUIs
// already expect the shape of — this borrows only the OPTION SHAPE, not
// any calibration data or Elo claim (explicitly rejected in D39).
//
// Level 20 (MAX_SKILL_LEVEL, the default) is the "off" position: no depth
// cap at all (skill_depth_cap returns None) and a 100% time fraction —
// byte-identical to how the engine behaved before this feature existed,
// same backward-compatibility pattern as MultiPV=1 and the Move Overhead
// default (D38).
//
// Tier ordering/spacing is validated empirically with `uci_match_runner.rs`
// across multiple seeds (D36's methodology) — see ROADMAP Phase 20 for
// status; the table below is deliberately simple and monotonic so that
// validation has a clean, easy-to-reason-about baseline to confirm or
// adjust from, rather than a complex formula tuned before any match data
// exists.
// ============================================================================

/// Highest selectable Skill Level. This value means "full strength" —
/// no depth cap, no time reduction. Matches the UCI option's declared max.
pub const MAX_SKILL_LEVEL: u8 = 20;

/// Depth cap for a given Skill Level.
///
/// `None` means no cap at all (Skill Level 20, the default) — the search
/// depth is governed only by whatever `iterative_deepening()` would already
/// use (a fixed `go depth`, or `MAX_DEPTH`). For levels 0..19, the cap is
/// `level + 1`: Skill Level 0 → depth 1 (weakest), Skill Level 19 → depth 20.
///
/// Combined with the caller's own depth limit via `.min()` — a tier can
/// only ever make the search shallower than what was already requested,
/// never deeper (so `go depth 3` at Skill Level 20 still only searches to
/// depth 3; a tier cap never overrides an explicit, shallower user request).
pub fn skill_depth_cap(level: u8) -> Option<i32> {
    if level >= MAX_SKILL_LEVEL {
        None
    } else {
        Some(level as i32 + 1)
    }
}

/// Time-budget fraction for a given Skill Level, as a percentage (0..=100).
///
/// Applied to the soft/hard time allocation in `allocate_time()` for the
/// movetime and clock-based branches only — NOT to `infinite`/`ponder`
/// (analysis-style searches, where skill doesn't apply) and NOT to the
/// fixed-depth/fixed-nodes sentinel branches (those are already governed by
/// the depth cap above, and scaling a near-infinite sentinel is meaningless).
///
/// 100 at Skill Level 20 (the default) means zero change to time allocation
/// — same backward-compatibility pattern as the depth cap.
pub fn skill_time_fraction_pct(level: u8) -> u32 {
    if level >= MAX_SKILL_LEVEL {
        100
    } else {
        // 10% at level 0, +5% per level, capped at 98% (kept strictly below
        // 100% for every capped level so "capped" and "uncapped" are always
        // distinguishable — level 19 -> 98%, not 100%, even though the
        // formula alone would reach 105%).
        (10 + (level as u32) * 5).min(98)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_level_has_no_depth_cap() {
        assert_eq!(skill_depth_cap(MAX_SKILL_LEVEL), None,
            "Skill Level 20 (default) should mean no depth cap at all");
    }

    #[test]
    fn test_max_level_has_full_time_fraction() {
        assert_eq!(skill_time_fraction_pct(MAX_SKILL_LEVEL), 100,
            "Skill Level 20 (default) should mean 100% time — no reduction");
    }

    #[test]
    fn test_min_level_depth_cap_is_one() {
        assert_eq!(skill_depth_cap(0), Some(1),
            "Skill Level 0 (weakest) should cap depth at 1");
    }

    #[test]
    fn test_depth_cap_increases_monotonically() {
        let mut prev = skill_depth_cap(0).unwrap();
        for level in 1..MAX_SKILL_LEVEL {
            let cap = skill_depth_cap(level).unwrap();
            assert!(cap >= prev,
                "depth cap must never decrease as Skill Level rises \
                 (level {level}: {cap} < prev {prev})");
            prev = cap;
        }
    }

    #[test]
    fn test_time_fraction_increases_monotonically_and_stays_capped() {
        let mut prev = skill_time_fraction_pct(0);
        for level in 1..MAX_SKILL_LEVEL {
            let pct = skill_time_fraction_pct(level);
            assert!(pct >= prev,
                "time fraction must never decrease as Skill Level rises \
                 (level {level}: {pct} < prev {prev})");
            assert!(pct <= 98,
                "every capped level must stay strictly below 100% so it's \
                 distinguishable from the uncapped default");
            prev = pct;
        }
    }

    #[test]
    fn test_levels_above_max_treated_as_uncapped() {
        // Defensive: a value above the declared UCI max (e.g. from a
        // misbehaving GUI that ignores the declared spin bounds) should
        // still behave as "uncapped," not panic or wrap.
        assert_eq!(skill_depth_cap(255), None);
        assert_eq!(skill_time_fraction_pct(255), 100);
    }
}
