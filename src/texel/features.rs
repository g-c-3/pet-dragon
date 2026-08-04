// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// texel/features.rs — Per-position feature extraction for Texel tuning (D35)
//
// `extract_features()` walks a Position exactly once and produces a
// `TexelFeatures` struct: every board-derived quantity `crate::eval`'s six
// submodules need, with the *weight* application stripped out (that's
// `predict.rs`'s job). Every extraction function here is a deliberate
// mirror of the matching `crate::eval::*` submodule's board-scanning logic
// — same loops, same index math, same conditions — so that plugging the
// current default weights back in through `predict()` reproduces
// `crate::eval::evaluate()` exactly. See the self-consistency test in
// `predict.rs` for the actual proof of that claim; nothing here should be
// trusted "by inspection" alone at this scale (per D35).
//
// Most terms reduce to a simple (us_count - them_count) diff, since the
// same constant weight multiplies both sides in the original code. The two
// exceptions, which need raw per-side components instead of a diff:
//   - PST: bonus depends on WHICH square/piece-kind combination, not just
//     a count, so `pst_diff[kind][idx]` is a per-(kind, table-index) diff.
//   - Mobility: same shape — bonus depends on which bucket, not just count.
//   - King safety: the two kings are independent, phase-scaled, and each
//     side's "danger" term is separately clamped (D35's one nonlinearity)
//     — so raw per-side components are kept instead of a diff.
//
// PlayStyle (Phase 30, ROADMAP 29.7): `style` is a NEW, separate field,
// always `None` from `extract_features()` itself — that function and its
// self-consistency test (predict.rs, proving bit-exact agreement with
// `crate::eval::evaluate()`, the UNSTYLED core) are completely untouched
// by this addition. Only `src/bin/texel_tune.rs`'s data loading calls the
// new `extract_style_features()` below, for samples whose data line
// carries a PlayStyle mode tag — see that file and predict.rs/
// predict_f64.rs's own style-bonus additions for the rest of the picture.
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks, queen_attacks};
use crate::bitboard::masks::{knight_attacks, king_attacks, pawn_attacks};
use crate::bitboard::Bitboard;
use crate::eval::material::game_phase;
use crate::eval::style::{BALANCED, KILLER, TACTICAL, POSITIONAL, ENDGAME};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

/// Full per-position feature summary — everything `predict()` needs to
/// recompute the HCE score for any `TunableWeights`, without re-walking
/// the board.
#[derive(Debug, Clone)]
pub struct TexelFeatures {
    pub phase: i32,

    // ── Material ─────────────────────────────────────────────────────────
    /// Pawn, Knight, Bishop, Rook, Queen count diffs (us - them).
    pub material_diff: [i32; 5],
    /// (us has bishop pair) - (them has bishop pair), each 0 or 1.
    pub bishop_pair_diff: i32,

    // ── Piece-square tables ─────────────────────────────────────────────
    /// [piece_kind][table_index] net occupancy (us - them) at that
    /// color-adjusted table index. King included (kind index 5) even
    /// though it has no material value — it still has a PST.
    pub pst_diff: [[i32; 64]; 6],

    // ── Mobility ─────────────────────────────────────────────────────────
    pub knight_mobility_diff: [i32; 9],
    pub bishop_mobility_diff: [i32; 14],
    pub rook_mobility_diff: [i32; 15],
    pub queen_mobility_diff: [i32; 28],

    // ── Pawn structure ───────────────────────────────────────────────────
    pub pawn_isolated_diff: i32,
    pub pawn_doubled_diff: i32,
    pub pawn_backward_diff: i32,
    pub pawn_passed_diff: [i32; 8],
    /// Sum over passed pawns of (enemy_king_dist_to_promo_sq * advancement),
    /// us minus them. Paired with `TunableWeights::enemy_king_dist_eg`.
    /// D63 item 1 — see `eval::pawns::passed_pawn_king_distance_bonus`.
    pub passed_king_enemy_dist_diff: i32,
    /// Sum over passed pawns of (own_king_dist_to_promo_sq * advancement),
    /// us minus them. Paired with `TunableWeights::own_king_dist_eg`.
    pub passed_king_own_dist_diff: i32,

    // ── King safety (raw per-side components — see module doc) ─────────
    pub king_us_attacker_count: usize,
    pub king_us_attack_units: i32,
    pub king_us_shield_pawns: i32,
    pub king_us_open_files: i32,
    pub king_us_semi_open_files: i32,
    /// D63 item 2 — count of king-adjacent files whose most-advanced enemy
    /// pawn falls in each rank-distance bucket (0..=7). Paired with
    /// `TunableWeights::pawn_storm_bonus`.
    pub king_us_storm_buckets: [i32; 8],
    /// D63 item 3 (design option A) — count of OUR knights/bishops in
    /// the same king-file-third zone as OUR OWN king. Paired with
    /// `TunableWeights::knight_near_own_king`/`bishop_near_own_king`.
    pub king_us_knights_near_king: i32,
    pub king_us_bishops_near_king: i32,
    pub king_them_attacker_count: usize,
    pub king_them_attack_units: i32,
    pub king_them_shield_pawns: i32,
    pub king_them_open_files: i32,
    pub king_them_semi_open_files: i32,
    pub king_them_storm_buckets: [i32; 8],
    pub king_them_knights_near_king: i32,
    pub king_them_bishops_near_king: i32,

    // ── Open lines ────────────────────────────────────────────────────────
    pub rook_open_diff: i32,
    pub rook_semi_diff: i32,
    pub rook_seventh_diff: i32,
    pub rooks_connected_diff: i32,
    pub battery_rook_queen_diff: i32,
    pub contested_file_diff: i32,
    pub queen_open_diff: i32,
    pub queen_semi_diff: i32,
    pub battery_bishop_queen_diff: i32,

