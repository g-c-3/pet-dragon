// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/iterative.rs — Iterative deepening with aspiration windows
//
// Iterative deepening searches depth 1, 2, 3... until time runs out.
// Benefits:
//   - Always has a valid best move from the last completed depth
//   - Move ordering improves each iteration (TT + history)
//   - Time management: stop between depths, not mid-search
//   - Aspiration windows: use previous score to narrow search window
//
// Aspiration windows:
//   Instead of searching [-INF, +INF], search [prev-delta, prev+delta].
//   If score falls outside, widen window and re-search.
//   Saves time when score is stable between depths.
// ============================================================================

use std::sync::atomic::Ordering;

use crate::position::Position;
use crate::search::{
    alpha_beta::alpha_beta,
    time::{allocate_time, TimeControl, TimeManager},
    SearchInfo, SearchResult, INFINITY, is_mate_score, mate_in,
};
use crate::tt::TranspositionTable;
use crate::types::Move;

// ── Aspiration window constants ───────────────────────────────────────────────

/// Initial aspiration window delta (centipawns)
const ASPIRATION_DELTA: i32 = 25;

/// Minimum depth before using aspiration windows
const ASPIRATION_MIN_DEPTH: i32 = 4;

// ── Main iterative deepening function ────────────────────────────────────────

