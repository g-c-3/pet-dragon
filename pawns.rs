// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// movegen/pawns.rs — Pawn move generation (Pet Dragon custom)
//
// This file implements Pet Dragon's custom pawn rules.
//
// Pet Dragon pawn rules (confirmed):
//   Direction:   White always moves toward rank 8, Black toward rank 1.
//                No exceptions regardless of starting square.
//
//   Double step: A pawn may double-step on its FIRST move only,
//                from its ACTUAL starting square.
//                White pawn on rank 1 → can jump to rank 3
//                White pawn on rank 2 → can jump to rank 4
//                Black pawn on rank 8 → can jump to rank 6
//                Black pawn on rank 7 → can jump to rank 5
//                Eligibility check: is pawn currently on its start square?
//                (pawn_starts map records this — never changes during game)
//
//   En passant:  Follows naturally from double-step.
//                Standard logic, anchored to actual starting square.
//
//   Promotion:   White promotes on rank 8, Black on rank 1.
//                Standard choices: Queen, Rook, Bishop, Knight.
//
//   Everything else: identical to standard chess.
//
// Key insight: we don't need to track "has this pawn moved" separately.
// If a pawn is still on its start square → it hasn't moved → double-step ok.
// If it has moved away → it's no longer on start square → no double-step.
// The pawn_starts map is checked against current position automatically.
// ============================================================================

use crate::bitboard::masks::pawn_attacks;
use crate::movegen::MoveList;
use crate::position::Position;
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

// ── Main entry points ─────────────────────────────────────────────────────────

/// Generate all pseudo-legal pawn moves for a color
/// Includes: single push, double push, captures, en passant, promotions
pub fn generate_pawn_moves(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    generate_pawn_pushes(pos, color, list);
    generate_pawn_captures(pos, color, list);
}

/// Generate only pawn captures (used in quiescence search)
/// Includes: diagonal captures, en passant, promotion captures
pub fn generate_pawn_captures(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    generate_pawn_diagonal_captures(pos, color, list);
    generate_en_passant(pos, color, list);
}

// ── Pawn pushes ───────────────────────────────────────────────────────────────

/// Generate single and double pawn pushes
fn generate_pawn_pushes(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let empty = pos.empty_squares();
    let mut pawns = pos.piece_bb(color, PieceKind::Pawn);

    while let Some(from) = pawns.pop_lsb() {
        // ── Single push ───────────────────────────────────────────────────────
        let to_single = match color {
            Color::White => {
                let sq = Square::from_file_rank(from.file(), from.rank() + 1);
                sq.filter(|&s| empty.contains(s))
            }
            Color::Black => {
                if from.rank() == 0 { None }
                else {
                    let sq = Square::from_file_rank(
                        from.file(), from.rank() - 1
                    );
                    sq.filter(|&s| empty.contains(s))
                }
            }
        };

        if let Some(to) = to_single {
            let promotion_rank = match color {
                Color::White => 7, // rank 8 (0-indexed = 7)
                Color::Black => 0, // rank 1 (0-indexed = 0)
            };

            if to.rank() == promotion_rank {
                // Single push to promotion rank
                add_promotions(from, to, false, list);
            } else {
                list.push(Move::new(from, to, MoveKind::Quiet));

                // ── Double push ───────────────────────────────────────────────
                // Pet Dragon custom: only from actual starting square
                // Check if this pawn is currently on its start square
                if pos.pawn_starts.started_here(from, color) {
                    let to_double = match color {
                        Color::White => {
                            if from.rank() + 2 <= 7 {
                                Square::from_file_rank(
                                    from.file(), from.rank() + 2
                                ).filter(|&s| empty.contains(s))
                            } else { None }
                        }
                        Color::Black => {
                            if from.rank() >= 2 {
                                Square::from_file_rank(
                                    from.file(), from.rank() - 2
                                ).filter(|&s| empty.contains(s))
                            } else { None }
                        }
                    };

                    if let Some(to2) = to_double {
                        // Double push — record en passant target square
                        // EP target is the square the pawn passed through
                        list.push(Move::new(
                            from, to2,
                            MoveKind::DoublePush,
                        ));
                    }
                }
            }
        }
    }
}

// ── Pawn captures ─────────────────────────────────────────────────────────────

