// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/king_safety.rs — King safety evaluation
//
// Evaluates how safe the king is based on:
//   1. Pawn shield: friendly pawns near the king block attacking lines
//   2. Open files near king: dangerous — attackers can use open files
//   3. Attacker count: enemy pieces targeting squares near king
//   4. Weak squares adjacent to king
//
// Weights adapted from Ethereal (GPL v3, Andrew Grant).
//
// ⚠️ Pet Dragon critical: NO castling safety bonus.
//    ~74% of Pet Dragon games have no castling at all.
//    King safety is evaluated purely from current pawn structure
//    and attacker proximity — never from whether castling occurred.
//
// King safety applies primarily in the middlegame (phase > 0).
// In the endgame (phase → 0) king centralisation matters more
// (handled via piece-square tables in tables.rs).
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks, queen_attacks};
use crate::bitboard::masks::{knight_attacks, king_attacks};
use crate::bitboard::Bitboard;
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

// ── King safety weights ───────────────────────────────────────────────────────
// Originally from Ethereal (GPL v3, Andrew Grant); as of Phase 14 (D35)
// ATTACKER_WEIGHT/OPEN_FILE_NEAR_KING/SEMI_OPEN_FILE_NEAR_KING/
// PAWN_SHIELD_BONUS are Pet-Dragon-specific Texel-tuned values (147,283
// samples, weight_decay=0.08, 100 epochs — see SESSION_LOG). Ethereal's
// values remain the tuner's starting point (src/texel/weights.rs).
// MAX_KING_DANGER is untouched — a structural clamp, not a tunable weight
// (D35's one deliberate nonlinearity in the whole HCE).

/// Penalty per attacker targeting the king zone (indexed by attacker count 0-7+)
/// Escalates rapidly — many attackers = danger
const ATTACKER_WEIGHT: [i32; 8] = [0, -5, 43, 79, 89, 94, 97, 99];

/// Penalty for each open file adjacent to or on the king's file (MG only)
const OPEN_FILE_NEAR_KING: i32 = -21;

/// Penalty for each semi-open file adjacent to or on the king's file (MG only)
const SEMI_OPEN_FILE_NEAR_KING: i32 = -19;

/// Pawn shield bonus per pawn within 2 ranks of king on king's or adjacent file (MG only)
const PAWN_SHIELD_BONUS: i32 = 16;

/// Pawn-storm bonus (D63 item 2, ROADMAP Phase 24) — the mirror image of
/// the pawn shield: scores ENEMY pawns advanced on files near this king
/// as an attacking resource for the opponent, rather than scoring the
/// defender's own shield pawns. MG only, same phase-scaling scope as
/// every other term here (`evaluate_king_safety`'s `* phase / 24`).
///
/// Indexed by the rank distance from the most-advanced enemy pawn on a
/// king-adjacent file to the king's own rank — a pawn already 0-2 ranks
/// away is a live attacking resource, one still on its own half of the
/// board isn't a storm yet. Hand-picked starting values, NOT yet
/// Texel-tuned — same status Phase 8's original Ethereal-derived HCE
/// terms had before Phase 14's tuning pass. Like every other constant in
/// this file, wired into `texel::predict()`'s `king_safety_side()` via a
/// matching `TunableWeights::pawn_storm_bonus` field — see
/// `src/texel/{features,predict,weights,weights_f64,predict_f64}.rs`
/// (learned the hard way on D63 item 1 not to skip this).
const PAWN_STORM_BONUS: [i32; 8] = [40, 32, 24, 16, 8, 0, 0, 0];

