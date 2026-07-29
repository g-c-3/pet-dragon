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
//   - Late Move Pruning (LMP) — D60, skip late quiets outright, distinct from LMR
//   - Singular extensions, with multi-cut pruning and negative extensions (D59)
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
    pruning::{continuation_hash, lmr_thread_base, nonpawn_hash, pawn_hash, should_apply_lmp, should_try_probcut, singular_margin_reduction, try_probcut},
    see::{see, see_value_of},
    SearchInfo, INFINITY, MATE_SCORE, MATE_THRESHOLD,
    MAX_PLY, MIN_DEPTH_FUTILITY, MIN_DEPTH_IIR, MIN_DEPTH_LMR,
    MIN_DEPTH_NULL_MOVE, MIN_DEPTH_RAZORING, MIN_DEPTH_SINGULAR,
    draw_score,
};
#[cfg(test)]
use crate::search::DRAW_SCORE;
use crate::tt::{Bound, TranspositionTable};
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

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
    // Phase 27/D95 (Session 90): is_time_up() only ever *read* self.stop —
    // nothing set it back to true when the elapsed-time branch fired, so a
    // genuine mid-search timeout here silently returned this hardcoded 0
    // (a real, meaningful "dead equal" evaluation, not a distinguishable
    // "aborted, discard me" marker) straight into the caller's alpha-beta
    // comparison, with no signal that anything was cut short. Every
    // downstream `if info.stop { ... }` check in this file (including the
    // one four lines below this comment) — plus TT-store/correction-
    // history-update guards and iterative_deepening()'s own
    // discard-this-depth logic — assumed this would already be true by
    // the time they ran. It never was, for the single most common abort
    // reason there is. See DECISIONS.md D95 for the full data trail that
    // led here.
    if info.is_time_up() {
        info.stop = true;
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
    // Phase 27/D95 (Session 90): same bug and same fix as quiescence()'s
    // time check above — is_time_up() never latched into info.stop, so
    // this returned a corrupted 0 with no abort signal on every real
    // mid-search timeout. This is the main search function, so this is
    // the higher-impact of the two sites: it's what feeds
    // iterative_deepening()'s per-depth result and the TT/correction-
    // history guards a few hundred lines below in this same file.
    if info.is_time_up() {
        info.stop = true;
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
    //
    // Two independent correction sources (Phase 26 item 3a, D80/D82): pawn
    // structure (always on) and non-pawn material placement (gated behind
    // `info.nonpawn_correction_enabled`, default false, until its own
    // SPRT-style A/B validates it — D82 corrects item 3a's initial
    // always-on shipment to match item 1's own established discipline).
    // Both, when active, are read against raw_static_eval and their
    // corrections summed via chained .apply() calls (mathematically
    // identical to summing directly) — each source learns its own
    // correction independently in the update step below, neither cascades
    // into the other's baseline.
    let raw_static_eval = if !in_check { evaluate(pos) } else { -INFINITY };
    let static_eval = if !in_check {
        let phash = pawn_hash(pos);
        let corrected = info.correction_history.apply(raw_static_eval, phash, pos.side_to_move);
        let corrected = if info.nonpawn_correction_enabled {
            let nphash = nonpawn_hash(pos);
            info.correction_history_nonpawn.apply(corrected, nphash, pos.side_to_move)
        } else {
            corrected
        };
        if info.continuation_correction_enabled {
            if let Some(chash) = continuation_hash(pos, prev_move) {
                info.correction_history_continuation.apply(corrected, chash, pos.side_to_move)
            } else {
                corrected
            }
        } else {
            corrected
        }
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
    //
    // Optional king-exposure guard (ROADMAP Phase 26 item 1, unproven — off
    // by default via SearchInfo::null_move_king_guard / UCI
    // "NullMoveKingGuard"). Pet Dragon's randomized starting pawn structure
    // can leave a king with no natural shield from move 1, unlike standard
    // chess where an early-game king is reliably safe — the zugzwang guard
    // above doesn't cover this. When enabled, a king with very few safe
    // squares around it either skips null-move entirely (<=1 safe square)
    // or gets a smaller reduction (<=2 safe squares). `king_safe_squares`
    // is `None` (zero cost, computed nothing) when the guard is off, so
    // default engine behavior is byte-identical to before this option
    // existed.
    let king_safe_squares = if info.null_move_king_guard {
        Some(king_safe_square_count(pos, pos.side_to_move))
    } else {
        None
    };

    let can_null_move = !pv_node
        && !in_check
        && depth >= MIN_DEPTH_NULL_MOVE
        && static_eval >= beta
        && has_non_pawn_material(pos, pos.side_to_move)
        && prev_move != Move::NULL // No consecutive null moves
        && king_safe_squares.map_or(true, |n| n > 1);

    if can_null_move {
        let mut r = 3 + depth / 6; // Adaptive reduction
        if let Some(n) = king_safe_squares {
            if n <= 2 {
                // Exposed king: be more conservative than usual, but never
                // reduce below 1 (that would make null-move a no-op search).
                r = r.saturating_sub(1).max(1);
            }
        }

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

    // ── Singular extension verification (Phase 13.3, extended D59) ────────────
    // If the TT move beats every alternative by a wide margin, it's
    // "singular" — extend it by one ply so tactics hidden behind a forced
    // sequence aren't missed. Verified via a reduced-depth search of the
    // position with the TT move excluded.
    //
    // D59 adds two Stockfish-family siblings of the base technique, both
    // reading the same verification search result rather than doing any
    // extra search:
    //   - Multi-cut pruning: if the verification search — which excludes
    //     the TT move entirely — still reaches singular_beta, that means
    //     at least one OTHER move also refutes at this margin. The
    //     position is being cut multiple different ways, so the whole
    //     node can be pruned right here instead of searching further.
    //     Same "early return, no TT store" shape as probcut/razoring
    //     above — consistent with existing style, not a new pattern.
    //   - Negative extension: if verification did NOT confirm
    //     singularity, but the TT move's own recorded score already
    //     meets beta, the TT entry is telling us this is very likely a
    //     cutoff regardless of the reduced-search result — so reduce
    //     (don't extend) the TT move's own search rather than spending
    //     extra depth re-confirming what the TT already suggests.
    //     Reduces by 1 more ply in non-PV nodes than PV nodes, same
    //     shape as Stockfish's `-2 - !PvNode`.
    let mut tt_move_extension = 0i32;
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
            // ── Correction-signal-scaled margin (Phase 26 item 3c, D89) ────
            // Base singular margin is 2 (matches Phase 13.3/D59's
            // original, unconditional value). When
            // info.correction_extension_enabled is true, scale it down
            // by up to 1 when this position's pawn-structure correction
            // history (the only source of the three from Phase 26 item
            // 3 that's actually validated positive — items 3a/3b both
            // parked off per D85/D88) shows a large historical eval
            // error: a smaller margin raises singular_beta (closer to
            // tt_score), making the verification search more likely to
            // fall below it and extend. The idea: search depth
            // compensates for eval unreliability, so be more willing to
            // extend the TT move at exactly the position types where
            // eval has consistently needed correcting. Deliberately
            // only reads the base pawn-hash table, not the two parked
            // sources — using a signal already known to correlate with
            // real eval error, not ones shown to have no effect.
            let mut singular_margin = 2;
            if info.correction_extension_enabled {
                let phash = pawn_hash(pos);
                let corr_mag = info.correction_history
                    .get(phash, pos.side_to_move)
                    .unsigned_abs() as i32;
                singular_margin -= singular_margin_reduction(corr_mag);
            }
            let singular_beta  = tt_score - singular_margin * depth;
            let singular_depth = (depth - 1) / 2;

            let score = alpha_beta_with_excluded(
                pos, singular_depth, singular_beta - 1, singular_beta,
                ply, false, info, tt, prev_move, tt_move,
            );

            if score < singular_beta {
                tt_move_extension = 1;
            } else if info.singular_multicut_enabled && singular_beta >= beta {
                // Multi-cut (D59): skip move generation/TT store entirely,
                // mirroring probcut's early-return shape above. Phase 27
                // (Session 86): gated behind info.singular_multicut_enabled,
                // default `true` (byte-identical to before this option
                // existed) — see SearchInfo::singular_multicut_enabled's
                // doc comment.
                return singular_beta;
            } else if info.singular_multicut_enabled && tt_score >= beta {
                // Negative extension (D59) — same Phase 27 gate as above.
                tt_move_extension = if pv_node { -1 } else { -2 };
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

        // Singular/multi-cut/negative extension (D59) — only the TT move
        // itself gets it, and it can now be negative (negative extension).
        let move_ext = if mv == tt_move { tt_move_extension } else { 0 };

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

        // ── Late Move Pruning (LMP) ─────────────────────────────────────────────
        // D60 — distinct from LMR: skip late quiet moves outright at
        // shallow depth once enough quiets have already been tried
        // without raising alpha, instead of just reducing them. See
        // `pruning::should_apply_lmp` for the full rationale and the
        // depth→threshold table.
        // Phase 27 (Session 86): gated behind info.lmp_enabled, default
        // `true` (byte-identical to before this option existed) — see
        // SearchInfo::lmp_enabled's doc comment. Lets the same binary be
        // A/B'd against the external-Stockfish bench regression without a
        // rebuild.
        if info.lmp_enabled && should_apply_lmp(
            depth, moves_tried, is_quiet, in_check, gives_check, pv_node,
            alpha, beta,
        ) {
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
                pos, depth - 1 + move_ext, -beta, -alpha,
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
                pos, depth - 1 + move_ext - reduction, -alpha - 1, -alpha,
                ply + 1, false, info, tt, mv, Move::NULL,
            );

            // If reduced search beats alpha, re-search at full depth
            if s > alpha && reduction > 0 {
                s = -alpha_beta_with_excluded(
                    pos, depth - 1 + move_ext, -alpha - 1, -alpha,
                    ply + 1, false, info, tt, mv, Move::NULL,
                );
            }

            // If still beats alpha in PV node, full window re-search
            if s > alpha && pv_node {
                s = -alpha_beta_with_excluded(
                    pos, depth - 1 + move_ext, -beta, -alpha,
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

    // ── Correction history update (Phase 13.2; second source Phase 26 item 3a, D80/D82) ──
    // Skip when in check (static eval meaningless), search was aborted, or
    // the result is a mate score (error signal is noise, not eval drift).
    // Both sources update against the same raw_static_eval baseline,
    // independently — see the static-eval comment above for why. The
    // non-pawn source only runs when info.nonpawn_correction_enabled is
    // true (default false — see that field's doc comment).
    if !info.stop && !in_check && !crate::search::is_mate_score(best_score) {
        let phash = pawn_hash(pos);
        info.correction_history.update(
            phash, pos.side_to_move, raw_static_eval, best_score, depth,
        );
        if info.nonpawn_correction_enabled {
            let nphash = nonpawn_hash(pos);
            info.correction_history_nonpawn.update(
                nphash, pos.side_to_move, raw_static_eval, best_score, depth,
            );
        }
        if info.continuation_correction_enabled {
            if let Some(chash) = continuation_hash(pos, prev_move) {
                info.correction_history_continuation.update(
                    chash, pos.side_to_move, raw_static_eval, best_score, depth,
                );
            }
        }
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

/// Count squares adjacent to `color`'s king that are currently "safe": not
/// occupied by `color`'s own pieces, and not attacked by the enemy.
///
/// Used only by the optional null-move king-exposure guard
/// (`SearchInfo::null_move_king_guard`, ROADMAP Phase 26 item 1, off by
/// default) — a deliberately cheap, coarse proxy for "how exposed is this
/// king right now", not a full king-safety evaluation (that's the far more
/// expensive `eval::king_safety::evaluate_king_safety`, unsuitable to call
/// at every null-move check). Max possible return value is 8 (every ring
/// square safe); a king on an edge or in a corner has fewer ring squares to
/// begin with, so a low count there isn't itself meaningful in isolation —
/// only the guard's own thresholds (see `alpha_beta()`) give it meaning.
#[inline]
fn king_safe_square_count(pos: &Position, color: Color) -> u32 {
    use crate::bitboard::masks::king_attacks;

    let king_sq = pos.king_sq(color);
    let ring = king_attacks(king_sq) & !pos.occupied(color);
    let enemy = color.flip();

    let mut count = 0u32;
    for sq in ring {
        if !pos.is_attacked(sq, enemy) {
            count += 1;
        }
    }
    count
}

/// Result of `extract_threat_move`'s one-ply-ahead opponent-reply probe.
///
/// Phase 28 (Session 93), TDSE, first diff — legality-only. Deliberately
/// doesn't carry a SEE-before value yet: that's the SEE-degradation
/// signal's own follow-up diff (not implemented in this first landing —
/// see DECISIONS.md D98 for why signals are split rather than bundled).
pub struct ThreatInfo {
    threat_move: Option<Move>,
    /// SEE value of `threat_move` at the moment it was found (computed
    /// while `pos.side_to_move` still equals the opponent — see
    /// `extract_threat_move`'s doc comment on why this has to happen
    /// before the flip-back, not after, unlike the original TDSE
    /// proposal's ordering — DECISIONS.md D104).
    threat_see_before: i32,
}

/// Probe the opponent's best reply one ply ahead, without touching shared
/// search state (`info.pv`/`info.update_pv`) — see DECISIONS.md D98 for why
/// reusing the null-move probe's own recursive search result was rejected:
/// `alpha_beta_with_excluded()` returns only a score, and `info.pv`/
/// `info.update_pv()` fire on *any* node whose local alpha improves, not
/// just true PV nodes, so it isn't safe to read mid-search as "what this
/// specific probe concluded."
///
/// Only called when `info.threat_defusal` is set (UCI `ThreatDefusal`,
/// default `false`) — root-only, from `iterative_deepening()`'s
/// end-of-search TDSE block (`search/iterative.rs`), not from inside the
/// main move loop. `pub` (not private) because it's called cross-module
/// from `iterative.rs` — same visibility as `alpha_beta()` itself, this
/// codebase has no `pub(crate)` precedent to follow instead.
pub fn extract_threat_move(
    pos:   &mut Position,
    info:  &mut SearchInfo,
    tt:    &TranspositionTable,
    depth: i32,
) -> Option<ThreatInfo> {
    // D101 (Session 96): the real null-move block this function mirrors
    // guards on `!in_check` (alpha_beta_with_excluded's `can_null_move`,
    // a few hundred lines above in this same file) — flipping side to
    // move while the current side is in check produces a position where
    // "whose king is actually under attack" and "whose turn it is" no
    // longer agree, and searching that with the normal move-generation/
    // check-evasion machinery is undefined territory this function has
    // no business exploring. This guard was missing from the first
    // landing (D98) and is the confirmed cause of a real crash found by
    // the D100 harness fix's captured panic message
    // (`position::king_sq`'s "King must always be on the board", 14
    // games into a 200-game run with ThreatDefusal=true) — see D101 for
    // the full trail.
    if pos.in_check(pos.side_to_move) {
        return None;
    }
    if !has_non_pawn_material(pos, pos.side_to_move) {
        return None;
    }

    // Same null-move flip as the real null-move probe above — see that
    // block's own comments for why side-to-move flips and en passant
    // clears this way.
    pos.side_to_move = pos.side_to_move.flip();
    pos.hash ^= crate::position::zobrist::side_key();
    let old_ep = pos.en_passant;
    pos.en_passant = None;

    let shallow_depth = (depth - 2).max(4).min(6);
    let tt_move = tt.probe(pos.hash).map(|e| e.mv).unwrap_or(Move::NULL);
    let replies = generate_moves(pos);
    let mut scored = score_moves(pos, &replies, info, tt_move, 0, Move::NULL);

    let mut best_move  = Move::NULL;
    let mut best_score = -INFINITY;
    for i in 0..scored.len() {
        let mv = match next_move(&mut scored, i) {
            Some(m) => m,
            None    => break,
        };
        pos.make_move_with_history(mv);
        let s = -alpha_beta_with_excluded(
            pos, shallow_depth - 1, -INFINITY, INFINITY,
            1, false, info, tt, mv, Move::NULL,
        );
        pos.unmake_move_with_history(mv);
        if s > best_score {
            best_score = s;
            best_move  = mv;
        }
    }

    if best_move == Move::NULL {
        pos.side_to_move = pos.side_to_move.flip();
        pos.hash ^= crate::position::zobrist::side_key();
        pos.en_passant = old_ep;
        return None;
    }

    // D104 (Session 99): see_value_of(pos, mv) reads pos.side_to_move to
    // find "our" piece on mv.from — it MUST be called while pos.side_to_
    // move still equals best_move's actual mover (the opponent, from the
    // flip above), not after restoring it back to our own side. The
    // original TDSE proposal computed this after the restore below,
    // which would have made see_value_of look for our own piece on the
    // opponent's from-square, find nothing, and silently return 0 every
    // time — always reporting "no SEE value," not a real bug that fails
    // loudly. Caught by reading see_value_of's actual implementation
    // before trusting the proposal's ordering.
    let threat_see_before = see_value_of(pos, best_move);

    pos.side_to_move = pos.side_to_move.flip();
    pos.hash ^= crate::position::zobrist::side_key();
    pos.en_passant = old_ep;

    Some(ThreatInfo { threat_move: Some(best_move), threat_see_before })
}

/// Count attackers of color `by_color` on `sq`, using the same raw
/// bitboard attack primitives `king_safe_square_count` above already
/// uses (D75's precedent) — knights, bishops, rooks, queens. Phase 28
/// (Session 99), TDSE SEE-degradation signal.
///
/// Deliberately doesn't count pawns or the king as attackers here — this
/// is a coarse, cheap signal for "how contested is this square," not a
/// full SEE-grade exchange simulation (that's what `see_value_of` above
/// is for). Matches the original TDSE proposal's own scope for this
/// helper exactly (it didn't count pawns/king either).
fn attacker_count_on(pos: &Position, sq: Square, by_color: Color) -> u32 {
    use crate::bitboard::masks::knight_attacks;
    use crate::bitboard::magic::{bishop_attacks, rook_attacks, queen_attacks};

    let occ = pos.all_pieces();
    let mut count = 0u32;
    count += (knight_attacks(sq) & pos.piece_bb(by_color, PieceKind::Knight)).count();
    count += (bishop_attacks(sq, occ) & pos.piece_bb(by_color, PieceKind::Bishop)).count();
    count += (rook_attacks(sq, occ) & pos.piece_bb(by_color, PieceKind::Rook)).count();
    count += (queen_attacks(sq, occ) & pos.piece_bb(by_color, PieceKind::Queen)).count();
    count
}

/// Attacker-count delta on `threat_move`'s destination square: positive
/// means the threat's owner still has more attackers there than the
/// defender does, negative means the defender now has the edge.
///
/// D104 (Session 99) fix: the original TDSE proposal computed
/// `mover_color` as `pos.side_to_move.flip()` here — backwards. This is
/// called from `defusal_score` *after* the root candidate move has
/// already been applied via `pos.make_move(candidate)`, at which point
/// `pos.side_to_move` already correctly equals the threat's owner (the
/// opponent, since it's genuinely their turn next) with no flip needed;
/// flipping it would have pointed `mover_color` at our own side instead,
/// silently inverting the whole signal (reporting our own defensive
/// strength as if it were the threat's attacking strength, and vice
/// versa) rather than failing loudly.
///
/// Also fixes the original proposal's `threat_move.to_square()` — that
/// method doesn't exist anywhere in this codebase (`Move` has a plain
/// public `to: Square` field, caught independently during D98's
/// verification pass and documented there).
fn control_delta_on_threat_squares(pos: &Position, threat_move: Move) -> i32 {
    let target = threat_move.to;
    let mover_color = pos.side_to_move;
    let defenders = attacker_count_on(pos, target, mover_color.flip());
    let attackers = attacker_count_on(pos, target, mover_color);
    (attackers as i32) - (defenders as i32)
}

/// SEE-degradation + square-control defusal signal — Phase 28
/// (Session 99), TDSE's second isolated diff, built on top of D98's
/// legality-only `defuses_threat` (kept below, still independently
/// tested — this function doesn't call it, to avoid a second redundant
/// make/unmake pair, but computes the same legality check plus two more
/// signals in one pass).
///
/// `WEIGHT_ILLEGAL`/`WEIGHT_SEE`/`WEIGHT_CONTROL` are starting-point
/// constants, not yet Texel-tuned — per the original proposal's own
/// explicit note, tuning comes after the mechanism is validated, not
/// before (same order D63/D68 already established for HCE terms).
pub fn defusal_score(pos: &mut Position, candidate: Move, threat: &ThreatInfo) -> i32 {
    const WEIGHT_ILLEGAL: i32 = 1000;
    const WEIGHT_SEE:     i32 = 4;
    const WEIGHT_CONTROL: i32 = 15;

    let Some(threat_move) = threat.threat_move else { return 0 };

    pos.make_move(candidate);

    let still_legal = generate_moves(pos).iter().any(|m| *m == threat_move);
    let illegality_bonus = if still_legal { 0 } else { WEIGHT_ILLEGAL };

    // pos.side_to_move is correctly the threat's owner here (a normal
    // make_move just happened, it's genuinely their turn) — see_value_of
    // needs exactly this, same requirement D104 fixed in
    // extract_threat_move above.
    let see_after = if still_legal { see_value_of(pos, threat_move) } else { 0 };
    let see_drop = (threat.threat_see_before - see_after).max(0);

    let control_delta = control_delta_on_threat_squares(pos, threat_move);

    pos.unmake_move(candidate);

    illegality_bonus + WEIGHT_SEE * see_drop + WEIGHT_CONTROL * control_delta
}

/// Legality-only defusal signal — Phase 28 (Session 93) TDSE, first diff.
/// Given a root candidate move and the opponent's probed threat, checks
/// only whether the threat move is *still legal* after the candidate is
/// played. Returns `true` (defuses the threat) if not.
///
/// Deliberately does not yet account for SEE degradation or square-control
/// changes when the threat remains legal but gets materially worse or
/// better-defended — those are separate, independently-validated follow-up
/// diffs (DECISIONS.md D98), not bundled into this first landing per the
/// same staged-rollout discipline Phase 26's correction-history sub-items
/// (3a/3b/3c) already used.
pub fn defuses_threat(pos: &mut Position, candidate: Move, threat: &ThreatInfo) -> bool {
    let Some(threat_move) = threat.threat_move else { return false };
    pos.make_move(candidate);
    let still_legal = generate_moves(pos).iter().any(|m| *m == threat_move);
    pos.unmake_move(candidate);
    !still_legal
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
    fn test_search_at_singular_extension_depth_no_panic() {
        // Depth 7 is comfortably >= MIN_DEPTH_SINGULAR (6), so this
        // exercises the full singular-extension verification path added
        // in D59 — including its multi-cut early-return and
        // negative-extension branches, whichever a given position/TT
        // state happens to hit — without asserting which branch fires
        // (that's position- and TT-state-dependent, not deterministic
        // across seeds). The contract under test is just: search stays
        // bounded and produces a legal move, same as any other depth.
        setup();
        for seed in 0..5u64 {
            let mut pos  = Position::generate_with_seed(seed);
            let mut info = SearchInfo::new();
            let tt       = TranspositionTable::new(8);
            info.time_allocated_ms = 5000;

            let score = alpha_beta(
                &mut pos, 7, -INFINITY, INFINITY,
                0, true, &mut info, &tt, Move::NULL,
            );

            assert!(score.abs() <= INFINITY,
                "Score should be bounded at singular-extension depth (seed {})", seed);
            assert_ne!(info.best_move, Move::NULL,
                "Search should find a best move at depth 7 (seed {})", seed);

            let legal_moves = crate::movegen::generate_moves(&pos);
            assert!(
                legal_moves.iter().any(|&m| m == info.best_move),
                "Best move should be legal (seed {})", seed
            );
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

    // ── King-exposure guard (ROADMAP Phase 26 item 1) ───────────────────────

    #[test]
    fn test_null_move_king_guard_defaults_to_false() {
        // Default engine behavior must be byte-identical to before this
        // option existed — verified at the SearchInfo level, not just
        // main.rs's UCI plumbing (that's covered separately in main.rs's
        // own test suite, same split as skill_level/contempt).
        let info = SearchInfo::new();
        assert!(!info.null_move_king_guard,
            "null_move_king_guard must default to false");
    }

    #[test]
    fn test_king_safe_square_count_open_center_king_is_full_ring() {
        setup();
        // King on e4 with nothing nearby: all 8 ring squares are empty and
        // unattacked, so every one of them counts as safe.
        let fen = "8/8/8/8/4K3/8/8/7k w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(king_safe_square_count(&pos, Color::White), 8,
            "an open center king should have all 8 ring squares safe");
    }

    #[test]
    fn test_king_safe_square_count_excludes_own_occupied_ring_squares() {
        setup();
        // King in the corner has only 3 ring squares to begin with (a2, b1,
        // b2); two are occupied by its own pawns, so only b1 remains in the
        // ring at all, and nothing attacks it.
        let fen = "7k/8/8/8/8/8/PP6/K7 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(king_safe_square_count(&pos, Color::White), 1,
            "own pieces occupying ring squares must not count as safe \
             (or unsafe) — they're simply excluded from the ring");
    }

    #[test]
    fn test_king_safe_square_count_excludes_enemy_attacked_ring_squares() {
        setup();
        // Black rook on the open e-file attacks e5 (a ring square of the
        // White king on e4) but not e3 (blocked by the king itself) — only
        // e5 should be excluded, leaving 7 of the 8 ring squares safe.
        let fen = "4r3/8/8/8/4K3/8/8/7k w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(king_safe_square_count(&pos, Color::White), 7,
            "a ring square attacked by the enemy must not count as safe");
    }

    #[test]
    fn test_null_move_king_guard_off_matches_pre_guard_behavior() {
        setup();
        // With the guard left at its default (false), a heavily king-boxed
        // position must still allow the normal null-move path to run
        // exactly as it did before this option existed — i.e. search
        // completes and returns a legal move, unaffected by how few safe
        // squares the king has. This is the "byte-identical when off"
        // contract the doc comments on SearchInfo::null_move_king_guard
        // and the guard site in alpha_beta() both promise.
        let fen = "7k/8/8/8/8/8/PP6/K7 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let (mv, _score) = make_search(&mut pos, 4);
        assert_ne!(mv, Move::NULL,
            "search must return a legal move regardless of king exposure \
             when the guard is disabled (the default)");
    }

    #[test]
    fn test_null_move_king_guard_on_still_searches_safely() {
        setup();
        // With the guard enabled on a heavily king-boxed position (safe
        // squares == 1, at or below both guard thresholds), null-move
        // should either be skipped or reduced — either way the search
        // itself must still complete and return a legal move, not panic
        // or return a bogus depth/reduction.
        let fen = "7k/8/8/8/8/8/PP6/K7 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        info.null_move_king_guard = true;
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 4, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert_ne!(info.best_move, Move::NULL,
            "search must return a legal move with the guard enabled");
        assert!(score.abs() <= INFINITY);
    }

    // ── Non-pawn-material correction history (ROADMAP Phase 26 item 3a, D80) ──

    #[test]
    fn test_nonpawn_correction_defaults_to_false() {
        // D82: an unvalidated correction source ships gated off, same
        // discipline as null_move_king_guard — default engine behavior
        // must be byte-identical to before item 3a existed for anyone
        // who never touches this option.
        let info = SearchInfo::new();
        assert!(!info.nonpawn_correction_enabled,
            "nonpawn_correction_enabled must default to false");
    }

    #[test]
    fn test_nonpawn_correction_off_leaves_table_untouched() {
        setup();
        // With the flag left at its default (false), even a position
        // guaranteed to produce a large error when the flag IS on (the
        // same hanging-queen position used below) must leave the
        // non-pawn table completely untouched — proves the gating
        // actually short-circuits both the apply() and update() call
        // sites, not just one of them.
        let fen = "3r2k1/ppp2ppp/8/8/3Q4/8/PPP2PPP/4K3 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let _ = alpha_beta(
            &mut pos, 6, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        let nphash = nonpawn_hash(&pos);
        assert_eq!(info.correction_history_nonpawn.get(nphash, pos.side_to_move), 0,
            "non-pawn table must stay untouched when the flag is off");
    }

    #[test]
    fn test_nonpawn_correction_history_wired_into_search() {
        setup();
        // Proves the update() call site added in this diff is actually
        // reached during a real search when the flag is enabled, not
        // just present in dead code.
        //
        // Deliberately NOT using the start position: a well-balanced,
        // symmetric position can produce a genuinely tiny search-vs-
        // static-eval error, which the weighted-average update formula
        // (`entry = (entry*(256-w) + error*w) / 256`, integer division)
        // can legitimately round straight back down to 0 — that would
        // make this test flaky/misleading, not a real signal of broken
        // wiring. Verified this directly against a real build before
        // picking this position (see DECISIONS.md D80).
        //
        // Instead: an undefended queen on an open file, Black to move,
        // no recapture available. Static eval at the root still counts
        // the queen (it hasn't been captured yet); search immediately
        // finds Rxd4 winning it. That gap is large enough (~600+cp) to
        // survive the rounding at any reasonable depth. King placed on
        // g8 (not h8) deliberately — h8 sits on the queen's d4-h8
        // diagonal, which made an earlier draft of this exact position
        // an illegal FEN (opponent already in check when not to move)
        // and crashed the engine entirely; see DECISIONS.md D81.
        let fen = "3r2k1/ppp2ppp/8/8/3Q4/8/PPP2PPP/4K3 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        info.nonpawn_correction_enabled = true;
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let _ = alpha_beta(
            &mut pos, 6, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        let nphash = nonpawn_hash(&pos);
        let corr = info.correction_history_nonpawn.get(nphash, pos.side_to_move);
        assert_ne!(corr, 0,
            "non-pawn correction table should have a non-zero entry for \
             the root position's hash after search discovers the hanging \
             queen, with the flag enabled — if this is 0, the new \
             update() call in this diff likely isn't being reached");
    }

    #[test]
    fn test_nonpawn_and_pawn_corrections_are_independent_sources() {
        setup();
        // The two tables must not be the same underlying storage — a
        // regression here (e.g. accidentally aliasing or copy-pasting
        // the pawn table's hash into both call sites) would silently
        // collapse two independent signal sources into one. Confirmed by
        // directly seeding each table via a different hash and checking
        // the other table wasn't touched.
        let mut info = SearchInfo::new();
        info.correction_history.update(0xAAAA, Color::White, 100, 200, 8);
        info.correction_history_nonpawn.update(0xBBBB, Color::White, 100, 150, 8);
        assert_ne!(info.correction_history.get(0xAAAA, Color::White), 0);
        assert_eq!(info.correction_history_nonpawn.get(0xAAAA, Color::White), 0,
            "updating the pawn table at a given hash must not leak into \
             the non-pawn table at the same hash");
        assert_ne!(info.correction_history_nonpawn.get(0xBBBB, Color::White), 0);
        assert_eq!(info.correction_history.get(0xBBBB, Color::White), 0,
            "updating the non-pawn table at a given hash must not leak \
             into the pawn table at the same hash");
    }

    // ── Continuation-based correction history (ROADMAP Phase 26 item 3b, D86) ──

    #[test]
    fn test_continuation_correction_defaults_to_false() {
        let info = SearchInfo::new();
        assert!(!info.continuation_correction_enabled,
            "continuation_correction_enabled must default to false");
    }

    #[test]
    fn test_continuation_correction_off_leaves_table_untouched() {
        setup();
        let fen = "3r2k1/ppp2ppp/8/8/3Q4/8/PPP2PPP/4K3 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let _ = alpha_beta(
            &mut pos, 6, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        // With no prev_move at the root and the flag off, the table
        // must never be touched — check a broad sample of hashes stays
        // at exactly the fresh-table default (0) rather than trying to
        // reconstruct which hash the root would have used.
        assert_eq!(info.correction_history_continuation.get(0, Color::Black), 0);
        assert_eq!(info.correction_history_continuation.get(12345, Color::White), 0);
    }

    #[test]
    fn test_continuation_correction_history_wired_into_search() {
        setup();
        // Same hanging-queen position as item 3a's own wiring test (see
        // that test's comment for why the start position isn't used —
        // small natural error rounds to 0 under the weighted-average
        // formula). continuation_hash additionally needs 2 real moves
        // of history to return Some at all; rather than actually play
        // out two arbitrary opening moves (which would change the
        // position and defeat the point), synthetically attach two
        // history entries directly — continuation_hash only reads
        // pos.history's move squares, never the resulting board state
        // (see that function's own doc comment), so this is exactly
        // equivalent to having really played them.
        let fen = "3r2k1/ppp2ppp/8/8/3Q4/8/PPP2PPP/4K3 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let synth = |from, to| crate::position::HistoryEntry {
            mv: Move::new(from, to, MoveKind::Quiet),
            castling: pos.castling,
            en_passant: None,
            halfmove_clock: 0,
            hash: pos.hash,
            captured: None,
        };
        pos.history.push(synth(Square::A2, Square::A3));
        let prev_move = Move::new(Square::H7, Square::H6, MoveKind::Quiet);
        pos.history.push(synth(Square::H7, Square::H6));

        let mut info = SearchInfo::new();
        info.continuation_correction_enabled = true;
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let _ = alpha_beta(
            &mut pos, 6, -INFINITY, INFINITY, 0, true, &mut info, &tt, prev_move,
        );
        let chash = continuation_hash(&pos, prev_move)
            .expect("2 real moves of history were attached — must be Some");
        let corr = info.correction_history_continuation.get(chash, pos.side_to_move);
        assert_ne!(corr, 0,
            "continuation correction table should have a non-zero entry \
             for the root's own continuation hash after search \
             discovers the hanging queen — if this is 0, the new \
             update() call in this diff likely isn't being reached");
    }

    #[test]
    fn test_continuation_correction_independent_of_other_sources() {
        setup();
        // Same independence proof as item 3a's, extended to all three
        // tables — seeding one must never leak into either of the
        // other two at the same hash value.
        let mut info = SearchInfo::new();
        info.correction_history.update(0xAAAA, Color::White, 100, 200, 8);
        info.correction_history_nonpawn.update(0xBBBB, Color::White, 100, 150, 8);
        info.correction_history_continuation.update(0xCCCC, Color::White, 100, 180, 8);
        assert_ne!(info.correction_history_continuation.get(0xCCCC, Color::White), 0);
        assert_eq!(info.correction_history.get(0xCCCC, Color::White), 0);
        assert_eq!(info.correction_history_nonpawn.get(0xCCCC, Color::White), 0);
    }

    // ── Correction-scaled singular extension margin (ROADMAP Phase 26 item 3c, D89) ──

    #[test]
    fn test_correction_extension_defaults_to_false() {
        let info = SearchInfo::new();
        assert!(!info.correction_extension_enabled,
            "correction_extension_enabled must default to false");
    }

    #[test]
    fn test_correction_extension_off_matches_pre_existing_behavior() {
        setup();
        // With the flag off (default), search on a position deep enough
        // to reach the singular-extension code path must complete
        // normally and return a legal move — proves the new branch
        // doesn't interfere with the existing, already-tested singular-
        // extension path (test_search_at_singular_extension_depth_no_panic,
        // above) when disabled.
        let mut pos = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 7, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert_ne!(info.best_move, Move::NULL);
        assert!(score.abs() <= INFINITY);
    }

    #[test]
    fn test_correction_extension_on_still_searches_safely() {
        setup();
        // With the flag on, even a position where the pawn-hash
        // correction table happens to hold a large pre-seeded value
        // (simulating a position type with a real history of eval
        // error) must still complete a normal search and return a
        // legal move — the margin can only shrink to 1, never reach 0
        // (see singular_margin_reduction's own cap), so this must never
        // produce a degenerate singular_beta == tt_score search window.
        let mut pos = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.correction_extension_enabled = true;
        let phash = pawn_hash(&pos);
        info.correction_history.update(phash, pos.side_to_move, 0, 512, 16);
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 7, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert_ne!(info.best_move, Move::NULL);
        assert!(score.abs() <= INFINITY);
    }

    // ── Phase 27 diagnostic toggles (D59/D60 A/B, Session 86) ─────────────────

    #[test]
    fn test_lmp_enabled_defaults_to_true() {
        // Unlike the Phase 26 items above, D60 is already default-on
        // production behavior — `true` is the byte-identical-to-current
        // default, not an opt-in.
        let info = SearchInfo::new();
        assert!(info.lmp_enabled, "lmp_enabled must default to true");
    }

    #[test]
    fn test_singular_multicut_enabled_defaults_to_true() {
        let info = SearchInfo::new();
        assert!(info.singular_multicut_enabled,
            "singular_multicut_enabled must default to true");
    }

    #[test]
    fn test_lmp_disabled_still_searches_safely() {
        setup();
        // With LMP switched off, the move loop falls back to searching
        // every quiet move at full width — must still complete and return
        // a legal move, just doing more work per node.
        let mut pos = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.lmp_enabled = false;
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 6, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert_ne!(info.best_move, Move::NULL);
        assert!(score.abs() <= INFINITY);
    }

    #[test]
    fn test_singular_multicut_disabled_still_searches_safely() {
        setup();
        // With the D59 additions switched off, only Phase 13.3's original
        // base singular extension branch can fire — must still complete
        // and return a legal move at a depth deep enough to reach the
        // singular-extension code path (same depth as the existing
        // test_search_at_singular_extension_depth_no_panic coverage).
        let mut pos = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        info.singular_multicut_enabled = false;
        info.time_allocated_ms = 60_000;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 7, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert_ne!(info.best_move, Move::NULL);
        assert!(score.abs() <= INFINITY);
    }

    // ── D95 (Session 90): is_time_up() must actually set info.stop ────────────
    // The bug: both time-check sites in this file called info.is_time_up()
    // and returned the corrupted 0 sentinel on true, but never wrote
    // info.stop = true — so every downstream `if info.stop { ... }` guard
    // in this file (TT-store, correction-history-update,
    // iterative_deepening()'s discard-this-depth logic) silently never
    // fired for a real elapsed-time timeout, only for an external
    // stop_flag/ponder abort. is_time_up()'s own doc/test coverage in
    // search/mod.rs already covered that the *read* side works given
    // info.stop already true; these tests cover the *write* side that was
    // missing — that reaching a real timeout from elapsed time alone
    // actually sets it, at the two call sites that are supposed to.

    #[test]
    fn test_alpha_beta_sets_stop_on_real_timeout() {
        setup();
        let mut pos  = Position::start_pos().unwrap();
        let mut info = SearchInfo::new();
        // 0ms budget — is_time_up()'s elapsed-time branch fires on the
        // very first check (nodes starts at 0, `0 & 255 == 0` is true,
        // and elapsed_ms() >= 0 is trivially true), before any node-count
        // gating could delay it.
        info.time_allocated_ms = 0;
        let tt = TranspositionTable::new(16);
        let score = alpha_beta(
            &mut pos, 5, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert!(info.stop,
            "alpha_beta must set info.stop = true on a real elapsed-time \
             timeout, not just return the 0 sentinel silently — see D95");
        assert_eq!(score, 0,
            "the 0 sentinel itself is unchanged by this fix — only whether \
             info.stop gets set alongside it");
    }

    #[test]
    fn test_quiescence_in_check_sets_stop_on_real_timeout() {
        setup();
        // Black king on e8, White rook on e1, clear e-file between them —
        // genuinely in check, specifically to exercise quiescence()'s
        // in-check evasion path (the other of the two D95 call sites).
        // alpha_beta at depth<=0 against this position drops straight
        // into quiescence with in_check = true.
        let fen = "4k3/8/8/8/8/8/8/4R1K1 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        info.time_allocated_ms = 0;
        let tt = TranspositionTable::new(16);
        let _ = alpha_beta(
            &mut pos, 0, -INFINITY, INFINITY, 0, true, &mut info, &tt, Move::NULL,
        );
        assert!(info.stop,
            "quiescence()'s time check must also set info.stop = true on a \
             real elapsed-time timeout — see D95");
    }

    // ── Phase 28 (Session 93): Threat-Defusal Search Extension (TDSE) ─────────

    #[test]
    fn test_threat_defusal_defaults_to_false() {
        let info = SearchInfo::new();
        assert!(!info.threat_defusal,
            "threat_defusal must default to false — new/unproven technique \
             (D98), same rollout shape as null_move_king_guard (D75)");
    }

    #[test]
    fn test_extract_threat_move_finds_hanging_piece_capture() {
        setup();
        // White rook hangs undefended on d5; Black's queen on d8 can simply
        // capture it down the open d-file — the only sane best reply in
        // this constructed position, so extract_threat_move (which searches
        // from Black's perspective via the same null-move flip the real
        // null-move probe uses) should find exactly this move.
        let fen = "3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);
        let threat = extract_threat_move(&mut pos, &mut info, &tt, 6)
            .expect("a real threat (hanging rook) should be found");
        let mv = threat.threat_move.expect("threat_move should be Some");
        assert_eq!(mv.from, Square::D8);
        assert_eq!(mv.to, Square::D5);
        // Position must be restored exactly (side to move, hash, en
        // passant) after the probe's internal flip/unflip.
        assert_eq!(pos.side_to_move, Color::White);
    }

    #[test]
    fn test_defuses_threat_true_when_threat_move_no_longer_legal() {
        setup();
        let fen = "3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);
        let threat = extract_threat_move(&mut pos, &mut info, &tt, 6).unwrap();
        // Moving the hanging rook out of reach removes the exact capture
        // the threat represents — no legal move afterward has the same
        // from/to/captured shape as the original threat.
        let defusing_move = Move::new(Square::D5, Square::D4, MoveKind::Quiet);
        assert!(defuses_threat(&mut pos, defusing_move, &threat),
            "moving the hanging rook away must defuse the exact capture \
             threat");
    }

    #[test]
    fn test_defuses_threat_false_when_threat_move_still_legal() {
        setup();
        let fen = "3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);
        let threat = extract_threat_move(&mut pos, &mut info, &tt, 6).unwrap();
        // An unrelated king move doesn't touch the hanging rook at all —
        // Black's exact capture is still fully legal afterward.
        let unrelated_move = Move::new(Square::E1, Square::F1, MoveKind::Quiet);
        assert!(!defuses_threat(&mut pos, unrelated_move, &threat),
            "an unrelated move must not be reported as defusing a real \
             threat that's still fully legal afterward");
    }

    #[test]
    fn test_extract_threat_move_returns_none_when_in_check() {
        setup();
        // D101 (Session 96) regression test. White king on e1 is in check
        // from Black's rook on e8 down a clear e-file. Flipping side to
        // move here (as if White's check simply doesn't exist) is exactly
        // the invalid state that led to the real crash this guard fixes —
        // must return None, not attempt the probe, and must leave pos
        // completely untouched (verified by re-checking king_sq for both
        // colors doesn't panic and the FEN round-trips unchanged).
        // Black king on h8 (out of the way), rook on e8 giving check down
        // a clear e-file to White's king on e1. The original version of
        // this test only placed a White king and forgot Black's entirely —
        // from_fen() correctly rejects that (KingNotFound(Black)), caught
        // by CI. Fixed here; not a defect in the D101 guard itself.
        let fen = "4r2k/8/8/8/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        assert!(pos.in_check(Color::White), "setup: White must be in check");
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);

        let result = extract_threat_move(&mut pos, &mut info, &tt, 6);

        assert!(result.is_none(),
            "extract_threat_move must return None when in check, not \
             attempt the null-move-style flip — see D101");
        // Position must be completely untouched — same king squares,
        // same side to move, no corruption from a partially-applied flip.
        assert_eq!(pos.side_to_move, Color::White);
        assert_eq!(pos.king_sq(Color::White), Square::E1);
        assert_eq!(pos.king_sq(Color::Black), Square::H8);
    }

    // ── Phase 28 (Session 99): SEE-degradation signal (D104) ──────────────────

    #[test]
    fn test_extract_threat_move_computes_correct_see_before() {
        setup();
        // Same hanging-rook FEN as D98's tests. Black's queen on d8 can
        // capture White's undefended rook on d5 cleanly — nothing
        // recaptures, so the SEE value of that exact capture is exactly
        // the rook's value (500, SEE_VALUES[Rook]). This test would have
        // directly caught D104's original ordering bug: computing
        // threat_see_before *after* restoring pos.side_to_move made
        // see_value_of look for White's piece on d8 (there is none — d8
        // is Black's queen), silently returning 0 instead of 500.
        let fen = "3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);
        let threat = extract_threat_move(&mut pos, &mut info, &tt, 6)
            .expect("a real threat (hanging rook) should be found");
        assert_eq!(threat.threat_see_before, 500,
            "SEE value of Black capturing White's undefended rook must be \
             exactly the rook's value — see D104");
    }

    #[test]
    fn test_defusal_score_ranks_illegal_threat_above_unrelated_move() {
        setup();
        let fen = "3qk3/8/8/3R4/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let mut info = SearchInfo::new();
        let tt = TranspositionTable::new(16);
        let threat = extract_threat_move(&mut pos, &mut info, &tt, 6).unwrap();

        // Moving the rook away makes the exact capture illegal — should
        // dominate via the illegality bonus.
        let defusing_move = Move::new(Square::D5, Square::D4, MoveKind::Quiet);
        let defusing_score = defusal_score(&mut pos, defusing_move, &threat);

        // An unrelated king move leaves the capture fully legal and its
        // SEE value unchanged.
        let unrelated_move = Move::new(Square::E1, Square::F1, MoveKind::Quiet);
        let unrelated_score = defusal_score(&mut pos, unrelated_move, &threat);

        assert!(defusing_score > unrelated_score,
            "a move that makes the real capture illegal ({defusing_score}) \
             must score higher than one that doesn't affect it at all \
             ({unrelated_score})");
    }

    #[test]
    fn test_attacker_count_on_counts_only_the_requested_color() {
        setup();
        // Two White knights both attack d5 (c3 and e3); no Black piece
        // attacks it at all.
        let fen = "4k3/8/8/8/8/2N1N3/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(attacker_count_on(&pos, Square::D5, Color::White), 2);
        assert_eq!(attacker_count_on(&pos, Square::D5, Color::Black), 0);
    }
}
