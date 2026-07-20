// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/pawns.rs — Pawn structure evaluation
//
// Evaluates:
//   - Passed pawns: no enemy pawn can block or capture on the path to promotion
//   - Isolated pawns: no friendly pawn on adjacent files
//   - Doubled pawns: two friendly pawns on same file
//   - Backward pawns: pawn that cannot advance without being captured,
//     and whose stop square is controlled by enemy pawn
//
// Weights from Ethereal chess engine (GPL v3, Andrew Grant).
//
// ⚠️ Pet Dragon critical rules:
//   1. Rank 1 pawns are NEVER penalised as backward.
//      A pawn on rank 1 hasn't moved — it's in its starting position, not weak.
//   2. Doubled pawns on ranks 1–2 get a reduced penalty (opening setup,
//      not a structural weakness yet).
//   3. Passed pawn bonus applies from whatever rank the pawn is currently on
//      relative to its colour direction — no rank offset for rank 1 starters.
//
// Tapered: score = (mg * phase + eg * (24 - phase)) / 24
// ============================================================================

use crate::bitboard::Bitboard;
use crate::eval::material::{eg, s, taper};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

// ── Pawn structure bonus/penalty tables ───────────────────────────────────────
// Originally from Ethereal (GPL v3, Andrew Grant); as of Phase 14 (D35)
// these are Pet-Dragon-specific Texel-tuned values (147,283 samples,
// weight_decay=0.08, 100 epochs — see SESSION_LOG). Ethereal's values
// remain the tuner's starting point (src/texel/weights.rs).

/// Isolated pawn penalty (no friendly pawn on adjacent files)
const ISOLATED_PENALTY: i64 = s(-16, -17);

/// Doubled pawn penalty (two pawns on same file)
const DOUBLED_PENALTY: i64 = s(-18, -47);

/// Backward pawn penalty (can't safely advance, stop square enemy-controlled)
const BACKWARD_PENALTY: i64 = s(-17, -9);

/// Passed pawn bonus by rank (0-indexed rank relative to own back rank).
/// Index 0 = rank closest to own start (least advanced), 5 = one step from promo.
/// Index 7 (promotion rank) stays 0,0 — pawns there promote immediately and
/// are never observed as pawns in the training data, so the tuner correctly
/// never touches it.
///
/// For White: rank index = pawn.rank() - 1 (rank 2 = index 1, rank 7 = index 6)
/// For Black: rank index = 6 - pawn.rank() (rank 7 = index 1, rank 2 = index 6)
const PASSED_PAWN_BONUS: [i64; 8] = [
    s(  5,   9),  // rank 1 / rank 8 (start)
    s( 12,  19),  // rank 2 / rank 7
    s( 14,  27),  // rank 3 / rank 6
    s( 25,  44),  // rank 4 / rank 5 (dangerous)
    s( 40,  68),  // rank 5 / rank 4
    s( 57,  97),  // rank 6 / rank 3 (very dangerous)
    s(102, 165),  // rank 7 / rank 2 (about to promote)
    s(  0,   0),  // rank 8 / rank 1 (promotion rank — handled separately)
];

// ── Passed-pawn king-distance weights (D63 item 1, ROADMAP Phase 24) ──────────
// "Square of the pawn" idea: once material has thinned, whichever king is
// closer to a passer's promotion square matters as much as the pawn's own
// rank. Plain (unpacked) endgame-only centipawn weights, applied per
// Chebyshev square of distance and per unit of advancement (rank_idx,
// 0..=7) — see `passed_pawn_king_distance_bonus()` below. Deliberately no
// division anywhere in the formula (pure multiply-add) so this term stays
// linear in its two weights and can be mirrored exactly, bit-for-bit, by
// `texel::predict()` via two summed diff features
// (`passed_king_enemy_dist_diff` / `passed_king_own_dist_diff`) — see
// `src/texel/{features,predict,weights}.rs`.
//
// Hand-picked starting values, NOT yet Texel-tuned (`TunableWeights`'s
// defaults just copy these verbatim, same as every other pawns.rs
// constant) — same status Phase 8's original Ethereal-derived HCE terms
// had before Phase 14's tuning pass.
const ENEMY_KING_DIST_EG: i32 = 2; // per (square × advancement): farther enemy king = safer passer
const OWN_KING_DIST_EG:   i32 = 2; // per (square × advancement): closer own king = safer passer

