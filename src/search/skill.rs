// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/skill.rs — Skill Level tier table (Phase 20 / D39; Elo mapping D43)
//
// Difficulty tiers themselves are depth-cap based, NOT calibrated against
// real games — see DECISIONS.md D39 for that original reasoning (Pet
// Dragon's custom pawn rules apply from move one, so no external Elo
// table built from real chess games transfers, even for the one opening
// that visually resembles a standard start). D39's rejection of UCI_Elo
// specifically was later overridden — see D43 — once Gokul explicitly
// chose to proceed with self-assumed Elo anchors rather than an external
// rating pool. ELO_TABLE / elo_to_skill_level() below implement that;
// read D43 before trusting these numbers as anything more than what
// they are: two hand-picked anchor points (1200, 2600) plus this
// project's own real measured relative tier gaps, rescaled to fit.
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

// ── Move-selection noise (Phase 20 follow-up, Session 66 validation) ──────────
//
// Depth-cap alone strongly separates the low tiers but plateaus at the high
// end — once a cap exceeds roughly the depth this engine already converges
// at for a given time budget, extra plies stop changing the chosen move.
// Match-runner validation confirmed this: 10-vs-15 (depth caps 11 vs 16)
// measured at -8.7 Elo over 40 games, essentially a statistical tie, while
// 0-vs-5 (depth caps 1 vs 6) measured at -381.7 Elo — the difference isn't
// noise, it's that depth simply stops mattering much once the search is
// already reasonably deep. This mirrors a well-known property of essentially
// all engines' Skill Level implementations, including Stockfish's own
// (its own high tiers are notoriously close together too) — not a Pet
// Dragon-specific bug.
//
// The fix is independent of depth: instead of always taking the single best
// root move, a capped tier has some chance of taking a nearby-scored
// alternative instead. This is the same general mechanism Stockfish's own
// Skill Level uses (weighted randomness among top candidates) — but the
// window/selection formula below is our own, designed from scratch for this
// project, not ported from theirs (D39 applies here too: no calibration
// data or internal formula was borrowed, only the general concept of
// "sometimes pick a near-best move instead of the best one").
//
// SESSION 67 FOLLOW-UP FINDING: an earlier version of this mechanism gated
// eligibility ONLY on a fixed centipawn window (how close a candidate's
// score is to the best move's). That measured backwards in match-runner
// validation — Skill Level 5 beat Skill Level 10 in 3 separate runs
// totaling 150 games (57% cumulative), the opposite of the intended
// ordering. Root cause: root-move score gaps naturally SHRINK as search
// gets deeper (deeper search converges on more genuinely-close
// alternatives), so a nominally "tighter" cp window at a higher tier's
// greater depth can end up catching MORE eligible candidates than a
// "wider" window at a lower tier's shallower depth — inverting how often
// each tier actually deviates from its best move, independent of the
// window sizes' own ordering. Fix: `skill_noise_trigger_pct()` below adds
// a separate probability gate, checked BEFORE the cp window, that controls
// deviation FREQUENCY directly from Skill Level rather than leaving it as
// an incidental side effect of how clustered a given position's candidates
// happen to be at that depth. The cp window still applies after a
// triggered roll, purely as a safety bound (never pick something wildly
// worse than best), but no longer controls how OFTEN deviation happens.

/// Centipawn window for move-selection noise at a given Skill Level: the
/// maximum score gap from the best root move within which an alternative
/// candidate remains eligible to be chosen instead. `0` at
/// `MAX_SKILL_LEVEL` (the default) — no noise at all, deterministic best
/// move, byte-identical to pre-noise behavior. Widens linearly as level
/// drops: `(MAX_SKILL_LEVEL - level) * 8`, so level 19 -> 8cp (barely any
/// noise, matching how close 19 already is to full strength) and level 0
/// -> 160cp (frequently willing to play a clearly-worse move, matching how
/// weak that tier already is from its depth-1 cap).
pub fn skill_noise_window_cp(level: u8) -> i32 {
    if level >= MAX_SKILL_LEVEL {
        0
    } else {
        ((MAX_SKILL_LEVEL - level) as i32) * 8
    }
}

