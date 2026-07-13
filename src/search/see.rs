// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/see.rs — Static Exchange Evaluation
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks};
use crate::bitboard::masks::{knight_attacks, king_attacks, pawn_attacks};
use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

const SEE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 20000];

#[inline]
fn see_value(kind: PieceKind) -> i32 {
    SEE_VALUES[kind as usize]
}

/// SEE: returns true if the capture sequence result >= threshold
pub fn see(pos: &Position, mv: Move, threshold: i32) -> bool {
    let from  = mv.from;
    let to    = mv.to;
    let color = pos.side_to_move;

    let target_value = match mv.kind {
        MoveKind::EnPassant => see_value(PieceKind::Pawn),
        _ => pos.piece_on(to, color.flip())
                .map(see_value)
                .unwrap_or(0),
    };

    if target_value < threshold {
        return false;
    }

    let our_piece = match pos.piece_on(from, color) {
        Some(k) => k,
        None    => return false,
    };

    let mut gain  = [0i32; 32];
    let mut depth = 0usize;
    gain[0]       = target_value;

    let mut occupancy = pos.all_pieces();
    occupancy.clear(from);
    if mv.kind == MoveKind::EnPassant {
        let ep_sq = Square::from_file_rank(to.file(), from.rank()).unwrap();
        occupancy.clear(ep_sq);
    }

    let mut attackers  = all_attackers(pos, to, occupancy);
    let mut side       = color.flip();
    let mut next_piece = our_piece;

    loop {
        let (attacker_sq, attacker_kind) = match
            least_valuable_attacker(pos, &attackers, side, occupancy)
        {
            Some(x) => x,
            None    => break,
        };

        depth += 1;
        if depth >= gain.len() { break; }

        gain[depth] = see_value(next_piece) - gain[depth - 1];

        if gain[depth].max(-gain[depth - 1]) < 0 { break; }

        occupancy.clear(attacker_sq);
        attackers  = all_attackers(pos, to, occupancy);
        next_piece = attacker_kind;
        side       = side.flip();
    }

    while depth > 0 {
        gain[depth - 1] = -((-gain[depth - 1]).max(gain[depth]));
        depth -= 1;
    }

    gain[0] >= threshold
}

/// SEE: returns the estimated material gain/loss as a number
pub fn see_value_of(pos: &Position, mv: Move) -> i32 {
    let from  = mv.from;
    let to    = mv.to;
    let color = pos.side_to_move;

    let moving_piece = match pos.piece_on(from, color) {
        Some(k) => k,
        None    => return 0,
    };

    let mut gain  = [0i32; 32];
    let mut depth = 0usize;

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
        let (attacker_sq, attacker_kind) = match
            least_valuable_attacker(pos, &attackers, side, occupancy)
        {
            Some(x) => x,
            None    => break,
        };

        depth += 1;
        if depth >= gain.len() { break; }

        gain[depth] = see_value(next_piece) - gain[depth - 1];

        if gain[depth].max(-gain[depth - 1]) < 0 { break; }

        occupancy.clear(attacker_sq);
        attackers  = all_attackers(pos, to, occupancy);
        next_piece = attacker_kind;
        side       = side.flip();
    }

    while depth > 0 {
        gain[depth - 1] = -((-gain[depth - 1]).max(gain[depth]));
        depth -= 1;
    }

    gain[0]
}

fn all_attackers(pos: &Position, sq: Square, occupancy: Bitboard) -> Bitboard {
    let mut attackers = Bitboard::EMPTY;

    attackers |= pawn_attacks(Color::Black, sq)
               & pos.piece_bb(Color::White, PieceKind::Pawn);
    attackers |= pawn_attacks(Color::White, sq)
               & pos.piece_bb(Color::Black, PieceKind::Pawn);

    let knight_atk = knight_attacks(sq);
    attackers |= knight_atk & pos.piece_bb(Color::White, PieceKind::Knight);
    attackers |= knight_atk & pos.piece_bb(Color::Black, PieceKind::Knight);

    let diag_atk = bishop_attacks(sq, occupancy);
    attackers |= diag_atk & (
        pos.piece_bb(Color::White, PieceKind::Bishop)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Bishop)
      | pos.piece_bb(Color::Black, PieceKind::Queen)
    );

    let straight_atk = rook_attacks(sq, occupancy);
    attackers |= straight_atk & (
        pos.piece_bb(Color::White, PieceKind::Rook)
      | pos.piece_bb(Color::White, PieceKind::Queen)
      | pos.piece_bb(Color::Black, PieceKind::Rook)
      | pos.piece_bb(Color::Black, PieceKind::Queen)
    );

    let king_atk = king_attacks(sq);
    attackers |= king_atk & pos.piece_bb(Color::White, PieceKind::King);
    attackers |= king_atk & pos.piece_bb(Color::Black, PieceKind::King);

    attackers & occupancy
}

fn least_valuable_attacker(
    pos:       &Position,
    attackers: &Bitboard,
    side:      Color,
    occupancy: Bitboard,
) -> Option<(Square, PieceKind)> {
    for &kind in &[
        PieceKind::Pawn, PieceKind::Knight, PieceKind::Bishop,
        PieceKind::Rook, PieceKind::Queen,  PieceKind::King,
    ] {
        let piece_bb = pos.piece_bb(side, kind) & *attackers & occupancy;
        if let Some(sq) = piece_bb.lsb() {
            return Some((sq, kind));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Move, MoveKind, PieceKind, Square};

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
        let fen = "4k3/8/8/3r4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::E4, Square::D5, PieceKind::Rook);
        assert!(see(&pos, mv, 0));
        let val = see_value_of(&pos, mv);
        assert!(val > 0, "Should gain material: {}", val);
    }

    #[test]
    fn test_see_losing_capture() {
        setup();
        let fen = "4k3/8/8/3rp3/3R4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::E5, PieceKind::Pawn);
        assert!(!see(&pos, mv, 1),
            "Rook capturing defended pawn should be SEE < 1");
    }

    #[test]
    fn test_see_even_exchange() {
        setup();
        // White Rook takes Black Rook, Black Rook recaptures — even exchange
        // Need Black to have a recapturer — add Black Rook on d8
        let fen = "3rk3/8/8/3r4/3R4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::D5, PieceKind::Rook);
        let val = see_value_of(&pos, mv);
        assert_eq!(val, 0, "Rook for Rook should be even: {}", val);
    }

    #[test]
    fn test_see_queen_capture_chain() {
        setup();
        let fen = "4k3/8/8/3q4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::E4, Square::D5, PieceKind::Queen);
        assert!(see(&pos, mv, 0));
        let val = see_value_of(&pos, mv);
        assert!(val > 0, "Should gain material capturing queen with pawn");
    }

    #[test]
    fn test_see_threshold() {
        setup();
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
        for seed in 0..20u64 {
            let pos = Position::generate_with_seed(seed);
            let moves = crate::movegen::generate_captures(&pos);
            for mv in moves.iter() {
                if mv.kind == MoveKind::Capture {
                    let _ = see_value_of(&pos, *mv);
                }
            }
        }
    }

    #[test]
    fn test_see_no_capture_available() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mv  = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        let val = see_value_of(&pos, mv);
        assert_eq!(val, 0);
    }

    #[test]
    fn test_see_xray_attacker() {
        setup();
        let fen = "4k3/8/8/3r4/3Q4/8/8/3RK3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mv  = capture(Square::D4, Square::D5, PieceKind::Rook);
        let val = see_value_of(&pos, mv);
        assert!(val > 0,
            "Queen capturing rook with rook behind should gain: {}", val);
    }
}
