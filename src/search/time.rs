// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/time.rs — Time management
//
// Decides how much time to allocate per move from UCI time controls.
// Also handles:
//   - Dynamic time extension (extend when score drops)
//   - Best move stability (stop early when move is stable)
//   - Overhead compensation (buffer for network/GUI lag)
//   - Pondering time management
// ============================================================================

use crate::types::Move;

// ── Time control constants ────────────────────────────────────────────────────

/// Minimum time to spend on a move in milliseconds
const MIN_TIME_MS: u64 = 20;

/// Default safety buffer subtracted from available time (Phase 19: UCI's
/// "Move Overhead" option makes this runtime-configurable via
/// `TimeControl::overhead_ms`; this constant is now only the *default*
/// value a fresh `TimeControl` starts with, not a hardcoded ceiling).
/// Compensates for network lag, GUI overhead, clock inaccuracy.
pub const OVERHEAD_MS: u64 = 30;

/// Default moves to go when not specified
/// Assumes ~30 moves remaining in the game
const DEFAULT_MOVES_TO_GO: u64 = 30;

/// Fraction of remaining time to use per move (when no movestogo)
/// 1/20 = use 5% of remaining time per move
const TIME_FRACTION: u64 = 20;

/// Maximum fraction of total time for one move
/// Never use more than 80% of remaining time on a single move
const MAX_TIME_FRACTION: u64 = 8; // 1/8 = 12.5%... wait we do 80% via /10*8

/// How much to extend time when score drops significantly
const SCORE_DROP_EXTENSION: f64 = 1.5;

/// Score drop threshold that triggers time extension (centipawns)
const SCORE_DROP_THRESHOLD: i32 = 30;

/// How many depths the best move must be stable to stop early
const STABILITY_THRESHOLD: usize = 4;

// ── UCI time control input ────────────────────────────────────────────────────

/// Time control parameters from UCI go command
#[derive(Debug, Clone)]
pub struct TimeControl {
    /// White time remaining in ms (0 = not set)
    pub wtime: u64,
    /// Black time remaining in ms
    pub btime: u64,
    /// White increment per move in ms
    pub winc: u64,
    /// Black increment per move in ms
    pub binc: u64,
    /// Moves until next time control (0 = sudden death)
    pub movestogo: u64,
    /// Fixed time per move in ms (overrides other settings)
    pub movetime: u64,
    /// Search to fixed depth (0 = no limit)
    pub depth: i32,
    /// Search for fixed number of nodes (0 = no limit)
    pub nodes: u64,
    /// Infinite search (only stop on stop command)
    pub infinite: bool,
    /// Pondering (think during opponent's time)
    pub ponder: bool,
    /// Safety buffer subtracted from available time, in ms (Phase 19:
    /// UCI's "Move Overhead" option — runtime-configurable per session,
    /// unlike the old hardcoded constant this replaces). Defaults to
    /// `OVERHEAD_MS`; set from `EngineState.move_overhead_ms` in
    /// `main.rs`'s `cmd_go`, not parsed from the `go` line itself (Move
    /// Overhead is a persistent `setoption`, not a per-search parameter).
    pub overhead_ms: u64,
}

impl Default for TimeControl {
    fn default() -> Self {
        TimeControl {
            wtime: 0,
            btime: 0,
            winc: 0,
            binc: 0,
            movestogo: 0,
            movetime: 0,
            depth: 0,
            nodes: 0,
            infinite: false,
            ponder: false,
            overhead_ms: OVERHEAD_MS,
        }
    }
}

// ── Time allocation ───────────────────────────────────────────────────────────

/// Calculate how much time to allocate for this move
/// Returns (soft_limit_ms, hard_limit_ms)
///   soft_limit: ideal time, can extend dynamically
///   hard_limit: absolute maximum, never exceed
pub fn allocate_time(tc: &TimeControl, is_white: bool) -> (u64, u64) {
    // Fixed movetime overrides everything
    if tc.movetime > 0 {
        let t = tc.movetime.saturating_sub(tc.overhead_ms);
        return (t, t);
    }

    // Infinite search — return very large values
    if tc.infinite || tc.ponder {
        return (u64::MAX / 2, u64::MAX / 2);
    }

    // Fixed depth or nodes — time is not the limit
    if tc.depth > 0 || tc.nodes > 0 {
        return (u64::MAX / 2, u64::MAX / 2);
    }

    // Get our time and increment
    let (our_time, our_inc) = if is_white {
        (tc.wtime, tc.winc)
    } else {
        (tc.btime, tc.binc)
    };

    // If no time set, use defaults
    if our_time == 0 {
        return (5000, 10000);
    }

    // Apply overhead buffer
    let available = our_time.saturating_sub(tc.overhead_ms);

    let soft;
    let hard;

    if tc.movestogo > 0 {
        // Time control with movestogo
        // Divide time by moves remaining, plus a portion of increment
        let mtg = tc.movestogo.max(1);
        soft = (available / mtg + our_inc * 3 / 4).min(available * 4 / 5);
        hard = soft * 3; // Hard limit is 3x soft
    } else {
        // Sudden death — estimate moves remaining
        soft = available / TIME_FRACTION + our_inc * 3 / 4;
        hard = (available / 4).min(soft * 5);
    }

    let soft = soft.max(MIN_TIME_MS).min(available * 8 / 10);
    let hard = hard.max(soft).min(available * 9 / 10);

    (soft, hard)
}