    // ── Threats (Phase 24 item 4, D68) ──────────────────────────────────
    /// Count of our undefended pieces (attackers > defenders on that
    /// square) minus theirs, per piece kind. Pawns/King excluded — see
    /// eval::threats module doc for why.
    pub undefended_knight_diff: i32,
    pub undefended_bishop_diff: i32,
    pub undefended_rook_diff: i32,
    pub undefended_queen_diff: i32,
    /// Count of our knight/bishop attacks landing on an enemy rook or
    /// queen, minus theirs. A fork counts twice, naturally.
    pub threat_by_minor_diff: i32,
    /// Count of our rook attacks landing on an enemy queen, minus theirs
    /// (Session 133 / Phase 34.2, external review §8.1).
    pub threat_by_rook_diff: i32,

    /// PlayStyle bonus features (Phase 30, ROADMAP 29.7) — `None` unless
    /// populated separately via `extract_style_features()` below (never
    /// set by `extract_features()` itself). See this file's module doc
    /// comment.
    pub style: Option<StyleFeatures>,
}

/// Extract the full feature summary for a position, from the side-to-move's
/// perspective (matching every `crate::eval::*` submodule's own
/// `us = pos.side_to_move` / `them = us.flip()` convention).
pub fn extract_features(pos: &Position) -> TexelFeatures {
    let us = pos.side_to_move;
    let them = us.flip();

    let phase = game_phase(pos);
    let (material_diff, bishop_pair_diff) = extract_material(pos, us, them);
    let pst_diff = extract_pst(pos, us, them);
    let (knight_mobility_diff, bishop_mobility_diff, rook_mobility_diff, queen_mobility_diff) =
        extract_mobility(pos, us, them);
    let (pawn_isolated_diff, pawn_doubled_diff, pawn_backward_diff, pawn_passed_diff,
         passed_king_enemy_dist_diff, passed_king_own_dist_diff) =
        extract_pawns(pos, us, them);
    let king = extract_king_safety(pos, us, them);
    let ol = extract_open_lines(pos, us, them);
    let th = extract_threats(pos, us, them);

    TexelFeatures {
        phase,
        material_diff,
        bishop_pair_diff,
        pst_diff,
        knight_mobility_diff,
        bishop_mobility_diff,
        rook_mobility_diff,
        queen_mobility_diff,
        pawn_isolated_diff,
        pawn_doubled_diff,
        pawn_backward_diff,
        pawn_passed_diff,
        passed_king_enemy_dist_diff,
        passed_king_own_dist_diff,
        king_us_attacker_count: king.0,
        king_us_attack_units: king.1,
        king_us_shield_pawns: king.2,
        king_us_open_files: king.3,
        king_us_semi_open_files: king.4,
        king_us_storm_buckets: king.5,
        king_us_knights_near_king: king.6,
        king_us_bishops_near_king: king.7,
        king_them_attacker_count: king.8,
        king_them_attack_units: king.9,
        king_them_shield_pawns: king.10,
        king_them_open_files: king.11,
        king_them_semi_open_files: king.12,
        king_them_storm_buckets: king.13,
        king_them_knights_near_king: king.14,
        king_them_bishops_near_king: king.15,
        rook_open_diff: ol.0,
        rook_semi_diff: ol.1,
        rook_seventh_diff: ol.2,
        rooks_connected_diff: ol.3,
        battery_rook_queen_diff: ol.4,
        contested_file_diff: ol.5,
        queen_open_diff: ol.6,
        queen_semi_diff: ol.7,
        battery_bishop_queen_diff: ol.8,
        undefended_knight_diff: th.0,
        undefended_bishop_diff: th.1,
        undefended_rook_diff: th.2,
        undefended_queen_diff: th.3,
        threat_by_minor_diff: th.4,
        threat_by_rook_diff: th.5,
        style: None, // populated separately by extract_style_features(), never here
    }
}

// ── Material (mirrors eval::material::evaluate_material) ───────────────────

fn extract_material(pos: &Position, us: Color, them: Color) -> ([i32; 5], i32) {
    use PieceKind::*;
    let kinds = [Pawn, Knight, Bishop, Rook, Queen];
    let mut diff = [0i32; 5];
    for (i, &k) in kinds.iter().enumerate() {
        diff[i] = pos.count_pieces(us, k) as i32 - pos.count_pieces(them, k) as i32;
    }
    let us_pair = (pos.count_pieces(us, Bishop) >= 2) as i32;
    let them_pair = (pos.count_pieces(them, Bishop) >= 2) as i32;
    (diff, us_pair - them_pair)
}

// ── PST (mirrors eval::tables::evaluate_tables / pst_value) ────────────────

/// Same index math as `eval::tables::pst_value` — duplicated here rather
/// than imported since the original is private table-lookup plumbing, not
/// a reusable index function.
fn pst_table_index(sq: Square, color: Color) -> usize {
    match color {
        Color::White => {
            let file = sq.file() as usize;
            let rank = 7 - sq.rank() as usize;
            rank * 8 + file
        }
        Color::Black => sq.index() as usize,
    }
}

fn extract_pst(pos: &Position, us: Color, them: Color) -> [[i32; 64]; 6] {
    let mut diff = [[0i32; 64]; 6];
    for color in Color::ALL {
        let sign: i32 = if color == us { 1 } else if color == them { -1 } else { 0 };
        for kind in PieceKind::ALL {
            let mut pieces = pos.piece_bb(color, kind);
            while let Some(sq) = pieces.pop_lsb() {
                let idx = pst_table_index(sq, color);
                diff[kind as usize][idx] += sign;
            }
        }
    }
    diff
}