/// Probability (0..=100) that a given move even ATTEMPTS a noisy pick at
/// all, at a given Skill Level — checked BEFORE `skill_noise_window_cp()`
/// is consulted. `0` at `MAX_SKILL_LEVEL` (no noise, as always). Widens
/// linearly as level drops: `(MAX_SKILL_LEVEL - level) * 4`, so level 0 ->
/// 80% of moves attempt a deviation, level 19 -> 4%.
///
/// This exists specifically to keep deviation FREQUENCY under direct
/// Skill Level control, independent of `skill_noise_window_cp()` — see the
/// module-level comment above for why leaving frequency purely to the cp
/// window backfired (root-move score gaps shrink at greater search depth,
/// so a "tighter" window at a higher tier's greater depth could trigger
/// MORE often than a "wider" window at a lower tier's shallower depth).
pub fn skill_noise_trigger_pct(level: u8) -> u32 {
    if level >= MAX_SKILL_LEVEL {
        0
    } else {
        (MAX_SKILL_LEVEL - level) as u32 * 4
    }
}

/// Minimal xorshift64 PRNG. Deliberately NOT a new crate dependency (no
/// `rand` crate anywhere in this project) — this only needs "not always the
/// same move," not cryptographic unpredictability, so a few lines here beat
/// pulling in a dependency for it.
struct NoiseRng(u64);

impl NoiseRng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Given root candidates as `(move, score)` pairs sorted best-first, pick
/// one. Two-stage process (Session 67 follow-up — see module comment):
/// first roll against `skill_noise_trigger_pct(level)` to decide whether
/// this move deviates from the best move AT ALL; only if that fires does
/// `skill_noise_window_cp(level)` get consulted to pick uniformly at
/// random among whichever candidates fall within that score window of the
/// best. Always returns index `0` (the best move) when there's only one
/// candidate, the level is uncapped, the trigger roll misses, or the
/// window excludes everything but the best move itself.
///
/// `seed` should vary from call to call (e.g. derived from node count /
/// search state) so the same position doesn't always noise the same way —
/// see the call site in `iterative_deepening()` for how that's built
/// without wall-clock time or a new dependency.
///
/// Returns an INDEX into `candidates`, not a move, so the caller decides
/// what to do with a non-zero result (swap in the move, adjust the
/// reported score/PV, etc.) rather than this function reaching into
/// `SearchResult` itself.
pub fn pick_noisy_move_index(candidates: &[(crate::types::Move, i32)], level: u8, seed: u64) -> usize {
    if candidates.len() <= 1 {
        return 0;
    }
    let trigger_pct = skill_noise_trigger_pct(level);
    if trigger_pct == 0 {
        return 0;
    }
    let mut rng = NoiseRng(seed | 1); // xorshift needs a non-zero state
    if (rng.next_u64() % 100) as u32 >= trigger_pct {
        return 0; // this move didn't roll into deviating at all
    }
    let window = skill_noise_window_cp(level);
    if window <= 0 {
        return 0;
    }
    let best_score = candidates[0].1;
    let eligible: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, (_, score))| best_score - score <= window)
        .map(|(i, _)| i)
        .collect();
    if eligible.len() <= 1 {
        return 0;
    }
    let pick = (rng.next_u64() as usize) % eligible.len();
    eligible[pick]
}

// ── Elo mapping (D43 — overrides D39's UCI_Elo rejection) ─────────────────────
//
// NOT a calibrated rating in any external sense. Built from exactly two
// inputs: (1) Gokul's explicitly chosen anchor points, Skill 0 = 1200 and
// Skill 20 = 2600; (2) this project's own real measured relative tier
// gaps from Session 68's 200-games/pair `uci_match_runner` validation —
// 0v5 -619.4 Elo, 5v10 -117.2, 10v15 -65.0, 15v20 -81.35 (avg of two
// consistent runs) — rescaled by a single constant factor so the four
// gaps sum to exactly 1400 (2600 - 1200) instead of their original
// unscaled sum. Levels 1-4, 6-9, 11-14, 16-19 were NEVER individually
// match-tested — they're linear interpolation within each rescaled
// band, not measurements. The four scaled/anchored levels (0, 5, 10, 15,
// 20) are the only entries with any real game data behind their relative
// spacing; even those are relative gaps fitted to a chosen absolute
// range, not an externally-calibrated rating.
pub const ELO_TABLE: [i32; (MAX_SKILL_LEVEL as usize) + 1] = [
    1200, 1396, 1593, 1789, 1986, // 0-4
    2182, 2219, 2257, 2294, 2331, // 5-9
    2368, 2389, 2409, 2430, 2451, // 10-14
    2471, 2497, 2523, 2549, 2574, // 15-19
    2600,                         // 20
];