// ── Dynamic time management ───────────────────────────────────────────────────

/// Manages time dynamically during iterative deepening
pub struct TimeManager {
    /// Soft time limit (ideal stopping point)
    pub soft_limit_ms: u64,
    /// Hard time limit (absolute maximum)
    pub hard_limit_ms: u64,
    /// Score from previous depth (for drop detection)
    prev_score: i32,
    /// Best move from previous depth (for stability detection)
    prev_best_move: Move,
    /// How many consecutive depths the best move has been stable
    stability_count: usize,
    /// Current time multiplier (1.0 = normal, >1.0 = extended)
    time_multiplier: f64,
}

impl TimeManager {
    pub fn new(soft_ms: u64, hard_ms: u64) -> Self {
        TimeManager {
            soft_limit_ms:   soft_ms,
            hard_limit_ms:   hard_ms,
            prev_score:      0,
            prev_best_move:  Move::NULL,
            stability_count: 0,
            time_multiplier: 1.0,
        }
    }

    /// Update after each completed depth iteration
    /// Returns true if we should stop searching
    pub fn update(
        &mut self,
        elapsed_ms:    u64,
        best_move:     Move,
        score:         i32,
        depth:         i32,
    ) -> bool {
        // Always respect hard limit
        if elapsed_ms >= self.hard_limit_ms {
            return true;
        }

        // Don't stop before depth 1
        if depth < 1 {
            return false;
        }

        // ── Best move stability ───────────────────────────────────────────────
        // If same move has been best for several depths, stop early
        if best_move == self.prev_best_move && best_move != Move::NULL {
            self.stability_count += 1;
        } else {
            self.stability_count = 0;
        }
        self.prev_best_move = best_move;

        // ── Score drop detection ──────────────────────────────────────────────
        // If score dropped significantly, extend time
        let score_drop = self.prev_score - score;
        if depth > 4 && score_drop > SCORE_DROP_THRESHOLD {
            self.time_multiplier = SCORE_DROP_EXTENSION;
        } else if score_drop <= 0 {
            // Score improved or stable — reset multiplier
            self.time_multiplier = 1.0;
        }
        self.prev_score = score;

        // ── Stop decision ─────────────────────────────────────────────────────
        let effective_soft = (self.soft_limit_ms as f64
            * self.time_multiplier) as u64;

        // Stop early if move is very stable and we've used soft time
        if self.stability_count >= STABILITY_THRESHOLD
            && elapsed_ms >= effective_soft / 2
        {
            return true;
        }

        // Stop at soft limit (adjusted by multiplier)
        elapsed_ms >= effective_soft
    }

