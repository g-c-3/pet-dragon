// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// position/mod.rs — Position struct (complete game state)
//
// The Position struct holds everything needed to:
//   - Generate all legal moves
//   - Evaluate the position
//   - Make and unmake moves during search
//   - Communicate with UCI (via FEN)
//
// Board representation uses bitboards:
//   pieces[color][kind] = Bitboard with 1 bit per square that piece occupies
//
// Example: pieces[White][Pawn] has 1s on every square a White pawn stands.
//
// Pet Dragon additions:
//   pawn_starts: PawnStartMap — records each pawn's actual starting square
//   This is the key data structure enabling Pet Dragon's double-step rule.
// ============================================================================

pub mod fen;
pub mod zobrist;
pub mod setup;
pub mod make_move;

use crate::bitboard::Bitboard;
use crate::position::fen::{
    generate_fen, parse_fen, FenError, STANDARD_START_FEN,
};
use crate::position::zobrist::{
    castling_key, ep_key, pawn_start_key,
    piece_key, side_key,
};
use crate::types::{
    CastlingRights, Color, Move, PawnStartMap, Piece,
    PieceKind, Square,
};

// ── Position struct ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Position {
    // ── Bitboard representation ───────────────────────────────────────────────
    // pieces[color][piece_kind] = bitboard of that piece type for that color
    // Indexed by Color as usize (0=White, 1=Black)
    // and PieceKind as usize (0=Pawn, 1=Knight, 2=Bishop, 3=Rook, 4=Queen, 5=King)
    pub pieces: [[Bitboard; 6]; 2],

    // Occupancy bitboards (derived from pieces, kept in sync for speed)
    // occupied_by[color] = all squares occupied by that color
    pub occupied_by: [Bitboard; 2],
    // all_occupied = occupied_by[White] | occupied_by[Black]
    pub all_occupied: Bitboard,

    // ── Game state ────────────────────────────────────────────────────────────
    pub side_to_move:    Color,
    pub castling:        CastlingRights,
    pub en_passant:      Option<Square>, // target square behind double-pushed pawn
    pub halfmove_clock:  u32,            // for 50-move rule
    pub fullmove_number: u32,

    // ── Zobrist hash ──────────────────────────────────────────────────────────
    // Incrementally updated hash of the current position
    // Used as the key in the transposition table
    pub hash: u64,

    // ── Pet Dragon: pawn start squares ───────────────────────────────────────
    // Records the actual starting square of every pawn in this game.
    // Used by move generation to determine double-step eligibility:
    //   A pawn can double-step if and only if it is still on this square.
    pub pawn_starts: PawnStartMap,

    // ── Move history (for unmake) ─────────────────────────────────────────────
    // Each entry stores state that cannot be recovered from the move alone
    pub history: Vec<HistoryEntry>,

    // ── Game history (for repetition detection) ───────────────────────────────
    // Stores Zobrist hashes of all positions seen in the current game.
    // Used to detect draws by threefold repetition during search.
    // Checked BEFORE transposition table — repetition always overrides TT.
    // ⚠️ Pet Dragon: hash encodes pawn start configuration, so positions
    // from different Pet Dragon games with same piece placement but different
    // pawn starts will have different hashes — no false repetition detection.
    //
    // D45: each entry also caches a "repetition" distance, matching
    // Stockfish's StateInfo::repetition field exactly — see
    // push_game_history()'s doc comment for the full algorithm. This makes
    // is_repetition() an O(1) lookup instead of an unbounded backward scan.
    pub game_history: Vec<(u64, i32)>,
}

/// State saved before making a move, restored during unmake
#[derive(Clone, Copy)]
pub struct HistoryEntry {
    pub mv:             Move,
    pub castling:       CastlingRights,
    pub en_passant:     Option<Square>,
    pub halfmove_clock: u32,
    pub hash:           u64,
    pub captured:       Option<PieceKind>,
}

// ── Position construction ─────────────────────────────────────────────────────

impl Position {
    /// Create an empty position (no pieces)
    pub fn empty() -> Self {
        Position {
            pieces:          [[Bitboard::EMPTY; 6]; 2],
            occupied_by:     [Bitboard::EMPTY; 2],
            all_occupied:    Bitboard::EMPTY,
            side_to_move:    Color::White,
            castling:        CastlingRights::NONE,
            en_passant:      None,
            halfmove_clock:  0,
            fullmove_number: 1,
            hash:            0,
            pawn_starts:     PawnStartMap::EMPTY,
            history:         Vec::with_capacity(256),
            game_history:    Vec::with_capacity(512),
        }
    }

