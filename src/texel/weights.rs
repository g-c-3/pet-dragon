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
// ============================================================================

use crate::eval::material::s;

pub const MAX_KING_DANGER: i32 = 2400;

#[derive(Debug, Clone)]
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

    // ── King safety (flat, non-tapered) ─────────────────────────────────
    pub attacker_weight: [i32; 8],
    pub open_file_near_king: i32,
    pub semi_open_file_near_king: i32,
    pub pawn_shield_bonus: i32,

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

    // ── Tempo ─────────────────────────────────────────────────────────────
    pub tempo: i32,
}

impl Default for TunableWeights {
    fn default() -> Self {
        TunableWeights {
            // eval/material.rs: MG_VALUES / EG_VALUES / BISHOP_PAIR_MG/EG
            material_values: [
                s(82, 94),    // Pawn
                s(337, 281),  // Knight
                s(365, 297),  // Bishop
                s(477, 512),  // Rook
                s(1025, 936), // Queen
            ],
            bishop_pair: s(22, 30),

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
                s(-62, -81), s(-53, -56), s(-12, -31), s(-4, -16),
                s(3, 5), s(13, 11), s(22, 17), s(28, 20),
                s(33, 25),
            ],
            bishop_mobility: [
                s(-48, -59), s(-20, -23), s(16, -3), s(26, 13),
                s(38, 24), s(51, 42), s(55, 54), s(63, 57),
                s(63, 65), s(68, 73), s(81, 78), s(81, 86),
                s(91, 88), s(98, 97),
            ],
            rook_mobility: [
                s(-58, -76), s(-27, -18), s(-15, 28), s(-10, 55),
                s(-5, 69), s(-2, 82), s(9, 87), s(16, 94),
                s(20, 102), s(25, 102), s(32, 106), s(38, 109),
                s(46, 111), s(48, 114), s(58, 114),
            ],
            queen_mobility: [
                s(-39, -36), s(-21, -15), s(3, 8), s(3, 18),
                s(14, 34), s(22, 54), s(28, 61), s(41, 73),
                s(43, 79), s(48, 92), s(56, 94), s(60, 104),
                s(60, 113), s(66, 120), s(67, 123), s(70, 126),
                s(71, 133), s(73, 136), s(79, 140), s(80, 143),
                s(86, 148), s(93, 166), s(97, 170), s(99, 175),
                s(102, 184), s(100, 191), s(106, 206), s(109, 212),
            ],

            // eval/pawns.rs
            isolated_penalty: s(-5, -15),
            doubled_penalty: s(-11, -51),
            backward_penalty: s(-9, -8),
            passed_pawn_bonus: [
                s(0, 0),
                s(2, 10),
                s(7, 17),
                s(15, 35),
                s(35, 65),
                s(65, 105),
                s(110, 175),
                s(0, 0),
            ],

            // eval/king_safety.rs
            attacker_weight: [0, 0, 50, 75, 88, 94, 97, 99],
            open_file_near_king: -20,
            semi_open_file_near_king: -10,
            pawn_shield_bonus: 12,

            // eval/open_lines.rs
            rook_open_file: s(48, 21),
            rook_semi_open_file: s(23, 11),
            rook_on_seventh: s(17, 54),
            rooks_connected: s(11, 13),
            queen_open_file: s(3, 6),
            queen_semi_open_file: s(2, 4),
            battery_rook_queen: s(18, 10),
            battery_bishop_queen: s(14, 8),
            contested_file: s(-8, -4),

            // eval/mod.rs
            tempo: 10,
        }
    }
}

// ── PST tables, copied verbatim from eval/tables.rs ─────────────────────────

