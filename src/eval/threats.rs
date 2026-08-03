// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/threats.rs — Threats term (Phase 24 item 4, D68; extended Phase 34.2
// with ThreatByRook, Session 133)
//
// Adapted from Stockfish's Threats evaluation concept (GPL v3) — the fourth
// HCE gap found in D63's original audit (D68, Session 84), scoped down to
// two sub-terms for the original implementation pass rather than
// Stockfish's full threats.cpp (hanging pieces, weak queen protection,
// restricted pieces, slider-on-queen, etc. — several of those overlap with
// terms Pet Dragon already has elsewhere, see the double-counting note
// below). A third sub-term, THREAT_BY_ROOK_BONUS, was added in Session 133
// per the external review's §8.1 (cheapest, most-precedented of that
// review's missing-term list — near-identical shape to THREAT_BY_MINOR
// below, same double-counting argument already accepted for it):
//
//   1. UNDEFENDED_PENALTY — one of our pieces (knight/bishop/rook/queen;
//      pawns excluded, see note) is attacked by more enemy pieces than it
//      has defenders. Scaled by piece kind — losing a queen this way hurts
//      far more than losing a knight.
//   2. THREAT_BY_MINOR_BONUS — one of our knights or bishops is currently
//      attacking an enemy rook or queen. A live tactical threat, distinct
//      from mobility.rs's plain square-count (which scores every attacked
//      square equally regardless of what piece sits on it, if any).
//   3. THREAT_BY_ROOK_BONUS — one of our rooks is currently attacking an
//      enemy queen. Scoped to rook-attacks-queen only, not rook-attacks-
//      rook (a roughly even trade, weak signal) — matches Stockfish's own
//      restriction of ThreatByRook's meaningful bonus to higher-value
//      targets. Distinct from THREAT_BY_MINOR_BONUS, which is scoped to
//      knight/bishop attackers only — rooks were never counted there, so
//      this is genuinely new signal, not a restatement.
//
// Pawns excluded from UNDEFENDED_PENALTY: pawn "hanging-ness" is already
// substantially captured by pawns.rs's isolated/doubled/backward terms,
// and adding a 5th array slot for comparatively little marginal signal
// wasn't judged worth the added parameter count for this pass.
// "Restricted piece" (Stockfish's low-safe-mobility penalty) deliberately
// NOT included — it's the most direct overlap risk with mobility.rs's
// existing per-count bonus table, which already implicitly scores a
// piece with few safe squares lower than one with many. Dropped rather
// than risk double-counting the same signal from two angles.
//
// Double-counting check against existing terms (D68's explicit
// requirement, same discipline Phase 24 items 1-3 used, applied again to
// THREAT_BY_ROOK_BONUS in Session 133):
//   - mobility.rs counts squares attacked, not what's ON those squares —
//     structurally different from "is THIS piece attacked more than
//     defended," which depends on enemy attacker/defender counts on this
//     piece's own square, not squares it attacks. Same argument covers
//     THREAT_BY_ROOK_BONUS: mobility.rs still doesn't look at what's on
//     the attacked square.
//   - king_safety.rs's ATTACKER_WEIGHT only counts attackers in the KING
//     ZONE specifically — UNDEFENDED_PENALTY applies to any of our pieces
//     anywhere on the board, king-zone or not, and is about THIS piece's
//     own defense status, not king danger.
//   - THREAT_BY_MINOR_BONUS is scoped to knight/bishop attackers only;
//     rooks aren't counted there, so THREAT_BY_ROOK_BONUS doesn't overlap
//     it.
//   - UNDEFENDED_PENALTY scores the *victim's* undefended status once,
//     regardless of attacker; THREAT_BY_ROOK_BONUS scores the *attacker's*
//     side of the same event — this file already accepts that dual-
//     perspective pattern for UNDEFENDED_PENALTY/THREAT_BY_MINOR_BONUS, so
//     this is consistent precedent, not new risk.
//   No existing term measures "is this specific piece under-defended," "is
//   this minor piece threatening a bigger enemy piece," or "is one of our
//   rooks threatening the enemy queen" — all three are genuinely new
//   signal, not restated signal.
//
// Reuses the same low-level attack primitives (knight_attacks,
// bishop_attacks, rook_attacks, queen_attacks, pawn_attacks) every other
// eval module already uses — no new move-generation machinery, per D68.
// ============================================================================

use crate::bitboard::{bishop_attacks, rook_attacks, Bitboard};
use crate::bitboard::masks::{knight_attacks, king_attacks, pawn_attacks};
use crate::eval::material::{s, taper};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

/// Penalty when one of our pieces is attacked by more enemy pieces than it
/// has defenders. Indexed by `PieceKind as usize`; Pawn (0) and King (5)
/// slots unused (always `s(0,0)`) — see module doc for why pawns are
/// excluded, and a king can't sensibly be "undefended" in this sense
/// (king safety already has its own dedicated term for king danger).
const UNDEFENDED_PENALTY: [i64; 6] = [
    s(0, 0),     // Pawn — unused
    s(-25, -15), // Knight
    s(-25, -15), // Bishop
    s(-40, -25), // Rook
    s(-80, -50), // Queen
    s(0, 0),     // King — unused
];