/// Generate diagonal pawn captures (including promotion captures)
fn generate_pawn_diagonal_captures(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    let enemies = pos.occupied(color.flip());
    let mut pawns = pos.piece_bb(color, PieceKind::Pawn);

    let promotion_rank = match color {
        Color::White => 7u8,
        Color::Black => 0u8,
    };

    while let Some(from) = pawns.pop_lsb() {
        // Get diagonal attack squares for this pawn
        let mut captures = pawn_attacks(color, from) & enemies;

        while let Some(to) = captures.pop_lsb() {
            let captured = pos.piece_on(to, color.flip()).unwrap();

            if to.rank() == promotion_rank {
                // Capture + promotion
                add_promotion_captures(from, to, captured, list);
            } else {
                // Normal capture
                list.push(Move::capture(
                    from, to,
                    MoveKind::Capture,
                    captured,
                ));
            }
        }
    }
}

/// Generate en passant captures
fn generate_en_passant(
    pos:   &Position,
    color: Color,
    list:  &mut MoveList,
) {
    // En passant target square: the square BEHIND the double-pushed pawn
    let ep_sq = match pos.en_passant {
        Some(sq) => sq,
        None => return,
    };

    // Find pawns that can capture en passant
    // A pawn can EP capture if it attacks the EP target square
    let mut pawns = pos.piece_bb(color, PieceKind::Pawn);

    while let Some(from) = pawns.pop_lsb() {
        let attacks = pawn_attacks(color, from);
        if attacks.contains(ep_sq) {
            list.push(Move::capture(
                from,
                ep_sq,
                MoveKind::EnPassant,
                PieceKind::Pawn, // always captures a pawn
            ));
        }
    }
}

// ── Promotion helpers ─────────────────────────────────────────────────────────

/// Add all four promotion moves (quiet push to promotion rank)
fn add_promotions(
    from:     Square,
    to:       Square,
    _capture: bool,
    list:     &mut MoveList,
) {
    list.push(Move::new(from, to, MoveKind::PromoQueen));
    list.push(Move::new(from, to, MoveKind::PromoRook));
    list.push(Move::new(from, to, MoveKind::PromoBishop));
    list.push(Move::new(from, to, MoveKind::PromoKnight));
}

