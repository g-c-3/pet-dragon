// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// tests/node_count.rs — Fixed-depth node count benchmarks
//
// Measures search node counts and NPS at fixed depth on standard positions.
// Establishes a baseline for tracking pruning efficiency over sessions.
//
// These tests are `#[ignore]`-tagged — they do NOT run in normal `cargo test`.
// They are executed by the "Node Count Bench" GitHub Actions job on every
// push to main, and results appear in the Actions log.
//
// Run locally:
//   cargo test --release node_count -- --ignored --nocapture
//
// ## Comparison notes
// Pet Dragon is a VARIANT-chess engine with custom pawn rules:
//   White pawns may start from rank 1 OR rank 2 (double-push available from both)
//   Black pawns may start from rank 8 OR rank 7
// Node counts at equivalent depths will be HIGHER than standard chess engines
// (Ethereal, Stockfish) due to the larger branching factor from extra pawn
// start squares. Use these numbers only to track Pet Dragon's own pruning
// efficiency over time — not for direct comparison against standard engines.
//
// ## Approximate Ethereal 14 baseline (standard chess, for rough reference)
//   Startpos depth 10 ≈ 1.2M – 2.5M nodes (varies by HW and build flags)
//
// ## Pet Dragon baselines (fill in after first Actions run)
//   Session 15 (Phase 13.6 cont_hist complete):
//     node_count_startpos_depth10   : nodes = ???,  nps = ???
//     node_count_kiwipete_depth9    : nodes = ???,  nps = ???
//     node_count_endgame_depth11    : nodes = ???,  nps = ???
//     node_count_tactical_depth9    : nodes = ???,  nps = ???
//     node_count_pet_dragon_rank1_d8: nodes = ???,  nps = ???
// ============================================================================

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::search::iterative::iterative_deepening;
use pet_dragon_lib::search::SearchInfo;
use pet_dragon_lib::search::time::TimeControl;
use pet_dragon_lib::tt::TranspositionTable;
use pet_dragon_lib::types::Move;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() {
    init_masks();
    init_magic();
    init_zobrist();
}

fn fixed_depth_tc(depth: i32) -> TimeControl {
    TimeControl { depth, ..Default::default() }
}

/// Run a fixed-depth search on the given FEN and return (nodes, nps, best_move).
///
/// Uses a 32 MB transposition table for consistent, realistic numbers.
fn bench_position(fen: &str, depth: i32) -> (u64, u64, Move) {
    let mut pos  = Position::from_fen(fen).expect("valid bench FEN");
    let mut info = SearchInfo::new();
    let tt       = TranspositionTable::new(32);
    let tc       = fixed_depth_tc(depth);

    let result = iterative_deepening(&mut pos, &tc, &mut info, &tt);
    (result.nodes, result.nps, result.best_move)
}

// ── Benchmark tests ───────────────────────────────────────────────────────────
// Each test is #[ignore] — skipped by `cargo test`, run by `cargo test -- --ignored`.

/// Starting position at depth 10 — primary search efficiency baseline.
///
/// All future sessions should compare against this number. A decrease in nodes
/// after adding a pruning technique means the technique is working.
#[test]
#[ignore]
fn node_count_startpos_depth10() {
    setup();
    let (nodes, nps, best) = bench_position(
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        10,
    );
    println!("=== Startpos depth 10 ===");
    println!("  Nodes : {nodes}");
    println!("  NPS   : {nps}");
    println!("  Best  : {best}");
    assert!(nodes > 0,          "must search at least one node");
    assert!(nps   > 0,          "must compute NPS");
    assert!(best != Move::NULL, "must return a legal move");
}

/// Kiwipete at depth 9 — high-branching-factor tactical reference position.
///
/// r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1
/// Rich in castling rights, pins, and captures. Standard engine torture test.
#[test]
#[ignore]
fn node_count_kiwipete_depth9() {
    setup();
    let (nodes, nps, best) = bench_position(
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
        9,
    );
    println!("=== Kiwipete depth 9 ===");
    println!("  Nodes : {nodes}");
    println!("  NPS   : {nps}");
    println!("  Best  : {best}");
    assert!(nodes > 0         );
    assert!(nps   > 0         );
    assert!(best != Move::NULL);
}

/// Rook-and-pawns endgame at depth 11 — tests search depth in simplified positions.
///
/// Fewer pieces → shallower real branching factor → deeper effective search.
/// Node count here reflects endgame search quality (less pruning needed, more depth).
#[test]
#[ignore]
fn node_count_endgame_depth11() {
    setup();
    let (nodes, nps, best) = bench_position(
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        11,
    );
    println!("=== Endgame (rook + pawns) depth 11 ===");
    println!("  Nodes : {nodes}");
    println!("  NPS   : {nps}");
    println!("  Best  : {best}");
    assert!(nodes > 0         );
    assert!(nps   > 0         );
    assert!(best != Move::NULL);
}

/// Tactical middlegame at depth 9 — Sicilian Dragon structure.
///
/// Tests move ordering quality. Good ordering → fewer nodes via early cutoffs.
/// A decrease here after ordering improvements (cont_hist, LMR) is expected.
#[test]
#[ignore]
fn node_count_tactical_depth9() {
    setup();
    // Sicilian Dragon middlegame — high piece activity, multiple threats
    let (nodes, nps, best) = bench_position(
        "r1bq1rk1/pp2ppbp/2np1np1/3pP3/2PP4/2NBBP2/PP4PP/R2QK1NR b KQ - 0 8",
        9,
    );
    println!("=== Tactical middlegame depth 9 ===");
    println!("  Nodes : {nodes}");
    println!("  NPS   : {nps}");
    println!("  Best  : {best}");
    assert!(nodes > 0         );
    assert!(nps   > 0         );
    assert!(best != Move::NULL);
}

/// Pet Dragon variant position at depth 8 — White pawn on rank 1.
///
/// Position: standard Black setup vs White with a pawn on a1 (rank 1).
/// White's a1 pawn can double-push to a3 — unique to Pet Dragon's pawn rules.
/// Node count here will be HIGHER than startpos-depth-8 due to extra pawn moves.
/// Tracks that the variant rule adds branching factor as expected.
#[test]
#[ignore]
fn node_count_pet_dragon_rank1_depth8() {
    setup();
    // White has a pawn on a1 (not a legal standard chess position).
    // Pet Dragon allows this — pawn starts on rank 1 can double-push.
    let (nodes, nps, best) = bench_position(
        "rnbqkbnr/pppppppp/8/8/8/8/1PPPPPPP/PNBQKBNR w KQkq - 0 1",
        8,
    );
    println!("=== Pet Dragon rank-1 pawn start depth 8 ===");
    println!("  Nodes : {nodes}");
    println!("  NPS   : {nps}");
    println!("  Best  : {best}");
    println!("  (Note: a1 pawn has double-push to a3 — extra branching vs startpos)");
    assert!(nodes > 0         );
    assert!(nps   > 0         );
    assert!(best != Move::NULL);
}