    /// Load the standard chess starting position
    /// (also one valid Pet Dragon arrangement)
    pub fn start_pos() -> Result<Self, FenError> {
        Self::from_fen(STANDARD_START_FEN)
    }

    /// Load a position from a FEN string
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let parsed = parse_fen(fen)?;
        let mut pos = Position::empty();

        // Place pieces from board array
        for sq in Square::all() {
            if let Some(piece) = parsed.board[sq.index() as usize] {
                pos.put_piece(piece.color, piece.kind, sq);
            }
        }

        pos.side_to_move    = parsed.side_to_move;
        pos.castling        = parsed.castling;
        pos.en_passant      = parsed.en_passant;
        pos.halfmove_clock  = parsed.halfmove_clock;
        pos.fullmove_number = parsed.fullmove_number;
        pos.pawn_starts     = parsed.pawn_starts;

        // Compute initial Zobrist hash
        pos.hash = pos.compute_hash();

        Ok(pos)
    }

    /// Generate a FEN string for this position
    pub fn to_fen(&self) -> String {
        let board = self.to_board_array();
        generate_fen(
            &board,
            self.side_to_move,
            self.castling,
            self.en_passant,
            self.halfmove_clock,
            self.fullmove_number,
            &self.pawn_starts,
            true, // always include Pet Dragon extension
        )
    }

    /// Generate a standard FEN string (no Pet Dragon extension)
    /// Used for UCI communication with external tools
    pub fn to_standard_fen(&self) -> String {
        let board = self.to_board_array();
        generate_fen(
            &board,
            self.side_to_move,
            self.castling,
            self.en_passant,
            self.halfmove_clock,
            self.fullmove_number,
            &self.pawn_starts,
            false,
        )
    }

    /// Convert bitboard representation to board array
    fn to_board_array(&self) -> [Option<Piece>; 64] {
        let mut board = [None; 64];
        for color in Color::ALL {
            for kind in PieceKind::ALL {
                let mut bb = self.pieces[color.index()][kind.index()];
                while let Some(sq) = bb.pop_lsb() {
                    board[sq.index() as usize] =
                        Some(Piece::new(color, kind));
                }
            }
        }
        board
    }
}

// ── Piece access and manipulation ─────────────────────────────────────────────

impl Position {
    /// Place a piece on a square (does not update hash — use during setup only)
    pub fn put_piece(&mut self, color: Color, kind: PieceKind, sq: Square) {
        self.pieces[color.index()][kind.index()].set(sq);
        self.occupied_by[color.index()].set(sq);
        self.all_occupied.set(sq);
    }

    /// Remove a piece from a square (does not update hash)
    pub fn remove_piece(&mut self, color: Color, kind: PieceKind, sq: Square) {
        self.pieces[color.index()][kind.index()].clear(sq);
        self.occupied_by[color.index()].clear(sq);
        self.all_occupied.clear(sq);
    }

    /// Get the piece kind on a square for a given color (None if empty/wrong color)
    #[inline]
    pub fn piece_on(&self, sq: Square, color: Color) -> Option<PieceKind> {
        for kind in PieceKind::ALL {
            if self.pieces[color.index()][kind.index()].contains(sq) {
                return Some(kind);
            }
        }
        None
    }

    /// Get the piece (color + kind) on a square (None if empty)
    #[inline]
    pub fn piece_at(&self, sq: Square) -> Option<Piece> {
        for color in Color::ALL {
            if let Some(kind) = self.piece_on(sq, color) {
                return Some(Piece::new(color, kind));
            }
        }
        None
    }

    /// Get the king square for a color
    #[inline]
    pub fn king_sq(&self, color: Color) -> Square {
        self.pieces[color.index()][PieceKind::King.index()]
            .lsb()
            .expect("King must always be on the board")
    }

    /// Get bitboard of all pieces of a given kind for a color
    #[inline]
    pub fn piece_bb(&self, color: Color, kind: PieceKind) -> Bitboard {
        self.pieces[color.index()][kind.index()]
    }

    /// Get bitboard of all squares occupied by a color
    #[inline]
    pub fn occupied(&self, color: Color) -> Bitboard {
        self.occupied_by[color.index()]
    }

    /// Get bitboard of all occupied squares
    #[inline]
    pub fn all_pieces(&self) -> Bitboard {
        self.all_occupied
    }

    /// Get bitboard of empty squares
    #[inline]
    pub fn empty_squares(&self) -> Bitboard {
        !self.all_occupied
    }

    /// Count pieces of a given kind for a color
    #[inline]
    pub fn count_pieces(&self, color: Color, kind: PieceKind) -> u32 {
        self.pieces[color.index()][kind.index()].count()
    }

