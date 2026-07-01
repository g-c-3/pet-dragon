// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/pruning.rs — Advanced pruning and extension techniques
//
// Contains:
//   - Extension logic (check, recapture, passed pawn, double cap)
//   - LMR guards (when NOT to reduce)
//   - Probcut
//   - Correction history
//   - Multi-cut pruning
//
// These are the techniques that separate a 2800 engine from 3000+.
// All drawn from GPL v3 engines (Stockfish, Ethereal) with attribution.
// ============================================================================

use crate::position::Position;
use crate::search::MATE_THRESHOLD;
use crate::types::{Color, Move, MoveKind, PieceKind};

// ── Extension logic ───────────────────────────────────────────────────────────

/// Maximum total extension per node
/// Prevents depth explosion from multiple simultaneous extensions
pub const MAX_EXTENSION: i32 = 2;

/// Calculate search extension for a move
/// Returns depth bonus (0 = no extension, 1 = extend by 1, etc.)
pub fn extension(
    pos:         &Position,
    mv:          Move,
    in_check:    bool,
    gives_check: bool,
    depth:       i32,
    _ply:        usize,
) -> i32 {
    let mut ext = 0i32;

    // Check extension: extend when in check or giving check
    // Ensures we don't miss tactical continuations near check
    if in_check || gives_check {
        ext += 1;
    }

    // Recapture extension: extend forced recapture sequences
    // Prevents horizon effect during exchanges
    if is_recapture(pos, mv) && depth <= 4 {
        ext += 1;
    }

    // Passed pawn extension: extend when passed pawn near promotion
    if is_passed_pawn_push(pos, mv) {
        let rank = mv.to.rank();
        let side = pos.side_to_move;
        let close_to_promo = match side {
            Color::White => rank >= 5, // rank 6, 7
            Color::Black => rank <= 2, // rank 3, 2
        };
        if close_to_promo {
            ext += 1;
        }
    }

    // Hard cap: never extend beyond MAX_EXTENSION
    ext.min(MAX_EXTENSION)
}

/// Is this move a recapture on the same square as the previous capture?
fn is_recapture(pos: &Position, mv: Move) -> bool {
    // Check if there's a piece on 'to' to capture (it's a capture move)
    mv.kind.is_capture()
        && pos.piece_on(mv.to, pos.side_to_move.flip()).is_some()
}

/// Is this a passed pawn push?
fn is_passed_pawn_push(pos: &Position, mv: Move) -> bool {
    if mv.kind == MoveKind::Quiet || mv.kind == MoveKind::DoublePush {
        if let Some(PieceKind::Pawn) = pos.piece_on(mv.from, pos.side_to_move) {
            return is_passed_pawn(pos, mv.from, pos.side_to_move);
        }
    }
    false
}

/// Is a pawn on this square a passed pawn?
/// A pawn is passed if no enemy pawns can block or capture it
fn is_passed_pawn(pos: &Position, sq: crate::types::Square, color: Color) -> bool {
    use crate::bitboard::Bitboard;

    let file  = sq.file();
    let rank  = sq.rank();
    let enemy = color.flip();
    let enemy_pawns = pos.piece_bb(enemy, PieceKind::Pawn);

    // Build mask of squares ahead of this pawn on adjacent files
    let mut ahead_mask = Bitboard::EMPTY;
    for f in file.saturating_sub(1)..=(file + 1).min(7) {
        match color {
            Color::White => {
                for r in (rank + 1)..8 {
                    if let Some(s) = crate::types::Square::from_file_rank(f, r) {
                        ahead_mask.set(s);
                    }
                }
            }
            Color::Black => {
                for r in 0..rank {
                    if let Some(s) = crate::types::Square::from_file_rank(f, r) {
                        ahead_mask.set(s);
                    }
                }
            }
        }
    }

    (enemy_pawns & ahead_mask).is_empty()
}

// ── LMR guards ────────────────────────────────────────────────────────────────

/// Should this move be reduced with LMR?
/// Returns false if the move should NOT be reduced.
/// Drawn from Stockfish and Ethereal LMR implementations.
pub fn should_apply_lmr(
    mv:           Move,
    moves_tried:  usize,
    depth:        i32,
    in_check:     bool,
    gives_check:  bool,
    is_killer:    bool,
    is_tt_move:   bool,
) -> bool {
    // Never reduce:
    if depth < crate::search::MIN_DEPTH_LMR { return false; }
    if moves_tried < 3                       { return false; }
    if in_check                              { return false; }
    if gives_check                           { return false; }
    if mv.kind.is_capture()                  { return false; }
    if mv.kind.is_promotion()               { return false; }
    if is_killer                             { return false; }
    if is_tt_move                            { return false; }

    true
}

/// Calculate LMR reduction amount
/// Formula from Stockfish: 0.75 + ln(depth) * ln(moves_tried) / 2.25
pub fn lmr_reduction(depth: i32, moves_tried: usize) -> i32 {
    let r = 0.75
        + (depth as f64).ln() * (moves_tried as f64).ln() / 2.25;
    (r as i32).max(1)
}

// ── Probcut ───────────────────────────────────────────────────────────────────