// ── Mobility (mirrors eval::mobility::mobility_for_color) ──────────────────

fn extract_mobility(
    pos: &Position,
    us: Color,
    them: Color,
) -> ([i32; 9], [i32; 14], [i32; 15], [i32; 28]) {
    let mut knight = [0i32; 9];
    let mut bishop = [0i32; 14];
    let mut rook = [0i32; 15];
    let mut queen = [0i32; 28];

    for (color, sign) in [(us, 1i32), (them, -1i32)] {
        let own_pieces = pos.occupied(color);
        let all_pieces = pos.all_pieces();

        let mut knights = pos.piece_bb(color, PieceKind::Knight);
        while let Some(sq) = knights.pop_lsb() {
            let m = (knight_attacks(sq) & !own_pieces).count() as usize;
            knight[m.min(8)] += sign;
        }

        let mut bishops = pos.piece_bb(color, PieceKind::Bishop);
        while let Some(sq) = bishops.pop_lsb() {
            let m = (bishop_attacks(sq, all_pieces) & !own_pieces).count() as usize;
            bishop[m.min(13)] += sign;
        }

        let mut rooks = pos.piece_bb(color, PieceKind::Rook);
        while let Some(sq) = rooks.pop_lsb() {
            let m = (rook_attacks(sq, all_pieces) & !own_pieces).count() as usize;
            rook[m.min(14)] += sign;
        }

        let mut queens = pos.piece_bb(color, PieceKind::Queen);
        while let Some(sq) = queens.pop_lsb() {
            let m = (queen_attacks(sq, all_pieces) & !own_pieces).count() as usize;
            queen[m.min(27)] += sign;
        }
    }

    (knight, bishop, rook, queen)
}

// ── Pawn structure (mirrors eval::pawns::pawn_structure_for_color) ─────────

fn extract_pawns(pos: &Position, us: Color, them: Color) -> (i32, i32, i32, [i32; 8], i32, i32) {
    let mut isolated_diff = 0i32;
    let mut doubled_diff = 0i32;
    let mut backward_diff = 0i32;
    let mut passed_diff = [0i32; 8];
    let mut king_enemy_dist_diff = 0i32;
    let mut king_own_dist_diff = 0i32;

    for (color, sign) in [(us, 1i32), (them, -1i32)] {
        let our_pawns = pos.piece_bb(color, PieceKind::Pawn);
        let enemy_pawns = pos.piece_bb(color.flip(), PieceKind::Pawn);
        let our_king = pos.king_sq(color);
        let enemy_king = pos.king_sq(color.flip());

        let mut pawns_bb = our_pawns;
        while let Some(sq) = pawns_bb.pop_lsb() {
            let file = sq.file();
            let rank = sq.rank();
            let adj_files = adjacent_file_mask(file);

            // Isolated
            if (our_pawns & adj_files).is_empty() {
                isolated_diff += sign;
            }

            // Doubled — only the rearmost pawn of a stack is penalised
            let file_mask = Bitboard::file_mask(file);
            let same_file_pawns = (our_pawns & file_mask).count();
            if same_file_pawns >= 2 {
                let is_rearmost = match color {
                    Color::White => (our_pawns & file_mask & rank_mask_below(rank)).is_empty(),
                    Color::Black => (our_pawns & file_mask & rank_mask_above(rank)).is_empty(),
                };
                if is_rearmost {
                    doubled_diff += sign;
                }
            }

            // Backward — Pet Dragon: rank-1/rank-8 start-square pawns are
            // NEVER eligible (D2/pawns.rs rule), matched exactly here.
            let is_rank1 = rank == 0;
            let is_rank8 = rank == 7;
            let is_start_rank = match color {
                Color::White => is_rank1,
                Color::Black => is_rank8,
            };
            if !is_start_rank {
                if let Some(stop) = stop_square(sq, color) {
                    let stop_attacked = pawn_attacks_square(enemy_pawns, stop, color.flip());
                    let has_support = pawns_behind_on_adj_files(our_pawns, sq, color);
                    if stop_attacked && !has_support {
                        backward_diff += sign;
                    }
                }
            }

            // Passed
            if is_passed_pawn(sq, color, enemy_pawns) {
                let idx = passed_pawn_rank_index(sq, color);
                passed_diff[idx] += sign;

                // D63 item 1: king-distance-to-promo-square features,
                // mirroring eval::pawns::passed_pawn_king_distance_bonus.
                let promo_sq = promotion_square(sq, color);
                let advancement = idx as i32;
                king_enemy_dist_diff +=
                    sign * chebyshev_distance(enemy_king, promo_sq) * advancement;
                king_own_dist_diff +=
                    sign * chebyshev_distance(our_king, promo_sq) * advancement;
            }
        }
    }

    (isolated_diff, doubled_diff, backward_diff, passed_diff,
     king_enemy_dist_diff, king_own_dist_diff)
}

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

#[inline]
fn rank_mask_below(rank: u8) -> Bitboard {
    if rank == 0 {
        Bitboard::EMPTY
    } else {
        Bitboard((1u64 << (rank * 8)) - 1)
    }
}

#[inline]
fn rank_mask_above(rank: u8) -> Bitboard {
    if rank == 7 {
        Bitboard::EMPTY
    } else {
        Bitboard(u64::MAX << ((rank + 1) * 8))
    }
}

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

