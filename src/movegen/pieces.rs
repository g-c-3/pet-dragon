// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// movegen/pieces.rs — Move generation for Knights, Bishops, Rooks,
//                     Queens and Kings
//
// All piece movement here follows standard chess rules.
// No Pet Dragon customisation — that's all in pawns.rs.
//
// Uses magic bitboards (Phase 2) for fast sliding piece attacks.
// Knight and King attacks use precomputed tables (Phase 2).
//
// Two entry points:
//   generate_piece_moves()    — all quiet moves + captures
//   generate_piece_captures() — captures only (for quiescence search)
// ============================================================================

use crate::bitboard::{bishop_attacks, queen_attacks, rook_attacks};
use crate::bitboard::masks::{king_attacks, knight_attacks};
use crate::movegen::MoveList;
use crate::position::Position;
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

// ── Main entry points ─────────────────────────────────────────────────────────

/// Generate all pseudo-legal piece moves (quiet + captures)
/// Does not include pawns or castling — those are in separate files
pub fn generate_piece_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    generate_knight_moves(pos, color, list);
    generate_bishop_moves(pos, color, list);
    generate_rook_moves(pos, color, list);
    generate_queen_moves(pos, color, list);
    generate_king_moves(pos, color, list);
}

/// Generate only capture moves for pieces (used in quiescence search)
pub fn generate_piece_captures(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let enemies = pos.occupied(color.flip());

    generate_knight_moves_to(pos, color, enemies, list);
    generate_bishop_moves_to(pos, color, enemies, list);
    generate_rook_moves_to(pos, color, enemies, list);
    generate_queen_moves_to(pos, color, enemies, list);
    generate_king_moves_to(pos, color, enemies, list);
}

// ── Knight moves ──────────────────────────────────────────────────────────────

fn generate_knight_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    // Can move anywhere except squares occupied by own pieces
    let targets = !pos.occupied(color);
    generate_knight_moves_to(pos, color, targets, list);
}

fn generate_knight_moves_to(
    pos:     &Position,
    color:   Color,
    targets: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    let enemies = pos.occupied(color.flip());
    let mut knights = pos.piece_bb(color, PieceKind::Knight);

    while let Some(from) = knights.pop_lsb() {
        // Get all squares this knight can jump to
        let mut attacks = knight_attacks(from) & targets;

        while let Some(to) = attacks.pop_lsb() {
            if enemies.contains(to) {
                // Capture
                let captured = pos.piece_on(to, color.flip());
                list.push(Move::capture(
                    from, to,
                    MoveKind::Capture,
                    captured.unwrap(),
                ));
            } else {
                // Quiet move
                list.push(Move::new(from, to, MoveKind::Quiet));
            }
        }
    }
}

// ── Bishop moves ──────────────────────────────────────────────────────────────

fn generate_bishop_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let targets = !pos.occupied(color);
    generate_bishop_moves_to(pos, color, targets, list);
}

fn generate_bishop_moves_to(
    pos:     &Position,
    color:   Color,
    targets: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    let enemies  = pos.occupied(color.flip());
    let occupied = pos.all_pieces();
    let mut bishops = pos.piece_bb(color, PieceKind::Bishop);

    while let Some(from) = bishops.pop_lsb() {
        let mut attacks = bishop_attacks(from, occupied) & targets;

        while let Some(to) = attacks.pop_lsb() {
            add_move(pos, color, from, to, enemies, list);
        }
    }
}

// ── Rook moves ────────────────────────────────────────────────────────────────

fn generate_rook_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let targets = !pos.occupied(color);
    generate_rook_moves_to(pos, color, targets, list);
}

fn generate_rook_moves_to(
    pos:     &Position,
    color:   Color,
    targets: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    let enemies  = pos.occupied(color.flip());
    let occupied = pos.all_pieces();
    let mut rooks = pos.piece_bb(color, PieceKind::Rook);

    while let Some(from) = rooks.pop_lsb() {
        let mut attacks = rook_attacks(from, occupied) & targets;

        while let Some(to) = attacks.pop_lsb() {
            add_move(pos, color, from, to, enemies, list);
        }
    }
}

// ── Queen moves ───────────────────────────────────────────────────────────────

fn generate_queen_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let targets = !pos.occupied(color);
    generate_queen_moves_to(pos, color, targets, list);
}

