// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// search/mod.rs — Search infrastructure and constants
//
// The search is the engine's brain. It explores the game tree to find
// the best move in a position. Built on alpha-beta with PVS, iterative
// deepening, and many pruning techniques from the master plan.
//
// Score system:
//   Scores are in centipawns (1 pawn = 100 centipawns).
//   Positive = good for side to move.
//   Negative = good for opponent.
//   Mate scores: MATE_SCORE - ply (closer mate = higher score)
//
// Search flow:
//   iterative_deepening() → alpha_beta() → quiescence()
//   At each node: check TT, generate moves, order moves,
//                 recurse, update TT, return best score
// ============================================================================

pub mod alpha_beta;
pub mod iterative;
pub mod ordering;
pub mod time;
pub mod see;
pub mod pruning;
pub mod skill;

use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use crate::types::Move;

// ── Score constants ───────────────────────────────────────────────────────────

/// Effectively infinite score — used as initial alpha/beta bounds
pub const INFINITY: i32 = 1_000_000;

/// Score for a drawn position
pub const DRAW_SCORE: i32 = 0;

/// Draw score adjusted for the UCI `Contempt` setting (default 0, in which
/// case this is always exactly `DRAW_SCORE` — byte-identical to pre-
/// Contempt behavior).
///
/// Contempt is defined relative to the *root* side (positive = the root
/// side dislikes draws and treats one as worth `-contempt` to itself).
/// Since `alpha_beta` uses negamax convention (every returned score is
/// from the current node's side-to-move perspective) and `ply` increments
/// by exactly 1 per node starting at 0 at the root, `ply % 2 == 0` means
/// the current node's side to move IS the root side — no separate
/// root-side field needs to be threaded through the search for this.
#[inline]
pub fn draw_score(ply: usize, contempt: i32) -> i32 {
    if contempt == 0 {
        DRAW_SCORE
    } else if ply % 2 == 0 {
        DRAW_SCORE - contempt
    } else {
        DRAW_SCORE + contempt
    }
}

/// Base mate score — actual mate score is MATE_SCORE - ply
/// So mate in 1 = 999_999, mate in 2 = 999_998, etc.
pub const MATE_SCORE: i32 = 999_999;

/// Any score above this is a forced mate
/// Used to detect mate scores in TT and elsewhere
pub const MATE_THRESHOLD: i32 = 900_000;

/// Minimum depth for various pruning techniques
pub const MIN_DEPTH_NULL_MOVE:   i32 = 3;
pub const MIN_DEPTH_FUTILITY:    i32 = 1;
pub const MIN_DEPTH_LMR:         i32 = 3;
pub const MIN_DEPTH_PROBCUT:     i32 = 5;
pub const MIN_DEPTH_IIR:         i32 = 4;
pub const MIN_DEPTH_RAZORING:    i32 = 1;
pub const MIN_DEPTH_SINGULAR:    i32 = 6;

/// Maximum search depth
pub const MAX_DEPTH: i32 = 128;

/// Maximum ply in search tree
pub const MAX_PLY: usize = 128;

/// Number of killer moves per ply
pub const KILLER_COUNT: usize = 2;

// ── Killer moves ──────────────────────────────────────────────────────────────

/// Killer moves table: killers[ply][0..KILLER_COUNT]
/// Stores quiet moves that caused beta cutoffs at each ply
pub type KillerTable = [[Move; KILLER_COUNT]; MAX_PLY];

/// History heuristic table: history[color][from][to]
/// Stores how often a move caused a beta cutoff
pub type HistoryTable = [[[i32; 64]; 64]; 2];

/// Countermove table: countermoves[from][to]
/// Stores best response to opponent's last move
pub type CountermoveTable = [[Move; 64]; 64];

/// Continuation history: cont_history[piece][to]
/// History across two consecutive moves
pub type ContHistoryTable = [[i32; 64]; 12]; // 6 piece kinds × 2 colors

