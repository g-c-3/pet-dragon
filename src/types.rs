// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// types.rs — Core data types
//
// Every concept the engine thinks in is defined here:
//   Square    — one of 64 board squares (a1..h8)
//   File      — column a..h
//   Rank      — row 1..8
//   Color     — White or Black
//   PieceKind — Pawn, Knight, Bishop, Rook, Queen, King
//   Piece     — a colored piece (Color + PieceKind)
//   Move      — a move from one square to another
//   MoveKind  — what kind of move (quiet, capture, castle, etc.)
//   CastlingRights — which castling options remain available
//
// Pet Dragon custom:
//   PawnStartSquare — records where each pawn actually started
//                     (needed for double-step from rank 1 or rank 2)
// ============================================================================

// ── Square ───────────────────────────────────────────────────────────────────
//
// Represents one of the 64 squares on the board.
// Stored as a u8 (0..63) using Little-Endian Rank-File mapping:
//
//   a1=0,  b1=1,  c1=2,  d1=3,  e1=4,  f1=5,  g1=6,  h1=7
//   a2=8,  b2=9,  ...                              h2=15
//   ...
//   a8=56, b8=57, c8=58, d8=59, e8=60, f8=61, g8=62, h8=63
//
// This layout means:
//   square index = rank * 8 + file
//   file = index % 8
//   rank = index / 8

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Square {
    A1= 0, B1= 1, C1= 2, D1= 3, E1= 4, F1= 5, G1= 6, H1= 7,
    A2= 8, B2= 9, C2=10, D2=11, E2=12, F2=13, G2=14, H2=15,
    A3=16, B3=17, C3=18, D3=19, E3=20, F3=21, G3=22, H3=23,
    A4=24, B4=25, C4=26, D4=27, E4=28, F4=29, G4=30, H4=31,
    A5=32, B5=33, C5=34, D5=35, E5=36, F5=37, G5=38, H5=39,
    A6=40, B6=41, C6=42, D6=43, E6=44, F6=45, G6=46, H6=47,
    A7=48, B7=49, C7=50, D7=51, E7=52, F7=53, G7=54, H7=55,
    A8=56, B8=57, C8=58, D8=59, E8=60, F8=61, G8=62, H8=63,
}

impl Square {
    /// Total number of squares on the board
    pub const COUNT: usize = 64;

    /// Create a Square from a raw u8 index (0..63)
    /// Returns None if index is out of range
    #[inline]
    pub fn from_index(index: u8) -> Option<Self> {
        if index < 64 {
            // SAFETY: Square is repr(u8) with variants 0..63
            Some(unsafe { std::mem::transmute(index) })
        } else {
            None
        }
    }

    /// Create a Square from file and rank (both 0-based)
    /// file: 0=a, 1=b, ..., 7=h
    /// rank: 0=rank1, 1=rank2, ..., 7=rank8
    #[inline]
    pub fn from_file_rank(file: u8, rank: u8) -> Option<Self> {
        if file < 8 && rank < 8 {
            Self::from_index(rank * 8 + file)
        } else {
            None
        }
    }

    /// Get the raw index (0..63)
    #[inline]
    pub fn index(self) -> u8 {
        self as u8
    }

    /// Get the file (0=a .. 7=h)
    #[inline]
    pub fn file(self) -> u8 {
        self as u8 % 8
    }

    /// Get the rank (0=rank1 .. 7=rank8)
    #[inline]
    pub fn rank(self) -> u8 {
        self as u8 / 8
    }

    /// Get the File enum for this square
    #[inline]
    pub fn file_enum(self) -> File {
        File::from_index(self.file())
    }

    /// Get the Rank enum for this square
    #[inline]
    pub fn rank_enum(self) -> Rank {
        Rank::from_index(self.rank())
    }

    /// Get the color of this square on the board
    /// (used to validate bishop placement in Pet Dragon setup)
    /// a1 is dark (false), b1 is light (true)
    #[inline]
    pub fn is_light(self) -> bool {
        (self.file() + self.rank()) % 2 != 0
    }

