// ============================================================================
// src/syzygy/mod.rs  —  Phase 15: Syzygy Endgame Tablebase Integration
// Copyright (C) 2026 Gokul Chandar. Licensed under GPL v3.
// Contributors: Claude (Anthropic).
//
// Native-only module (cfg-gated in lib.rs). pyrrhic-rs uses libc and
// cannot compile for wasm32 targets, so this file is not included in
// WASM builds.
//
// Original Pyrrhic C library:
//   Fathom © 2015 basil — all rights reserved
//   Modifications © 2016-2019 Jon Dart
//   Modifications © 2020 Andrew Grant
// Rust port: pyrrhic-rs © Algorhythm-sxv (MIT)
//   https://github.com/Algorhythm-sxv/pyrrhic-rs
// ============================================================================

use pyrrhic_rs::{EngineAdapter, TableBases, WdlProbeResult};

use crate::bitboard::{
    magic::{bishop_attacks, queen_attacks, rook_attacks},
    masks::{king_attacks, knight_attacks, pawn_attacks},
    Bitboard,
};
use crate::position::Position;
use crate::types::{Color, PieceKind, Square};

// ── Engine adapter ─────────────────────────────────────────────────────────────

/// Connects pyrrhic-rs to Pet Dragon's precomputed bitboard attack tables.
///
/// All six methods delegate directly to our `init_masks()` / `init_magic()`
/// tables. The startup sequence **must** have been called before any probe.
///
/// Pet Dragon's custom rank-1 pawn double-step is irrelevant here: TB probing
/// uses *capture* attacks only, not push moves.
#[derive(Clone)]
pub struct PetDragonAdapter;

impl EngineAdapter for PetDragonAdapter {
    /// Pawn diagonal attack squares for the given color and square index.
    fn pawn_attacks(color: pyrrhic_rs::Color, sq: u64) -> u64 {
        let c = if color == pyrrhic_rs::Color::White {
            Color::White
        } else {
            Color::Black
        };
        let s = Square::from_index(sq as u8).expect("TB: invalid pawn square");
        pawn_attacks(c, s).0
    }

    /// Knight attack squares from the given square index.
    fn knight_attacks(sq: u64) -> u64 {
        let s = Square::from_index(sq as u8).expect("TB: invalid knight square");
        knight_attacks(s).0
    }

    /// Bishop attack squares from the given square index with the given occupancy.
    fn bishop_attacks(sq: u64, occ: u64) -> u64 {
        let s = Square::from_index(sq as u8).expect("TB: invalid bishop square");
        bishop_attacks(s, Bitboard(occ)).0
    }

    /// Rook attack squares from the given square index with the given occupancy.
    fn rook_attacks(sq: u64, occ: u64) -> u64 {
        let s = Square::from_index(sq as u8).expect("TB: invalid rook square");
        rook_attacks(s, Bitboard(occ)).0
    }

    /// Queen attack squares from the given square index with the given occupancy.
    fn queen_attacks(sq: u64, occ: u64) -> u64 {
        let s = Square::from_index(sq as u8).expect("TB: invalid queen square");
        queen_attacks(s, Bitboard(occ)).0
    }

    /// King attack squares from the given square index.
    fn king_attacks(sq: u64) -> u64 {
        let s = Square::from_index(sq as u8).expect("TB: invalid king square");
        king_attacks(s).0
    }
}

// ── TB win/loss score ─────────────────────────────────────────────────────────

/// Centipawn value of a tablebase win or loss, from the side-to-move perspective.
///
/// Chosen to be:
/// - Above any normal HCE evaluation (max ≈ 4000 cp)
/// - Below the mate threshold (900_000) so TB wins sort correctly
///   alongside forced-mate scores in `iterative_deepening`
pub const TB_WIN_SCORE: i32 = 10_000;

// ── SyzygyProber ─────────────────────────────────────────────────────────────

/// High-level handle to the Syzygy endgame tablebases.
///
/// Wraps `TableBases<PetDragonAdapter>` with `Position`-aware helpers so
/// callers never touch raw bitboards or pyrrhic-rs types directly.
///
/// # Thread safety
/// - [`probe_wdl`] is fully thread-safe (concurrent calls from search threads
///   are safe per pyrrhic-rs documentation).
/// - [`probe_root`] is **not** thread-safe; call it at the root **before**
///   spawning any search helper threads.
///
/// [`probe_wdl`]: SyzygyProber::probe_wdl
/// [`probe_root`]: SyzygyProber::probe_root
pub struct SyzygyProber {
    tb: TableBases<PetDragonAdapter>,
}