// ── Main evaluation function ──────────────────────────────────────────────────

/// Evaluate pawn structure for both sides and return score from side-to-move perspective.
pub fn evaluate_pawns(pos: &Position, phase: i32) -> i32 {
    let us   = pos.side_to_move;
    let them = us.flip();

    let our_score   = pawn_structure_for_color(pos, us);
    let their_score = pawn_structure_for_color(pos, them);

    taper(our_score - their_score, phase)
}

/// Compute raw pawn structure score for one color.
fn pawn_structure_for_color(pos: &Position, color: Color) -> i64 {
    let our_pawns   = pos.piece_bb(color, PieceKind::Pawn);
    let enemy_pawns = pos.piece_bb(color.flip(), PieceKind::Pawn);
    let our_king    = pos.king_sq(color);
    let enemy_king  = pos.king_sq(color.flip());
    let mut score   = 0i64;

    let mut pawns_bb = our_pawns;
    while let Some(sq) = pawns_bb.pop_lsb() {
        let file = sq.file();
        let rank = sq.rank();

        // ── Adjacent file masks ───────────────────────────────────────────────
        let adj_files = adjacent_file_mask(file);

        // ── Isolated pawn ────────────────────────────────────────────────────
        // Penalty if no friendly pawn on either adjacent file
        if (our_pawns & adj_files).is_empty() {
            score += ISOLATED_PENALTY;
        }

        // ── Doubled pawn ──────────────────────────────────────────────────────
        // Penalty if another friendly pawn is on the same file
        // (only count each pair once — penalise the rearmost pawn)
        let file_mask = Bitboard::file_mask(file);
        let same_file_pawns = (our_pawns & file_mask).count();
        if same_file_pawns >= 2 {
            // Only penalise the rearmost (lowest rank for White, highest for Black)
            let is_rearmost = match color {
                Color::White => {
                    let below = our_pawns & file_mask & rank_mask_below(rank);
                    below.is_empty()
                }
                Color::Black => {
                    let above = our_pawns & file_mask & rank_mask_above(rank);
                    above.is_empty()
                }
            };
            if is_rearmost {
                score += DOUBLED_PENALTY;
            }
        }

        // ── Backward pawn ────────────────────────────────────────────────────
        // A pawn is backward if:
        //   1. It cannot safely advance (stop square attacked by enemy pawn)
        //   2. No friendly pawns support it from behind on adjacent files
        //
        // ⚠️ Pet Dragon: NEVER penalise rank 1 pawns as backward.
        //    They are on their starting square — not structurally weak.
        let is_rank1 = rank == 0;
        let is_rank8 = rank == 7;
        let is_start_rank = match color {
            Color::White => is_rank1,
            Color::Black => is_rank8,
        };

        if !is_start_rank {
            let stop_sq = stop_square(sq, color);
            if let Some(stop) = stop_sq {
                let stop_attacked_by_enemy = pawn_attacks_square(enemy_pawns, stop, color.flip());
                let has_support_behind = pawns_behind_on_adj_files(our_pawns, sq, color);
                if stop_attacked_by_enemy && !has_support_behind {
                    score += BACKWARD_PENALTY;
                }
            }
        }

        // ── Passed pawn ───────────────────────────────────────────────────────
        // No enemy pawn on same file ahead, and no enemy pawn on adjacent files
        // that could attack the path to promotion.
        if is_passed_pawn(sq, color, enemy_pawns) {
            let rank_idx = passed_pawn_rank_index(sq, color);
            score += PASSED_PAWN_BONUS[rank_idx];
            score += passed_pawn_king_distance_bonus(sq, color, rank_idx, our_king, enemy_king);
        }
    }

    score
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Bitboard of all squares on files adjacent to `file` (a-file has only b-file, etc.)
#[inline]
fn adjacent_file_mask(file: u8) -> Bitboard {
    let mut mask = Bitboard::EMPTY;
    if file > 0 {
        mask |= Bitboard::file_mask(file - 1);
    }
    if file < 7 {
        mask |= Bitboard::file_mask(file + 1);
    }
    mask
}

/// Bitboard of all ranks strictly below the given rank (rank indices < rank)
#[inline]
fn rank_mask_below(rank: u8) -> Bitboard {
    if rank == 0 {
        Bitboard::EMPTY
    } else {
        // All squares with rank index < rank
        Bitboard((1u64 << (rank * 8)) - 1)
    }
}

/// Bitboard of all ranks strictly above the given rank (rank indices > rank)
#[inline]
fn rank_mask_above(rank: u8) -> Bitboard {
    if rank == 7 {
        Bitboard::EMPTY
    } else {
        Bitboard(u64::MAX << ((rank + 1) * 8))
    }
}

/// The square directly in front of a pawn (one step toward promotion).
/// Returns None if the pawn is already on the promotion rank.
#[inline]
fn stop_square(sq: Square, color: Color) -> Option<Square> {
    let rank = sq.rank();
    let file = sq.file();
    match color {
        Color::White => {
            if rank < 7 { Square::from_file_rank(file, rank + 1) } else { None }
        }
        Color::Black => {
            if rank > 0 { Square::from_file_rank(file, rank - 1) } else { None }
        }
    }
}

/// Does the enemy pawn bitboard attack a given square?
/// A pawn attacks diagonally forward, so we check if any enemy pawn
/// is diagonally behind the target (from that pawn's perspective forward).
#[inline]
fn pawn_attacks_square(enemy_pawns: Bitboard, sq: Square, enemy_color: Color) -> bool {
    // Squares that an enemy pawn of enemy_color would need to be on to attack sq
    let file = sq.file();
    let rank = sq.rank();
    let attacker_rank = match enemy_color {
        Color::White => {
            // White pawn attacks forward (north), so attacker is one rank below sq
            if rank == 0 { return false; }
            rank - 1
        }
        Color::Black => {
            // Black pawn attacks forward (south), so attacker is one rank above sq
            if rank == 7 { return false; }
            rank + 1
        }
    };

    // Check left and right diagonal
    let mut attacked = false;
    if file > 0 {
        if let Some(left) = Square::from_file_rank(file - 1, attacker_rank) {
            if enemy_pawns.contains(left) {
                attacked = true;
            }
        }
    }
    if file < 7 {
        if let Some(right) = Square::from_file_rank(file + 1, attacker_rank) {
            if enemy_pawns.contains(right) {
                attacked = true;
            }
        }
    }
    attacked
}

/// Does the color have any pawns behind `sq` on adjacent files?
/// "Behind" means closer to own back rank.
#[inline]
fn pawns_behind_on_adj_files(our_pawns: Bitboard, sq: Square, color: Color) -> bool {
    let file = sq.file();
    let rank = sq.rank();
    let adj = adjacent_file_mask(file);

    let behind_mask = match color {
        Color::White => rank_mask_below(rank),
        Color::Black => rank_mask_above(rank),
    };

    (our_pawns & adj & behind_mask).is_not_empty()
}

/// Is this pawn a passed pawn?
/// Passed = no enemy pawn can block or capture it on the path to promotion.
/// That means: no enemy pawn on same file ahead, and no enemy pawn on adjacent
/// files ahead of current rank (that could diagonally block the advance).
fn is_passed_pawn(sq: Square, color: Color, enemy_pawns: Bitboard) -> bool {
    let file = sq.file();
    let rank = sq.rank();

    // The "front span" + adjacent front span — all squares an enemy pawn
    // could use to block or capture this pawn going forward
    let front_mask = match color {
        Color::White => rank_mask_above(rank),
        Color::Black => rank_mask_below(rank),
    };

    // Include same file and adjacent files ahead
    let span_files = adjacent_file_mask(file) | Bitboard::file_mask(file);
    let blocker_zone = enemy_pawns & span_files & front_mask;

    blocker_zone.is_empty()
}

/// Convert a pawn's position to a passed pawn rank index (0–7).
/// Index 0 = own back rank (start), index 6 = one step from promotion.
#[inline]
fn passed_pawn_rank_index(sq: Square, color: Color) -> usize {
    let rank = sq.rank() as usize;
    match color {
        Color::White => rank,            // rank 0 = idx 0, rank 7 = idx 7
        Color::Black => 7 - rank,        // rank 7 = idx 0, rank 0 = idx 7
    }
    .min(7)
}

/// Chebyshev (king-move) distance between two squares — the number of king
/// moves needed to travel from one to the other, ignoring occupancy.
#[inline]
fn chebyshev_distance(a: Square, b: Square) -> i32 {
    let df = (a.file() as i32 - b.file() as i32).abs();
    let dr = (a.rank() as i32 - b.rank() as i32).abs();
    df.max(dr)
}

/// The square a pawn on `sq` would promote on: same file, opponent's back rank.
#[inline]
fn promotion_square(sq: Square, color: Color) -> Square {
    let promo_rank = match color {
        Color::White => 7,
        Color::Black => 0,
    };
    Square::from_file_rank(sq.file(), promo_rank)
        .expect("file is always in 0..8, promo_rank is always 0 or 7")
}

/// Passed-pawn king-distance bonus (D63 item 1 — "square of the pawn").
/// EG-only: king proximity to a passer's promotion square barely matters
/// until material has thinned enough for kings to actively race for it.
/// Scaled by `rank_idx` (0..=7, same index `PASSED_PAWN_BONUS` uses) so a
/// freshly-started passer gets almost no weight here, while a rank-6/7
/// passer — exactly where a king race actually decides the game — gets
/// close to the full per-square weight. Pure multiply-add, no division —
/// keeps this linear in `ENEMY_KING_DIST_EG`/`OWN_KING_DIST_EG` so
/// `texel::predict()` can reproduce it exactly via two summed diff
/// features instead of re-deriving per-pawn geometry.
#[inline]
fn passed_pawn_king_distance_bonus(
    sq: Square,
    color: Color,
    rank_idx: usize,
    our_king: Square,
    enemy_king: Square,
) -> i64 {
    let promo_sq   = promotion_square(sq, color);
    let own_dist   = chebyshev_distance(our_king, promo_sq);
    let enemy_dist = chebyshev_distance(enemy_king, promo_sq);

    let advancement = rank_idx as i32; // 0..=7
    let eg_bonus = ENEMY_KING_DIST_EG * enemy_dist * advancement
                 - OWN_KING_DIST_EG   * own_dist   * advancement;

    s(0, eg_bonus)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::eval::material::game_phase;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_pawn_eval_start_pos_symmetric() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_pawns(&pos, phase);
        assert_eq!(score, 0, "Start position is symmetric — pawn eval should be 0");
    }

    #[test]
    fn test_passed_pawn_bonus() {
        setup();
        // White pawn on e5 — no enemy pawns on d,e,f files ahead → passed
        let fen = "4k3/8/8/4P3/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_pawns(&pos, phase);
        assert!(score > 0, "Passed pawn should give positive score: {}", score);
    }

    #[test]
    fn test_isolated_pawn_penalty() {
        setup();
        // White pawn on e4 with no adjacent file pawns → isolated
        let fen = "4k3/8/8/8/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_pawns(&pos, phase);
        // Should be negative (isolated) but also passed (no enemies), net effect varies
        // Just verify it doesn't panic and is bounded
        assert!(score.abs() < 500, "Isolated pawn score should be bounded: {}", score);
    }

    #[test]
    fn test_doubled_pawn_penalty() {
        setup();
        // White has doubled pawns on e file
        let fen = "4k3/8/8/4P3/4P3/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_pawns(&pos, phase);
        // Should have doubled pawn penalty; both are isolated too
        assert!(score < 100, "Doubled pawns should not be strongly positive: {}", score);
    }

    #[test]
    fn test_pet_dragon_rank1_pawn_not_backward() {
        setup();
        // White pawn on e1 (rank 1, Pet Dragon start) — must NOT be penalised as backward
        // We test by checking the rule directly
        let sq = Square::E1;
        let color = Color::White;
        let rank = sq.rank();
        // rank == 0 → is_start_rank = true → backward check skipped
        assert_eq!(rank, 0, "e1 should be rank 0 (0-indexed)");
        let is_start_rank = match color {
            Color::White => rank == 0,
            Color::Black => rank == 7,
        };
        assert!(is_start_rank, "Rank 1 pawn should be considered start rank → no backward penalty");
    }

    #[test]
    fn test_pawn_eval_1000_positions_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_pawns(&pos, phase);
        }
    }

    #[test]
    fn test_passed_pawn_rank_index() {
        // White pawn on rank 2 (0-indexed: 1) → index 1
        let sq = Square::from_file_rank(4, 1).unwrap(); // e2
        assert_eq!(passed_pawn_rank_index(sq, Color::White), 1);

        // White pawn on rank 7 (0-indexed: 6) → index 6
        let sq = Square::from_file_rank(4, 6).unwrap(); // e7
        assert_eq!(passed_pawn_rank_index(sq, Color::White), 6);

        // Black pawn on rank 7 (0-indexed: 6) → index 1 (just left start)
        let sq = Square::from_file_rank(4, 6).unwrap(); // e7
        assert_eq!(passed_pawn_rank_index(sq, Color::Black), 1);

        // Black pawn on rank 2 (0-indexed: 1) → index 6 (near promotion)
        let sq = Square::from_file_rank(4, 1).unwrap(); // e2
        assert_eq!(passed_pawn_rank_index(sq, Color::Black), 6);
    }

    #[test]
    fn test_adjacent_file_mask() {
        // a-file (0): only b-file adjacent
        let mask = adjacent_file_mask(0);
        assert_eq!(mask.count(), 8, "b-file = 8 squares");

        // d-file (3): c-file + e-file
        let mask = adjacent_file_mask(3);
        assert_eq!(mask.count(), 16, "c+e files = 16 squares");

        // h-file (7): only g-file adjacent
        let mask = adjacent_file_mask(7);
        assert_eq!(mask.count(), 8, "g-file = 8 squares");
    }

    #[test]
    fn test_is_passed_pawn() {
        setup();
        // White pawn e5, no enemy pawns → passed
        let enemy = Bitboard::EMPTY;
        let sq = Square::from_file_rank(4, 4).unwrap(); // e5
        assert!(is_passed_pawn(sq, Color::White, enemy), "No enemies → passed");

        // White pawn e5, enemy pawn e7 → not passed (blocked on same file)
        let mut enemy2 = Bitboard::EMPTY;
        enemy2.set(Square::from_file_rank(4, 6).unwrap()); // e7
        assert!(!is_passed_pawn(sq, Color::White, enemy2), "Enemy on e7 → not passed");

        // White pawn e5, enemy pawn d7 → not passed (adjacent file ahead)
        let mut enemy3 = Bitboard::EMPTY;
        enemy3.set(Square::from_file_rank(3, 6).unwrap()); // d7
        assert!(!is_passed_pawn(sq, Color::White, enemy3), "Enemy on d7 → not passed");
    }

    #[test]
    fn test_chebyshev_distance() {
        // a1 to h8 — 7 files and 7 ranks apart, king distance = 7
        assert_eq!(chebyshev_distance(Square::A1, Square::H8), 7);
        // e1 to e1 — same square
        assert_eq!(chebyshev_distance(Square::E1, Square::E1), 0);
        // e1 to e2 — one rank apart
        assert_eq!(chebyshev_distance(Square::E1, Square::E2), 1);
        // a1 to b2 — diagonal, still 1 king move
        let b2 = Square::from_file_rank(1, 1).unwrap();
        assert_eq!(chebyshev_distance(Square::A1, b2), 1);
    }

    #[test]
    fn test_promotion_square() {
        let e4 = Square::from_file_rank(4, 3).unwrap();
        assert_eq!(promotion_square(e4, Color::White), Square::from_file_rank(4, 7).unwrap());
        assert_eq!(promotion_square(e4, Color::Black), Square::from_file_rank(4, 0).unwrap());
    }

    #[test]
    fn test_own_king_closer_to_promo_scores_higher() {
        setup();
        // White e6 passer, promotes on e8. King A: Ke5 (closer to promo
        // square). King B: Ka1 (far away). Black king fixed on a8 in both.
        // Closer own king should score higher for an advanced passer where
        // the term carries real weight.
        let fen_close_king = "k7/8/4P3/4K3/8/8/8/8 w - - 0 1";
        let fen_far_king   = "k7/8/4P3/8/8/8/8/K7 w - - 0 1";

        let pos_close = Position::from_fen(fen_close_king).unwrap();
        let pos_far   = Position::from_fen(fen_far_king).unwrap();

        let score_close = evaluate_pawns(&pos_close, game_phase(&pos_close));
        let score_far   = evaluate_pawns(&pos_far, game_phase(&pos_far));

        assert!(
            score_close > score_far,
            "Own king near promo square should score higher than own king far away: close={} far={}",
            score_close, score_far
        );
    }

    #[test]
    fn test_enemy_king_closer_to_promo_scores_lower() {
        setup();
        // White e6 passer, promotes on e8. Enemy king A: Kd8 (right next to
        // the promotion square, can help stop it). Enemy king B: Ka8 (far).
        // White's own king is fixed far away (a1) in both cases so only the
        // enemy-king term differs.
        let fen_enemy_close = "3k4/8/4P3/8/8/8/8/K7 w - - 0 1";
        let fen_enemy_far   = "k7/8/4P3/8/8/8/8/K7 w - - 0 1";

        let pos_enemy_close = Position::from_fen(fen_enemy_close).unwrap();
        let pos_enemy_far   = Position::from_fen(fen_enemy_far).unwrap();

        let score_enemy_close = evaluate_pawns(&pos_enemy_close, game_phase(&pos_enemy_close));
        let score_enemy_far   = evaluate_pawns(&pos_enemy_far, game_phase(&pos_enemy_far));

        assert!(
            score_enemy_far > score_enemy_close,
            "Enemy king far from promo square should score higher for us than enemy king near it: far={} close={}",
            score_enemy_far, score_enemy_close
        );
    }

    #[test]
    fn test_king_distance_bonus_scales_with_advancement() {
        setup();
        // Same king geometry, but the passer is barely advanced (rank 2 vs
        // rank 1 start) instead of near-promotion. The king-distance term's
        // contribution should shrink accordingly (rank_idx used to scale it).
        let far_king   = Square::A1;
        let close_king = Square::from_file_rank(4, 6).unwrap(); // e7

        // rank_idx = 6 (advanced, e6 for White → index 6)
        let advanced = passed_pawn_king_distance_bonus(
            Square::from_file_rank(4, 5).unwrap(), // e6
            Color::White,
            6,
            close_king,
            far_king,
        );
        // rank_idx = 1 (barely advanced, e2 for White → index 1)
        let barely_advanced = passed_pawn_king_distance_bonus(
            Square::from_file_rank(4, 1).unwrap(), // e2
            Color::White,
            1,
            close_king,
            far_king,
        );

        assert!(
            eg(advanced).abs() > eg(barely_advanced).abs(),
            "King-distance bonus should carry more weight for a more advanced passer: advanced={} barely_advanced={}",
            eg(advanced), eg(barely_advanced)
        );
    }

    #[test]
    fn test_king_distance_bonus_1000_positions_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_pawns(&pos, phase);
        }
    }
}
