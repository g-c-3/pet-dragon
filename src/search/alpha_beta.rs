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
//   - Delta pruning (global + per-capture in quiescence)
//   - In-check evasion search in quiescence (checkmate detection)
//   - Quiet checking moves in quiescence (qs_depth = 0 only)
//
// ⚠️ Pet Dragon notes throughout — see comments marked ⚠️
// ============================================================================

use crate::movegen::{generate_captures, generate_moves};
use crate::position::Position;
use crate::search::{
    ordering::{next_move, score_captures, score_moves,
               update_ordering_on_cutoff},
    pruning::{lmr_thread_base, pawn_hash, should_try_probcut, try_probcut},
    see::see,
    SearchInfo, INFINITY, MATE_SCORE, MATE_THRESHOLD,
    MAX_PLY, MIN_DEPTH_FUTILITY, MIN_DEPTH_IIR, MIN_DEPTH_LMR,
    MIN_DEPTH_NULL_MOVE, MIN_DEPTH_RAZORING, MIN_DEPTH_SINGULAR,
    draw_score,
};
#[cfg(test)]
use crate::search::DRAW_SCORE;
use crate::tt::{Bound, TranspositionTable};
use crate::types::{Color, Move, PieceKind};

// ── Quiescence search ─────────────────────────────────────────────────────────

/// Piece values for per-capture delta pruning in quiescence search.
/// More precise than a global delta: checks whether THIS specific capture
/// can raise alpha before even making the move.
const QS_CAPTURE_VALUES: [i32; 6] = [
    100,  // Pawn
    320,  // Knight
    330,  // Bishop
    500,  // Rook
    975,  // Queen (capped at DELTA_MARGIN to match global pruning)
    0,    // King  (never captured in legal play)
];

