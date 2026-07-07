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

/// One labeled test position: a FEN string plus a human description of what
/// the "obviously correct" evaluation sign/magnitude should be, from
/// White's perspective (all evals below are reported from White's POV
/// regardless of side to move, for readability — see `white_pov`).
struct TestCase {
    label: &'static str,
    fen: &'static str,
    expectation: &'static str,
}

const CASES: &[TestCase] = &[
    TestCase {
        label: "Start position",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        expectation: "should be close to 0 (roughly balanced)",
    },
    TestCase {
        label: "White up a queen",
        fen: "rnb1kbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        expectation: "should be strongly positive (White massively ahead)",
    },
    TestCase {
        label: "White down a queen",
        fen: "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNB1KBNR w KQkq - 0 1",
        expectation: "should be strongly negative (White massively behind)",
    },
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

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    println!("Pet Dragon NNUE calibration diagnostic (Phase 17.5d)");
    println!("All evals shown from White's POV. HCE and NNUE are both");
    println!("independently in centipawns before any blending.\n");

    for case in CASES {
        let pos = match Position::from_fen(case.fen) {
            Ok(p) => p,
            Err(e) => {
                println!("{}: FEN PARSE ERROR: {:?}", case.label, e);
                continue;
            }
        };

        let hce_stm = evaluate(&pos);
        let nnue_stm = evaluate_nnue(&pos);

        set_nnue_weight_pct(100);
        let blended_100_stm = evaluate_blended(&pos);
        set_nnue_weight_pct(0);

        let hce = white_pov(hce_stm, &pos);
        let nnue = white_pov(nnue_stm, &pos);
        let blended_100 = white_pov(blended_100_stm, &pos);

        println!("== {} ==", case.label);
        println!("  FEN: {}", case.fen);
        println!("  Expectation: {}", case.expectation);
        println!("  HCE (White POV):          {:>6} cp", hce);
        println!("  NNUE raw (White POV):     {:>6} cp", nnue);
        println!("  Blended @ 100% (White POV): {:>6} cp", blended_100);
        let agree = (hce >= 0) == (nnue >= 0);
        println!(
            "  HCE/NNUE agree on sign: {}\n",
            if agree { "YES" } else { "NO — MISCALIBRATION SUSPECT" }
        );
    }
}
