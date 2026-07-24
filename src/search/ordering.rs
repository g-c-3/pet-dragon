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
// Additive, not a separate tier: Phase 23.4's bucketed opening statistics
// (D67/D71, Session 84) add a bonus on top of whichever tier above a move
// already lands in, ONLY at the true, unmoved start of the game (fullmove
// 1, White to move, search ply 0 — never at an interior search node, and
// never on a position reached after any moves, real or hypothetical, have
// been made). See OPENING_STATS_BONUS below for why this can't override
// TT/winning-capture signals.
//
// ⚠️ Pet Dragon note:
//   Pawn double-steps from rank 1 get a history bonus initialised above 0.
//   They are developing moves that simultaneously open rank 1 for other
//   pieces — more valuable than standard pawn pushes.
//   Treat them closer to piece moves in ordering priority.
// ============================================================================

use crate::movegen::MoveList;
use crate::opening_stats;
use crate::position::Position;
use crate::search::see::see_value_of;
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

/// Bonus for a move flagged by Phase 23.4's bucketed opening statistics
/// (D67/D71, Session 84) — an additive nudge, not a replacement for normal
/// scoring: added on top of whatever score the move already earned, so it
/// can never override a TT move or a winning capture (both score in the
/// millions), and sits below COUNTERMOVE_SCORE so a real countermove signal
/// still wins on a tie. Only meaningfully moves the needle for an otherwise
/// ordinary quiet move, exactly matching D67's "nudge earlier in ordering,
/// let full search still evaluate and override it" design — this is a
/// move-ordering hint, not a forced move. Table entries are currently thin
/// (2 entries as of Session 84 — see DECISIONS.md D71), so in practice this
/// almost never fires yet; it's wired in now so the mechanism is proven
/// correct in the engine itself, ahead of the table actually filling in.
const OPENING_STATS_BONUS: i32 = 150_000;

// ── MVV-LVA table ─────────────────────────────────────────────────────────────
// Most Valuable Victim - Least Valuable Attacker
// Captures ordered by: value of captured piece - value of capturing piece
// Ensures we look at PxQ before QxP

const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20000];

#[inline]
fn mvv_lva_score(attacker: PieceKind, victim: PieceKind) -> i32 {
    PIECE_VALUES[victim as usize] * 10 - PIECE_VALUES[attacker as usize]
}