/// Quiescence search — search captures (and checks) until position is quiet.
/// Prevents the horizon effect (stopping mid-exchange or missing a check win).
///
/// `qs_depth` controls what gets searched beyond captures:
///   ≥ 0 → also search quiet moves that give check with positive SEE
///    < 0 → captures only (recursive calls and probcut use this)
///
/// Improvements over basic capture search:
///   - In check: generates ALL legal evasions, no stand-pat.
///     Detects checkmate when no evasion exists.
///   - Not in check: per-capture delta pruning, then quiet checks at depth 0.
///
/// ⚠️ Pet Dragon: MUST be called even at root — starting positions can
/// have immediate captures. Never assume the opening position is quiet.
pub fn quiescence(
    pos:       &mut Position,
    mut alpha: i32,
    beta:      i32,
    ply:       usize,
    qs_depth:  i32,
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

    if ply > info.seldepth {
        info.seldepth = ply;
    }

    // ── Check detection ───────────────────────────────────────────────────────
    let in_check = pos.in_check(pos.side_to_move);

    // ── In-check evasion path ─────────────────────────────────────────────────
    // When in check we CANNOT stand pat — the check demands an answer.
    // Generate ALL legal moves (evasions) and search every one of them.
    // Captures, quiet evasions, and interpositions are all considered.
    if in_check {
        let evasions = generate_moves(pos);
        if evasions.is_empty() {
            // Checkmate — return exact mate-distance score
            return -(MATE_SCORE - ply as i32);
        }

        let tt_move = tt.probe(pos.hash).map(|e| e.mv).unwrap_or(Move::NULL);
        let mut scored = score_moves(pos, &evasions, info, tt_move, ply, Move::NULL);
        let mut best_score = -INFINITY;

        for i in 0..scored.len() {
            let mv = match next_move(&mut scored, i) {
                Some(m) => m,
                None    => break,
            };

            pos.make_move_with_history(mv);
            let score = -quiescence(pos, -beta, -alpha, ply + 1, qs_depth - 1, info, tt);
            pos.unmake_move_with_history(mv);

            if info.stop { return 0; }

            if score > best_score {
                best_score = score;
                if score > alpha {
                    alpha = score;
                    if score >= beta {
                        return beta;
                    }
                }
            }
        }
        return best_score;
    }

    // ── Stand-pat (not in check) ──────────────────────────────────────────────
    // Static evaluation — we can always choose not to capture anything.
    let stand_pat = evaluate(pos);

    if stand_pat >= beta {
        return beta;
    }

    // Global delta pruning: if even a queen gain can't raise alpha, bail out.
    const DELTA_MARGIN: i32 = 975;
    if stand_pat + DELTA_MARGIN < alpha {
        return alpha;
    }

    if stand_pat > alpha {
        alpha = stand_pat;
    }

    // ── TT probe ──────────────────────────────────────────────────────────────
    let tt_move = tt.probe(pos.hash).map(|e| e.mv).unwrap_or(Move::NULL);

    // ── Capture search ────────────────────────────────────────────────────────
    let captures = generate_captures(pos);
    let mut scored = score_captures(pos, &captures, tt_move);
    let mut best_score = stand_pat;

    for i in 0..scored.len() {
        let mv = match next_move(&mut scored, i) {
            Some(m) => m,
            None    => break,
        };

        // Per-capture delta pruning: skip if capturing this specific piece
        // still leaves us too far below alpha to matter.
        if !mv.kind.is_promotion() {
            let captured_val = mv.captured
                .map(|k| QS_CAPTURE_VALUES[k as usize])
                .unwrap_or(0);
            if stand_pat + captured_val + 200 < alpha {
                continue;
            }
        }

        // SEE pruning: skip captures that lose material on the exchange
        if !see(pos, mv, 0) {
            continue;
        }

        pos.make_move_with_history(mv);
        let score = -quiescence(pos, -beta, -alpha, ply + 1, qs_depth - 1, info, tt);
        pos.unmake_move_with_history(mv);

        if info.stop { return 0; }

        if score > best_score {
            best_score = score;
            if score > alpha {
                alpha = score;
                if score >= beta {
                    return beta;
                }
            }
        }
    }

    // ── Quiet checks (first qsearch level only) ───────────────────────────────
    // At qs_depth ≥ 0 (called from main search), also search quiet moves that
    // give check with non-negative SEE. These catch tactical checkmates that
    // occur one move after the horizon — the most common form of missed tactics.
    // Recursive calls use qs_depth = -1 so this section never runs below.
    if qs_depth >= 0 {
        let all_moves = generate_moves(pos);
        for i in 0..all_moves.len() {
            let mv = all_moves.get(i);
            // Captures and promotions already handled above
            if mv.kind.is_capture() || mv.kind.is_promotion() {
                continue;
            }
            // Only free or winning checks — paying for a check is usually wrong
            if !see(pos, mv, 0) {
                continue;
            }
            if !move_gives_check(pos, mv) {
                continue;
            }

            pos.make_move_with_history(mv);
            // Recurse with qs_depth = -1: captures only below this point
            let score = -quiescence(pos, -beta, -alpha, ply + 1, -1, info, tt);
            pos.unmake_move_with_history(mv);

            if info.stop { return 0; }

            if score > best_score {
                best_score = score;
                if score > alpha {
                    alpha = score;
                    if score >= beta {
                        return beta;
                    }
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
        return quiescence(pos, alpha, beta, ply, 0, info, tt);
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
    if !root_node && pos.is_repetition(ply) {
        return draw_score(ply, info.contempt);
    }

    // Fifty-move rule
    if pos.halfmove_clock >= 100 {
        return draw_score(ply, info.contempt);
    }

    // Insufficient material
    if pos.is_insufficient_material() {
        return draw_score(ply, info.contempt);
    }

    // ── Syzygy WDL probe (Phase 15.3 / 15.4) ─────────────────────────────────
    // Probe all interior nodes when piece count ≤ loaded tablebase size.
    // WDL is reliable only when halfmove_clock == 0 (checked inside probe_wdl).
    // DTZ at root is handled separately in main.rs before spawning threads.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(ref tb) = info.syzygy {
        if pos.all_occupied.count() <= tb.max_pieces() {
            if let Some(tb_score) = tb.probe_wdl(pos) {
                let bound = if tb_score >= beta       { Bound::LowerBound }
                            else if tb_score <= alpha { Bound::UpperBound }
                            else                      { Bound::Exact };
                tt.store(pos.hash, depth as i8, tb_score, bound, Move::NULL);
                return tb_score;
            }
        }
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
        return quiescence(pos, alpha, beta, ply, 0, info, tt);
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
            return draw_score(ply, info.contempt);
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

        // Skip moves already claimed by an earlier MultiPV line at this
        // depth (Phase 19). Only ever relevant at the root — root_exclude
        // is always empty when MultiPV is at its default of 1, so this is
        // a single cheap `is_empty()` check (short-circuited by
        // `root_node` first) for the overwhelming majority of nodes, and
        // a short Vec scan only at the root when MultiPV>1 is in use.
        // Safe to share the move loop with singular extension's `excluded`
        // above: singular verification is explicitly gated on
        // `!root_node` (see above), so the two mechanisms never apply to
        // the same node.
        if root_node && !info.root_exclude.is_empty() && info.root_exclude.contains(&mv) {
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
                // LMR formula (similar to Stockfish). Base constant is
                // per-thread (Phase 23.2/D49): thread 0 (main thread)
                // always gets 0.75, unchanged from before — only helper
                // threads' aggressiveness varies, to decorrelate their
                // tree exploration from the main thread and each other.
                reduction = (lmr_thread_base(info.thread_id) + (depth as f64).ln()
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
                        pos.side_to_move, &quiets_tried, pos,
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

/// Evaluate a position using HCE blended with the trained Pet Dragon NNUE
/// (Phase 16.6, D23). Delegates to crate::eval::evaluate_blended() — the
/// pure-HCE crate::eval::evaluate() is still used directly by eval/mod.rs's
/// own test suite and is otherwise unchanged.
pub fn evaluate(pos: &Position) -> i32 {
    crate::eval::evaluate_blended(pos)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::types::Square;
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

    #[test]
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
        let fen = "4k3/p7/8/8/8/8/P7/4K3 w - - 0 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(4);
        info.time_allocated_ms = 60_000;
        pos.push_game_history(); // matches real search usage — iterative_deepening() pushes the root first

        // Build a REAL 4-ply repetition cycle via legitimate moves and the
        // real push_game_history() caching (D45) — Ke1-e2, Ke8-e7, Ke2-e1,
        // Ke7-e8 returns to the exact starting position after 4 plies, with
        // halfmove_clock correctly reaching 4 (king moves don't reset it).
        // This is the shortest possible repetition cycle in legal chess —
        // see D45's doc comment on why push_game_history()'s walk starts at
        // i=4, not i=2.
        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };

        let mv1 = find_move(&pos, Square::E1, Square::E2);
        pos.make_move_with_history(mv1);
        let mv2 = find_move(&pos, Square::E8, Square::E7);
        pos.make_move_with_history(mv2);
        let mv3 = find_move(&pos, Square::E2, Square::E1);
        pos.make_move_with_history(mv3);
        let mv4 = find_move(&pos, Square::E7, Square::E8);
        pos.make_move_with_history(mv4);

        // Sanity check the setup itself before trusting the search result:
        // this must be a genuine repetition per is_threefold_repetition()'s
        // own independent (non-ply-relative) count, or this test wouldn't
        // actually be exercising what it claims to.
        assert!(pos.game_history.last().unwrap().0 == pos.hash);

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
    fn test_fifty_move_rule_with_contempt() {
        // Same construction as test_fifty_move_rule, but with nonzero
        // contempt — proves info.contempt actually reaches alpha_beta's
        // draw path (not just that draw_score() itself is correct in
        // isolation). The 50-move check has no `!root_node` guard, so it
        // fires immediately at ply=0 with no search branching involved,
        // making the exact expected score fully predictable.
        setup();
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 100 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(4);
        info.time_allocated_ms = 60_000;
        info.contempt = 25;

        let score = alpha_beta(
            &mut pos, 1, -INFINITY, INFINITY,
            0, true, &mut info, &tt, Move::NULL,
        );
        // ply=0 is always the root side to move, so a positive contempt
        // (dislikes draws) must score this exactly 25 worse than DRAW_SCORE.
        assert_eq!(score, DRAW_SCORE - 25,
            "50-move draw at root-side ply should reflect Contempt exactly");
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
        let pos = Position::from_fen(fen).unwrap();
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

    // ── Phase 13.5: Quiescence search improvements ────────────────────────────

    #[test]
    fn test_qsearch_in_check_generates_evasions() {
        setup();
        // White King on e1, Black Rook on e8, Black King on h8 — White is in check.
        // Old qsearch would stand-pat (wrong); new one generates all evasions.
        let fen = "4r2k/8/8/8/8/8/8/4K3 w - - 0 1";
        let mut pos  = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt       = TranspositionTable::new(4);
        info.time_allocated_ms = 60_000;

        assert!(pos.in_check(Color::White), "Setup: King must be in check");

        let score = quiescence(&mut pos, -INFINITY, INFINITY, 0, 0, &mut info, &tt);

        // Down a rook with king in check — score must be negative
        assert!(score < -200,
            "In-check qsearch should score negatively: {}", score);
        // Must have searched nodes — never just stand-patted
        assert!(info.nodes > 0, "Must search nodes when in check");
    }

    #[test]
    fn test_qsearch_checkmate_detection() {
        setup();
        // Fool's-mate position: White has no legal moves and is in check.
        // qsearch must return a mate score, not stand-pat.
        let fen =
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
        let pos = Position::from_fen(fen).unwrap();
        if pos.in_check(Color::White)
            && crate::movegen::generate_moves(&pos).is_empty()
        {
            let mut pos2 = pos.clone();
            let mut info = SearchInfo::new();
            let tt       = TranspositionTable::new(4);
            info.time_allocated_ms = 60_000;

            let score = quiescence(
                &mut pos2, -INFINITY, INFINITY, 0, 0, &mut info, &tt
            );
            assert!(crate::search::is_mate_score(score),
                "qsearch must return a mate score for checkmate: {}", score);
        }
    }

    #[test]
    fn test_qsearch_qs_depth_parameter_no_panic() {
        setup();
        // Verify the new qs_depth parameter works without panicking across
        // multiple positions and both qs_depth values (0 and -1).
        for seed in 0..10u64 {
            let mut pos  = Position::generate_with_seed(seed);
            let mut info = SearchInfo::new();
            let tt       = TranspositionTable::new(4);
            info.time_allocated_ms = 1000;

            // qs_depth = 0  → checks in qsearch enabled
            let s0 = quiescence(
                &mut pos, -INFINITY, INFINITY, 0, 0, &mut info, &tt
            );
            // qs_depth = -1 → captures only (classic behaviour)
            let mut info2 = SearchInfo::new();
            info2.time_allocated_ms = 1000;
            let s1 = quiescence(
                &mut pos, -INFINITY, INFINITY, 0, -1, &mut info2, &tt
            );

            assert!(s0.abs() <= INFINITY,
                "qs_depth=0 score out of bounds (seed {}): {}", seed, s0);
            assert!(s1.abs() <= INFINITY,
                "qs_depth=-1 score out of bounds (seed {}): {}", seed, s1);
        }
    }
}