/// Minor-piece defensive clustering bonus (D63 item 3, ROADMAP Phase 24,
/// design option A — see SESSION_LOG/DECISIONS discussion before
/// implementing). Flat MG-only bonus per OUR knight/bishop sitting in
/// the same king-file-third zone as OUR OWN king. Deliberately coarse —
/// zone-level, not per-square — so this stays a handful of parameters
/// rather than a full king-relative PST bucket set (which would hit the
/// same parameter-count-vs-training-data wall NNUE already has; see
/// D63 in DECISIONS.md for the full reasoning).
///
/// This is the mirror image of `ATTACKER_WEIGHT` above: that scores
/// ENEMY pieces attacking near THIS king (tropism, attacking side);
/// this scores OUR OWN minor pieces sitting near OUR OWN king
/// (clustering, defensive side) — a genuinely different effect, not
/// double-counting the same proximity from two angles.
const KNIGHT_NEAR_OWN_KING_BONUS: i32 = 8;
const BISHOP_NEAR_OWN_KING_BONUS: i32 = 6;

/// Max king danger score before scaling (prevents integer overflow)
const MAX_KING_DANGER: i32 = 2400;

// ── Main evaluation function ──────────────────────────────────────────────────

/// Evaluate king safety for both sides and return score from side-to-move perspective.
///
/// King safety is purely a middlegame concern — scaled by phase.
/// In the endgame, the PST tables handle king centralisation.
pub fn evaluate_king_safety(pos: &Position, phase: i32) -> i32 {
    // King safety only matters in the middlegame
    if phase == 0 {
        return 0;
    }

    let us   = pos.side_to_move;
    let them = us.flip();

    let our_safety   = king_safety_score(pos, us,   them);
    let their_safety = king_safety_score(pos, them, us);

    // Scale by phase: full weight in middlegame, zero in endgame
    (our_safety - their_safety) * phase / 24
}

/// Compute king safety score for one king (our_color = king being evaluated,
/// attacker_color = the side attacking).
/// Returns a raw i32 danger score (negative = danger, positive = safe).
fn king_safety_score(pos: &Position, king_color: Color, attacker_color: Color) -> i32 {
    let king_sq = pos.king_sq(king_color);
    let all_occ = pos.all_pieces();

    // ── King zone ─────────────────────────────────────────────────────────────
    // Squares around the king (including king itself)
    let king_zone = king_attacks(king_sq) | Bitboard::from_square(king_sq);

    // ── Count attackers targeting king zone ───────────────────────────────────
    let mut attacker_count = 0usize;
    let mut attack_units   = 0i32;

    // Knights
    let mut knights = pos.piece_bb(attacker_color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        if (knight_attacks(sq) & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units   += 2;
        }
    }

    // Bishops
    let mut bishops = pos.piece_bb(attacker_color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        let attacks = bishop_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units   += 2;
        }
    }

    // Rooks
    let mut rooks = pos.piece_bb(attacker_color, PieceKind::Rook);
    while let Some(sq) = rooks.pop_lsb() {
        let attacks = rook_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units   += 3;
        }
    }

    // Queens
    let mut queens = pos.piece_bb(attacker_color, PieceKind::Queen);
    while let Some(sq) = queens.pop_lsb() {
        let attacks = queen_attacks(sq, all_occ);
        if (attacks & king_zone).is_not_empty() {
            attacker_count += 1;
            attack_units   += 5;
        }
    }

    // ── Pawn shield ───────────────────────────────────────────────────────────
    let our_pawns = pos.piece_bb(king_color, PieceKind::Pawn);
    let shield_pawns = pawn_shield(king_sq, king_color, our_pawns);
    let shield_score = shield_pawns as i32 * PAWN_SHIELD_BONUS;

    // ── Open/semi-open files near king ────────────────────────────────────────
    let enemy_pawns = pos.piece_bb(attacker_color, PieceKind::Pawn);
    let open_file_penalty = open_files_near_king(king_sq, our_pawns, enemy_pawns);

    // ── Pawn storm (D63 item 2) ─────────────────────────────────────────────
    let storm_danger = pawn_storm_danger(king_sq, enemy_pawns);

    // ── Minor-piece defensive clustering (D63 item 3) ───────────────────────
    let shelter_bonus = minor_piece_shelter_bonus(pos, king_sq, king_color);

    // ── Combine ───────────────────────────────────────────────────────────────
    // King danger = attacker units scaled by attacker count weight
    let weight_idx = attacker_count.min(7);
    let danger = (attack_units * ATTACKER_WEIGHT[weight_idx] / 100)
               .min(MAX_KING_DANGER);

    shield_score + open_file_penalty - danger - storm_danger + shelter_bonus
}