// ── Search info ───────────────────────────────────────────────────────────────

/// All state needed during a search
/// Passed through the search tree by reference
pub struct SearchInfo {
    // ── Time management ───────────────────────────────────────────────────────
    /// Time allocated for this move in milliseconds
    pub time_allocated_ms: u64,
    /// Time when search started
    pub start_time: web_time::Instant,
    /// Hard stop flag — set when time runs out or stop command received
    pub stop: bool,

    // ── Output control ────────────────────────────────────────────────────────
    /// When true (default), iterative_deepening() prints UCI "info depth ..."
    /// lines to stdout every iteration — required for the real UCI loop
    /// (main.rs). Silent-search callers (selfplay.rs, match_runner.rs) run
    /// thousands of searches per invocation and must set this false, or the
    /// resulting stdout volume (tens of thousands of lines) can overwhelm a
    /// subprocess-output relay with a hard timeout — e.g. Kaggle's papermill
    /// execution hit exactly this via a 4s IOPub timeout on a fully-successful
    /// 750-game run (Session 39).
    pub print_info: bool,

    // ── Node counting ─────────────────────────────────────────────────────────
    /// Total nodes searched this iteration
    pub nodes: u64,
    /// Nodes per second (computed periodically)
    pub nps: u64,

    // ── Move tables ───────────────────────────────────────────────────────────
    /// Killer moves [ply][slot]
    pub killers: KillerTable,
    /// History heuristic [color][from][to]
    pub history: HistoryTable,
    /// Countermove table [from][to]
    pub countermoves: CountermoveTable,
    /// Continuation history: cont_hist[prev_to][piece_idx][curr_to]
    /// piece_idx = piece_kind as usize * 2 + color as usize (0..11)
    /// Conditions move quality on the previous move's destination square
    /// and the currently-moving piece — captures same-direction continuations.
    /// Boxed because 64×12×64×4 = 192KB would overflow the stack as a bare array.
    pub cont_hist: Box<[[[i32; 64]; 12]; 64]>,

    // ── Search limits ─────────────────────────────────────────────────────────
    /// Maximum depth to search
    pub max_depth: i32,
    /// Fixed node count limit (0 = no limit)
    pub node_limit: u64,

    // ── Principal variation ───────────────────────────────────────────────────
    /// PV line length at each ply
    pub pv_length: [usize; MAX_PLY],
    /// PV table [ply][move_index]
    pub pv_table: [[Move; MAX_PLY]; MAX_PLY],

    // ── Current search state ──────────────────────────────────────────────────
    /// Current search depth (for display)
    pub current_depth: i32,
    /// Best score found so far
    pub best_score: i32,
    /// Best move found so far
    pub best_move: Move,
    /// Seldepth — maximum ply reached in search
    pub seldepth: usize,
    /// Correction history — pawn-structure-indexed eval error tracker (Phase 13.2)
    pub correction_history: crate::search::pruning::CorrectionHistory,
    /// Shared stop flag — set by UCI `stop` command or when time expires.
    /// All threads sharing this Arc terminate as soon as the flag is set.
    pub stop_flag: Arc<AtomicBool>,

    /// Ponder-hit soft-limit override, in ms (Phase 18/D37). Expressed
    /// relative to THIS search's own `start_time`, not reset to zero —
    /// pondering time is free per the UCI spec, and `start_time` is owned
    /// by the search thread so it can't safely be reset from main.rs's
    /// `ponderhit` handler on another thread. `u64::MAX` = not yet fired.
    /// Set once by `ponderhit`; consumed once (via `swap`) by
    /// `iterative_deepening()` between depth iterations to replace the
    /// effectively-infinite ponder `TimeManager` with a real, bounded one.
    pub ponder_hit_soft_ms: Arc<AtomicU64>,
    /// Ponder-hit hard-limit override — same units/semantics as
    /// `ponder_hit_soft_ms`, but this is the one `is_time_up()` actually
    /// checks every 256 nodes (kept separate so that hot-path check stays
    /// a single atomic load, matching the existing `stop_flag` pattern).
    /// Left set (not consumed) for the rest of the search once ponderhit
    /// fires — a new search always gets a fresh `u64::MAX` via
    /// `reset_for_search()`.
    pub ponder_hit_hard_ms: Arc<AtomicU64>,

