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
    tt:     &mut TranspositionTable,
) -> SearchResult {
    let is_white = pos.side_to_move == crate::types::Color::White;
    let (soft_ms, hard_ms) = allocate_time(tc, is_white);

    let mut tm = TimeManager::new(soft_ms, hard_ms);

    // Override with fixed depth/nodes if set
    let max_depth = if tc.depth > 0 {
        tc.depth
    } else {
        crate::search::MAX_DEPTH as i32
    };
    if tc.nodes > 0 {
        info.node_limit = tc.nodes;
    }

    info.reset_for_search();
    tt.new_search();

    // Record root position in game history
    pos.game_history.push(pos.hash);

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
    };

    // ── Iterative deepening loop ───────────────────────────────────────────────
    for depth in 1..=max_depth {
        info.current_depth = depth;

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
        };

        // Print UCI info for this depth
        println!("{}", result.to_uci_info());

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

    // Remove root position from game history
    pos.game_history.pop();

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
    tt:         &mut TranspositionTable,
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

fn search_with_aspiration(
    pos:        &mut Position,
    depth:      i32,
    prev_score: i32,
    info:       &mut SearchInfo,
    tt:         &mut TranspositionTable,
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
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

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
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(8);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        assert_eq!(result.depth, 8,
            "Should reach requested depth");
    }

    #[test]
    fn test_iterative_deepening_respects_time() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let mut tt   = TranspositionTable::new(16);
        let tc       = movetime_tc(200); // 200ms

        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        // Should complete within reasonable time
        assert!(result.time_ms <= 500,
            "Should complete within time limit (got {}ms)", result.time_ms);
        assert_ne!(result.best_move, Move::NULL);
    }

    #[test]
    fn test_finds_mate_in_1() {
        setup();
        // White Queen on h7, King on g6, Black King on h8 — mate in 1
        let fen = "7k/7Q/6K1/8/8/8/8/8 w - - 0 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(3);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        assert!(result.is_mate, "Should detect forced mate");
        assert!(result.mate_in > 0, "Should be mating (positive mate_in)");
    }

    #[test]
    fn test_result_move_is_legal() {
        setup();
        for seed in 0..5u64 {
            let mut pos  = Position::generate_with_seed(seed);
            let mut info = SearchInfo::new();
            let mut tt   = TranspositionTable::new(16);
            let tc       = fixed_depth_tc(4);

            let result = iterative_deepening(
                &mut pos, &tc, &mut info, &mut tt
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
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(6);

        // Should not panic even with aspiration window failures
        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);
        assert_ne!(result.best_move, Move::NULL);
    }

    #[test]
    fn test_game_history_restored_after_search() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let initial_history_len = pos.game_history.len();
        let mut info = SearchInfo::new();
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(4);

        iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        assert_eq!(pos.game_history.len(), initial_history_len,
            "Game history should be same length after search");
    }

    #[test]
    fn test_nodes_counted() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let mut tt   = TranspositionTable::new(16);
        let tc       = fixed_depth_tc(5);

        let result = iterative_deepening(&mut pos, &tc, &mut info, &mut tt);

        assert!(result.nodes > 0, "Should count searched nodes");
        assert!(result.nps > 0,   "Should compute NPS");
    }
}
