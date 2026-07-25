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

// ── Adversarial Pet Dragon perft (ROADMAP Phase 26 item 2, D78) ───────────────
//
// The tests above either use standard chess positions (external, known-
// correct perft values apply because standard chess is a valid Pet Dragon
// arrangement) or loose "reasonable range" checks on randomly seeded Pet
// Dragon positions (no independent oracle exists for arbitrary random
// starts, so only bounds can be asserted, not exact counts). Neither
// covers what this section does: deliberately constructed positions that
// exercise Pet Dragon's two genuinely custom mechanics — rank-1 pawn
// double-push and its interaction with en passant, and castling-path
// blocking — carried through multiple plies (not just a single move-list
// check, which `movegen/pawns.rs` and `movegen/castling.rs` already cover
// in their own unit tests). The expected node counts below are hand-
// enumerated, not generated by the engine itself — see each test's
// comment for the full count.

#[test]
fn test_perft_rank1_double_push_en_passant_depth1() {
    setup();
    // White King a1, White Pawn on its recorded start square e1, Black
    // King e8, Black Pawn d3 (positioned to capture en passant once White
    // double-pushes e1-e3, passing through e2). Same position as
    // `movegen::pawns::test_en_passant_after_rank1_double_push`, carried
    // to perft here to prove the mechanic survives a real make/unmake +
    // zobrist + legality round trip, not just direct move-list generation.
    let fen = "4k3/8/8/8/8/3p4/8/K3P3 w - - 0 1 e1:w";
    let pos = Position::from_fen(fen).unwrap();
    // Hand count: King a1 -> {a2,b1,b2} = 3 (none attacked by the far-off
    // black king or by the d3 pawn, which only covers c2/e2).
    // Pawn e1 -> {e2 (single push), e3 (double push, recorded start)} = 2.
    assert_eq!(perft(&pos, 1), 5,
        "perft(1): 3 king moves + 2 pawn moves (single/double push)");
}

#[test]
fn test_perft_rank1_double_push_en_passant_depth2() {
    setup();
    let fen = "4k3/8/8/8/8/3p4/8/K3P3 w - - 0 1 e1:w";
    let pos = Position::from_fen(fen).unwrap();
    // Hand count, branch by White's 5 depth-1 moves. Black's king (e8)
    // always has 5 free replies (d8,d7,e7,f7,f8 — White's pieces never
    // get close enough to attack any of them in this position), so each
    // branch's total is 5 + (Black pawn d3's move count):
    //   Ka2, Kb1, Kb2 (pawn still on e1, d3 has no diagonal target):
    //     pawn = {d3-d2} = 1  ->  5+1 = 6  (x3 branches = 18)
    //   e1-e2 (pawn now on e2, in d3's diagonal capture range):
    //     pawn = {d3-d2, d3xe2} = 2  ->  5+2 = 7
    //   e1-e3 (double push; en_passant target must be e2 — the square
    //   actually passed through, not e3, which isn't even a square d3
    //   can diagonally reach):
    //     pawn = {d3-d2, d3xe2 e.p.} = 2  ->  5+2 = 7
    // Total = 18 + 7 + 7 = 32.
    //
    // This discriminates a hardcoded-rank en-passant bug: if the target
    // were wrongly computed as e3 instead of e2 (a rank-2-to-rank-4
    // assumption applied to a rank-1 start), d3 could not diagonally
    // reach e3 at all, the e.p. capture would silently vanish, and this
    // total would come out as 31, not 32.
    assert_eq!(perft(&pos, 2), 32,
        "perft(2) must be 32 — see comment for the hand-verified branch \
         breakdown; a wrong en-passant target square (e3 instead of e2) \
         would silently drop to 31");
}

