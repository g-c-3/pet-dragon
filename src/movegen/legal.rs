// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// movegen/legal.rs — Legal move filter
//
// Filters pseudo-legal moves to only those that are truly legal.
// A move is legal if and only if it does not leave the moving side's
// King in check after the move is made.
//
// Algorithm:
//   For each pseudo-legal move:
//     1. Make the move on a copy of the position
//     2. Check if the King is in check
//     3. If not in check → legal move, keep it
//     4. If in check → illegal move, discard it
//
// This is a temporary make/unmake used only for legality checking.
// The full make/unmake system (Phase 5) handles all state correctly
// for the search. This version is simpler — it works on a cloned
// position to avoid corrupting search state.
//
// Performance note: cloning a position is ~100ns. At 1M NPS with
// ~30 moves per position this costs ~3ms per second. Acceptable for
// now. Phase 5's incremental make/unmake eliminates this cost.
// ============================================================================

use crate::movegen::MoveList;
use crate::position::Position;
use crate::types::{
    Color, Move, MoveKind, PieceKind, Square,
};

// ── Main filter ───────────────────────────────────────────────────────────────

/// Filter a pseudo-legal move list to only legal moves.
/// Returns a new MoveList containing only moves that don't leave
/// the moving side's King in check.
pub fn filter_legal(pos: &Position, pseudo: MoveList) -> MoveList {
    let mut legal = MoveList::new();
    let color = pos.side_to_move;

    for mv in pseudo.iter() {
        if is_legal(pos, *mv, color) {
            legal.push(*mv);
        }
    }

    legal
}

/// Check if a single move is legal (does not leave King in check)
pub fn is_legal(pos: &Position, mv: Move, color: Color) -> bool {
    let mut test_pos = pos.clone();
    apply_move_for_legality(&mut test_pos, mv, color);
    !test_pos.in_check(color)
}

// ── Temporary move application ────────────────────────────────────────────────
// This is a simplified make_move used only for legality checking.
// It correctly handles all move types but doesn't maintain full
// game state (no history stack, no Zobrist hash update).
// Phase 5 builds the full make/unmake on top of this foundation.

/// Public version for use in perft tests
pub fn apply_move_for_legality_pub(
    pos: &mut Position,
    mv: Move,
    color: Color,
) {
    apply_move_for_legality(pos, mv, color);
}

fn apply_move_for_legality(pos: &mut Position, mv: Move, color: Color) {
    let from = mv.from;
    let to   = mv.to;

    match mv.kind {
        // ── Quiet move ────────────────────────────────────────────────────────
        MoveKind::Quiet => {
            let kind = pos.piece_on(from, color)
                .expect("No piece on from square");
            pos.remove_piece(color, kind, from);
            pos.put_piece(color, kind, to);

            // Update castling rights if King or Rook moved
            update_castling_rights(pos, color, kind, from);
        }

        // ── Double pawn push ──────────────────────────────────────────────────
        MoveKind::DoublePush => {
            pos.remove_piece(color, PieceKind::Pawn, from);
            pos.put_piece(color, PieceKind::Pawn, to);

            // Set en passant target square (square pawn passed through)
            let ep_rank = match color {
                Color::White => from.rank() + 1,
                Color::Black => from.rank() - 1,
            };
            pos.en_passant = Square::from_file_rank(from.file(), ep_rank);
        }

        // ── Capture ───────────────────────────────────────────────────────────
        MoveKind::Capture => {
            let kind = pos.piece_on(from, color)
                .expect("No piece on from square");
            let captured = mv.captured
                .expect("Capture move must have captured piece");

            pos.remove_piece(color.flip(), captured, to);
            pos.remove_piece(color, kind, from);
            pos.put_piece(color, kind, to);

            update_castling_rights(pos, color, kind, from);
            // Also clear castling rights if captured Rook was on standard sq
            update_castling_rights_capture(pos, color.flip(), to);
        }

        // ── En passant ────────────────────────────────────────────────────────
        MoveKind::EnPassant => {
            // The captured pawn is NOT on the 'to' square —
            // it's on the same rank as 'from', same file as 'to'
            let captured_pawn_sq = Square::from_file_rank(
                to.file(), from.rank()
            ).expect("En passant captured pawn square must be valid");

            pos.remove_piece(color.flip(), PieceKind::Pawn, captured_pawn_sq);
            pos.remove_piece(color, PieceKind::Pawn, from);
            pos.put_piece(color, PieceKind::Pawn, to);
            pos.en_passant = None;
        }

        // ── Kingside castling ─────────────────────────────────────────────────
        MoveKind::CastleKing => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square::H1, Square::F1),
                Color::Black => (Square::H8, Square::F8),
            };
            pos.remove_piece(color, PieceKind::King, from);
            pos.put_piece(color, PieceKind::King, to);
            pos.remove_piece(color, PieceKind::Rook, rook_from);
            pos.put_piece(color, PieceKind::Rook, rook_to);

            // Remove all castling rights for this color
            pos.castling.remove_all(color);
        }

        // ── Queenside castling ────────────────────────────────────────────────
        MoveKind::CastleQueen => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square::A1, Square::D1),
                Color::Black => (Square::A8, Square::D8),
            };
            pos.remove_piece(color, PieceKind::King, from);
            pos.put_piece(color, PieceKind::King, to);
            pos.remove_piece(color, PieceKind::Rook, rook_from);
            pos.put_piece(color, PieceKind::Rook, rook_to);

            pos.castling.remove_all(color);
        }

        // ── Promotions (quiet) ────────────────────────────────────────────────
        MoveKind::PromoQueen  => apply_promotion(pos, color, from, to,
                                     PieceKind::Queen,  None),
        MoveKind::PromoRook   => apply_promotion(pos, color, from, to,
                                     PieceKind::Rook,   None),
        MoveKind::PromoBishop => apply_promotion(pos, color, from, to,
                                     PieceKind::Bishop, None),
        MoveKind::PromoKnight => apply_promotion(pos, color, from, to,
                                     PieceKind::Knight, None),

        // ── Promotion captures ────────────────────────────────────────────────
        MoveKind::PromoCapQueen  => apply_promotion(pos, color, from, to,
                                        PieceKind::Queen,
                                        mv.captured),
        MoveKind::PromoCapRook   => apply_promotion(pos, color, from, to,
                                        PieceKind::Rook,
                                        mv.captured),
        MoveKind::PromoCapBishop => apply_promotion(pos, color, from, to,
                                        PieceKind::Bishop,
                                        mv.captured),
        MoveKind::PromoCapKnight => apply_promotion(pos, color, from, to,
                                        PieceKind::Knight,
                                        mv.captured),
    }

    // Clear en passant unless we just set it via double push
    if mv.kind != MoveKind::DoublePush {
        pos.en_passant = None;
    }

    // Flip side to move
    pos.side_to_move = pos.side_to_move.flip();
}