    #[inline]
    pub fn is_dark(self) -> bool {
        !self.is_light()
    }

    /// Mirror this square vertically (rank 1 ↔ rank 8)
    /// Used in Pet Dragon setup: White rank 1 mirrors to Black rank 8
    /// White rank 2 mirrors to Black rank 7
    #[inline]
    pub fn mirror_rank(self) -> Self {
        let mirrored_rank = 7 - self.rank();
        // SAFETY: file and mirrored_rank are both 0..7
        Self::from_file_rank(self.file(), mirrored_rank).unwrap()
    }

    /// Get UCI string for this square (e.g. "e1", "h8")
    pub fn to_uci(self) -> String {
        let file_char = (b'a' + self.file()) as char;
        let rank_char = (b'1' + self.rank()) as char;
        format!("{}{}", file_char, rank_char)
    }

    /// Parse a square from a UCI string (e.g. "e1", "h8")
    pub fn from_uci(s: &str) -> Option<Self> {
        let bytes = s.as_bytes();
        if bytes.len() < 2 {
            return None;
        }
        let file = bytes[0].checked_sub(b'a')?;
        let rank = bytes[1].checked_sub(b'1')?;
        Self::from_file_rank(file, rank)
    }

    /// All 64 squares in order a1..h8
    pub fn all() -> impl Iterator<Item = Square> {
        (0u8..64).map(|i| Square::from_index(i).unwrap())
    }
}

impl std::fmt::Display for Square {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uci())
    }
}

// ── File ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum File {
    A = 0, B = 1, C = 2, D = 3,
    E = 4, F = 5, G = 6, H = 7,
}

impl File {
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => File::A, 1 => File::B, 2 => File::C, 3 => File::D,
            4 => File::E, 5 => File::F, 6 => File::G, 7 => File::H,
            _ => panic!("Invalid file index: {}", index),
        }
    }

    pub fn index(self) -> u8 { self as u8 }
}

// ── Rank ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Rank {
    R1 = 0, R2 = 1, R3 = 2, R4 = 3,
    R5 = 4, R6 = 5, R7 = 6, R8 = 7,
}

impl Rank {
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Rank::R1, 1 => Rank::R2, 2 => Rank::R3, 3 => Rank::R4,
            4 => Rank::R5, 5 => Rank::R6, 6 => Rank::R7, 7 => Rank::R8,
            _ => panic!("Invalid rank index: {}", index),
        }
    }

    pub fn index(self) -> u8 { self as u8 }
}

// ── Color ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    /// Flip to the other color
    #[inline]
    pub fn flip(self) -> Self {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// Index for array lookups [White=0, Black=1]
    #[inline]
    pub fn index(self) -> usize { self as usize }

    /// Both colors — useful for iterating
    pub const ALL: [Color; 2] = [Color::White, Color::Black];
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Color::White => write!(f, "White"),
            Color::Black => write!(f, "Black"),
        }
    }
}

// ── PieceKind ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceKind {
    Pawn   = 0,
    Knight = 1,
    Bishop = 2,
    Rook   = 3,
    Queen  = 4,
    King   = 5,
}

impl PieceKind {
    /// All piece kinds in order
    pub const ALL: [PieceKind; 6] = [
        PieceKind::Pawn,
        PieceKind::Knight,
        PieceKind::Bishop,
        PieceKind::Rook,
        PieceKind::Queen,
        PieceKind::King,
    ];

    /// Index for array lookups
    #[inline]
    pub fn index(self) -> usize { self as usize }

    /// Standard material value in centipawns
    /// (1 pawn = 100 centipawns)
    pub fn base_value(self) -> i32 {
        match self {
            PieceKind::Pawn   => 100,
            PieceKind::Knight => 320,
            PieceKind::Bishop => 330,
            PieceKind::Rook   => 500,
            PieceKind::Queen  => 900,
            PieceKind::King   => 20000, // Effectively infinite
        }
    }

