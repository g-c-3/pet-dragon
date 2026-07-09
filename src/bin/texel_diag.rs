// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/texel_diag.rs — Texel-tuned HCE weights sanity diagnostic
// (Phase 14, D35 — the check flagged as missing after Session 55's first
// two real tuning runs)
//
// Prints HCE eval under the CURRENT default weights side by side with HCE
// eval under a candidate texel_tune.rs output, for the same test-case set
// eval_diag.rs already uses (real Pet Dragon random starts — should be
// ~0 since Black exactly mirrors White's setup — plus up-a-queen/
// down-a-queen swings and a couple of generic endgame checks). Where
// eval_diag.rs checks "does HCE and NNUE roughly agree", this checks
// "does the TUNED HCE still get the obviously-correct answer on cases
// where the answer isn't in doubt" — a raw parameter-value sanity check
// (like the one that caught the first tuning run's negative bishop_pair
// and negative attacker_weight entries) doesn't catch every possible
// failure mode; this catches failures that only show up when the
// parameters interact in an actual position.
//
// Takes a path to a texel_tune.rs output file (the exact
// s(mg,eg)/array-literal text format `write_tuned_weights` produces) and
// parses it back into a `TunableWeights` by scanning for `s(mg, eg)`
// tokens and scalar fields IN THE SAME ORDER `write_tuned_weights` emits
// them — the inverse of that function, not a general-purpose parser.
// Asserts the expected token count (476 `s(mg,eg)` pairs, matching
// `TunableWeightsF64::PARAM_COUNT`'s accounting) and panics loudly on a
// mismatch rather than silently misassigning fields.
//
// Usage (GitHub Actions only, per D15 — see .github/workflows/texel_diag.yml):
//   cargo run --release --bin texel_diag -- <tuned_weights_path>
// ============================================================================

use std::env;
use std::fs;

use pet_dragon_lib::bitboard::magic::init_magic;
use pet_dragon_lib::bitboard::masks::init_masks;
use pet_dragon_lib::position::zobrist::init_zobrist;
use pet_dragon_lib::position::Position;
use pet_dragon_lib::texel::features::extract_features;
use pet_dragon_lib::texel::predict::predict;
use pet_dragon_lib::texel::weights::TunableWeights;
use pet_dragon_lib::types::{Color, PieceKind};

/// Number of `s(mg, eg)` pairs `write_tuned_weights` emits, in order:
/// material_values(5) + bishop_pair(1) + pst(6*64=384) + mobility(9+14+15+28=66)
/// + isolated/doubled/backward(3) + passed_pawn_bonus(8) + open_lines(9) = 476.
const EXPECTED_PAIR_COUNT: usize = 5 + 1 + 384 + 66 + 3 + 8 + 9;

fn main() {
    init_masks();
    init_magic();
    init_zobrist();

    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: texel_diag <tuned_weights_path>");
        std::process::exit(1);
    };

    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("ERROR: could not read {}: {}", path, e);
        std::process::exit(1);
    });

    let default_weights = TunableWeights::default();
    let tuned_weights = parse_tuned_weights(&text);

    println!("Pet Dragon Texel-tuned HCE sanity diagnostic (Phase 14, D35)");
    println!("Comparing DEFAULT weights (current eval/*.rs consts) vs TUNED");
    println!("weights from: {}\n", path);
    println!(
        "Same test-case philosophy as eval_diag.rs — real Pet Dragon random \
         starts via Position::generate_with_seed (in-distribution, per D32), \
         plus material swings and generic endgame checks. HCE(default) is \
         expected to be bit-exact with crate::eval::evaluate() (Session 53's \
         self-consistency test already proves this); the comparison here is \
         DEFAULT-vs-TUNED, not TUNED-vs-evaluate().\n"
    );

    let mut mismatches = 0usize;

    // ── Primary signal: real Pet Dragon random starts (should be ~0) ──────
    for seed in 1..=5u64 {
        let pos = Position::generate_with_seed(seed);
        mismatches += report(
            &format!("Random Pet Dragon start (seed={seed})"),
            "should be close to 0 (Black exactly mirrors White)",
            &pos,
            &default_weights,
            &tuned_weights,
            SignCheck::NearZero,
        );
    }

    // ── Material-imbalance cases, derived from a real random start ────────
    let mut up_a_queen = Position::generate_with_seed(1);
    if remove_queen(&mut up_a_queen, Color::Black) {
        mismatches += report(
            "Random start (seed=1), White up a queen",
            "should be strongly positive (White massively ahead)",
            &up_a_queen,
            &default_weights,
            &tuned_weights,
            SignCheck::Positive,
        );
    }

    let mut down_a_queen = Position::generate_with_seed(1);
    if remove_queen(&mut down_a_queen, Color::White) {
        mismatches += report(
            "Random start (seed=1), White down a queen",
            "should be strongly negative (White massively behind)",
            &down_a_queen,
            &default_weights,
            &tuned_weights,
            SignCheck::Negative,
        );
    }

    let mut up_a_rook = Position::generate_with_seed(2);
    if remove_piece_of_kind(&mut up_a_rook, Color::Black, PieceKind::Rook) {
        mismatches += report(
            "Random start (seed=2), White up a rook",
            "should be positive (White ahead)",
            &up_a_rook,
            &default_weights,
            &tuned_weights,
            SignCheck::Positive,
        );
    }

    // ── Generic, start-independent endgame checks ──────────────────────────
    if let Ok(pos) = Position::from_fen("8/8/8/4k3/8/4P3/4K3/8 w - - 0 1") {
        mismatches += report(
            "Trivial K+P vs K win for White",
            "should be positive (White has a winning extra pawn)",
            &pos,
            &default_weights,
            &tuned_weights,
            SignCheck::Positive,
        );
    }
    if let Ok(pos) = Position::from_fen("8/4k3/4p3/8/4K3/8/8/8 b - - 0 1") {
        mismatches += report(
            "Trivial K+P vs K win for Black (mirror)",
            "Black is up a pawn and winning, so White-POV eval should be negative",
            &pos,
            &default_weights,
            &tuned_weights,
            SignCheck::Negative,
        );
    }

    println!("== Summary ==");
    if mismatches == 0 {
        println!("All cases: TUNED weights agree in sign/direction with DEFAULT weights. PASS.");
    } else {
        println!(
            "{} case(s) where TUNED weights DISAGREE in sign/direction with the \
             expected outcome — inspect above before writing the eval/*.rs delta. FAIL.",
            mismatches
        );
    }
}

