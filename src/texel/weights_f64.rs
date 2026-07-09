// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/weights_f64.rs — f64 weight vector for gradient descent (D35 step 5)
//
// `TunableWeights` (weights.rs, Session 53) intentionally uses the exact
// integer/packed-i64 representation `crate::eval` itself uses, so the
// self-consistency test could assert BIT-EXACT agreement with
// `evaluate()`. Gradient descent needs continuous values instead — this
// file is a parallel f64 copy of the exact same shape (same field names,
// same nesting, same order), with `S { mg, eg }` replacing packed i64 and
// plain `f64` replacing the king-safety/tempo `i32` fields.
//
// `TunableWeightsF64::from(&TunableWeights::default())` is the optimizer's
// starting point — so gradient descent always starts from "current
// evaluate() behavior" and moves from there, never from an arbitrary or
// random initialization. `to_tunable_weights()` rounds back to the integer
// representation `eval/*.rs` actually compiles with (step 6, next step
// after this).
//
// `flatten`/`unflatten` give a fixed-order flat `Vec<f64>` view of every
// parameter, used only for the Adam optimizer's per-parameter moment
// state in `src/bin/texel_tune.rs` — the struct-based gradient
// accumulation in `predict_f64.rs` stays 1:1 mirrored against `predict()`
// for correctness; only the *optimizer step itself* needs a flat vector.
// ============================================================================

use crate::eval::material::{eg, s};
use crate::texel::weights::TunableWeights;

/// A tapered (mg, eg) weight pair — the f64 analogue of the packed i64
/// `s(mg, eg)` representation `crate::eval::material` uses.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct S {
    pub mg: f64,
    pub eg: f64,
}

impl S {
    pub fn new(mg: f64, eg: f64) -> Self {
        S { mg, eg }
    }

    pub fn zero() -> Self {
        S { mg: 0.0, eg: 0.0 }
    }

    /// Linear interpolation between mg/eg by phase — the f64 analogue of
    /// `crate::eval::material::taper`, without integer truncation.
    #[inline]
    pub fn taper(self, phase: i32) -> f64 {
        (self.mg * phase as f64 + self.eg * (24 - phase) as f64) / 24.0
    }

    /// Round back to the packed i64 `s(mg, eg)` representation.
    pub fn to_packed(self) -> i64 {
        s(self.mg.round() as i32, self.eg.round() as i32)
    }
}

impl From<i64> for S {
    fn from(packed: i64) -> Self {
        // `crate::eval::material::mg()`/`taper()` are designed to decode an
        // ACCUMULATED SUM of many `s(mg, eg)` terms (predict.rs never calls
        // them on a single unsummed weight) — the addition-based packing
        // means `mg()` alone (`score >> 32`) is only correct once summed;
        // applied to one literal `s(mg, eg)` with eg < 0 it silently returns
        // mg - 1 (a borrow from the low 32 bits). `eg()` (`score as i32`,
        // a low-32-bit reinterpret) IS exact for a single term regardless.
        // So: trust eg() as-is, then recover the true mg by exact division
        // now that eg is known (packed - eg is always an exact multiple of
        // 2^32 by construction of `s()`).
        let eg_val = eg(packed);
        let mg_val = ((packed - eg_val as i64) >> 32) as i32;
        S { mg: mg_val as f64, eg: eg_val as f64 }
    }
}

/// f64 mirror of `TunableWeights` — same shape, field-for-field, in the
/// same order. Used both as the tunable weight vector itself and (via
/// `TunableWeightsF64::zero()`) as the shape for gradient accumulators
/// and Adam's per-parameter moment estimates.
#[derive(Debug, Clone)]
pub struct TunableWeightsF64 {
    pub material_values: [S; 5],
    pub bishop_pair: S,

    pub pst: [[S; 64]; 6],

    pub knight_mobility: [S; 9],
    pub bishop_mobility: [S; 14],
    pub rook_mobility: [S; 15],
    pub queen_mobility: [S; 28],

    pub isolated_penalty: S,
    pub doubled_penalty: S,
    pub backward_penalty: S,
    pub passed_pawn_bonus: [S; 8],

    pub attacker_weight: [f64; 8],
    pub open_file_near_king: f64,
    pub semi_open_file_near_king: f64,
    pub pawn_shield_bonus: f64,

    pub rook_open_file: S,
    pub rook_semi_open_file: S,
    pub rook_on_seventh: S,
    pub rooks_connected: S,
    pub queen_open_file: S,
    pub queen_semi_open_file: S,
    pub battery_rook_queen: S,
    pub battery_bishop_queen: S,
    pub contested_file: S,

