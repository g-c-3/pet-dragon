// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// nnue/features.rs — NNUE feature index encoding (Phase 16.2)
//
// Feature layout per perspective (D10, 896 total):
//   [0..768)   piece-square: kind.index()*128 + relative_color*64 + relative_sq
//   [768..896) pawn-start:   768 + relative_color*64 + relative_sq
//
// "Perspective" flips the board for Black: each side sees its own pieces as
// relative_color 0 ("us") and the opponent's as relative_color 1 ("them"),
// and squares are rank-mirrored via Square::mirror_rank() when the
// perspective is Black. This lets a single network learn from White's and
// Black's point of view symmetrically instead of needing separate weights
// per side — matches NORU's stm_features / nstm_features training API.
//
// Pawn-start features (D11): a pawn-start feature is active only while the
// pawn is STILL on its actual starting square — the moment it makes its
// first move (to any destination) the feature drops to inactive. This is
// checked via PawnStartMap::started_here(), the exact same test move
// generation already uses for double-step eligibility, so the feature
// definition can never drift out of sync with the actual game rule.
// ============================================================================

use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

/// Number of standard piece-square features per perspective (6 kinds x 2 x 64).
pub const NUM_PIECE_SQUARE_FEATURES: usize = 768;

/// Number of Pet Dragon pawn-start features per perspective (2 x 64).
pub const NUM_PAWN_START_FEATURES: usize = 128;

/// Total NNUE input feature count per perspective (D10).
pub const NUM_FEATURES: usize = NUM_PIECE_SQUARE_FEATURES + NUM_PAWN_START_FEATURES;

/// Mirror a square for the Black perspective; identity for White.
///
/// Board features are always encoded relative to the perspective side, so
/// Black "sees" the board flipped rank-wise (e1 <-> e8, etc.) via the same
/// `mirror_rank()` Pet Dragon setup generation already uses.
#[inline]
fn relative_square(perspective: Color, square: Square) -> Square {
    match perspective {
        Color::White => square,
        Color::Black => square.mirror_rank(),
    }
}

/// Feature index for one piece, from the given perspective.
///
/// `relative_color` is 0 for the perspective's own pieces ("us") and 1 for
/// the opponent's ("them") — this is what lets one network serve both sides.
pub fn piece_feature_index(
    perspective: Color,
    piece_color: Color,
    piece_kind: PieceKind,
    square: Square,
) -> usize {
    let relative_color = if piece_color == perspective { 0 } else { 1 };
    let sq = relative_square(perspective, square);
    piece_kind.index() * 128 + relative_color * 64 + sq.index() as usize
}

/// Feature index for one pawn-start marker, from the given perspective.
///
/// Only call this for a pawn that is still on its actual starting square
/// (`pos.pawn_starts.started_here(square, pawn_color) == true`) —
/// `extract_features` enforces this so the feature always tracks D11's
/// convergence rule exactly.
pub fn pawn_start_feature_index(
    perspective: Color,
    pawn_color: Color,
    start_square: Square,
) -> usize {
    let relative_color = if pawn_color == perspective { 0 } else { 1 };
    let sq = relative_square(perspective, start_square);
    NUM_PIECE_SQUARE_FEATURES + relative_color * 64 + sq.index() as usize
}

/// Extract the full sparse active-feature list for a position, from one
/// perspective. Returned indices are sorted ascending and fit directly into
/// NORU's `TrainingSample::stm_features` / `nstm_features` fields — see
/// `extract_stm_nstm_features` for the paired convenience call.
pub fn extract_features(pos: &Position, perspective: Color) -> Vec<usize> {
    let mut features = Vec::with_capacity(40);

    for color in Color::ALL {
        for kind in PieceKind::ALL {
            let bb = pos.pieces[color.index()][kind.index()];
            for square in bb {
                features.push(piece_feature_index(perspective, color, kind, square));

                if kind == PieceKind::Pawn && pos.pawn_starts.started_here(square, color) {
                    features.push(pawn_start_feature_index(perspective, color, square));
                }
            }
        }
    }

    features.sort_unstable();
    features
}

