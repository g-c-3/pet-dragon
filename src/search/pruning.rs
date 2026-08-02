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
///
/// D117 (Session 119): this whole function is dead code in the live
/// search — nothing in `alpha_beta.rs` calls it (its only caller,
/// before this session, was its own unit test below). `alpha_beta.rs`
/// has its own separate, correct, always-live check-extension
/// (`if in_check { depth += 1; }`, applied once per node rather than
/// per move here). This function still exists and is now internally
/// correct (see `is_recapture`'s fix below) as a coherent reference
/// implementation and test target, but recapture/passed-pawn-push
/// extension only actually runs in real games via
/// `recapture_and_passed_pawn_extension()` below, called directly from
/// `alpha_beta.rs`'s move loop, gated behind
/// `SearchInfo::recapture_extension_enabled` (default `false`) —
/// deliberately scoped to recapture only for now (review finding #3),
/// not passed-pawn-push, which was never flagged as buggy and stays
/// dead until/unless a separate decision activates it too.
pub fn extension(
    pos:         &Position,
    mv:          Move,
    prev_move:   Move,
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

    // Recapture + passed-pawn-push extension — see
    // `recapture_and_passed_pawn_extension()`'s own doc comment.
    ext += recapture_and_passed_pawn_extension(pos, mv, prev_move, depth);

    // Hard cap: never extend beyond MAX_EXTENSION
    ext.min(MAX_EXTENSION)
}

/// Recapture + passed-pawn-push extension (D117, Session 119). +1 if
/// `mv` is a genuine recapture (see `is_recapture`) at shallow
/// remaining depth (`depth <= 4`); independently, +1 if `mv` is a
/// passed pawn pushing to within 2 ranks of promotion. The two are
/// additive (a move can be both), same as the original bundled
/// `extension()` always intended.
///
/// Called directly from `alpha_beta.rs`'s move loop (gated behind
/// `SearchInfo::recapture_extension_enabled`) as well as from
/// `extension()` above — pulled out to its own function so both call
/// sites share one implementation instead of the live path duplicating
/// `extension()`'s recapture/passed-pawn logic a second time the way
/// `should_apply_lmr()` vs. the inline LMR gate already do (review
/// finding #4 — deliberately not repeating that pattern here).
/// Doesn't apply `MAX_EXTENSION` itself — callers combine this with
/// whatever other extension a move already has (e.g. the TT move's own
/// singular-extension result) and cap the total themselves.
pub fn recapture_and_passed_pawn_extension(
    pos:       &Position,
    mv:        Move,
    prev_move: Move,
    depth:     i32,
) -> i32 {
    let mut ext = 0i32;

    if is_recapture(mv, prev_move) && depth <= 4 {
        ext += 1;
    }

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

    ext
}