    pub tempo: f64,
}

impl TunableWeightsF64 {
    /// All-zero weight vector — the correct shape for a gradient
    /// accumulator or an Adam moment-estimate buffer (never a valid
    /// starting point for the weights themselves).
    pub fn zero() -> Self {
        TunableWeightsF64 {
            material_values: [S::zero(); 5],
            bishop_pair: S::zero(),
            pst: [[S::zero(); 64]; 6],
            knight_mobility: [S::zero(); 9],
            bishop_mobility: [S::zero(); 14],
            rook_mobility: [S::zero(); 15],
            queen_mobility: [S::zero(); 28],
            isolated_penalty: S::zero(),
            doubled_penalty: S::zero(),
            backward_penalty: S::zero(),
            passed_pawn_bonus: [S::zero(); 8],
            attacker_weight: [0.0; 8],
            open_file_near_king: 0.0,
            semi_open_file_near_king: 0.0,
            pawn_shield_bonus: 0.0,
            rook_open_file: S::zero(),
            rook_semi_open_file: S::zero(),
            rook_on_seventh: S::zero(),
            rooks_connected: S::zero(),
            queen_open_file: S::zero(),
            queen_semi_open_file: S::zero(),
            battery_rook_queen: S::zero(),
            battery_bishop_queen: S::zero(),
            contested_file: S::zero(),
            tempo: 0.0,
        }
    }

    /// Round every field back to `TunableWeights`' exact integer
    /// representation (D35 step 6's input — the eval/*.rs delta gets
    /// written FROM this, next session).
    pub fn to_tunable_weights(&self) -> TunableWeights {
        TunableWeights {
            material_values: self.material_values.map(S::to_packed),
            bishop_pair: self.bishop_pair.to_packed(),
            pst: self.pst.map(|row| row.map(S::to_packed)),
            knight_mobility: self.knight_mobility.map(S::to_packed),
            bishop_mobility: self.bishop_mobility.map(S::to_packed),
            rook_mobility: self.rook_mobility.map(S::to_packed),
            queen_mobility: self.queen_mobility.map(S::to_packed),
            isolated_penalty: self.isolated_penalty.to_packed(),
            doubled_penalty: self.doubled_penalty.to_packed(),
            backward_penalty: self.backward_penalty.to_packed(),
            passed_pawn_bonus: self.passed_pawn_bonus.map(S::to_packed),
            attacker_weight: self.attacker_weight.map(|v| v.round() as i32),
            open_file_near_king: self.open_file_near_king.round() as i32,
            semi_open_file_near_king: self.semi_open_file_near_king.round() as i32,
            pawn_shield_bonus: self.pawn_shield_bonus.round() as i32,
            rook_open_file: self.rook_open_file.to_packed(),
            rook_semi_open_file: self.rook_semi_open_file.to_packed(),
            rook_on_seventh: self.rook_on_seventh.to_packed(),
            rooks_connected: self.rooks_connected.to_packed(),
            queen_open_file: self.queen_open_file.to_packed(),
            queen_semi_open_file: self.queen_semi_open_file.to_packed(),
            battery_rook_queen: self.battery_rook_queen.to_packed(),
            battery_bishop_queen: self.battery_bishop_queen.to_packed(),
            contested_file: self.contested_file.to_packed(),
            tempo: self.tempo.round() as i32,
        }
    }

    /// Total number of scalar parameters (mg and eg counted separately)
    /// — matches D35's ~970-parameter estimate; used to size the flat
    /// vector and to sanity-check `flatten`/`unflatten` round-trip.
    pub const PARAM_COUNT: usize =
        5 * 2 + 2       // material_values + bishop_pair
        + 6 * 64 * 2    // pst
        + (9 + 14 + 15 + 28) * 2 // mobility
        + 3 * 2 + 8 * 2 // isolated/doubled/backward + passed_pawn_bonus
        + 8 + 1 + 1 + 1 // king safety flat terms
        + 9 * 2         // open lines
        + 1; // tempo

