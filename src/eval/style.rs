// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// eval/style.rs — PlayStyle additive evaluation bonus (D111)
//
// Adds a small, independently-computed bonus on top of the existing tuned
// evaluation, selected at runtime via the UCI `PlayStyle` spin option
// (0-4: Balanced/Killer/Tactical/Positional/Endgame). Search itself is
// completely untouched — same alpha-beta/PVS, same pruning, same TT — only
// the leaf evaluation is biased, so an unsound sacrifice in Killer mode is
// still refuted by deeper search exactly as it would be today.
//
// Balanced (mode 0, default) is a guaranteed no-op: returns 0 for any
// position and touches no other file's behavior, so the engine is
// byte-identical to pre-PlayStyle behavior whenever this option is left at
// its default.
//
// Deliberately NOT wired into `king_safety.rs`, `mobility.rs`, `pawns.rs`,
// or `open_lines.rs` — those Texel-tuned tables (Phase 25, Session 84,
// 62,125 positions) stay untouched by design (see playstyle-proposal.md
// §2 for the "edit tuned tables directly" alternative that was rejected).
// This file only reads public bitboard/position primitives already used
// elsewhere (`bishop_attacks`, `rook_attacks`, `queen_attacks`,
// `knight_attacks`, `king_attacks`, `pawn_attacks`) and never imports a
// private helper from another eval module — fully decoupled from the
// tuned core.
//
// ⚠️ All four mode constants below are HAND-PICKED STARTING POINTS, not
// yet Texel-tuned — unlike every constant in king_safety.rs/mobility.rs/
// pawns.rs/open_lines.rs. Once a mode has enough self-play games logged,
// its constants can go through the same Texel pipeline as a follow-up
// (playstyle-proposal.md §7).
// ============================================================================

use std::sync::atomic::{AtomicU32, Ordering};

use crate::bitboard::{bishop_attacks, queen_attacks, rook_attacks, Bitboard};
use crate::bitboard::masks::{king_attacks, knight_attacks, pawn_attacks};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

// ── PlayStyle mode selection ────────────────────────────────────────────────

pub const BALANCED: u32 = 0;
pub const KILLER: u32 = 1;
pub const TACTICAL: u32 = 2;
pub const POSITIONAL: u32 = 3;
pub const ENDGAME: u32 = 4;

/// Highest valid `PlayStyle` value — matches the UCI option's declared
/// `max 4` in `main.rs`.
pub const MAX_PLAY_STYLE: u32 = ENDGAME;

/// Runtime-configurable style selector (0-4). Follows the exact same
/// pattern as `eval::NNUE_BLEND_WEIGHT_PCT`: a bare `static AtomicU32`,
/// set directly from `main.rs`'s `"playstyle"` setoption arm at parse
/// time, no `EngineState` field, `Relaxed` ordering — PlayStyle only
/// affects the leaf eval (no `cmd_go`-time behavior to apply later), so
/// the same "one stale search node is fine" tradeoff already accepted
/// for NNUEWeight applies here too.
static PLAY_STYLE: AtomicU32 = AtomicU32::new(BALANCED);

/// Set the active PlayStyle (0-4). Out-of-range values are clamped rather
/// than rejected, matching the existing `Hash`/`NNUEWeight`/`Threads` UCI
/// option pattern in `main.rs`. Called from `setoption name PlayStyle
/// value <N>`.
pub fn set_play_style(mode: u32) {
    PLAY_STYLE.store(mode.min(MAX_PLAY_STYLE), Ordering::Relaxed);
}

/// Current PlayStyle mode (0-4).
pub fn play_style() -> u32 {
    PLAY_STYLE.load(Ordering::Relaxed)
}

/// Compute the style bonus for the current PlayStyle mode, in centipawns
/// from the side-to-move's perspective — added on top of
/// `evaluate_blended()` by `eval::evaluate_styled()`.
///
/// Balanced (or any unrecognized/out-of-range value, defensively) returns
/// exactly 0.
pub fn evaluate_style(pos: &Position, phase: i32) -> i32 {
    match play_style() {
        KILLER => killer_bonus(pos, phase),
        TACTICAL => tactical_bonus(pos, phase),
        POSITIONAL => positional_bonus(pos, phase),
        ENDGAME => endgame_bonus(pos, phase),
        _ => 0, // BALANCED, or any future/invalid value — no-op by design
    }
}