    // ── MultiPV (Phase 19) ──────────────────────────────────────────────────
    /// Number of principal variations to report, per UCI's `MultiPV`
    /// option. `1` (the default) is the common case and produces byte-
    /// identical behavior to before this field existed — the extra-lines
    /// logic in `iterative_deepening()` is entirely gated behind
    /// `multipv > 1`. Set from `main.rs`'s `EngineState.multipv` in cmd_go.
    pub multipv: usize,
    /// Root moves already claimed by an earlier MultiPV line at the
    /// current depth, so the next line's search skips them. Only ever
    /// checked at the root (ply 0) in `alpha_beta`'s move loop — see the
    /// skip check there for why this can't collide with singular
    /// extension's `excluded` parameter (root_node never triggers
    /// singular verification). Always empty when `multipv <= 1`, so that
    /// check costs a single `is_empty()` per root visit in the common case.
    pub root_exclude: Vec<Move>,

    /// Syzygy tablebase handle — native only (Phase 15).
    /// Set by main.rs when a SyzygyPath is configured. None = no tablebases.
    /// Arc makes it cheap to clone into helper threads.
    #[cfg(not(target_arch = "wasm32"))]
    pub syzygy: Option<std::sync::Arc<crate::syzygy::SyzygyProber>>,

    // ── Skill Level (Phase 20 / D39) ──────────────────────────────────────────
    /// UCI `Skill Level` setting, 0..=20. `20` (`skill::MAX_SKILL_LEVEL`,
    /// the default) means full strength — no depth cap, no time reduction.
    /// Below 20, `iterative_deepening()` caps `max_depth` via
    /// `skill::skill_depth_cap()` and `cmd_go`/`allocate_time()` reduce the
    /// time budget via `skill::skill_time_fraction_pct()` (Session 65: depth
    /// alone left a "instaflies then sits idle" gap on long time controls,
    /// so both are tier-driven together). A persistent session setting like
    /// `multipv` — NOT reset by `reset_for_search()`, since a Skill Level
    /// choice should carry across moves within a game, same as MultiPV.
    pub skill_level: u8,

    // ── Contempt ─────────────────────────────────────────────────────────────
    /// UCI `Contempt` setting, centipawns, default 0 (no change from prior
    /// behavior). Positive = this engine dislikes draws and will steer away
    /// from them when a genuinely better alternative exists; negative =
    /// this engine actively seeks draws. Applied via `draw_score()` at every
    /// draw-detection site in `alpha_beta` rather than stored per-node —
    /// see `draw_score()`'s doc comment for how root-relative sign works.
    /// Persistent across moves, same as `skill_level` above.
    pub contempt: i32,
}

impl SearchInfo {
    /// Create new SearchInfo with default values
    pub fn new() -> Self {
        SearchInfo {
            time_allocated_ms: 5000,
            start_time:        web_time::Instant::now(),
            stop:              false,
            print_info:        true,
            nodes:             0,
            nps:               0,
            killers:           [[Move::NULL; KILLER_COUNT]; MAX_PLY],
            history:           [[[0i32; 64]; 64]; 2],
            countermoves:      [[Move::NULL; 64]; 64],
            cont_hist:         Box::new([[[0i32; 64]; 12]; 64]),
            max_depth:         MAX_DEPTH as i32,
            node_limit:        0,
            pv_length:         [0; MAX_PLY],
            pv_table:          [[Move::NULL; MAX_PLY]; MAX_PLY],
            current_depth:     0,
            best_score:        -INFINITY,
            best_move:         Move::NULL,
            seldepth:          0,
            correction_history: crate::search::pruning::CorrectionHistory::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            ponder_hit_soft_ms: Arc::new(AtomicU64::new(u64::MAX)),
            ponder_hit_hard_ms: Arc::new(AtomicU64::new(u64::MAX)),
            multipv: 1,
            root_exclude: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            syzygy: None,
            skill_level: crate::search::skill::MAX_SKILL_LEVEL,
            contempt: 0,
        }
    }

