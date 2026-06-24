// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/ordering.rs — Move ordering
//
// Move ordering is critical for alpha-beta efficiency.
// With perfect ordering, alpha-beta searches sqrt(N) nodes vs N for minimax.
// With good ordering, we get close to this theoretical maximum.
//
// Order (highest priority first):
//   1. TT move (best move from previous search of this position)
//   2. Winning captures by SEE (captures that gain material)
//   3. Equal captures (SEE == 0)
//   4. Killer moves (quiet moves that caused cutoffs at this ply)
//   5. Countermove (best response to opponent's last move)
//   6. Quiet moves by history score (moves that historically cause cutoffs)
//   7. Losing captures (SEE < 0) — last resort
//
// ⚠️ Pet Dragon note:
//   Pawn double-steps from rank 1 get a history bonus initialised above 0.
//   They are developing moves that simultaneously open rank 1 for other
//   pieces — more valuable than standard pawn pushes.
//   Treat them closer to piece moves in ordering priority.
// ============================================================================

use crate::movegen::MoveList;
use crate::position::Position;
use crate::search::see::{see, see_value_of};
use crate::search::SearchInfo;
use crate::types::{Color, Move, MoveKind, PieceKind};

// ── Move score constants ──────────────────────────────────────────────────────
// These scores determine ordering priority.
// Higher score = searched first.

const TT_MOVE_SCORE:         i32 = 2_000_000;
const WINNING_CAPTURE_BASE:  i32 = 1_000_000;
const EQUAL_CAPTURE_SCORE:   i32 = 500_000;
const KILLER_1_SCORE:        i32 = 400_000;
const KILLER_2_SCORE:        i32 = 300_000;
const COUNTERMOVE_SCORE:     i32 = 200_000;
const QUIET_BASE_SCORE:      i32 = 0;
const LOSING_CAPTURE_BASE:   i32 = -500_000;

/// Bonus for pawn double-steps from rank 1 (Pet Dragon specific)
/// These are developing moves — rank above standard pawn pushes
const PET_DRAGON_RANK1_PUSH_BONUS: i32 = 50_000;

// ── MVV-LVA table ─────────────────────────────────────────────────────────────
// Most Valuable Victim - Least Valuable Attacker
// Captures ordered by: value of captured piece - value of capturing piece
// Ensures we look at PxQ before QxP

const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20000];

#[inline]
fn mvv_lva_score(attacker: PieceKind, victim: PieceKind) -> i32 {
    PIECE_VALUES[victim as usize] * 10 - PIECE_VALUES[attacker as usize]
}

// ── Scored move ───────────────────────────────────────────────────────────────

/// A move paired with its ordering score
#[derive(Clone, Copy)]
struct ScoredMove {
    mv:    Move,
    score: i32,
}

// ── Main scoring function ─────────────────────────────────────────────────────

/// Score all moves in a move list for ordering
pub fn score_moves(
    pos:        &Position,
    moves:      &MoveList,
    info:       &SearchInfo,
    tt_move:    Move,
    ply:        usize,
    prev_move:  Move,
) -> Vec<ScoredMove> {
    let color   = pos.side_to_move;
    let color_idx = color as usize;

    let killer1 = if ply < crate::search::MAX_PLY {
        info.killers[ply][0]
    } else {
        Move::NULL
    };
    let killer2 = if ply < crate::search::MAX_PLY {
        info.killers[ply][1]
    } else {
        Move::NULL
    };

    let countermove = if prev_move != Move::NULL {
        info.get_countermove(
            prev_move.from.index() as usize,
            prev_move.to.index() as usize,
        )
    } else {
        Move::NULL
    };

    let mut scored: Vec<ScoredMove> = Vec::with_capacity(moves.len());

    for i in 0..moves.len() {
        let mv    = moves.get(i);
        let score = score_move(
            pos, mv, info, tt_move,
            killer1, killer2, countermove,
            color_idx, ply,
        );
        scored.push(ScoredMove { mv, score });
    }

    scored
}