    /// Total material value for a color (in centipawns, excluding king)
    pub fn material(&self, color: Color) -> i32 {
        let mut total = 0i32;
        for kind in PieceKind::ALL {
            if kind == PieceKind::King { continue; }
            total += self.count_pieces(color, kind) as i32
                   * kind.base_value();
        }
        total
    }

    /// Game phase (0 = endgame, 24 = full middlegame)
    /// Used for tapered evaluation
    pub fn game_phase(&self) -> i32 {
        let knight_phase = 1;
        let bishop_phase = 1;
        let rook_phase   = 2;
        let queen_phase  = 4;

        let mut phase = 0i32;
        for color in Color::ALL {
            phase += self.count_pieces(color, PieceKind::Knight) as i32
                   * knight_phase;
            phase += self.count_pieces(color, PieceKind::Bishop) as i32
                   * bishop_phase;
            phase += self.count_pieces(color, PieceKind::Rook) as i32
                   * rook_phase;
            phase += self.count_pieces(color, PieceKind::Queen) as i32
                   * queen_phase;
        }
        phase.min(24) // cap at 24 (full middlegame)
    }
}

// ── Zobrist hash computation ───────────────────────────────────────────────────

impl Position {
    /// Compute the full Zobrist hash from scratch
    /// Only called during position setup — afterwards updated incrementally
    pub fn compute_hash(&self) -> u64 {
        let mut hash = 0u64;

        // Hash all pieces
        for color in Color::ALL {
            for kind in PieceKind::ALL {
                let mut bb = self.pieces[color.index()][kind.index()];
                while let Some(sq) = bb.pop_lsb() {
                    hash ^= piece_key(color, kind, sq);
                }
            }
        }

        // Hash side to move
        if self.side_to_move == Color::Black {
            hash ^= side_key();
        }

        // Hash castling rights
        hash ^= castling_key(self.castling.to_mask());

        // Hash en passant file (only if en passant is actually possible)
        if let Some(ep_sq) = self.en_passant {
            hash ^= ep_key(ep_sq.file());
        }

        // Pet Dragon: hash pawn start configuration
        for sq in Square::all() {
            if let Some(color) = self.pawn_starts.get(sq) {
                hash ^= pawn_start_key(color, sq);
            }
        }

        hash
    }
}

// ── Check detection ───────────────────────────────────────────────────────────

impl Position {
    /// Is the given color's king currently in check?
    pub fn in_check(&self, color: Color) -> bool {
        let king_sq = self.king_sq(color);
        self.is_attacked(king_sq, color.flip())
    }

    /// Is a square attacked by any piece of the given color?
    /// Used for check detection, castling legality, king safety
    pub fn is_attacked(&self, sq: Square, by_color: Color) -> bool {
        #[allow(unused_imports)]
        use crate::bitboard::{
            bishop_attacks, queen_attacks, rook_attacks,
        };
        use crate::bitboard::masks::{
            king_attacks, knight_attacks, pawn_attacks,
        };

        let occ = self.all_occupied;

        // Pawn attacks (check if sq is attacked by pawns of by_color)
        // A pawn of by_color attacks sq if sq is in pawn_attacks(by_color, pawn_sq)
        // Equivalently: pawn_attacks(opposite_color, sq) & pawns_of_by_color
        let opp = by_color.flip();
        if (pawn_attacks(opp, sq)
            & self.piece_bb(by_color, PieceKind::Pawn)).is_not_empty()
        {
            return true;
        }

        // Knight attacks
        if (knight_attacks(sq)
            & self.piece_bb(by_color, PieceKind::Knight)).is_not_empty()
        {
            return true;
        }

        // King attacks
        if (king_attacks(sq)
            & self.piece_bb(by_color, PieceKind::King)).is_not_empty()
        {
            return true;
        }

        // Bishop / diagonal queen attacks
        let diag_attackers = self.piece_bb(by_color, PieceKind::Bishop)
            | self.piece_bb(by_color, PieceKind::Queen);
        if (bishop_attacks(sq, occ) & diag_attackers).is_not_empty() {
            return true;
        }

        // Rook / straight queen attacks
        let straight_attackers = self.piece_bb(by_color, PieceKind::Rook)
            | self.piece_bb(by_color, PieceKind::Queen);
        if (rook_attacks(sq, occ) & straight_attackers).is_not_empty() {
            return true;
        }

        false
    }