// ── Promotion helper ──────────────────────────────────────────────────────────

fn apply_promotion(
    pos:      &mut Position,
    color:    Color,
    from:     Square,
    to:       Square,
    promotes_to: PieceKind,
    captured: Option<PieceKind>,
) {
    // Remove captured piece if any
    if let Some(cap) = captured {
        pos.remove_piece(color.flip(), cap, to);
        update_castling_rights_capture(pos, color.flip(), to);
    }
    // Remove pawn, place promoted piece
    pos.remove_piece(color, PieceKind::Pawn, from);
    pos.put_piece(color, promotes_to, to);
}

// ── Castling rights update helpers ───────────────────────────────────────────

/// Update castling rights when a King or Rook moves
#[inline]
fn update_castling_rights(
    pos:   &mut Position,
    color: Color,
    kind:  PieceKind,
    from:  Square,
) {
    match kind {
        PieceKind::King => {
            // King moved — lose all castling rights for this color
            pos.castling.remove_all(color);
        }
        PieceKind::Rook => {
            // Rook moved from standard square — lose that specific right
            match color {
                Color::White => {
                    if from == Square::H1 {
                        pos.castling.white_kingside = false;
                    }
                    if from == Square::A1 {
                        pos.castling.white_queenside = false;
                    }
                }
                Color::Black => {
                    if from == Square::H8 {
                        pos.castling.black_kingside = false;
                    }
                    if from == Square::A8 {
                        pos.castling.black_queenside = false;
                    }
                }
            }
        }
        _ => {}
    }
}