/// Score a single move
fn score_move(
    pos:         &Position,
    mv:          Move,
    info:        &SearchInfo,
    tt_move:     Move,
    killer1:     Move,
    killer2:     Move,
    countermove: Move,
    color_idx:   usize,
    ply:         usize,
) -> i32 {
    // ── TT move — highest priority ────────────────────────────────────────────
    if mv == tt_move {
        return TT_MOVE_SCORE;
    }

    let from = mv.from.index() as usize;
    let to   = mv.to.index() as usize;

    // ── Captures ──────────────────────────────────────────────────────────────
    if mv.kind.is_capture() {
        // Promotions with capture — very high value
        if mv.kind.is_promotion() {
            return WINNING_CAPTURE_BASE + 500_000
                + mv.kind.promotion_piece()
                    .map(|p| PIECE_VALUES[p as usize])
                    .unwrap_or(0);
        }

        // Use SEE to determine if capture wins/loses material
        let see_val = see_value_of(pos, mv);
        let attacker = pos.piece_on(mv.from, pos.side_to_move)
            .unwrap_or(PieceKind::Pawn);
        let victim = mv.captured.unwrap_or(PieceKind::Pawn);

        return if see_val > 0 {
            // Winning capture — order by MVV-LVA within winners
            WINNING_CAPTURE_BASE + mvv_lva_score(attacker, victim)
        } else if see_val == 0 {
            // Equal capture
            EQUAL_CAPTURE_SCORE + mvv_lva_score(attacker, victim)
        } else {
            // Losing capture — goes last, ordered by how much we lose
            LOSING_CAPTURE_BASE + see_val
        };
    }

    // ── Promotions (quiet) ────────────────────────────────────────────────────
    if mv.kind.is_promotion() {
        return WINNING_CAPTURE_BASE + 400_000
            + mv.kind.promotion_piece()
                .map(|p| PIECE_VALUES[p as usize])
                .unwrap_or(0);
    }

    // ── En passant ────────────────────────────────────────────────────────────
    if mv.kind == MoveKind::EnPassant {
        return EQUAL_CAPTURE_SCORE;
    }

    // ── Killer moves ──────────────────────────────────────────────────────────
    if mv == killer1 {
        return KILLER_1_SCORE;
    }
    if mv == killer2 {
        return KILLER_2_SCORE;
    }

    // ── Countermove ───────────────────────────────────────────────────────────
    if mv == countermove {
        return COUNTERMOVE_SCORE;
    }

    // ── Quiet moves — ordered by history ─────────────────────────────────────
    let mut history_score = info.history[color_idx][from][to];

    // ⚠️ Pet Dragon: bonus for pawn double-steps from rank 1
    // These moves develop position AND open rank 1 simultaneously
    if mv.kind == MoveKind::DoublePush {
        let pawn_rank = mv.from.rank();
        let is_rank1_push = match pos.side_to_move {
            Color::White => pawn_rank == 0, // rank 1 (0-indexed)
            Color::Black => pawn_rank == 7, // rank 8 (0-indexed)
        };
        if is_rank1_push {
            history_score += PET_DRAGON_RANK1_PUSH_BONUS;
        }
    }

    QUIET_BASE_SCORE + history_score
}

// ── Incremental move selection ────────────────────────────────────────────────
// Instead of sorting all moves upfront, we pick the best one each time.
// This avoids sorting moves that are never searched (due to cutoffs).

/// Pick the next best move from the scored list (partial selection sort)
pub fn pick_next_move(scored: &mut Vec<ScoredMove>, start: usize) -> Option<Move> {
    if start >= scored.len() {
        return None;
    }

    // Find the move with the highest score from 'start' onwards
    let mut best_idx   = start;
    let mut best_score = scored[start].score;

    for i in (start + 1)..scored.len() {
        if scored[i].score > best_score {
            best_score = scored[i].score;
            best_idx   = i;
        }
    }

    // Swap best to front
    scored.swap(start, best_idx);
    Some(scored[start].mv)
}

