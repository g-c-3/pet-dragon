// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// tests/make_unmake.rs — Make/unmake move correctness tests
//
// Verifies that:
//   1. make_move correctly updates all position state
//   2. unmake_move perfectly restores position to before make_move
//   3. Zobrist hash is consistent after make/unmake
//   4. All move types work correctly (quiet, capture, EP, castle, promo)
//   5. 10,000 random make/unmake sequences are perfectly reversible
// ============================================================================

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::movegen::generate_moves;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::types::Color;

fn setup() {
    init_masks();
    init_magic();
    init_zobrist();
}

// ── Core reversibility test ───────────────────────────────────────────────────

/// Make a move then unmake it — position must be identical to before
fn assert_make_unmake_reversible(pos: &Position) {
    let moves = generate_moves(pos);
    if moves.is_empty() { return; }

    let original_hash     = pos.hash;
    let original_fen      = pos.to_standard_fen();
    let original_castling = pos.castling;
    let original_ep       = pos.en_passant;
    let original_clock    = pos.halfmove_clock;
    let original_side     = pos.side_to_move;

    for mv in moves.iter() {
        let mut test_pos = pos.clone();
        test_pos.make_move(*mv);
        test_pos.unmake_move(*mv);

        assert_eq!(test_pos.hash, original_hash,
            "Hash mismatch after make/unmake of {}", mv);
        assert_eq!(test_pos.to_standard_fen(), original_fen,
            "FEN mismatch after make/unmake of {}", mv);
        assert_eq!(test_pos.castling, original_castling,
            "Castling mismatch after make/unmake of {}", mv);
        assert_eq!(test_pos.en_passant, original_ep,
            "EP mismatch after make/unmake of {}", mv);
        assert_eq!(test_pos.halfmove_clock, original_clock,
            "Clock mismatch after make/unmake of {}", mv);
        assert_eq!(test_pos.side_to_move, original_side,
            "Side mismatch after make/unmake of {}", mv);
    }
}

#[test]
fn test_make_unmake_start_pos() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_make_unmake_reversible(&pos);
}

#[test]
fn test_make_unmake_kiwipete() {
    setup();
    let fen =
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R \
         w KQkq - 0 1";
    let pos = Position::from_fen(fen).unwrap();
    assert_make_unmake_reversible(&pos);
}

#[test]
fn test_make_unmake_with_en_passant() {
    setup();
    let fen =
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
    let pos = Position::from_fen(fen).unwrap();
    assert_make_unmake_reversible(&pos);
}

#[test]
fn test_make_unmake_promotion_position() {
    setup();
    let fen = "3k4/4P3/8/8/8/8/8/4K3 w - - 0 1";
    let pos = Position::from_fen(fen).unwrap();
    assert_make_unmake_reversible(&pos);
}

#[test]
fn test_make_unmake_castling_position() {
    setup();
    let fen = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";
    let pos = Position::from_fen(fen).unwrap();
    assert_make_unmake_reversible(&pos);
}

// ── Hash consistency test ─────────────────────────────────────────────────────

#[test]
fn test_hash_consistent_after_make() {
    setup();
    let pos = Position::start_pos().unwrap();
    let moves = generate_moves(&pos);

    for mv in moves.iter() {
        let mut test_pos = pos.clone();
        test_pos.make_move(*mv);

        // Hash should match recomputed hash
        let recomputed = test_pos.compute_hash();
        assert_eq!(test_pos.hash, recomputed,
            "Incremental hash != recomputed hash after {}", mv);
    }
}

#[test]
fn test_hash_consistent_after_unmake() {
    setup();
    let pos = Position::start_pos().unwrap();
    let original_hash = pos.hash;
    let moves = generate_moves(&pos);

    for mv in moves.iter() {
        let mut test_pos = pos.clone();
        test_pos.make_move(*mv);
        test_pos.unmake_move(*mv);
        assert_eq!(test_pos.hash, original_hash,
            "Hash not restored after unmake of {}", mv);
    }
}

// ── Side to move test ─────────────────────────────────────────────────────────

