// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// tests/setup.rs — Integration tests for Pet Dragon position generator
//
// These tests run from outside the engine modules (integration tests)
// and validate every Pet Dragon rule across 1000 generated positions.
//
// Rules validated:
//   1. White King always on e1
//   2. Black King always on e8
//   3. All White pieces on ranks 1-2
//   4. All Black pieces on ranks 7-8
//   5. Both Bishops on opposite coloured squares (both sides)
//   6. Black strictly mirrors White (file preserved, rank mirrored)
//   7. Correct piece counts for both sides (8P 2N 2B 2R 1Q 1K)
//   8. Per-pawn start squares recorded for all pawns
//   9. Castling rights only set when Rook on standard square
//  10. No pieces in middle ranks (3-6) at setup
//  11. Exactly 32 pieces on board
//  12. FEN roundtrip preserves position perfectly
//  13. Same seed produces same position (reproducibility)
//  14. Different seeds produce different positions
//  15. Standard chess starting position is a valid Pet Dragon arrangement
// ============================================================================

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::bitboard::Bitboard;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::types::{Color, PieceKind, Square};

/// Initialise all engine tables before any test runs
fn setup() {
    init_masks();
    init_magic();
    init_zobrist();
}

// ── Core rule tests ───────────────────────────────────────────────────────────

#[test]
fn test_king_always_fixed() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        assert_eq!(
            pos.king_sq(Color::White), Square::E1,
            "White King not on e1 at seed {}", seed
        );
        assert_eq!(
            pos.king_sq(Color::Black), Square::E8,
            "Black King not on e8 at seed {}", seed
        );
    }
}

#[test]
fn test_all_white_pieces_on_home_ranks() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        let outside = (pos.occupied(Color::White)
                      & !Bitboard::WHITE_SETUP_RANKS).count();
        assert_eq!(outside, 0,
            "White pieces found outside ranks 1-2 at seed {}", seed);
    }
}

#[test]
fn test_all_black_pieces_on_home_ranks() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        let outside = (pos.occupied(Color::Black)
                      & !Bitboard::BLACK_SETUP_RANKS).count();
        assert_eq!(outside, 0,
            "Black pieces found outside ranks 7-8 at seed {}", seed);
    }
}

#[test]
fn test_bishops_opposite_colours_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        for color in [Color::White, Color::Black] {
            let bb = pos.piece_bb(color, PieceKind::Bishop);
            let light = (bb & Bitboard::LIGHT_SQUARES).count();
            let dark  = (bb & Bitboard::DARK_SQUARES).count();
            assert_eq!(light, 1,
                "{:?} light bishop count wrong at seed {}: {}",
                color, seed, light);
            assert_eq!(dark, 1,
                "{:?} dark bishop count wrong at seed {}: {}",
                color, seed, dark);
        }
    }
}

#[test]
fn test_black_mirrors_white_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        for sq in Square::all() {
            if let Some(white_kind) = pos.piece_on(sq, Color::White) {
                let mirror_sq = sq.mirror_rank();
                let black_kind = pos.piece_on(mirror_sq, Color::Black);
                assert_eq!(
                    black_kind, Some(white_kind),
                    "Mirror mismatch at seed {}: \
                     White {:?} on {} ↔ Black {:?} on {}",
                    seed, white_kind, sq, black_kind, mirror_sq
                );
            }
        }
    }
}

#[test]
fn test_piece_counts_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        for color in [Color::White, Color::Black] {
            assert_eq!(pos.count_pieces(color, PieceKind::Pawn),   8,
                "{:?} pawn count wrong at seed {}", color, seed);
            assert_eq!(pos.count_pieces(color, PieceKind::Knight), 2,
                "{:?} knight count wrong at seed {}", color, seed);
            assert_eq!(pos.count_pieces(color, PieceKind::Bishop), 2,
                "{:?} bishop count wrong at seed {}", color, seed);
            assert_eq!(pos.count_pieces(color, PieceKind::Rook),   2,
                "{:?} rook count wrong at seed {}", color, seed);
            assert_eq!(pos.count_pieces(color, PieceKind::Queen),  1,
                "{:?} queen count wrong at seed {}", color, seed);
            assert_eq!(pos.count_pieces(color, PieceKind::King),   1,
                "{:?} king count wrong at seed {}", color, seed);
        }
    }
}

#[test]
fn test_total_piece_count_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        assert_eq!(pos.all_pieces().count(), 32,
            "Wrong total piece count at seed {}", seed);
    }
}