// ── Killer mode ──────────────────────────────────────────────────────────────
//
// Rewards attacker density near the enemy king, pawn storms, and piece
// proximity to the enemy king. Middlegame-weighted, fades out with phase —
// an all-out-attack bonus makes little sense in a king-and-pawn ending.
// Deliberately one-sided (attacks against THEIR king only): the mirror
// term — danger to OUR OWN king — is already scored by
// `king_safety.rs::evaluate_king_safety()` elsewhere in `evaluate()`, so
// adding a symmetric subtraction here would double-count that signal.

/// Nonlinear per-attacker-count bonus (hand-picked starting point,
/// deliberately steeper than the tuned `king_safety::ATTACKER_WEIGHT`
/// since this is a style bonus layered on top, not a replacement).
const KILLER_ATTACKER_BONUS: [i32; 8] = [0, 20, 55, 95, 130, 150, 160, 165];

/// Flat bonus per own pawn on the 3 files around the opponent king that
/// has advanced past rank 4 (White) / rank 5 (Black) — a storm proxy.
/// Hand-picked starting point.
const KILLER_STORM_BONUS_PER_PAWN: i32 = 15;

fn killer_bonus(pos: &Position, phase: i32) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let their_king_sq = pos.king_sq(them);
    let all_occ = pos.all_pieces();

    // Same one-liner king_safety.rs:145 builds — deliberately duplicated
    // rather than imported, keeping this file fully decoupled from the
    // tuned core (playstyle-proposal.md §4.1).
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

    let attacker_bonus = KILLER_ATTACKER_BONUS[attacker_count.min(7)];

    // Pawn storm: our pawns on the 3 files around their king, advanced
    // past the threshold rank for our color.
    let their_king_file = their_king_sq.file() as i32;
    let mut storm_pawns = 0i32;
    let mut our_pawns = pos.piece_bb(us, PieceKind::Pawn);
    while let Some(sq) = our_pawns.pop_lsb() {
        let file_diff = (sq.file() as i32 - their_king_file).abs();
        if file_diff > 1 {
            continue;
        }
        let advanced = if us == Color::White {
            sq.rank() >= 4 // past rank 4 (0-indexed: rank index 4 = rank 5)
        } else {
            sq.rank() <= 3 // past rank 5 (0-indexed: rank index 3 = rank 4)
        };
        if advanced {
            storm_pawns += 1;
        }
    }

    let raw = attacker_bonus + storm_pawns * KILLER_STORM_BONUS_PER_PAWN;
    raw * phase / 24
}

// ── Tactical mode ────────────────────────────────────────────────────────────
//
// A cheap proxy for initiative: net squares controlled in enemy territory.
// Deliberately modest — not a real threat detector, just a gentle nudge
// toward sharp, contact-heavy positions over quiet ones when the "real"
// eval terms are otherwise close. Middlegame-weighted, same phase scaling
// as Killer mode.

/// Flat bonus per net square of enemy-territory control (hand-picked
/// starting point, deliberately small — see module doc).
const TACTICAL_BONUS_PER_SQUARE: i32 = 3;

fn tactical_bonus(pos: &Position, phase: i32) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let occ = pos.all_pieces();

    // "Our" enemy territory (their half) vs "their" enemy territory (our
    // half) — net presence in each other's camp, not a one-sided count.
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

    let net = our_count - their_count;
    (net * TACTICAL_BONUS_PER_SQUARE) * phase / 24
}

// ── Positional mode ──────────────────────────────────────────────────────────
//
// Central/extended-central space control, net of the opponent. Flat
// bonus, no phase scaling — space matters throughout the game, unlike a
// king attack. Intentionally the smallest, least "opinionated" of the
// four non-Balanced modes, since positional understanding is already
// reasonably well covered by the tuned mobility.rs/pawns.rs terms.

/// Flat bonus per net controlled central square (hand-picked starting
/// point, small — this mode is meant to nudge, not override).
const POSITIONAL_BONUS_PER_SQUARE: i32 = 4;

fn positional_bonus(pos: &Position, _phase: i32) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();
    let occ = pos.all_pieces();

    // c3-f6 box: files C-F, ranks 3-6.
    let center_box = (Bitboard::FILE_C | Bitboard::FILE_D | Bitboard::FILE_E | Bitboard::FILE_F)
        & (Bitboard::RANK_3 | Bitboard::RANK_4 | Bitboard::RANK_5 | Bitboard::RANK_6);

    let our_count = (pawn_and_minor_attacks(pos, us, occ) & center_box).count() as i32;
    let their_count = (pawn_and_minor_attacks(pos, them, occ) & center_box).count() as i32;

    (our_count - their_count) * POSITIONAL_BONUS_PER_SQUARE
}