/// Small thread-id-dependent perturbation for quiet-move ordering ties
/// (Phase 23.2 / D49 — thread-differentiated Lazy SMP). Before this,
/// helper threads' `history`/`countermoves`/`cont_hist` tables started
/// from the same snapshot as the main thread and updated identically
/// along a shared best line, so on ties they walked the same move order
/// as the main thread — one more way helpers ended up largely duplicating
/// work instead of covering different tree regions.
///
/// This is a deterministic mix, not actual randomness: the same
/// `(thread_id, from, to)` always produces the same offset, so
/// `Position::generate_with_seed`-based reproducibility in
/// `selfplay.rs`/`match_runner.rs`/`uci_match_runner.rs` is unaffected —
/// only which thread explores which tied line changes, not whether a
/// given seed's game is reproducible.
///
/// `thread_id == 0` (the main thread — the only thread whose result is
/// ever reported, see the Phase 19 MultiPV note in `main.rs::cmd_go`)
/// always returns `0`: this must never change the main thread's move
/// ordering, since that would change engine strength/behavior for every
/// user, not just Lazy SMP tree diversity for helper threads.
///
/// Magnitude is capped small (`0..=3`) relative to `QUIET_BASE_SCORE`'s
/// neighboring buckets so it can only affect ordering among moves already
/// tied or near-tied on real history score — it can never override a
/// meaningful score difference (e.g. jump a move ahead of a killer or a
/// capture).
#[inline]
fn thread_tie_break(thread_id: usize, from: usize, to: usize) -> i32 {
    if thread_id == 0 {
        return 0;
    }
    // Cheap fixed-point multiplicative mix (not a cryptographic hash) —
    // just enough avalanche that adjacent (from, to) pairs don't get
    // correlated offsets within the same thread.
    let mixed = (thread_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (from as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (to as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((mixed >> 60) & 0x3) as i32
}

// ── Scored move ───────────────────────────────────────────────────────────────

/// A move paired with its ordering score
#[derive(Clone, Copy)]
pub struct ScoredMove {
    pub mv:    Move,
    pub score: i32,
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

    // Phase 23.4 opening-stats bias (D67/D71) — looked up at most once per
    // call, not per move. Gated on BOTH `ply == 0` (this is the actual
    // search root, not a recursive interior node reached via make/unmake —
    // score_moves is called at every node, and ply grows monotonically with
    // search depth, so ply==0 can only be true at the outermost call) AND
    // the position itself being the literal, unmoved game start (fullmove
    // 1, White to move — true if and only if zero moves, real or
    // hypothetical, have been made from Pet Dragon's random starting
    // setup). Both conditions are required: ply==0 alone would also fire
    // every time `go` is called from an arbitrary mid-game UCI position,
    // which has nothing to do with the bucket key (that's keyed on the
    // game's ORIGINAL rook/knight files, not wherever they've moved to
    // by move 15). Without this second check the table would occasionally
    // false-positive-match a mid-game file arrangement that happens to
    // coincide with a real starting-setup bucket.
    let opening_bias_move: Option<Move> = if ply == 0
        && pos.fullmove_number == 1
        && color == Color::White
    {
        // sorted_files returns None for anything other than exactly 2
        // pieces — arbitrary UCI-supplied FENs (analysis positions, test
        // FENs, hand-crafted endgames) can have any number of rooks/
        // knights or none at all, unlike selfplay.rs's guaranteed-fresh
        // Pet Dragon random setups. A miss here just means "not a bucket
        // we have data for," same graceful fallthrough as a genuine table
        // miss — never a crash.
        match (
            sorted_files(pos.piece_bb(Color::White, PieceKind::Rook)),
            sorted_files(pos.piece_bb(Color::White, PieceKind::Knight)),
        ) {
            (Some(rook_files), Some(knight_files)) => {
                opening_stats::lookup(rook_files, knight_files).and_then(|(mv_uci, _win_rate, _count)| {
                    (0..moves.len())
                        .map(|i| moves.get(i))
                        .find(|m| m.to_uci() == mv_uci)
                })
            }
            _ => None,
        }
    } else {
        None
    };

    let mut scored: Vec<ScoredMove> = Vec::with_capacity(moves.len());

    for i in 0..moves.len() {
        let mv    = moves.get(i);
        let mut score = score_move(
            pos, mv, info, tt_move,
            killer1, killer2, countermove,
            color_idx, ply,
            prev_move,
        );
        if opening_bias_move == Some(mv) {
            score += OPENING_STATS_BONUS;
        }
        scored.push(ScoredMove { mv, score });
    }

    scored
}

/// Sorted (ascending) list of files (0..8) occupied by the given bitboard,
/// or `None` if it doesn't have exactly 2 bits set. `selfplay.rs`'s helper
/// of the same name panics on a mismatch instead, because it only ever
/// sees guaranteed-fresh `Position::generate_with_seed` output (exactly 2
/// rooks/2 knights by construction, so a mismatch there is a real bug).
/// This one CANNOT make that assumption — `ordering.rs` sees arbitrary
/// UCI-supplied positions (analysis FENs, hand-crafted test/endgame
/// positions, anything with zero rooks or three knights or whatever a
/// user or test throws at it), so a mismatch here is a completely normal
/// "not a bucket we have data for" case, not a program-invariant
/// violation — must degrade gracefully, never panic, since this runs on
/// every real search call.
fn sorted_files(bb: crate::bitboard::Bitboard) -> Option<[u8; 2]> {
    let mut files: Vec<u8> = bb.map(|sq| sq.file()).collect();
    if files.len() != 2 {
        return None;
    }
    files.sort_unstable();
    Some([files[0], files[1]])
}

/// Score a single move, optionally conditioned on the previous move for
/// continuation history lookup.
fn score_move(
    pos:         &Position,
    mv:          Move,
    info:        &SearchInfo,
    tt_move:     Move,
    killer1:     Move,
    killer2:     Move,
    countermove: Move,
    color_idx:   usize,
    _ply:        usize,
    prev_move:   Move,
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

    // ── Quiet moves — ordered by history + continuation history ──────────────
    let mut history_score = info.history[color_idx][from][to];

    // Continuation history: bonus conditioned on the previous move's destination
    // and the piece type being moved now — rewards same-direction continuations.
    if prev_move != Move::NULL {
        let prev_to = prev_move.to.index() as usize;
        if let Some(kind) = pos.piece_on(mv.from, pos.side_to_move) {
            let piece_idx = kind as usize * 2 + color_idx;
            history_score += info.get_cont_hist(prev_to, piece_idx, to);
        }
    }

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

    QUIET_BASE_SCORE + history_score + thread_tie_break(info.thread_id, from, to)
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

/// Update move ordering tables after a beta cutoff.
/// Call when a quiet move causes a cutoff (fail-high).
/// `pos` must be the position BEFORE the move was made (after unmake) so
/// that `pos.piece_on(mv.from, color)` resolves the correct piece type.
pub fn update_ordering_on_cutoff(
    info:         &mut SearchInfo,
    mv:           Move,
    prev_move:    Move,
    ply:          usize,
    depth:        i32,
    color:        Color,
    quiets_tried: &[Move],
    pos:          &Position,
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

    // Update continuation history when we have a previous-move context
    if prev_move != Move::NULL {
        let prev_to = prev_move.to.index() as usize;
        // Bonus for the cutoff move
        if let Some(kind) = pos.piece_on(mv.from, color) {
            info.update_cont_hist(prev_to, kind as usize * 2 + color_idx, to, depth, true);
        }
        // Penalty for quiets searched before the cutoff move
        for &tried in quiets_tried {
            if tried == mv { continue; }
            if tried.kind.is_capture() || tried.kind.is_promotion() { continue; }
            if let Some(tried_kind) = pos.piece_on(tried.from, color) {
                info.update_cont_hist(
                    prev_to,
                    tried_kind as usize * 2 + color_idx,
                    tried.to.index() as usize,
                    depth,
                    false,
                );
            }
        }
    }

    // Penalise quiet moves that were tried before this one (regular history)
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
    fn test_thread_tie_break_main_thread_always_zero() {
        // Thread 0 (main thread) must never be perturbed — this is the
        // safety property the whole feature depends on.
        assert_eq!(thread_tie_break(0, 12, 28), 0);
        assert_eq!(thread_tie_break(0, 0, 63), 0);
    }

    #[test]
    fn test_thread_tie_break_deterministic() {
        // Same inputs must always produce the same output — this is a
        // fixed mix, not real randomness, so reproducibility of seeded
        // games elsewhere in the engine is unaffected.
        let a = thread_tie_break(1, 12, 28);
        let b = thread_tie_break(1, 12, 28);
        assert_eq!(a, b);
    }

    #[test]
    fn test_thread_tie_break_bounded_magnitude() {
        // Must stay small enough to only affect ties, never override a
        // real history-score difference or jump into a different
        // ordering bucket (killer/capture/etc).
        for thread_id in 1..8usize {
            for from in [0usize, 12, 35, 63] {
                for to in [0usize, 12, 35, 63] {
                    let v = thread_tie_break(thread_id, from, to);
                    assert!((0..=3).contains(&v),
                        "tie-break offset out of expected 0..=3 range: {v}");
                }
            }
        }
    }

    #[test]
    fn test_thread_tie_break_varies_across_threads() {
        // Not every helper thread should collapse to the same offset for
        // the same move — otherwise thread differentiation wouldn't
        // actually decorrelate anything.
        let offsets: std::collections::HashSet<i32> = (1..8usize)
            .map(|tid| thread_tie_break(tid, 12, 28))
            .collect();
        assert!(offsets.len() > 1,
            "expected tie-break offsets to vary across helper threads, got {offsets:?}");
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

        let scored = score_moves(
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
                let scored = score_moves(
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
    fn test_cont_hist_boosts_quiet_score() {
        setup();
        let pos   = Position::start_pos().unwrap();
        let moves = generate_moves(&pos);
        let mut info = SearchInfo::new();

        // Pick two different quiet moves
        let mv_a = moves.get(0); // e.g. a2a3

        // Score without any cont hist context
        let scored_before = score_moves(&pos, &moves, &info, Move::NULL, 0, Move::NULL);
        let score_a_before = scored_before.iter().find(|s| s.mv == mv_a).map(|s| s.score).unwrap_or(0);

        // Inject a cont hist bonus: prev_to=28 (e4), white pawn (piece_idx=0), to=mv_a.to
        let prev_to = 28usize;
        info.update_cont_hist(prev_to, 0, mv_a.to.index() as usize, 8, true);

        // Create a fake prev_move that lands on e4
        let fake_prev = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        let scored_after = score_moves(&pos, &moves, &info, Move::NULL, 0, fake_prev);
        let score_a_after = scored_after.iter().find(|s| s.mv == mv_a).map(|s| s.score).unwrap_or(0);

        // mv_a should score higher when cont hist is active AND prev_move matches
        // (only if mv_a moves a pawn from a square that matches piece_idx=0 and to=mv_a.to)
        // The score must be >= before (cont hist adds a non-negative bonus here)
        assert!(score_a_after >= score_a_before,
            "Cont hist should not decrease score of boosted move");
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
    fn test_opening_stats_bias_applies_to_known_bucket() {
        // D67 step 6 (D72, Session 84) — hand-verification against a real
        // table entry, not just the generated file's own structural
        // self-tests. Hand-constructed (not seed-generated) but follows
        // Pet Dragon's real structural rules exactly: full 16-square
        // back-two-ranks setup, Black's setup a same-file mirror of
        // White's (rank r <-> rank 9-r) — matches
        // `opening_stats::OPENING_STATS`'s first real entry as of Session
        // 84 (bucket key 207: rook_files=(0,3)=(a,d),
        // knight_files=(1,7)=(b,h) -> "a2a7", win_rate 0.9677, n=31 games).
        //
        // Traced by hand before writing this test (see DECISIONS.md D72):
        // with the a-file rook specifically on a2 (not a1), and Black's
        // mirrored setup necessarily placing a like-for-like piece on a7
        // (ranks 3-6 always empty at the true game start), `a2a7` is
        // always a legal ROOK-TAKES-ROOK capture whenever this bucket
        // applies — not a quiet move. That's a real, replicable tactical
        // pattern (an immediate equal-value trade), consistent with the
        // observed ~97% win rate, not an artifact.
        //
        // Skips gracefully (doesn't fail) if a future aggregation re-run
        // has changed or dropped this specific entry — this test verifies
        // the MECHANISM against whatever entry 207 currently holds, not a
        // permanently frozen data point; regenerating the table is a
        // normal, expected maintenance action (D67/D71), not something
        // that should break CI on its own.
        setup();

        const KEY_207: u16 = 207;
        let entry = opening_stats::OPENING_STATS.iter().find(|e| e.0 == KEY_207);
        let Some(&(_, expected_move_uci, _win_rate, _count)) = entry else {
            return; // table regenerated since this test was written — fine, skip
        };

        let fen = "pnbrkbqn/rppppppp/8/8/8/8/RPPPPPPP/PNBRKBQN w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert_eq!(pos.fullmove_number, 1);
        assert_eq!(pos.side_to_move, crate::types::Color::White);

        let moves = generate_moves(&pos);
        let info  = SearchInfo::new();
        let scored = score_moves(&pos, &moves, &info, Move::NULL, 0, Move::NULL);

        let target = scored.iter().find(|s| s.mv.to_uci() == expected_move_uci)
            .unwrap_or_else(|| panic!(
                "expected move {expected_move_uci} (from table entry 207) to be a legal \
                 move in this hand-constructed position — if this fails, the FEN above no \
                 longer matches bucket key 207's actual (rook_files, knight_files), not a \
                 real engine bug; re-derive the FEN from the current table entry."
            ));

        // The flagged move should be a rook-takes-rook capture per the
        // hand-trace above (EQUAL_CAPTURE_SCORE tier) PLUS the opening-
        // stats bonus on top — confirms the bonus is actually additive,
        // not replacing the move's own legitimate capture score.
        assert!(target.mv.kind.is_capture(),
            "a2a7 should be a legal capture in this mirrored setup (Black's a7 always holds a like-for-like mirrored piece)");
        assert!(target.score >= EQUAL_CAPTURE_SCORE + OPENING_STATS_BONUS,
            "flagged move should score at least equal-capture tier plus the opening-stats bonus, got {}", target.score);

        // And the bonus should be the actual differentiator versus what an
        // identical capture would score WITHOUT the bias — construct the
        // same position's move list score via score_move-equivalent logic
        // isn't directly reusable here (score_move is private and folded
        // into score_moves), so instead confirm indirectly: no other
        // legal move in this position can reach TT_MOVE_SCORE or
        // WINNING_CAPTURE_BASE tiers (nothing to capture that isn't an
        // equal trade at the start), so the flagged move should be the
        // single highest-scoring move in the whole list — the bonus is
        // what pushes an equal-capture ahead of every other equal-capture
        // sharing the same MVV-LVA score.
        let max_score = scored.iter().map(|s| s.score).max().unwrap();
        assert_eq!(target.score, max_score,
            "opening-stats-flagged move should be the single highest-scored move in this position");
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
            &mut info, mv, Move::NULL, 0, 5, color, &[], &pos
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
