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
        if has_castling_rights(pos) {
            return None; // Syzygy tables don't encode castling — see below
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
        if has_castling_rights(pos) {
            return None; // Syzygy tables don't encode castling — see below
        }
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

/// Does `pos` still have any castling right, for either color?
///
/// Bug fix (confirmed 2026-08-03, external bug report): Syzygy tables are
/// generated under the fixed assumption that castling is never possible —
/// the underlying retrograde-analysis tables don't encode castling rights
/// at all, so every mainstream Syzygy-consuming engine (Stockfish
/// included) gates every probe on zero remaining castling rights. Pet
/// Dragon never did this at either call site. `ENGINE_ARCHITECTURE.md`
/// previously claimed this was safe because "castling rights are gone by
/// the time few enough pieces remain for tablebase lookup" — that
/// assumption has no backing decision or test and is contradicted by the
/// project's own data (roughly 26% of games retain at least one castling
/// right, with no game rule forcing rights to clear before material thins
/// into tablebase range; a king and its never-moved rook can easily
/// survive untouched into a 5-7 piece endgame). Centralized here (rather
/// than duplicated at each call site) so a future third call site can't
/// reintroduce the gap.
fn has_castling_rights(pos: &Position) -> bool {
    pos.castling.has_any(Color::White) || pos.castling.has_any(Color::Black)
}

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

    /// `SyzygyProber::new()`, and the castling-rights guard on both probe
    /// entry points, in a single test.
    ///
    /// CI bug fix (confirmed 2026-08-03, session 130): this used to be
    /// three separate tests, each calling `SyzygyProber::new()`. That's
    /// unsafe — `pyrrhic_rs::TableBases` is a process-wide singleton
    /// (DECISIONS.md D17), so only the *first* `new()` call across the
    /// whole test binary succeeds; every later call returns
    /// `Err(AlreadyInitialized)` regardless of path. `cargo test` runs
    /// tests concurrently by default, so which call "wins" is a race —
    /// any test that `.unwrap()`s its own call is flaky by construction.
    /// This was caught by a real CI run: `test_probe_wdl_refuses_...`
    /// failed with exactly this `AlreadyInitialized` panic while its
    /// sibling `test_probe_root_refuses_...` (same code, same call)
    /// happened to win the race and passed. Consolidating to one `new()`
    /// call for the whole module removes the race outright rather than
    /// papering over one specific ordering.
    ///
    /// NOTE (carried over from the original construction-only test):
    /// pyrrhic-rs's `TableBases::new()` does not validate the search path
    /// against the filesystem at init time — observed CI behavior shows
    /// it returns `Ok` with `max_pieces() == 7` (the library's full
    /// ceiling) even for a nonexistent directory. Real tablebase absence
    /// only surfaces later, as a probe failure (`probe_wdl`/`probe_root`
    /// return `None`), which is untestable here without bundling real
    /// `.rtbw` files.
    #[test]
    fn test_syzygy_prober_construction_and_castling_guard() {
        crate::bitboard::masks::init_masks();
        crate::bitboard::magic::init_magic();
        crate::position::zobrist::init_zobrist();

        // Construction must not panic, regardless of path validity.
        let tb = SyzygyProber::new("/nonexistent/syzygy/path/for/test")
            .expect("SyzygyProber::new() should return Ok even for a \
                     nonexistent path (pyrrhic-rs doesn't validate the \
                     path at init time) — this is the ONLY call to \
                     SyzygyProber::new() in this test binary; see the \
                     doc comment above for why that matters");

        // Low piece count (well within any real tablebase's range) but
        // White still has kingside castling rights.
        let pos = Position::from_fen("4k3/8/8/8/8/8/8/4K2R w K - 0 1").unwrap();
        assert!(has_castling_rights(&pos), "sanity: rights present in this FEN");

        assert_eq!(tb.probe_wdl(&pos), None,
            "probe_wdl must return None while castling rights remain, \
             even before considering whether real TB files are loaded");
        assert_eq!(tb.probe_root(&pos), None,
            "probe_root must return None while castling rights remain, \
             so the engine never plays a TB-suggested move that ignores \
             a legal castling option");
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

    /// has_castling_rights must detect a right on either side, and be
    /// false once both sides have none.
    #[test]
    fn test_has_castling_rights() {
        crate::bitboard::masks::init_masks();
        crate::bitboard::magic::init_magic();
        crate::position::zobrist::init_zobrist();

        let with_rights = Position::start_pos().unwrap();
        assert!(has_castling_rights(&with_rights),
            "starting position has castling rights for both sides");

        // K+R vs K endgame FEN with no castling rights remaining.
        let no_rights =
            Position::from_fen("8/8/8/4k3/8/8/4K3/R7 w - - 0 1").unwrap();
        assert!(!has_castling_rights(&no_rights),
            "position with '-' castling field should report no rights");
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