#[inline]
fn pawn_attacks_square(enemy_pawns: Bitboard, sq: Square, enemy_color: Color) -> bool {
    let file = sq.file();
    let rank = sq.rank();
    let attacker_rank = match enemy_color {
        Color::White => {
            if rank == 0 { return false; }
            rank - 1
        }
        Color::Black => {
            if rank == 7 { return false; }
            rank + 1
        }
    };

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

fn is_passed_pawn(sq: Square, color: Color, enemy_pawns: Bitboard) -> bool {
    let file = sq.file();
    let rank = sq.rank();

    let front_mask = match color {
        Color::White => rank_mask_above(rank),
        Color::Black => rank_mask_below(rank),
    };

    let span_files = adjacent_file_mask(file) | Bitboard::file_mask(file);
    let blocker_zone = enemy_pawns & span_files & front_mask;

    blocker_zone.is_empty()
}

#[inline]
fn passed_pawn_rank_index(sq: Square, color: Color) -> usize {
    let rank = sq.rank() as usize;
    match color {
        Color::White => rank,
        Color::Black => 7 - rank,
    }
    .min(7)
}

/// Mirrors `eval::pawns::chebyshev_distance` exactly (duplicated here per
/// this file's existing convention — see module doc).
#[inline]
fn chebyshev_distance(a: Square, b: Square) -> i32 {
    let df = (a.file() as i32 - b.file() as i32).abs();
    let dr = (a.rank() as i32 - b.rank() as i32).abs();
    df.max(dr)
}

/// Mirrors `eval::pawns::promotion_square` exactly.
#[inline]
fn promotion_square(sq: Square, color: Color) -> Square {
    let promo_rank = match color {
        Color::White => 7,
        Color::Black => 0,
    };
    Square::from_file_rank(sq.file(), promo_rank)
        .expect("file is always in 0..8, promo_rank is always 0 or 7")
}

// ── King safety (mirrors eval::king_safety::king_safety_score) ─────────────

type KingSafetyRaw = (
    usize, i32, i32, i32, i32, [i32; 8], i32, i32,
    usize, i32, i32, i32, i32, [i32; 8], i32, i32,
);

fn extract_king_safety(pos: &Position, us: Color, them: Color) -> KingSafetyRaw {
    let (us_ac, us_au, us_sp, us_of, us_sof, us_storm, us_kn, us_bp) = king_safety_side_raw(pos, us, them);
    let (them_ac, them_au, them_sp, them_of, them_sof, them_storm, them_kn, them_bp) = king_safety_side_raw(pos, them, us);
    (
        us_ac, us_au, us_sp, us_of, us_sof, us_storm, us_kn, us_bp,
        them_ac, them_au, them_sp, them_of, them_sof, them_storm, them_kn, them_bp,
    )
}

/// Returns (attacker_count, attack_units, shield_pawns, open_files,
/// semi_open_files, storm_buckets, knights_near_own_king,
/// bishops_near_own_king) for one king — exactly the raw components
/// `king_safety_score` combines, before any weight is applied.
fn king_safety_side_raw(
    pos: &Position,
    king_color: Color,
    attacker_color: Color,
) -> (usize, i32, i32, i32, i32, [i32; 8], i32, i32) {
    let king_sq = pos.king_sq(king_color);
    let all_occ = pos.all_pieces();

    let king_zone = crate::bitboard::masks::king_attacks(king_sq) | Bitboard::from_square(king_sq);

    let mut attacker_count = 0usize;
    let mut attack_units = 0i32;

    let mut knights = pos.piece_bb(attacker_color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        if (knight_attacks(sq) & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units += 2;
        }
    }

    let mut bishops = pos.piece_bb(attacker_color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        let attacks = bishop_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units += 2;
        }
    }

    let mut rooks = pos.piece_bb(attacker_color, PieceKind::Rook);
    while let Some(sq) = rooks.pop_lsb() {
        let attacks = rook_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units += 3;
        }
    }

    let mut queens = pos.piece_bb(attacker_color, PieceKind::Queen);
    while let Some(sq) = queens.pop_lsb() {
        let attacks = queen_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units += 5;
        }
    }

    let our_pawns = pos.piece_bb(king_color, PieceKind::Pawn);
    let shield_pawns = pawn_shield_raw(king_sq, king_color, our_pawns) as i32;

    let enemy_pawns = pos.piece_bb(attacker_color, PieceKind::Pawn);
    let (open_files, semi_open_files) = open_files_near_king_raw(king_sq, our_pawns, enemy_pawns);
    let storm_buckets = pawn_storm_buckets(king_sq, enemy_pawns);
    let (knights_near_king, bishops_near_king) = minor_piece_shelter_counts(pos, king_sq, king_color);

    (attacker_count, attack_units, shield_pawns, open_files, semi_open_files, storm_buckets,
     knights_near_king, bishops_near_king)
}

fn pawn_shield_raw(king_sq: Square, color: Color, our_pawns: Bitboard) -> u32 {
    let king_file = king_sq.file();
    let king_rank = king_sq.rank();

    let mut file_mask = Bitboard::file_mask(king_file);
    if king_file > 0 {
        file_mask |= Bitboard::file_mask(king_file - 1);
    }
    if king_file < 7 {
        file_mask |= Bitboard::file_mask(king_file + 1);
    }

    let shield_ranks = match color {
        Color::White => {
            let r1 = king_rank.saturating_add(1).min(7);
            let r2 = king_rank.saturating_add(2).min(7);
            Bitboard::rank_mask(r1) | Bitboard::rank_mask(r2)
        }
        Color::Black => {
            let r1 = king_rank.saturating_sub(1);
            let r2 = king_rank.saturating_sub(2);
            Bitboard::rank_mask(r1) | Bitboard::rank_mask(r2)
        }
    };

    (our_pawns & file_mask & shield_ranks).count()
}