/// Bonus per hit when one of our knights or bishops attacks an enemy rook
/// or queen. A fork (one minor attacking both) scores twice, naturally —
/// no special-case needed, matches how MVV-LVA-style scoring elsewhere in
/// this codebase already lets multiple simultaneous threats add up.
const THREAT_BY_MINOR_BONUS: i64 = s(15, 10);

/// Bonus per hit when one of our rooks attacks an enemy queen. Distinct
/// from THREAT_BY_MINOR_BONUS (knight/bishop attackers only) — a rook
/// threatening a queen carries different follow-up risk than a minor doing
/// so, so it gets its own, smaller-magnitude table (added Session 133, per
/// external review §8.1). Deliberately not rook-attacks-rook: a roughly
/// even trade is a weak signal, matching Stockfish's own restriction of
/// ThreatByRook's meaningful bonus to higher-value targets.
const THREAT_BY_ROOK_BONUS: i64 = s(20, 12);

/// Evaluate threats for both sides. Returns a tapered score from the side-
/// to-move's perspective, matching every other `evaluate_*` function in
/// this module (`us = pos.side_to_move`, not hardcoded White/Black — see
/// `evaluate_material`/`evaluate_open_lines` for the same pattern).
pub fn evaluate_threats(pos: &Position, phase: i32) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let ours = threats_for_color(pos, us);
    let theirs = threats_for_color(pos, them);
    taper(ours - theirs, phase)
}

fn threats_for_color(pos: &Position, color: Color) -> i64 {
    let enemy = color.flip();
    let all_occ = pos.all_pieces();
    let mut score: i64 = 0;

    // ── Undefended pieces ────────────────────────────────────────────────
    for kind in [PieceKind::Knight, PieceKind::Bishop, PieceKind::Rook, PieceKind::Queen] {
        for sq in pos.piece_bb(color, kind) {
            let attackers = count_attackers(pos, sq, enemy, all_occ);
            let defenders = count_attackers(pos, sq, color, all_occ);
            if attackers > defenders {
                score += UNDEFENDED_PENALTY[kind as usize];
            }
        }
    }

    // ── Threat by minor: our knights/bishops attacking enemy rook/queen ───
    let enemy_rq = pos.piece_bb(enemy, PieceKind::Rook) | pos.piece_bb(enemy, PieceKind::Queen);
    for kind in [PieceKind::Knight, PieceKind::Bishop] {
        for sq in pos.piece_bb(color, kind) {
            let attacks = match kind {
                PieceKind::Knight => knight_attacks(sq),
                PieceKind::Bishop => bishop_attacks(sq, all_occ),
                _ => unreachable!(),
            };
            let hits = (attacks & enemy_rq).count();
            score += THREAT_BY_MINOR_BONUS * hits as i64;
        }
    }

    // ── Threat by rook: our rooks attacking enemy queen (Session 133) ─────
    let enemy_queen = pos.piece_bb(enemy, PieceKind::Queen);
    for sq in pos.piece_bb(color, PieceKind::Rook) {
        let attacks = rook_attacks(sq, all_occ);
        let hits = (attacks & enemy_queen).count();
        score += THREAT_BY_ROOK_BONUS * hits as i64;
    }

    score
}

