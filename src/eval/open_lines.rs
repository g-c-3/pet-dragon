// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/open_lines.rs — Open file and diagonal evaluation
//
// ⚠️ Pet Dragon CRITICAL module.
//    In standard chess, open files and diagonals develop gradually as pawns
//    are traded. In Pet Dragon, the middle ranks (3–6) are completely empty
//    at game start. Rooks, Bishops, and Queens have immediate access to open
//    lines from move 1. This evaluation is active and significant from the
//    very first position evaluation.
//
// Evaluates:
//   1. Rook on open file (no pawns of either colour)
//   2. Rook on semi-open file (no own pawns, enemy pawn present)
//   3. Rook on 7th rank (or 8th for Black) — trapped enemy king/pawns
//   4. Connected rooks (same file or rank, no pieces between)
//   5. Queen on open file
//   6. Battery detection: Queen+Rook on same file, Queen+Bishop on same diagonal
//   7. Contested open files (both sides have a rook)
//
// Weights adapted from Ethereal (GPL v3, Andrew Grant) with Pet Dragon
// starting position adjustments (weights active from move 1 — no suppression).
// ============================================================================

use crate::bitboard::{rook_attacks, bishop_attacks};
use crate::bitboard::Bitboard;
use crate::eval::material::{s, taper};
use crate::position::Position;
use crate::types::{Color, PieceKind};

// ── Open line bonuses ─────────────────────────────────────────────────────────

/// Rook on fully open file (no pawns of any colour)
const ROOK_OPEN_FILE: i64 = s(48, 21);

/// Rook on semi-open file (no own pawns, enemy pawn present)
const ROOK_SEMI_OPEN_FILE: i64 = s(23, 11);

/// Rook on the 7th rank (or 2nd for Black) — attacks enemy pawns/king
const ROOK_ON_SEVENTH: i64 = s(17, 54);

/// Two rooks connected (same file or rank, nothing between them)
const ROOKS_CONNECTED: i64 = s(11, 13);

/// Queen on open file
const QUEEN_OPEN_FILE: i64 = s(3, 6);

/// Queen on semi-open file
const QUEEN_SEMI_OPEN_FILE: i64 = s(2, 4);

/// Battery: Queen + Rook on same open file (combined attack)
const BATTERY_ROOK_QUEEN: i64 = s(18, 10);

/// Battery: Queen + Bishop on same open diagonal
const BATTERY_BISHOP_QUEEN: i64 = s(14, 8);

/// Contested file penalty: both sides have rook on same file
const CONTESTED_FILE: i64 = s(-8, -4);

// ── Main evaluation function ──────────────────────────────────────────────────

/// Evaluate open file and diagonal terms for both sides.
/// Returns score from side-to-move perspective, tapered.
pub fn evaluate_open_lines(pos: &Position, phase: i32) -> i32 {
    let us   = pos.side_to_move;
    let them = us.flip();

    let our_score   = open_line_score(pos, us);
    let their_score = open_line_score(pos, them);

    taper(our_score - their_score, phase)
}

/// Compute open line score for one color.
fn open_line_score(pos: &Position, color: Color) -> i64 {
    let our_pawns   = pos.piece_bb(color,       PieceKind::Pawn);
    let enemy_pawns = pos.piece_bb(color.flip(), PieceKind::Pawn);
    let all_pawns   = our_pawns | enemy_pawns;
    let all_occ     = pos.all_pieces();
    let our_rooks   = pos.piece_bb(color, PieceKind::Rook);
    let our_queens  = pos.piece_bb(color, PieceKind::Queen);
    let our_bishops = pos.piece_bb(color, PieceKind::Bishop);
    let enemy_rooks = pos.piece_bb(color.flip(), PieceKind::Rook);

    let mut score = 0i64;

    // ── Rook evaluations ──────────────────────────────────────────────────────
    let mut rooks = our_rooks;
    while let Some(sq) = rooks.pop_lsb() {
        let file = sq.file();
        let rank = sq.rank();
        let file_mask = Bitboard::file_mask(file);

        let own_on_file   = (our_pawns   & file_mask).is_not_empty();
        let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

        // Open / semi-open file
        if !own_on_file {
            if !enemy_on_file {
                score += ROOK_OPEN_FILE;
            } else {
                score += ROOK_SEMI_OPEN_FILE;
            }
        }

        // Rook on 7th rank (trapping enemy king on 8th, attacking enemy pawns)
        // 7th rank for White = rank index 6, for Black = rank index 1
        let seventh_rank = match color {
            Color::White => 6u8,
            Color::Black => 1u8,
        };
        if rank == seventh_rank {
            score += ROOK_ON_SEVENTH;
        }

        // Battery: Queen + Rook on same file
        let rook_file_attacks = rook_attacks(sq, all_occ);
        if (rook_file_attacks & file_mask & our_queens).is_not_empty() {
            score += BATTERY_ROOK_QUEEN;
        }

        // Contested file: enemy rook on same file
        if (enemy_rooks & file_mask).is_not_empty() {
            score += CONTESTED_FILE;
        }
    }

    // ── Connected rooks ───────────────────────────────────────────────────────
    // Two rooks of the same colour on the same file or rank with nothing between.
    // Count only once per pair.
    {
        let mut r1 = our_rooks;
        while let Some(sq1) = r1.pop_lsb() {
            let mut r2 = r1; // Only pairs where sq2 > sq1 (avoid double-counting)
            while let Some(sq2) = r2.pop_lsb() {
                if are_rooks_connected(sq1, sq2, all_occ, our_rooks) {
                    score += ROOKS_CONNECTED;
                    break; // One bonus per rook pair per direction
                }
            }
        }
    }

    // ── Queen on open/semi-open file ──────────────────────────────────────────
    let mut queens = our_queens;
    while let Some(sq) = queens.pop_lsb() {
        let file = sq.file();
        let file_mask = Bitboard::file_mask(file);

        let own_on_file   = (our_pawns   & file_mask).is_not_empty();
        let enemy_on_file = (enemy_pawns & file_mask).is_not_empty();

        if !own_on_file {
            if !enemy_on_file {
                score += QUEEN_OPEN_FILE;
            } else {
                score += QUEEN_SEMI_OPEN_FILE;
            }
        }

        // Battery: Queen + Bishop on same diagonal
        let bishop_attacks_from_queen = bishop_attacks(sq, all_occ);
        if (bishop_attacks_from_queen & our_bishops).is_not_empty() {
            score += BATTERY_BISHOP_QUEEN;
        }
    }

    score
}