#[rustfmt::skip]
const PAWN_TABLE: [i64; 64] = [
    s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0),
    s( 98,178), s(134,173), s( 61,158), s( 95,134), s( 67,147), s(126,132), s( 34,165), s(-11,187),
    s( -6, 94), s(  7,100), s( 26, 85), s( 31, 67), s( 65, 56), s( 56, 53), s( 25, 82), s(-20, 87),
    s(-14, 32), s( 13, 24), s(  6, 13), s( 21,  5), s( 23, -2), s( 12,  4), s( 17, 17), s(-23, 17),
    s(-27,  3), s( -2,  3), s( -5, -4), s( 12,-19), s( 17,-18), s(  6,-11), s( 10,  8), s(-25,  8),
    s(-26,  0), s( -4, -2), s( -4, -1), s(-10, 4),  s(  3,  7), s(  3, -6), s( 33, -9), s(-12,-14),
    s(-35,  0), s( -1, -1), s(-20,  0), s(-23, -2), s(-15, 14), s( 24, -1), s( 38,-10), s(-22,-20),
    s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0),
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i64; 64] = [
    s(-167,-58), s(-89,-38), s(-34,-13), s(-49,-28), s( 61,-31), s(-97,-27), s(-15,-63), s(-107,-99),
    s( -73,-25), s(-41, -8), s( 72,-25), s( 36,  6), s( 23,  6), s( 62,-17), s(  7,-24), s( -17,-52),
    s( -47,-24), s( 60,-20), s( 37, 10), s( 65, 18), s( 84, 18), s(129,  8), s( 73, -4), s(  44,-17),
    s(  -9,-10), s( 17,  6), s( 19, 20), s( 53, 34), s( 37, 34), s( 69, 20), s( 18,  6), s(  22, -6),
    s( -13,-10), s(  4,  6), s( 16, 20), s( 13, 34), s( 28, 34), s( 19, 20), s( 21,  6), s(  -8,-10),
    s( -23,-20), s( -9, -8), s( 12, -4), s( 10, 18), s( 22, 18), s( 15, -4), s( 36, -8), s( -21,-20),
    s( -29,-60), s(-53,-20), s(-12,-20), s( -3, -8), s( -1, -8), s( 18,-20), s(-14,-20), s( -19,-60),
    s(-105,-40), s(-21,-60), s(-58,-20), s(-33,-20), s(-17,-20), s(-28,-20), s(-19,-60), s( -23,-40),
];

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s(-29,-14), s(  4,-21), s(-82,-11), s(-37, -8), s(-25, -7), s(-42, -9), s(  7,-17), s( -8,-24),
    s(-26, -8), s( 16,  6), s(-18,  1), s(-13, -7), s( 30, -3), s( 59, -9), s( 18, -4), s(-47, -21),
    s(-16,  2), s( 37,  0), s( 43,  2), s( 40, -2), s( 35,  6), s( 50,  0), s( 37, -2), s( -2,  4),
    s( -4, -6), s(  5,  0), s( 19,  4), s( 50, -2), s( 37,  4), s( 37, -4), s(  7,  0), s( -2, -6),
    s( -6, -4), s( 13,  0), s( 13,  4), s( 26,  4), s( 34,  0), s(  0,  4), s(  2,  0), s( -6, -6),
    s(  0, -4), s( 15,  0), s( 15,  0), s( 15,  2), s( 14,  4), s( 27,  4), s( 18,  0), s(  4, -8),
    s(  4,-13), s( 15, -6), s(  6, -5), s(  7, -5), s( 10, -5), s( 18, -8), s( 22,-11), s(  1,-13),
    s(-33,-14), s( -3,-21), s( -14,-11),s(-21,-8),  s(-13,-7),  s(-12,-9),  s(-39,-17), s(-21,-24),
];

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s( 32, 13), s( 42, 10), s( 32, 18), s( 51, 15), s( 63, 12), s(  9, 12), s( 31,  8), s( 43,  5),
    s( 27, 11), s( 32, 13), s( 58, 13), s( 62, 11), s( 80,  3), s( 67,  3), s( 26,  8), s( 44,  3),
    s( -5,  7), s( 19,  7), s( 26,  7), s( 36,  5), s( 17,  5), s( 45, -3), s( 61, -5), s( 16, -3),
    s(-24,  4), s(-11,  3), s(  7,  5), s( 26,  4), s( 24,  3), s( 35, -2), s(  3, -3), s( -3, -1),
    s(-27,  3), s(-27,  3), s( -4,  3), s(  3,  5), s( 13,  2), s( -2, -3), s(-10, -2), s(-27,  0),
    s(-30,  0), s( -6,  0), s( -1,  1), s(  9,  3), s(  8,  3), s(  6, -3), s(  2, -4), s(-20, -6),
    s(-33, -3), s(-29,  0), s(-13,  0), s(-11,  1), s( -3,  3), s( -1,  3), s( -5,  0), s(-30, -3),
    s(-53, -2), s(-38, -4), s(-31, -2), s(-26, -1), s(-29,  1), s(-44,  3), s(-10, -4), s(-44, -7),
];

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s(-28, -9), s(  0, 22), s( 29, 22), s( 12, 27), s( 59, 27), s( 44, 19), s( 43, 10), s( 45, 20),
    s(-24,-17), s(-39,  3), s( -5, -3), s(  1, 14), s(-16, 22), s( 57, 22), s( 28, 22), s( 54,  5),
    s(-13,-20), s(-17,  3), s(  7,  3), s(  8,  5), s( 29, 11), s( 56, 16), s( 47, 12), s( 57,  4),
    s(-27,  0), s(-27,  4), s(-16,  5), s(-16,  5), s( -1, 13), s( 17, 16), s( -2, 18), s(  1,  9),
    s( -9, -4), s(-26,  4), s( -9,  5), s(-10,  5), s( -2,  5), s( -4,  8), s(  3,  8), s(  9, -1),
    s(-14, -5), s(  2, -8), s(-11,  3), s( -2,  3), s( -5,  3), s(  2,  6), s( 14,  2), s(  5,  3),
    s(-35, -8), s( -8,-15), s( 11,-14), s(  2, -8), s(  8, -8), s( 15,-14), s( -3,-13), s(  1,-17),
    s( -1,-20), s(-18,-17), s( -9,-12), s( 10,-15), s(-15,-11), s(-25,-20), s(-31,-12), s(-50,-14),
];

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s(-65,-50), s( 23,-30), s( 16,-30), s(-15,-50), s(-56,-50), s(-34,-30), s(  2,-30), s( 13,-50),
    s( 29,-30), s( -1,-10), s(-20,-10), s(-63,-30), s(-22,-30), s(-33,-10), s( -1,-10), s( 28,-30),
    s( -9,-10), s( 24,  0), s(  2,  0), s(-16,-10), s(-20,-10), s(  6,  0), s( 22,  0), s(-22,-10),
    s(-17,-20), s(-20,-10), s(-12, -5), s(-27, -5), s(-30, -5), s(-25, -5), s(-14,-10), s(-36,-20),
    s(-49,-30), s(-1,-20),  s(-27,-10), s(-39,-10), s(-46,-10), s(-44,-10), s(-33,-20), s(-51,-30),
    s(-14,-30), s(-14,-20), s(-22,-10), s(-46,-10), s(-44,-10), s(-30,-10), s(-15,-20), s(-27,-30),
    s(  1,-10), s(  7,  0), s( -8,  0), s(-64,-10), s(-43,-10), s(-16,  0), s(  9,  0), s(  8,-10),
    s(-15,-50), s( 36,-30), s( 12,-30), s(-54,-50), s(  8,-30), s(-28,-30), s( 24,-30), s( 14,-50),
];