/// Add all four promotion capture moves
fn add_promotion_captures(
    from:     Square,
    to:       Square,
    captured: PieceKind,
    list:     &mut MoveList,
) {
    list.push(Move::capture(from, to, MoveKind::PromoCapQueen,  captured));
    list.push(Move::capture(from, to, MoveKind::PromoCapRook,   captured));
    list.push(Move::capture(from, to, MoveKind::PromoCapBishop, captured));
    list.push(Move::capture(from, to, MoveKind::PromoCapKnight, captured));
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, MoveKind, Square};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    // ── Standard chess pawn tests ─────────────────────────────────────────────

    #[test]
    fn test_white_pawn_single_push_rank2() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);

        // Each of 8 White pawns on rank 2 should have:
        // 1 single push + 1 double push = 2 moves each = 16 total
        assert_eq!(list.len(), 16,
            "8 pawns × 2 moves each = 16, got {}", list.len());
    }

    #[test]
    fn test_standard_pawn_move_count() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_pawn_moves(&pos, Color::White, &mut list);
        // 8 pawns × 2 moves = 16 pawn moves at start
        assert_eq!(list.len(), 16,
            "White should have 16 pawn moves at start, got {}",
            list.len());
    }

    #[test]
    fn test_double_push_standard_rank2() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);

        let double_pushes: Vec<_> = list.iter()
            .filter(|m| m.kind == MoveKind::DoublePush)
            .collect();
        assert_eq!(double_pushes.len(), 8,
            "All 8 pawns on rank 2 should have double push");

        // Each double push should land on rank 4 (index 3)
        for mv in &double_pushes {
            assert_eq!(mv.to.rank(), 3,
                "Standard double push should land on rank 4");
        }
    }

    // ── Pet Dragon specific pawn tests ────────────────────────────────────────

    #[test]
    fn test_pet_dragon_pawn_rank1_double_push() {
        setup();
        // Create a position with a White pawn starting on rank 1
        // Use a FEN where pawn is on e1 (not standard but valid Pet Dragon)
        // We'll use the setup generator and find a position with rank 1 pawns
        let mut found_rank1_pawn = false;
        for seed in 0..200u64 {
            let pos = Position::generate_with_seed(seed);
            let white_pawns = pos.piece_bb(Color::White, PieceKind::Pawn);
            let rank1_pawns = white_pawns & Bitboard::RANK_1;

            if rank1_pawns.is_not_empty() {
                found_rank1_pawn = true;
                let mut list = MoveList::new();
                generate_pawn_pushes(&pos, Color::White, &mut list);

                // Check that rank 1 pawns have double push to rank 3
                let mut rank1_bb = rank1_pawns;
                while let Some(pawn_sq) = rank1_bb.pop_lsb() {
                    // Find double push from this square
                    let expected_to = Square::from_file_rank(
                        pawn_sq.file(), 2 // rank 3 (0-indexed)
                    ).unwrap();

                    // Check if the path is clear (rank 2 square must be empty)
                    let intermediate = Square::from_file_rank(
                        pawn_sq.file(), 1
                    ).unwrap();

                    if pos.piece_at(intermediate).is_none()
                    && pos.piece_at(expected_to).is_none() {
                        let has_double_push = list.iter().any(|m| {
                            m.from == pawn_sq
                            && m.to == expected_to
                            && m.kind == MoveKind::DoublePush
                        });
                        assert!(has_double_push,
                            "White pawn on rank 1 ({}) should have \
                             double push to rank 3 ({}) — seed {}",
                            pawn_sq, expected_to, seed
                        );
                    }
                }
                break;
            }
        }
        assert!(found_rank1_pawn,
            "Should find at least one position with rank 1 pawn in 200 seeds");
    }

    #[test]
    fn test_pet_dragon_pawn_rank2_double_push() {
        setup();
        // Standard chess start — all pawns on rank 2
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);

        // Verify double pushes land on rank 4
        let double_pushes: Vec<_> = list.iter()
            .filter(|m| m.kind == MoveKind::DoublePush)
            .collect();
        for mv in &double_pushes {
            assert_eq!(mv.to.rank(), 3,
                "Rank 2 pawn double push should land on rank 4 (index 3)");
            assert_eq!(mv.from.rank(), 1,
                "Double push should start from rank 2 (index 1)");
        }
    }

    #[test]
    fn test_no_double_push_after_moving() {
        setup();
        // Pawn that has moved away from start square cannot double push
        // After e2-e3, the pawn on e3 is NOT on its start square (e2)
        let fen =
            "rnbqkbnr/pppppppp/8/8/8/4P3/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        // Now it's Black's turn, but let's check White pawns anyway
        // The pawn on e3 should NOT have a double push
        // (it started on e2, has moved to e3, so not on start square)
        generate_pawn_pushes(&pos, Color::White, &mut list);
        let e3_double = list.iter().any(|m| {
            m.from == Square::E3 && m.kind == MoveKind::DoublePush
        });
        assert!(!e3_double,
            "Pawn on e3 (moved from e2) should not have double push");
    }

    #[test]
    fn test_en_passant_after_rank1_double_push() {
        // Regression guard: the EP target square set by a DoublePush move
        // must be derived from the ACTUAL from-square of that specific
        // push, not a hardcoded rank-2-to-rank-4 assumption — a Pet Dragon
        // pawn double-pushing from its custom rank-1 start (e1 -> e3)
        // passes through e2, NOT e3, so en passant against it must target
        // e2. See make_move.rs's DoublePush handling: `ep_rank =
        // from.rank() + 1` for White, computed relative to `from`, which is
        // what makes this correct regardless of which rank the push
        // originated from — this test regression-guards that generality.
        setup();
        // Extended FEN: White pawn's recorded start square is e1 (Pet
        // Dragon custom), currently still sitting there (hasn't moved
        // yet). Black pawn on d3 is positioned to capture en passant once
        // White pushes e1-e3. Kings placed on safe, irrelevant squares.
        let fen = "4k3/8/8/8/8/3p4/8/K3P3 w - - 0 1 e1:w";
        let mut pos = Position::from_fen(fen).unwrap();

        // Confirm the double push is actually offered from e1 first —
        // if this fails, the rest of the test is moot (see
        // test_pet_dragon_pawn_rank1_double_push for the dedicated check).
        let mut pushes = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut pushes);
        let double_push = pushes.iter().find(|m| {
            m.from == Square::E1 && m.kind == MoveKind::DoublePush
        }).expect("e1 pawn (recorded start square) should have a double \
                   push available");
        assert_eq!(double_push.to, Square::E3,
            "double push from e1 should land on e3");

        // Apply it for real and check the resulting en passant target.
        pos.make_move(*double_push);
        assert_eq!(pos.en_passant, Square::from_uci("e2"),
            "en passant target after an e1-e3 double push must be e2 (the \
             square actually passed through), not e3 — a hardcoded \
             rank-2-to-rank-4 assumption would get this wrong");

        // And confirm Black's d3 pawn can actually capture en passant onto
        // e2, ending up exactly where the passed-through square is.
        let mut ep_list = MoveList::new();
        generate_en_passant(&pos, Color::Black, &mut ep_list);
        assert_eq!(ep_list.len(), 1,
            "Black's d3 pawn should have exactly one en passant capture");
        assert_eq!(ep_list.get(0).from, Square::D3);
        assert_eq!(ep_list.get(0).to, Square::E2);
        assert_eq!(ep_list.get(0).kind, MoveKind::EnPassant);
    }

    #[test]
    fn test_en_passant_after_rank8_double_push() {
        // Symmetric counterpart to test_en_passant_after_rank1_double_push
        // — same regression guard, Black side. A Black pawn double-pushing
        // from its custom rank-8 start (e8 -> e6) passes through e7, NOT
        // e6, so en passant against it must target e7. make_move.rs's
        // `ep_rank = from.rank() - 1` for Black is what makes this correct
        // regardless of origin rank, mirroring White's `+ 1`.
        setup();
        // Extended FEN: Black pawn's recorded start is e8, still there.
        // White pawn on d6 is positioned to capture en passant once Black
        // pushes e8-e6.
        let fen = "k3p3/8/3P4/8/8/8/8/4K3 b - - 0 1 e8:b";
        let mut pos = Position::from_fen(fen).unwrap();

        let mut pushes = MoveList::new();
        generate_pawn_pushes(&pos, Color::Black, &mut pushes);
        let double_push = pushes.iter().find(|m| {
            m.from == Square::E8 && m.kind == MoveKind::DoublePush
        }).expect("e8 pawn (recorded start square) should have a double \
                   push available");
        assert_eq!(double_push.to, Square::E6,
            "double push from e8 should land on e6");

        pos.make_move(*double_push);
        assert_eq!(pos.en_passant, Square::from_uci("e7"),
            "en passant target after an e8-e6 double push must be e7 (the \
             square actually passed through), not e6");

        let mut ep_list = MoveList::new();
        generate_en_passant(&pos, Color::White, &mut ep_list);
        assert_eq!(ep_list.len(), 1,
            "White's d6 pawn should have exactly one en passant capture");
        assert_eq!(ep_list.get(0).from, Square::D6);
        assert_eq!(ep_list.get(0).to, Square::E7);
        assert_eq!(ep_list.get(0).kind, MoveKind::EnPassant);
    }

    #[test]
    fn test_black_pawn_moves_standard() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mut list = MoveList::new();
        generate_pawn_moves(&pos, Color::Black, &mut list);
        // Black also has 16 pawn moves at start
        assert_eq!(list.len(), 16,
            "Black should have 16 pawn moves at start, got {}",
            list.len());
    }

    #[test]
    fn test_black_double_push_from_rank8() {
        setup();
        // Find a Pet Dragon position with a Black pawn on rank 8
        let mut found_rank8_pawn = false;
        for seed in 0..200u64 {
            let pos = Position::generate_with_seed(seed);
            let black_pawns = pos.piece_bb(Color::Black, PieceKind::Pawn);
            let rank8_pawns = black_pawns & Bitboard::RANK_8;

            if rank8_pawns.is_not_empty() {
                found_rank8_pawn = true;
                let mut list = MoveList::new();
                generate_pawn_pushes(&pos, Color::Black, &mut list);

                let mut rank8_bb = rank8_pawns;
                while let Some(pawn_sq) = rank8_bb.pop_lsb() {
                    let expected_to = Square::from_file_rank(
                        pawn_sq.file(), 5 // rank 6 (0-indexed)
                    ).unwrap();
                    let intermediate = Square::from_file_rank(
                        pawn_sq.file(), 6
                    ).unwrap();

                    if pos.piece_at(intermediate).is_none()
                    && pos.piece_at(expected_to).is_none() {
                        let has_double_push = list.iter().any(|m| {
                            m.from == pawn_sq
                            && m.to == expected_to
                            && m.kind == MoveKind::DoublePush
                        });
                        assert!(has_double_push,
                            "Black pawn on rank 8 ({}) should have \
                             double push to rank 6 ({}) — seed {}",
                            pawn_sq, expected_to, seed
                        );
                    }
                }
                break;
            }
        }
        assert!(found_rank8_pawn,
            "Should find at least one position with rank 8 pawn in 200 seeds");
    }

    #[test]
    fn test_pawn_capture() {
        setup();
        // White pawn on e4, Black pawn on d5 — can capture
        let fen =
            "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_pawn_diagonal_captures(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 1, "White pawn should capture d5 pawn");
        assert_eq!(list.get(0).from, Square::E4);
        assert_eq!(list.get(0).to, Square::D5);
        assert_eq!(list.get(0).kind, MoveKind::Capture);
    }

    #[test]
    fn test_en_passant() {
        setup();
        // White pawn on e5, Black just played d7-d5
        // En passant target = d6
        let fen =
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_en_passant(&pos, Color::White, &mut list);
        assert_eq!(list.len(), 1, "Should have one en passant capture");
        assert_eq!(list.get(0).from, Square::E5);
        assert_eq!(list.get(0).to, Square::D6);
        assert_eq!(list.get(0).kind, MoveKind::EnPassant);
    }

    #[test]