enum SignCheck {
    NearZero,
    Positive,
    Negative,
}

/// Convert a side-to-move-relative eval into White-POV, matching
/// eval_diag.rs's convention.
fn white_pov(eval_stm: i32, pos: &Position) -> i32 {
    if pos.side_to_move == Color::White {
        eval_stm
    } else {
        -eval_stm
    }
}

fn remove_queen(pos: &mut Position, color: Color) -> bool {
    remove_piece_of_kind(pos, color, PieceKind::Queen)
}

fn remove_piece_of_kind(pos: &mut Position, color: Color, kind: PieceKind) -> bool {
    match pos.piece_bb(color, kind).lsb() {
        Some(sq) => {
            pos.remove_piece(color, kind, sq);
            true
        }
        None => false,
    }
}

/// Print one DEFAULT-vs-TUNED comparison line and return 1 if the tuned
/// weights disagree with `check`'s expected sign/direction (a real
/// regression worth stopping for), 0 otherwise. Disagreeing with the
/// DEFAULT weights' exact magnitude is fine and expected — tuning is
/// supposed to change numbers; disagreeing on the SIGN of an
/// unambiguous case is not.
fn report(
    label: &str,
    expectation: &str,
    pos: &Position,
    default_weights: &TunableWeights,
    tuned_weights: &TunableWeights,
    check: SignCheck,
) -> usize {
    let features = extract_features(pos);
    let default_eval = white_pov(predict(&features, default_weights), pos);
    let tuned_eval = white_pov(predict(&features, tuned_weights), pos);

    let tuned_ok = match check {
        SignCheck::NearZero => tuned_eval.abs() < 150,
        SignCheck::Positive => tuned_eval > 0,
        SignCheck::Negative => tuned_eval < 0,
    };

    println!("== {label} ==");
    println!("  Expectation: {expectation}");
    println!("  HCE default (White POV): {:>6} cp", default_eval);
    println!("  HCE tuned   (White POV): {:>6} cp", tuned_eval);
    println!(
        "  Tuned weights match expectation: {}\n",
        if tuned_ok { "YES" } else { "NO — REGRESSION SUSPECT" }
    );

    if tuned_ok {
        0
    } else {
        1
    }
}