/// Returns (fully_open_file_count, semi_open_file_count) for files near the
/// king — the two counts `open_files_near_king` used to combine directly
/// into a single penalty via fixed constants.
fn open_files_near_king_raw(king_sq: Square, our_pawns: Bitboard, enemy_pawns: Bitboard) -> (i32, i32) {
    let king_file = king_sq.file();
    let mut open = 0i32;
    let mut semi_open = 0i32;

    let files_to_check = [
        king_file.checked_sub(1),
        Some(king_file),
        if king_file < 7 { Some(king_file + 1) } else { None },
    ];

    for file_opt in &files_to_check {
        if let Some(file) = file_opt {
            let file_mask = Bitboard::file_mask(*file);
            let own_on_file = (our_pawns & file_mask).is_not_empty();
            let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

            if !own_on_file {
                if !enemy_on_file {
                    open += 1;
                } else {
                    semi_open += 1;
                }
            }
        }
    }

    (open, semi_open)
}

/// Mirrors `eval::king_safety::pawn_storm_danger` exactly, except it
/// returns per-distance-bucket file counts instead of a pre-weighted sum
/// — the weight (`TunableWeights::pawn_storm_bonus[bucket]`) is applied
/// later in `predict.rs`, same pattern as `passed_pawn_bonus`.
fn pawn_storm_buckets(king_sq: Square, enemy_pawns: Bitboard) -> [i32; 8] {
    let king_file = king_sq.file();
    let king_rank = king_sq.rank() as i32;
    let mut buckets = [0i32; 8];

    let files_to_check = [
        king_file.checked_sub(1),
        Some(king_file),
        if king_file < 7 { Some(king_file + 1) } else { None },
    ];

    for file_opt in &files_to_check {
        if let Some(file) = file_opt {
            let file_mask = Bitboard::file_mask(*file);
            let mut pawns_on_file = enemy_pawns & file_mask;

            let mut best_dist = 8i32;
            while let Some(sq) = pawns_on_file.pop_lsb() {
                let dist = (sq.rank() as i32 - king_rank).abs();
                if dist < best_dist {
                    best_dist = dist;
                }
            }
            if best_dist <= 7 {
                buckets[best_dist as usize] += 1;
            }
        }
    }

    buckets
}

/// Mirrors `eval::king_safety::king_file_zone` exactly.
#[inline]
fn king_file_zone(file: u8) -> u8 {
    match file {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    }
}

/// Mirrors `eval::king_safety::minor_piece_shelter_bonus` exactly, except
/// it returns the raw (knights_near_king, bishops_near_king) counts
/// instead of a pre-weighted sum — the weights
/// (`TunableWeights::knight_near_own_king`/`bishop_near_own_king`) are
/// applied later in `predict.rs`.
fn minor_piece_shelter_counts(pos: &Position, king_sq: Square, king_color: Color) -> (i32, i32) {
    let king_zone = king_file_zone(king_sq.file());

    let mut knights_near = 0i32;
    let mut knights = pos.piece_bb(king_color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        if king_file_zone(sq.file()) == king_zone {
            knights_near += 1;
        }
    }

    let mut bishops_near = 0i32;
    let mut bishops = pos.piece_bb(king_color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        if king_file_zone(sq.file()) == king_zone {
            bishops_near += 1;
        }
    }

    (knights_near, bishops_near)
}

// ── Open lines (mirrors eval::open_lines::open_line_score) ─────────────────

type OpenLinesRaw = (i32, i32, i32, i32, i32, i32, i32, i32, i32);

fn extract_open_lines(pos: &Position, us: Color, them: Color) -> OpenLinesRaw {
    let (r_open_u, r_semi_u, r_7th_u, r_conn_u, bat_rq_u, contest_u, q_open_u, q_semi_u, bat_bq_u) =
        open_line_side_raw(pos, us);
    let (r_open_t, r_semi_t, r_7th_t, r_conn_t, bat_rq_t, contest_t, q_open_t, q_semi_t, bat_bq_t) =
        open_line_side_raw(pos, them);

    (
        r_open_u - r_open_t,
        r_semi_u - r_semi_t,
        r_7th_u - r_7th_t,
        r_conn_u - r_conn_t,
        bat_rq_u - bat_rq_t,
        contest_u - contest_t,
        q_open_u - q_open_t,
        q_semi_u - q_semi_t,
        bat_bq_u - bat_bq_t,
    )
}

/// Returns raw event counts for one color:
/// (rook_open, rook_semi, rook_7th, rooks_connected_pairs, battery_rook_queen,
///  contested_file, queen_open, queen_semi, battery_bishop_queen)
/// Mirrors `eval::threats::evaluate_threats` exactly (Phase 24 item 4,
/// D68, extended Phase 34.2/Session 133 for threat_by_rook) — same loops,
/// same attacker/defender counting, same conditions.
/// Returns (undefended_knight_diff, undefended_bishop_diff,
/// undefended_rook_diff, undefended_queen_diff, threat_by_minor_diff,
/// threat_by_rook_diff).
fn extract_threats(pos: &Position, us: Color, them: Color) -> (i32, i32, i32, i32, i32, i32) {
    let (uk_u, ub_u, ur_u, uq_u, tbm_u, tbr_u) = threats_side_raw(pos, us);
    let (uk_t, ub_t, ur_t, uq_t, tbm_t, tbr_t) = threats_side_raw(pos, them);
    (uk_u - uk_t, ub_u - ub_t, ur_u - ur_t, uq_u - uq_t, tbm_u - tbm_t, tbr_u - tbr_t)
}