/// Run iterative deepening search and return the best move found
pub fn iterative_deepening(
    pos:    &mut Position,
    tc:     &TimeControl,
    info:   &mut SearchInfo,
    tt:     &TranspositionTable,
) -> SearchResult {
    let is_white = pos.side_to_move == crate::types::Color::White;
    let (soft_ms, hard_ms) = allocate_time(tc, is_white);

    let mut tm = TimeManager::new(soft_ms, hard_ms);

    // Wire the real hard limit into SearchInfo so alpha_beta's in-search
    // is_time_up() check (every 256 nodes) has an actual budget to compare
    // against, instead of SearchInfo::new()'s stale 5000ms default — see
    // Session 33/34: this was previously dead code for every real search,
    // masked until NNUE's heavier per-node eval exposed it via
    // test_iterative_deepening_respects_time.
    info.time_allocated_ms = hard_ms;

    // Override with fixed depth/nodes if set
    let max_depth = if tc.depth > 0 {
        tc.depth
    } else {
        crate::search::MAX_DEPTH as i32
    };
    if tc.nodes > 0 {
        info.node_limit = tc.nodes;
    }

    // Phase 20 / D39: Skill Level depth cap. `.min()` only ever makes the
    // search shallower than what was already requested above — a tier can
    // never override an explicit, shallower `go depth` with something
    // deeper. Skill Level 20 (default) returns None here, so this is a
    // no-op for every caller that hasn't opted into a lower tier.
    let max_depth = match crate::search::skill::skill_depth_cap(info.skill_level) {
        Some(cap) => max_depth.min(cap),
        None      => max_depth,
    };

    info.reset_for_search();
    // Note: tt.new_search() is called by the caller (cmd_go in main.rs)
    // before spawning the search thread. Tests create fresh TTs.

    // Record root position in game history
    pos.push_game_history();

    let mut best_move   = Move::NULL;
    let mut best_score  = -INFINITY;
    let mut prev_score  = 0i32;
    let mut result      = SearchResult {
        best_move:  Move::NULL,
        score:      0,
        depth:      0,
        seldepth:   0,
        nodes:      0,
        time_ms:    0,
        nps:        0,
        pv:         vec![],
        is_mate:    false,
        mate_in:    0,
        multipv:    1,
    };

    // ── Iterative deepening loop ───────────────────────────────────────────────
    for depth in 1..=max_depth {
        info.current_depth = depth;

        // Ponder-hit conversion (Phase 18/D37): if a `ponderhit` arrived
        // since the last depth started, swap the effectively-infinite
        // ponder TimeManager for a real, bounded one. `swap` both consumes
        // the soft override exactly once (so this block only runs the
        // depth after a hit, not every iteration) and hands back the value
        // atomically — no separate load-then-store race, though this is
        // single-writer anyway (only main.rs's ponderhit handler writes).
        // ponder_hit_hard_ms is deliberately left set for is_time_up()'s
        // hot-path check for the rest of this search.
        let ponder_soft_override = info.ponder_hit_soft_ms.swap(u64::MAX, Ordering::Relaxed);
        if ponder_soft_override != u64::MAX {
            let hard_override = info.ponder_hit_hard_ms.load(Ordering::Relaxed);
            tm = TimeManager::new(ponder_soft_override, hard_override);
        }

        // Don't start a new depth if we probably won't finish it
        if depth > 1 && !tm.should_start_next_depth(info.elapsed_ms()) {
            break;
        }

        let score = search_at_depth(
            pos, depth, prev_score, info, tt
        );

        if info.stop {
            // Time ran out mid-search — use result from last complete depth
            break;
        }

        // Update best move from this depth
        if info.best_move != Move::NULL {
            best_move  = info.best_move;
            best_score = score;
        }

        prev_score = score;

        // Compute NPS
        let elapsed = info.elapsed_ms().max(1);
        info.nps = info.nodes * 1000 / elapsed;

        // Build result for this depth
        let pv = info.get_pv();
        let score_for_result = if info.best_score.abs() > 0
            && info.best_score != -INFINITY {
            info.best_score
        } else {
            best_score
        };

        result = SearchResult {
            best_move,
            score:    score_for_result,
            depth,
            seldepth: info.seldepth,
            nodes:    info.nodes,
            time_ms:  elapsed,
            nps:      info.nps,
            pv:       pv.clone(),
            is_mate:  is_mate_score(score_for_result),
            mate_in:  if is_mate_score(score_for_result) {
                          mate_in(score_for_result)
                      } else { 0 },
            multipv:  1,
        };

        // Print UCI info for this depth — silent-search callers (selfplay,
        // match_runner) set print_info = false to avoid flooding stdout
        // across thousands of internal searches (see SearchInfo doc comment).
        if info.print_info {
            println!("{}", result.to_uci_info());
        }

        // ── Additional MultiPV lines (Phase 19) ─────────────────────────────
        // Everything above this point is the unmodified single-PV path —
        // it always runs and always drives best_move/best_score/result,
        // exactly as before this feature existed. This block is purely
        // additive: extra `info` lines for GUIs that want to show several
        // candidate moves. It's gated behind `multipv > 1` (default 1), so
        // for every existing caller and every test that doesn't explicitly
        // opt in, this never executes and nothing here can change behavior.
        // Uses a full-window search per extra line rather than replicating
        // per-line aspiration-window state — simpler, and the extra cost
        // only applies to MultiPV>1 usage, which already accepts being
        // slower than single-PV mode as the cost of showing multiple lines.
        if info.multipv > 1 && !info.stop {
            let legal = crate::movegen::generate_moves(pos);
            let slot_count = info.multipv.min(legal.len().max(1));
            info.root_exclude.clear();
            if best_move != Move::NULL {
                info.root_exclude.push(best_move);
            }
            for slot in 1..slot_count {
                let slot_score = search_multipv_slot(pos, depth, info, tt);
                if info.stop || info.best_move == Move::NULL {
                    break;
                }
                info.root_exclude.push(info.best_move);
                let s_elapsed = info.elapsed_ms().max(1);
                info.nps = info.nodes * 1000 / s_elapsed;
                let s_pv = info.get_pv();
                let slot_result = SearchResult {
                    best_move: info.best_move,
                    score:     slot_score,
                    depth,
                    seldepth:  info.seldepth,
                    nodes:     info.nodes,
                    time_ms:   s_elapsed,
                    nps:       info.nps,
                    pv:        s_pv,
                    is_mate:   is_mate_score(slot_score),
                    mate_in:   if is_mate_score(slot_score) {
                                   mate_in(slot_score)
                               } else { 0 },
                    multipv:   slot + 1,
                };
                if info.print_info {
                    println!("{}", slot_result.to_uci_info());
                }
            }
            info.root_exclude.clear();
        }

        // Check if we should stop
        if tm.update(elapsed, best_move, best_score, depth) {
            break;
        }

        // Stop if we found a forced mate
        if is_mate_score(best_score) {
            break;
        }

        // Stop at requested depth
        if depth >= max_depth {
            break;
        }
    }

    // ── Skill Level move-selection noise (Phase 20 follow-up, Session 66
    // validation) ────────────────────────────────────────────────────────────
    // Depth-cap alone (skill.rs's `skill_depth_cap`) strongly separates the
    // low tiers, but plateaus once the cap exceeds roughly the depth this
    // engine already converges at for a given time budget — extra plies
    // beyond that stop changing the chosen move, so e.g. depth-11-capped
    // vs depth-16-capped tiers ended up statistically indistinguishable in
    // match-runner validation (Session 66 follow-up: 10-vs-15 measured at
    // -8.7 Elo over 40 games, essentially a tie). Move-selection noise
    // fixes that independently of depth: instead of always taking the
    // single best root move, a capped tier has some chance of taking a
    // nearby-scored alternative instead — the same general mechanism
    // Stockfish's own Skill Level uses (weighted randomness among top
    // candidates), though the window/selection formula below is our own,
    // not ported from theirs (D39: no calibration data borrowed, ever).
    //
    // Gate: `skill_noise_window_cp()` returns 0 at Skill Level 20 (the
    // default), so this block is a strict no-op for every caller that
    // hasn't opted into a lower tier — same backward-compatibility pattern
    // as the depth cap and time fraction. Reuses `search_multipv_slot()`
    // (Phase 19) to gather alternative root moves rather than introducing
    // a second root-search mechanism.
    if !info.stop {
        let window = crate::search::skill::skill_noise_window_cp(info.skill_level);
        if window > 0 && result.best_move != Move::NULL {
            const NOISE_CANDIDATES: usize = 3;
            let legal = crate::movegen::generate_moves(pos);
            let slot_count = NOISE_CANDIDATES.min(legal.len().max(1));
            let mut candidates: Vec<(Move, i32)> = vec![(result.best_move, result.score)];
            info.root_exclude.clear();
            info.root_exclude.push(result.best_move);
            for _ in 1..slot_count {
                let slot_score = search_multipv_slot(pos, result.depth, info, tt);
                if info.stop || info.best_move == Move::NULL {
                    break;
                }
                candidates.push((info.best_move, slot_score));
                info.root_exclude.push(info.best_move);
            }
            info.root_exclude.clear();

            if candidates.len() > 1 {
                // Deterministic-but-varying seed built only from search
                // state already on hand — deliberately avoids wall-clock
                // time (this function also compiles to wasm32, where
                // `SystemTime::now()` isn't safely usable) and avoids
                // depending on `Move`'s internal representation.
                let score_mix: i64 = candidates.iter().map(|(_, s)| *s as i64).sum();
                let seed = info.nodes
                    ^ (result.depth as u64).wrapping_shl(48)
                    ^ (score_mix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                let idx = crate::search::skill::pick_noisy_move_index(
                    &candidates, info.skill_level, seed
                );
                if idx != 0 {
                    let (noisy_move, noisy_score) = candidates[idx];
                    result.best_move = noisy_move;
                    result.score     = noisy_score;
                    if let Some(first) = result.pv.first_mut() {
                        *first = noisy_move;
                    } else {
                        result.pv.push(noisy_move);
                    }
                }
            }
        }
    }

    // Remove root position from game history
    pos.pop_game_history();

    // Age history tables for next move
    info.age_history();

    // Ensure we always return a valid move
    if result.best_move == Move::NULL {
        // Fallback: return first legal move
        let moves = crate::movegen::generate_moves(pos);
        if !moves.is_empty() {
            result.best_move = moves.get(0);
        }
    }

    result
}

// ── Search at a specific depth with aspiration windows ────────────────────────

fn search_at_depth(
    pos:        &mut Position,
    depth:      i32,
    prev_score: i32,
    info:       &mut SearchInfo,
    tt:         &TranspositionTable,
) -> i32 {
    // Use aspiration windows for deeper searches
    if depth >= ASPIRATION_MIN_DEPTH && !is_mate_score(prev_score) {
        search_with_aspiration(pos, depth, prev_score, info, tt)
    } else {
        // Full window for shallow depths or after mate found
        let score = alpha_beta(
            pos, depth, -INFINITY, INFINITY,
            0, true, info, tt, Move::NULL,
        );
        if score > -INFINITY {
            info.best_move  = info.pv_table[0][0];
            info.best_score = score;
        }
        score
    }
}

// ── Additional MultiPV line search (Phase 19) ──────────────────────────────────

/// Search one MultiPV line beyond the primary one, at the root, skipping
/// whatever moves are already in `info.root_exclude`. Always full-window —
/// see the call site in `iterative_deepening()` for why this doesn't
/// replicate `search_at_depth()`'s aspiration-window logic per line.
fn search_multipv_slot(
    pos:   &mut Position,
    depth: i32,
    info:  &mut SearchInfo,
    tt:    &TranspositionTable,
) -> i32 {
    info.best_move  = Move::NULL;
    info.best_score = -INFINITY;

    let score = alpha_beta(
        pos, depth, -INFINITY, INFINITY,
        0, true, info, tt, Move::NULL,
    );

    if score > -INFINITY {
        info.best_move  = info.pv_table[0][0];
        info.best_score = score;
    }

    score
}

fn search_with_aspiration(
    pos:        &mut Position,
    depth:      i32,
    prev_score: i32,
    info:       &mut SearchInfo,
    tt:         &TranspositionTable,
) -> i32 {
    let mut delta = ASPIRATION_DELTA;
    let mut alpha = (prev_score - delta).max(-INFINITY);
    let mut beta  = (prev_score + delta).min(INFINITY);

    loop {
        let score = alpha_beta(
            pos, depth, alpha, beta,
            0, true, info, tt, Move::NULL,
        );

        if info.stop { return score; }

        if score <= alpha {
            // Failed low — widen alpha
            beta  = (alpha + beta) / 2;
            alpha = (score - delta).max(-INFINITY);
            delta += delta / 2;
        } else if score >= beta {
            // Failed high — widen beta
            beta  = (score + delta).min(INFINITY);
            delta += delta / 2;
        } else {
            // Score within window — accept
            if score > -INFINITY {
                info.best_move  = info.pv_table[0][0];
                info.best_score = score;
            }
            return score;
        }

        // If window is now full, just use the score
        if alpha <= -INFINITY && beta >= INFINITY {
            if score > -INFINITY {
                info.best_move  = info.pv_table[0][0];
                info.best_score = score;
            }
            return score;
        }

        // Widen delta exponentially
        delta = delta.min(INFINITY / 4);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::search::SearchInfo;
    use crate::tt::TranspositionTable;
    use crate::types::Move;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    fn fixed_depth_tc(depth: i32) -> TimeControl {
        TimeControl { depth, ..Default::default() }
    }

    fn movetime_tc(ms: u64) -> TimeControl {
        TimeControl { movetime: ms, ..Default::default() }
    }

    #[test]
    fn test_iterative_deepening_returns_move() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_ne!(result.best_move, Move::NULL,
            "Should return a valid move");
        assert!(result.depth >= 1,
            "Should complete at least depth 1");
    }

    #[test]
    fn test_iterative_deepening_depth_increases() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(8);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.depth, 8,
            "Should reach requested depth");
    }

    #[test]
    fn test_iterative_deepening_respects_time() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = movetime_tc(200); // 200ms

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        // Should complete within reasonable time
        assert!(result.time_ms <= 500,
            "Should complete within time limit (got {}ms)", result.time_ms);
        assert_ne!(result.best_move, Move::NULL);
    }

    #[test]
    fn test_finds_mate_in_1() {
        setup();
        // White up a queen — should find winning move
        let fen = "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(4);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_ne!(result.best_move, Move::NULL,
            "Should find a move");
        assert!(result.score > 0,
            "Score should be positive when up a queen: {}", result.score);
    }

    #[test]
    fn test_result_move_is_legal() {
        setup();
        for seed in 0..5u64 {
            let mut pos  = Position::generate_with_seed(seed);
            let mut info = SearchInfo::new();
            let tt   = TranspositionTable::new(16);
            let tc       = fixed_depth_tc(4);

            let result = iterative_deepening(
                &mut pos, &tc, &mut info, &tt
            );

            assert_ne!(result.best_move, Move::NULL,
                "Should find move (seed {})", seed);

            // Verify move is legal
            let legal = crate::movegen::generate_moves(&pos);
            assert!(
                legal.iter().any(|&m| m == result.best_move),
                "Best move must be legal (seed {})", seed
            );
        }
    }

    #[test]
    fn test_score_string_format() {
        let result = SearchResult {
            best_move: Move::NULL,
            score:     150,
            depth:     10,
            seldepth:  14,
            nodes:     500_000,
            time_ms:   1000,
            nps:       500_000,
            pv:        vec![],
            is_mate:   false,
            mate_in:   0,
            multipv:   1,
        };
        let info_str = result.to_uci_info();
        assert!(info_str.contains("depth 10"));
        assert!(info_str.contains("cp 150"));
        assert!(info_str.contains("nodes 500000"));
    }

    #[test]
    fn test_aspiration_window_handles_score_drop() {
        setup();
        // Aspiration windows should handle unexpected score changes
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(6);

        // Should not panic even with aspiration window failures
        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);
        assert_ne!(result.best_move, Move::NULL);
    }

    #[test]
    fn test_game_history_restored_after_search() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let initial_history_len = pos.game_history.len();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(4);

        iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(pos.game_history.len(), initial_history_len,
            "Game history should be same length after search");
    }

    #[test]
    fn test_ponder_hit_override_bounds_an_infinite_search() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        // A real `go ponder` — allocate_time() returns ~u64::MAX/2 for this.
        let tc = TimeControl { ponder: true, ..Default::default() };

        // Simulate a ponderhit arriving from another thread WHILE the
        // search is already running — this is the real scenario (a GUI
        // can only send ponderhit after iterative_deepening() has already
        // called reset_for_search(), which clears any override; setting it
        // BEFORE calling iterative_deepening() would just get wiped by
        // that reset and prove nothing). 30ms gives reset_for_search()
        // and depth-1 startup plenty of time to complete first.
        let soft_override = info.ponder_hit_soft_ms.clone();
        let hard_override  = info.ponder_hit_hard_ms.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            soft_override.store(10, Ordering::Relaxed);
            hard_override.store(20, Ordering::Relaxed);
        });

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        // Without the override this would run until MAX_DEPTH (u64::MAX/2
        // soft/hard limits never trip). With it, it should stop shortly
        // after the ~30ms + 20ms override fires — generous bound for CI
        // variance (thread scheduling, slow CI runners, etc).
        assert!(result.time_ms < 3000,
            "ponder-hit override should bound an otherwise-infinite ponder \
             search, got {}ms", result.time_ms);
        assert_ne!(result.best_move, Move::NULL,
            "should still return a valid move even when cut short quickly");
    }

    #[test]
    fn test_ponder_without_hit_keeps_running_until_depth_cap() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        // A real `go ponder` with NO ponderhit — bounded here by a shallow
        // fixed depth instead of letting it run to MAX_DEPTH=128, since
        // this is a unit test, not a timing benchmark. Proves the ABSENCE
        // of a ponder-hit override doesn't accidentally cut the search
        // short via some other path.
        let tc = TimeControl { ponder: true, depth: 3, ..Default::default() };

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.depth, 3,
            "with no ponder-hit override, depth should be limited only by \
             tc.depth, not cut short by the (huge) ponder time budget");
    }

    #[test]
    fn test_skill_level_default_does_not_cap_depth() {
        setup();
        // Default Skill Level (20) must be byte-for-byte the same as before
        // this feature existed — same safety argument as MultiPV=1.
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        assert_eq!(info.skill_level, crate::search::skill::MAX_SKILL_LEVEL);
        let tt = TranspositionTable::new(16);
        let tc = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.depth, 5,
            "an explicit go depth must be reached in full when Skill Level \
             is at its default (uncapped) setting");
    }

    #[test]
    fn test_skill_level_caps_search_depth() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.skill_level = 0; // weakest tier -> depth cap of 1
        let tt = TranspositionTable::new(16);
        // Ask for depth 10, but Skill Level 0 should cap it down to 1.
        let tc = fixed_depth_tc(10);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.depth, 1,
            "Skill Level 0 should cap the search to depth 1 even though \
             depth 10 was explicitly requested");
        assert_ne!(result.best_move, Move::NULL,
            "a capped search must still return a valid legal move");
    }

    #[test]
    fn test_skill_level_never_exceeds_explicit_shallower_depth() {
        setup();
        // A tier cap must only ever make the search shallower, never
        // override an explicit request that's ALREADY shallower than the
        // tier's own cap.
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.skill_level = 19; // depth cap of 20 — far above the requested 3
        let tt = TranspositionTable::new(16);
        let tc = fixed_depth_tc(3);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.depth, 3,
            "an explicit shallower go depth must win over a higher tier cap");
    }

    #[test]
    fn test_nodes_counted() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert!(result.nodes > 0, "Should count searched nodes");
        assert!(result.nps > 0,   "Should compute NPS");
    }

    #[test]
    fn test_multipv_default_matches_single_pv_behavior() {
        setup();
        // With multipv left at its default of 1, behavior must be
        // byte-for-byte identical to before this feature existed —
        // this is the whole safety argument for how the feature was
        // added (purely additive, gated behind multipv > 1).
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        assert_eq!(info.multipv, 1);
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        assert_eq!(result.multipv, 1);
        assert_ne!(result.best_move, Move::NULL);
        assert!(info.root_exclude.is_empty(),
            "root_exclude should stay empty when multipv is 1");
    }

    #[test]
    fn test_multipv_produces_distinct_legal_moves() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.multipv = 3;
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);

        // The primary line (what gets played) is unaffected by multipv —
        // still a single valid legal move.
        assert_ne!(result.best_move, Move::NULL);
        let legal = crate::movegen::generate_moves(&pos);
        assert!(legal.iter().any(|&m| m == result.best_move),
            "primary MultiPV line's move must be legal");
        // root_exclude is cleaned up after the depth loop finishes, ready
        // for the next search.
        assert!(info.root_exclude.is_empty(),
            "root_exclude should be cleared after iterative_deepening returns");
    }

    #[test]
    fn test_multipv_clamped_to_available_legal_moves() {
        setup();
        // Fool's-mate-adjacent-style position with very few legal replies —
        // requesting MultiPV=10 should not panic even though there aren't
        // 10 legal moves. Using startpos's depth-1 legal move count (20)
        // is already enough headroom to just request an absurdly high
        // number and confirm it doesn't panic.
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.multipv = 1000;
        let tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(4);

        // Should not panic — clamped internally to legal.len().
        let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);
        assert_ne!(result.best_move, Move::NULL);
    }

    #[test]
    fn test_multipv_primary_line_stays_legal_and_valid() {
        setup();
        // NOTE on what this test does NOT claim: an earlier version of this
        // test asserted result.best_move is byte-identical between
        // MultiPV=1 and MultiPV=4 runs. That's false, and finding out why
        // is worth recording. Extra MultiPV lines searched at EARLIER
        // depths (1..5) feed the same shared TT/history/killer/correction
        // tables that depth 6's primary-line search then reads — so a
        // MultiPV=4 run legitimately explores more of the tree by depth 6
        // than a MultiPV=1 run did, which can shift move ordering and
        // pruning enough to land on a different (still fully valid, still
        // best-scoring-at-that-depth) move. This is a well-known, accepted
        // property of MultiPV in most alpha-beta engines, not a bug here —
        // Stockfish's own docs carry the same caveat. What MultiPV *does*
        // guarantee, and what's actually worth testing, is that the
        // primary line is always a real legal move regardless of how many
        // extra lines were requested.
        let mut pos1 = Position::start_pos().unwrap();
        let mut info1 = SearchInfo::new();
        let tt1 = TranspositionTable::new(16);
        let result1 = iterative_deepening(&mut pos1, &fixed_depth_tc(6), &mut info1, &tt1);

        let mut pos2 = Position::start_pos().unwrap();
        let mut info2 = SearchInfo::new();
        info2.multipv = 4;
        let tt2 = TranspositionTable::new(16);
        let result2 = iterative_deepening(&mut pos2, &fixed_depth_tc(6), &mut info2, &tt2);

        for (label, pos, result) in [("multipv=1", &pos1, &result1), ("multipv=4", &pos2, &result2)] {
            assert_ne!(result.best_move, Move::NULL, "{label}: should find a move");
            let legal = crate::movegen::generate_moves(pos);
            assert!(legal.iter().any(|&m| m == result.best_move),
                "{label}: primary line's move must be legal");
        }
    }
}