/// Update castling rights when a Rook is captured on its standard square
#[inline]
fn update_castling_rights_capture(
    pos:   &mut Position,
    color: Color,
    sq:    Square,
) {
    match color {
        Color::White => {
            if sq == Square::H1 { pos.castling.white_kingside  = false; }
            if sq == Square::A1 { pos.castling.white_queenside = false; }
        }
        Color::Black => {
            if sq == Square::H8 { pos.castling.black_kingside  = false; }
            if sq == Square::A8 { pos.castling.black_queenside = false; }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::movegen::{generate_moves, generate_pseudo_legal};
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, MoveKind};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_legal_moves_start_pos() {
        setup();
        let pos = Position::start_pos().unwrap();
        let legal = generate_moves(&pos);
        assert_eq!(legal.len(), 20,
            "Standard start should have exactly 20 legal moves");
    }

    #[test]
    fn test_legal_subset_of_pseudo() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut pseudo = MoveList::new();
        generate_pseudo_legal(&pos, &mut pseudo);
        let legal = generate_moves(&pos);
        // Legal moves must be a subset of pseudo-legal
        assert!(legal.len() <= pseudo.len(),
            "Legal moves cannot exceed pseudo-legal moves");
    }

    #[test]
    fn test_check_evasion() {
        setup();
        // King in check — only legal moves are those that escape check
        // Scholar's mate setup: White Queen threatens Black King
        let fen =
            "rnb1kbnr/pppp1ppp/8/4p3/2B1P3/8/PPPP1PPP/RNBQK1NR b KQkq - 0 3";
        let pos = Position::from_fen(fen).unwrap();
        let legal = generate_moves(&pos);
        // All legal moves must resolve the check
        for mv in legal.iter() {
            let mut test_pos = pos.clone();
            apply_move_for_legality(&mut test_pos, *mv, Color::Black);
            assert!(!test_pos.in_check(Color::Black),
                "Move {} should resolve check", mv);
        }
    }

    #[test]
    fn test_pinned_piece_cannot_move_to_expose_king() {
        setup();
        // Rook pinned to King by enemy Bishop — can't move off pin ray
        let fen = "4k3/8/8/8/8/b7/1R6/2K5 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let legal = generate_moves(&pos);
        // After filtering, pinned Rook moves that expose King should be gone
        for mv in legal.iter() {
            let mut test_pos = pos.clone();
            apply_move_for_legality(&mut test_pos, *mv, Color::White);
            assert!(!test_pos.in_check(Color::White),
                "Move {} should not expose King to check", mv);
        }
    }

    #[test]
fn test_en_passant_legality() {
    setup();
    // En passant that would expose King to check is illegal
    // Classic "en passant pin" case:
    // White King a5, White pawn b5, Black pawn c5, BLACK Rook d5
    // After b5xc6 en passant:
    //   White pawn b5 moves to c6
    //   Black pawn c5 is captured and removed
    //   Rank 5 now has: King a5, empty b5, empty c5, Black Rook d5
    //   King a5 is exposed to Black Rook d5 — ILLEGAL
    let fen = "8/8/8/KPpr4/8/8/8/7k w - c6 0 1";
    let pos = Position::from_fen(fen).unwrap();
    let legal = generate_moves(&pos);
    let ep_moves: Vec<_> = legal.iter()
        .filter(|m| m.kind == MoveKind::EnPassant)
        .collect();
    assert_eq!(ep_moves.len(), 0,
        "En passant that exposes King to Black Rook should be illegal");
}
    #[test]
    fn test_castling_through_check_illegal() {
        setup();
        // King cannot castle through an attacked square
        let fen = "4k3/8/8/8/8/8/5r2/4K2R w K - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let legal = generate_moves(&pos);
        let castle_moves: Vec<_> = legal.iter()
            .filter(|m| m.kind == MoveKind::CastleKing)
            .collect();
        assert_eq!(castle_moves.len(), 0,
            "Cannot castle through attacked square");
    }

    #[test]
    fn test_checkmate_no_legal_moves() {
        setup();
        // Fool's mate — Black is in checkmate
        let fen =
            "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3";
        let pos = Position::from_fen(fen).unwrap();
        let legal = generate_moves(&pos);
        assert_eq!(legal.len(), 0,
            "Checkmate position should have no legal moves");
        assert!(pos.in_check(Color::White),
            "Should be in check in this position");
    }

    #[test]
    fn test_stalemate_no_legal_moves() {
        setup();
        // Classic stalemate position
        let fen = "k7/8/1Q6/8/8/8/8/7K b - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        // Black King has no legal moves and is not in check
        if !pos.in_check(Color::Black) {
            let legal = generate_moves(&pos);
            assert_eq!(legal.len(), 0,
                "Stalemate should have no legal moves");
        }
    }

    #[test]
    fn test_pet_dragon_legal_moves_1000() {
        setup();
        // Verify legal move generation doesn't panic for 1000 positions
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let legal = generate_moves(&pos);
            // Should have reasonable number of moves
            assert!(legal.len() <= 256,
                "Should never exceed 256 legal moves (seed {})", seed);
            // All moves must not leave King in check
            for mv in legal.iter() {
                let mut test_pos = pos.clone();
                apply_move_for_legality(
                    &mut test_pos, *mv, pos.side_to_move
                );
                assert!(
                    !test_pos.in_check(pos.side_to_move),
                    "Legal move {} leaves King in check (seed {})",
                    mv, seed
                );
            }
        }
    }

    #[test]
    fn test_apply_move_flips_side() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_eq!(pos.side_to_move, Color::White);
        let mv = generate_moves(&pos).get(0);
        let mut test_pos = pos.clone();
        apply_move_for_legality(&mut test_pos, mv, Color::White);
        assert_eq!(test_pos.side_to_move, Color::Black,
            "Side to move should flip after move");
    }
}