    /// Flatten into a fixed-order `Vec<f64>` — order must exactly match
    /// `unflatten`'s read order (see the round-trip test below).
    pub fn flatten(&self) -> Vec<f64> {
        let mut v = Vec::with_capacity(Self::PARAM_COUNT);
        for s in &self.material_values {
            v.push(s.mg);
            v.push(s.eg);
        }
        v.push(self.bishop_pair.mg);
        v.push(self.bishop_pair.eg);
        for row in &self.pst {
            for s in row {
                v.push(s.mg);
                v.push(s.eg);
            }
        }
        for s in &self.knight_mobility {
            v.push(s.mg);
            v.push(s.eg);
        }
        for s in &self.bishop_mobility {
            v.push(s.mg);
            v.push(s.eg);
        }
        for s in &self.rook_mobility {
            v.push(s.mg);
            v.push(s.eg);
        }
        for s in &self.queen_mobility {
            v.push(s.mg);
            v.push(s.eg);
        }
        v.push(self.isolated_penalty.mg);
        v.push(self.isolated_penalty.eg);
        v.push(self.doubled_penalty.mg);
        v.push(self.doubled_penalty.eg);
        v.push(self.backward_penalty.mg);
        v.push(self.backward_penalty.eg);
        for s in &self.passed_pawn_bonus {
            v.push(s.mg);
            v.push(s.eg);
        }
        for w in &self.attacker_weight {
            v.push(*w);
        }
        v.push(self.open_file_near_king);
        v.push(self.semi_open_file_near_king);
        v.push(self.pawn_shield_bonus);
        v.push(self.rook_open_file.mg);
        v.push(self.rook_open_file.eg);
        v.push(self.rook_semi_open_file.mg);
        v.push(self.rook_semi_open_file.eg);
        v.push(self.rook_on_seventh.mg);
        v.push(self.rook_on_seventh.eg);
        v.push(self.rooks_connected.mg);
        v.push(self.rooks_connected.eg);
        v.push(self.queen_open_file.mg);
        v.push(self.queen_open_file.eg);
        v.push(self.queen_semi_open_file.mg);
        v.push(self.queen_semi_open_file.eg);
        v.push(self.battery_rook_queen.mg);
        v.push(self.battery_rook_queen.eg);
        v.push(self.battery_bishop_queen.mg);
        v.push(self.battery_bishop_queen.eg);
        v.push(self.contested_file.mg);
        v.push(self.contested_file.eg);
        v.push(self.tempo);
        debug_assert_eq!(v.len(), Self::PARAM_COUNT);
        v
    }

