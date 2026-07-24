// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/weights.rs — Tunable weight vector for Texel tuning (D35 step 2)
//
// `TunableWeights` mirrors `TexelFeatures`' shape field-for-field.
// `TunableWeights::default()` is initialised FROM the current compile-time
// consts in eval/material.rs, eval/tables.rs, eval/mobility.rs, eval/pawns.rs,
// eval/king_safety.rs, eval/open_lines.rs — copied verbatim, not re-derived
// — so that `predict(extract_features(pos), &TunableWeights::default())`
// reproduces `crate::eval::evaluate(pos)` exactly. That equivalence is what
// the self-consistency test in predict.rs actually checks; this file only
// needs to get the numbers right.
//
// Packed tapered terms use the same `s(mg, eg) -> i64` representation
// `crate::eval::material` already defines (high 32 bits = MG, low 32 = EG)
// so `taper()` can be reused unmodified. King safety's flat (non-tapered)
// constants stay plain i32, matching their originals.
//
// `MAX_KING_DANGER` is NOT a `TunableWeights` field — per D35 it's the one
// genuine nonlinearity (a clamp, not a linear weight), kept as a structural
// constant exactly like the original `king_safety.rs`.
//
// Re-tuned in Phase 25 (Session 84, D66) against 62,125 fresh self-play
// positions (weight_decay=0.03, 75 epochs — see SESSION_LOG), superseding
// the Phase 14 values throughout this file. Two exceptions, deliberately
// NOT updated to their tuned values — kept at Phase 24 hand-picked
// defaults instead, mirroring eval/king_safety.rs exactly: see
// `knight_near_own_king`/`bishop_near_own_king` below and D66/D63 item 3
// in DECISIONS.md for why.
// ============================================================================

use crate::eval::material::s;

pub const MAX_KING_DANGER: i32 = 2400;

#[derive(Debug, Clone, PartialEq)]
pub struct TunableWeights {
    // ── Material ─────────────────────────────────────────────────────────
    /// s(mg, eg) per kind, order: Pawn, Knight, Bishop, Rook, Queen.
    pub material_values: [i64; 5],
    pub bishop_pair: i64,

    // ── Piece-square tables ─────────────────────────────────────────────
    /// [piece_kind][table_index], kind order: Pawn, Knight, Bishop, Rook,
    /// Queen, King (matches `PieceKind as usize`).
    pub pst: [[i64; 64]; 6],

    // ── Mobility ─────────────────────────────────────────────────────────
    pub knight_mobility: [i64; 9],
    pub bishop_mobility: [i64; 14],
    pub rook_mobility: [i64; 15],
    pub queen_mobility: [i64; 28],

    // ── Pawn structure ───────────────────────────────────────────────────
    pub isolated_penalty: i64,
    pub doubled_penalty: i64,
    pub backward_penalty: i64,
    pub passed_pawn_bonus: [i64; 8],
    /// D63 item 1 — plain (unpacked) EG-only per-(square×advancement)
    /// weights, matching `eval::pawns::ENEMY_KING_DIST_EG`/
    /// `OWN_KING_DIST_EG`. Not packed via `s()` since the mg component is
    /// always exactly 0 for this term (see `predict.rs`'s use site).
    pub enemy_king_dist_eg: i32,
    pub own_king_dist_eg: i32,

    // ── King safety (flat, non-tapered) ─────────────────────────────────
    pub attacker_weight: [i32; 8],
    pub open_file_near_king: i32,
    pub semi_open_file_near_king: i32,
    pub pawn_shield_bonus: i32,
    /// D63 item 2 — matches `eval::king_safety::PAWN_STORM_BONUS` exactly.
    pub pawn_storm_bonus: [i32; 8],
    /// D63 item 3 (design option A) — matches
    /// `eval::king_safety::KNIGHT_NEAR_OWN_KING_BONUS`/
    /// `BISHOP_NEAR_OWN_KING_BONUS`.
    pub knight_near_own_king: i32,
    pub bishop_near_own_king: i32,

    // ── Open lines ────────────────────────────────────────────────────────
    pub rook_open_file: i64,
    pub rook_semi_open_file: i64,
    pub rook_on_seventh: i64,
    pub rooks_connected: i64,
    pub queen_open_file: i64,
    pub queen_semi_open_file: i64,
    pub battery_rook_queen: i64,
    pub battery_bishop_queen: i64,
    pub contested_file: i64,

    // ── Threats (Phase 24 item 4, D68) ──────────────────────────────────
    pub undefended_knight: i64,
    pub undefended_bishop: i64,
    pub undefended_rook: i64,
    pub undefended_queen: i64,
    pub threat_by_minor: i64,