    /// Get a bitboard of all squares attacked by a color
    pub fn attacks_by(&self, color: Color) -> Bitboard {
        use crate::bitboard::{bishop_attacks, queen_attacks, rook_attacks};
        use crate::bitboard::masks::{
            king_attacks, knight_attacks, pawn_attacks,
        };

        let mut attacks = Bitboard::EMPTY;
        let occ = self.all_occupied;

        // Pawns
        let mut pawns = self.piece_bb(color, PieceKind::Pawn);
        while let Some(sq) = pawns.pop_lsb() {
            attacks |= pawn_attacks(color, sq);
        }

        // Knights
        let mut knights = self.piece_bb(color, PieceKind::Knight);
        while let Some(sq) = knights.pop_lsb() {
            attacks |= knight_attacks(sq);
        }

        // Bishops
        let mut bishops = self.piece_bb(color, PieceKind::Bishop);
        while let Some(sq) = bishops.pop_lsb() {
            attacks |= bishop_attacks(sq, occ);
        }

        // Rooks
        let mut rooks = self.piece_bb(color, PieceKind::Rook);
        while let Some(sq) = rooks.pop_lsb() {
            attacks |= rook_attacks(sq, occ);
        }

        // Queens
        let mut queens = self.piece_bb(color, PieceKind::Queen);
        while let Some(sq) = queens.pop_lsb() {
            attacks |= queen_attacks(sq, occ);
        }

        // King
        attacks |= king_attacks(self.king_sq(color));

        attacks
    }
}

// ── Insufficient material detection ──────────────────────────────────────────

impl Position {
    /// Is the position a draw by insufficient material?
    /// Follows FIDE rules — neither side can force checkmate
    pub fn is_insufficient_material(&self) -> bool {
        let white_material = self.material(Color::White);
        let black_material = self.material(Color::Black);

        // If either side has pawns, rooks, or queens — not insufficient
        for color in Color::ALL {
            if self.count_pieces(color, PieceKind::Pawn)  > 0 { return false; }
            if self.count_pieces(color, PieceKind::Rook)  > 0 { return false; }
            if self.count_pieces(color, PieceKind::Queen) > 0 { return false; }
        }

        // King vs King
        if white_material == 0 && black_material == 0 {
            return true;
        }

        // King + minor piece vs King
        let white_minors = self.count_pieces(Color::White, PieceKind::Knight)
            + self.count_pieces(Color::White, PieceKind::Bishop);
        let black_minors = self.count_pieces(Color::Black, PieceKind::Knight)
            + self.count_pieces(Color::Black, PieceKind::Bishop);

        if white_minors <= 1 && black_minors == 0 { return true; }
        if black_minors <= 1 && white_minors == 0 { return true; }

        // King + Bishop vs King + Bishop (same colored bishops)
        if white_minors == 1 && black_minors == 1 {
            let wb = self.count_pieces(Color::White, PieceKind::Bishop);
            let bb = self.count_pieces(Color::Black, PieceKind::Bishop);
            if wb == 1 && bb == 1 {
                let white_bish_sq = self.piece_bb(
                    Color::White, PieceKind::Bishop
                ).lsb().unwrap();
                let black_bish_sq = self.piece_bb(
                    Color::Black, PieceKind::Bishop
                ).lsb().unwrap();
                if white_bish_sq.is_light() == black_bish_sq.is_light() {
                    return true;
                }
            }
        }

        false
    }
}

