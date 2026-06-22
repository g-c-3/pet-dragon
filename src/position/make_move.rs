// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// position/make_move.rs — Full incremental make/unmake move
//
// The search calls make_move() before searching deeper and unmake_move()
// when coming back up. This happens millions of times per second.
//
// Every state change is tracked in a HistoryEntry saved before the move.
// unmake_move() uses this entry to restore the position exactly.
//
// State that changes on a move:
//   - Piece bitboards (piece moves/captures)
//   - Occupancy bitboards (derived from pieces)
//   - Side to move (always flips)
//   - Castling rights (may be lost if King/Rook moves)
//   - En passant square (set by double push, cleared otherwise)
//   - Halfmove clock (reset on capture/pawn move, else +1)
//   - Fullmove number (increments after Black's move)
//   - Zobrist hash (updated incrementally)
//
// State that NEVER changes (does not need saving/restoring):
//   - pawn_starts map (records starting squares, never modified)
//   - Board dimensions, rules
// ============================================================================

use crate::position::{HistoryEntry, Position};
use crate::position::zobrist::{
    castling_key, ep_key, pawn_start_key, piece_key, side_key,
};
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

impl Position {
    // ── Make move ─────────────────────────────────────────────────────────────

    /// Apply a move to the position, saving undo information.
    /// The move MUST be legal — no legality check is performed here.
    /// Call unmake_move() with the same move to restore position.
    pub fn make_move(&mut self, mv: Move) {
        let color   = self.side_to_move;
        let from    = mv.from;
        let to      = mv.to;

        // ── Save undo information ─────────────────────────────────────────────
        let entry = HistoryEntry {
            mv,
            castling:       self.castling,
            en_passant:     self.en_passant,
            halfmove_clock: self.halfmove_clock,
            hash:           self.hash,
            captured:       mv.captured,
        };
        self.history.push(entry);

        // ── Update hash: remove old state ─────────────────────────────────────
        // Remove castling rights from hash (will re-add after update)
        self.hash ^= castling_key(self.castling.to_mask());

        // Remove en passant from hash
        if let Some(ep) = self.en_passant {
            self.hash ^= ep_key(ep.file());
        }

        // Remove side to move
        if color == Color::Black {
            self.hash ^= side_key();
        }

        // ── Apply the move ────────────────────────────────────────────────────
        match mv.kind {
            MoveKind::Quiet => {
                self.move_piece(color, from, to);
                self.update_castling_on_move(color, from);
                self.halfmove_clock += 1;
            }

            MoveKind::DoublePush => {
                self.move_piece_pawn(color, from, to);
                // Set en passant target (square pawn passed through)
                let ep_rank = match color {
                    Color::White => from.rank() + 1,
                    Color::Black => from.rank() - 1,
                };
                self.en_passant =
                    Square::from_file_rank(from.file(), ep_rank);
                self.halfmove_clock = 0; // pawn move resets clock
            }

            MoveKind::Capture => {
                let captured = mv.captured
                    .expect("Capture must have captured piece");
                // Remove captured piece from hash before removing from board
                self.hash ^= piece_key(color.flip(), captured, to);
                self.remove_piece(color.flip(), captured, to);
                self.move_piece(color, from, to);
                self.update_castling_on_move(color, from);
                self.update_castling_on_capture(color.flip(), to);
                self.halfmove_clock = 0; // capture resets clock
            }

            MoveKind::EnPassant => {
                // Captured pawn is on same rank as 'from', same file as 'to'
                let captured_sq = Square::from_file_rank(
                    to.file(), from.rank()
                ).expect("En passant captured square must be valid");
                self.hash ^= piece_key(
                    color.flip(), PieceKind::Pawn, captured_sq
                );
                self.remove_piece(color.flip(), PieceKind::Pawn, captured_sq);
                self.move_piece_pawn(color, from, to);
                self.halfmove_clock = 0;
            }

            MoveKind::CastleKing => {
                let (rook_from, rook_to) = match color {
                    Color::White => (Square::H1, Square::F1),
                    Color::Black => (Square::H8, Square::F8),
                };
                self.move_piece(color, from, to);
                self.move_piece(color, rook_from, rook_to);
                self.castling.remove_all(color);
                self.halfmove_clock += 1;
            }

            MoveKind::CastleQueen => {
                let (rook_from, rook_to) = match color {
                    Color::White => (Square::A1, Square::D1),
                    Color::Black => (Square::A8, Square::D8),
                };
                self.move_piece(color, from, to);
                self.move_piece(color, rook_from, rook_to);
                self.castling.remove_all(color);
                self.halfmove_clock += 1;
            }

            MoveKind::PromoQueen  => self.apply_promotion(
                color, from, to, PieceKind::Queen,  mv.captured),
            MoveKind::PromoRook   => self.apply_promotion(
                color, from, to, PieceKind::Rook,   mv.captured),
            MoveKind::PromoBishop => self.apply_promotion(
                color, from, to, PieceKind::Bishop, mv.captured),
            MoveKind::PromoKnight => self.apply_promotion(
                color, from, to, PieceKind::Knight, mv.captured),

            MoveKind::PromoCapQueen  => self.apply_promotion(
                color, from, to, PieceKind::Queen,  mv.captured),
            MoveKind::PromoCapRook   => self.apply_promotion(
                color, from, to, PieceKind::Rook,   mv.captured),
            MoveKind::PromoCapBishop => self.apply_promotion(
                color, from, to, PieceKind::Bishop, mv.captured),
            MoveKind::PromoCapKnight => self.apply_promotion(
                color, from, to, PieceKind::Knight, mv.captured),
        }

        // Clear en passant unless just set by double push
        if mv.kind != MoveKind::DoublePush {
            self.en_passant = None;
        }

        // ── Update hash: add new state ────────────────────────────────────────
        // Add new castling rights
        self.hash ^= castling_key(self.castling.to_mask());

        // Add new en passant
        if let Some(ep) = self.en_passant {
            self.hash ^= ep_key(ep.file());
        }

        // Flip side to move
        self.side_to_move = color.flip();

        // Add new side to move (Black gets the key, White gets nothing)
        if self.side_to_move == Color::Black {
            self.hash ^= side_key();
        }

        // Increment fullmove number after Black's move
        if color == Color::Black {
            self.fullmove_number += 1;
        }
    }