/// Count friendly pawns in the king's pawn shield zone.
/// Zone = king file ± 1, within 1-2 ranks ahead of king.
fn pawn_shield(king_sq: Square, color: Color, our_pawns: Bitboard) -> u32 {
    let king_file = king_sq.file();
    let king_rank = king_sq.rank();

    // Files to check: king file and adjacent
    let mut file_mask = Bitboard::file_mask(king_file);
    if king_file > 0 {
        file_mask |= Bitboard::file_mask(king_file - 1);
    }
    if king_file < 7 {
        file_mask |= Bitboard::file_mask(king_file + 1);
    }

    // Rank range ahead of king (1 or 2 squares in pawn's direction)
    let shield_ranks = match color {
        Color::White => {
            // Shield is on ranks above king (toward rank 8)
            let r1 = king_rank.saturating_add(1).min(7);
            let r2 = king_rank.saturating_add(2).min(7);
            Bitboard::rank_mask(r1) | Bitboard::rank_mask(r2)
        }
        Color::Black => {
            // Shield is on ranks below king (toward rank 1)
            let r1 = king_rank.saturating_sub(1);
            let r2 = king_rank.saturating_sub(2);
            Bitboard::rank_mask(r1) | Bitboard::rank_mask(r2)
        }
    };

    (our_pawns & file_mask & shield_ranks).count()
}

/// Compute open file penalty for files near the king.
/// Open file = no own pawn on that file.
/// Semi-open file = no own pawn but enemy pawn present.
fn open_files_near_king(king_sq: Square, our_pawns: Bitboard, enemy_pawns: Bitboard) -> i32 {
    let king_file = king_sq.file();
    let mut penalty = 0i32;

    // Check king file and adjacent files
    let files_to_check = [
        king_file.checked_sub(1),
        Some(king_file),
        if king_file < 7 { Some(king_file + 1) } else { None },
    ];

    for file_opt in &files_to_check {
        if let Some(file) = file_opt {
            let file_mask = Bitboard::file_mask(*file);
            let own_on_file   = (our_pawns   & file_mask).is_not_empty();
            let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

            if !own_on_file {
                if !enemy_on_file {
                    // Fully open file
                    penalty += OPEN_FILE_NEAR_KING;
                } else {
                    // Semi-open (enemy pawn present)
                    penalty += SEMI_OPEN_FILE_NEAR_KING;
                }
            }
        }
    }

    penalty
}

/// Compute the pawn-storm danger contributed by enemy pawns advanced on
/// files near this king (D63 item 2). For each of the king's own file and
/// the two adjacent files, finds the most-advanced enemy pawn (smallest
/// rank distance to the king) and looks up its threat by that distance —
/// mirrors `pawn_shield()`'s file-selection logic exactly, just measuring
/// the opponent's pawns instead of the defender's own.
fn pawn_storm_danger(king_sq: Square, enemy_pawns: Bitboard) -> i32 {
    let king_file = king_sq.file();
    let king_rank = king_sq.rank() as i32;
    let mut danger = 0i32;

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
                danger += PAWN_STORM_BONUS[best_dist as usize];
            }
        }
    }

    danger
}

/// Zone index for a file: 0 = queenside (a-c), 1 = center (d-e),
/// 2 = kingside (f-h). Deliberately coarse (3 buckets, not 8 per-file)
/// per D63's "stay coarse" constraint.
#[inline]
fn king_file_zone(file: u8) -> u8 {
    match file {
        0..=2 => 0,
        3..=4 => 1,
        _ => 2,
    }
}