    // ── Tempo ─────────────────────────────────────────────────────────────
    pub tempo: i32,
}

impl Default for TunableWeights {
    fn default() -> Self {
        TunableWeights {
            // eval/material.rs (Phase 25 Texel-tuned, Session 84, D66 —
            // 62,125 samples, weight_decay=0.03, 75 epochs — see SESSION_LOG)
            material_values: [
                s(97, 118),   // Pawn
                s(304, 286),  // Knight
                s(350, 294),  // Bishop
                s(474, 540),  // Rook
                s(1037, 930), // Queen
            ],
            bishop_pair: s(2, 15),

            // eval/tables.rs: PAWN_TABLE, KNIGHT_TABLE, BISHOP_TABLE,
            // ROOK_TABLE, QUEEN_TABLE, KING_TABLE — copied verbatim.
            pst: [
                PAWN_TABLE,
                KNIGHT_TABLE,
                BISHOP_TABLE,
                ROOK_TABLE,
                QUEEN_TABLE,
                KING_TABLE,
            ],

            // eval/mobility.rs
            knight_mobility: [
                s(-72, -94), s(-30, -56), s(-19, -13), s(-1, -4),
                s(8, 10), s(2, 14), s(17, 12), s(9, 11),
                s(10, 21),
            ],
            bishop_mobility: [
                s(-29, -39), s(1, -21), s(20, 3), s(30, 33),
                s(37, 33), s(49, 50), s(44, 49), s(64, 52),
                s(59, 55), s(53, 58), s(58, 67), s(65, 80),
                s(70, 91), s(79, 91),
            ],
            rook_mobility: [
                s(-63, -78), s(-14, -14), s(-17, 35), s(8, 64),
                s(-17, 84), s(-8, 80), s(9, 95), s(19, 101),
                s(11, 99), s(8, 100), s(30, 117), s(44, 100),
                s(45, 121), s(44, 111), s(55, 109),
            ],
            queen_mobility: [
                s(-64, -48), s(-23, -32), s(14, 6), s(6, 33),
                s(0, 23), s(30, 47), s(34, 57), s(49, 85),
                s(44, 80), s(39, 79), s(69, 101), s(63, 99),
                s(56, 127), s(62, 114), s(71, 123), s(59, 112),
                s(86, 152), s(63, 139), s(78, 138), s(79, 159),
                s(74, 148), s(90, 151), s(105, 176), s(87, 164),
                s(94, 174), s(116, 202), s(97, 194), s(112, 217),
            ],

            // eval/pawns.rs
            isolated_penalty: s(-17, -17),
            doubled_penalty: s(-27, -44),
            backward_penalty: s(-15, -22),
            passed_pawn_bonus: [
                s(25, 21),
                s(22, 2),
                s(-12, 38),
                s(41, 57),
                s(27, 77),
                s(71, 83),
                s(75, 135),
                s(0, 0),
            ],
            // D63 item 1 — copied verbatim from eval/pawns.rs's
            // ENEMY_KING_DIST_EG / OWN_KING_DIST_EG. NOT updated to Phase
            // 25's tuned result (3, a 3x jump) — that value broke
            // test_passed_pawn_bonus in eval/pawns.rs (CI-confirmed).
            // Kept at Phase 24 hand-picked defaults — see DECISIONS.md D70.
            enemy_king_dist_eg: 1,
            own_king_dist_eg: 1,

            // eval/king_safety.rs
            attacker_weight: [0, -16, 28, 86, 103, 96, 97, 99],
            open_file_near_king: -12,
            semi_open_file_near_king: -22,
            pawn_shield_bonus: 23,
            // D63 item 2 — copied verbatim from eval/king_safety.rs's
            // PAWN_STORM_BONUS. Texel-tuned for the first time in Phase 25;
            // non-monotonic curve — see DECISIONS.md D66 watch-item note.
            pawn_storm_bonus: [14, 10, 46, 13, 14, 6, 1, -5],
            // D63 item 3 (design option A) — NOT updated to Phase 25's tuned
            // result (knight -1, bishop -3) because that result contradicts
            // two validated eval/king_safety.rs tests (shelter should help, not
            // hurt). Kept at Phase 24 hand-picked defaults, mirroring
            // eval/king_safety.rs::KNIGHT_NEAR_OWN_KING_BONUS/
            // BISHOP_NEAR_OWN_KING_BONUS exactly — see DECISIONS.md D66.
            knight_near_own_king: 8,
            bishop_near_own_king: 6,

            // eval/open_lines.rs
            rook_open_file: s(41, 9),
            rook_semi_open_file: s(14, 4),
            rook_on_seventh: s(-14, 39),
            rooks_connected: s(11, 14),
            queen_open_file: s(5, 13),
            queen_semi_open_file: s(14, 21),
            battery_rook_queen: s(33, 12),
            battery_bishop_queen: s(33, 19),
            contested_file: s(-12, -7),

            // eval/threats.rs (Phase 24 item 4, D68) — hand-picked starting
            // values, not yet Texel-tuned (same status Phase 8's original
            // Ethereal-derived HCE terms had before Phase 14's tuning pass).
            undefended_knight: s(-25, -15),
            undefended_bishop: s(-25, -15),
            undefended_rook: s(-40, -25),
            undefended_queen: s(-80, -50),
            threat_by_minor: s(15, 10),

            // eval/mod.rs
            tempo: 24,
        }
    }
}

