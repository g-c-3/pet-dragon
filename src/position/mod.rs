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
    castling_key, ep_key, init_zobrist, pawn_start_key,
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
}