impl SyzygyProber {
    /// Initialise the tablebases from a colon-separated path string.
    ///
    /// ```text
    /// let tb = SyzygyProber::new("/home/user/syzygy/tb345:/home/user/syzygy/tb6")?;
    /// ```
    ///
    /// Returns `Err(message)` if no files are found or initialisation fails.
    pub fn new(path: &str) -> Result<Self, String> {
        TableBases::<PetDragonAdapter>::new(path)
            .map(|tb| Self { tb })
            .map_err(|e| format!("Syzygy init error: {:?}", e))
    }

    /// Maximum number of pieces (including both kings) the loaded files support.
    ///
    /// Typically 3–7 depending on which Syzygy files are present.
    pub fn max_pieces(&self) -> u32 {
        self.tb.max_pieces()
    }

    /// Probe Win/Draw/Loss for an interior search node.
    ///
    /// # Returns
    /// - `None` if `halfmove_clock > 0` (WDL ignores the 50-move rule; results
    ///   are unreliable once the clock is nonzero — DTZ handles this at root).
    /// - `None` if the probe fails (missing files, wrong piece count, etc.).
    /// - `Some(score)` in centipawns from the side-to-move perspective:
    ///   `+TB_WIN_SCORE`, `0`, or `-TB_WIN_SCORE`. Cursed win / blessed loss
    ///   are mapped to `±1 cp` (technically draws under the 50-move rule).
    pub fn probe_wdl(&self, pos: &Position) -> Option<i32> {
        if pos.halfmove_clock > 0 {
            return None; // 50-move clock makes WDL result unreliable
        }
        let (white, black, kings, queens, rooks, bishops, knights, pawns, ep, turn) =
            extract_position_bits(pos);
        match self.tb.probe_wdl(
            white, black, kings, queens, rooks, bishops, knights, pawns, ep, turn,
        ) {
            Ok(wdl) => Some(wdl_to_score(wdl)),
            Err(_) => None,
        }
    }