#[test]
fn test_pawn_starts_all_recorded_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);

        // Every White pawn must have its start square recorded
        let mut white_pawns = pos.piece_bb(Color::White, PieceKind::Pawn);
        while let Some(sq) = white_pawns.pop_lsb() {
            assert!(
                pos.pawn_starts.started_here(sq, Color::White),
                "White pawn on {} missing start record at seed {}",
                sq, seed
            );
        }

        // Every Black pawn must have its start square recorded
        let mut black_pawns = pos.piece_bb(Color::Black, PieceKind::Pawn);
        while let Some(sq) = black_pawns.pop_lsb() {
            assert!(
                pos.pawn_starts.started_here(sq, Color::Black),
                "Black pawn on {} missing start record at seed {}",
                sq, seed
            );
        }
    }
}

#[test]
fn test_pawn_starts_count_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        // Should be exactly 8 White pawn starts and 8 Black pawn starts
        let mut white_count = 0;
        let mut black_count = 0;
        for sq in Square::all() {
            match pos.pawn_starts.get(sq) {
                Some(Color::White) => white_count += 1,
                Some(Color::Black) => black_count += 1,
                None => {}
            }
        }
        assert_eq!(white_count, 8,
            "Should record exactly 8 White pawn starts at seed {}", seed);
        assert_eq!(black_count, 8,
            "Should record exactly 8 Black pawn starts at seed {}", seed);
    }
}

#[test]
fn test_castling_rights_consistency_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);

        // If castling right is set, Rook MUST be on standard square
        if pos.castling.white_kingside {
            assert!(
                pos.piece_bb(Color::White, PieceKind::Rook)
                   .contains(Square::H1),
                "White KS castling set but no Rook on h1 at seed {}",
                seed
            );
        }
        if pos.castling.white_queenside {
            assert!(
                pos.piece_bb(Color::White, PieceKind::Rook)
                   .contains(Square::A1),
                "White QS castling set but no Rook on a1 at seed {}",
                seed
            );
        }
        if pos.castling.black_kingside {
            assert!(
                pos.piece_bb(Color::Black, PieceKind::Rook)
                   .contains(Square::H8),
                "Black KS castling set but no Rook on h8 at seed {}",
                seed
            );
        }
        if pos.castling.black_queenside {
            assert!(
                pos.piece_bb(Color::Black, PieceKind::Rook)
                   .contains(Square::A8),
                "Black QS castling set but no Rook on a8 at seed {}",
                seed
            );
        }

        // If Rook NOT on standard square, castling must NOT be set
        if !pos.piece_bb(Color::White, PieceKind::Rook)
               .contains(Square::H1) {
            assert!(!pos.castling.white_kingside,
                "White KS castling set despite no Rook on h1 at seed {}",
                seed);
        }
        if !pos.piece_bb(Color::White, PieceKind::Rook)
               .contains(Square::A1) {
            assert!(!pos.castling.white_queenside,
                "White QS castling set despite no Rook on a1 at seed {}",
                seed);
        }
    }
}

#[test]
fn test_no_pieces_in_middle_ranks_1000() {
    setup();
    let middle = !(Bitboard::WHITE_SETUP_RANKS | Bitboard::BLACK_SETUP_RANKS);
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        assert!(
            (pos.all_pieces() & middle).is_empty(),
            "Pieces found in ranks 3-6 at seed {}", seed
        );
    }
}

#[test]
fn test_no_overlap_white_black() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        let overlap = pos.occupied(Color::White)
                    & pos.occupied(Color::Black);
        assert!(overlap.is_empty(),
            "White and Black pieces overlap at seed {}", seed);
    }
}

// ── Validate method agrees ────────────────────────────────────────────────────

#[test]
fn test_validate_passes_1000() {
    setup();
    for seed in 0..1000u64 {
        let pos = Position::generate_with_seed(seed);
        assert!(
            pos.validate_pet_dragon_setup().is_ok(),
            "Validation failed at seed {}: {:?}",
            seed,
            pos.validate_pet_dragon_setup()
        );
    }
}

// ── Reproducibility and uniqueness ───────────────────────────────────────────

#[test]
fn test_same_seed_same_position() {
    setup();
    for seed in [0u64, 1, 42, 100, 999, 12345] {
        let pos1 = Position::generate_with_seed(seed);
        let pos2 = Position::generate_with_seed(seed);
        assert_eq!(pos1.hash, pos2.hash,
            "Same seed {} produced different positions", seed);
    }
}

#[test]
fn test_different_seeds_different_positions() {
    setup();
    // With 1000 seeds, we expect many different hashes
    let hashes: std::collections::HashSet<u64> = (0..100u64)
        .map(|seed| Position::generate_with_seed(seed).hash)
        .collect();
    assert!(hashes.len() > 50,
        "Too few unique positions across 100 seeds: {}", hashes.len());
}