/// Mirrors `eval::threats::threats_for_color` exactly, counting occurrences
/// instead of summing packed `s()` scores — same duplication pattern
/// already used for `open_line_side_raw` above (and every other term in
/// this file): a deliberate mirror, not a shared function, so
/// `predict()` can stay generic over `TunableWeights` without eval/*.rs
/// needing to expose its internals.
fn threats_side_raw(pos: &Position, color: Color) -> (i32, i32, i32, i32, i32, i32) {
    let enemy = color.flip();
    let all_occ = pos.all_pieces();

    let mut undefended_knight = 0i32;
    let mut undefended_bishop = 0i32;
    let mut undefended_rook = 0i32;
    let mut undefended_queen = 0i32;

    for kind in [PieceKind::Knight, PieceKind::Bishop, PieceKind::Rook, PieceKind::Queen] {
        for sq in pos.piece_bb(color, kind) {
            let attackers = count_attackers_raw(pos, sq, enemy, all_occ);
            let defenders = count_attackers_raw(pos, sq, color, all_occ);
            if attackers > defenders {
                match kind {
                    PieceKind::Knight => undefended_knight += 1,
                    PieceKind::Bishop => undefended_bishop += 1,
                    PieceKind::Rook => undefended_rook += 1,
                    PieceKind::Queen => undefended_queen += 1,
                    _ => unreachable!(),
                }
            }
        }
    }

    let mut threat_by_minor = 0i32;
    let enemy_rq = pos.piece_bb(enemy, PieceKind::Rook) | pos.piece_bb(enemy, PieceKind::Queen);
    for kind in [PieceKind::Knight, PieceKind::Bishop] {
        for sq in pos.piece_bb(color, kind) {
            let attacks = match kind {
                PieceKind::Knight => knight_attacks(sq),
                PieceKind::Bishop => bishop_attacks(sq, all_occ),
                _ => unreachable!(),
            };
            threat_by_minor += (attacks & enemy_rq).count() as i32;
        }
    }

    // Threat by rook: our rooks attacking enemy queen (Session 133 / Phase
    // 34.2). Mirrors `eval::threats::threats_for_color`'s rook loop exactly.
    let mut threat_by_rook = 0i32;
    let enemy_queen = pos.piece_bb(enemy, PieceKind::Queen);
    for sq in pos.piece_bb(color, PieceKind::Rook) {
        let attacks = rook_attacks(sq, all_occ);
        threat_by_rook += (attacks & enemy_queen).count() as i32;
    }

    (undefended_knight, undefended_bishop, undefended_rook, undefended_queen,
     threat_by_minor, threat_by_rook)
}

/// Mirrors `eval::threats::count_attackers` exactly.
fn count_attackers_raw(pos: &Position, sq: Square, attacker_color: Color, all_occ: Bitboard) -> u32 {
    let mut n = 0u32;
    n += (knight_attacks(sq) & pos.piece_bb(attacker_color, PieceKind::Knight)).count();
    n += (bishop_attacks(sq, all_occ)
        & (pos.piece_bb(attacker_color, PieceKind::Bishop) | pos.piece_bb(attacker_color, PieceKind::Queen)))
        .count();
    n += (rook_attacks(sq, all_occ)
        & (pos.piece_bb(attacker_color, PieceKind::Rook) | pos.piece_bb(attacker_color, PieceKind::Queen)))
        .count();
    n += (pawn_attacks(attacker_color.flip(), sq) & pos.piece_bb(attacker_color, PieceKind::Pawn)).count();
    n += (king_attacks(sq) & pos.piece_bb(attacker_color, PieceKind::King)).count();
    n
}

fn open_line_side_raw(pos: &Position, color: Color) -> (i32, i32, i32, i32, i32, i32, i32, i32, i32) {
    let our_pawns = pos.piece_bb(color, PieceKind::Pawn);
    let enemy_pawns = pos.piece_bb(color.flip(), PieceKind::Pawn);
    let all_occ = pos.all_pieces();
    let our_rooks = pos.piece_bb(color, PieceKind::Rook);
    let our_queens = pos.piece_bb(color, PieceKind::Queen);
    let our_bishops = pos.piece_bb(color, PieceKind::Bishop);
    let enemy_rooks = pos.piece_bb(color.flip(), PieceKind::Rook);

    let mut rook_open = 0i32;
    let mut rook_semi = 0i32;
    let mut rook_7th = 0i32;
    let mut battery_rq = 0i32;
    let mut contested = 0i32;

    let mut rooks = our_rooks;
    while let Some(sq) = rooks.pop_lsb() {
        let file = sq.file();
        let rank = sq.rank();
        let file_mask = Bitboard::file_mask(file);

        let own_on_file = (our_pawns & file_mask).is_not_empty();
        let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

        if !own_on_file {
            if !enemy_on_file {
                rook_open += 1;
            } else {
                rook_semi += 1;
            }
        }

        let seventh_rank = match color {
            Color::White => 6u8,
            Color::Black => 1u8,
        };
        if rank == seventh_rank {
            rook_7th += 1;
        }

        // Mirrors eval/open_lines.rs exactly — file AND rank battery,
        // one attack-set computation, no double-count risk (see comment
        // in open_lines.rs for why).
        let rook_attack_bb = rook_attacks(sq, all_occ);
        if (rook_attack_bb & file_mask & our_queens).is_not_empty() {
            battery_rq += 1;
        }
        let rank_mask = Bitboard::rank_mask(rank);
        if (rook_attack_bb & rank_mask & our_queens).is_not_empty() {
            battery_rq += 1;
        }

        if (enemy_rooks & file_mask).is_not_empty() {
            contested += 1;
        }
    }

    // Connected rooks — same pairwise dedup logic as open_lines.rs
    let mut connected_pairs = 0i32;
    {
        let mut r1 = our_rooks;
        while let Some(sq1) = r1.pop_lsb() {
            let mut r2 = r1;
            while let Some(sq2) = r2.pop_lsb() {
                if are_rooks_connected(sq1, sq2, all_occ, our_rooks) {
                    connected_pairs += 1;
                    break;
                }
            }
        }
    }

    let mut queen_open = 0i32;
    let mut queen_semi = 0i32;
    let mut battery_bq = 0i32;

    let mut queens = our_queens;
    while let Some(sq) = queens.pop_lsb() {
        let file = sq.file();
        let file_mask = Bitboard::file_mask(file);

        let own_on_file = (our_pawns & file_mask).is_not_empty();
        let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

        if !own_on_file {
            if !enemy_on_file {
                queen_open += 1;
            } else {
                queen_semi += 1;
            }
        }

        let bishop_attacks_from_queen = bishop_attacks(sq, all_occ);
        if (bishop_attacks_from_queen & our_bishops).is_not_empty() {
            battery_bq += 1;
        }
    }

    (
        rook_open, rook_semi, rook_7th, connected_pairs, battery_rq, contested, queen_open,
        queen_semi, battery_bq,
    )
}

