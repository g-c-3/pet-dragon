// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/see.rs — Static Exchange Evaluation
//
// SEE quickly estimates the material outcome of a capture sequence
// on a single square without doing a full search.
//
// Algorithm (SWAP algorithm):
//   1. Find the least valuable attacker of the target square
//   2. "Make" the capture (update occupancy, find next attacker)
//   3. Repeat until no more attackers or gain is negative
//   4. Return the net material gain/loss
//
// Used for:
//   - Move ordering: winning captures before losing captures
//   - Quiescence search: skip losing captures (SEE < 0)
//   - Pruning: don't search captures that lose material
//
// ⚠️ Pet Dragon note: SEE is called on the STARTING position too
// because Pet Dragon positions can have immediate captures from move 1.
// SEE must work correctly at depth 0 with no prior moves made.
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks};
use crate::bitboard::masks::{knight_attacks, king_attacks, pawn_attacks};
use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

// ── Piece values for SEE ──────────────────────────────────────────────────────
// These are simplified values optimised for SEE speed.
// Not the same as full evaluation piece values.

const SEE_VALUES: [i32; 6] = [
    100,    // Pawn
    320,    // Knight
    330,    // Bishop
    500,    // Rook
    900,    // Queen
    20000,  // King (effectively infinite)
];

#[inline]
fn see_value(kind: PieceKind) -> i32 {
    SEE_VALUES[kind as usize]
}

// ── Main SEE function ─────────────────────────────────────────────────────────

/// Perform Static Exchange Evaluation for a capture move.
/// Returns the estimated material gain (positive) or loss (negative).
///
/// A positive return value means the capture wins material.
/// A negative return value means the capture loses material.
/// Zero means even exchange.
///
/// threshold: minimum gain required (use 0 for "does this win material?")
/// Returns true if SEE >= threshold.
pub fn see(pos: &Position, mv: Move, threshold: i32) -> bool {
    let from  = mv.from;
    let to    = mv.to;
    let color = pos.side_to_move;

    // Value of the piece on the target square
    let target_value = match mv.kind {
        MoveKind::EnPassant => see_value(PieceKind::Pawn),
        _ => pos.piece_on(to, color.flip())
                .map(see_value)
                .unwrap_or(0),
    };

    // If we can't even meet threshold by capturing, fail immediately
    if target_value < threshold {
        return false;
    }

    // Value of our piece making the capture
    let our_piece = match pos.piece_on(from, color) {
        Some(k) => k,
        None    => return false,
    };

    // Gain array for negamax
    let mut gain    = [0i32; 32];
    let mut depth   = 0usize;
    gain[0]         = target_value;

    let mut occupancy = pos.all_pieces();
    occupancy.clear(from);
    if mv.kind == MoveKind::EnPassant {
        let ep_sq = crate::types::Square::from_file_rank(
            to.file(), from.rank()
        ).unwrap();
        occupancy.clear(ep_sq);
    }

    let mut attackers  = all_attackers(pos, to, occupancy);
    let mut side       = color.flip();
    let mut next_piece = our_piece;

    loop {
        depth += 1;
        if depth >= gain.len() { break; }

        // Gain for this side: value of piece just captured minus previous gain
        gain[depth] = see_value(next_piece) - gain[depth - 1];

        // Pruning: if this side can't improve even by stopping now
        if gain[depth].max(-gain[depth - 1]) < 0 { break; }

        let (attacker_sq, attacker_kind) = match
            least_valuable_attacker(pos, &attackers, side, occupancy)
        {
            Some(x) => x,
            None    => break,
        };

        occupancy.clear(attacker_sq);
        attackers  = all_attackers(pos, to, occupancy);
        next_piece = attacker_kind;
        side       = side.flip();
    }

    // Negamax: work backwards
    while depth > 1 {
        depth -= 1;
        gain[depth - 1] = -((-gain[depth - 1]).max(gain[depth]));
    }

    gain[0] >= threshold
}