// ── Endgame mode ─────────────────────────────────────────────────────────────
//
// King centralization only for v1 — deliberately narrow scope. Passed-pawn
// urgency is NOT duplicated here: pawns.rs already has real passed-pawn
// detection, and re-implementing a cruder version in this file risks the
// two disagreeing. Weight is (24 - phase): zero in the opening, grows as
// material comes off — the mirror image of Killer/Tactical's scaling.

/// Bonus per unit of net Chebyshev-distance-to-center advantage
/// (hand-picked starting point).
const ENDGAME_BONUS_PER_UNIT: i32 = 6;

/// The four true center squares.
const CENTER_SQUARES: [Square; 4] = [Square::D4, Square::D5, Square::E4, Square::E5];

fn chebyshev_distance(a: Square, b: Square) -> i32 {
    let df = (a.file() as i32 - b.file() as i32).abs();
    let dr = (a.rank() as i32 - b.rank() as i32).abs();
    df.max(dr)
}

/// Minimum Chebyshev distance from `sq` to any of the four center squares.
fn distance_to_center(sq: Square) -> i32 {
    CENTER_SQUARES
        .iter()
        .map(|&c| chebyshev_distance(sq, c))
        .min()
        .unwrap_or(0)
}

fn endgame_bonus(pos: &Position, phase: i32) -> i32 {
    let us = pos.side_to_move;
    let them = us.flip();

    let our_dist = distance_to_center(pos.king_sq(us));
    let their_dist = distance_to_center(pos.king_sq(them));

    // Smaller distance is better, so a positive net means WE are more
    // central than THEM.
    let net = their_dist - our_dist;
    (net * ENDGAME_BONUS_PER_UNIT) * (24 - phase) / 24
}

// ── Shared helpers ───────────────────────────────────────────────────────────

/// Union of every square attacked by any of `color`'s pieces (pawns,
/// knights, bishops, rooks, queens, king). Used by Tactical mode's
/// enemy-territory-control proxy.
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
    attacks |= king_attacks(pos.king_sq(color));

    attacks
}