fn are_rooks_connected(sq1: Square, sq2: Square, occupancy: Bitboard, own_rooks: Bitboard) -> bool {
    let r1 = sq1.rank();
    let f1 = sq1.file();
    let r2 = sq2.rank();
    let f2 = sq2.file();

    if r1 == r2 {
        let rank_attacks = rook_attacks(sq1, occupancy ^ Bitboard::from_square(sq2));
        return (rank_attacks & own_rooks).contains(sq2);
    }

    if f1 == f2 {
        let file_attacks = rook_attacks(sq1, occupancy ^ Bitboard::from_square(sq2));
        return (file_attacks & own_rooks).contains(sq2);
    }

    false
}

// ── PlayStyle (Phase 30, ROADMAP 29.7) ──────────────────────────────────────
//
// Raw feature counts for exactly one active PlayStyle mode — mirrors
// `eval::style.rs`'s four bonus functions term-for-term, but stops short of
// applying the tunable weight (that's predict.rs/predict_f64.rs's job, same
// split as every other feature category in this file). Deliberately
// duplicates style.rs's small amount of king-zone/center-box/distance
// logic rather than importing it, matching style.rs's own stated
// precedent ("deliberately duplicated rather than imported... fully
// decoupled from the tuned core") for exactly the same reason.

/// Raw per-mode feature counts. Only the fields relevant to `mode` are
/// ever computed by `extract_style_features` — the rest are left at 0,
/// matching `eval::style::evaluate_style`'s own single-mode dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StyleFeatures {
    pub mode: u32,
    /// Index into `TunableWeights::killer_attacker_bonus` (0..=7) — mirrors
    /// `killer_bonus`'s `attacker_count.min(7)`. Only meaningful when
    /// `mode == KILLER`.
    pub killer_attacker_idx: usize,
    /// Mirrors `killer_bonus`'s `storm_pawns` count.
    pub killer_storm_pawns: i32,
    /// Mirrors `tactical_bonus`'s `net` (our enemy-territory-control
    /// squares minus theirs).
    pub tactical_net_squares: i32,
    /// Mirrors `positional_bonus`'s `net` (our center-box control minus
    /// theirs).
    pub positional_net_squares: i32,
    /// Mirrors `endgame_bonus`'s `net` (their king's distance-to-center
    /// minus ours).
    pub endgame_net_units: i32,
}

/// Extract `StyleFeatures` for one specific `mode` (BALANCED..=ENDGAME,
/// from `eval::style`) — called separately from `extract_features()`,
/// only by `src/bin/texel_tune.rs`'s data loading, for samples whose data
/// line carries a PlayStyle mode tag. Returns raw, weight-independent
/// counts only — no `phase` parameter needed here, since the phase-based
/// scaling ((phase/24) or ((24-phase)/24), matching `eval::style`'s own
/// functions) is applied later, once, in `predict.rs`/`predict_f64.rs`'s
/// `style_score`/`style_score_f64`, using `TexelFeatures.phase`.
pub fn extract_style_features(pos: &Position, mode: u32) -> StyleFeatures {
    match mode {
        KILLER => {
            let (killer_attacker_idx, killer_storm_pawns) = extract_killer(pos);
            StyleFeatures { mode, killer_attacker_idx, killer_storm_pawns, ..Default::default() }
        }
        TACTICAL => {
            let tactical_net_squares = extract_tactical(pos);
            StyleFeatures { mode, tactical_net_squares, ..Default::default() }
        }
        POSITIONAL => {
            let positional_net_squares = extract_positional(pos);
            StyleFeatures { mode, positional_net_squares, ..Default::default() }
        }
        ENDGAME => {
            let endgame_net_units = extract_endgame(pos);
            StyleFeatures { mode, endgame_net_units, ..Default::default() }
        }
        _ => StyleFeatures { mode: BALANCED, ..Default::default() }, // no-op, mirrors evaluate_style()
    }
}