/// Parse a `write_tuned_weights`-format file back into `TunableWeights` —
/// the inverse of that function, scanning for tokens IN THE EXACT ORDER
/// they were written (not a general-purpose Rust literal parser).
fn parse_tuned_weights(text: &str) -> TunableWeights {
    let pairs = extract_s_pairs(text);
    assert_eq!(
        pairs.len(),
        EXPECTED_PAIR_COUNT,
        "expected {} s(mg,eg) pairs in tuned weights file, found {} — file format \
         doesn't match write_tuned_weights' output, refusing to guess field assignment",
        EXPECTED_PAIR_COUNT,
        pairs.len()
    );

    let mut i = 0usize;
    let mut next_s = || {
        let (mg, eg) = pairs[i];
        i += 1;
        pet_dragon_lib::eval::material::s(mg, eg)
    };

    let mut material_values = [0i64; 5];
    for slot in material_values.iter_mut() {
        *slot = next_s();
    }
    let bishop_pair = next_s();

    let mut pst = [[0i64; 64]; 6];
    for row in pst.iter_mut() {
        for slot in row.iter_mut() {
            *slot = next_s();
        }
    }

    let mut knight_mobility = [0i64; 9];
    for slot in knight_mobility.iter_mut() {
        *slot = next_s();
    }
    let mut bishop_mobility = [0i64; 14];
    for slot in bishop_mobility.iter_mut() {
        *slot = next_s();
    }
    let mut rook_mobility = [0i64; 15];
    for slot in rook_mobility.iter_mut() {
        *slot = next_s();
    }
    let mut queen_mobility = [0i64; 28];
    for slot in queen_mobility.iter_mut() {
        *slot = next_s();
    }

    let isolated_penalty = next_s();
    let doubled_penalty = next_s();
    let backward_penalty = next_s();
    let mut passed_pawn_bonus = [0i64; 8];
    for slot in passed_pawn_bonus.iter_mut() {
        *slot = next_s();
    }

    let rook_open_file = next_s();
    let rook_semi_open_file = next_s();
    let rook_on_seventh = next_s();
    let rooks_connected = next_s();
    let queen_open_file = next_s();
    let queen_semi_open_file = next_s();
    let battery_rook_queen = next_s();
    let battery_bishop_queen = next_s();
    let contested_file = next_s();

    assert_eq!(i, EXPECTED_PAIR_COUNT, "s(mg,eg) pair count mismatch after parsing");

    let attacker_weight = extract_int_array(text, "attacker_weight");
    let open_file_near_king = extract_scalar(text, "open_file_near_king");
    let semi_open_file_near_king = extract_scalar(text, "semi_open_file_near_king");
    let pawn_shield_bonus = extract_scalar(text, "pawn_shield_bonus");
    let tempo = extract_scalar(text, "tempo");

    TunableWeights {
        material_values,
        bishop_pair,
        pst,
        knight_mobility,
        bishop_mobility,
        rook_mobility,
        queen_mobility,
        isolated_penalty,
        doubled_penalty,
        backward_penalty,
        passed_pawn_bonus,
        attacker_weight,
        open_file_near_king,
        semi_open_file_near_king,
        pawn_shield_bonus,
        rook_open_file,
        rook_semi_open_file,
        rook_on_seventh,
        rooks_connected,
        queen_open_file,
        queen_semi_open_file,
        battery_rook_queen,
        battery_bishop_queen,
        contested_file,
        tempo,
    }
}

/// Scan `text` left to right for every `s(<int>, <int>)` occurrence, in
/// order, and return the (mg, eg) pairs. Hand-rolled rather than a regex
/// dependency (none of this project's other bins pull one in) — the
/// format is fixed and simple enough that a small manual scanner is more
/// robust than adding a new dependency for one parser.
fn extract_s_pairs(text: &str) -> Vec<(i32, i32)> {
    let mut pairs = Vec::new();
    let bytes = text.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b's' && idx + 1 < bytes.len() && bytes[idx + 1] == b'(' {
            let start = idx + 2;
            if let Some(rel_end) = text[start..].find(')') {
                let inner = &text[start..start + rel_end];
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    if let (Ok(mg), Ok(eg)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                        pairs.push((mg, eg));
                        idx = start + rel_end + 1;
                        continue;
                    }
                }
            }
        }
        idx += 1;
    }
    pairs
}

/// Extract `<key>: [a, b, c, ...],` as an `[i32; 8]` — used only for
/// `attacker_weight`.
fn extract_int_array(text: &str, key: &str) -> [i32; 8] {
    let needle = format!("{}: [", key);
    let start = text.find(&needle).unwrap_or_else(|| {
        panic!("could not find '{}' array in tuned weights file", key)
    }) + needle.len();
    let end = text[start..].find(']').expect("unterminated array") + start;
    let inner = &text[start..end];
    let mut out = [0i32; 8];
    for (i, part) in inner.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).enumerate() {
        out[i] = part
            .parse()
            .unwrap_or_else(|_| panic!("bad int '{}' in '{}' array", part, key));
    }
    out
}

/// Extract `<key>: <int>,` as a plain `i32` scalar.
fn extract_scalar(text: &str, key: &str) -> i32 {
    let needle = format!("\n{}: ", key);
    let start = text
        .find(&needle)
        .unwrap_or_else(|| panic!("could not find scalar field '{}' in tuned weights file", key))
        + needle.len();
    let end = text[start..].find(',').expect("unterminated scalar field") + start;
    text[start..end]
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("bad scalar value for '{}'", key))
}
