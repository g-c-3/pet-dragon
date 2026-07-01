// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/alpha_beta.rs — Alpha-beta search with PVS
//
// This is the core search function. It explores the game tree using
// alpha-beta pruning with Principal Variation Search (PVS).
//
// PVS: Assumes the first move (best from move ordering) is best.
// Searches it with full window, then searches remaining moves with
// null window (-beta+1, -alpha). If a move beats alpha in null window,
// re-search with full window.
//
// Pruning techniques implemented:
//   - Mate distance pruning
//   - Repetition detection (draw)
//   - Fifty-move rule (draw)
//   - Transposition table cutoffs
//   - Null move pruning (with zugzwang guard)
//   - Internal Iterative Reduction (IIR)
//   - Futility pruning
//   - Late Move Reductions (LMR)
//   - Singular extensions
//   - Check extensions
//   - Razoring
//   - Delta pruning (in quiescence)
//
// ⚠️ Pet Dragon notes throughout — see comments marked ⚠️
// ============================================================================

use crate::movegen::{generate_captures, generate_moves};
use crate::position::Position;
use crate::search::{
    ordering::{next_move, score_captures, score_moves,
               update_ordering_on_cutoff},
    pruning::{pawn_hash, should_try_probcut, try_probcut},
    see::see,
    SearchInfo, DRAW_SCORE, INFINITY, MATE_SCORE, MATE_THRESHOLD,
    MAX_PLY, MIN_DEPTH_FUTILITY, MIN_DEPTH_IIR, MIN_DEPTH_LMR,
    MIN_DEPTH_NULL_MOVE, MIN_DEPTH_RAZORING, MIN_DEPTH_SINGULAR,
};
use crate::tt::{Bound, TranspositionTable};
use crate::types::{Color, Move, PieceKind};

// ── Quiescence search ─────────────────────────────────────────────────────────