// ── Quiescence move ordering ──────────────────────────────────────────────────

/// Score captures for quiescence search
/// Only winning/equal captures, ordered by MVV-LVA then SEE
pub fn score_captures(
    pos:    &Position,
    moves:  &MoveList,
    tt_move: Move,
) -> Vec<ScoredMove> {
    let mut scored = Vec::with_capacity(moves.len());

    for i in 0..moves.len() {
        let mv = moves.get(i);

        let score = if mv == tt_move {
            TT_MOVE_SCORE
        } else if mv.kind.is_promotion() {
            WINNING_CAPTURE_BASE + 500_000
        } else {
            let see_val = see_value_of(pos, mv);
            let attacker = pos.piece_on(mv.from, pos.side_to_move)
                .unwrap_or(PieceKind::Pawn);
            let victim = mv.captured.unwrap_or(PieceKind::Pawn);
            if see_val >= 0 {
                WINNING_CAPTURE_BASE + mvv_lva_score(attacker, victim)
            } else {
                LOSING_CAPTURE_BASE + see_val
            }
        };

        scored.push(ScoredMove { mv, score });
    }

    scored
}

// ── Public interface ──────────────────────────────────────────────────────────

/// Get the next best move from a scored move list, starting at index
/// Uses partial selection sort — O(n) per call but avoids full sort
pub fn next_move(scored: &mut Vec<ScoredMove>, index: usize) -> Option<Move> {
    pick_next_move(scored, index)
}