    /// Create SearchInfo that shares an external stop flag.
    /// Used in Lazy SMP (Phase 13.4): all helper threads share one AtomicBool
    /// so the main thread terminating (or UCI `stop`) kills all helpers.
    pub fn new_with_stop(stop_flag: Arc<AtomicBool>) -> Self {
        let mut info = Self::new();
        info.stop_flag = stop_flag;
        info
    }

    /// Reset tables between searches (keep history across moves for better ordering)
    pub fn reset_for_search(&mut self) {
        self.stop        = false;
        self.nodes       = 0;
        self.nps         = 0;
        self.best_score  = -INFINITY;
        self.best_move   = Move::NULL;
        self.seldepth    = 0;
        self.start_time  = web_time::Instant::now();
        // Defense-in-depth: cmd_go already resets these on EngineState's
        // copies before every search, but a fresh search should never
        // inherit a stale override even if some future caller skips that.
        self.ponder_hit_soft_ms.store(u64::MAX, Ordering::Relaxed);
        self.ponder_hit_hard_ms.store(u64::MAX, Ordering::Relaxed);
        // A fresh search should never inherit a stale MultiPV exclusion
        // list from whatever the engine was doing before (multipv itself
        // is a persistent setting, not reset here — only the per-depth
        // exclusion state).
        self.root_exclude.clear();
        self.pv_length   = [0; MAX_PLY];
        self.pv_table    = [[Move::NULL; MAX_PLY]; MAX_PLY];
        self.killers     = [[Move::NULL; KILLER_COUNT]; MAX_PLY];
        // Note: history and countermoves kept between searches
        // They improve move ordering over multiple moves in the game

        // Cont hist is reset each search — it is position-dependent and
        // goes stale more quickly than regular history across moves.
        for row in self.cont_hist.iter_mut() {
            for col in row.iter_mut() {
                *col = [0i32; 64];
            }
        }
    }

    /// Age history scores (reduce by half between moves)
    /// Prevents old history from dominating current search
    pub fn age_history(&mut self) {
        for color in 0..2 {
            for from in 0..64 {
                for to in 0..64 {
                    self.history[color][from][to] /= 2;
                }
            }
        }
    }

    /// Is time up? Check periodically during search
    #[inline]
    pub fn is_time_up(&self) -> bool {
        if self.stop || self.stop_flag.load(Ordering::Relaxed) {
            return true;
        }
        // Ponder-hit override (Phase 18/D37): once a `ponderhit` has fired,
        // this is set to a real, finite deadline (still relative to this
        // search's own start_time) — use it instead of the effectively-
        // infinite time_allocated_ms a `go ponder` search started with.
        // u64::MAX = no override yet, fall through to the normal check.
        let hard_override = self.ponder_hit_hard_ms.load(Ordering::Relaxed);
        let effective_limit = if hard_override != u64::MAX {
            hard_override
        } else {
            self.time_allocated_ms
        };
        // Sampled every 256 nodes (was 2048) — Phase 16.6's NNUE-blended
        // eval is heavier per node than pure HCE, so the old interval could
        // let a single uninterrupted burst run well past the time budget
        // before the next check (see test_iterative_deepening_respects_time,
        // Session 33 CI failure: 881-node search never crossed a 2048
        // boundary, so no mid-search check ever fired). Instant::now() is
        // cheap enough that checking 8x more often has no measurable search
        // overhead.
        self.nodes & 255 == 0 && self.elapsed_ms() >= effective_limit
    }