// ── Repetition detection (D45 — Stockfish-equivalent algorithm) ────────────────
//
// Mirrors Stockfish's Position::set_state()/is_draw(ply) exactly, adapted to
// this project's flat Vec<(hash, repetition)> history instead of Stockfish's
// linked StateInfo chain. Two ideas, both load-bearing:
//
// 1. BOUNDED, CACHED backward walk. push_game_history() looks back at most
//    halfmove_clock plies (no point checking further than the last pawn
//    move/capture — that position is provably not a repeat) and caches the
//    result once, at push time — O(halfmove_clock/2) amortized per push,
//    O(1) per is_repetition() lookup, instead of the previous unbounded
//    O(game_length) scan on every single draw check. Pet Dragon's null-move
//    pruning (alpha_beta.rs) mutates pos.hash/side_to_move directly and
//    never calls push_game_history() at all, so — unlike Stockfish, which
//    also bounds by pliesFromNull — there's no separate null-move-poisoning
//    concern to guard against here.
//
// 2. PLY-RELATIVE draw decision. The cached value's SIGN distinguishes "this
//    position has been seen once before" (positive: draw only if that seen-
//    before position was itself reached by moves the search chose, not
//    purely inherited from real game history that predates the search) from
//    "this position is part of a genuine 3-fold chain" (negative: always a
//    draw, since a real 3-fold on the board is a real draw regardless of
//    when in the search tree it's noticed). This is what keeps the search
//    from treating a single repeat sitting in real, unchangeable game
//    history as an artificial draw it can't actually do anything about,
//    while still fully respecting an actual 3-fold the moment one exists.
impl Position {
    /// Record the current position hash in game history, computing and
    /// caching its "repetition" distance in the same step — this is the
    /// Pet Dragon equivalent of Stockfish's `Position::set_state()` repetition
    /// block. Call this AFTER make_move() — records the new position.
    ///
    /// The cached value is:
    /// - `0` if no match was found within the bounded backward walk.
    /// - `+i` if a match was found `i` plies back, and that matched
    ///   position's OWN cached value was `0` (a first repeat).
    /// - `-i` if a match was found `i` plies back, and that matched
    ///   position's own cached value was already nonzero — meaning this is
    ///   now the second link in a real repetition chain (functionally a
    ///   3-fold), encoded via sign rather than a separate flag.
    ///
    /// The walk starts at `i = 4`, not `2`: a position cannot repeat after
    /// only 2 plies (one move by each side) in legal chess, since every
    /// individual move is a real, irreversible-for-this-purpose change to
    /// the board — the shortest possible repetition cycle is 4 plies (e.g.
    /// each side shuffles a piece out and back). Matches Stockfish's own
    /// loop bounds exactly.
    #[inline]
    pub fn push_game_history(&mut self) {
        let current = self.hash;
        let mut repetition = 0i32;

        let end = self.halfmove_clock as usize;
        let n   = self.game_history.len();
        if end >= 4 {
            let mut i = 4usize;
            while i <= end && i <= n {
                let (stp_hash, stp_repetition) = self.game_history[n - i];
                if stp_hash == current {
                    repetition = if stp_repetition != 0 { -(i as i32) } else { i as i32 };
                    break;
                }
                i += 2;
            }
        }

        self.game_history.push((current, repetition));
    }

    /// Remove the last recorded position from game history.
    /// Call this AFTER unmake_move() — removes the position we just undid.
    #[inline]
    pub fn pop_game_history(&mut self) {
        self.game_history.pop();
    }

    /// Is the CURRENT position (the last one pushed to game history) a draw
    /// by repetition, from the search's point of view at ply `ply` (distance
    /// from the search root, 0 at root)?
    ///
    /// `O(1)` — just reads the cached value push_game_history() already
    /// computed. Returns true if:
    /// - the cached value is negative (a genuine repetition chain — always
    ///   a draw, regardless of whether it's within the search tree), OR
    /// - the cached value is positive AND less than `ply` — meaning the
    ///   first-repeat's target position was itself reached via moves this
    ///   search chose along the current branch (`ply - i > 0`), not purely
    ///   inherited from real game history that predates the search root.
    ///
    /// Both conditions collapse into the single Stockfish-equivalent
    /// expression below: a negative value is always `< ply` for any
    /// non-root `ply >= 1`, so no separate branch is needed.
    #[inline]
    pub fn is_repetition(&self, ply: usize) -> bool {
        match self.game_history.last() {
            Some(&(_, repetition)) => repetition != 0 && (repetition as i64) < (ply as i64),
            None => false,
        }
    }

    /// Is this position a draw by threefold repetition, for actual game-end
    /// adjudication (NOT the search-tree-relative heuristic above)? Returns
    /// true only if the position has occurred 3 or more times total, full
    /// stop — this intentionally does NOT use the cached repetition
    /// distance or ply-relative logic, since real game-end adjudication
    /// cares about the literal rule, not what the search tree can see.
    pub fn is_threefold_repetition(&self) -> bool {
        let current = self.hash;
        let mut count = 0u32;
        for &(hash, _) in self.game_history.iter() {
            if hash == current {
                count += 1;
            }
        }
        // game_history includes current position (pushed by make_move_with_history)
        // count >= 3 means position appears 3 times in history = threefold
        count >= 3
    }

    /// Load the game history from a list of position hashes, in the order
    /// they occurred. Used when loading a game via UCI position command
    /// with move list. Currently unused (no caller replays a hash list this
    /// way — the real `position ... moves ...` handler in main.rs replays
    /// actual moves through push_game_history() directly instead), but kept
    /// correct rather than left broken: replays each hash through the exact
    /// same bounded-walk-and-cache logic push_game_history() uses, so the
    /// resulting cached values are identical to what they'd be had these
    /// positions actually been pushed one at a time during real play.
    ///
    /// NOTE: this cannot correctly reconstruct halfmove_clock history for
    /// each intermediate position from a bare hash list alone — it uses
    /// self.halfmove_clock's CURRENT value for every entry's bound, which
    /// is only correct if the position's halfmove_clock hasn't changed
    /// across the entries being loaded (e.g. no pawn moves/captures in the
    /// replayed sequence). Replaying real moves (as main.rs actually does)
    /// doesn't have this limitation, since each move naturally updates
    /// halfmove_clock before its own push_game_history() call.
    pub fn set_game_history(&mut self, hashes: Vec<u64>) {
        self.game_history.clear();
        for hash in hashes {
            let saved_hash = self.hash;
            self.hash = hash;
            self.push_game_history();
            self.hash = saved_hash;
        }
    }