/// Union of every square attacked by `color`'s pawns and minor pieces
/// only (no rooks/queens/king) — used by Positional mode, which is
/// specifically about pawn/minor space control, not major-piece
/// influence (that's already `mobility.rs`'s territory).
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

    /// Guards the process-global `PLAY_STYLE` atomic against parallel
    /// test interleaving, same reasoning as eval/mod.rs's
    /// `test_nnue_weight_setter_getter_and_blend_at_zero`.
    fn reset_to_balanced() {
        set_play_style(BALANCED);
    }

    #[test]
    fn test_balanced_is_always_zero() {
        setup();
        reset_to_balanced();
        assert_eq!(play_style(), BALANCED);

        let positions = [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", // start
            "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/2N2N2/PPPP1PPP/R1BQK2R w KQkq - 0 1", // sharp MG
            "8/8/8/2k5/5K2/8/8/8 w - - 0 1", // king-only endgame
        ];
        for fen in positions {
            let pos = Position::from_fen(fen).unwrap();
            let phase = game_phase(&pos);
            assert_eq!(
                evaluate_style(&pos, phase),
                0,
                "Balanced must be a no-op for {fen}"
            );
        }
        reset_to_balanced();
    }

    #[test]
    fn test_out_of_range_play_style_treated_as_no_op() {
        setup();
        // Defensive default in evaluate_style's match — even though
        // set_play_style() itself clamps, a value that somehow bypassed
        // the setter should still fall through to 0, not panic or index
        // out of bounds.
        PLAY_STYLE.store(99, Ordering::Relaxed);
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        assert_eq!(evaluate_style(&pos, phase), 0);
        reset_to_balanced();
    }

    #[test]
    fn test_set_play_style_clamps_out_of_range() {
        setup();
        set_play_style(999);
        assert_eq!(play_style(), MAX_PLAY_STYLE, "should clamp to max (4)");
        reset_to_balanced();
        assert_eq!(play_style(), BALANCED);
    }

    #[test]
    fn test_killer_mode_rewards_pieces_massed_near_enemy_king() {
        setup();
        reset_to_balanced();
        // White queen and rook both bearing down on the black king's
        // zone (attacking f8 and h7, adjacent to the g8 king — not the
        // king square itself, so this stays a legal from_fen position)
        // vs the same material spread out far away.
        let attacking = "6k1/8/8/8/5R2/8/K7/1Q6 w - - 0 1";
        let spread_out = "6k1/8/8/8/8/8/8/R2QK3 w - - 0 1";

        set_play_style(KILLER);
        let pos_attack = Position::from_fen(attacking).unwrap();
        let phase_attack = game_phase(&pos_attack);
        let bonus_attack = evaluate_style(&pos_attack, phase_attack);

        let pos_spread = Position::from_fen(spread_out).unwrap();
        let phase_spread = game_phase(&pos_spread);
        let bonus_spread = evaluate_style(&pos_spread, phase_spread);

        assert!(
            bonus_attack > bonus_spread,
            "massed attackers near enemy king should score higher \
             ({bonus_attack} vs {bonus_spread})"
        );
        reset_to_balanced();
    }

    #[test]
    fn test_tactical_mode_rewards_enemy_camp_presence() {
        setup();
        reset_to_balanced();
        set_play_style(TACTICAL);
        // White knight deep in Black's camp vs a knight still at home.
        let advanced = "4k3/8/4N3/8/8/8/8/4K3 w - - 0 1";
        let at_home = "4k3/8/8/8/8/8/4N3/4K3 w - - 0 1";

        let pos_adv = Position::from_fen(advanced).unwrap();
        let phase_adv = game_phase(&pos_adv);
        let bonus_adv = evaluate_style(&pos_adv, phase_adv);

        let pos_home = Position::from_fen(at_home).unwrap();
        let phase_home = game_phase(&pos_home);
        let bonus_home = evaluate_style(&pos_home, phase_home);

        assert!(
            bonus_adv >= bonus_home,
            "knight advanced into enemy territory should score >= a knight \
             at home ({bonus_adv} vs {bonus_home})"
        );
        reset_to_balanced();
    }

    #[test]
    fn test_positional_mode_rewards_central_control() {
        setup();
        reset_to_balanced();
        set_play_style(POSITIONAL);
        // White knight on d4 (central) vs a knight on a1 (rim).
        let central = "4k3/8/8/8/3N4/8/8/4K3 w - - 0 1";
        let rim = "4k3/8/8/8/8/8/8/N3K3 w - - 0 1";

        let pos_c = Position::from_fen(central).unwrap();
        let phase_c = game_phase(&pos_c);
        let bonus_c = evaluate_style(&pos_c, phase_c);

        let pos_r = Position::from_fen(rim).unwrap();
        let phase_r = game_phase(&pos_r);
        let bonus_r = evaluate_style(&pos_r, phase_r);

        assert!(
            bonus_c > bonus_r,
            "central knight should score higher than a rim knight \
             ({bonus_c} vs {bonus_r})"
        );
        reset_to_balanced();
    }

    #[test]
    fn test_endgame_mode_rewards_king_centralization() {
        setup();
        reset_to_balanced();
        set_play_style(ENDGAME);
        // White king centralized (e4) vs cornered (a1); black king fixed
        // in a far corner (h8) both times so only White's centralization
        // changes between the two positions.
        let centralized = "7k/8/8/8/4K3/8/8/8 w - - 0 1";
        let cornered = "7k/8/8/8/8/8/8/K7 w - - 0 1";

        let pos_cen = Position::from_fen(centralized).unwrap();
        let phase_cen = game_phase(&pos_cen);
        let bonus_cen = evaluate_style(&pos_cen, phase_cen);

        let pos_cor = Position::from_fen(cornered).unwrap();
        let phase_cor = game_phase(&pos_cor);
        let bonus_cor = evaluate_style(&pos_cor, phase_cor);

        assert!(
            bonus_cen > bonus_cor,
            "centralized king should score higher than a cornered king \
             ({bonus_cen} vs {bonus_cor})"
        );
        reset_to_balanced();
    }

    #[test]
    fn test_endgame_mode_zero_weight_at_full_middlegame_phase() {
        setup();
        reset_to_balanced();
        set_play_style(ENDGAME);
        // Phase 24 (full material) — (24 - phase) = 0, so the bonus must
        // be exactly zero regardless of king position.
        let pos = Position::start_pos().unwrap();
        let phase = game_phase(&pos);
        assert_eq!(phase, 24, "start position should be phase 24");
        assert_eq!(evaluate_style(&pos, phase), 0);
        reset_to_balanced();
    }
}
