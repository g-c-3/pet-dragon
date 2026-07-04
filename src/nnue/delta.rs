// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// nnue/delta.rs — Incremental feature updates for make/unmake (Phase 16.3)
//
// Re-running features::extract_features() on every node would scan all 64
// squares per call — too slow for millions of nodes/sec. Instead, this
// module computes the *change* a single Move causes to the active-feature
// set, mirroring make_move()'s own match arms exactly (src/position/make_move.rs)
// so the two can never drift apart. The result is perspective-agnostic
// (board-space color/kind/square changes); rendering into perspective-
// specific feature indices happens separately via `render_for_perspective`,
// which is exactly the (added, removed) pair NORU's
// `FeatureDelta::from_slices()` expects for `Accumulator::update_incremental()`.
//
// IMPORTANT: `compute_move_changes` must be called BEFORE `Position::make_move()`
// mutates the position — it reads `pos.piece_on()` and `pos.pawn_starts` in
// their pre-move state (the same state make_move() itself reads from).
// Calling it after make_move() has already run will compute the wrong delta.
//
// This module does not yet touch search or maintain a live Accumulator on
// Position — that lands with evaluate() integration in Phase 16.6, once a
// trained network exists. For now this is the correctness-critical piece:
// a delta engine proven equivalent to full re-extraction (see tests below).
// ============================================================================

use crate::position::Position;
use crate::types::{Color, Move, MoveKind, PieceKind, Square};

use super::features::{pawn_start_feature_index, piece_feature_index};

/// One piece appearing or disappearing on a square, in board (not perspective) terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardFeatureChange {
    pub color: Color,
    pub kind: PieceKind,
    pub square: Square,
    /// true = piece is appearing on `square`, false = piece is leaving it.
    pub added: bool,
}

/// One pawn-start feature toggling, in board (not perspective) terms.
///
/// Emitted only when a pawn crosses onto/off of a square it actually started
/// on (per `PawnStartMap::started_here`) — see D11.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PawnStartFeatureChange {
    pub color: Color,
    pub square: Square,
    pub added: bool,
}

/// All feature-relevant changes caused by applying one `Move`.
#[derive(Debug, Clone, Default)]
pub struct MoveFeatureChanges {
    pub board: Vec<BoardFeatureChange>,
    pub pawn_start: Vec<PawnStartFeatureChange>,
}

#[inline]
fn remove_piece_changes(
    pos: &Position,
    color: Color,
    kind: PieceKind,
    square: Square,
    out: &mut MoveFeatureChanges,
) {
    out.board.push(BoardFeatureChange { color, kind, square, added: false });
    if kind == PieceKind::Pawn && pos.pawn_starts.started_here(square, color) {
        out.pawn_start.push(PawnStartFeatureChange { color, square, added: false });
    }
}

#[inline]
fn add_piece_changes(
    pos: &Position,
    color: Color,
    kind: PieceKind,
    square: Square,
    out: &mut MoveFeatureChanges,
) {
    out.board.push(BoardFeatureChange { color, kind, square, added: true });
    if kind == PieceKind::Pawn && pos.pawn_starts.started_here(square, color) {
        out.pawn_start.push(PawnStartFeatureChange { color, square, added: true });
    }
}

#[inline]
fn move_piece_changes(
    pos: &Position,
    color: Color,
    kind: PieceKind,
    from: Square,
    to: Square,
    out: &mut MoveFeatureChanges,
) {
    remove_piece_changes(pos, color, kind, from, out);
    add_piece_changes(pos, color, kind, to, out);
}

/// Compute the board-space feature changes for applying `mv` to `pos`.
///
/// Must be called with `pos` in its state immediately BEFORE `mv` is applied
/// (i.e. before `pos.make_move(mv)` runs) — mirrors every match arm of
/// `Position::make_move()` so the delta stays exactly in sync with it.
pub fn compute_move_changes(pos: &Position, mv: Move) -> MoveFeatureChanges {
    let color = pos.side_to_move;
    let from = mv.from;
    let to = mv.to;
    let mut out = MoveFeatureChanges::default();

    match mv.kind {
        MoveKind::Quiet => {
            let kind = pos.piece_on(from, color).expect("piece_on(from) in Quiet move");
            move_piece_changes(pos, color, kind, from, to, &mut out);
        }

        MoveKind::DoublePush => {
            move_piece_changes(pos, color, PieceKind::Pawn, from, to, &mut out);
        }

        MoveKind::Capture => {
            let captured = mv.captured.expect("Capture must have captured piece");
            remove_piece_changes(pos, color.flip(), captured, to, &mut out);
            let kind = pos.piece_on(from, color).expect("piece_on(from) in Capture move");
            move_piece_changes(pos, color, kind, from, to, &mut out);
        }

        MoveKind::EnPassant => {
            let captured_sq = Square::from_file_rank(to.file(), from.rank())
                .expect("En passant captured square must be valid");
            remove_piece_changes(pos, color.flip(), PieceKind::Pawn, captured_sq, &mut out);
            move_piece_changes(pos, color, PieceKind::Pawn, from, to, &mut out);
        }

        MoveKind::CastleKing => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square::H1, Square::F1),
                Color::Black => (Square::H8, Square::F8),
            };
            move_piece_changes(pos, color, PieceKind::King, from, to, &mut out);
            move_piece_changes(pos, color, PieceKind::Rook, rook_from, rook_to, &mut out);
        }

        MoveKind::CastleQueen => {
            let (rook_from, rook_to) = match color {
                Color::White => (Square::A1, Square::D1),
                Color::Black => (Square::A8, Square::D8),
            };
            move_piece_changes(pos, color, PieceKind::King, from, to, &mut out);
            move_piece_changes(pos, color, PieceKind::Rook, rook_from, rook_to, &mut out);
        }

        MoveKind::PromoQueen
        | MoveKind::PromoRook
        | MoveKind::PromoBishop
        | MoveKind::PromoKnight
        | MoveKind::PromoCapQueen
        | MoveKind::PromoCapRook
        | MoveKind::PromoCapBishop
        | MoveKind::PromoCapKnight => {
            let promo_kind = mv.kind.promotion_piece().expect("promotion move must have promo piece");
            if let Some(captured) = mv.captured {
                remove_piece_changes(pos, color.flip(), captured, to, &mut out);
            }
            // Pawn disappears from `from` (may drop a pawn-start feature, D11).
            remove_piece_changes(pos, color, PieceKind::Pawn, from, &mut out);
            // Promoted piece appears at `to` — never a pawn, so no pawn-start check.
            add_piece_changes(pos, color, promo_kind, to, &mut out);
        }
    }

    out
}