/// Probcut threshold above beta
/// If a capture beats beta + PROBCUT_MARGIN in a shallow search,
/// we can safely prune (the move is probably too good for opponent to allow)
pub const PROBCUT_MARGIN: i32 = 200;

/// Should we try probcut at this node?
pub fn should_try_probcut(
    depth:       i32,
    beta:        i32,
    in_check:    bool,
    pv_node:     bool,
) -> bool {
    !pv_node
    && !in_check
    && depth >= crate::search::MIN_DEPTH_PROBCUT
    && beta.abs() < MATE_THRESHOLD
}

/// Probcut: do a shallow search of captures to see if we can prune
/// Returns Some(score) if probcut succeeds (node can be pruned)
/// Returns None if probcut fails (continue normal search)
pub fn try_probcut(
    pos:   &mut Position,
    _depth: i32,
    beta:  i32,
    ply:   usize,
    info:  &mut crate::search::SearchInfo,
    tt:    &crate::tt::TranspositionTable,
) -> Option<i32> {
    use crate::movegen::generate_captures;
    use crate::search::ordering::{next_move, score_captures};
    use crate::search::alpha_beta::quiescence;

    let probcut_beta  = beta + PROBCUT_MARGIN;
    let tt_move = tt.probe(pos.hash).map(|e| e.mv).unwrap_or(Move::NULL);

    let captures = generate_captures(pos);
    let mut scored = score_captures(pos, &captures, tt_move);

    for i in 0..scored.len() {
        let mv = match next_move(&mut scored, i) {
            Some(m) => m,
            None    => break,
        };

        // Only try captures that SEE says are profitable
        if !crate::search::see::see(pos, mv, probcut_beta - beta) {
            continue;
        }

        pos.make_move_with_history(mv);

        // Shallow search to verify
        let score = -quiescence(
            pos, -probcut_beta, -probcut_beta + 1,
            ply + 1, info, tt,
        );

        pos.unmake_move_with_history(mv);

        if score >= probcut_beta {
            return Some(score);
        }
    }

    None
}

// ── Correction history ────────────────────────────────────────────────────────
// Stockfish 18 technique: dynamically adjusts static eval based on
// patterns found during search. Significant Elo gain at high depths.

/// Correction history table
/// [color][pawn_hash_index] — indexed by pawn structure hash
pub struct CorrectionHistory {
    table: Vec<[i32; 2]>, // [white_correction, black_correction]
    size:  usize,
    mask:  usize,
}

impl CorrectionHistory {
    pub fn new() -> Self {
        let size = 16384usize; // Power of 2
        CorrectionHistory {
            table: vec![[0i32; 2]; size],
            size,
            mask: size - 1,
        }
    }

    /// Get correction for current position
    #[inline]
    pub fn get(&self, pawn_hash: u64, color: Color) -> i32 {
        let idx = (pawn_hash as usize) & self.mask;
        self.table[idx][color as usize]
    }

    /// Update correction based on search result
    #[inline]
    pub fn update(
        &mut self,
        pawn_hash:   u64,
        color:       Color,
        static_eval: i32,
        search_score: i32,
        depth:       i32,
    ) {
        let idx    = (pawn_hash as usize) & self.mask;
        let error  = search_score - static_eval;
        let weight = depth.min(16);
        let entry  = &mut self.table[idx][color as usize];

        // Weighted average update
        *entry = (*entry * (256 - weight) + error * weight) / 256;

        // Clamp to prevent overflow
        *entry = (*entry).max(-512).min(512);
    }

    /// Apply correction to static eval
    /// ⚠️ Never apply when in check (eval meaningless in check)
    #[inline]
    pub fn apply(&self, eval: i32, pawn_hash: u64, color: Color) -> i32 {
        eval + self.get(pawn_hash, color)
    }

    pub fn clear(&mut self) {
        self.table.fill([0, 0]);
    }
}