/// Mirrors `eval::style::killer_bonus`'s feature-gathering half exactly.
fn extract_killer(pos: &Position) -> (usize, i32) {
    let us = pos.side_to_move;
    let them = us.flip();
    let their_king_sq = pos.king_sq(them);
    let all_occ = pos.all_pieces();

    let king_zone = king_attacks(their_king_sq) | Bitboard::from_square(their_king_sq);

    let mut attacker_count = 0usize;
    let mut knights = pos.piece_bb(us, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        if (knight_attacks(sq) & king_zone).is_not_empty() {
            attacker_count += 1;
        }
    }
    let mut bishops = pos.piece_bb(us, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        if (bishop_attacks(sq, all_occ) & king_zone).is_not_empty() {
            attacker_count += 1;
        }
    }
    let mut rooks = pos.piece_bb(us, PieceKind::Rook);
    while let Some(sq) = rooks.pop_lsb() {
        if (rook_attacks(sq, all_occ) & king_zone).is_not_empty() {
            attacker_count += 1;
        }
    }
    let mut queens = pos.piece_bb(us, PieceKind::Queen);
    while let Some(sq) = queens.pop_lsb() {
        if (queen_attacks(sq, all_occ) & king_zone).is_not_empty() {
            attacker_count += 1;
        }
    }

    let their_king_file = their_king_sq.file() as i32;
    let mut storm_pawns = 0i32;
    let mut our_pawns = pos.piece_bb(us, PieceKind::Pawn);
    while let Some(sq) = our_pawns.pop_lsb() {
        let file_diff = (sq.file() as i32 - their_king_file).abs();
        if file_diff > 1 {
            continue;
        }
        let advanced = if us == Color::White { sq.rank() >= 4 } else { sq.rank() <= 3 };
        if advanced {
            storm_pawns += 1;
        }
    }

    (attacker_count.min(7), storm_pawns)
}

/// Mirrors `eval::style::tactical_bonus`'s feature-gathering half exactly.
fn extract_tactical(pos: &Position) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let occ = pos.all_pieces();

    let (our_target_half, their_target_half) = if us == Color::White {
        (
            Bitboard::RANK_5 | Bitboard::RANK_6 | Bitboard::RANK_7 | Bitboard::RANK_8,
            Bitboard::RANK_1 | Bitboard::RANK_2 | Bitboard::RANK_3 | Bitboard::RANK_4,
        )
    } else {
        (
            Bitboard::RANK_1 | Bitboard::RANK_2 | Bitboard::RANK_3 | Bitboard::RANK_4,
            Bitboard::RANK_5 | Bitboard::RANK_6 | Bitboard::RANK_7 | Bitboard::RANK_8,
        )
    };

    let our_count = (all_piece_attacks(pos, us, occ) & our_target_half).count() as i32;
    let their_count = (all_piece_attacks(pos, them, occ) & their_target_half).count() as i32;
    our_count - their_count
}

/// Mirrors `eval::style::positional_bonus`'s feature-gathering half exactly.
fn extract_positional(pos: &Position) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let occ = pos.all_pieces();

    let center_box = (Bitboard::FILE_C | Bitboard::FILE_D | Bitboard::FILE_E | Bitboard::FILE_F)
        & (Bitboard::RANK_3 | Bitboard::RANK_4 | Bitboard::RANK_5 | Bitboard::RANK_6);

    let our_count = (pawn_and_minor_attacks(pos, us, occ) & center_box).count() as i32;
    let their_count = (pawn_and_minor_attacks(pos, them, occ) & center_box).count() as i32;
    our_count - their_count
}

/// Mirrors `eval::style::endgame_bonus`'s feature-gathering half exactly.
fn extract_endgame(pos: &Position) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let our_dist = distance_to_center(pos.king_sq(us));
    let their_dist = distance_to_center(pos.king_sq(them));
    their_dist - our_dist
}

const CENTER_SQUARES: [Square; 4] = [Square::D4, Square::D5, Square::E4, Square::E5];

// chebyshev_distance() already exists in this file (used by the
// passed-pawn king-distance feature above) — reused here rather than
// redefined; see that definition's own doc comment.

fn distance_to_center(sq: Square) -> i32 {
    CENTER_SQUARES.iter().map(|&c| chebyshev_distance(sq, c)).min().unwrap_or(0)
}

/// Union of every square attacked by any of `color`'s pieces. Mirrors
/// `eval::style::all_piece_attacks` exactly.
fn all_piece_attacks(pos: &Position, color: Color, occ: Bitboard) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;

    let mut pawns = pos.piece_bb(color, PieceKind::Pawn);
    while let Some(sq) = pawns.pop_lsb() {
        attacks |= pawn_attacks(color, sq);
    }
    let mut knights = pos.piece_bb(color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        attacks |= knight_attacks(sq);
    }
    let mut bishops = pos.piece_bb(color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        attacks |= bishop_attacks(sq, occ);
    }
    let mut rooks = pos.piece_bb(color, PieceKind::Rook);
    while let Some(sq) = rooks.pop_lsb() {
        attacks |= rook_attacks(sq, occ);
    }
    let mut queens = pos.piece_bb(color, PieceKind::Queen);
    while let Some(sq) = queens.pop_lsb() {
        attacks |= queen_attacks(sq, occ);
    }
    let king_sq = pos.king_sq(color);
    attacks |= king_attacks(king_sq);

    attacks
}

/// Pawn + knight + bishop attacks only (no rooks/queens/king) — mirrors
/// `eval::style::pawn_and_minor_attacks` exactly.
fn pawn_and_minor_attacks(pos: &Position, color: Color, occ: Bitboard) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;

    let mut pawns = pos.piece_bb(color, PieceKind::Pawn);
    while let Some(sq) = pawns.pop_lsb() {
        attacks |= pawn_attacks(color, sq);
    }
    let mut knights = pos.piece_bb(color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        attacks |= knight_attacks(sq);
    }
    let mut bishops = pos.piece_bb(color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        attacks |= bishop_attacks(sq, occ);
    }

    attacks
}