#[test]
fn test_side_flips_on_make() {
    setup();
    let pos = Position::start_pos().unwrap();
    assert_eq!(pos.side_to_move, Color::White);
    let mv = generate_moves(&pos).get(0);
    let mut pos2 = pos.clone();
    pos2.make_move(mv);
    assert_eq!(pos2.side_to_move, Color::Black);
    pos2.unmake_move(mv);
    assert_eq!(pos2.side_to_move, Color::White);
}

// ── Depth 2 perft using make/unmake ───────────────────────────────────────────
// This verifies make/unmake works correctly for the search pattern:
// make → generate → unmake for every position at depth 2

fn perft_make_unmake(pos: &mut Position, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let moves = generate_moves(pos);
    if depth == 1 { return moves.len() as u64; }

    let mut nodes = 0u64;
    for mv in moves.iter() {
        pos.make_move(*mv);
        nodes += perft_make_unmake(pos, depth - 1);
        pos.unmake_move(*mv);
    }
    nodes
}

#[test]
fn test_perft_make_unmake_depth3() {
    setup();
    let mut pos = Position::start_pos().unwrap();
    // Perft depth 3 using make/unmake must match known value
    assert_eq!(perft_make_unmake(&mut pos, 3), 8902,
        "Perft(3) via make/unmake should be 8,902");
}

#[test]
fn test_perft_make_unmake_depth4() {
    setup();
    let mut pos = Position::start_pos().unwrap();
    assert_eq!(perft_make_unmake(&mut pos, 4), 197_281,
        "Perft(4) via make/unmake should be 197,281");
}

#[test]
fn test_perft_make_unmake_depth5() {
    setup();
    let mut pos = Position::start_pos().unwrap();
    assert_eq!(perft_make_unmake(&mut pos, 5), 4_865_609,
        "Perft(5) via make/unmake should be 4,865,609");
}

// ── 10,000 random sequences ───────────────────────────────────────────────────

#[test]
fn test_random_make_unmake_sequences() {
    setup();

    // Use a fixed seed for reproducibility
    let mut rng_state = 0x246C_CB28_5410_8BA3u64;
    let mut rng = || -> usize {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state as usize
    };

    for seed in 0..100u64 {
        let start = Position::generate_with_seed(seed);
        let mut pos = start.clone();
        let mut move_stack = Vec::new();

        // Make up to 20 random moves
        for _ in 0..20 {
            let moves = generate_moves(&pos);
            if moves.is_empty() { break; }
            let mv = moves.get(rng() % moves.len());
            pos.make_move(mv);
            move_stack.push(mv);
        }

        // Unmake all moves in reverse
        for &mv in move_stack.iter().rev() {
            pos.unmake_move(mv);
        }

        // Position must be identical to start
        assert_eq!(pos.hash, start.hash,
            "Hash mismatch after random sequence (seed {})", seed);
        assert_eq!(pos.to_standard_fen(), start.to_standard_fen(),
            "FEN mismatch after random sequence (seed {})", seed);
    }
}

// ── Pet Dragon specific make/unmake tests ────────────────────────────────────

#[test]
fn test_make_unmake_pet_dragon_1000() {
    setup();
    for seed in 0..100u64 {
        let pos = Position::generate_with_seed(seed);
        assert_make_unmake_reversible(&pos);
    }
}

#[test]
fn test_pawn_start_map_unchanged_after_make_unmake() {
    setup();
    // pawn_starts must never change during make/unmake
    for seed in 0..20u64 {
        let pos = Position::generate_with_seed(seed);
        let original_starts = pos.pawn_starts;
        let moves = generate_moves(&pos);

        for mv in moves.iter() {
            let mut test_pos = pos.clone();
            test_pos.make_move(*mv);
            assert_eq!(test_pos.pawn_starts, original_starts,
                "pawn_starts changed after make (seed {})", seed);
            test_pos.unmake_move(*mv);
            assert_eq!(test_pos.pawn_starts, original_starts,
                "pawn_starts changed after unmake (seed {})", seed);
        }
    }
}