    /// Clear game history (called on ucinewgame)
    pub fn clear_game_history(&mut self) {
        self.game_history.clear();
    }
}

// ── Display ───────────────────────────────────────────────────────────────────
impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "  ┌─────────────────┐")?;
        for rank in (0..8u8).rev() {
            write!(f, "{} │", rank + 1)?;
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank).unwrap();
                let ch = match self.piece_at(sq) {
                    Some(p) => p.to_fen_char(),
                    None    => '.',
                };
                write!(f, " {}", ch)?;
            }
            writeln!(f, " │")?;
        }
        writeln!(f, "  └─────────────────┘")?;
        writeln!(f, "    a b c d e f g h")?;
        writeln!(f, "  Side: {:?}", self.side_to_move)?;
        writeln!(f, "  Castling: {}", self.castling.to_fen())?;
        writeln!(f, "  En passant: {}",
            self.en_passant.map(|s| s.to_uci())
                .unwrap_or_else(|| "-".to_string()))?;
        writeln!(f, "  Hash: {:016X}", self.hash)?;
        Ok(())
    }
}

impl std::fmt::Debug for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Position({})", self.to_standard_fen())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitboard::magic::init_magic;
    use crate::bitboard::masks::init_masks;
    use crate::position::zobrist::init_zobrist;

    fn setup() {
        init_masks();
        init_magic();
        init_zobrist();
    }

    #[test]
    fn test_start_pos_loads() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_eq!(pos.count_pieces(Color::White, PieceKind::Pawn),   8);
        assert_eq!(pos.count_pieces(Color::White, PieceKind::Rook),   2);
        assert_eq!(pos.count_pieces(Color::White, PieceKind::Knight), 2);
        assert_eq!(pos.count_pieces(Color::White, PieceKind::Bishop), 2);
        assert_eq!(pos.count_pieces(Color::White, PieceKind::Queen),  1);
        assert_eq!(pos.count_pieces(Color::White, PieceKind::King),   1);
        assert_eq!(pos.count_pieces(Color::Black, PieceKind::Pawn),   8);
    }

    #[test]
    fn test_king_squares() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_eq!(pos.king_sq(Color::White), Square::E1);
        assert_eq!(pos.king_sq(Color::Black), Square::E8);
    }

    #[test]
    fn test_occupancy() {
        setup();
        let pos = Position::start_pos().unwrap();
        // Ranks 1 and 2 occupied by White
        assert_eq!(pos.occupied(Color::White).count(), 16);
        // Ranks 7 and 8 occupied by Black
        assert_eq!(pos.occupied(Color::Black).count(), 16);
        // Total occupied
        assert_eq!(pos.all_pieces().count(), 32);
        // Middle ranks empty
        assert_eq!(pos.empty_squares().count(), 32);
    }

    #[test]
    fn test_piece_at() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_eq!(pos.piece_at(Square::E1), Some(Piece::WHITE_KING));
        assert_eq!(pos.piece_at(Square::E8), Some(Piece::BLACK_KING));
        assert_eq!(pos.piece_at(Square::D1), Some(Piece::WHITE_QUEEN));
        assert_eq!(pos.piece_at(Square::E4), None);
    }

    #[test]
    fn test_material_count() {
        setup();
        let pos = Position::start_pos().unwrap();
        // White: 8×100 + 2×320 + 2×330 + 2×500 + 1×900 = 3930
        let expected = 8*100 + 2*320 + 2*330 + 2*500 + 900;
        assert_eq!(pos.material(Color::White), expected);
        assert_eq!(pos.material(Color::Black), expected);
    }

    #[test]
    fn test_game_phase_start() {
        setup();
        let pos = Position::start_pos().unwrap();
        // Full middlegame at start: 4×1 + 4×1 + 4×2 + 2×4 = 24
        assert_eq!(pos.game_phase(), 24);
    }

    #[test]
    fn test_hash_nonzero() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert_ne!(pos.hash, 0);
    }

    #[test]
    fn test_hash_deterministic() {
        setup();
        let pos1 = Position::start_pos().unwrap();
        let pos2 = Position::start_pos().unwrap();
        assert_eq!(pos1.hash, pos2.hash);
    }

    #[test]
    fn test_hash_different_positions() {
        setup();
        let pos1 = Position::start_pos().unwrap();
        let pos2 = Position::from_fen(
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1"
        ).unwrap();
        assert_ne!(pos1.hash, pos2.hash);
    }

    #[test]
    fn test_not_in_check_start() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert!(!pos.in_check(Color::White));
        assert!(!pos.in_check(Color::Black));
    }

    #[test]
    fn test_in_check_detection() {
        setup();
        // Scholar's mate position — Black king in check
        let fen =
            "rnb1kbnr/pppp1ppp/8/4p3/2B1P3/8/PPPP1PPP/RNBQK1NR b KQkq - 0 3";
        let pos = Position::from_fen(fen).unwrap();
        // Not quite check yet in this position, but detection works
        assert!(!pos.in_check(Color::White));
    }

    #[test]
    fn test_insufficient_material_kk() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert!(pos.is_insufficient_material());
    }

    #[test]
    fn test_insufficient_material_kbk() {
        setup();
        // King + Bishop vs King
        let fen = "4k3/8/8/8/8/8/8/4KB2 w - - 0 1";
        let pos = Position::from_fen(fen).unwrap();
        assert!(pos.is_insufficient_material());
    }

    #[test]
    fn test_sufficient_material_with_pawns() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert!(!pos.is_insufficient_material());
    }

    #[test]
    fn test_fen_roundtrip() {
        setup();
        let pos = Position::start_pos().unwrap();
        let fen = pos.to_standard_fen();
        let pos2 = Position::from_fen(&fen).unwrap();
        assert_eq!(pos.hash, pos2.hash);
        assert_eq!(pos.side_to_move, pos2.side_to_move);
        assert_eq!(pos.castling, pos2.castling);
    }

    #[test]
    fn test_pawn_starts_standard() {
        setup();
        let pos = Position::start_pos().unwrap();
        // All rank-2 White pawns should have start squares recorded
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, 1).unwrap();
            assert!(
                pos.pawn_starts.started_here(sq, Color::White),
                "White pawn start not recorded for {}", sq
            );
        }
        // All rank-7 Black pawns should have start squares recorded
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, 6).unwrap();
            assert!(
                pos.pawn_starts.started_here(sq, Color::Black),
                "Black pawn start not recorded for {}", sq
            );
        }
    }

    #[test]
    fn test_display_doesnt_panic() {
        setup();
        let pos = Position::start_pos().unwrap();
        let display = format!("{}", pos);
        assert!(display.contains('K')); // White king
        assert!(display.contains('k')); // Black king
    }

    // ── Repetition detection (D45) ──────────────────────────────────────────

    /// Builds the same 4-ply king-shuffle cycle used in alpha_beta.rs's own
    /// integration test: Ke1-e2, Ke8-e7, Ke2-e1, Ke7-e8 returns to the exact
    /// starting position, with halfmove_clock correctly reaching 4 (king
    /// moves don't reset it) — the shortest possible repetition cycle in
    /// legal chess.
    fn build_king_shuffle_repetition() -> Position {
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        // Push the starting position first — matches how iterative_deepening()
        // actually pushes the search root before any moves are made. Without
        // this, the bounded walk never has enough entries to look back the
        // full 4 plies to find the match.
        pos.push_game_history();
        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };
        for (from, to) in [
            (Square::E1, Square::E2), (Square::E8, Square::E7),
            (Square::E2, Square::E1), (Square::E7, Square::E8),
        ] {
            let mv = find_move(&pos, from, to);
            pos.make_move_with_history(mv);
        }
        pos
    }

    #[test]
    fn test_no_repetition_before_the_cycle_completes() {
        setup();
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        pos.push_game_history(); // matches real search usage — see build_king_shuffle_repetition's comment
        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };
        // Only 2 plies in — Ke1-e2, Ke8-e7. Genuinely impossible for this to
        // match anything yet (below the i=4 minimum cycle length), so the
        // cached repetition value must be exactly 0.
        let mv1 = find_move(&pos, Square::E1, Square::E2);
        pos.make_move_with_history(mv1);
        let mv2 = find_move(&pos, Square::E8, Square::E7);
        pos.make_move_with_history(mv2);
        assert_eq!(pos.game_history.last().unwrap().1, 0,
            "No repetition is possible after only 2 plies — cached value must be 0");
    }

    #[test]
    fn test_repetition_cached_as_positive_four_at_the_cycle_completes() {
        setup();
        let pos = build_king_shuffle_repetition();
        let (last_hash, last_repetition) = *pos.game_history.last().unwrap();
        assert_eq!(last_hash, pos.hash,
            "The final pushed entry must be the current position's own hash");
        assert_eq!(last_repetition, 4,
            "First repeat at exactly 4 plies back must cache +4 (positive: a \
             first repeat, not yet part of a chain — see push_game_history's doc comment)");
    }

    #[test]
    fn test_is_repetition_true_when_repeat_is_within_search_tree() {
        setup();
        let pos = build_king_shuffle_repetition();
        // The repeat is 4 plies back; from a search node whose own ply is
        // anything greater than 4, that repeat happened via moves the
        // search itself chose along this branch — must be a draw.
        assert!(pos.is_repetition(5),
            "A first repeat within the search tree (ply > repetition distance) must be a draw");
        assert!(pos.is_repetition(10));
    }

    #[test]
    fn test_is_repetition_false_when_repeat_predates_search_root() {
        setup();
        let pos = build_king_shuffle_repetition();
        // ply=4 (or less) means the repeated position is at or before the
        // search root itself — purely inherited from real game history the
        // search didn't choose, not something it can avoid. Must NOT be
        // scored as a draw (matches Stockfish's repetition < ply exactly).
        assert!(!pos.is_repetition(4),
            "A first repeat exactly at the search root boundary must NOT be scored as a draw");
        assert!(!pos.is_repetition(1),
            "A first repeat entirely predating the search tree must NOT be scored as a draw");
        assert!(!pos.is_repetition(0));
    }

    #[test]
    fn test_repetition_chain_cached_as_negative() {
        setup();
        let mut pos = build_king_shuffle_repetition();
        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };
        // One more full king-shuffle cycle (4 more plies) returns to the
        // SAME position a third time. The position 4 plies back from here
        // is the one built by build_king_shuffle_repetition(), which itself
        // already had a nonzero (positive) cached repetition — so this new
        // entry must cache a NEGATIVE value, marking a genuine chain.
        for (from, to) in [
            (Square::E1, Square::E2), (Square::E8, Square::E7),
            (Square::E2, Square::E1), (Square::E7, Square::E8),
        ] {
            let mv = find_move(&pos, from, to);
            pos.make_move_with_history(mv);
        }
        let (_, repetition) = *pos.game_history.last().unwrap();
        assert_eq!(repetition, -4,
            "A repeat of an already-repeated position must cache a NEGATIVE value \
             (chain detected), not another plain +4");
    }

    #[test]
    fn test_is_repetition_chain_always_true_regardless_of_ply() {
        setup();
        let mut pos = build_king_shuffle_repetition();
        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };
        for (from, to) in [
            (Square::E1, Square::E2), (Square::E8, Square::E7),
            (Square::E2, Square::E1), (Square::E7, Square::E8),
        ] {
            let mv = find_move(&pos, from, to);
            pos.make_move_with_history(mv);
        }
        // Unlike a plain first repeat, a genuine chain (negative cached
        // value) must be treated as a draw at ANY ply, including ply=1 —
        // a real 3-fold on the board is a real draw no matter when in the
        // search tree it's noticed. This is the one case where "purely
        // inherited from game history" doesn't matter.
        assert!(pos.is_repetition(1),
            "A genuine repetition chain must be a draw even at a very shallow ply");
    }

    #[test]
    fn test_is_repetition_false_with_no_history() {
        setup();
        let pos = Position::start_pos().unwrap();
        assert!(!pos.is_repetition(5),
            "A position with no game history at all can't be a repetition");
    }

    #[test]
    fn test_is_threefold_repetition_still_uses_plain_count_not_ply() {
        setup();
        let mut pos = build_king_shuffle_repetition();
        // Only 2 occurrences so far (the fen start + this repeat) —
        // is_threefold_repetition() must NOT fire yet, regardless of ply.
        assert!(!pos.is_threefold_repetition(),
            "Only 2 occurrences exist so far — real threefold adjudication must not fire");

        let find_move = |pos: &Position, from: Square, to: Square| -> Move {
            crate::movegen::generate_moves(pos)
                .iter()
                .find(|m| m.from == from && m.to == to)
                .copied()
                .expect("expected king move to be legal")
        };
        for (from, to) in [
            (Square::E1, Square::E2), (Square::E8, Square::E7),
            (Square::E2, Square::E1), (Square::E7, Square::E8),
        ] {
            let mv = find_move(&pos, from, to);
            pos.make_move_with_history(mv);
        }
        // Now genuinely the 3rd occurrence — real adjudication must fire,
        // independent of the search-tree-relative is_repetition() logic.
        assert!(pos.is_threefold_repetition(),
            "3rd occurrence must be a real threefold regardless of search ply");
    }
}