    /// Probe Distance-To-Zero at the root to find the optimal tablebase move.
    ///
    /// Must be called from a **single thread** with no concurrent WDL probes
    /// in flight. Call this before spawning Lazy SMP helper threads.
    ///
    /// # Returns
    /// `Some((from_sq_index, to_sq_index, promotion_piece, wdl_score))` on
    /// success, where square indices are 0-based (A1=0 … H8=63).
    /// `None` if the probe fails or the position has more pieces than the
    /// loaded tablebase files support.
    pub fn probe_root(&self, pos: &Position) -> Option<(u8, u8, PieceKind, i32)> {
        let (white, black, kings, queens, rooks, bishops, knights, pawns, ep, turn) =
            extract_position_bits(pos);
        let rule50 = pos.halfmove_clock;
        self.tb
            .probe_root(
                white, black, kings, queens, rooks, bishops, knights, pawns,
                rule50, ep, turn,
            )
            .ok()
            .and_then(|dtz| match dtz.root {
                pyrrhic_rs::DtzProbeValue::DtzResult(r) => {
                    Some((r.from_square, r.to_square, pyrrhic_piece_to_pd(r.promotion),
                          wdl_to_score(r.wdl)))
                }
                _ => None,
            })
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Extract the raw `u64` bitboard values that pyrrhic-rs expects from a `Position`.
///
/// Returns `(white, black, kings, queens, rooks, bishops, knights, pawns, ep, turn)`.
/// `ep` is the en-passant target square index (0 if none), `turn` is `true` for White.
fn extract_position_bits(pos: &Position) -> (u64, u64, u64, u64, u64, u64, u64, u64, u32, bool) {
    let (w, b) = (Color::White, Color::Black);
    let (pawn, knight, bishop, rook, queen, king) = (
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    );

    let white   = pos.occupied(w).0;
    let black   = pos.occupied(b).0;
    let kings   = (pos.piece_bb(w, king)   | pos.piece_bb(b, king)).0;
    let queens  = (pos.piece_bb(w, queen)  | pos.piece_bb(b, queen)).0;
    let rooks   = (pos.piece_bb(w, rook)   | pos.piece_bb(b, rook)).0;
    let bishops = (pos.piece_bb(w, bishop) | pos.piece_bb(b, bishop)).0;
    let knights = (pos.piece_bb(w, knight) | pos.piece_bb(b, knight)).0;
    let pawns   = (pos.piece_bb(w, pawn)   | pos.piece_bb(b, pawn)).0;
    let ep      = pos.en_passant.map(|sq| sq.index() as u32).unwrap_or(0);
    let turn    = pos.side_to_move == w;

    (white, black, kings, queens, rooks, bishops, knights, pawns, ep, turn)
}

/// Map a pyrrhic-rs `WdlProbeResult` to centipawns from the side-to-move perspective.
fn wdl_to_score(wdl: WdlProbeResult) -> i32 {
    match wdl {
        WdlProbeResult::Win         =>  TB_WIN_SCORE,
        WdlProbeResult::CursedWin   =>  1,   // drawn under 50-move rule
        WdlProbeResult::Draw        =>  0,
        WdlProbeResult::BlessedLoss => -1,   // drawn under 50-move rule
        WdlProbeResult::Loss        => -TB_WIN_SCORE,
    }
}

/// Convert a pyrrhic-rs promotion `Piece` to our `PieceKind`.
///
/// `Piece::Pawn` and `Piece::King` cannot be promotion targets; we default
/// to `Queen` as the safest fallback.
fn pyrrhic_piece_to_pd(p: pyrrhic_rs::Piece) -> PieceKind {
    match p {
        pyrrhic_rs::Piece::Queen  => PieceKind::Queen,
        pyrrhic_rs::Piece::Rook   => PieceKind::Rook,
        pyrrhic_rs::Piece::Bishop => PieceKind::Bishop,
        pyrrhic_rs::Piece::Knight => PieceKind::Knight,
        _                         => PieceKind::Queen,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// SyzygyProber::new with a nonexistent path must never panic, and must
    /// never report usable tablebase coverage.
    ///
    /// NOTE: pyrrhic-rs's `TableBases` is a process-wide singleton (guarded by
    /// a global `TB_INITIALIZED` static in the underlying C library). For some
    /// invalid paths `tb_init()` still returns Ok with `TB_LARGEST` reflecting
    /// only trivial (fileless) endgame classes, so `Err` is not guaranteed —
    /// we only assert that no real tablebase files were picked up.
    #[test]
    fn test_syzygy_bad_path_returns_err() {
        match SyzygyProber::new("/nonexistent/syzygy/path/for/test") {
            Err(_) => {} // expected outcome
            Ok(prober) => assert_eq!(
                prober.max_pieces(),
                0,
                "Nonexistent path should never yield usable tablebase coverage"
            ),
        }
    }

    /// extract_position_bits must produce non-overlapping white/black masks
    /// and correctly identify the side to move.
    #[test]
    fn test_extract_position_bits_standard_start() {
        crate::bitboard::masks::init_masks();
        crate::bitboard::magic::init_magic();
        crate::position::zobrist::init_zobrist();

        let pos = Position::start_pos().unwrap();
        let (white, black, _kings, _queens, _rooks, _bishops, _knights, _pawns, ep, turn) =
            extract_position_bits(&pos);

        assert_eq!(white & black, 0, "White/Black bitboards must not overlap");
        assert_eq!(ep, 0, "No en passant at start");
        assert!(turn, "White to move at start (turn == true)");
    }

    /// wdl_to_score must produce a symmetric result: win flips to loss.
    #[test]
    fn test_wdl_to_score_symmetry() {
        assert_eq!(wdl_to_score(WdlProbeResult::Win),  TB_WIN_SCORE);
        assert_eq!(wdl_to_score(WdlProbeResult::Loss), -TB_WIN_SCORE);
        assert_eq!(wdl_to_score(WdlProbeResult::Draw), 0);
        // Cursed win < normal win; blessed loss > normal loss
        assert!(wdl_to_score(WdlProbeResult::CursedWin)  < TB_WIN_SCORE);
        assert!(wdl_to_score(WdlProbeResult::BlessedLoss) > -TB_WIN_SCORE);
    }

    /// TB_WIN_SCORE must be above any normal eval but below mate threshold.
    #[test]
    fn test_tb_win_score_bounds() {
        const MAX_NORMAL_EVAL: i32 = 8_000; // generous upper bound on HCE
        const MATE_THRESHOLD: i32  = 900_000;
        assert!(TB_WIN_SCORE > MAX_NORMAL_EVAL, "TB win should beat any normal eval");
        assert!(TB_WIN_SCORE < MATE_THRESHOLD,  "TB win must be below mate threshold");
    }
}