fn test_promotion() {
    setup();
    // White pawn on e7, can promote — Black King moved away from e8
    let fen = "3k4/4P3/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);
        // Should generate 4 promotion moves
        assert_eq!(list.len(), 4,
            "Pawn on e7 should generate 4 promotion moves");
        let kinds: Vec<MoveKind> = list.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&MoveKind::PromoQueen));
        assert!(kinds.contains(&MoveKind::PromoRook));
        assert!(kinds.contains(&MoveKind::PromoBishop));
        assert!(kinds.contains(&MoveKind::PromoKnight));
    }

    #[test]
    fn test_promotion_capture() {
        setup();
        // White pawn on e7, Black piece on d8 — promotion capture
        let fen = "3nk3/4P3/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_pawn_diagonal_captures(&pos, Color::White, &mut list);
        // Should generate 4 promotion capture moves
        assert_eq!(list.len(), 4,
            "Should generate 4 promotion capture moves");
        for mv in list.iter() {
            assert!(mv.kind == MoveKind::PromoCapQueen
                 || mv.kind == MoveKind::PromoCapRook
                 || mv.kind == MoveKind::PromoCapBishop
                 || mv.kind == MoveKind::PromoCapKnight,
                "Should be a promotion capture");
        }
    }

    #[test]
    fn test_blocked_pawn_no_push() {
        setup();
        // White pawn blocked by own piece
        let fen = "4k3/8/8/8/8/4N3/4P3/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);
        // e2 pawn blocked by knight on e3
        let e2_moves: Vec<_> = list.iter()
            .filter(|m| m.from == Square::E2)
            .collect();
        assert_eq!(e2_moves.len(), 0,
            "Pawn blocked by own piece should have no pushes");
    }

    #[test]
    fn test_double_push_blocked_intermediate() {
        setup();
        // White pawn on e2, piece on e3 — no double push possible
        let fen = "4k3/8/8/8/8/4p3/4P3/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let mut list = MoveList::new();
        generate_pawn_pushes(&pos, Color::White, &mut list);
        let e2_double: Vec<_> = list.iter()
            .filter(|m| m.from == Square::E2
                     && m.kind == MoveKind::DoublePush)
            .collect();
        assert_eq!(e2_double.len(), 0,
            "Double push blocked when intermediate square occupied");
    }

    #[test]
    fn test_pet_dragon_1000_positions_pawn_moves() {
        setup();
        // Verify pawn generation doesn't panic for 1000 Pet Dragon positions
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let mut list = MoveList::new();
            generate_pawn_moves(&pos, Color::White, &mut list);
            generate_pawn_moves(&pos, Color::Black, &mut list);
            // Should always generate some moves
            assert!(list.len() > 0,
                "Should have pawn moves in Pet Dragon position (seed {})",
                seed);
        }
    }
}