    /// Should we start the next depth iteration?
    /// Conservative check before starting — don't start if
    /// likely to run out of time mid-search
    pub fn should_start_next_depth(&self, elapsed_ms: u64) -> bool {
        // Don't start if we've used more than 75% of soft limit
        // Guard against overflow when soft_limit_ms is u64::MAX/2
        if self.soft_limit_ms > u64::MAX / 4 {
            return true; // Infinite time — always start next depth
        }
        elapsed_ms < self.soft_limit_ms * 3 / 4
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tc(wtime: u64, btime: u64, inc: u64) -> TimeControl {
        TimeControl {
            wtime,
            btime,
            winc: inc,
            binc: inc,
            ..Default::default()
        }
    }

    #[test]
    fn test_fixed_movetime() {
        let tc = TimeControl { movetime: 1000, ..Default::default() };
        let (soft, hard) = allocate_time(&tc, true);
        assert_eq!(soft, hard);
        assert!(soft <= 1000);
        assert!(soft > 900); // Allow for overhead
    }

    #[test]
    fn test_infinite_search() {
        let tc = TimeControl { infinite: true, ..Default::default() };
        let (soft, hard) = allocate_time(&tc, true);
        assert!(soft > 1_000_000);
        assert!(hard > 1_000_000);
    }

    #[test]
    fn test_sudden_death_white() {
        let tc = make_tc(60_000, 60_000, 0);
        let (soft, hard) = allocate_time(&tc, true);
        // Should use roughly 1/20 of remaining time
        assert!(soft > MIN_TIME_MS);
        assert!(soft < 60_000);
        assert!(hard >= soft);
    }

    #[test]
    fn test_increment_adds_time() {
        let tc_no_inc  = make_tc(60_000, 60_000, 0);
        let tc_with_inc = make_tc(60_000, 60_000, 500);
        let (soft_no, _)   = allocate_time(&tc_no_inc,  true);
        let (soft_inc, _)  = allocate_time(&tc_with_inc, true);
        assert!(soft_inc > soft_no,
            "Increment should increase allocated time");
    }

    #[test]
    fn test_movestogo() {
        let tc = TimeControl {
            wtime: 60_000, btime: 60_000,
            movestogo: 10,
            ..Default::default()
        };
        let (soft, hard) = allocate_time(&tc, true);
        // With 10 moves to go, should get ~1/10 of time
        assert!(soft > MIN_TIME_MS);
        assert!(hard >= soft);
        assert!(soft <= 60_000);
    }

    #[test]
    fn test_hard_limit_never_exceeds_available() {
        let tc = make_tc(5_000, 5_000, 0);
        let (_, hard) = allocate_time(&tc, true);
        assert!(hard <= 5_000,
            "Hard limit should never exceed available time");
    }

    #[test]
    fn test_time_manager_hard_limit() {
        let mut tm = TimeManager::new(1000, 2000);
        // Past hard limit — always stop
        assert!(tm.update(2001, Move::NULL, 0, 5));
    }

    #[test]
    fn test_time_manager_stable_move_stops_early() {
        use crate::types::{MoveKind, Square};
        let mv = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        let mut tm = TimeManager::new(10_000, 30_000);

        // Same move best for STABILITY_THRESHOLD depths
        for depth in 1..=(STABILITY_THRESHOLD + 1) as i32 {
            tm.update(5_001, mv, 100, depth);
        }
        // After stability threshold, should stop at half soft limit
        let should_stop = tm.update(5_001, mv, 100,
            STABILITY_THRESHOLD as i32 + 2);
        assert!(should_stop,
            "Should stop early when best move is stable");
    }

    #[test]
    fn test_time_manager_score_drop_extends() {
        let mut tm = TimeManager::new(1000, 10_000);
        tm.prev_score = 200;
        // Big score drop — should extend time
        tm.update(500, Move::NULL, 200 - SCORE_DROP_THRESHOLD - 10, 5);
        assert!(tm.time_multiplier > 1.0,
            "Score drop should extend time multiplier");
    }

    #[test]
    fn test_should_start_next_depth() {
        let tm = TimeManager::new(1000, 5000);
        assert!(tm.should_start_next_depth(100),
            "Should start next depth early in search");
        assert!(!tm.should_start_next_depth(800),
            "Should not start next depth near soft limit");
    }

    #[test]
    fn test_min_time_enforced() {
        // Very low time — should still get minimum
        let tc = make_tc(100, 100, 0);
        let (soft, _) = allocate_time(&tc, true);
        assert!(soft >= MIN_TIME_MS || soft == 0,
            "Should enforce minimum time or be 0 for very low time");
    }

    #[test]
    fn test_overhead_ms_defaults_to_constant() {
        let tc = TimeControl::default();
        assert_eq!(tc.overhead_ms, OVERHEAD_MS,
            "a fresh TimeControl should default to the standard overhead");
    }

    #[test]
    fn test_move_overhead_is_configurable() {
        // Same clock values, different overhead — larger overhead should
        // eat further into available time and produce a smaller allocation.
        let tc_default = make_tc(10_000, 10_000, 0);
        let tc_large_overhead = TimeControl {
            overhead_ms: 2000,
            ..make_tc(10_000, 10_000, 0)
        };
        let (soft_default, _) = allocate_time(&tc_default, true);
        let (soft_large, _)   = allocate_time(&tc_large_overhead, true);
        assert!(soft_large < soft_default,
            "a larger Move Overhead should reduce the time allocation");
    }

    #[test]
    fn test_movetime_respects_custom_overhead() {
        let tc = TimeControl {
            movetime: 1000,
            overhead_ms: 100,
            ..Default::default()
        };
        let (soft, hard) = allocate_time(&tc, true);
        assert_eq!(soft, 900, "movetime - custom overhead");
        assert_eq!(hard, 900);
    }
}