/// Nearest-match a target Elo onto the closest `ELO_TABLE` entry's Skill
/// Level. Input is clamped to `ELO_TABLE`'s own min/max first — the table
/// is monotonically increasing, so nearest-match naturally saturates at
/// the endpoints anyway, but clamping first keeps the search loop simple
/// and avoids any edge-case ambiguity from an out-of-range input.
///
/// Ties (equidistant between two adjacent table entries) resolve to the
/// LOWER Skill Level — deliberately conservative, since `UCI_LimitStrength`
/// exists to make the engine weaker on request, and rounding up on a tie
/// would silently give a slightly stronger engine than what was asked for.
pub fn elo_to_skill_level(target_elo: i32) -> u8 {
    let clamped = target_elo.clamp(ELO_TABLE[0], ELO_TABLE[MAX_SKILL_LEVEL as usize]);
    let mut best_level = 0u8;
    let mut best_dist   = i32::MAX;
    for (level, &elo) in ELO_TABLE.iter().enumerate() {
        let dist = (elo - clamped).abs();
        if dist < best_dist {
            best_dist = dist;
            best_level = level as u8;
        }
    }
    best_level
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
    fn test_elo_table_endpoints_match_chosen_anchors() {
        assert_eq!(ELO_TABLE[0], 1200);
        assert_eq!(ELO_TABLE[MAX_SKILL_LEVEL as usize], 2600);
    }

    #[test]
    fn test_elo_table_is_strictly_monotonic() {
        for w in ELO_TABLE.windows(2) {
            assert!(w[1] > w[0],
                "ELO_TABLE must be strictly increasing so nearest-match is unambiguous: {:?}", w);
        }
    }

    #[test]
    fn test_elo_to_skill_level_exact_anchor_hits() {
        // Every table entry should map back to its own exact level.
        for (level, &elo) in ELO_TABLE.iter().enumerate() {
            assert_eq!(elo_to_skill_level(elo), level as u8,
                "Elo {} should map exactly back to Skill Level {}", elo, level);
        }
    }

    #[test]
    fn test_elo_to_skill_level_clamps_below_range() {
        assert_eq!(elo_to_skill_level(0), 0,
            "Below-range Elo should clamp to the weakest tier, not panic or underflow");
        assert_eq!(elo_to_skill_level(-500), 0);
    }

    #[test]
    fn test_elo_to_skill_level_clamps_above_range() {
        assert_eq!(elo_to_skill_level(9999), MAX_SKILL_LEVEL,
            "Above-range Elo should clamp to the strongest tier");
    }

    #[test]
    fn test_elo_to_skill_level_ties_resolve_to_lower_level() {
        // Midpoint between ELO_TABLE[0]=1200 and ELO_TABLE[1]=1396 is 1298.
        // Distance to each is identical (98), so this must resolve to the
        // lower level (0), per elo_to_skill_level's documented tie rule.
        let midpoint = (ELO_TABLE[0] + ELO_TABLE[1]) / 2;
        assert_eq!(elo_to_skill_level(midpoint), 0,
            "Exact ties must resolve to the lower (weaker) Skill Level, never the higher one");
    }

    #[test]
    fn test_elo_to_skill_level_nearest_match_not_exact() {
        // An Elo strictly between two table entries, closer to the upper
        // one, should map to the upper level.
        let near_upper = ELO_TABLE[1] - 5; // 5 below Skill 1's exact value
        assert_eq!(elo_to_skill_level(near_upper), 1);
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

    #[test]
    fn test_max_level_has_no_noise_window() {
        assert_eq!(skill_noise_window_cp(MAX_SKILL_LEVEL), 0,
            "Skill Level 20 (default) should mean zero noise — \
             deterministic best-move selection, same as before this \
             feature existed");
    }

    #[test]
    fn test_min_level_has_widest_noise_window() {
        assert_eq!(skill_noise_window_cp(0), 160,
            "Skill Level 0 (weakest) should have the widest noise window");
    }

    #[test]
    fn test_noise_window_increases_as_level_drops() {
        let mut prev = skill_noise_window_cp(0);
        for level in 1..MAX_SKILL_LEVEL {
            let w = skill_noise_window_cp(level);
            assert!(w <= prev,
                "noise window must never widen as Skill Level rises \
                 (level {level}: {w} > prev {prev})");
            prev = w;
        }
    }

    #[test]
    fn test_max_level_has_no_trigger_chance() {
        assert_eq!(skill_noise_trigger_pct(MAX_SKILL_LEVEL), 0,
            "Skill Level 20 (default) should never even roll for a \
             deviation — deterministic best-move selection");
    }

    #[test]
    fn test_min_level_has_highest_trigger_chance() {
        assert_eq!(skill_noise_trigger_pct(0), 80,
            "Skill Level 0 (weakest) should deviate from its best move \
             most often");
    }

    #[test]
    fn test_trigger_pct_increases_as_level_drops() {
        let mut prev = skill_noise_trigger_pct(0);
        for level in 1..MAX_SKILL_LEVEL {
            let p = skill_noise_trigger_pct(level);
            assert!(p <= prev,
                "trigger chance must never rise as Skill Level rises \
                 (level {level}: {p} > prev {prev})");
            prev = p;
        }
    }

    #[test]
    fn test_trigger_pct_is_independent_of_depth() {
        // Session 67 follow-up regression guard: this is the specific
        // property that was missing before the fix. skill_noise_trigger_
        // pct() must be a pure function of `level` alone — nothing here
        // should vary based on how close together a position's candidate
        // scores happen to be at that level's search depth. Asserting the
        // function signature only takes `level` (no score/depth
        // parameter) is enforced by the compiler; this test just pins the
        // two concrete values a regression would most likely disturb.
        assert_eq!(skill_noise_trigger_pct(5), 60);
        assert_eq!(skill_noise_trigger_pct(10), 40,
            "level 10 must trigger LESS often than level 5 — this is the \
             exact ordering that was inverted (in effective frequency, via \
             the cp-window-only mechanism) before this fix");
    }

    #[test]
    fn test_pick_noisy_move_index_no_op_at_max_level() {
        let candidates = vec![
            (crate::types::Move::NULL, 100),
            (crate::types::Move::NULL, 90),
        ];
        // Same move value in both slots is fine here — this test only
        // checks the returned INDEX, which the score-based window logic
        // decides independently of what the move itself is.
        for seed in [1u64, 2, 3, 4, 5] {
            assert_eq!(
                pick_noisy_move_index(&candidates, MAX_SKILL_LEVEL, seed), 0,
                "uncapped Skill Level must always return the best move's \
                 index (0), regardless of seed"
            );
        }
    }

    #[test]
    fn test_pick_noisy_move_index_single_candidate_is_always_zero() {
        let candidates = vec![(crate::types::Move::NULL, 100)];
        assert_eq!(pick_noisy_move_index(&candidates, 0, 42), 0,
            "a single candidate has nothing to be noisy about");
    }

    #[test]
    fn test_pick_noisy_move_index_never_exceeds_window() {
        // At Skill Level 0 the window is 160cp — a candidate scored 500cp
        // worse than the best should never be selected, however the seed
        // lands, because it's outside the window entirely.
        let candidates = vec![
            (crate::types::Move::NULL, 100),
            (crate::types::Move::NULL, 100 - 500),
        ];
        for seed in 0u64..50 {
            assert_eq!(pick_noisy_move_index(&candidates, 0, seed), 0,
                "a candidate outside the noise window must never be chosen \
                 (seed {seed})");
        }
    }

    #[test]
    fn test_pick_noisy_move_index_can_pick_alternate_within_window() {
        // At Skill Level 0 the window is 160cp — a candidate scored 50cp
        // worse than the best IS eligible, so across enough seeds it
        // should get picked at least once (this is inherently a
        // probabilistic assertion, but with 50 different seeds against a
        // 2-way uniform choice, a pathological PRNG would be needed to
        // never once pick index 1).
        let candidates = vec![
            (crate::types::Move::NULL, 100),
            (crate::types::Move::NULL, 50),
        ];
        let picked_alternate = (0u64..50)
            .any(|seed| pick_noisy_move_index(&candidates, 0, seed) == 1);
        assert!(picked_alternate,
            "an eligible in-window alternate should be picked at least \
             once across 50 different seeds");
    }
}