    /// Single character for display/FEN
    /// Uppercase = White, Lowercase = Black (handled by Piece)
    pub fn to_char(self) -> char {
        match self {
            PieceKind::Pawn   => 'P',
            PieceKind::Knight => 'N',
            PieceKind::Bishop => 'B',
            PieceKind::Rook   => 'R',
            PieceKind::Queen  => 'Q',
            PieceKind::King   => 'K',
        }
    }

    /// Parse from FEN character (case insensitive)
    pub fn from_char(c: char) -> Option<Self> {
        match c.to_ascii_uppercase() {
            'P' => Some(PieceKind::Pawn),
            'N' => Some(PieceKind::Knight),
            'B' => Some(PieceKind::Bishop),
            'R' => Some(PieceKind::Rook),
            'Q' => Some(PieceKind::Queen),
            'K' => Some(PieceKind::King),
            _   => None,
        }
    }
}

impl std::fmt::Display for PieceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

// ── Piece ─────────────────────────────────────────────────────────────────────
// A piece is a color + kind combination.
// Stored as a single u8: bits [0..2] = kind, bit [3] = color
// This is compact and cache-friendly.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Piece {
    pub color: Color,
    pub kind:  PieceKind,
}

impl Piece {
    #[inline]
    pub fn new(color: Color, kind: PieceKind) -> Self {
        Self { color, kind }
    }

    // Convenience constructors
    pub const WHITE_PAWN:   Piece = Piece { color: Color::White, kind: PieceKind::Pawn   };
    pub const WHITE_KNIGHT: Piece = Piece { color: Color::White, kind: PieceKind::Knight };
    pub const WHITE_BISHOP: Piece = Piece { color: Color::White, kind: PieceKind::Bishop };
    pub const WHITE_ROOK:   Piece = Piece { color: Color::White, kind: PieceKind::Rook   };
    pub const WHITE_QUEEN:  Piece = Piece { color: Color::White, kind: PieceKind::Queen  };
    pub const WHITE_KING:   Piece = Piece { color: Color::White, kind: PieceKind::King   };
    pub const BLACK_PAWN:   Piece = Piece { color: Color::Black, kind: PieceKind::Pawn   };
    pub const BLACK_KNIGHT: Piece = Piece { color: Color::Black, kind: PieceKind::Knight };
    pub const BLACK_BISHOP: Piece = Piece { color: Color::Black, kind: PieceKind::Bishop };
    pub const BLACK_ROOK:   Piece = Piece { color: Color::Black, kind: PieceKind::Rook   };
    pub const BLACK_QUEEN:  Piece = Piece { color: Color::Black, kind: PieceKind::Queen  };
    pub const BLACK_KING:   Piece = Piece { color: Color::Black, kind: PieceKind::King   };

    /// FEN character: uppercase = White, lowercase = Black
    pub fn to_fen_char(self) -> char {
        let c = self.kind.to_char();
        match self.color {
            Color::White => c.to_ascii_uppercase(),
            Color::Black => c.to_ascii_lowercase(),
        }
    }

    /// Parse from FEN character
    pub fn from_fen_char(c: char) -> Option<Self> {
        let kind = PieceKind::from_char(c)?;
        let color = if c.is_uppercase() {
            Color::White
        } else {
            Color::Black
        };
        Some(Piece::new(color, kind))
    }
}

impl std::fmt::Display for Piece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_fen_char())
    }
}

// ── MoveKind ─────────────────────────────────────────────────────────────────
// What kind of move is this?
// Stored compactly — used in the Move struct below.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MoveKind {
    // Standard moves
    Quiet        = 0,   // Normal move, no capture
    DoublePush   = 1,   // Pawn double step (from actual start square)
                        // ← Pet Dragon: can be from rank 1 OR rank 2

    // Captures
    Capture      = 2,   // Normal capture
    EnPassant    = 3,   // En passant capture

    // Castling
    CastleKing   = 4,   // Kingside castling (only if Rook started on h1/h8)
    CastleQueen  = 5,   // Queenside castling (only if Rook started on a1/a8)

    // Promotions (quiet — no capture)
    PromoKnight  = 6,
    PromoBishop  = 7,
    PromoRook    = 8,
    PromoQueen   = 9,

    // Promotion captures
    PromoCapKnight = 10,
    PromoCapBishop = 11,
    PromoCapRook   = 12,
    PromoCapQueen  = 13,
}