    /// Milliseconds elapsed since search started
    #[inline]
    pub fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Update killer moves at a ply
    #[inline]
    pub fn update_killer(&mut self, mv: Move, ply: usize) {
        if ply >= MAX_PLY { return; }
        // Don't store duplicate
        if self.killers[ply][0] != mv {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = mv;
        }
    }

    /// Update history score for a move that caused a beta cutoff
    #[inline]
    pub fn update_history(
        &mut self,
        color: usize,
        from:  usize,
        to:    usize,
        depth: i32,
        good:  bool,
    ) {
        let bonus = if good { depth * depth } else { -(depth * depth) };
        let entry = &mut self.history[color][from][to];
        // Gravity formula — prevents overflow
        *entry += bonus - (*entry * bonus.abs() / 16384);
    }

    /// Update countermove for opponent's last move
    #[inline]
    pub fn update_countermove(&mut self, prev_from: usize, prev_to: usize, mv: Move) {
        self.countermoves[prev_from][prev_to] = mv;
    }

    /// Get countermove for opponent's last move
    #[inline]
    pub fn get_countermove(&self, prev_from: usize, prev_to: usize) -> Move {
        self.countermoves[prev_from][prev_to]
    }

    /// Get continuation history score conditioned on previous move destination
    /// and the currently-moving piece type.
    #[inline]
    pub fn get_cont_hist(&self, prev_to: usize, piece_idx: usize, to: usize) -> i32 {
        self.cont_hist[prev_to][piece_idx][to]
    }

    /// Update continuation history with gravity formula (prevents overflow).
    /// `good = true` → bonus, `good = false` → penalty.
    #[inline]
    pub fn update_cont_hist(
        &mut self,
        prev_to:   usize,
        piece_idx: usize,
        to:        usize,
        depth:     i32,
        good:      bool,
    ) {
        let bonus = if good { depth * depth } else { -(depth * depth) };
        let entry = &mut self.cont_hist[prev_to][piece_idx][to];
        *entry += bonus - (*entry * bonus.abs() / 16384);
    }

    /// Update PV table at a ply
    #[inline]
    pub fn update_pv(&mut self, mv: Move, ply: usize) {
        if ply >= MAX_PLY { return; }
        self.pv_table[ply][0] = mv;
        let next_len = if ply + 1 < MAX_PLY {
            self.pv_length[ply + 1]
        } else {
            0
        };
        for i in 0..next_len {
            if i + 1 < MAX_PLY {
                self.pv_table[ply][i + 1] = self.pv_table[ply + 1][i];
            }
        }
        self.pv_length[ply] = 1 + next_len;
    }

    /// Get the principal variation as a vector of moves
    pub fn get_pv(&self) -> Vec<Move> {
        self.pv_table[0][..self.pv_length[0]]
            .iter()
            .copied()
            .filter(|&m| m != Move::NULL)
            .collect()
    }
}

impl Default for SearchInfo {
    fn default() -> Self {
        Self::new()
    }
}

// ── Search result ─────────────────────────────────────────────────────────────

/// Result returned from a completed search
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Best move found
    pub best_move:  Move,
    /// Score for the best move (centipawns, from side to move perspective)
    pub score:      i32,
    /// Depth searched to
    pub depth:      i32,
    /// Maximum selective depth reached
    pub seldepth:   usize,
    /// Total nodes searched
    pub nodes:      u64,
    /// Time taken in milliseconds
    pub time_ms:    u64,
    /// Nodes per second
    pub nps:        u64,
    /// Principal variation
    pub pv:         Vec<Move>,
    /// Is the score a forced mate?
    pub is_mate:    bool,
    /// Mate in N moves (if is_mate)
    pub mate_in:    i32,
    /// Which MultiPV line this is (1-indexed, per UCI convention). Always
    /// 1 when MultiPV is at its default of 1 — this field exists so the
    /// `info` line format matches standard UCI even in the common case
    /// (Phase 19).
    pub multipv:    usize,
}