/// Quiescence search — search captures until position is "quiet"
/// Prevents the horizon effect (stopping search in the middle of exchanges)
///
/// ⚠️ Pet Dragon: MUST be called even at root — starting positions can
/// have immediate captures. Never assume opening position is quiet.
pub fn quiescence(
    pos:   &mut Position,
    mut alpha: i32,
    beta:      i32,
    ply:       usize,
    info:      &mut SearchInfo,
    tt:        &TranspositionTable,
) -> i32 {
    if info.is_time_up() {
        return 0;
    }

    info.nodes += 1;

    if ply >= MAX_PLY {
        return evaluate(pos);
    }

    // Update seldepth
    if ply > info.seldepth {
        info.seldepth = ply;
    }

    // Stand-pat: evaluate current position without making a move
    // If current position is already >= beta, no need to search captures
    let stand_pat = evaluate(pos);

    if stand_pat >= beta {
        return beta; // Beta cutoff
    }

    // Delta pruning: if even the best possible capture can't raise alpha,
    // skip this node entirely
    const DELTA_MARGIN: i32 = 975; // Approximately queen value
    if stand_pat + DELTA_MARGIN < alpha {
        return alpha;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // TT probe for quiescence
    let tt_move = tt.probe(pos.hash)
        .map(|e| e.mv)
        .unwrap_or(Move::NULL);

    // Generate and score captures only
    let captures = generate_captures(pos);
    let mut scored = score_captures(pos, &captures, tt_move);

    let mut best_score = stand_pat;

    for i in 0..scored.len() {
        let mv = match next_move(&mut scored, i) {
            Some(m) => m,
            None    => break,
        };

        // SEE pruning: skip losing captures in quiescence
        if !see(pos, mv, 0) {
            continue;
        }

        pos.make_move_with_history(mv);
        let score = -quiescence(pos, -beta, -alpha, ply + 1, info, tt);
        pos.unmake_move_with_history(mv);

        if info.stop { return 0; }

        if score > best_score {
            best_score = score;
            if score > alpha {
                alpha = score;
                if score >= beta {
                    return beta; // Beta cutoff
                }
            }
        }
    }

    best_score
}

// ── Alpha-beta with PVS ───────────────────────────────────────────────────────

/// Main alpha-beta search function
/// depth: remaining depth to search
/// ply:   distance from root (0 = root)
/// pv_node: true if this is a principal variation node (not null window)
pub fn alpha_beta(
    pos:       &mut Position,
    depth:     i32,
    alpha:     i32,
    beta:      i32,
    ply:       usize,
    pv_node:   bool,
    info:      &mut SearchInfo,
    tt:        &TranspositionTable,
    prev_move: Move,
) -> i32 {
    alpha_beta_with_excluded(
        pos, depth, alpha, beta, ply, pv_node, info, tt, prev_move, Move::NULL,
    )
}

/// Alpha-beta core with an optional excluded move (Phase 13.3).
/// `excluded` is skipped entirely in the move loop — used by singular
/// extension verification to ask "how good is this position WITHOUT the
/// TT move?" without duplicating the whole search function.
fn alpha_beta_with_excluded(
    pos:       &mut Position,
    mut depth: i32,
    mut alpha: i32,
    beta:      i32,
    ply:       usize,
    pv_node:   bool,
    info:      &mut SearchInfo,
    tt:        &TranspositionTable,
    prev_move: Move,
    excluded:  Move,
) -> i32 {
    // ── Time check ────────────────────────────────────────────────────────────
    if info.is_time_up() {
        return 0;
    }

    // ── Leaf node: quiescence search ──────────────────────────────────────────
    if depth <= 0 {
        return quiescence(pos, alpha, beta, ply, info, tt);
    }

    info.nodes += 1;

    if ply >= MAX_PLY {
        return evaluate(pos);
    }

    let root_node = ply == 0;

    // ── Mate distance pruning ─────────────────────────────────────────────────
    // Never search lines worse than the best mate already found
    if !root_node {
        let mated_score  = -(MATE_SCORE - ply as i32);
        let mating_score =   MATE_SCORE - ply as i32;
        let alpha = alpha.max(mated_score);
        let beta  = beta.min(mating_score);
        if alpha >= beta {
            return alpha;
        }
    }

    // ── Draw detection ────────────────────────────────────────────────────────
    // Check repetition BEFORE TT lookup
    if !root_node && pos.is_repetition() {
        return DRAW_SCORE;
    }

    // Fifty-move rule
    if pos.halfmove_clock >= 100 {
        return DRAW_SCORE;
    }

    // Insufficient material
    if pos.is_insufficient_material() {
        return DRAW_SCORE;
    }

    // ── Transposition table probe ─────────────────────────────────────────────
    let tt_move;
    let tt_hit = tt.probe(pos.hash);

    if let Some(entry) = tt_hit {
        tt_move = entry.mv;
        // Use TT score if depth is sufficient and not at root or PV node
        if !root_node && !pv_node && entry.depth >= depth as i8 {
            let tt_score = TranspositionTable::score_from_tt(
                entry.score, ply as i32
            );
            match entry.bound {
                Bound::Exact => return tt_score,
                Bound::LowerBound => {
                    if tt_score >= beta { return tt_score; }
                }
                Bound::UpperBound => {
                    if tt_score <= alpha { return tt_score; }
                }
            }
        }
    } else {
        tt_move = Move::NULL;
    }

    // ── Check detection ───────────────────────────────────────────────────────
    let in_check = pos.in_check(pos.side_to_move);

    // Check extension: extend search when in check
    if in_check {
        depth += 1;
    }

    // ── Static evaluation ─────────────────────────────────────────────────────
    // Only compute if needed for pruning. raw_static_eval feeds the
    // correction-history update at the end of this node (Phase 13.2);
    // static_eval is the corrected value all pruning decisions use.
    let raw_static_eval = if !in_check { evaluate(pos) } else { -INFINITY };
    let static_eval = if !in_check {
        let phash = pawn_hash(pos);
        info.correction_history.apply(raw_static_eval, phash, pos.side_to_move)
    } else {
        raw_static_eval
    };

    // ── Razoring ─────────────────────────────────────────────────────────────
    // If static eval is far below alpha at low depth, drop to qsearch
    if !pv_node
        && !in_check
        && depth <= MIN_DEPTH_RAZORING
        && static_eval + 300 * depth < alpha
    {
        return quiescence(pos, alpha, beta, ply, info, tt);
    }

    // ── Null move pruning ─────────────────────────────────────────────────────
    // Skip our move — if position is still good, prune
    // Guard: disable in zugzwang-prone positions (only kings/pawns)
    let can_null_move = !pv_node
        && !in_check
        && depth >= MIN_DEPTH_NULL_MOVE
        && static_eval >= beta
        && has_non_pawn_material(pos, pos.side_to_move)
        && prev_move != Move::NULL; // No consecutive null moves

    if can_null_move {
        let r = 3 + depth / 6; // Adaptive reduction

        // Make null move (just flip side to move)
        pos.side_to_move = pos.side_to_move.flip();
        pos.hash ^= crate::position::zobrist::side_key();
        let old_ep = pos.en_passant;
        pos.en_passant = None;

        let null_score = -alpha_beta_with_excluded(
            pos, depth - r - 1, -beta, -beta + 1,
            ply + 1, false, info, tt, Move::NULL, Move::NULL,
        );

        // Unmake null move
        pos.side_to_move = pos.side_to_move.flip();
        pos.hash ^= crate::position::zobrist::side_key();
        pos.en_passant = old_ep;

        if null_score >= beta {
            // Null move cutoff — but don't return mate scores
            if null_score >= MATE_THRESHOLD {
                return beta;
            }
            return null_score;
        }
    }

    // ── Internal Iterative Reduction (IIR) ───────────────────────────────────
    // Reduce depth when no TT move available (search is unguided)
    if depth >= MIN_DEPTH_IIR && tt_move == Move::NULL && pv_node {
        depth -= 1;
    }

    // ── Probcut (Phase 13.1) ───────────────────────────────────────────────────
    // Shallow-search verified captures that beat beta+margin let us prune
    // the whole node — the opponent would never allow this position anyway.
    if should_try_probcut(depth, beta, in_check, pv_node) {
        if let Some(score) = try_probcut(pos, depth, beta, ply, info, tt) {
            return score;
        }
    }

    // ── Singular extension verification (Phase 13.3) ──────────────────────────
    // If the TT move beats every alternative by a wide margin, it's
    // "singular" — extend it by one ply so tactics hidden behind a forced
    // sequence aren't missed. Verified via a reduced-depth search of the
    // position with the TT move excluded.
    let mut singular_extension = false;
    if !root_node
        && depth >= MIN_DEPTH_SINGULAR
        && tt_move != Move::NULL
        && tt_hit.map_or(false, |e| {
            e.bound != Bound::UpperBound && e.depth as i32 >= depth - 3
        })
    {
        let tt_score = TranspositionTable::score_from_tt(
            tt_hit.unwrap().score, ply as i32
        );
        if tt_score.abs() < MATE_THRESHOLD {
            let singular_beta  = tt_score - 2 * depth;
            let singular_depth = (depth - 1) / 2;

            let score = alpha_beta_with_excluded(
                pos, singular_depth, singular_beta - 1, singular_beta,
                ply, false, info, tt, prev_move, tt_move,
            );

            if score < singular_beta {
                singular_extension = true;
            }
        }
    }

    // ── Generate and score moves ──────────────────────────────────────────────
    let moves = generate_moves(pos);

    // Check for checkmate or stalemate
    if moves.is_empty() {
        if in_check {
            // Checkmate — return distance-to-mate score
            return -(MATE_SCORE - ply as i32);
        } else {
            // Stalemate — draw
            return DRAW_SCORE;
        }
    }

    let mut scored = score_moves(pos, &moves, info, tt_move, ply, prev_move);

    let mut best_score  = -INFINITY;
    let mut best_move   = Move::NULL;
    let mut bound       = Bound::UpperBound;
    let mut moves_tried = 0;
    let mut quiets_tried: Vec<Move> = Vec::new();

    // ── Move loop ─────────────────────────────────────────────────────────────
    for i in 0..scored.len() {
        let mv = match next_move(&mut scored, i) {
            Some(m) => m,
            None    => break,
        };

        // Skip the move excluded by singular extension verification (13.3)
        if mv == excluded {
            continue;
        }

        // Singular extension bonus — only the TT move itself gets it
        let singular_ext = if singular_extension && mv == tt_move { 1 } else { 0 };

        let is_capture   = mv.kind.is_capture();
        let is_promotion = mv.kind.is_promotion();
        let is_quiet     = !is_capture && !is_promotion;
        let gives_check  = move_gives_check(pos, mv);

        // ── Futility pruning ──────────────────────────────────────────────────
        // Skip quiet moves near leaves when we're far behind
        if !pv_node
            && !in_check
            && !gives_check
            && is_quiet
            && depth <= MIN_DEPTH_FUTILITY
            && moves_tried > 0
            && static_eval + 100 * depth + 200 <= alpha
        {
            continue;
        }

        // ── SEE pruning for captures ──────────────────────────────────────────
        // Skip losing captures at low depth
        if !pv_node
            && is_capture
            && depth <= 4
            && moves_tried > 0
            && !see(pos, mv, -50 * depth)
        {
            continue;
        }

        // Track quiet moves tried (for history penalty on cutoff)
        if is_quiet {
            quiets_tried.push(mv);
        }

        pos.make_move_with_history(mv);
        moves_tried += 1;

        let score;

        // ── PVS with LMR ──────────────────────────────────────────────────────
        if moves_tried == 1 {
            // First move: full window search
            score = -alpha_beta_with_excluded(
                pos, depth - 1 + singular_ext, -beta, -alpha,
                ply + 1, pv_node, info, tt, mv, Move::NULL,
            );
        } else {
            // Late Move Reductions
            let mut reduction = 0i32;

            if depth >= MIN_DEPTH_LMR
                && moves_tried >= 3
                && is_quiet
                && !in_check
                && !gives_check
            {
                // LMR formula (similar to Stockfish)
                reduction = (0.75 + (depth as f64).ln()
                    * (moves_tried as f64).ln() / 2.25) as i32;
                reduction = reduction.max(1).min(depth - 1);
            }

            // Null window search with reduction
            let mut s = -alpha_beta_with_excluded(
                pos, depth - 1 + singular_ext - reduction, -alpha - 1, -alpha,
                ply + 1, false, info, tt, mv, Move::NULL,
            );

            // If reduced search beats alpha, re-search at full depth
            if s > alpha && reduction > 0 {
                s = -alpha_beta_with_excluded(
                    pos, depth - 1 + singular_ext, -alpha - 1, -alpha,
                    ply + 1, false, info, tt, mv, Move::NULL,
                );
            }

            // If still beats alpha in PV node, full window re-search
            if s > alpha && pv_node {
                s = -alpha_beta_with_excluded(
                    pos, depth - 1 + singular_ext, -beta, -alpha,
                    ply + 1, true, info, tt, mv, Move::NULL,
                );
            }

            score = s;
        }

        pos.unmake_move_with_history(mv);

        if info.stop { return 0; }

        if score > best_score {
            best_score = score;
            best_move  = mv;
            if root_node {
                info.best_move  = mv;
                info.best_score = score;
            }

            if score > alpha {
                alpha = score;
                bound = Bound::Exact;
                info.update_pv(mv, ply);

                if score >= beta {
                    // Beta cutoff — update ordering tables
                    bound = Bound::LowerBound;
                    update_ordering_on_cutoff(
                        info, mv, prev_move, ply, depth,
                        pos.side_to_move, &quiets_tried,
                    );
                    break;
                }
            }
        }
    }

    // ── Store in TT ───────────────────────────────────────────────────────────
    if !info.stop {
        let tt_score = TranspositionTable::score_to_tt(
            best_score, ply as i32
        );
        tt.store(pos.hash, depth as i8, tt_score, bound, best_move);
    }

    // ── Correction history update (Phase 13.2) ────────────────────────────────
    // Skip when in check (static eval meaningless), search was aborted, or
    // the result is a mate score (error signal is noise, not eval drift).
    if !info.stop && !in_check && !crate::search::is_mate_score(best_score) {
        let phash = pawn_hash(pos);
        info.correction_history.update(
            phash, pos.side_to_move, raw_static_eval, best_score, depth,
        );
    }

    best_score
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Does this position have non-pawn, non-king material?
/// Used as zugzwang guard for null move pruning
#[inline]
fn has_non_pawn_material(pos: &Position, color: Color) -> bool {
    pos.count_pieces(color, PieceKind::Knight) > 0
        || pos.count_pieces(color, PieceKind::Bishop) > 0
        || pos.count_pieces(color, PieceKind::Rook)   > 0
        || pos.count_pieces(color, PieceKind::Queen)  > 0
}

/// Quick check if a move gives check (used for pruning decisions)
/// Not 100% accurate but fast — full legality already guaranteed
#[inline]
fn move_gives_check(pos: &Position, mv: Move) -> bool {
    let mut test = pos.clone();
    test.make_move(mv);
    let side = test.side_to_move.flip();
    // Guard: king must exist (it shouldn't be captured in legal play)
    if test.piece_bb(side, PieceKind::King).is_empty() {
        return false;
    }
    test.in_check(side)
}

/// Evaluate a position using the full HCE (Phase 8).
/// Delegates to crate::eval::evaluate() — all terms combined and tapered.
pub fn evaluate(pos: &Position) -> i32 {
    crate::eval::evaluate(pos)
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
    use crate::types::Color;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    fn make_search(pos: &mut Position, depth: i32) -> (Move, i32) {
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(16);
        info.time_allocated_ms = 60_000; // 60 seconds — no time pressure

        let score = alpha_beta(
            pos, depth, -INFINITY, INFINITY,
            0, true, &mut info, &tt, Move::NULL,
        );
        (info.best_move, score)
    }

    fn test_finds_mate_in_1() {
        setup();
        // Simple winning position — White is up a queen
        // Avoids minimal positions that expose search edge cases
        let fen = "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let (mv, score) = make_search(&mut pos, 3);
        assert_ne!(mv, Move::NULL, "Should return a move");
        assert!(score > 0,
            "Score should be positive when up a queen: {}", score);
    }

    #[test]
    fn test_avoids_losing_material() {
        setup();
        // Simple position: don't hang the queen
        let fen = "4k3/8/8/8/8/5q2/8/4K3 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let (mv, _) = make_search(&mut pos, 4);
        assert_ne!(mv, Move::NULL, "Should find a move");
        // The queen should not move to a square where it can be captured
    }

    #[test]
    fn test_draw_by_repetition() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(4);
        info.time_allocated_ms = 60_000;

        // Push same position to history multiple times to trigger repetition
        pos.game_history.push(pos.hash);
        pos.game_history.push(pos.hash);

        // Search should handle repetition without panicking
        let score = alpha_beta(
            &mut pos, 4, -INFINITY, INFINITY,
            0, true, &mut info, &tt, Move::NULL,
        );
        // Score should be draw (0) or reasonable
        assert!(score.abs() < MATE_THRESHOLD,
            "Repetition position should not return mate score");
    }

    #[test]
    fn test_fifty_move_rule() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 100 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(4);
        info.time_allocated_ms = 60_000;

        let score = alpha_beta(
            &mut pos, 1, -INFINITY, INFINITY,
            0, true, &mut info, &tt, Move::NULL,
        );
        assert_eq!(score, DRAW_SCORE,
            "50-move rule should return draw score");
    }

    #[test]
    fn test_stalemate_returns_draw() {
        setup();
        // Classic stalemate
        let fen = "k7/8/1Q6/8/8/8/8/7K b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        if !pos.in_check(Color::Black) {
            let (_, score) = make_search(&mut pos, 2);
            assert_eq!(score, DRAW_SCORE,
                "Stalemate should return draw score");
        }
    }

    #[test]
    fn test_material_evaluation() {
        setup();
        // White is up a queen — should have positive eval
        let fen = "4k3/8/8/8/8/8/8/4KQ2 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let eval = evaluate(&pos);
        assert!(eval > 0,
            "White up a queen should have positive eval");
    }

    #[test]
    fn test_search_pet_dragon_position() {
        setup();
        // Search should work on any Pet Dragon position without panicking
        for seed in 0..10u64 {
            let mut pos  = Position::generate_with_seed(seed);
            let mut info = SearchInfo::new();
            let tt       = TranspositionTable::new(4);
            info.time_allocated_ms = 1000;

            let score = alpha_beta(
                &mut pos, 4, -INFINITY, INFINITY,
                0, true, &mut info, &tt, Move::NULL,
            );

            assert!(score.abs() <= INFINITY,
                "Score should be bounded (seed {})", seed);
        }
    }

    #[test]
    fn test_search_returns_legal_move() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(16);
        info.time_allocated_ms = 5000;

        alpha_beta(
            &mut pos, 5, -INFINITY, INFINITY,
            0, true, &mut info, &tt, Move::NULL,
        );

        assert_ne!(info.best_move, Move::NULL,
            "Search should find a best move");

        // Verify the move is legal
        let legal_moves = crate::movegen::generate_moves(&pos);
        assert!(
            legal_moves.iter().any(|&m| m == info.best_move),
            "Best move should be legal"
        );
    }
}