/// Render board-space changes into perspective-specific NORU feature deltas:
/// `(added_indices, removed_indices)`, ready for
/// `noru::network::FeatureDelta::from_slices(&added, &removed)`.
pub fn render_for_perspective(
    changes: &MoveFeatureChanges,
    perspective: Color,
) -> (Vec<usize>, Vec<usize>) {
    let mut added = Vec::with_capacity(changes.board.len() + changes.pawn_start.len());
    let mut removed = Vec::with_capacity(changes.board.len() + changes.pawn_start.len());

    for c in &changes.board {
        let idx = piece_feature_index(perspective, c.color, c.kind, c.square);
        if c.added {
            added.push(idx);
        } else {
            removed.push(idx);
        }
    }

    for c in &changes.pawn_start {
        let idx = pawn_start_feature_index(perspective, c.color, c.square);
        if c.added {
            added.push(idx);
        } else {
            removed.push(idx);
        }
    }

    (added, removed)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::movegen::generate_moves;
    use crate::nnue::features::extract_features;
    use crate::position::zobrist::init_zobrist;
    use crate::position::Position;
    use std::collections::BTreeSet;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    /// The correctness-critical test: applying a computed delta to the
    /// pre-move feature set must equal a full re-extraction of the
    /// post-move position, for both perspectives, across many Pet Dragon
    /// positions and move types (quiet, capture, en passant, castle, promo).
    #[test]
    fn test_incremental_delta_matches_full_extraction() {
        setup();
        for seed in 0..300u64 {
            let pos = Position::generate_with_seed(seed);
            let moves = generate_moves(&pos);

            for mv in moves.iter().take(6) {
                for &perspective in &Color::ALL {
                    let before: BTreeSet<usize> =
                        extract_features(&pos, perspective).into_iter().collect();

                    let changes = compute_move_changes(&pos, *mv);
                    let (added, removed) = render_for_perspective(&changes, perspective);

                    let mut predicted = before.clone();
                    for r in &removed {
                        predicted.remove(r);
                    }
                    for a in &added {
                        predicted.insert(*a);
                    }

                    let mut pos_after = pos.clone();
                    pos_after.make_move(*mv);
                    let actual: BTreeSet<usize> =
                        extract_features(&pos_after, perspective).into_iter().collect();

                    assert_eq!(
                        predicted, actual,
                        "delta mismatch: seed {}, move {}, perspective {:?}",
                        seed, mv, perspective
                    );
                }
            }
        }
    }

    #[test]
    fn test_quiet_king_move_no_pawn_start_changes() {
        setup();
        let pos = Position::start_pos().unwrap();
        // Not a legal opening move, but compute_move_changes only reads
        // piece_on/pawn_starts — fine for a unit test of the King arm.
        let mv = Move::new(Square::E1, Square::E2, MoveKind::Quiet);
        // e2 has a pawn on the standard start position, so piece_on(E1) is
        // what matters here, not the destination's prior contents.
        let changes = compute_move_changes(&pos, mv);
        assert!(changes.pawn_start.is_empty(), "King move must never touch pawn-start features");
        assert_eq!(changes.board.len(), 2, "one remove + one add for a simple move");
    }

    #[test]
    fn test_promotion_drops_pawn_start_feature() {
        setup();
        // Pet Dragon: a pawn starting on rank 1 promoting on move 1 isn't a
        // real game state, but this exercises the promo arm's pawn-start
        // handling in isolation using a constructed position.
        let fen = "4k3/8/8/8/8/8/8/P3K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        pos.pawn_starts.set(Square::A1, Color::White);
        let mv = Move::new(Square::A1, Square::A2, MoveKind::PromoQueen);
        let changes = compute_move_changes(&pos, mv);
        assert_eq!(changes.pawn_start.len(), 1);
        assert!(!changes.pawn_start[0].added, "pawn-start feature must be removed, not added");
    }
}