    // ── Unmake move ───────────────────────────────────────────────────────────

    /// Restore position to state before the last make_move() call.
    /// Must be called with the same move passed to make_move().
    pub fn unmake_move(&mut self, mv: Move) {
        // Restore saved state
        let entry = self.history.pop()
            .expect("unmake_move called with empty history");

        // Flip side back — the moving side is now 'color'
        self.side_to_move = self.side_to_move.flip();
        let color = self.side_to_move;

        let from = mv.from;
        let to   = mv.to;

        // Restore saved state directly
        self.castling       = entry.castling;
        self.en_passant     = entry.en_passant;
        self.halfmove_clock = entry.halfmove_clock;
        self.hash           = entry.hash; // restore exact hash

        // Decrement fullmove number if Black just moved
        if color == Color::Black {
            self.fullmove_number -= 1;
        }

        // ── Reverse the move ──────────────────────────────────────────────────
        match mv.kind {
            MoveKind::Quiet => {
                self.unmove_piece(color, from, to);
            }

            MoveKind::DoublePush => {
                self.unmove_piece_pawn(color, from, to);
            }

            MoveKind::Capture => {
                let captured = entry.captured
                    .expect("Capture entry must have captured piece");
                self.unmove_piece(color, from, to);
                self.put_piece(color.flip(), captured, to);
            }

            MoveKind::EnPassant => {
                let captured_sq = Square::from_file_rank(
                    to.file(), from.rank()
                ).expect("En passant captured square valid");
                self.unmove_piece_pawn(color, from, to);
                self.put_piece(color.flip(), PieceKind::Pawn, captured_sq);
            }

            MoveKind::CastleKing => {
                let (rook_from, rook_to) = match color {
                    Color::White => (Square::H1, Square::F1),
                    Color::Black => (Square::H8, Square::F8),
                };
                self.unmove_piece(color, from, to);
                self.unmove_piece(color, rook_from, rook_to);
            }

            MoveKind::CastleQueen => {
                let (rook_from, rook_to) = match color {
                    Color::White => (Square::A1, Square::D1),
                    Color::Black => (Square::A8, Square::D8),
                };
                self.unmove_piece(color, from, to);
                self.unmove_piece(color, rook_from, rook_to);
            }

            // Promotions: remove promoted piece, restore pawn
            MoveKind::PromoQueen  | MoveKind::PromoRook   |
            MoveKind::PromoBishop | MoveKind::PromoKnight => {
                let promo_piece = mv.kind.promotion_piece().unwrap();
                self.remove_piece(color, promo_piece, to);
                self.put_piece(color, PieceKind::Pawn, from);
            }

            // Promotion captures: remove promoted piece, restore pawn,
            // restore captured piece
            MoveKind::PromoCapQueen  | MoveKind::PromoCapRook   |
            MoveKind::PromoCapBishop | MoveKind::PromoCapKnight => {
                let promo_piece = mv.kind.promotion_piece().unwrap();
                let captured = entry.captured
                    .expect("Promo capture must have captured piece");
                self.remove_piece(color, promo_piece, to);
                self.put_piece(color, PieceKind::Pawn, from);
                self.put_piece(color.flip(), captured, to);
            }
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Move a piece from one square to another, updating hash
    #[inline]
    fn move_piece(&mut self, color: Color, from: Square, to: Square) {
        let kind = self.piece_on(from, color)
            .expect("move_piece: no piece on from square");
        self.hash ^= piece_key(color, kind, from);
        self.hash ^= piece_key(color, kind, to);
        self.remove_piece(color, kind, from);
        self.put_piece(color, kind, to);
        // Update castling hash if King moved
        if kind == PieceKind::King {
            self.castling.remove_all(color);
        }
    }

    /// Move a pawn (updates hash, does NOT update castling)
    #[inline]
    fn move_piece_pawn(&mut self, color: Color, from: Square, to: Square) {
        self.hash ^= piece_key(color, PieceKind::Pawn, from);
        self.hash ^= piece_key(color, PieceKind::Pawn, to);
        self.remove_piece(color, PieceKind::Pawn, from);
        self.put_piece(color, PieceKind::Pawn, to);
    }

    /// Reverse a piece move (from/to are original squares, piece went from→to)
    #[inline]
    fn unmove_piece(&mut self, color: Color, from: Square, to: Square) {
        let kind = self.piece_on(to, color)
            .expect("unmove_piece: no piece on to square");
        self.remove_piece(color, kind, to);
        self.put_piece(color, kind, from);
        // Note: hash is restored from entry directly, no need to update here
    }

    /// Reverse a pawn move
    #[inline]
    fn unmove_piece_pawn(&mut self, color: Color, from: Square, to: Square) {
        self.remove_piece(color, PieceKind::Pawn, to);
        self.put_piece(color, PieceKind::Pawn, from);
    }

    /// Apply promotion (with optional capture)
    fn apply_promotion(
        &mut self,
        color:       Color,
        from:        Square,
        to:          Square,
        promotes_to: PieceKind,
        captured:    Option<PieceKind>,
    ) {
        // Remove captured piece if any
        if let Some(cap) = captured {
            self.hash ^= piece_key(color.flip(), cap, to);
            self.remove_piece(color.flip(), cap, to);
            self.update_castling_on_capture(color.flip(), to);
        }
        // Remove pawn from hash and board
        self.hash ^= piece_key(color, PieceKind::Pawn, from);
        self.remove_piece(color, PieceKind::Pawn, from);
        // Place promoted piece
        self.hash ^= piece_key(color, promotes_to, to);
        self.put_piece(color, promotes_to, to);
        self.halfmove_clock = 0;
    }

    /// Update castling rights when King or Rook moves
    #[inline]
    fn update_castling_on_move(&mut self, color: Color, from: Square) {
        match color {
            Color::White => {
                if from == Square::H1 {
                    self.castling.white_kingside  = false;
                }
                if from == Square::A1 {
                    self.castling.white_queenside = false;
                }
            }
            Color::Black => {
                if from == Square::H8 {
                    self.castling.black_kingside  = false;
                }
                if from == Square::A8 {
                    self.castling.black_queenside = false;
                }
            }
        }
        // King moving removes all rights for that color
        // (handled in move_piece via piece_on detection)
    }

    /// Update castling rights when a Rook is captured on its standard square
    #[inline]
    fn update_castling_on_capture(&mut self, color: Color, sq: Square) {
        match color {
            Color::White => {
                if sq == Square::H1 {
                    self.castling.white_kingside  = false;
                }
                if sq == Square::A1 {
                    self.castling.white_queenside = false;
                }
            }
            Color::Black => {
                if sq == Square::H8 {
                    self.castling.black_kingside  = false;
                }
                if sq == Square::A8 {
                    self.castling.black_queenside = false;
                }
            }
        }
    }

    /// Make a move AND record position in game history (for repetition detection).
    /// Use this in the search instead of make_move() alone.
    #[inline]
    pub fn make_move_with_history(&mut self, mv: Move) {
        self.make_move(mv);
        self.push_game_history();
    }

    /// Unmake a move AND remove position from game history.
    /// Use this in the search instead of unmake_move() alone.
    #[inline]
    pub fn unmake_move_with_history(&mut self, mv: Move) {
        self.pop_game_history();
        self.unmake_move(mv);
    }
}