    /// Inverse of `flatten` — read order must exactly match `flatten`'s
    /// push order.
    pub fn unflatten(v: &[f64]) -> Self {
        debug_assert_eq!(v.len(), Self::PARAM_COUNT);
        let mut i = 0usize;
        let mut next = || {
            let x = v[i];
            i += 1;
            x
        };

        let mut material_values = [S::zero(); 5];
        for slot in material_values.iter_mut() {
            *slot = S::new(next(), next());
        }
        let bishop_pair = S::new(next(), next());

        let mut pst = [[S::zero(); 64]; 6];
        for row in pst.iter_mut() {
            for slot in row.iter_mut() {
                *slot = S::new(next(), next());
            }
        }

        let mut knight_mobility = [S::zero(); 9];
        for slot in knight_mobility.iter_mut() {
            *slot = S::new(next(), next());
        }
        let mut bishop_mobility = [S::zero(); 14];
        for slot in bishop_mobility.iter_mut() {
            *slot = S::new(next(), next());
        }
        let mut rook_mobility = [S::zero(); 15];
        for slot in rook_mobility.iter_mut() {
            *slot = S::new(next(), next());
        }
        let mut queen_mobility = [S::zero(); 28];
        for slot in queen_mobility.iter_mut() {
            *slot = S::new(next(), next());
        }

        let isolated_penalty = S::new(next(), next());
        let doubled_penalty = S::new(next(), next());
        let backward_penalty = S::new(next(), next());
        let mut passed_pawn_bonus = [S::zero(); 8];
        for slot in passed_pawn_bonus.iter_mut() {
            *slot = S::new(next(), next());
        }

        let mut attacker_weight = [0.0f64; 8];
        for slot in attacker_weight.iter_mut() {
            *slot = next();
        }
        let open_file_near_king = next();
        let semi_open_file_near_king = next();
        let pawn_shield_bonus = next();

        let rook_open_file = S::new(next(), next());
        let rook_semi_open_file = S::new(next(), next());
        let rook_on_seventh = S::new(next(), next());
        let rooks_connected = S::new(next(), next());
        let queen_open_file = S::new(next(), next());
        let queen_semi_open_file = S::new(next(), next());
        let battery_rook_queen = S::new(next(), next());
        let battery_bishop_queen = S::new(next(), next());
        let contested_file = S::new(next(), next());

        let tempo = next();

        debug_assert_eq!(i, Self::PARAM_COUNT);

        TunableWeightsF64 {
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
}

impl From<&TunableWeights> for TunableWeightsF64 {
    fn from(w: &TunableWeights) -> Self {
        TunableWeightsF64 {
            material_values: w.material_values.map(S::from),
            bishop_pair: S::from(w.bishop_pair),
            pst: w.pst.map(|row| row.map(S::from)),
            knight_mobility: w.knight_mobility.map(S::from),
            bishop_mobility: w.bishop_mobility.map(S::from),
            rook_mobility: w.rook_mobility.map(S::from),
            queen_mobility: w.queen_mobility.map(S::from),
            isolated_penalty: S::from(w.isolated_penalty),
            doubled_penalty: S::from(w.doubled_penalty),
            backward_penalty: S::from(w.backward_penalty),
            passed_pawn_bonus: w.passed_pawn_bonus.map(S::from),
            attacker_weight: w.attacker_weight.map(|x| x as f64),
            open_file_near_king: w.open_file_near_king as f64,
            semi_open_file_near_king: w.semi_open_file_near_king as f64,
            pawn_shield_bonus: w.pawn_shield_bonus as f64,
            rook_open_file: S::from(w.rook_open_file),
            rook_semi_open_file: S::from(w.rook_semi_open_file),
            rook_on_seventh: S::from(w.rook_on_seventh),
            rooks_connected: S::from(w.rooks_connected),
            queen_open_file: S::from(w.queen_open_file),
            queen_semi_open_file: S::from(w.queen_semi_open_file),
            battery_rook_queen: S::from(w.battery_rook_queen),
            battery_bishop_queen: S::from(w.battery_bishop_queen),
            contested_file: S::from(w.contested_file),
            tempo: w.tempo as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// flatten/unflatten must round-trip exactly, and `to_tunable_weights`
    /// applied to a from-default conversion must reproduce the exact
    /// integer defaults (round-trip through f64 must not drift the
    /// starting point at all, since every source value is already a
    /// whole number).
    #[test]
    fn test_flatten_unflatten_roundtrip_and_default_conversion() {
        let default_weights = TunableWeights::default();
        let f64_weights = TunableWeightsF64::from(&default_weights);

        let flat = f64_weights.flatten();
        assert_eq!(flat.len(), TunableWeightsF64::PARAM_COUNT);
        let restored = TunableWeightsF64::unflatten(&flat);
        assert_eq!(restored.flatten(), flat);

        let back_to_int = restored.to_tunable_weights();
        assert_eq!(back_to_int.material_values, default_weights.material_values);
        assert_eq!(back_to_int.bishop_pair, default_weights.bishop_pair);
        assert_eq!(back_to_int.pst, default_weights.pst);
        assert_eq!(back_to_int.knight_mobility, default_weights.knight_mobility);
        assert_eq!(back_to_int.bishop_mobility, default_weights.bishop_mobility);
        assert_eq!(back_to_int.rook_mobility, default_weights.rook_mobility);
        assert_eq!(back_to_int.queen_mobility, default_weights.queen_mobility);
        assert_eq!(back_to_int.isolated_penalty, default_weights.isolated_penalty);
        assert_eq!(back_to_int.doubled_penalty, default_weights.doubled_penalty);
        assert_eq!(back_to_int.backward_penalty, default_weights.backward_penalty);
        assert_eq!(back_to_int.passed_pawn_bonus, default_weights.passed_pawn_bonus);
        assert_eq!(back_to_int.attacker_weight, default_weights.attacker_weight);
        assert_eq!(back_to_int.open_file_near_king, default_weights.open_file_near_king);
        assert_eq!(
            back_to_int.semi_open_file_near_king,
            default_weights.semi_open_file_near_king
        );
        assert_eq!(back_to_int.pawn_shield_bonus, default_weights.pawn_shield_bonus);
        assert_eq!(back_to_int.rook_open_file, default_weights.rook_open_file);
        assert_eq!(back_to_int.rook_semi_open_file, default_weights.rook_semi_open_file);
        assert_eq!(back_to_int.rook_on_seventh, default_weights.rook_on_seventh);
        assert_eq!(back_to_int.rooks_connected, default_weights.rooks_connected);
        assert_eq!(back_to_int.queen_open_file, default_weights.queen_open_file);
        assert_eq!(back_to_int.queen_semi_open_file, default_weights.queen_semi_open_file);
        assert_eq!(back_to_int.battery_rook_queen, default_weights.battery_rook_queen);
        assert_eq!(back_to_int.battery_bishop_queen, default_weights.battery_bishop_queen);
        assert_eq!(back_to_int.contested_file, default_weights.contested_file);
        assert_eq!(back_to_int.tempo, default_weights.tempo);
    }
}