impl MoveKind {
    /// Is this move a capture of any kind?
    #[inline]
    pub fn is_capture(self) -> bool {
        matches!(self,
            MoveKind::Capture
            | MoveKind::EnPassant
            | MoveKind::PromoCapKnight
            | MoveKind::PromoCapBishop
            | MoveKind::PromoCapRook
            | MoveKind::PromoCapQueen
        )
    }

    /// Is this move a promotion of any kind?
    #[inline]
    pub fn is_promotion(self) -> bool {
        matches!(self,
            MoveKind::PromoKnight
            | MoveKind::PromoBishop
            | MoveKind::PromoRook
            | MoveKind::PromoQueen
            | MoveKind::PromoCapKnight
            | MoveKind::PromoCapBishop
            | MoveKind::PromoCapRook
            | MoveKind::PromoCapQueen
        )
    }

    /// Is this a castling move?
    #[inline]
    pub fn is_castle(self) -> bool {
        matches!(self, MoveKind::CastleKing | MoveKind::CastleQueen)
    }

    /// What piece does this promote to? (None if not a promotion)
    pub fn promotion_piece(self) -> Option<PieceKind> {
        match self {
            MoveKind::PromoKnight
            | MoveKind::PromoCapKnight  => Some(PieceKind::Knight),
            MoveKind::PromoBishop
            | MoveKind::PromoCapBishop  => Some(PieceKind::Bishop),
            MoveKind::PromoRook
            | MoveKind::PromoCapRook    => Some(PieceKind::Rook),
            MoveKind::PromoQueen
            | MoveKind::PromoCapQueen   => Some(PieceKind::Queen),
            _                           => None,
        }
    }
}

// ── Move ──────────────────────────────────────────────────────────────────────
// A chess move: from square, to square, and what kind of move.
//
// Packed into a u32 for performance:
//   bits  0.. 5  = from square (0..63)
//   bits  6..11  = to square   (0..63)
//   bits 12..15  = move kind   (0..13)
//   bits 16..31  = reserved for move ordering score during search
//
// We also store the captured piece separately for fast unmake.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move {
    /// The square the piece moves from
    pub from: Square,
    /// The square the piece moves to
    pub to: Square,
    /// What kind of move this is
    pub kind: MoveKind,
    /// The piece that was captured (None for quiet moves)
    /// Stored here so unmake_move doesn't need to look it up
    pub captured: Option<PieceKind>,
}

impl Move {
    /// Create a new move
    #[inline]
    pub fn new(from: Square, to: Square, kind: MoveKind) -> Self {
        Self { from, to, kind, captured: None }
    }

    /// Create a capture move
    #[inline]
    pub fn capture(
        from: Square,
        to: Square,
        kind: MoveKind,
        captured: PieceKind,
    ) -> Self {
        Self { from, to, kind, captured: Some(captured) }
    }

    /// Is this a null move? (used in null move pruning — Phase 13)
    #[inline]
    pub fn is_null(self) -> bool {
        self.from == self.to
    }

    /// The null move (from==to, used as sentinel value in search)
    pub const NULL: Move = Move {
        from:     Square::A1,
        to:       Square::A1,
        kind:     MoveKind::Quiet,
        captured: None,
    };

    /// UCI string representation (e.g. "e2e4", "e1g1", "a7a8q")
    pub fn to_uci(self) -> String {
        let promo = match self.kind.promotion_piece() {
            Some(PieceKind::Knight) => "n",
            Some(PieceKind::Bishop) => "b",
            Some(PieceKind::Rook)   => "r",
            Some(PieceKind::Queen)  => "q",
            _                       => "",
        };
        format!("{}{}{}", self.from.to_uci(), self.to.to_uci(), promo)
    }
}

impl std::fmt::Display for Move {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uci())
    }
}