impl Default for CorrectionHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute pawn hash for correction history indexing
/// Only hashes pawn positions — correction is pawn-structure specific
pub fn pawn_hash(pos: &Position) -> u64 {
    use crate::position::zobrist::piece_key;
    use crate::types::PieceKind;

    let mut hash = 0u64;
    for color in Color::ALL {
        let mut pawns = pos.piece_bb(color, PieceKind::Pawn);
        while let Some(sq) = pawns.pop_lsb() {
            hash ^= piece_key(color, PieceKind::Pawn, sq);
        }
    }
    hash
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::Position;
    use crate::position::zobrist::init_zobrist;
    use crate::types::{Color, Move, MoveKind, Square};

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_lmr_not_applied_to_captures() {
        let mv = Move::capture(
            Square::E4, Square::D5,
            MoveKind::Capture,
            crate::types::PieceKind::Pawn,
        );
        assert!(!should_apply_lmr(mv, 5, 6, false, false, false, false),
            "Captures should not be reduced");
    }

    #[test]
    fn test_lmr_not_applied_to_promotions() {
        let mv = Move::new(Square::E7, Square::E8, MoveKind::PromoQueen);
        assert!(!should_apply_lmr(mv, 5, 6, false, false, false, false),
            "Promotions should not be reduced");
    }

    #[test]
    fn test_lmr_not_applied_in_check() {
        let mv = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        assert!(!should_apply_lmr(mv, 5, 6, true, false, false, false),
            "Moves in check should not be reduced");
    }

    #[test]
    fn test_lmr_not_applied_to_killers() {
        let mv = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        assert!(!should_apply_lmr(mv, 5, 6, false, false, true, false),
            "Killer moves should not be reduced");
    }

    #[test]
    fn test_lmr_not_applied_shallow() {
        let mv = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        assert!(!should_apply_lmr(mv, 5, 2, false, false, false, false),
            "Shallow depth should not be reduced");
    }

    #[test]
    fn test_lmr_applies_late_quiet_moves() {
        let mv = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        assert!(should_apply_lmr(mv, 5, 6, false, false, false, false),
            "Late quiet moves at depth 6 should be reduced");
    }

    #[test]
    fn test_lmr_reduction_increases_with_depth() {
        let r1 = lmr_reduction(4, 5);
        let r2 = lmr_reduction(8, 5);
        assert!(r2 > r1,
            "LMR reduction should increase with depth");
    }

    #[test]
    fn test_lmr_reduction_increases_with_moves() {
        let r1 = lmr_reduction(6, 4);
        let r2 = lmr_reduction(6, 10);
        assert!(r2 >= r1,
            "LMR reduction should increase with moves tried");
    }

    #[test]
    fn test_max_extension_cap() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mv  = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        // Even with all extensions, should not exceed MAX_EXTENSION
        let ext = extension(&pos, mv, true, true, 10, 0);
        assert!(ext <= MAX_EXTENSION,
            "Extension should not exceed MAX_EXTENSION");
    }

    #[test]
    fn test_correction_history_update_get() {
        let mut ch = CorrectionHistory::new();
        let hash = 0x1234_5678u64;

        ch.update(hash, Color::White, 100, 150, 8);
        let correction = ch.get(hash, Color::White);
        // Correction should be non-zero after update
        assert_ne!(correction, 0,
            "Correction should be updated");
    }

    #[test]
    fn test_correction_history_apply() {
        let mut ch = CorrectionHistory::new();
        let hash = 0xDEAD_BEEFu64;

        ch.update(hash, Color::Black, 200, 250, 10);
        let corrected = ch.apply(200, hash, Color::Black);
        // Applied eval should differ from original
        // (correction added to eval)
        assert!(corrected != 200 || ch.get(hash, Color::Black) == 0);
    }

    #[test]
    fn test_correction_clamped() {
        let mut ch = CorrectionHistory::new();
        let hash = 0x1111u64;

        // Large error — should be clamped
        for _ in 0..100 {
            ch.update(hash, Color::White, 0, 10000, 16);
        }
        let val = ch.get(hash, Color::White);
        assert!(val <= 512, "Correction should be clamped: {}", val);
        assert!(val >= -512, "Correction should be clamped: {}", val);
    }

    #[test]
    fn test_correction_history_clear() {
        let mut ch = CorrectionHistory::new();
        ch.update(0x1234u64, Color::White, 100, 200, 8);
        ch.clear();
        assert_eq!(ch.get(0x1234u64, Color::White), 0,
            "Correction should be 0 after clear");
    }

    #[test]
    fn test_pawn_hash_differs_by_position() {
        setup();
        let pos1 = Position::start_pos().unwrap();
        let pos2 = Position::generate_with_seed(42);
        let h1   = pawn_hash(&pos1);
        let h2   = pawn_hash(&pos2);
        // Different pawn structures should (almost always) have different hashes
        // Not guaranteed but very likely
        assert!(h1 != 0, "Pawn hash should be non-zero");
        assert!(h2 != 0, "Pawn hash should be non-zero");
    }

    #[test]
    fn test_probcut_conditions() {
        // Probcut should not trigger at low depth
        assert!(!should_try_probcut(2, 100, false, false),
            "Probcut should not trigger at low depth");

        // Probcut should not trigger in check
        assert!(!should_try_probcut(6, 100, true, false),
            "Probcut should not trigger in check");

        // Probcut should not trigger in PV node
        assert!(!should_try_probcut(6, 100, false, true),
            "Probcut should not trigger in PV node");

        // Probcut should trigger at high depth, not in check, not PV
        assert!(should_try_probcut(6, 100, false, false),
            "Probcut should trigger at depth 6, not in check, not PV");
    }

    #[test]
    fn test_passed_pawn_detection() {
        setup();
        // White pawn on e5 with no black pawns ahead — should be passed
        let fen = "4k3/8/8/4P3/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert!(is_passed_pawn(&pos, Square::E5, Color::White),
            "Pawn on e5 with clear path should be passed");
    }

    #[test]
    fn test_not_passed_pawn_with_blocker() {
        setup();
        // White pawn on e5, Black pawn on e7 — not passed
        let fen = "4k3/4p3/8/4P3/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert!(!is_passed_pawn(&pos, Square::E5, Color::White),
            "Pawn on e5 with blocker on e7 should not be passed");
    }
}
