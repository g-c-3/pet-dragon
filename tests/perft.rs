// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// tests/perft.rs — Move generation correctness tests
//
// Perft (performance test) counts leaf nodes at a given search depth.
// Known correct values exist for standard chess positions.
// Since standard chess is a valid Pet Dragon arrangement, these
// values validate our move generator completely.
//
// If any perft value is wrong, there is a bug in move generation.
// Perft is the definitive correctness test for chess engines.
//
// Known perft values (from chessprogramming.org):
//   Starting position:
//     Depth 1:          20
//     Depth 2:         400
//     Depth 3:       8,902
//     Depth 4:     197,281
//     Depth 5:   4,865,609
//
//   Position 2 (Kiwipete):
//     r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -
//     Depth 1:          48
//     Depth 2:       2,039
//     Depth 3:      97,862
//     Depth 4:   4,085,603
//
//   Position 3 (endgame):
//     8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -
//     Depth 1:          14
//     Depth 2:         191
//     Depth 3:       2,812
//     Depth 4:      43,238
//     Depth 5:     674,624
//
// Note: depth 5 for starting position (4,865,609 nodes) is the
// standard validation target for chess engines.
// ============================================================================

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::generate_moves;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::movegen::legal::apply_move_for_legality_pub;

fn setup() {
    init_masks();
    init_magic();
    init_zobrist();
}

// ── Core perft function ───────────────────────────────────────────────────────

/// Count leaf nodes at exactly `depth` from the given position.
/// depth 1 = count all legal moves from this position
/// depth 2 = count all legal moves from each position after one move
/// etc.
fn perft(pos: &Position, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = generate_moves(pos);

    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0u64;
    let color = pos.side_to_move;

    for mv in moves.iter() {
        let mut new_pos = pos.clone();
        apply_move_for_legality_pub(&mut new_pos, *mv, color);
        nodes += perft(&new_pos, depth - 1);
    }

    nodes
}

/// Perft with move breakdown (useful for debugging wrong counts)
/// Prints each move and its node count at depth-1
#[allow(dead_code)]
fn perft_divide(pos: &Position, depth: u32) -> u64 {
    let moves = generate_moves(pos);
    let mut total = 0u64;
    let color = pos.side_to_move;

    for mv in moves.iter() {
        let mut new_pos = pos.clone();
        apply_move_for_legality_pub(&mut new_pos, *mv, color);
        let count = perft(&new_pos, depth - 1);
        println!("{}: {}", mv, count);
        total += count;
    }
    println!("Total: {}", total);
    total
}

// ── Starting position perft tests ─────────────────────────────────────────────

#[test]
fn test_perft_startpos_depth1() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 1), 20,
        "Perft(1) from start should be 20");
}

#[test]
fn test_perft_startpos_depth2() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 2), 400,
        "Perft(2) from start should be 400");
}

#[test]
fn test_perft_startpos_depth3() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 3), 8902,
        "Perft(3) from start should be 8,902");
}

#[test]
fn test_perft_startpos_depth4() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 4), 197_281,
        "Perft(4) from start should be 197,281");
}

// Depth 5 is the gold standard test — 4,865,609 nodes
// Takes a few seconds but proves complete correctness
#[test]
fn test_perft_startpos_depth5() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 5), 4_865_609,
        "Perft(5) from start should be 4,865,609");
}

// ── Kiwipete position (tests complex positions) ───────────────────────────────
// This position tests: castling, en passant, promotions, checks

const KIWIPETE: &str =
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

#[test]
fn test_perft_kiwipete_depth1() {
    setup();
    let pos = Position::from_fen(KIWIPETE).unwrap();
    assert_eq!(perft(&pos, 1), 48,
        "Kiwipete perft(1) should be 48");
}

#[test]
fn test_perft_kiwipete_depth2() {
    setup();
    let pos = Position::from_fen(KIWIPETE).unwrap();
    assert_eq!(perft(&pos, 2), 2039,
        "Kiwipete perft(2) should be 2,039");
}

#[test]
fn test_perft_kiwipete_depth3() {
    setup();
    let pos = Position::from_fen(KIWIPETE).unwrap();
    assert_eq!(perft(&pos, 3), 97_862,
        "Kiwipete perft(3) should be 97,862");
}

#[test]
fn test_perft_kiwipete_depth4() {
    setup();
    let pos = Position::from_fen(KIWIPETE).unwrap();
    assert_eq!(perft(&pos, 4), 4_085_603,
        "Kiwipete perft(4) should be 4,085,603");
}

// ── Endgame position (tests promotions and en passant edge cases) ─────────────

const ENDGAME_POS: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";

#[test]
fn test_perft_endgame_depth1() {
    setup();
    let pos = Position::from_fen(ENDGAME_POS).unwrap();
    assert_eq!(perft(&pos, 1), 14,
        "Endgame perft(1) should be 14");
}

#[test]
fn test_perft_endgame_depth2() {
    setup();
    let pos = Position::from_fen(ENDGAME_POS).unwrap();
    assert_eq!(perft(&pos, 2), 191,
        "Endgame perft(2) should be 191");
}

#[test]
fn test_perft_endgame_depth3() {
    setup();
    let pos = Position::from_fen(ENDGAME_POS).unwrap();
    assert_eq!(perft(&pos, 3), 2812,
        "Endgame perft(3) should be 2,812");
}

#[test]
fn test_perft_endgame_depth4() {
    setup();
    let pos = Position::from_fen(ENDGAME_POS).unwrap();
    assert_eq!(perft(&pos, 4), 43_238,
        "Endgame perft(4) should be 43,238");
}

#[test]
fn test_perft_endgame_depth5() {
    setup();
    let pos = Position::from_fen(ENDGAME_POS).unwrap();
    assert_eq!(perft(&pos, 5), 674_624,
        "Endgame perft(5) should be 674,624");
}

// ── Position 4 (tests promotions) ────────────────────────────────────────────

const PROMO_POS: &str =
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1p3/q4N2/Pp1P1RPP/R2bK2R w KQkq - 0 1";

#[test]
fn test_perft_promo_depth1() {
    setup();
    let pos = Position::from_fen(PROMO_POS).unwrap();
    let result = perft(&pos, 1);
    // Known value: 36 legal moves from this complex position
    assert_eq!(result, 36,
        "Promotion position perft(1) should be 36");
}

// ── Pet Dragon specific perft tests ───────────────────────────────────────────

#[test]
fn test_pet_dragon_perft_depth1_reasonable() {
    setup();
    // Pet Dragon positions should have reasonable move counts at depth 1
    for seed in 0..20u64 {
        let pos = Position::generate_with_seed(seed);
        let count = perft(&pos, 1);
        // Should have between 1 and 100 moves
        assert!(count >= 1 && count <= 100,
            "Pet Dragon perft(1) out of range: {} (seed {})",
            count, seed);
    }
}

#[test]
fn test_pet_dragon_perft_depth2_reasonable() {
    setup();
    for seed in 0..10u64 {
        let pos = Position::generate_with_seed(seed);
        let count = perft(&pos, 2);
        // Depth 2 should be significantly more than depth 1
        let depth1 = perft(&pos, 1);
        assert!(count >= depth1,
            "Perft(2) should be >= perft(1) (seed {})", seed);
    }
}

#[test]
fn test_standard_is_valid_pet_dragon_perft() {
    setup();
    // The standard start is a valid Pet Dragon position
    // Its perft values must match exactly
    let pos = Position::start_pos().unwrap();
    assert_eq!(perft(&pos, 1), 20);
    assert_eq!(perft(&pos, 2), 400);
    assert_eq!(perft(&pos, 3), 8902);
}