// ── CastlingRights ────────────────────────────────────────────────────────────
// Tracks which castling options are still available.
//
// Pet Dragon custom:
// Rights are ONLY set if the Rook actually started on its standard square.
// This is detected during position generation (Phase 3) and stored here.
// If a Rook randomly landed elsewhere, that castling right is never set.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastlingRights {
    /// White can castle kingside (King e1 + Rook h1, both unmoved)
    pub white_kingside:  bool,
    /// White can castle queenside (King e1 + Rook a1, both unmoved)
    pub white_queenside: bool,
    /// Black can castle kingside (King e8 + Rook h8, both unmoved)
    pub black_kingside:  bool,
    /// Black can castle queenside (King e8 + Rook a8, both unmoved)
    pub black_queenside: bool,
}

impl CastlingRights {
    /// No castling rights at all
    pub const NONE: Self = Self {
        white_kingside:  false,
        white_queenside: false,
        black_kingside:  false,
        black_queenside: false,
    };

    /// All castling rights available
    /// (only valid when both Rooks happened to start on standard squares)
    pub const ALL: Self = Self {
        white_kingside:  true,
        white_queenside: true,
        black_kingside:  true,
        black_queenside: true,
    };

    /// Does the given color have any castling rights remaining?
    #[inline]
    pub fn has_any(self, color: Color) -> bool {
        match color {
            Color::White => self.white_kingside || self.white_queenside,
            Color::Black => self.black_kingside || self.black_queenside,
        }
    }

    /// Remove all castling rights for a color (called when King moves)
    #[inline]
    pub fn remove_all(&mut self, color: Color) {
        match color {
            Color::White => {
                self.white_kingside  = false;
                self.white_queenside = false;
            }
            Color::Black => {
                self.black_kingside  = false;
                self.black_queenside = false;
            }
        }
    }

    /// FEN string (e.g. "KQkq", "Kq", "-")
    pub fn to_fen(self) -> String {
        let mut s = String::new();
        if self.white_kingside  { s.push('K'); }
        if self.white_queenside { s.push('Q'); }
        if self.black_kingside  { s.push('k'); }
        if self.black_queenside { s.push('q'); }
        if s.is_empty() { s.push('-'); }
        s
    }
}

impl Default for CastlingRights {
    fn default() -> Self {
        Self::NONE
    }
}

// ── PawnStartSquare ───────────────────────────────────────────────────────────
// Pet Dragon custom type.
//
// In standard chess every pawn starts on rank 2 (White) or rank 7 (Black)
// and the double-step rule is hardcoded to those ranks.
//
// In Pet Dragon, pawns can start on rank 1 OR rank 2 (White) or
// rank 7 OR rank 8 (Black). The double-step right comes from the
// ACTUAL starting square, not an assumed rank.
//
// This type records where each pawn started so the move generator
// can correctly determine double-step eligibility.
//
// Stored as a fixed array indexed by square (64 entries).
// Value: Some(color) means a pawn of that color started here.
//        None means no pawn started on this square.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PawnStartMap(pub [Option<Color>; 64]);

impl PawnStartMap {
    /// Empty map — no pawns have starting squares recorded yet
    pub const EMPTY: Self = Self([None; 64]);

    /// Record that a pawn of the given color started on this square
    #[inline]
    pub fn set(&mut self, square: Square, color: Color) {
        self.0[square.index() as usize] = Some(color);
    }

    /// Did a pawn of the given color start on this square?
    #[inline]
    pub fn started_here(&self, square: Square, color: Color) -> bool {
        self.0[square.index() as usize] == Some(color)
    }

    /// Get whatever color's pawn started here (if any)
    #[inline]
    pub fn get(&self, square: Square) -> Option<Color> {
        self.0[square.index() as usize]
    }
}