fn generate_queen_moves_to(
    pos:     &Position,
    color:   Color,
    targets: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    let enemies  = pos.occupied(color.flip());
    let occupied = pos.all_pieces();
    let mut queens = pos.piece_bb(color, PieceKind::Queen);

    while let Some(from) = queens.pop_lsb() {
        let mut attacks = queen_attacks(from, occupied) & targets;

        while let Some(to) = attacks.pop_lsb() {
            add_move(pos, color, from, to, enemies, list);
        }
    }
}

// ── King moves ────────────────────────────────────────────────────────────────
// Note: castling is handled separately in castling.rs
// This generates only normal one-square King moves

fn generate_king_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let targets = !pos.occupied(color);
    generate_king_moves_to(pos, color, targets, list);
}

fn generate_king_moves_to(
    pos:     &Position,
    color:   Color,
    targets: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    let enemies  = pos.occupied(color.flip());
    let from     = pos.king_sq(color);
    let mut attacks = king_attacks(from) & targets;

    while let Some(to) = attacks.pop_lsb() {
        add_move(pos, color, from, to, enemies, list);
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Add a move to the list, determining if it's a capture or quiet move
#[inline]
fn add_move(
    pos:     &Position,
    color:   Color,
    from:    Square,
    to:      Square,
    enemies: crate::bitboard::Bitboard,
    list:    &mut MoveList,
) {
    if enemies.contains(to) {
        let captured = pos.piece_on(to, color.flip()).unwrap();
        list.push(Move::capture(from, to, MoveKind::Capture, captured));
    } else {
        list.push(Move::new(from, to, MoveKind::Quiet));
    }
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
    use crate::types::{Color, PieceKind};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_knight_moves_start_pos() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_knight_moves(&pos, Color::White, &mut list);
        // White knights on b1 and g1 each have 2 moves = 4 total
        assert_eq!(list.len(), 4,
            "White knights should have 4 moves at start");
    }

    #[test]
    fn test_no_piece_moves_blocked() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        // Bishops, Rooks, Queens all blocked at start
        generate_bishop_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0, "Bishops blocked at start");
        list.clear();
        generate_rook_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0, "Rooks blocked at start");
        list.clear();
        generate_queen_moves(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 0, "Queen blocked at start");
    }

    #[test]
    fn test_king_moves_start_pos() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_king_moves(&pos, Color::White, &mut list);
        // King blocked at start position
        assert_eq!(list.len(), 0, "King blocked at start");
    }

    #[test]
    fn test_rook_open_position() {
        setup();
        // Position with Rook on open file
        let fen = "4k3/8/8/8/8/8/8/R3K3 w Q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_rook_moves(&pos, Color::White, &mut list);
        // Rook on a1 can go to a2-a8 (7) and b1-d1 (3) = 10 moves
        assert_eq!(list.len(), 10,
            "Rook on a1 should have 10 moves");
    }

    #[test]
    fn test_queen_open_position() {
        setup();
        // Queen alone in center
        let fen = "4k3/8/8/8/3Q4/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_queen_moves(&pos, Color::White, &mut list);
        // Queen on d4 in open position — many moves
        // Exact count: rank(7) + file(7) + diag1(7) + diag2(5) - overlaps
        // minus squares attacked by/near kings
        assert!(list.len() > 20,
            "Queen in open position should have many moves, got {}",
            list.len());
    }

    #[test]
    fn test_captures_generated() {
        setup();
        // Position where White can capture
        let fen = "4k3/8/8/8/8/p7/8/R3K3 w Q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_rook_moves(&pos, Color::White, &mut list);
        // Rook should be able to capture the pawn on a3
        let has_capture = list.iter().any(|m| m.kind == MoveKind::Capture);
        assert!(has_capture, "Rook should be able to capture pawn");
    }

    #[test]
    fn test_piece_captures_only() {
        setup();
        let fen = "4k3/8/8/8/8/p7/8/R3K3 w Q - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_piece_captures(&pos, Color::White, &mut list);
        // Only captures should be generated
        for mv in list.iter() {
            assert_eq!(mv.kind, MoveKind::Capture,
                "generate_piece_captures should only return captures");
        }
        assert!(list.len() > 0, "Should find at least one capture");
    }

    #[test]
    fn test_pet_dragon_position_pieces() {
        setup();
        // Test piece generation in a Pet Dragon position
        let pos = crate::position::Position::generate_with_seed(42);
        let mut list = MoveList::new();
        // Should not panic — pieces generate correctly in any Pet Dragon pos
        generate_piece_moves(&pos, Color::White, &mut list);
        // At least knights should have moves in most positions
        assert!(list.len() > 0,
            "Should have piece moves in Pet Dragon position");
    }
}
