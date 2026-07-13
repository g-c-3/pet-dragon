// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// movegen/castling.rs — Castling move generation
//
// Pet Dragon castling rules (confirmed):
//   Castling is available ONLY if the King and Rook(s) happen to start
//   on their standard chess squares.
//
//   White: King on e1 AND Rook on h1 (kingside) or a1 (queenside)
//   Black: King on e8 AND Rook on h8 (kingside) or a8 (queenside)
//
//   Since the White King is ALWAYS on e1 (and Black King always e8),
//   castling availability depends entirely on where Rooks randomly landed.
//   This was detected during setup and stored in CastlingRights.
//
// All standard castling conditions apply:
//   1. Neither King nor Rook has previously moved
//      (tracked via CastlingRights — cleared when piece moves)
//   2. No pieces between King and Rook
//   3. King is not currently in check
//   4. King does not pass through a square attacked by opponent
//   5. King does not land on a square attacked by opponent
//
// Castling squares:
//   White kingside:  King e1→g1, Rook h1→f1, pass through f1
//   White queenside: King e1→c1, Rook a1→d1, pass through d1
//   Black kingside:  King e8→g8, Rook h8→f8, pass through f8
//   Black queenside: King e8→c8, Rook a8→d8, pass through d8
// ============================================================================

use crate::movegen::MoveList;
use crate::position::Position;
use crate::types::{Color, Move, MoveKind, Square};

// ── Castling square constants ─────────────────────────────────────────────────

// Squares that must be empty for kingside castling
const WHITE_KINGSIDE_EMPTY:  [Square; 2] = [Square::F1, Square::G1];
const WHITE_QUEENSIDE_EMPTY: [Square; 3] = [Square::B1, Square::C1, Square::D1];
const BLACK_KINGSIDE_EMPTY:  [Square; 2] = [Square::F8, Square::G8];
const BLACK_QUEENSIDE_EMPTY: [Square; 3] = [Square::B8, Square::C8, Square::D8];

// Squares the King passes through (must not be attacked)
// Includes King's starting square and destination
const WHITE_KINGSIDE_SAFE:  [Square; 3] = [Square::E1, Square::F1, Square::G1];
const WHITE_QUEENSIDE_SAFE: [Square; 3] = [Square::E1, Square::D1, Square::C1];
const BLACK_KINGSIDE_SAFE:  [Square; 3] = [Square::E8, Square::F8, Square::G8];
const BLACK_QUEENSIDE_SAFE: [Square; 3] = [Square::E8, Square::D8, Square::C8];

// ── Main entry point ──────────────────────────────────────────────────────────

/// Generate castling moves for the given color
/// Only generates castling if:
///   1. Castling rights exist (Rook started on standard square)
///   2. Path is clear
///   3. King not in check, not passing through check
pub fn generate_castling_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    match color {
        Color::White => {
            if pos.castling.white_kingside {
                try_castle_kingside(pos, color,
                    Square::E1, Square::G1,
                    &WHITE_KINGSIDE_EMPTY,
                    &WHITE_KINGSIDE_SAFE,
                    list,
                );
            }
            if pos.castling.white_queenside {
                try_castle_queenside(pos, color,
                    Square::E1, Square::C1,
                    &WHITE_QUEENSIDE_EMPTY,
                    &WHITE_QUEENSIDE_SAFE,
                    list,
                );
            }
        }
        Color::Black => {
            if pos.castling.black_kingside {
                try_castle_kingside(pos, color,
                    Square::E8, Square::G8,
                    &BLACK_KINGSIDE_EMPTY,
                    &BLACK_KINGSIDE_SAFE,
                    list,
                );
            }
            if pos.castling.black_queenside {
                try_castle_queenside(pos, color,
                    Square::E8, Square::C8,
                    &BLACK_QUEENSIDE_EMPTY,
                    &BLACK_QUEENSIDE_SAFE,
                    list,
                );
            }
        }
    }
}

// ── Castling attempt helpers ──────────────────────────────────────────────────

fn try_castle_kingside(
    pos:        &Position,
    color:      Color,
    king_from:  Square,
    king_to:    Square,
    empty_sqs:  &[Square],
    safe_sqs:   &[Square],
    list:       &mut MoveList,
) {
    // Check all squares between King and Rook are empty
    if !squares_empty(pos, empty_sqs) {
        return;
    }

    // Check King is not in check and doesn't pass through check
    let attacker = color.flip();
    if squares_attacked(pos, safe_sqs, attacker) {
        return;
    }

    list.push(Move::new(king_from, king_to, MoveKind::CastleKing));
}

