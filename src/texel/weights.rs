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
            // eval/material.rs (Phase 14 Texel-tuned, 147,283 samples,
            // weight_decay=0.08, 100 epochs — see SESSION_LOG D35 step 6)
            material_values: [
                s(93, 105),   // Pawn
                s(327, 278),  // Knight
                s(359, 291),  // Bishop
                s(477, 519),  // Rook
                s(1025, 932), // Queen
            ],
            bishop_pair: s(18, 29),

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
                s(-58, -75), s(-46, -47), s(-16, -24), s(-2, -8),
                s(6, 7), s(11, 8), s(18, 16), s(21, 14),
                s(25, 17),
            ],
            bishop_mobility: [
                s(-42, -50), s(-16, -26), s(22, 3), s(34, 20),
                s(43, 29), s(52, 44), s(52, 56), s(58, 55),
                s(57, 58), s(58, 65), s(74, 70), s(73, 78),
                s(90, 93), s(94, 91),
            ],
            rook_mobility: [
                s(-64, -79), s(-26, -13), s(-17, 24), s(-6, 60),
                s(-1, 69), s(3, 83), s(12, 92), s(17, 97),
                s(18, 102), s(22, 102), s(28, 103), s(34, 107),
                s(45, 112), s(48, 121), s(51, 108),
            ],
            queen_mobility: [
                s(-43, -37), s(-21, -18), s(0, 3), s(7, 18),
                s(11, 32), s(22, 50), s(31, 61), s(44, 75),
                s(42, 78), s(47, 95), s(54, 93), s(61, 104),
                s(62, 115), s(65, 118), s(67, 123), s(72, 128),
                s(73, 133), s(73, 136), s(76, 142), s(81, 144),
                s(88, 153), s(88, 159), s(94, 169), s(99, 174),
                s(108, 190), s(102, 193), s(106, 206), s(110, 214),
            ],

            // eval/pawns.rs
            isolated_penalty: s(-16, -17),
            doubled_penalty: s(-18, -47),
            backward_penalty: s(-17, -9),
            passed_pawn_bonus: [
                s(5, 9),
                s(12, 19),
                s(14, 27),
                s(25, 44),
                s(40, 68),
                s(57, 97),
                s(102, 165),
                s(0, 0),
            ],
            // D63 item 1 — copied verbatim from eval/pawns.rs's
            // ENEMY_KING_DIST_EG / OWN_KING_DIST_EG.
            enemy_king_dist_eg: 2,
            own_king_dist_eg: 2,

            // eval/king_safety.rs
            attacker_weight: [0, -5, 43, 79, 89, 94, 97, 99],
            open_file_near_king: -21,
            semi_open_file_near_king: -19,
            pawn_shield_bonus: 16,

            // eval/open_lines.rs
            rook_open_file: s(44, 14),
            rook_semi_open_file: s(21, 6),
            rook_on_seventh: s(8, 47),
            rooks_connected: s(17, 15),
            queen_open_file: s(5, 3),
            queen_semi_open_file: s(1, 1),
            battery_rook_queen: s(23, 14),
            battery_bishop_queen: s(15, 5),
            contested_file: s(-6, -7),

            // eval/mod.rs
            tempo: 20,
        }
    }
}

// ── PST tables, copied verbatim from eval/tables.rs ─────────────────────────