/// Is this move a recapture — does it capture on the exact same square
/// the opponent's immediately preceding move captured on?
///
/// D117 (Session 119) fix (review finding #3): the pre-fix version
/// only checked "is `mv` a capture, and is there currently an enemy
/// piece on its destination square" — it never looked at `prev_move`
/// at all, so it fired on essentially every capture, not genuine
/// recaptures specifically. Fixed to the standard definition: both
/// `mv` and `prev_move` must be captures, and they must land on the
/// same square. Same simplification Stockfish itself uses for its own
/// recapture check (`to_sq(m) == to_sq((ss-1)->currentMove)`) — this
/// doesn't special-case en passant's one-rank offset between the
/// captured pawn's square and the capturing pawn's destination square,
/// a known, accepted minor imprecision in that convention rather than
/// something unique to this fix.
fn is_recapture(mv: Move, prev_move: Move) -> bool {
    mv.kind.is_capture()
        && prev_move.kind.is_capture()
        && mv.to == prev_move.to
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

/// Per-thread base offset for the LMR formula's constant term (Phase 23.2
/// / D49 — thread-differentiated Lazy SMP). Before this, every Lazy SMP
/// helper thread (`main.rs`) ran the exact same LMR aggressiveness as the
/// main thread, differing only in start timing — so, combined with a
/// shared TT and near-identical move ordering, helpers spent most of
/// their time re-deriving lines the main thread was already finding
/// rather than covering genuinely different tree regions.
///
/// `thread_id == 0` (the main thread — the only thread whose result is
/// ever reported to the GUI, see the Phase 19 MultiPV note above
/// `iterative_deepening()`'s call site in `main.rs::cmd_go`) always
/// returns exactly the original Stockfish-derived constant, `0.75` — this
/// function cannot alter single-threaded search behavior, or the main
/// thread's own line in a multi-threaded search, at all. Only helper
/// threads' internal exploration changes.
///
/// Offsets cycle through a small fixed table rather than scaling linearly
/// with `thread_id`: Lazy SMP gets diminishing (and eventually negative)
/// returns from monotonically increasing reduction aggressiveness across
/// many threads, so a handful of distinct "personalities" repeating is
/// enough to decorrelate helpers from each other and from the main
/// thread, without any of them reducing so aggressively they stop
/// contributing useful transposition-table entries.
const LMR_THREAD_BASE_OFFSETS: [f64; 4] = [0.75, 0.45, 1.05, 0.60];

pub fn lmr_thread_base(thread_id: usize) -> f64 {
    LMR_THREAD_BASE_OFFSETS[thread_id % LMR_THREAD_BASE_OFFSETS.len()]
}

// ── Late Move Pruning (LMP) ────────────────────────────────────────────────────
// D60 — distinct from LMR: LMR still searches a late quiet move, just at
// reduced depth, so a buried tactic can still be found via re-search.
// LMP instead skips the move outright once enough quiet moves have
// already been tried at this node without raising alpha — accepting
// that moves ordered this late this deep essentially never turn out to
// matter. Stockfish/Ethereal-family technique.
//
// D114 (Session 116): Ethereal and Stockfish both differentiate the
// threshold by an "improving" flag (is static eval better than it was
// two plies ago for this side) — from D60/Session 82 through D113,
// Pet Dragon's alpha_beta didn't track that flag at all, so a single
// table (the values now called `LMP_THRESHOLDS_IMPROVING` below) was
// applied uniformly at every node as a deliberately conservative
// choice: using the higher (less-pruning) "improving" values
// everywhere never prunes *more* than a real improving-aware split
// would, only ever less. D114 adds the real split, gated behind
// `SearchInfo::improving_enabled` (default `false` — byte-identical to
// the pre-D114 uniform-table behavior until explicitly enabled and A/B
// validated, same rollout discipline as `null_move_king_guard`/D75).
const LMP_THRESHOLDS_IMPROVING: [usize; (crate::search::MAX_DEPTH_LMP + 1) as usize] = [
    0, 3, 4, 6, 9, 12, 16, 20, 25,
];

/// Non-improving LMP thresholds — used only when `improving_enabled` is
/// true AND the position is not improving (static eval no better than
/// two plies ago for this side, or unknown/in-check). Roughly half of
/// `LMP_THRESHOLDS_IMPROVING`, matching the halving relationship
/// Stockfish uses between its own improving/non-improving move-count
/// pruning thresholds: when the position isn't getting better, trust
/// the shallow-depth signal more and skip late quiets more
/// aggressively. Hand-picked starting values (not Texel-tuned — this
/// is a search pruning constant, not an eval weight, same status
/// `LMP_THRESHOLDS_IMPROVING` itself has always had), needs its own
/// SPRT-style A/B via `uci_match_runner` before any default-on
/// consideration, same as `improving_enabled` itself.
const LMP_THRESHOLDS_NON_IMPROVING: [usize; (crate::search::MAX_DEPTH_LMP + 1) as usize] = [
    0, 1, 2, 3, 4, 6, 8, 10, 12,
];

/// Quiet-move-count threshold for this depth, beyond which LMP prunes.
/// Depth is clamped into the table's range — callers should already be
/// gating on `depth <= MAX_DEPTH_LMP` via `should_apply_lmp`, this is
/// just a defensive clamp so an out-of-range depth can't panic.
/// `improving` selects which of the two D114 tables applies — pass
/// `true` unconditionally to reproduce pre-D114 behavior exactly (see
/// `LMP_THRESHOLDS_IMPROVING`'s doc comment).
pub fn lmp_threshold(depth: i32, improving: bool) -> usize {
    let table = if improving { &LMP_THRESHOLDS_IMPROVING } else { &LMP_THRESHOLDS_NON_IMPROVING };
    table[depth.clamp(0, crate::search::MAX_DEPTH_LMP) as usize]
}

/// Should this quiet move be skipped outright (not just reduced) by LMP?
/// Mirrors the existing futility-pruning guard style in `alpha_beta.rs`:
/// non-PV only, never in check or on a checking move, never near a mate
/// score (mate lines need full accuracy, not late-move pruning).
#[allow(clippy::too_many_arguments)]
pub fn should_apply_lmp(
    depth:       i32,
    moves_tried: usize,
    is_quiet:    bool,
    in_check:    bool,
    gives_check: bool,
    pv_node:     bool,
    alpha:       i32,
    beta:        i32,
    improving:   bool,
) -> bool {
    if pv_node                                  { return false; }
    if in_check || gives_check                  { return false; }
    if !is_quiet                                { return false; }
    if depth < 1 || depth > crate::search::MAX_DEPTH_LMP { return false; }
    if alpha.abs() >= MATE_THRESHOLD || beta.abs() >= MATE_THRESHOLD {
        return false;
    }
    moves_tried >= lmp_threshold(depth, improving)
}

// ── Futility pruning margin (D114, Session 116) ────────────────────────────────
// Base formula (`100 * depth + 200`) is unchanged from pre-D114 —
// extracted into its own function so `alpha_beta.rs`'s call site and
// this module's unit tests share one source of truth, and so the new
// improving-aware branch below has somewhere to live without
// duplicating the condition inline. When `improving` is true, this is
// byte-identical to the pre-D114 formula; when false, the constant
// term drops from 200 to 100 — a smaller margin makes the skip
// condition (`static_eval + margin <= alpha`) easier to satisfy, i.e.
// prunes more aggressively, same "trust the eval more when the
// position isn't getting better" reasoning as the LMP split above.
// Hand-picked, not Texel-tuned (search constant, not an eval weight);
// needs its own SPRT-style A/B, same as `improving_enabled` itself.
pub fn futility_margin(depth: i32, improving: bool) -> i32 {
    if improving {
        100 * depth + 200
    } else {
        100 * depth + 100
    }
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

        // Shallow search to verify — captures only (qs_depth = -1)
        // Probcut doesn't benefit from quiet check generation overhead
        let score = -quiescence(
            pos, -probcut_beta, -probcut_beta + 1,
            ply + 1, -1, info, tt,
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
#[derive(Clone)]
pub struct CorrectionHistory {
    table: Vec<[i32; 2]>, // [white_correction, black_correction]
    mask:  usize,
}

impl CorrectionHistory {
    pub fn new() -> Self {
        let size = 16384usize; // Power of 2
        CorrectionHistory {
            table: vec![[0i32; 2]; size],
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

/// Compute the singular-extension margin reduction for a given
/// correction-history magnitude (ROADMAP Phase 26 item 3c, D89).
///
/// Base singular margin is 2 (Phase 13.3/D59's original, unconditional
/// value). This returns how much to subtract from it — 0 when
/// `corr_mag` is small, up to a cap of 1 (never fully collapsing the
/// margin to 0, which would make `singular_beta == tt_score`,
/// degenerate) once `corr_mag` crosses `CORRECTION_EXTENSION_SCALE`.
/// `corr_mag` is expected to be the unsigned magnitude of a
/// `CorrectionHistory` entry (itself clamped to `[-512, 512]` — see
/// `CorrectionHistory::update`), so the full -1 reduction requires a
/// substantial, real systematic eval error at this position, not
/// noise.
#[inline]
pub fn singular_margin_reduction(corr_mag: i32) -> i32 {
    const CORRECTION_EXTENSION_SCALE: i32 = 300;
    (corr_mag / CORRECTION_EXTENSION_SCALE).min(1)
}

/// Compute pawn hash for correction history indexing
/// Only hashes pawn positions — correction is pawn-structure specific
pub fn pawn_hash(pos: &Position) -> u64 {
    use crate::position::zobrist::piece_key;

    let mut hash = 0u64;
    for color in Color::ALL {
        let mut pawns = pos.piece_bb(color, PieceKind::Pawn);
        while let Some(sq) = pawns.pop_lsb() {
            hash ^= piece_key(color, PieceKind::Pawn, sq);
        }
    }
    hash
}

/// Compute a continuation-based hash for correction history indexing
/// (ROADMAP Phase 26 item 3b, D86). Captures "what were the last two
/// real moves played" as a position-INDEPENDENT signal — deliberately
/// just the four square indices (from/to of each of the last two real
/// moves), not conditioned on piece type or color the way move
/// ordering's `cont_hist` is (see `search/mod.rs`'s `ContHistoryTable`
/// for that richer, per-candidate-move table). This is a coarser,
/// cheaper per-NODE signal for the correction table, not a per-move
/// score — some systematic eval errors track "what just happened"
/// (e.g. a piece retreating, a specific short tactical sequence)
/// rather than "what the board looks like right now", which is what
/// `pawn_hash`/`nonpawn_hash` capture instead.
///
/// Reads the second-to-last entry directly from `pos.history` (already
/// maintained by `make_move_with_history`/`unmake_move_with_history`)
/// rather than threading a new parameter through `alpha_beta`'s
/// recursion. `prev_move` (the last real move) is already an existing
/// parameter and always matches `pos.history.last().mv` whenever it's
/// not `Move::NULL` — every call site that passes a non-null
/// `prev_move` does so immediately after `pos.make_move_with_history`
/// pushed that same move onto `pos.history`; null-move pruning's
/// synthetic side-flip is never pushed to `pos.history` and always
/// passes `Move::NULL` as `prev_move`, so a null move is never
/// mistaken for real history here.
///
/// Returns `None` when there isn't a real two-move history yet (root
/// of search, or fewer than two real moves have been made in this
/// line) — the correction site treats `None` as "skip this source for
/// this node".
pub fn continuation_hash(pos: &Position, prev_move: Move) -> Option<u64> {
    if prev_move == Move::NULL {
        return None;
    }
    let n = pos.history.len();
    if n < 2 {
        return None;
    }
    let prev_prev_move = pos.history[n - 2].mv;

    let a = prev_move.from.index() as u64;
    let b = prev_move.to.index() as u64;
    let c = prev_prev_move.from.index() as u64;
    let d = prev_prev_move.to.index() as u64;

    // splitmix64-style mix of the four square indices (each 0..64, so
    // all four fit comfortably in the low 24 bits combined before mixing)
    let mut h = a | (b << 6) | (c << 12) | (d << 18);
    h ^= h >> 15;
    h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 13;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 16;
    Some(h)
}

/// Compute non-pawn-material hash for correction history indexing
/// (ROADMAP Phase 26 item 3a, D80). Hashes the placement of every
/// knight, bishop, rook, and queen (both colors) — deliberately
/// excludes pawns (already covered by `pawn_hash` above, a separate
/// signal) and kings (king position is far more volatile move-to-move
/// than a systematic-error signal benefits from indexing by; king
/// safety already has its own dedicated eval term and isn't what this
/// correction source targets).
///
/// This targets a different systematic-error pattern than the existing
/// pawn-structure correction: cases where the *piece* placement (a
/// knight stuck on the rim, a bishop pair vs. a lone bishop, rooks
/// doubled or not) causes the static evaluator to be consistently
/// wrong in ways search then has to discover and correct for at every
/// node, independent of what the pawns are doing. Same technique
/// Stockfish uses for its own non-pawn correction history source.
pub fn nonpawn_hash(pos: &Position) -> u64 {
    use crate::position::zobrist::piece_key;

    let mut hash = 0u64;
    for color in Color::ALL {
        for kind in [
            PieceKind::Knight,
            PieceKind::Bishop,
            PieceKind::Rook,
            PieceKind::Queen,
        ] {
            let mut pieces = pos.piece_bb(color, kind);
            while let Some(sq) = pieces.pop_lsb() {
                hash ^= piece_key(color, kind, sq);
            }
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
    fn test_lmr_not_applied_to_tt_move() {
        // D118 (Session 120, review finding #4): this exact case is the
        // one the pre-fix inline gate in alpha_beta.rs silently missed —
        // should_apply_lmr() itself always excluded the TT move
        // correctly, but nothing called it, so the bug was in the
        // wiring, not this function. Kept here anyway as a direct,
        // permanent guard against the same omission creeping back in.
        let mv = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        assert!(!should_apply_lmr(mv, 5, 6, false, false, false, true),
            "The TT move should not be reduced");
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
    fn test_lmr_thread_base_main_thread_unchanged() {
        // Thread 0 (main thread) must always get the original constant —
        // this is the safety property the whole feature depends on: it
        // must be impossible for this change to alter main-thread (i.e.
        // single-threaded, or the reported line of a multi-threaded)
        // search behavior.
        assert_eq!(lmr_thread_base(0), 0.75);
    }

    #[test]
    fn test_lmr_thread_base_varies_by_thread() {
        // Helper threads (id >= 1) should not all collapse to the same
        // base as each other or as the main thread — that would defeat
        // the point of thread differentiation.
        let b1 = lmr_thread_base(1);
        let b2 = lmr_thread_base(2);
        assert_ne!(b1, 0.75, "thread 1 should differ from main thread's base");
        assert_ne!(b2, 0.75, "thread 2 should differ from main thread's base");
        assert_ne!(b1, b2, "distinct helper threads should get distinct bases");
    }

    #[test]
    fn test_lmr_thread_base_wraps_around() {
        // With 4 offsets in the table, thread_id 4 should repeat thread 0's
        // slot, thread 5 repeat thread 1's, etc. — this documents the
        // wraparound as intentional behavior, not an accident, for any
        // future reader (or session) tempted to "fix" the modulo.
        assert_eq!(lmr_thread_base(4), lmr_thread_base(0));
        assert_eq!(lmr_thread_base(5), lmr_thread_base(1));
    }

    #[test]
    fn test_max_extension_cap() {
        setup();
        let pos = Position::start_pos().unwrap();
        let mv  = Move::new(Square::E2, Square::E3, MoveKind::Quiet);
        // Even with all extensions, should not exceed MAX_EXTENSION.
        // prev_move = Move::NULL is fine here — is_recapture() only
        // needs prev_move.kind.is_capture(), and NULL isn't a capture,
        // so this exercises the check-extension path only, which is
        // enough to prove the cap holds.
        let ext = extension(&pos, mv, Move::NULL, true, true, 10, 0);
        assert!(ext <= MAX_EXTENSION,
            "Extension should not exceed MAX_EXTENSION");
    }

    // ── is_recapture (D117, Session 119, review finding #3) ────────────────────

    #[test]
    fn test_is_recapture_true_for_genuine_recapture() {
        // Opponent captured on d5 (their previous move); we capture
        // back on d5 — a genuine recapture.
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::C6, Square::D5, MoveKind::Capture);
        assert!(is_recapture(mv, prev_move),
            "capturing on the same square the opponent just captured on \
             must be detected as a recapture");
    }

    #[test]
    fn test_is_recapture_false_for_ordinary_capture_different_square() {
        // The pre-D117 bug: this used to return true for ANY capture,
        // regardless of where the opponent's previous move landed. An
        // ordinary capture on a square the opponent's last move didn't
        // touch must not count as a recapture.
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::F6, Square::G4, MoveKind::Capture);
        assert!(!is_recapture(mv, prev_move),
            "a capture on a different square than the opponent's last \
             move must not be treated as a recapture (this is the exact \
             bug D117 fixed — the old version ignored prev_move entirely)");
    }

    #[test]
    fn test_is_recapture_false_when_previous_move_was_not_a_capture() {
        // Same destination square as a quiet previous move — still not
        // a recapture, since nothing was captured there to recapture.
        let prev_move = Move::new(Square::E2, Square::D5, MoveKind::Quiet);
        let mv        = Move::new(Square::C6, Square::D5, MoveKind::Capture);
        assert!(!is_recapture(mv, prev_move),
            "landing on the same square as a non-capturing previous \
             move is not a recapture");
    }

    #[test]
    fn test_is_recapture_false_when_this_move_is_not_a_capture() {
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::C6, Square::D5, MoveKind::Quiet);
        assert!(!is_recapture(mv, prev_move),
            "a non-capturing move can never be a recapture, regardless \
             of the previous move");
    }

    #[test]
    fn test_recapture_and_passed_pawn_extension_fires_for_recapture() {
        setup();
        let pos       = Position::start_pos().unwrap();
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::C6, Square::D5, MoveKind::Capture);
        assert_eq!(recapture_and_passed_pawn_extension(&pos, mv, prev_move, 3), 1,
            "a genuine recapture at shallow depth should extend by 1");
    }

    #[test]
    fn test_recapture_and_passed_pawn_extension_respects_depth_cutoff() {
        setup();
        let pos       = Position::start_pos().unwrap();
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::C6, Square::D5, MoveKind::Capture);
        // depth > 4: the recapture-extension's own depth <= 4 guard
        // must suppress it even though the move is a genuine recapture.
        assert_eq!(recapture_and_passed_pawn_extension(&pos, mv, prev_move, 5), 0,
            "recapture extension must not fire beyond depth 4");
    }

    #[test]
    fn test_recapture_and_passed_pawn_extension_zero_for_unrelated_capture() {
        setup();
        let pos       = Position::start_pos().unwrap();
        let prev_move = Move::new(Square::E4, Square::D5, MoveKind::Capture);
        let mv        = Move::new(Square::F6, Square::G4, MoveKind::Capture);
        assert_eq!(recapture_and_passed_pawn_extension(&pos, mv, prev_move, 3), 0,
            "an ordinary capture unrelated to the opponent's last move \
             must not extend — this is the D117 bug fix's direct effect");
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
    fn test_nonpawn_hash_differs_by_position() {
        setup();
        let pos1 = Position::start_pos().unwrap();
        let pos2 = Position::generate_with_seed(42);
        let h1   = nonpawn_hash(&pos1);
        let h2   = nonpawn_hash(&pos2);
        assert!(h1 != 0, "Non-pawn-material hash should be non-zero");
        assert!(h2 != 0, "Non-pawn-material hash should be non-zero");
    }

    #[test]
    fn test_nonpawn_hash_ignores_pawn_structure() {
        setup();
        // Two positions with identical non-pawn piece placement but
        // different pawn structure must produce the SAME non-pawn hash —
        // that's the entire point of the two hashes being independent
        // signal sources. Standard start vs. standard start with the
        // e-pawns pushed one square: same piece placement, different
        // pawns.
        let start = Position::start_pos().unwrap();
        let e4    = Position::from_fen(
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 1"
        ).unwrap();
        assert_eq!(nonpawn_hash(&start), nonpawn_hash(&e4),
            "non-pawn hash must be identical when only pawns differ");
        assert_ne!(pawn_hash(&start), pawn_hash(&e4),
            "sanity check: pawn hash SHOULD differ here (control case)");
    }

    #[test]
    fn test_nonpawn_hash_ignores_king_position() {
        setup();
        // Kings deliberately excluded from the non-pawn hash (see the
        // function's doc comment) — moving only a king must not change
        // the non-pawn-material hash.
        let pos1 = Position::from_fen(
            "4k3/8/8/8/8/8/8/4K2R w K - 0 1"
        ).unwrap();
        let pos2 = Position::from_fen(
            "4k3/8/8/8/8/8/4K3/7R w - - 0 1"
        ).unwrap();
        assert_eq!(nonpawn_hash(&pos1), nonpawn_hash(&pos2),
            "moving only the king must not change the non-pawn hash \
             (kings are deliberately excluded)");
    }

    #[test]
    fn test_continuation_hash_none_at_root() {
        setup();
        // prev_move == Move::NULL means "no history at all yet" (the
        // search root) — must return None, not compute anything.
        let pos = Position::start_pos().unwrap();
        assert_eq!(continuation_hash(&pos, Move::NULL), None,
            "continuation_hash must be None with no previous move");
    }

    #[test]
    fn test_continuation_hash_none_with_only_one_real_move() {
        setup();
        // A real prev_move exists, but pos.history has only that one
        // entry — not enough for a "pair", must return None.
        let mut pos = Position::start_pos().unwrap();
        let mv = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        pos.make_move_with_history(mv);
        assert_eq!(continuation_hash(&pos, mv), None,
            "continuation_hash must be None with only one move of history");
    }

    #[test]
    fn test_continuation_hash_some_with_two_real_moves() {
        setup();
        let mut pos = Position::start_pos().unwrap();
        let mv1 = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        pos.make_move_with_history(mv1);
        let mv2 = Move::new(Square::E7, Square::E5, MoveKind::DoublePush);
        pos.make_move_with_history(mv2);
        assert!(continuation_hash(&pos, mv2).is_some(),
            "continuation_hash must be Some once two real moves exist");
    }

    #[test]
    fn test_continuation_hash_differs_for_different_move_pairs() {
        setup();
        // Same first move (e2e4), different second move (e7e5 vs d7d5)
        // must produce different hashes — otherwise the "pair" signal
        // degenerates to just the single most-recent move.
        let mut pos_a = Position::start_pos().unwrap();
        pos_a.make_move_with_history(
            Move::new(Square::E2, Square::E4, MoveKind::DoublePush));
        let mv2a = Move::new(Square::E7, Square::E5, MoveKind::DoublePush);
        pos_a.make_move_with_history(mv2a);

        let mut pos_b = Position::start_pos().unwrap();
        pos_b.make_move_with_history(
            Move::new(Square::E2, Square::E4, MoveKind::DoublePush));
        let mv2b = Move::new(Square::D7, Square::D5, MoveKind::DoublePush);
        pos_b.make_move_with_history(mv2b);

        assert_ne!(continuation_hash(&pos_a, mv2a), continuation_hash(&pos_b, mv2b),
            "different second moves in the pair must hash differently");
    }

    #[test]
    fn test_continuation_hash_matches_history_regardless_of_current_position() {
        setup();
        // The hash is a pure function of the last two moves' squares —
        // it must be identical for two totally different resulting
        // positions/pieces, as long as the move pair (by square) is the
        // same. This is intentional (see the function's doc comment:
        // deliberately position- and piece-independent), not a bug —
        // this test documents that behavior explicitly so a future
        // change to make it piece-aware doesn't silently drift.
        let mut pos1 = Position::start_pos().unwrap();
        pos1.make_move_with_history(
            Move::new(Square::E2, Square::E4, MoveKind::DoublePush));
        let mv2 = Move::new(Square::E7, Square::E5, MoveKind::DoublePush);
        pos1.make_move_with_history(mv2);
        let h1 = continuation_hash(&pos1, mv2);

        // A contrived second position with an unrelated board but the
        // exact same trailing two-move history by square.
        let mut pos2 = Position::from_fen(
            "8/8/8/8/8/8/8/4K2k w - - 0 1"
        ).unwrap();
        // Manually replay the same two "moves" by square onto a
        // deliberately different board just to prove the hash only
        // depends on pos.history's squares, not piece identity. Both
        // entries must be pushed — continuation_hash's own contract
        // (see its doc comment) is that `prev_move` always equals
        // `pos.history.last().mv`; pushing only the first move here
        // would violate that and make this test invalid, not just
        // fail to prove the intended point.
        pos2.history.push(crate::position::HistoryEntry {
            mv: Move::new(Square::E2, Square::E4, MoveKind::DoublePush),
            castling: pos2.castling,
            en_passant: pos2.en_passant,
            halfmove_clock: pos2.halfmove_clock,
            hash: pos2.hash,
            captured: None,
        });
        pos2.history.push(crate::position::HistoryEntry {
            mv: mv2,
            castling: pos2.castling,
            en_passant: pos2.en_passant,
            halfmove_clock: pos2.halfmove_clock,
            hash: pos2.hash,
            captured: None,
        });
        assert_eq!(h1, continuation_hash(&pos2, mv2),
            "hash must depend only on the move-pair squares, not the \
             board position they occurred on");
    }

    // ── Correction-scaled singular margin (ROADMAP Phase 26 item 3c, D89) ──────

    #[test]
    fn test_singular_margin_reduction_zero_below_threshold() {
        assert_eq!(singular_margin_reduction(0), 0);
        assert_eq!(singular_margin_reduction(150), 0);
        assert_eq!(singular_margin_reduction(299), 0,
            "just below the 300 threshold must still be 0");
    }

    #[test]
    fn test_singular_margin_reduction_one_at_and_above_threshold() {
        assert_eq!(singular_margin_reduction(300), 1);
        assert_eq!(singular_margin_reduction(450), 1);
    }

    #[test]
    fn test_singular_margin_reduction_capped_at_one() {
        // CorrectionHistory entries are clamped to ±512 (see
        // CorrectionHistory::update), so 512 is the real maximum input
        // this ever receives in practice — must still cap at 1, not
        // scale further.
        assert_eq!(singular_margin_reduction(512), 1);
        assert_eq!(singular_margin_reduction(10_000), 1,
            "must cap at 1 regardless of how large the input is");
    }

    #[test]
    fn test_lmp_threshold_increases_with_depth() {
        // Deeper nodes must tolerate at least as many quiet moves before
        // pruning — a shrinking threshold would prune more aggressively
        // the deeper we go, which is backwards. Checked for both D114
        // tables independently.
        for d in 1..crate::search::MAX_DEPTH_LMP {
            assert!(lmp_threshold(d + 1, true) >= lmp_threshold(d, true),
                "LMP threshold (improving) should not shrink with depth (depth {})", d);
            assert!(lmp_threshold(d + 1, false) >= lmp_threshold(d, false),
                "LMP threshold (non-improving) should not shrink with depth (depth {})", d);
        }
    }

    #[test]
    fn test_lmp_threshold_clamped_out_of_range() {
        // Depth 0 and depths beyond the table must not panic — they
        // clamp to the nearest in-range entry.
        let _ = lmp_threshold(0, true);
        let _ = lmp_threshold(crate::search::MAX_DEPTH_LMP + 5, true);
        let _ = lmp_threshold(0, false);
        let _ = lmp_threshold(crate::search::MAX_DEPTH_LMP + 5, false);
    }

    #[test]
    fn test_lmp_threshold_improving_true_matches_pre_d114_table() {
        // improving=true must reproduce the exact pre-D114 values —
        // this is what makes improving_enabled=false (which always
        // passes improving=true, see alpha_beta.rs) byte-identical to
        // engine behavior before D114 existed.
        let expected = [0usize, 3, 4, 6, 9, 12, 16, 20, 25];
        for (d, &want) in expected.iter().enumerate() {
            assert_eq!(lmp_threshold(d as i32, true), want,
                "improving=true threshold at depth {d} must match the original table");
        }
    }

    #[test]
    fn test_lmp_threshold_non_improving_never_exceeds_improving() {
        // The whole point of the split: non-improving must prune at
        // least as aggressively (threshold no higher) as improving, at
        // every depth in range.
        for d in 0..=crate::search::MAX_DEPTH_LMP {
            assert!(lmp_threshold(d, false) <= lmp_threshold(d, true),
                "non-improving threshold must not exceed improving threshold (depth {d})");
        }
    }

    #[test]
    fn test_lmp_not_applied_in_pv_node() {
        assert!(!should_apply_lmp(4, 20, true, false, false, true, 0, 0, true),
            "LMP must never fire in a PV node");
    }

    #[test]
    fn test_lmp_not_applied_in_check_or_giving_check() {
        assert!(!should_apply_lmp(4, 20, true, true, false, false, 0, 0, true),
            "LMP must not fire when in check");
        assert!(!should_apply_lmp(4, 20, true, false, true, false, 0, 0, true),
            "LMP must not fire on a move that gives check");
    }

    #[test]
    fn test_lmp_not_applied_to_non_quiet_moves() {
        assert!(!should_apply_lmp(4, 20, false, false, false, false, 0, 0, true),
            "LMP must not fire on captures/promotions");
    }

    #[test]
    fn test_lmp_not_applied_beyond_max_depth() {
        let too_deep = crate::search::MAX_DEPTH_LMP + 1;
        assert!(!should_apply_lmp(too_deep, 100, true, false, false, false, 0, 0, true),
            "LMP must not fire beyond MAX_DEPTH_LMP");
    }

    #[test]
    fn test_lmp_not_applied_near_mate_scores() {
        assert!(!should_apply_lmp(4, 20, true, false, false, false, MATE_THRESHOLD, 0, true),
            "LMP must not fire when alpha is a mate-range score");
        assert!(!should_apply_lmp(4, 20, true, false, false, false, 0, MATE_THRESHOLD, true),
            "LMP must not fire when beta is a mate-range score");
    }

    #[test]
    fn test_lmp_fires_past_threshold() {
        let d = 4;
        let threshold = lmp_threshold(d, true);
        assert!(!should_apply_lmp(d, threshold - 1, true, false, false, false, 0, 0, true),
            "LMP must not fire just below the threshold");
        assert!(should_apply_lmp(d, threshold, true, false, false, false, 0, 0, true),
            "LMP must fire once the quiet-move count reaches the threshold");
    }

    #[test]
    fn test_lmp_fires_earlier_when_non_improving() {
        // D114: with the same moves_tried count, a non-improving node
        // must be at least as willing to prune as an improving one —
        // demonstrated at a count that clears the non-improving
        // threshold but not the improving one.
        let d = 4;
        let non_improving_threshold = lmp_threshold(d, false);
        let improving_threshold = lmp_threshold(d, true);
        assert!(non_improving_threshold < improving_threshold,
            "test assumes the two tables actually differ at depth {d}");
        assert!(should_apply_lmp(d, non_improving_threshold, true, false, false, false, 0, 0, false),
            "LMP must fire for a non-improving node at its own (lower) threshold");
        assert!(!should_apply_lmp(d, non_improving_threshold, true, false, false, false, 0, 0, true),
            "the same moves_tried count must not fire yet for an improving node");
    }

    #[test]
    fn test_futility_margin_improving_matches_pre_d114_formula() {
        // improving=true must reproduce the exact pre-D114 formula —
        // same byte-identical-when-disabled requirement as the LMP
        // table above.
        for depth in 1..=5 {
            assert_eq!(futility_margin(depth, true), 100 * depth + 200,
                "improving=true margin at depth {depth} must match the original formula");
        }
    }

    #[test]
    fn test_futility_margin_non_improving_smaller() {
        // Non-improving must use a smaller (more-pruning) margin than
        // improving, at every depth — never larger.
        for depth in 1..=5 {
            assert!(futility_margin(depth, false) < futility_margin(depth, true),
                "non-improving futility margin must be smaller at depth {depth}");
        }
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