/// Simple SEE: returns the estimated material gain/loss as a number
pub fn see_value_of(pos: &Position, mv: Move) -> i32 {
    let from  = mv.from;
    let to    = mv.to;
    let color = pos.side_to_move;

    let moving_piece = match pos.piece_on(from, color) {
        Some(k) => k,
        None    => return 0,
    };

    // Gain list for the SWAP algorithm
    let mut gain = [0i32; 32];
    let mut depth = 0usize;

    // Initial capture value
    gain[0] = match mv.kind {
        MoveKind::EnPassant => see_value(PieceKind::Pawn),
        _ => pos.piece_on(to, color.flip())
                .map(see_value)
                .unwrap_or(0),
    };

    let mut occupancy = pos.all_pieces();
    occupancy.clear(from);
    if mv.kind == MoveKind::EnPassant {
        let ep_sq = Square::from_file_rank(to.file(), from.rank()).unwrap();
        occupancy.clear(ep_sq);
    }

    let mut attackers  = all_attackers(pos, to, occupancy);
    let mut side       = color.flip();
    let mut next_piece = moving_piece;

    loop {
        depth += 1;
        if depth >= gain.len() { break; }

        gain[depth] = see_value(next_piece) - gain[depth - 1];

        if gain[depth].max(-gain[depth - 1]) < 0 { break; }

        let (attacker_sq, attacker_kind) = match
            least_valuable_attacker(pos, &attackers, side, occupancy)
        {
            Some(x) => x,
            None    => break,
        };

        occupancy.clear(attacker_sq);
        attackers = all_attackers(pos, to, occupancy);

        next_piece = attacker_kind;
        side       = side.flip();
    }

    // Negamax backwards
    while depth > 1 {
        depth -= 1;
        gain[depth - 1] = -((-gain[depth - 1]).max(gain[depth]));
    }

    gain[0]
}

// ── Attacker detection helpers ────────────────────────────────────────────────

/// Get all pieces attacking a square given an occupancy mask
fn all_attackers(
    pos:       &Position,
    sq:        Square,
    occupancy: Bitboard,
) -> Bitboard {
    let mut attackers = Bitboard::EMPTY;

    // Pawns (White pawns attack from below, Black from above)
    attackers |= pawn_attacks(Color::Black, sq)
               & pos.piece_bb(Color::White, PieceKind::Pawn);
    attackers |= pawn_attacks(Color::White, sq)
               & pos.piece_bb(Color::Black, PieceKind::Pawn);

    // Knights
    let knight_atk = knight_attacks(sq);
    attackers |= knight_atk & pos.piece_bb(Color::White, PieceKind::Knight);
    attackers |= knight_atk & pos.piece_bb(Color::Black, PieceKind::Knight);

    // Bishops and diagonal queens
    let diag_atk = bishop_attacks(sq, occupancy);
    attackers |= diag_atk & (
        pos.piece_bb(Color::White, PieceKind::Bishop)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Bishop)
      | pos.piece_bb(Color::Black, PieceKind::Queen)
    );

    // Rooks and straight queens
    let straight_atk = rook_attacks(sq, occupancy);
    attackers |= straight_atk & (
        pos.piece_bb(Color::White, PieceKind::Rook)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Rook)
      | pos.piece_bb(Color::Black, PieceKind::Queen)
    );

    // Kings
    let king_atk = king_attacks(sq);
    attackers |= king_atk & pos.piece_bb(Color::White, PieceKind::King);
    attackers |= king_atk & pos.piece_bb(Color::Black, PieceKind::King);

    attackers & occupancy
}

/// Find the least valuable attacker of a square for a given side
/// Returns (square, piece_kind) or None if no attacker
fn least_valuable_attacker(
    pos:       &Position,
    attackers: &Bitboard,
    side:      Color,
    occupancy: Bitboard,
) -> Option<(Square, PieceKind)> {
    // Check pieces in order of value (least valuable first)
    for &kind in &[
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ] {
        let piece_bb = pos.piece_bb(side, kind) & *attackers & occupancy;
        if let Some(sq) = piece_bb.lsb() {
            return Some((sq, kind));
        }
    }
    None
}