#[test]
fn test_perft_castling_blocked_by_intervening_piece_depth1() {
    setup();
    // White King e1, Knight f1 (randomly landed there — Pet Dragon's
    // random arrangement can place a non-rook piece between the king and
    // a rook that DID land on its standard square), Rook h1, Black King
    // e8. Same position as
    // `movegen::castling::test_castling_blocked_by_piece`, carried to
    // perft here. Note on ROADMAP wording: castling itself requires the
    // rook on its standard a1/h1/a8/h8 square (confirmed in
    // `castling.rs` — Pet Dragon's king is hardcoded e1/e8, so there is
    // no "unusual rook file" case to test; the real adversarial case is
    // an intervening piece blocking an otherwise-available castle, which
    // is what this position tests).
    let fen = "4k3/8/8/8/8/8/8/4KN1R w K - 0 1";
    let pos = Position::from_fen(fen).unwrap();
    // Hand count: King e1 -> {d1,d2,e2,f2} = 4 (f1 occupied by own
    // knight, excluded; none of the 4 attacked by the far-off black
    // king). Castling: 0 (path through f1 blocked — confirmed directly
    // by `test_castling_blocked_by_piece`).
    // Knight f1 -> {d2,e3,g3,h2} = 4.
    // Rook h1 -> {g1} along the rank (blocked at f1 by the knight) +
    // {h2,h3,h4,h5,h6,h7,h8} along the file (nothing in the way) = 8.
    assert_eq!(perft(&pos, 1), 16,
        "perft(1): king 4 + castling 0 (blocked by knight on f1) + \
         knight 4 + rook 8 = 16");
}

#[test]
fn test_perft_castling_blocked_by_intervening_piece_depth2() {
    setup();
    let fen = "4k3/8/8/8/8/8/8/4KN1R w K - 0 1";
    let pos = Position::from_fen(fen).unwrap();
    // Hand count, branch by White's 16 depth-1 moves, verified against a
    // standalone perft_divide run of this exact position (the first
    // version of this test had a hand-counting error — see below).
    // Black's king (e8) has 5 candidate replies (d8,d7,e7,f7,f8), reduced
    // whenever the h1 rook reaches a rank where it can attack one or more
    // of them:
    //   King moves (Kd1,Kd2,Ke2,Kf2): unaffected, 5 replies each
    //     -> 4 x 5 = 20
    //   Knight moves (Nd2,Ne3,Ng3,Nh2): unaffected, 5 replies each
    //     -> 4 x 5 = 20
    //   Rook moves Rg1,Rh2,Rh3,Rh4,Rh5,Rh6 (6 branches, none reach rank
    //     7 or 8): unaffected, 5 replies each -> 6 x 5 = 30
    //   Rook Rh7: now attacks along the fully open 7th rank (g7 through
    //     a7 — nothing blocks it), hitting d7, e7, AND f7. Only d8 and
    //     f8 (rank 8, untouched by a rook on h7) remain safe -> 2
    //   Rook Rh8: attacks along the fully open 8th rank instead (g8, f8,
    //     e8 itself — check). Legal replies are the 3 squares off both
    //     rank 8 and the h-file: d7, e7, f7 -> 3
    // Total = 20 + 20 + 30 + 2 + 3 = 75.
    //
    // The original version of this test asserted 78, having correctly
    // hand-counted the Rh8 branch (3) but wrongly assumed Rh7 was
    // unaffecting (assumed 5, actually 2) — missing that a rook doesn't
    // need to reach the king's own rank to restrict its mobility, only
    // the rank the king would be moving *to*. Caught by CI (`cargo test`
    // failed: left 75, right 78), root-caused by building the crate
    // standalone in a scratch cargo project and running `perft_divide`
    // directly against this FEN rather than re-guessing by hand a second
    // time.
    assert_eq!(perft(&pos, 2), 75,
        "perft(2) must be 75 — see comment for the hand-verified branch \
         breakdown, including both the Rh7 branch (restricts the king via \
         the now-open 7th rank without itself giving check) and the Rh8 \
         branch (gives check along the open 8th rank)");
}