/// Minor-piece defensive clustering bonus (D63 item 3): counts OUR
/// knights and bishops sitting in the same file-third zone as OUR OWN
/// king and returns a flat per-piece bonus. See `KNIGHT_NEAR_OWN_KING_
/// BONUS`/`BISHOP_NEAR_OWN_KING_BONUS` above for the rationale and why
/// this doesn't double-count `ATTACKER_WEIGHT`'s tropism term.
fn minor_piece_shelter_bonus(pos: &Position, king_sq: Square, king_color: Color) -> i32 {
    let king_zone = king_file_zone(king_sq.file());
    let mut bonus = 0i32;

    let mut knights = pos.piece_bb(king_color, PieceKind::Knight);
    while let Some(sq) = knights.pop_lsb() {
        if king_file_zone(sq.file()) == king_zone {
            bonus += KNIGHT_NEAR_OWN_KING_BONUS;
        }
    }

    let mut bishops = pos.piece_bb(king_color, PieceKind::Bishop);
    while let Some(sq) = bishops.pop_lsb() {
        if king_file_zone(sq.file()) == king_zone {
            bonus += BISHOP_NEAR_OWN_KING_BONUS;
        }
    }

    bonus
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
    fn test_king_safety_start_pos_symmetric() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_king_safety(&pos, phase);
        // Start is symmetric — both kings equally safe
        assert_eq!(score, 0, "Start position is symmetric — king safety should be 0");
    }

    #[test]
    fn test_king_safety_zero_in_endgame() {
        setup();
        // Pure endgame: phase = 0 → king safety = 0
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = evaluate_king_safety(&pos, 0);
        assert_eq!(score, 0, "King safety should be 0 in pure endgame (phase=0)");
    }

    #[test]
    fn test_king_exposed_vs_sheltered() {
        setup();
        // White king in corner with pawns (sheltered) vs Black king exposed
        // FEN: White Kg1 with pawns on f2/g2/h2; Black king on e8 exposed
        let fen = "4k3/8/8/8/8/8/5PPP/6K1 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = 20; // near middlegame
        let score = evaluate_king_safety(&pos, phase);
        // White king is sheltered, Black is exposed → White should be safer
        // (positive score from White's perspective = White safer)
        assert!(score > 0,
            "Sheltered White king should outscore exposed Black king: {}", score);
    }

    #[test]
    fn test_no_castling_bias() {
        setup();
        // King on g1 (castled-looking position) vs King on c1 (queenside-castled)
        // Both have equivalent pawn shields — should score similarly
        let fen1 = "4k3/8/8/8/8/8/5PPP/6K1 w - - 0 1"; // king on g1
        let fen2 = "4k3/8/8/8/8/8/PPP5/2K5 w - - 0 1"; // king on c1
        let pos1 = Position::from_fen(fen1).unwrap();
        let pos2 = Position::from_fen(fen2).unwrap();
        let phase = 20;
        let s1 = evaluate_king_safety(&pos1, phase);
        let s2 = evaluate_king_safety(&pos2, phase);
        // Both have similar shelter — scores should be close
        assert!((s1 - s2).abs() < 50,
            "Both king positions have similar shelter, scores should be close: {} vs {}",
            s1, s2);
    }

    #[test]
    fn test_pawn_shield_count() {
        setup();
        // White King on g1, pawns on f2/g2/h2 = 3 shield pawns
        let fen = "4k3/8/8/8/8/8/5PPP/6K1 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let king_sq = pos.king_sq(Color::White);
        let our_pawns = pos.piece_bb(Color::White, PieceKind::Pawn);
        let count = pawn_shield(king_sq, Color::White, our_pawns);
        assert_eq!(count, 3, "3 pawns directly shielding king on g1");
    }

    #[test]
    fn test_king_safety_1000_positions_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_king_safety(&pos, phase);
        }
    }

    #[test]
    fn test_king_safety_bounded() {
        setup();
        // Score should be bounded even in extreme positions
        let fens = [
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "r3k2r/pppppppp/8/8/8/8/PPPPPPPP/4K3 w kq - 0 1",
            "4k3/pppppppp/8/8/8/8/8/4K3 w - - 0 1",
        ];
        for fen in &fens {
            let pos = Position::from_fen(fen).unwrap();
            let phase = game_phase(&pos);
            let score = evaluate_king_safety(&pos, phase);
            assert!(score.abs() < 3000,
                "King safety score should be bounded: {} for {}", score, fen);
        }
    }

    #[test]
    fn test_pawn_storm_advanced_pawn_more_dangerous() {
        setup();
        let king_sq = Square::G1;

        let mut advanced = Bitboard::EMPTY;
        advanced.set(Square::G4); // rank distance 3 from g1

        let mut home = Bitboard::EMPTY;
        home.set(Square::G7); // rank distance 6 from g1

        let danger_advanced = pawn_storm_danger(king_sq, advanced);
        let danger_home = pawn_storm_danger(king_sq, home);

        assert!(
            danger_advanced > danger_home,
            "An advanced enemy pawn should be a bigger storm threat than one still at home: advanced={} home={}",
            danger_advanced, danger_home
        );
    }

    #[test]
    fn test_pawn_storm_far_file_ignored() {
        setup();
        let king_sq = Square::G1;
        let mut far_file_pawn = Bitboard::EMPTY;
        far_file_pawn.set(Square::A4); // not on f/g/h file — irrelevant to a g1 king
        assert_eq!(
            pawn_storm_danger(king_sq, far_file_pawn),
            0,
            "Pawn on a file not adjacent to the king should contribute no storm danger"
        );
    }

    #[test]
    fn test_pawn_storm_sums_across_files() {
        setup();
        let king_sq = Square::G1;

        let mut one_pawn = Bitboard::EMPTY;
        one_pawn.set(Square::G4);

        let mut two_pawns = Bitboard::EMPTY;
        two_pawns.set(Square::F4);
        two_pawns.set(Square::H4);

        let danger_one = pawn_storm_danger(king_sq, one_pawn);
        let danger_two = pawn_storm_danger(king_sq, two_pawns);

        assert_eq!(
            danger_two, danger_one * 2,
            "Two equally-advanced storm pawns on different adjacent files should sum: one={} two={}",
            danger_one, danger_two
        );
    }

    #[test]
    fn test_king_safety_penalizes_enemy_pawn_storm() {
        setup();
        // White Kg1, Black king far away on b8 (no bearing on White's
        // safety). Position A: Black's lone pawn is storming on g4.
        // Position B: same pawn, still at home on g7. White has no pawns
        // in either FEN, so shield/open-file terms are identical between
        // A and B — only the storm term differs.
        let fen_storming = "1k6/8/8/8/6p1/8/8/6K1 w - - 0 1";
        let fen_home      = "1k6/6p1/8/8/8/8/8/6K1 w - - 0 1";

        let pos_storming = Position::from_fen(fen_storming).unwrap();
        let pos_home      = Position::from_fen(fen_home).unwrap();
        let phase = 20; // near middlegame

        let score_storming = evaluate_king_safety(&pos_storming, phase);
        let score_home      = evaluate_king_safety(&pos_home, phase);

        assert!(
            score_storming < score_home,
            "White should be less safe with Black's pawn storming on g4 than sitting at home on g7: storming={} home={}",
            score_storming, score_home
        );
    }

    #[test]
    fn test_king_file_zone_buckets() {
        assert_eq!(king_file_zone(0), 0); // a
        assert_eq!(king_file_zone(2), 0); // c
        assert_eq!(king_file_zone(3), 1); // d
        assert_eq!(king_file_zone(4), 1); // e
        assert_eq!(king_file_zone(5), 2); // f
        assert_eq!(king_file_zone(7), 2); // h
    }

    #[test]
    fn test_minor_piece_shelter_knight_same_zone_vs_different() {
        setup();
        assert!(
            KNIGHT_NEAR_OWN_KING_BONUS > 0,
            "sanity: bonus constant should be positive"
        );

        // White Kg1 (kingside zone), knight on h3 (kingside, same zone)
        // vs knight on a1 (queenside, different zone). Black king a8 is
        // irrelevant to White's own shelter term.
        let fen_same = "k7/8/8/8/8/7N/8/6K1 w - - 0 1";
        let fen_far  = "k7/8/8/8/8/8/8/N5K1 w - - 0 1";

        let pos_same = Position::from_fen(fen_same).unwrap();
        let pos_far  = Position::from_fen(fen_far).unwrap();

        let bonus_same = minor_piece_shelter_bonus(&pos_same, pos_same.king_sq(Color::White), Color::White);
        let bonus_far  = minor_piece_shelter_bonus(&pos_far, pos_far.king_sq(Color::White), Color::White);

        assert_eq!(bonus_same, KNIGHT_NEAR_OWN_KING_BONUS);
        assert_eq!(bonus_far, 0);
    }

    #[test]
    fn test_minor_piece_shelter_bishop_same_zone() {
        setup();
        // White Kg1 (kingside zone), bishop on f2 (kingside, same zone)
        // vs bishop on b2 (queenside, different zone).
        let fen_same = "k7/8/8/8/8/8/5B2/6K1 w - - 0 1";
        let fen_far  = "k7/8/8/8/8/8/1B6/6K1 w - - 0 1";

        let pos_same = Position::from_fen(fen_same).unwrap();
        let pos_far  = Position::from_fen(fen_far).unwrap();

        let bonus_same = minor_piece_shelter_bonus(&pos_same, pos_same.king_sq(Color::White), Color::White);
        let bonus_far  = minor_piece_shelter_bonus(&pos_far, pos_far.king_sq(Color::White), Color::White);

        assert_eq!(bonus_same, BISHOP_NEAR_OWN_KING_BONUS);
        assert_eq!(bonus_far, 0);
    }

    #[test]
    fn test_minor_piece_shelter_sums_multiple_pieces() {
        setup();
        // White Kg1, knight h3 (kingside) and bishop f2 (kingside) — both
        // should count.
        let fen = "k7/8/8/8/8/7N/5B2/6K1 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();

        let bonus = minor_piece_shelter_bonus(&pos, pos.king_sq(Color::White), Color::White);
        assert_eq!(bonus, KNIGHT_NEAR_OWN_KING_BONUS + BISHOP_NEAR_OWN_KING_BONUS);
    }

    #[test]
    fn test_king_safety_rewards_minor_piece_shelter() {
        setup();
        // White Kg1, Black king far away on b8 (no bearing on White's
        // safety). Position A: White's knight and bishop both sit in the
        // kingside zone near the king. Position B: same pieces, both on
        // the queenside instead. White has no pawns in either FEN, so
        // shield/open-file/storm terms are identical between A and B —
        // only the shelter term differs.
        let fen_sheltered = "1k6/8/8/8/8/7N/5B2/6K1 w - - 0 1";
        let fen_scattered  = "1k6/8/8/8/8/N7/1B6/6K1 w - - 0 1";

        let pos_sheltered = Position::from_fen(fen_sheltered).unwrap();
        let pos_scattered  = Position::from_fen(fen_scattered).unwrap();
        let phase = 20; // near middlegame

        let score_sheltered = evaluate_king_safety(&pos_sheltered, phase);
        let score_scattered  = evaluate_king_safety(&pos_scattered, phase);

        assert!(
            score_sheltered > score_scattered,
            "White should be safer with minors sheltering near the king than scattered on the far side: sheltered={} scattered={}",
            score_sheltered, score_scattered
        );
    }
}