impl Default for PawnStartMap {
    fn default() -> Self {
        Self::EMPTY
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_square_indices() {
        assert_eq!(Square::A1.index(), 0);
        assert_eq!(Square::H1.index(), 7);
        assert_eq!(Square::A2.index(), 8);
        assert_eq!(Square::E1.index(), 4);
        assert_eq!(Square::E8.index(), 60);
        assert_eq!(Square::H8.index(), 63);
    }

    #[test]
    fn test_square_file_rank() {
        assert_eq!(Square::E1.file(), 4); // e = file 4
        assert_eq!(Square::E1.rank(), 0); // rank 1 = index 0
        assert_eq!(Square::A8.file(), 0);
        assert_eq!(Square::A8.rank(), 7);
        assert_eq!(Square::H8.file(), 7);
        assert_eq!(Square::H8.rank(), 7);
    }

    #[test]
    fn test_square_mirror() {
        // Pet Dragon: rank 1 mirrors to rank 8, file preserved
        assert_eq!(Square::E1.mirror_rank(), Square::E8);
        assert_eq!(Square::A1.mirror_rank(), Square::A8);
        assert_eq!(Square::H2.mirror_rank(), Square::H7);
        assert_eq!(Square::D2.mirror_rank(), Square::D7);
    }

    #[test]
    fn test_square_color() {
        // a1 is dark in standard chess
        assert!(Square::A1.is_dark());
        assert!(Square::B1.is_light());
        assert!(Square::H1.is_light());
        assert!(Square::A8.is_light());
    }

    #[test]
    fn test_square_uci() {
        assert_eq!(Square::E1.to_uci(), "e1");
        assert_eq!(Square::E8.to_uci(), "e8");
        assert_eq!(Square::A1.to_uci(), "a1");
        assert_eq!(Square::H8.to_uci(), "h8");
        assert_eq!(Square::from_uci("e1"), Some(Square::E1));
        assert_eq!(Square::from_uci("h8"), Some(Square::H8));
        assert_eq!(Square::from_uci("xx"), None);
    }

    #[test]
    fn test_color_flip() {
        assert_eq!(Color::White.flip(), Color::Black);
        assert_eq!(Color::Black.flip(), Color::White);
    }

    #[test]
    fn test_piece_fen_chars() {
        assert_eq!(Piece::WHITE_KING.to_fen_char(),  'K');
        assert_eq!(Piece::BLACK_KING.to_fen_char(),  'k');
        assert_eq!(Piece::WHITE_PAWN.to_fen_char(),  'P');
        assert_eq!(Piece::BLACK_QUEEN.to_fen_char(), 'q');
    }

    #[test]
    fn test_castling_rights_fen() {
        assert_eq!(CastlingRights::ALL.to_fen(),  "KQkq");
        assert_eq!(CastlingRights::NONE.to_fen(), "-");
        let rights = CastlingRights {
            white_kingside:  true,
            white_queenside: false,
            black_kingside:  true,
            black_queenside: false,
        };
        assert_eq!(rights.to_fen(), "Kk");
    }

    #[test]
    fn test_pawn_start_map() {
        let mut map = PawnStartMap::EMPTY;
        // White pawn started on e1 (Pet Dragon rank 1 start)
        map.set(Square::E1, Color::White);
        assert!(map.started_here(Square::E1, Color::White));
        assert!(!map.started_here(Square::E1, Color::Black));
        assert!(!map.started_here(Square::E2, Color::White));

        // Black pawn started on e8 (mirrored from e1)
        map.set(Square::E8, Color::Black);
        assert!(map.started_here(Square::E8, Color::Black));
    }

    #[test]
    fn test_move_uci() {
        let m = Move::new(Square::E2, Square::E4, MoveKind::DoublePush);
        assert_eq!(m.to_uci(), "e2e4");

        let promo = Move::new(Square::A7, Square::A8, MoveKind::PromoQueen);
        assert_eq!(promo.to_uci(), "a7a8q");
    }

    #[test]
    fn test_all_squares_count() {
        assert_eq!(Square::all().count(), 64);
    }

    #[test]
    fn test_from_file_rank_roundtrip() {
        for sq in Square::all() {
            let reconstructed = Square::from_file_rank(
                sq.file(), sq.rank()
            ).unwrap();
            assert_eq!(sq, reconstructed);
        }
    }
}