/// Update move ordering tables after a beta cutoff
/// Call when a quiet move causes a cutoff (fail-high)
pub fn update_ordering_on_cutoff(
    info:      &mut SearchInfo,
    mv:        Move,
    prev_move: Move,
    ply:       usize,
    depth:     i32,
    color:     Color,
    quiets_tried: &[Move],
) {
    if mv.kind.is_capture() || mv.kind.is_promotion() {
        return; // Only update for quiet moves
    }

    let color_idx = color as usize;
    let from      = mv.from.index() as usize;
    let to        = mv.to.index() as usize;

    // Update killer moves
    info.update_killer(mv, ply);

    // Update countermove
    if prev_move != Move::NULL {
        info.update_countermove(
            prev_move.from.index() as usize,
            prev_move.to.index() as usize,
            mv,
        );
    }

    // Update history — bonus for the move that caused cutoff
    info.update_history(color_idx, from, to, depth, true);

    // Penalise quiet moves that were tried before this one
    for &tried in quiets_tried {
        if tried == mv { continue; }
        if tried.kind.is_capture() || tried.kind.is_promotion() { continue; }
        info.update_history(
            color_idx,
            tried.from.index() as usize,
            tried.to.index() as usize,
            depth,
            false,
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::movegen::generate_moves;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::search::SearchInfo;
    use crate::types::{Move, MoveKind, Square};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_tt_move_first() {
        setup();
        let pos     = Position::start_pos().unwrap();
        let moves   = generate_moves(&pos);
        let info    = SearchInfo::new();
        let tt_move = moves.get(3); // Pick a random move as TT move

        let mut scored = score_moves(
            &pos, &moves, &info, tt_move, 0, Move::NULL
        );

        // TT move should be picked first
        let first = next_move(&mut scored, 0).unwrap();
        assert_eq!(first, tt_move,
            "TT move should be ordered first");
    }

    #[test]
    fn test_capture_before_quiet() {
        setup();
        // Position where White has both captures and quiet moves
        let fen = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let moves = generate_moves(&pos);
        let info  = SearchInfo::new();

        let mut scored = score_moves(
            &pos, &moves, &info, Move::NULL, 0, Move::NULL
        );

        // First move should be a capture (pawn takes pawn)
        let first = next_move(&mut scored, 0).unwrap();
        assert!(
            first.kind.is_capture(),
            "Capture should be ordered before quiet moves"
        );
    }

    #[test]
    fn test_killer_ordering() {
        setup();
        let pos   = Position::start_pos().unwrap();
        let moves = generate_moves(&pos);
        let mut info = SearchInfo::new();

        // Set a killer move
        let killer = moves.get(5);
        info.update_killer(killer, 0);

        let mut scored = score_moves(
            &pos, &moves, &info, Move::NULL, 0, Move::NULL
        );

        // Find killer in scored list — it should have KILLER_1_SCORE
        let killer_scored = scored.iter().find(|s| s.mv == killer);
        if let Some(ks) = killer_scored {
            assert_eq!(ks.score, KILLER_1_SCORE,
                "Killer move should have killer score");
        }
    }

    #[test]
    fn test_pet_dragon_rank1_pawn_bonus() {
        setup();
        // Find a Pet Dragon position with a rank 1 White pawn
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let white_pawns = pos.piece_bb(
                crate::types::Color::White,
                PieceKind::Pawn
            );
            let rank1_pawns = white_pawns & crate::bitboard::Bitboard::RANK_1;

            if rank1_pawns.is_not_empty() {
                let moves = generate_moves(&pos);
                let info  = SearchInfo::new();
                let mut scored = score_moves(
                    &pos, &moves, &info, Move::NULL, 0, Move::NULL
                );

                // Find a double-push from rank 1
                let rank1_double = moves.iter().find(|m| {
                    m.kind == MoveKind::DoublePush && m.from.rank() == 0
                });

                if let Some(&dp) = rank1_double {
                    let dp_scored = scored.iter().find(|s| s.mv == dp);
                    if let Some(dps) = dp_scored {
                        assert!(
                            dps.score >= QUIET_BASE_SCORE
                                + PET_DRAGON_RANK1_PUSH_BONUS,
                            "Rank 1 double push should get Pet Dragon bonus"
                        );
                    }
                }
                return;
            }
        }
    }

    #[test]
    fn test_mvv_lva_ordering() {
        // Pawn captures Queen should score higher than Queen captures Pawn
        let pxq = mvv_lva_score(PieceKind::Pawn,  PieceKind::Queen);
        let qxp = mvv_lva_score(PieceKind::Queen, PieceKind::Pawn);
        assert!(pxq > qxp,
            "PxQ should score higher than QxP in MVV-LVA");
    }

    #[test]
    fn test_pick_next_move_order() {
        setup();
        let pos   = Position::start_pos().unwrap();
        let moves = generate_moves(&pos);
        let info  = SearchInfo::new();

        let mut scored = score_moves(
            &pos, &moves, &info, Move::NULL, 0, Move::NULL
        );

        // Pick moves one by one — should be in descending score order
        let mut prev_score = i32::MAX;
        for i in 0..scored.len() {
            let mv = next_move(&mut scored, i).unwrap();
            let score = scored[i].score;
            assert!(score <= prev_score,
                "Moves should come out in descending score order");
            prev_score = score;
            let _ = mv;
        }
    }

    #[test]
    fn test_update_ordering_on_cutoff() {
        setup();
        let pos   = Position::start_pos().unwrap();
        let moves = generate_moves(&pos);
        let mut info = SearchInfo::new();
        let mv    = moves.get(0);
        let color = pos.side_to_move;

        update_ordering_on_cutoff(
            &mut info, mv, Move::NULL, 0, 5, color, &[]
        );

        // History should be updated
        assert!(
            info.history[color as usize]
                [mv.from.index() as usize]
                [mv.to.index() as usize] > 0,
            "History should be updated after cutoff"
        );

        // Killer should be updated
        assert_eq!(info.killers[0][0], mv,
            "Killer should be updated after cutoff");
    }
}