fn try_castle_queenside(
    pos:        &Position,
    color:      Color,
    king_from:  Square,
    king_to:    Square,
    empty_sqs:  &[Square],
    safe_sqs:   &[Square],
    list:       &mut MoveList,
) {
    // Check all squares between King and Rook are empty
    if !squares_empty(pos, empty_sqs) {
        return;
    }

    // Check King is not in check and doesn't pass through check
    let attacker = color.flip();
    if squares_attacked(pos, safe_sqs, attacker) {
        return;
    }

    list.push(Move::new(king_from, king_to, MoveKind::CastleQueen));
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Are all given squares empty?
#[inline]
fn squares_empty(pos: &Position, squares: &[Square]) -> bool {
    squares.iter().all(|&sq| pos.piece_at(sq).is_none())
}

/// Is any of the given squares attacked by the attacker?
#[inline]
fn squares_attacked(
    pos:      &Position,
    squares:  &[Square],
    attacker: Color,
) -> bool {
    squares.iter().any(|&sq| pos.is_attacked(sq, attacker))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::movegen::MoveList;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, MoveKind};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_no_castling_at_standard_start() {
        setup();
        // At standard start, pieces block castling path
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0,
            "No castling available at standard start (path blocked)");
    }

    #[test]
    fn test_kingside_castling_available() {
        setup();
        // King and kingside Rook in place, path clear
        let fen = "4k3/8/8/8/8/8/8/4K2R w K - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 1,
            "White kingside castling should be available");
        assert_eq!(list.get(0).kind, MoveKind::CastleKing);
        assert_eq!(list.get(0).from, Square::E1);
        assert_eq!(list.get(0).to,   Square::G1);
    }

    #[test]
    fn test_queenside_castling_available() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 1,
            "White queenside castling should be available");
        assert_eq!(list.get(0).kind, MoveKind::CastleQueen);
        assert_eq!(list.get(0).from, Square::E1);
        assert_eq!(list.get(0).to,   Square::C1);
    }

    #[test]
    fn test_both_castling_available() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/R3K2R w KQ - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 2,
            "Both castling moves should be available");
    }

    #[test]
    fn test_castling_blocked_by_piece() {
        setup();
        // Knight on f1 blocks kingside castling
        let fen = "4k3/8/8/8/8/8/8/4KN1R w K - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0,
            "Castling blocked by piece on f1");
    }

    #[test]
    fn test_castling_blocked_by_check() {
        setup();
        // King in check — cannot castle
        let fen = "4k3/8/8/8/8/8/4r3/4K2R w K - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0,
            "Cannot castle while in check");
    }

    #[test]
    fn test_castling_blocked_by_attacked_square() {
        setup();
        // Rook attacks f1 — King would pass through attacked square
        let fen = "4k3/8/8/8/8/8/5r2/4K2R w K - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0,
            "Cannot castle through attacked square f1");
    }

    #[test]
    fn test_black_kingside_castling() {
        setup();
        let fen = "4k2r/8/8/8/8/8/8/4K3 b k - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::Black, &mut list);
        assert_eq!(list.len(), 1,
            "Black kingside castling should be available");
        assert_eq!(list.get(0).from, Square::E8);
        assert_eq!(list.get(0).to,   Square::G8);
    }

    #[test]
    fn test_black_queenside_castling() {
        setup();
        let fen = "r3k3/8/8/8/8/8/8/4K3 b q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::Black, &mut list);
        assert_eq!(list.len(), 1,
            "Black queenside castling should be available");
        assert_eq!(list.get(0).from, Square::E8);
        assert_eq!(list.get(0).to,   Square::C8);
    }

    #[test]
    fn test_pet_dragon_castling_probability() {
        setup();
        // In Pet Dragon ~26% of games should have at least one castling option
        let mut any_castling = 0u32;
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            if pos.castling.white_kingside
            || pos.castling.white_queenside {
                any_castling += 1;
            }
        }
        // Should be roughly 26% — allow wide margin
        assert!(any_castling > 100 && any_castling < 900,
            "Castling availability should be ~26%, got {}%",
            any_castling / 10);
    }

    #[test]
    fn test_no_castling_rights_no_moves() {
        setup();
        // Position with no castling rights — no castling moves generated
        let fen = "4k3/8/8/8/8/8/8/R3K2R w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_castling_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0,
            "No castling moves when rights not set");
    }
}