impl SearchResult {
    /// Format score for UCI output
    pub fn score_string(&self) -> String {
        if self.is_mate {
            format!("mate {}", self.mate_in)
        } else {
            format!("cp {}", self.score)
        }
    }

    /// Format as UCI info string
    pub fn to_uci_info(&self) -> String {
        let pv_str: Vec<String> = self.pv.iter()
            .map(|m| m.to_uci())
            .collect();
        format!(
            "info depth {} seldepth {} multipv {} score {} nodes {} nps {} time {} pv {}",
            self.depth,
            self.seldepth,
            self.multipv,
            self.score_string(),
            self.nodes,
            self.nps,
            self.time_ms,
            pv_str.join(" "),
        )
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Is this score a mate score?
#[inline]
pub fn is_mate_score(score: i32) -> bool {
    score.abs() >= MATE_THRESHOLD
}

/// Convert mate score to mate-in-N (positive = we are mating)
#[inline]
pub fn mate_in(score: i32) -> i32 {
    if score > 0 {
        (MATE_SCORE - score + 1) / 2
    } else {
        -(MATE_SCORE + score + 1) / 2
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Move, MoveKind, Square};

    #[test]
    fn test_score_constants() {
        assert!(INFINITY > MATE_SCORE);
        assert!(MATE_SCORE > MATE_THRESHOLD);
        assert_eq!(DRAW_SCORE, 0);
    }

    #[test]
    fn test_draw_score_zero_contempt_matches_draw_score() {
        // Default (contempt = 0) must be byte-identical to plain DRAW_SCORE
        // at every ply — this is what every pre-Contempt test still relies on.
        assert_eq!(draw_score(0, 0), DRAW_SCORE);
        assert_eq!(draw_score(1, 0), DRAW_SCORE);
        assert_eq!(draw_score(7, 0), DRAW_SCORE);
    }

    #[test]
    fn test_draw_score_positive_contempt_penalizes_root_side_draw() {
        // ply 0, 2, 4... = root side to move. Positive contempt = root
        // side dislikes draws, so a draw here scores WORSE than neutral.
        assert_eq!(draw_score(0, 30), DRAW_SCORE - 30);
        assert_eq!(draw_score(2, 30), DRAW_SCORE - 30);
    }

    #[test]
    fn test_draw_score_positive_contempt_rewards_opponent_side_draw() {
        // ply 1, 3, 5... = opponent to move. From THEIR perspective, the
        // root side's dislike of the draw is a positive (good for them).
        assert_eq!(draw_score(1, 30), DRAW_SCORE + 30);
        assert_eq!(draw_score(3, 30), DRAW_SCORE + 30);
    }

    #[test]
    fn test_draw_score_negative_contempt_seeks_draws() {
        // Negative contempt = this engine actively wants a draw, so at a
        // root-side-to-move node a draw should score BETTER than neutral.
        assert_eq!(draw_score(0, -40), DRAW_SCORE + 40);
        assert_eq!(draw_score(1, -40), DRAW_SCORE - 40);
    }

    #[test]
    fn test_is_mate_score() {
        assert!(is_mate_score(MATE_SCORE));
        assert!(is_mate_score(-MATE_SCORE));
        assert!(is_mate_score(MATE_THRESHOLD + 1));
        assert!(!is_mate_score(100));
        assert!(!is_mate_score(-500));
        assert!(!is_mate_score(MATE_THRESHOLD - 1));
    }

    #[test]
    fn test_mate_in() {
        // Mate in 1: score = MATE_SCORE - 0 (found at ply 1)
        let mate_in_1 = MATE_SCORE - 1;
        assert_eq!(mate_in(mate_in_1), 1);

        // Mate in 3
        let mate_in_3 = MATE_SCORE - 5;
        assert_eq!(mate_in(mate_in_3), 3);
    }

    #[test]
    fn test_search_info_creation() {
        let info = SearchInfo::new();
        assert!(!info.stop);
        assert_eq!(info.nodes, 0);
        assert_eq!(info.best_move, Move::NULL);
    }

    #[test]
    fn test_killer_update() {
        let mut info = SearchInfo::new();
        let mv1 = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        let mv2 = Move::new(Square::D2, Square::D4, MoveKind::DoublePush);

        info.update_killer(mv1, 3);
        assert_eq!(info.killers[3][0], mv1);

        info.update_killer(mv2, 3);
        assert_eq!(info.killers[3][0], mv2);
        assert_eq!(info.killers[3][1], mv1);
    }

    #[test]
    fn test_killer_no_duplicate() {
        let mut info = SearchInfo::new();
        let mv = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);

        info.update_killer(mv, 0);
        info.update_killer(mv, 0); // Same move again
        assert_eq!(info.killers[0][0], mv);
        assert_eq!(info.killers[0][1], Move::NULL); // Not duplicated
    }

    #[test]
    fn test_history_update() {
        let mut info = SearchInfo::new();
        info.update_history(0, 12, 28, 5, true);
        assert!(info.history[0][12][28] > 0,
            "History should be positive after good move");

        info.update_history(0, 12, 28, 5, false);
        // After penalty, score should decrease
    }

    #[test]
    fn test_pv_update() {
        let mut info = SearchInfo::new();
        let mv = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        info.update_pv(mv, 0);
        assert_eq!(info.pv_length[0], 1);
        assert_eq!(info.pv_table[0][0], mv);
    }

    #[test]
    fn test_search_result_score_string() {
        let result = SearchResult {
            best_move: Move::NULL,
            score:     150,
            depth:     10,
            seldepth:  14,
            nodes:     100_000,
            time_ms:   500,
            nps:       200_000,
            pv:        vec![],
            is_mate:   false,
            mate_in:   0,
            multipv:   1,
        };
        assert_eq!(result.score_string(), "cp 150");
    }

    #[test]
    fn test_search_result_mate_string() {
        let result = SearchResult {
            best_move: Move::NULL,
            score:     MATE_SCORE - 4,
            depth:     5,
            seldepth:  5,
            nodes:     500,
            time_ms:   10,
            nps:       50_000,
            pv:        vec![],
            is_mate:   true,
            mate_in:   3,
            multipv:   1,
        };
        assert_eq!(result.score_string(), "mate 3");
    }

    #[test]
    fn test_to_uci_info_includes_multipv() {
        let result = SearchResult {
            best_move: Move::NULL,
            score:     42,
            depth:     8,
            seldepth:  12,
            nodes:     1000,
            time_ms:   100,
            nps:       10_000,
            pv:        vec![],
            is_mate:   false,
            mate_in:   0,
            multipv:   3,
        };
        assert!(result.to_uci_info().contains("multipv 3"),
            "to_uci_info should report which MultiPV line this is");
    }

    #[test]
    fn test_multipv_and_root_exclude_defaults() {
        let info = SearchInfo::new();
        assert_eq!(info.multipv, 1,
            "MultiPV should default to 1 (single-PV, the common case)");
        assert!(info.root_exclude.is_empty(),
            "root_exclude should start empty");
    }

    #[test]
    fn test_root_exclude_cleared_by_reset_for_search() {
        let mut info = SearchInfo::new();
        info.root_exclude.push(Move::NULL);
        info.reset_for_search();
        assert!(info.root_exclude.is_empty(),
            "reset_for_search should clear any stale MultiPV exclusion list");
    }

    #[test]
    fn test_time_check() {
        let mut info = SearchInfo::new();
        info.time_allocated_ms = 100;
        // Freshly created — not time up
        assert!(!info.is_time_up());
        // Set stop flag
        info.stop = true;
        assert!(info.is_time_up());
    }

    #[test]
    fn test_ponder_hit_override_defaults_to_unset() {
        let info = SearchInfo::new();
        assert_eq!(info.ponder_hit_soft_ms.load(Ordering::Relaxed), u64::MAX,
            "fresh SearchInfo should have no ponder-hit override");
        assert_eq!(info.ponder_hit_hard_ms.load(Ordering::Relaxed), u64::MAX);
    }

    #[test]
    fn test_ponder_hit_override_ignored_when_unset() {
        let mut info = SearchInfo::new();
        // Simulate a "go ponder" search: huge time_allocated_ms, override unset.
        info.time_allocated_ms = u64::MAX / 2;
        assert!(!info.is_time_up(),
            "with no ponder-hit override, an infinite budget should never time out");
    }

    #[test]
    fn test_ponder_hit_hard_override_triggers_timeout() {
        let mut info = SearchInfo::new();
        // Simulate an active "go ponder" search (huge nominal budget)...
        info.time_allocated_ms = u64::MAX / 2;
        // ...that just received a ponderhit with an already-expired real budget.
        info.ponder_hit_hard_ms.store(0, Ordering::Relaxed);
        assert!(info.is_time_up(),
            "a ponder-hit hard override should take priority over the \
             original (huge) time_allocated_ms");
    }

    #[test]
    fn test_ponder_hit_override_reset_by_reset_for_search() {
        let mut info = SearchInfo::new();
        info.ponder_hit_soft_ms.store(123, Ordering::Relaxed);
        info.ponder_hit_hard_ms.store(456, Ordering::Relaxed);
        info.reset_for_search();
        assert_eq!(info.ponder_hit_soft_ms.load(Ordering::Relaxed), u64::MAX,
            "reset_for_search should clear any stale ponder-hit override");
        assert_eq!(info.ponder_hit_hard_ms.load(Ordering::Relaxed), u64::MAX);
    }
    
    #[test]
    fn test_cont_hist_update_get() {
        let mut info = SearchInfo::new();
        // prev_to=28 (e4), piece_idx=0 (white pawn), to=36 (e5)
        info.update_cont_hist(28, 0, 36, 5, true);
        assert!(info.get_cont_hist(28, 0, 36) > 0,
            "Cont hist should be positive after good move");
        // Apply penalty — should decrease
        let before = info.get_cont_hist(28, 0, 36);
        info.update_cont_hist(28, 0, 36, 5, false);
        assert!(info.get_cont_hist(28, 0, 36) < before,
            "Cont hist should decrease after penalty");
    }

    #[test]
    fn test_cont_hist_reset_on_search() {
        let mut info = SearchInfo::new();
        info.update_cont_hist(10, 3, 20, 6, true);
        assert!(info.get_cont_hist(10, 3, 20) > 0);
        info.reset_for_search();
        assert_eq!(info.get_cont_hist(10, 3, 20), 0,
            "Cont hist should be zeroed by reset_for_search");
    }
    
    #[test]
    fn test_skill_level_defaults_to_max() {
        let info = SearchInfo::new();
        assert_eq!(info.skill_level, crate::search::skill::MAX_SKILL_LEVEL,
            "Skill Level should default to full strength (20), matching \
             pre-Phase-20 behavior for every existing caller");
    }

    #[test]
    fn test_skill_level_survives_reset_for_search() {
        let mut info = SearchInfo::new();
        info.skill_level = 5;
        info.reset_for_search();
        assert_eq!(info.skill_level, 5,
            "Skill Level is a persistent session setting (like multipv) \
             and must survive reset_for_search(), not reset per-search");
    }

    #[test]
    fn test_age_history() {
        let mut info = SearchInfo::new();
        info.history[0][0][1] = 1000;
        info.history[1][5][10] = 2000;
        info.age_history();
        assert_eq!(info.history[0][0][1], 500);
        assert_eq!(info.history[1][5][10], 1000);
    }
}