/// Count pieces of `attacker_color` that attack `sq`, reusing the standard
/// "reverse attack generation" trick for pawns (the squares a pawn of the
/// OPPOSITE color standing ON `sq` would attack are, by diagonal symmetry,
/// exactly the squares an `attacker_color` pawn would need to stand on to
/// attack `sq`). Queen attacks fall out for free from the bishop/rook
/// checks below — a queen attacking `sq` does so via either a diagonal or
/// a straight ray from its actual square, never both at once for a single
/// target square, so there's no double-count risk combining the two.
fn count_attackers(pos: &Position, sq: Square, attacker_color: Color, all_occ: Bitboard) -> u32 {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;
    use crate::eval::material::game_phase;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_threats_start_pos_symmetric() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_threats(&pos, phase);
        assert_eq!(score, 0, "Start position is symmetric — threats should be 0");
    }

    #[test]
    fn test_undefended_rook_penalized() {
        setup();
        // White rook on d5, attacked by Black bishop on b3 (diagonal),
        // completely undefended by any White piece. Should score negative
        // for White (from White's perspective in the isolated term).
        let fen = "4k3/8/8/3R4/8/1b6/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = threats_for_color(&pos, Color::White);
        assert!(score < 0, "Undefended rook under attack should score negative: {}", score);
        let _ = phase; // phase unused here, just constructing a valid call elsewhere
    }

    #[test]
    fn test_defended_rook_not_penalized() {
        setup();
        // Same as above, but White queen on d1 now defends the rook along
        // the d-file — attackers(1) no longer exceeds defenders(1).
        let fen = "4k3/8/8/3R4/8/1b6/8/3QK3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = threats_for_color(&pos, Color::White);
        assert_eq!(score, 0, "Defended rook (attackers == defenders) should not be penalized: {}", score);
    }

    #[test]
    fn test_threat_by_minor_bonus_applies() {
        setup();
        // White knight on e5 forks Black's rook on d7 and queen on f7 (both
        // a knight-move away) — two hits, bonus should apply twice.
        let fen = "4k3/3r1q2/8/4N3/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = threats_for_color(&pos, Color::White);
        assert!(score > 0, "Knight forking rook+queen should score positive: {}", score);
        assert!(score >= 2 * THREAT_BY_MINOR_BONUS,
            "Forking both should score at least double the single-hit bonus: {}", score);
    }

    #[test]
    fn test_threat_by_rook_bonus_applies() {
        // Session 133 (external review §8.1): White rook on d5 attacks
        // Black's queen on d8 along the open d-file. White's rook is
        // defended by the c4 pawn (Black queen also attacks back down the
        // same file, so without a defender this would also trigger
        // UNDEFENDED_PENALTY and could mask/flip the sign of the bonus
        // being tested — the pawn isolates this test to ThreatByRook
        // alone). Should score positive, at least THREAT_BY_ROOK_BONUS.
        setup();
        let fen = "3qk3/8/8/3R4/2P5/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = threats_for_color(&pos, Color::White);
        assert!(score > 0, "Rook attacking undefended enemy queen should score positive: {}", score);
        assert!(score >= THREAT_BY_ROOK_BONUS,
            "Rook-attacks-queen should score at least the single-hit bonus: {}", score);
    }

    #[test]
    fn test_threat_by_rook_scoped_to_queen_only() {
        // Session 133: rook-attacks-rook must NOT trigger
        // THREAT_BY_ROOK_BONUS — deliberately scoped to queen targets only
        // (a roughly even trade is a weak signal, matches Stockfish's own
        // restriction). White rook on d5 attacks Black's rook on d8, no
        // queen anywhere on the board. White's rook is defended by the c4
        // pawn (attackers=1 from Black's rook, defenders=1) so
        // UNDEFENDED_PENALTY doesn't fire either — isolates this test to
        // ThreatByRook's scoping alone, score should be exactly 0.
        setup();
        let fen = "3rk3/8/8/3R4/2P5/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let score = threats_for_color(&pos, Color::White);
        assert_eq!(score, 0,
            "Rook attacking enemy rook (not queen) must not trigger \
             THREAT_BY_ROOK_BONUS: {}", score);
    }

    #[test]
    fn test_evaluate_threats_flips_sign_with_side_to_move() {
        setup();
        // Regression test for a real bug caught by CI (not by this test
        // suite originally — a gap in the initial test coverage, since
        // every hand-written test above happened to use a fresh/White-to-
        // move position, which is exactly what let this bug through
        // locally). `evaluate_threats` originally hardcoded
        // Color::White/Color::Black instead of using
        // `pos.side_to_move`/`.flip()` like every other `evaluate_*`
        // function — worked by coincidence whenever White was to move
        // (game start), silently wrong sign whenever Black was to move
        // (any mid-game position on Black's turn).
        //
        // Same board (White rook on d5, undefended, attacked by a Black
        // bishop), only the side-to-move flag differs. The two results
        // must be exact negatives of each other — "us minus them" flips
        // sign precisely when whose-turn-it-is flips, board unchanged.
        let fen_white_to_move = "4k3/8/8/3R4/8/1b6/8/4K3 w - - 0 1";
        let fen_black_to_move = "4k3/8/8/3R4/8/1b6/8/4K3 b - - 0 1";

        let pos_w = Position::from_fen(fen_white_to_move).unwrap();
        let pos_b = Position::from_fen(fen_black_to_move).unwrap();
        let phase_w = game_phase(&pos_w);
        let phase_b = game_phase(&pos_b);

        let score_w = evaluate_threats(&pos_w, phase_w);
        let score_b = evaluate_threats(&pos_b, phase_b);

        assert_eq!(score_w, -score_b,
            "same board, only side-to-move differs — scores must be exact negatives: white-to-move={}, black-to-move={}",
            score_w, score_b);
        assert!(score_w < 0, "White's own hanging rook should be bad for White (White to move): {}", score_w);
        assert!(score_b > 0, "White's hanging rook should be good for Black (Black to move): {}", score_b);
    }

    #[test]
    fn test_threats_1000_pet_dragon_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_threats(&pos, phase);
        }
    }

    #[test]
    fn test_threats_bounded() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let score = evaluate_threats(&pos, phase);
            assert!(score.abs() < 1000,
                "Threats score should be bounded, got {} (seed {})", score, seed);
        }
    }
}