#[rustfmt::skip]
const PAWN_TABLE: [i64; 64] = [
    s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0), s(  0,  0),
    s( 93,169), s(128,167), s( 55,150), s( 91,129), s( 62,140), s(127,128), s( 32,159), s(-12,183),
    s( -2, 94), s(  0, 94), s( 19, 77), s( 34, 60), s( 56, 47), s( 60, 49), s( 28, 84), s(-25, 77),
    s(-12, 29), s( 10, 23), s( 13, 10), s( 30,  0), s( 26, -2), s( 20,  5), s( 20,  9), s(-16, 22),
    s(-30,  7), s( -4,  5), s( -9, -6), s( 19,-14), s(  9,-18), s(  5, -7), s( 14, 12), s(-28,  8),
    s(-31,  3), s(-10, -7), s(  1, -3), s(-14,  5), s(  5,  5), s(  0, -8), s( 38, -5), s(-19,-13),
    s(-35,  4), s(  7,  1), s(-16,  9), s(-26,  3), s( -6, 20), s( 17, -5), s( 37, -4), s(-18,-17),
    s(  2,  9), s(  9, 10), s( -4,  6), s(  4,  6), s(  0,  0), s(  5, -1), s(  8,  3), s(  1,  6),
];

#[rustfmt::skip]
const KNIGHT_TABLE: [i64; 64] = [
    s(-169,-55), s(-88,-34), s(-35, -9), s(-46,-25), s( 62,-31), s(-94,-25), s(-20,-69), s(-103,-103),
    s( -66,-22), s(-36, -4), s( 76,-19), s( 32,  5), s( 13, -2), s( 60,-13), s(  1,-28), s( -16,-45),
    s( -54,-29), s( 63,-17), s( 37,  8), s( 64, 13), s( 76, 10), s(125,  1), s( 79,  2), s(  42,-13),
    s(  -6, -7), s( 10,  0), s( 16, 20), s( 48, 33), s( 32, 32), s( 70, 15), s( 18,  8), s(  21, -5),
    s( -14, -3), s(  5,  6), s( 15, 15), s(  7, 29), s( 26, 30), s( 17, 20), s( 21,  4), s(  -9, -4),
    s( -25,-23), s(-12,-16), s(  8,  0), s( 13, 19), s( 27, 20), s( 13, -6), s( 28,-14), s( -27,-22),
    s( -31,-56), s(-50,-12), s(-12,-15), s( -2,-11), s(  5,  0), s( 14,-19), s(-12,-16), s( -23,-56),
    s(-107,-36), s(-25,-57), s(-55,-21), s(-33,-21), s(-11,-16), s(-34,-21), s(-21,-56), s( -29,-38),
];

#[rustfmt::skip]
const BISHOP_TABLE: [i64; 64] = [
    s(-37,-16), s(  1,-16), s(-83, -8), s(-39, -8), s(-28,-11), s(-50,-15), s(  1,-21), s(-18,-34),
    s(-21, -4), s( 11,  8), s(-22, -4), s(-16,-12), s( 23, -8), s( 54,-12), s( 13,-10), s(-49,-21),
    s(-18, -3), s( 31, -2), s( 36, -1), s( 39, -1), s( 27, -1), s( 43,  0), s( 29,-10), s(  2, 10),
    s( -7, -9), s(  6,  3), s( 20, -2), s( 45, -5), s( 28, -3), s( 33, -2), s(  5, -6), s( -1,  1),
    s( -6, -8), s(  5, -8), s(  8,  1), s( 15, -1), s( 26, -6), s(  2,  3), s( -3, -3), s( -5,-10),
    s(  5, -4), s(  9, -4), s( 11, -3), s(  9,  2), s( 15,  1), s( 23,  3), s( 14, -4), s(  6, -5),
    s(  6,-11), s( 21,  2), s(  9, -3), s(  8, -6), s( 15, -4), s( 17, -8), s( 27, -8), s( -3,-14),
    s(-32,-11), s(  2,-13), s( -9, -2), s(-21,-11), s(-15,-11), s( -8, -1), s(-34,-13), s(-14,-14),
];

