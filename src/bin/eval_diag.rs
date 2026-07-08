// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/eval_diag.rs — NNUE calibration diagnostic (Phase 17.5d)
//
// Prints HCE, raw quantized NNUE, and blended (100% NNUE) eval side-by-side
// for a handful of known positions with an intuitive "right answer" —
// starting position (~0), a position up a full queen for White (strongly
// positive), a position down a full queen for White (strongly negative),
// and a simple King+Pawn vs King winning endgame.
//
// Purpose: Session 42's retrain lowered val_loss (0.53776 -> 0.51661) but
// made every match_runner blend weight point MORE net-negative, not less
// (D29). Since val_loss is a fit metric on the training/validation
// distribution and doesn't directly show what the network outputs on
// positions with an unambiguous correct evaluation, this binary is a cheap,
// no-Kaggle-needed way to see directly whether the quantized network's raw
// output is sane (same sign as HCE, roughly plausible magnitude) or
// miscalibrated (wrong sign, saturated, or wildly out of scale) on cases
// where the right answer isn't in doubt.
//
// Usage (no terminal needed — triggered via GitHub Actions workflow_dispatch,
// see .github/workflows/eval_diag.yml):
//   cargo run --release --bin eval_diag
// ============================================================================

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::eval::{evaluate, evaluate_blended, set_nnue_weight_pct};
use pet_dragon_lib::nnue::inference::evaluate_nnue;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::types::{Color, PieceKind};

/// One labeled test position: a FEN string plus a human description of what
/// the "obviously correct" evaluation sign/magnitude should be, from
/// White's perspective (all evals below are reported from White's POV
/// regardless of side to move, for readability — see `white_pov`).
struct TestCase {
    label: &'static str,
    fen: &'static str,
    expectation: &'static str,
}

// Only generic, start-distribution-independent endgame checks stay static —
// by the time a real game reaches a simplified K+P vs K endgame, how it
// started is irrelevant. See the dynamic section in main() for the cases
// that actually matter for calibration.
const CASES: &[TestCase] = &[
    TestCase {
        label: "Trivial K+P vs K win for White",
        fen: "8/8/8/4k3/8/4P3/4K3/8 w - - 0 1",
        expectation: "should be positive (White has a winning extra pawn + \
                       supported king)",
    },
    TestCase {
        label: "Trivial K+P vs K win for Black (mirror of above)",
        fen: "8/4k3/4p3/8/4K3/8/8/8 b - - 0 1",
        expectation: "should be positive from Black's POV / negative from \
                       White's POV (Black has the winning extra pawn)",
    },
];

/// Remove the first (only, in a fresh Pet Dragon start) queen of the given
/// color from `pos`, returning true if one was found and removed.
fn remove_queen(pos: &mut Position, color: Color) -> bool {
    match pos.piece_bb(color, PieceKind::Queen).lsb() {
        Some(sq) => {
            pos.remove_piece(color, PieceKind::Queen, sq);
            true
        }
        None => false,
    }
}

/// Convert a side-to-move-relative eval into a White-POV eval for
/// consistent, human-readable comparison across test cases regardless of
/// whose move it is in the FEN.
fn white_pov(eval_stm: i32, pos: &Position) -> i32 {
    if pos.side_to_move == pet_dragon_lib::types::Color::White {
        eval_stm
    } else {
        -eval_stm
    }
}

/// Print one HCE/NNUE/blended comparison line for `pos`, from White's POV.
fn report(label: &str, expectation: &str, pos: &Position) {
    let hce_stm = evaluate(pos);
    let nnue_stm = evaluate_nnue(pos);

    set_nnue_weight_pct(100);
    let blended_100_stm = evaluate_blended(pos);
    set_nnue_weight_pct(0);

    let hce = white_pov(hce_stm, pos);
    let nnue = white_pov(nnue_stm, pos);
    let blended_100 = white_pov(blended_100_stm, pos);

    println!("== {label} ==");
    println!("  Expectation: {expectation}");
    println!("  HCE (White POV):          {:>6} cp", hce);
    println!("  NNUE raw (White POV):     {:>6} cp", nnue);
    println!("  Blended @ 100% (White POV): {:>6} cp", blended_100);
    let agree = (hce >= 0) == (nnue >= 0);
    println!(
        "  HCE/NNUE agree on sign: {}\n",
        if agree { "YES" } else { "NO — MISCALIBRATION SUSPECT" }
    );
}

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    println!("Pet Dragon NNUE calibration diagnostic (Phase 17.5d, revised)");
    println!("All evals shown from White's POV. HCE and NNUE are both");
    println!("independently in centipawns before any blending.");
    println!(
        "Random-start cases use Position::generate_with_seed(N) — the SAME \
         generator selfplay.rs/match_runner.rs use for every real game, so \
         these are in-distribution, unlike a classic-chess-layout FEN would \
         be (astronomically rare under random rank-1/2 piece scatter, per \
         setup.rs — see D32).\n"
    );

    // ── Primary signal: real Pet Dragon random starts ──────────────────────
    // Black exactly mirrors White's arrangement (setup.rs Step 6), so these
    // are genuinely symmetric positions — "~0" remains the right expectation
    // despite being far more materially varied than a classic chess start.
    for seed in 1..=3u64 {
        let pos = Position::generate_with_seed(seed);
        report(
            &format!("Random Pet Dragon start (seed={seed})"),
            "should be close to 0 (Black exactly mirrors White)",
            &pos,
        );
    }

    // Material-imbalance cases derived from a REAL random start (seed=1),
    // not a hand-built classic-chess FEN — removing a queen from an actual
    // in-distribution position.
    let mut up_a_queen = Position::generate_with_seed(1);
    if remove_queen(&mut up_a_queen, Color::Black) {
        report(
            "Random start (seed=1), White up a queen",
            "should be strongly positive (White massively ahead)",
            &up_a_queen,
        );
    }

    let mut down_a_queen = Position::generate_with_seed(1);
    if remove_queen(&mut down_a_queen, Color::White) {
        report(
            "Random start (seed=1), White down a queen",
            "should be strongly negative (White massively behind)",
            &down_a_queen,
        );
    }

    // ── Secondary/informational: the classic chess layout ──────────────────
    // Still a technically-valid Pet Dragon position (VARIANT_ARCHITECTURE.md
    // notes this explicitly), but essentially never occurs under real random
    // generation — treat any miscalibration here as informational, not the
    // primary signal.
    if let Ok(pos) =
        Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
    {
        report(
            "Classic chess layout (OUT-OF-DISTRIBUTION — informational only)",
            "technically valid but ~never hit by real random generation; \
             a miscalibration here alone doesn't confirm an in-game problem",
            &pos,
        );
    }

    // ── Generic, start-independent endgame checks ───────────────────────────
    for case in CASES {
        match Position::from_fen(case.fen) {
            Ok(pos) => report(case.label, case.expectation, &pos),
            Err(e) => println!("{}: FEN PARSE ERROR: {:?}", case.label, e),
        }
    }
}