// ── FEN roundtrip ─────────────────────────────────────────────────────────────

#[test]
fn test_fen_roundtrip_100_positions() {
    setup();
    for seed in 0..100u64 {
        let pos1 = Position::generate_with_seed(seed);
        let fen  = pos1.to_fen();
        let pos2 = Position::from_fen(&fen)
            .unwrap_or_else(|e| panic!(
                "FEN parse failed at seed {}: {:?}\nFEN: {}",
                seed, e, fen
            ));

        assert_eq!(pos1.hash, pos2.hash,
            "Hash mismatch after FEN roundtrip at seed {}", seed);

        // Verify pawn starts survived the roundtrip
        for sq in Square::all() {
            assert_eq!(
                pos1.pawn_starts.get(sq),
                pos2.pawn_starts.get(sq),
                "Pawn start mismatch on {} after FEN roundtrip at seed {}",
                sq, seed
            );
        }

        // Verify castling rights survived
        assert_eq!(pos1.castling, pos2.castling,
            "Castling mismatch after FEN roundtrip at seed {}", seed);
    }
}

// ── Pet Dragon specific rule tests ────────────────────────────────────────────

#[test]
fn test_pawn_start_ranks_valid() {
    setup();
    // White pawns should start on rank 1 or rank 2 only
    // Black pawns should start on rank 7 or rank 8 only
    for seed in 0..200u64 {
        let pos = Position::generate_with_seed(seed);
        for sq in Square::all() {
            if let Some(color) = pos.pawn_starts.get(sq) {
                let rank = sq.rank();
                match color {
                    Color::White => assert!(
                        rank == 0 || rank == 1,
                        "White pawn start on invalid rank {} at seed {}",
                        rank + 1, seed
                    ),
                    Color::Black => assert!(
                        rank == 6 || rank == 7,
                        "Black pawn start on invalid rank {} at seed {}",
                        rank + 1, seed
                    ),
                }
            }
        }
    }
}

#[test]
fn test_double_push_squares_exist() {
    setup();
    use pet_dragon_lib::bitboard::masks::pawn_double_push_mask;
    // Every recorded pawn start square should have a valid double push target
    for seed in 0..100u64 {
        let pos = Position::generate_with_seed(seed);
        for sq in Square::all() {
            if let Some(color) = pos.pawn_starts.get(sq) {
                let mask = pawn_double_push_mask(color, sq);
                assert!(
                    mask.is_not_empty(),
                    "{:?} pawn on {} has no double push target at seed {}",
                    color, sq, seed
                );
            }
        }
    }
}

#[test]
fn test_castling_probability() {
    setup();
    // Empirically verify castling availability rates
    // Theory: P(any castling) ≈ 26%, P(both sides castling) is lower
    let total = 1000u64;
    let mut any_castling = 0u64;
    let mut both_sides   = 0u64;

    for seed in 0..total {
        let pos = Position::generate_with_seed(seed);
        let white_can_castle = pos.castling.white_kingside
                            || pos.castling.white_queenside;
        let black_can_castle = pos.castling.black_kingside
                            || pos.castling.black_queenside;

        if white_can_castle || black_can_castle {
            any_castling += 1;
        }
        if white_can_castle && black_can_castle {
            both_sides += 1;
        }
    }

    // At least 10% should have some castling available
    // (actual rate ≈26%, allowing generous margin for randomness)
    assert!(any_castling > 100,
        "Too few positions with castling available: {}/{}",
        any_castling, total);

    // Should not be 100% — castling is not always available
    assert!(any_castling < total,
        "All positions have castling — something is wrong");

    println!(
        "Castling stats: any={}/{} ({:.1}%), both={}/{} ({:.1}%)",
        any_castling, total,
        any_castling as f64 / total as f64 * 100.0,
        both_sides, total,
        both_sides as f64 / total as f64 * 100.0
    );
}

#[test]
fn test_standard_chess_is_valid_pet_dragon() {
    setup();
    use pet_dragon_lib::position::fen::STANDARD_START_FEN;
    let pos = Position::from_fen(STANDARD_START_FEN).unwrap();
    assert!(
        pos.validate_pet_dragon_setup().is_ok(),
        "Standard chess start should be valid Pet Dragon: {:?}",
        pos.validate_pet_dragon_setup()
    );
}

#[test]
fn test_hash_computed_for_all() {
    setup();
    for seed in 0..100u64 {
        let pos = Position::generate_with_seed(seed);
        assert_ne!(pos.hash, 0,
            "Hash should not be zero at seed {}", seed);
    }
}