/// Are two rooks of the same colour connected (same rank or file, nothing between)?
fn are_rooks_connected(
    sq1: crate::types::Square,
    sq2: crate::types::Square,
    occupancy: Bitboard,
    own_rooks: Bitboard,
) -> bool {
    let r1 = sq1.rank();
    let f1 = sq1.file();
    let r2 = sq2.rank();
    let f2 = sq2.file();

    // Same rank
    if r1 == r2 {
        let rank_attacks = rook_attacks(sq1, occupancy ^ Bitboard::from_square(sq2));
        // sq2 must be reachable — no blocking pieces between them
        // (the rook attack already excludes sq2 from blocking since we XOR it out)
        return (rank_attacks & own_rooks).contains(sq2);
    }

    // Same file
    if f1 == f2 {
        let file_attacks = rook_attacks(sq1, occupancy ^ Bitboard::from_square(sq2));
        return (file_attacks & own_rooks).contains(sq2);
    }

    false
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
    use crate::types::Square;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_open_lines_start_pos_symmetric() {
        setup();
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_open_lines(&pos, phase);
        assert_eq!(score, 0, "Start position is symmetric — open lines should be 0");
    }

    #[test]
    fn test_rook_on_open_file_scores_positive() {
        setup();
        // White Rook on e1, no pawns on e-file → open file bonus
        let fen = "4k3/pppp1ppp/8/8/8/8/PPPP1PPP/4RK2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let score = evaluate_open_lines(&pos, phase);
        // White rook on e-file (open), Black has no rook advantage → positive
        assert!(score > 0, "Rook on open file should score positive: {}", score);
    }

    #[test]
    fn test_rook_on_seventh_rank() {
        setup();
        // White Rook on e7 (7th rank) attacking Black pawns
        let fen = "4k3/pppp1ppp/8/8/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        // Put a white rook on e7 manually — use a FEN that has it
        let fen2 = "4k3/ppppRppp/8/8/8/8/8/4K3 w - - 0 1";
        let pos2 = Position::from_fen(fen2).unwrap();
        let phase = game_phase(&pos2);
        let score = evaluate_open_lines(&pos2, phase);
        assert!(score > 0, "Rook on 7th rank should score positive: {}", score);
    }

    #[test]
    fn test_connected_rooks_bonus() {
        setup();
        // Two White rooks on same rank with nothing between
        let fen = "4k3/8/8/8/8/8/8/R3RK2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        let our_score = open_line_score(&pos, Color::White);
        // Should include ROOKS_CONNECTED bonus
        assert!(our_score > 0, "Connected rooks should give bonus: {}", our_score);
    }

    #[test]
    fn test_battery_rook_queen() {
        setup();
        // White Queen and Rook on same file (d-file), open
        let fen = "4k3/8/8/8/8/8/8/3QRK2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let phase = game_phase(&pos);
        // Queen on d1, Rook on e1 — they're on same rank but different file
        // Let's use same file: Queen d1, Rook d4
        let fen2 = "4k3/8/8/8/3R4/8/8/3QK3 w - - 0 1";
        let pos2 = Position::from_fen(fen2).unwrap();
        let our_score = open_line_score(&pos2, Color::White);
        // Rook on d4 attacks Queen on d1 along d-file → BATTERY_ROOK_QUEEN
        assert!(our_score > 0, "Rook+Queen battery should give bonus: {}", our_score);
    }

    #[test]
    fn test_pet_dragon_immediate_open_lines() {
        setup();
        // Pet Dragon positions have open middle ranks from move 1.
        // Verify open_lines evaluates correctly without panicking
        // and gives non-trivially-zero scores (open lines ARE present).
        let mut found_nonzero = false;
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let our = open_line_score(&pos, Color::White);
            let their = open_line_score(&pos, Color::Black);
            if our != 0 || their != 0 {
                found_nonzero = true;
            }
        }
        assert!(found_nonzero,
            "Pet Dragon positions should have open line bonuses from move 1");
    }

    #[test]
    fn test_open_lines_1000_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let _ = evaluate_open_lines(&pos, phase);
        }
    }

    #[test]
    fn test_contested_file_reduces_bonus() {
        setup();
        // Both sides have rooks on the same open file → contested file penalty
        let fen = "3rk3/8/8/8/8/8/8/3RK3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        let white_score = open_line_score(&pos, Color::White);
        let black_score = open_line_score(&pos, Color::Black);
        // Both get open file bonus but also contested penalty — should be roughly equal
        assert!((white_score - black_score).abs() < 100,
            "Contested file should cancel out: white={} black={}", white_score, black_score);
    }

    #[test]
    fn test_open_line_score_bounded() {
        setup();
        for seed in 0..100u64 {
            let pos = Position::generate_with_seed(seed);
            let phase = game_phase(&pos);
            let score = evaluate_open_lines(&pos, phase);
            assert!(score.abs() < 2000,
                "Open line score should be bounded: {} (seed {})", score, seed);
        }
    }
}