/// Convenience wrapper: extract both perspectives at once, already matched
/// to NORU's training API (`stm_features` = side-to-move's own view,
/// `nstm_features` = the opponent's view of the same position).
pub fn extract_stm_nstm_features(pos: &Position) -> (Vec<usize>, Vec<usize>) {
    let stm = pos.side_to_move;
    let nstm = stm.flip();
    (extract_features(pos, stm), extract_features(pos, nstm))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;
    use crate::position::Position;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_feature_counts_match_d10() {
        assert_eq!(NUM_PIECE_SQUARE_FEATURES, 768);
        assert_eq!(NUM_PAWN_START_FEATURES, 128);
        assert_eq!(NUM_FEATURES, 896);
    }

    #[test]
    fn test_piece_feature_index_in_range() {
        for kind in PieceKind::ALL {
            for square in Square::all() {
                let idx = piece_feature_index(Color::White, Color::White, kind, square);
                assert!(idx < NUM_PIECE_SQUARE_FEATURES);
                let idx_black_view =
                    piece_feature_index(Color::Black, Color::White, kind, square);
                assert!(idx_black_view < NUM_PIECE_SQUARE_FEATURES);
            }
        }
    }

    #[test]
    fn test_pawn_start_feature_index_in_range() {
        for square in Square::all() {
            let idx = pawn_start_feature_index(Color::White, Color::White, square);
            assert!(idx >= NUM_PIECE_SQUARE_FEATURES && idx < NUM_FEATURES);
        }
    }

    #[test]
    fn test_own_vs_opponent_relative_color_differs() {
        // Same physical piece, opposite perspectives -> different half of
        // the feature space, so the indices must differ.
        let idx_us =
            piece_feature_index(Color::White, Color::White, PieceKind::Queen, Square::D1);
        let idx_them =
            piece_feature_index(Color::Black, Color::White, PieceKind::Queen, Square::D1);
        assert_ne!(idx_us, idx_them);
    }

    #[test]
    fn test_symmetric_start_pos_mirrors_between_perspectives() {
        setup();
        let pos = Position::start_pos().unwrap();
        let white_features = extract_features(&pos, Color::White);
        let black_features = extract_features(&pos, Color::Black);
        // Standard start is White/Black symmetric under rank-mirroring, so
        // both perspectives must see the same *count* of active features.
        assert_eq!(white_features.len(), black_features.len());
    }

    #[test]
    fn test_pawn_start_feature_present_for_unmoved_pawns() {
        setup();
        let pos = Position::start_pos().unwrap();
        // Standard start: every pawn is still on its starting square, so all
        // 16 pawns contribute a pawn-start feature for White's perspective.
        let features = extract_features(&pos, Color::White);
        let pawn_start_count = features
            .iter()
            .filter(|&&idx| idx >= NUM_PIECE_SQUARE_FEATURES)
            .count();
        assert_eq!(pawn_start_count, 16, "all 16 pawns still on start squares");
    }

    #[test]
    fn test_pawn_start_feature_drops_once_record_cleared() {
        setup();
        // Simulates a pawn that has moved off its starting square: its
        // PawnStartMap entry is cleared (exactly what happens once a pawn's
        // current square != its recorded start square). D11's core
        // distinction is that this feature must vanish even though the
        // piece-square feature for the pawn's new square is unaffected.
        let mut pos = Position::start_pos().unwrap();
        pos.pawn_starts.0[Square::E2.index() as usize] = None;

        let features = extract_features(&pos, Color::White);
        let e2_pawn_start_idx =
            pawn_start_feature_index(Color::White, Color::White, Square::E2);
        assert!(!features.contains(&e2_pawn_start_idx));
    }

    #[test]
    fn test_1000_pet_dragon_positions_no_panic() {
        setup();
        for seed in 0..1000u64 {
            let pos = Position::generate_with_seed(seed);
            let (stm, nstm) = extract_stm_nstm_features(&pos);
            assert!(stm.len() <= NUM_FEATURES);
            assert!(nstm.len() <= NUM_FEATURES);
        }
    }
}