// ── PST tables, copied verbatim from eval/tables.rs ─────────────────────────

#[rustfmt::skip]
const PAWN_TABLE: [i64; 64] = [
    s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0), s(   0,   0),
    s(  65, 140), s( 121, 145), s(  28, 123), s(  80, 103), s(  44, 113), s( 106, 100), s(   8, 134), s(  -2, 161),
    s( -27,  99), s( -15,  79), s(   0,  51), s(  57,  67), s(  46,  44), s(  47,  34), s(  10,  61), s(   3,  74),
    s( -20,   9), s(  -3,  23), s( -12,   7), s(  16,  19), s(  11,   5), s(  16,  -9), s(  -4,  21), s(   5,  16),
    s( -28,  -7), s(  12,   3), s( -32,   4), s(  22,  -8), s(   1, -25), s(  18,  -5), s(  22,  -1), s(  -7,  19),
    s( -29,  11), s(   6, -16), s(   7,  -9), s( -18,   7), s( -17,  -6), s( -13, -20), s(  56,   3), s( -30,   1),
    s( -21,  11), s(   2,   9), s(  -4,   1), s( -11,   8), s( -10,   2), s(  -6,   6), s(  56,   8), s( -23,   0),
    s(  -1,  32), s(  22,   9), s(  -6,   4), s(  11,  -3), s(   0,   0), s(   5,  20), s(   5,  18), s(  11,  29),
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i64; 64] = [
    s(-180, -44), s( -69, -20), s( -59, -32), s( -61, -25), s(  60, -39), s(-102,  -7), s(  -5, -52), s( -84, -92),
    s( -48,  -6), s( -27,  -4), s(  56, -11), s(  14,  13), s( -13, -10), s(  84,   1), s(   2, -20), s( -33, -62),
    s( -66, -12), s(  56,   0), s(  37,  13), s(  46,   6), s(  77,  16), s( 143,   1), s(  65,  -6), s(  33, -12),
    s(  -6,  18), s(  -1,   2), s(  10,  35), s(  44,  24), s(  28,  28), s(  56,   5), s(  35,  21), s(  26,  18),
    s( -35,  -5), s( -14,  -3), s(   9,  34), s(  15,  40), s(  11,  19), s(  28,  20), s(  12, -13), s( -27, -23),
    s( -13, -23), s( -13,  -3), s(   0,   2), s(   7,   6), s(  12,   8), s(  22, -15), s(  34, -14), s( -40, -44),
    s( -46, -76), s( -51, -20), s( -16, -22), s(  13,   3), s(  17,   5), s(   5, -11), s(  -5, -20), s( -42, -68),
    s(-113, -50), s( -26, -47), s( -48,  -1), s( -19, -27), s( -16, -21), s( -17,  -9), s( -12, -38), s( -15, -34),
];

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s( -65, -41), s(  -8, -40), s( -71,  14), s( -17,  12), s( -38,  -5), s( -74, -10), s(   4, -17), s( -45, -55),
    s( -13, -23), s( -10,   3), s( -13,  19), s(   3,  12), s(  17,  -5), s(  46, -14), s(  15, -17), s( -40, -40),
    s(   1,  23), s(  31,  -6), s(  28,  19), s(  41,  -6), s(  13,   2), s(  20, -13), s(  42, -11), s(   5,   8),
    s(  19,  18), s(  27,  -7), s(  -9,  -3), s(  43,  -3), s(  12, -24), s(  18,   4), s(  19, -16), s(  18,  10),
    s( -15, -21), s(   2,   6), s(  17, -12), s(   7,   0), s(   5,  -9), s(   6,  -4), s(   8,  23), s( -16, -29),
    s(  -2, -16), s(  21,   5), s(  27,  16), s(  22,   7), s(  19, -15), s(  13,   1), s(   8,  -3), s(  12,  19),
    s(   4, -20), s(  10,  21), s(  -4, -15), s(   1, -10), s(  26,  -9), s(  33,  13), s(  31,   0), s( -25, -13),
    s( -22, -34), s(   7, -13), s(   2,  -5), s(  -6, -19), s(   8,  10), s( -33, -21), s( -13,  -8), s( -15, -12),
];

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s(  35,  14), s(   7, -13), s(   9,   6), s(  34,   3), s(  84,  20), s(  14,  25), s(  17,   6), s(  46,  27),
    s(  17,  15), s(  20,   5), s(  36,   5), s(  31,  -7), s(  65,  -8), s(  36, -24), s(  15,   9), s(  55,  17),
    s(   8,  31), s(   5,   7), s(  11,  -3), s(  29,  -2), s(  26,   5), s(  63,  12), s(  48,   7), s(  38,  25),
    s( -14,  19), s( -10,  15), s( -17, -18), s(  -1, -11), s(   4, -19), s(  31,  -2), s( -17,   8), s(  11,  25),
    s( -29,  15), s( -59, -26), s(  -8,  17), s( -17,  -6), s(  28,   2), s(  -8,   0), s( -21,   6), s(  -6,  29),
    s( -21,  16), s(  -4,  -4), s(   8,  -7), s( -20, -20), s(  32,  29), s(  -1,   6), s(   6, -13), s(  11,  12),
    s( -33,   9), s( -38,  -7), s( -18,  -9), s( -19, -17), s(  18,   4), s( -19, -12), s( -18, -16), s( -14, -10),
    s( -49,   7), s( -45,  10), s( -20,   9), s( -21,   6), s(  -8, -13), s( -46,  10), s( -12,   0), s( -32,  14),
];

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s( -34,  -9), s( -17,  -2), s(  16,   5), s(  27,  34), s(  71,  34), s(  54,  36), s(  64,  26), s(  27,  14),
    s( -13,  -7), s( -40,   1), s(  -8,  -6), s(   8,   6), s(   4,  38), s(  55,  19), s(  48,  38), s(  72,  16),
    s( -20,  -8), s( -16,   7), s(  18,  13), s(  19,  25), s(  16,   8), s(  67,  28), s(  71,  34), s(  36,  -4),
    s( -10,  25), s( -13,  31), s( -34, -15), s(  -6,   0), s(  -8,  18), s(  28,   9), s( -21,   2), s(  -2,  10),
    s(   4,  10), s( -30,   2), s(   2,  18), s( -24, -15), s( -17,   1), s( -22,  -3), s(  28,  37), s(  -1, -15),
    s(   2,  12), s(  -2,  -7), s( -19,  11), s(  -7,  -6), s(  -6,  -7), s(  17,  17), s(  17,   6), s(  -6, -15),
    s( -43, -25), s( -11,  -5), s(  11,  -8), s( -21, -35), s(  21,  -8), s(  33, -30), s(   6,  -6), s(   9,   3),
    s( -20, -21), s( -31, -10), s( -24, -41), s(  -1, -23), s( -40, -31), s( -14, -12), s( -41, -28), s( -63,  -4),
];

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s( -57, -32), s(  33, -19), s(  23, -16), s( -29, -70), s( -59, -65), s( -36, -27), s( -10, -46), s(  18, -41),
    s(  14, -41), s( -14,  -7), s( -24,  -9), s( -65, -40), s( -29, -26), s( -51, -38), s(   4,   2), s(  15, -48),
    s( -31, -16), s(  40,   7), s(   4, -18), s( -11, -11), s( -29,  -6), s( -11, -16), s(  27, -10), s( -38, -23),
    s( -32, -22), s( -30, -11), s(  -6,   5), s( -37,  13), s( -51,   2), s( -46, -10), s( -37,  -6), s( -49,  -5),
    s( -38, -35), s( -29, -20), s( -29,   2), s( -49,   7), s( -55,   6), s( -71,   4), s( -48, -20), s( -41,  -4),
    s( -26, -35), s(  -3,  -5), s( -40, -12), s( -48,  -3), s( -28,  -1), s( -24,  -3), s( -41, -14), s( -37, -12),
    s( -21,   1), s( -18, -17), s(   7, -17), s( -50, -17), s( -54,   1), s( -18,  -1), s(  13, -16), s(  10,  -3),
    s( -10, -49), s(  13, -28), s( -11, -55), s( -30, -41), s(   9, -35), s( -26, -28), s(  23, -45), s(   5, -40),
];
