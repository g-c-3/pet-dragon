// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// movegen/mod.rs — Move generation entry point
//
// This module coordinates all move generation for Pet Dragon.
// The main entry point is generate_moves() which returns all legal
// moves in a position.
//
// Architecture:
//   1. Generate pseudo-legal moves (moves that follow piece movement
//      rules but may leave the King in check)
//   2. Filter to legal moves (remove any that leave King in check)
//
// Pet Dragon custom move generation:
//   - Pawns: double-step from actual starting square (rank 1 OR rank 2)
//   - Castling: only when Rook started on standard square
//   - Everything else: identical to standard chess
//
// Move list is stored as a fixed-size array for performance.
// Vec allocation on every node would be too slow at 1M+ NPS.
// ============================================================================

pub mod pawns;
pub mod pieces;
pub mod castling;
pub mod legal;

use crate::position::Position;
use crate::types::Move;

// ── Move list ─────────────────────────────────────────────────────────────────
// Fixed-size array avoids heap allocation during search.
// 256 moves is more than enough — maximum legal moves in any chess
// position is around 218. We use 256 for safety margin.

pub const MAX_MOVES: usize = 256;

/// A list of moves with a count.
/// Avoids Vec allocation — critical for search performance.
#[derive(Clone)]
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    count: usize,
}

impl MoveList {
    /// Create an empty move list
    #[inline]
    pub fn new() -> Self {
        MoveList {
            moves: [Move::NULL; MAX_MOVES],
            count: 0,
        }
    }

    /// Add a move to the list
    #[inline]
    pub fn push(&mut self, mv: Move) {
        debug_assert!(self.count < MAX_MOVES, "Move list overflow");
        self.moves[self.count] = mv;
        self.count += 1;
    }

    /// Number of moves in the list
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Is the list empty?
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get a move by index
    #[inline]
    pub fn get(&self, index: usize) -> Move {
        self.moves[index]
    }

    /// Iterate over moves
    pub fn iter(&self) -> impl Iterator<Item = &Move> {
        self.moves[..self.count].iter()
    }

    /// Clear the list
    #[inline]
    pub fn clear(&mut self) {
        self.count = 0;
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MoveList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MoveList[")?;
        for i in 0..self.count {
            if i > 0 { write!(f, ", ")?; }
            write!(f, "{}", self.moves[i])?;
        }
        write!(f, "]")
    }
}

// ── Main entry points ─────────────────────────────────────────────────────────

/// Generate all legal moves for the side to move.
/// Returns a MoveList containing every move that is legal in this position.
/// A move is legal if it follows piece movement rules AND does not leave
/// the moving side's King in check.
pub fn generate_moves(pos: &Position) -> MoveList {
    let mut pseudo = MoveList::new();

    // Generate all pseudo-legal moves
    generate_pseudo_legal(pos, &mut pseudo);

    // Filter to legal moves only
    legal::filter_legal(pos, pseudo)
}

/// Generate all pseudo-legal moves (may leave King in check).
/// Used internally and by perft for performance testing.
pub fn generate_pseudo_legal(pos: &Position, list: &mut MoveList) {
    let color = pos.side_to_move;

    // Pawn moves (Pet Dragon custom — double-step from actual start square)
    pawns::generate_pawn_moves(pos, color, list);

    // Piece moves (standard chess rules)
    pieces::generate_piece_moves(pos, color, list);

    // Castling (only when Rook started on standard square)
    castling::generate_castling_moves(pos, color, list);
}

/// Generate only capture moves (used in quiescence search)
pub fn generate_captures(pos: &Position) -> MoveList {
    let mut pseudo = MoveList::new();
    let color = pos.side_to_move;

    // Pawn captures and promotions
    pawns::generate_pawn_captures(pos, color, &mut pseudo);

    // Piece captures
    pieces::generate_piece_captures(pos, color, &mut pseudo);

    // Filter to legal captures
    legal::filter_legal(pos, pseudo)
}

/// Count legal moves without storing them (used for game state detection)
/// More efficient than generate_moves().len() for checkmate/stalemate detection
pub fn count_legal_moves(pos: &Position) -> usize {
    generate_moves(pos).len()
}

/// Is the current position checkmate?
/// (in check AND no legal moves)
pub fn is_checkmate(pos: &Position) -> bool {
    pos.in_check(pos.side_to_move) && count_legal_moves(pos) == 0
}

/// Is the current position stalemate?
/// (not in check AND no legal moves)
pub fn is_stalemate(pos: &Position) -> bool {
    !pos.in_check(pos.side_to_move) && count_legal_moves(pos) == 0
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_move_list_basic() {
        let mut list = MoveList::new();
        assert_eq!(list.len(), 0);
        assert!(list.is_empty());

        use crate::types::{Move, MoveKind, Square};
        let mv = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        list.push(mv);
        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());
        assert_eq!(list.get(0), mv);
    }

    #[test]
    fn test_start_pos_move_count() {
        setup();
        let pos = Position::start_pos().unwrap();
        let moves = generate_moves(&pos);
        // Standard chess starting position has exactly 20 legal moves
        // (16 pawn moves + 4 knight moves)
        assert_eq!(moves.len(), 20,
            "Standard start should have 20 legal moves, got {}",
            moves.len());
    }

    #[test]
    fn test_not_checkmate_or_stalemate_at_start() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert!(!is_checkmate(&pos));
        assert!(!is_stalemate(&pos));
    }

    #[test]
    fn test_captures_subset_of_moves() {
        setup();
        // In starting position there are no captures
        let pos = Position::start_pos().unwrap();
        let captures = generate_captures(&pos);
        assert_eq!(captures.len(), 0,
            "No captures available at start position");
    }
}