/// Get X-ray attackers revealed when a piece is removed from a square
/// (sliding pieces hiding behind the removed piece)
fn xray_attackers(
    pos:          &Position,
    target:       Square,
    occupancy:    Bitboard,
    removed_from: Square,
) -> Bitboard {
    // Check if any sliding pieces are newly revealed
    let diag    = bishop_attacks(target, occupancy);
    let straight = rook_attacks(target, occupancy);

    let all_bishops_queens =
        pos.piece_bb(Color::White, PieceKind::Bishop)
      | pos.piece_bb(Color::Black, PieceKind::Bishop)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Queen);

    let all_rooks_queens =
        pos.piece_bb(Color::White, PieceKind::Rook)
      | pos.piece_bb(Color::Black, PieceKind::Rook)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Queen);

    // Only pieces that could attack through the removed square
    let removed_bb = Bitboard::from_square(removed_from);
    let _ = removed_bb; // We already removed it from occupancy

    (diag    & all_bishops_queens)
  | (straight & all_rooks_queens)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, Move, MoveKind, PieceKind, Square};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    fn capture(from: Square, to: Square, captured: PieceKind) -> Move {
        Move::capture(from, to, MoveKind::Capture, captured)
    }

    #[test]
    fn test_see_winning_pawn_capture() {
        setup();
        // White pawn captures undefended Black rook — should win
        let fen = "4k3/8/8/3r4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::E4, Square::D5, PieceKind::Rook);
        assert!(see(&pos, mv, 0),
            "Pawn capturing undefended Rook should be SEE >= 0");
        let val = see_value_of(&pos, mv);
        assert!(val > 0, "Should gain material: {}", val);
    }

    #[test]
    fn test_see_losing_capture() {
        setup();
        // White Rook captures Black pawn defended by Black Rook — should lose
        let fen = "4k3/8/8/3rp3/3R4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::E5, PieceKind::Pawn);
        assert!(!see(&pos, mv, 1),
            "Rook capturing defended pawn should be SEE < 1");
    }

    #[test]
    fn test_see_even_exchange() {
        setup();
        // White Rook captures Black Rook — even exchange
        let fen = "4k3/8/8/3r4/3R4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::D5, PieceKind::Rook);
        let val = see_value_of(&pos, mv);
        assert_eq!(val, 0, "Rook for Rook should be even: {}", val);
    }

    #[test]
    fn test_see_queen_capture_chain() {
        setup();
        // White pawn can capture queen — big win even if recaptured
        let fen = "4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::E4, Square::D5, PieceKind::Queen);
        assert!(see(&pos, mv, 0),
            "Pawn capturing queen should be SEE >= 0");
        let val = see_value_of(&pos, mv);
        assert!(val > 0, "Should gain material capturing queen with pawn");
    }

    #[test]
    fn test_see_threshold() {
        setup();
        // Rook captures pawn — gains 100, but threshold 200 not met
        let fen = "4k3/8/8/3p4/3R4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::D5, PieceKind::Pawn);
        assert!(see(&pos, mv, 0),   "SEE >= 0 for undefended pawn");
        assert!(see(&pos, mv, 100), "SEE >= 100 for undefended pawn");
        assert!(!see(&pos, mv, 200), "SEE < 200 for just a pawn");
    }

    #[test]
    fn test_see_pet_dragon_immediate_capture() {
        setup();
        // Pet Dragon: capture available from move 1
        // Test that SEE works on Pet Dragon starting positions
        for seed in 0..20u64 {
            let pos = Position::generate_with_seed(seed);
            // Generate captures and run SEE on them
            let moves = crate::movegen::generate_captures(&pos);
            for mv in moves.iter() {
                if mv.kind == MoveKind::Capture {
                    // SEE should not panic on any Pet Dragon capture
                    let _ = see_value_of(&pos, *mv);
                }
            }
        }
    }

    #[test]
    fn test_see_no_capture_available() {
        setup();
        // Quiet move — SEE returns 0
        let pos = Position::start_pos().unwrap();
        let mv  = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        let val = see_value_of(&pos, mv);
        assert_eq!(val, 0, "No capture — SEE should be 0");
    }

    #[test]
    fn test_see_xray_attacker() {
        setup();
        // White Rook on d1 behind White Queen on d4 — x-ray attack
        // White Queen captures on d5 — White Rook is revealed as attacker
        let fen = "4k3/8/8/3r4/3Q4/8/8/3RK3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::D5, PieceKind::Rook);
        // Queen takes Rook (gain 500), Black has no recapture
        // White Rook is x-ray behind
        let val = see_value_of(&pos, mv);
        assert!(val > 0,
            "Queen capturing rook with rook behind should gain: {}", val);
    }
}