#[rustfmt::skip]
const ROOK_TABLE: [i64; 64] = [
    s( 30, 12), s( 33,  2), s( 34, 23), s( 50, 17), s( 65, 17), s(  8,  8), s( 29,  3), s( 44, 10),
    s( 26, 11), s( 28, 16), s( 54, 10), s( 54,  4), s( 84,  6), s( 59, -1), s( 17,  0), s( 41,  2),
    s( -2,  9), s( 15,  3), s( 17,  0), s( 32,  4), s( 19,  7), s( 42,  0), s( 58, -7), s( 24,  3),
    s(-18, 10), s(-10,  5), s( -2, -3), s( 24,  0), s( 21,  2), s( 32, -4), s( -2, -3), s( -3,  6),
    s(-22,  8), s(-33, -3), s( -9,  1), s( -1,  4), s( 13, -2), s( -4, -5), s(-14, -5), s(-19,  8),
    s(-24,  7), s( -4,  2), s( -4,  1), s(  4, -4), s( 14,  9), s(  7, -1), s(  1, -6), s(-16, -8),
    s(-37,  3), s(-31,  0), s(-17,  1), s(-13,  1), s(  1,  4), s(  2,  5), s(  1,  5), s(-31,  2),
    s(-56, -7), s(-42, -4), s(-35, -3), s(-17,  7), s(-31, -1), s(-37,  9), s(-19,-10), s(-43,  3),
];

#[rustfmt::skip]
const QUEEN_TABLE: [i64; 64] = [
    s(-24, -8), s( -1, 21), s( 29, 21), s(  5, 20), s( 60, 29), s( 48, 22), s( 43,  9), s( 41, 21),
    s(-20,-12), s(-34,  3), s(-10, -4), s(  4, 14), s(-12, 26), s( 56, 21), s( 32, 24), s( 56,  8),
    s(-18,-24), s(-12,  4), s(  8,  1), s( 11, 11), s( 32, 18), s( 54, 17), s( 48,  8), s( 56,  7),
    s(-20,  5), s(-21, 10), s(-13,  8), s(-15,  8), s( -5, 13), s( 10, 14), s( -1, 19), s( -1,  9),
    s( -9, -4), s(-20,  8), s(-13,  1), s(-12,  4), s(  1, 11), s( -2,  8), s(  5, 12), s(  9,  0),
    s(-14,-10), s( -2, -5), s( -5,  8), s( -1,  2), s( -3,  9), s( -4, -2), s(  9, -4), s(  9,  7),
    s(-40,-12), s(-11,-16), s(  5,-19), s( -3,-11), s( 10, -7), s( 14,-13), s( -4,-13), s(  8,-13),
    s( -9,-27), s(-19,-12), s( -6,-15), s(  3,-22), s(-17,-12), s(-26,-24), s(-34,-18), s(-53, -8),
];

#[rustfmt::skip]
const KING_TABLE: [i64; 64] = [
    s(-66,-46), s( 22,-29), s( 19,-23), s(-17,-50), s(-57,-48), s(-36,-35), s(  3,-25), s( 18,-42),
    s( 25,-29), s(  1, -6), s(-20, -7), s(-60,-22), s(-25,-35), s(-35,-15), s( -3,-14), s( 29,-25),
    s(-10,-10), s( 30,  9), s(  4,  1), s(-13, -5), s(-26,-13), s(  4,  0), s( 21,  0), s(-26,-10),
    s(-17,-17), s(-20, -6), s( -7,  3), s(-27,  1), s(-31,  0), s(-28, -7), s(-16, -5), s(-40,-17),
    s(-51,-32), s( -8,-17), s(-22, -2), s(-38, -4), s(-49, -3), s(-51, -8), s(-29,-14), s(-54,-28),
    s(-18,-35), s(-14,-16), s(-28, -9), s(-48,-10), s(-43, -8), s(-37, -9), s(-17,-21), s(-34,-31),
    s( -4,-12), s(  0, -5), s(-19, -9), s(-66,-12), s(-47, -7), s(-10, -3), s(  0, -9), s(  0,-15),
    s(-20,-56), s( 34,-35), s( 18,-34), s(-53,-44), s( 14,-24), s(-32,-34), s( 21,-36), s(  8,-50),
];
