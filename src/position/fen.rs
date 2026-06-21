// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// position/fen.rs — FEN string parsing and generation
//
// FEN = Forsyth-Edwards Notation
// Standard format for describing any chess position as a string.
//
// Format: "<pieces> <side> <castling> <ep> <halfmove> <fullmove>"
//
// Example (standard chess start):
//   rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1
//
// Pet Dragon extension:
//   We add an optional 7th field for pawn start squares.
//   This records which pawns started on rank 1/2 (White) or rank 7/8 (Black).
//   Format: comma-separated list of "square:color" pairs
//   Example: "e1:w,d2:w,..." for White pawns that started on those squares
//
//   This extension is needed because:
//   - Standard FEN only records current position, not starting squares
//   - Pet Dragon's double-step rule depends on actual starting square
//   - Without this, a loaded position loses pawn double-step information
//
//   When parsing standard FEN (no 7th field), we assume:
//   - White pawns currently on rank 1 or 2 started there
//   - Black pawns currently on rank 7 or 8 started there
//   This assumption is correct for freshly generated positions
//   and good enough for positions loaded mid-game.
// ============================================================================

use crate::types::{
    CastlingRights, Color, Piece, PieceKind, PawnStartMap, Square,
};

// ── FEN parsing error ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FenError {
    InvalidPiecePlacement(String),
    InvalidSideToMove(String),
    InvalidCastlingRights(String),
    InvalidEnPassant(String),
    InvalidHalfmoveClock(String),
    InvalidFullmoveNumber(String),
    WrongNumberOfFields(usize),
    KingNotFound(Color),
}

impl std::fmt::Display for FenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FenError::InvalidPiecePlacement(s) =>
                write!(f, "Invalid piece placement: {}", s),
            FenError::InvalidSideToMove(s) =>
                write!(f, "Invalid side to move: {}", s),
            FenError::InvalidCastlingRights(s) =>
                write!(f, "Invalid castling rights: {}", s),
            FenError::InvalidEnPassant(s) =>
                write!(f, "Invalid en passant: {}", s),
            FenError::InvalidHalfmoveClock(s) =>
                write!(f, "Invalid halfmove clock: {}", s),
            FenError::InvalidFullmoveNumber(s) =>
                write!(f, "Invalid fullmove number: {}", s),
            FenError::WrongNumberOfFields(n) =>
                write!(f, "Wrong number of FEN fields: {}", n),
            FenError::KingNotFound(c) =>
                write!(f, "King not found for {:?}", c),
        }
    }
}

// ── Parsed FEN data ───────────────────────────────────────────────────────────

/// All data extracted from a FEN string
/// Used by Position::from_fen() to build the full position struct
#[derive(Debug, Clone)]
pub struct ParsedFen {
    /// Piece on each square (None = empty)
    pub board: [Option<Piece>; 64],
    /// Which side moves next
    pub side_to_move: Color,
    /// Available castling options
    pub castling: CastlingRights,
    /// En passant target square (the square BEHIND the double-pushed pawn)
    pub en_passant: Option<Square>,
    /// Halfmove clock (for 50-move rule)
    pub halfmove_clock: u32,
    /// Fullmove number
    pub fullmove_number: u32,
    /// Pet Dragon: pawn start squares
    pub pawn_starts: PawnStartMap,
}

// ── FEN parser ────────────────────────────────────────────────────────────────

/// Parse a FEN string into a ParsedFen struct
pub fn parse_fen(fen: &str) -> Result<ParsedFen, FenError> {
    let fields: Vec<&str> = fen.trim().split_whitespace().collect();

    // Standard FEN has 6 fields, Pet Dragon FEN has 7
    if fields.len() < 6 || fields.len() > 7 {
        return Err(FenError::WrongNumberOfFields(fields.len()));
    }

    let board        = parse_piece_placement(fields[0])?;
    let side_to_move = parse_side_to_move(fields[1])?;
    let castling     = parse_castling_rights(fields[2])?;
    let en_passant   = parse_en_passant(fields[3])?;
    let halfmove_clock = fields[4].parse::<u32>().map_err(|_| {
        FenError::InvalidHalfmoveClock(fields[4].to_string())
    })?;
    let fullmove_number = fields[5].parse::<u32>().map_err(|_| {
        FenError::InvalidFullmoveNumber(fields[5].to_string())
    })?;

    // Parse Pet Dragon pawn start extension (7th field) if present
    let pawn_starts = if fields.len() == 7 {
        parse_pawn_starts(fields[6])?
    } else {
        // No 7th field — infer pawn starts from current position
        infer_pawn_starts(&board)
    };

    Ok(ParsedFen {
        board,
        side_to_move,
        castling,
        en_passant,
        halfmove_clock,
        fullmove_number,
        pawn_starts,
    })
}

/// Parse the piece placement field (e.g. "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR")
fn parse_piece_placement(s: &str) -> Result<[Option<Piece>; 64], FenError> {
    let mut board = [None; 64];
    let ranks: Vec<&str> = s.split('/').collect();

    if ranks.len() != 8 {
        return Err(FenError::InvalidPiecePlacement(
            format!("Expected 8 ranks, got {}", ranks.len())
        ));
    }

    // FEN ranks go from 8 (top) to 1 (bottom)
    for (rank_idx, rank_str) in ranks.iter().enumerate() {
        let rank = 7 - rank_idx as u8; // rank 8 first in FEN = index 7
        let mut file = 0u8;

        for ch in rank_str.chars() {
            if file > 8 {
                return Err(FenError::InvalidPiecePlacement(
                    format!("Too many squares in rank {}", rank + 1)
                ));
            }
            if ch.is_ascii_digit() {
                let skip = ch as u8 - b'0';
                file += skip;
            } else {
                let piece = Piece::from_fen_char(ch).ok_or_else(|| {
                    FenError::InvalidPiecePlacement(
                        format!("Unknown piece character '{}'", ch)
                    )
                })?;
                let sq = Square::from_file_rank(file, rank).ok_or_else(|| {
                    FenError::InvalidPiecePlacement(
                        format!("Square out of bounds: file={} rank={}", file, rank)
                    )
                })?;
                board[sq.index() as usize] = Some(piece);
                file += 1;
            }
        }

        if file != 8 {
            return Err(FenError::InvalidPiecePlacement(
                format!("Rank {} has {} files, expected 8", rank + 1, file)
            ));
        }
    }

    Ok(board)
}

/// Parse side to move ("w" or "b")
fn parse_side_to_move(s: &str) -> Result<Color, FenError> {
    match s {
        "w" => Ok(Color::White),
        "b" => Ok(Color::Black),
        _   => Err(FenError::InvalidSideToMove(s.to_string())),
    }
}

/// Parse castling rights ("KQkq", "Kq", "-", etc.)
fn parse_castling_rights(s: &str) -> Result<CastlingRights, FenError> {
    if s == "-" {
        return Ok(CastlingRights::NONE);
    }

    let mut rights = CastlingRights::NONE;
    for ch in s.chars() {
        match ch {
            'K' => rights.white_kingside  = true,
            'Q' => rights.white_queenside = true,
            'k' => rights.black_kingside  = true,
            'q' => rights.black_queenside = true,
            _   => return Err(FenError::InvalidCastlingRights(s.to_string())),
        }
    }
    Ok(rights)
}

/// Parse en passant target square ("-" or "e3", "d6", etc.)
fn parse_en_passant(s: &str) -> Result<Option<Square>, FenError> {
    if s == "-" {
        return Ok(None);
    }
    Square::from_uci(s)
        .map(Some)
        .ok_or_else(|| FenError::InvalidEnPassant(s.to_string()))
}

/// Parse Pet Dragon pawn start extension
/// Format: "e1:w,d2:w,a2:w,..." (square:color pairs)
fn parse_pawn_starts(s: &str) -> Result<PawnStartMap, FenError> {
    let mut map = PawnStartMap::EMPTY;
    if s == "-" {
        return Ok(map);
    }
    for entry in s.split(',') {
        let parts: Vec<&str> = entry.split(':').collect();
        if parts.len() != 2 {
            return Err(FenError::InvalidPiecePlacement(
                format!("Invalid pawn start entry: {}", entry)
            ));
        }
        let sq = Square::from_uci(parts[0]).ok_or_else(|| {
            FenError::InvalidPiecePlacement(
                format!("Invalid pawn start square: {}", parts[0])
            )
        })?;
        let color = match parts[1] {
            "w" => Color::White,
            "b" => Color::Black,
            _   => return Err(FenError::InvalidPiecePlacement(
                format!("Invalid pawn start color: {}", parts[1])
            )),
        };
        map.set(sq, color);
    }
    Ok(map)
}

/// Infer pawn start squares from current board position
/// Used when loading standard FEN without Pet Dragon extension
///
/// Assumption: any pawn currently on its "home" ranks
/// (rank 1 or 2 for White, rank 7 or 8 for Black)
/// started there. This is always true for fresh positions
/// and reasonable for mid-game positions.
fn infer_pawn_starts(board: &[Option<Piece>; 64]) -> PawnStartMap {
    let mut map = PawnStartMap::EMPTY;
    for sq in Square::all() {
        if let Some(piece) = board[sq.index() as usize] {
            if piece.kind == PieceKind::Pawn {
                let rank = sq.rank();
                let is_home_rank = match piece.color {
                    // White pawns on rank 1 or rank 2 (indices 0 or 1)
                    Color::White => rank == 0 || rank == 1,
                    // Black pawns on rank 7 or rank 8 (indices 6 or 7)
                    Color::Black => rank == 6 || rank == 7,
                };
                if is_home_rank {
                    map.set(sq, piece.color);
                }
            }
        }
    }
    map
}

// ── FEN generator ─────────────────────────────────────────────────────────────

/// Generate a FEN string from board state components
/// Used by Position::to_fen()
pub fn generate_fen(
    board:           &[Option<Piece>; 64],
    side_to_move:    Color,
    castling:        CastlingRights,
    en_passant:      Option<Square>,
    halfmove_clock:  u32,
    fullmove_number: u32,
    pawn_starts:     &PawnStartMap,
    include_pet_dragon_extension: bool,
) -> String {
    let mut fen = String::with_capacity(100);

    // ── Piece placement ───────────────────────────────────────────────────────
    // Write rank 8 first (FEN convention), rank 1 last
    for rank in (0..8u8).rev() {
        let mut empty_count = 0u8;
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, rank).unwrap();
            match board[sq.index() as usize] {
                None => {
                    empty_count += 1;
                }
                Some(piece) => {
                    if empty_count > 0 {
                        fen.push((b'0' + empty_count) as char);
                        empty_count = 0;
                    }
                    fen.push(piece.to_fen_char());
                }
            }
        }
        if empty_count > 0 {
            fen.push((b'0' + empty_count) as char);
        }
        if rank > 0 {
            fen.push('/');
        }
    }

    // ── Side to move ──────────────────────────────────────────────────────────
    fen.push(' ');
    fen.push(match side_to_move {
        Color::White => 'w',
        Color::Black => 'b',
    });

    // ── Castling rights ───────────────────────────────────────────────────────
    fen.push(' ');
    fen.push_str(&castling.to_fen());

    // ── En passant ────────────────────────────────────────────────────────────
    fen.push(' ');
    match en_passant {
        Some(sq) => fen.push_str(&sq.to_uci()),
        None     => fen.push('-'),
    }

    // ── Clocks ────────────────────────────────────────────────────────────────
    fen.push(' ');
    fen.push_str(&halfmove_clock.to_string());
    fen.push(' ');
    fen.push_str(&fullmove_number.to_string());

    // ── Pet Dragon extension (optional 7th field) ─────────────────────────────
    if include_pet_dragon_extension {
        fen.push(' ');
        let mut pawn_entries: Vec<String> = Vec::new();
        for sq in Square::all() {
            if let Some(color) = pawn_starts.get(sq) {
                let color_char = match color {
                    Color::White => 'w',
                    Color::Black => 'b',
                };
                pawn_entries.push(format!("{}:{}", sq.to_uci(), color_char));
            }
        }
        if pawn_entries.is_empty() {
            fen.push('-');
        } else {
            fen.push_str(&pawn_entries.join(","));
        }
    }

    fen
}

// ── Well-known FEN constants ──────────────────────────────────────────────────

/// Standard chess starting position FEN
/// Also one valid Pet Dragon arrangement
pub const STANDARD_START_FEN: &str =
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Color, Piece, PieceKind, Square};

    #[test]
    fn test_parse_standard_start() {
        let parsed = parse_fen(STANDARD_START_FEN).unwrap();

        // White pieces on rank 1
        assert_eq!(
            parsed.board[Square::E1.index() as usize],
            Some(Piece::WHITE_KING)
        );
        assert_eq!(
            parsed.board[Square::D1.index() as usize],
            Some(Piece::WHITE_QUEEN)
        );
        assert_eq!(
            parsed.board[Square::A1.index() as usize],
            Some(Piece::WHITE_ROOK)
        );
        assert_eq!(
            parsed.board[Square::H1.index() as usize],
            Some(Piece::WHITE_ROOK)
        );

        // Black pieces on rank 8
        assert_eq!(
            parsed.board[Square::E8.index() as usize],
            Some(Piece::BLACK_KING)
        );
        assert_eq!(
            parsed.board[Square::D8.index() as usize],
            Some(Piece::BLACK_QUEEN)
        );

        // White pawns on rank 2
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, 1).unwrap();
            assert_eq!(
                parsed.board[sq.index() as usize],
                Some(Piece::WHITE_PAWN),
                "Expected White pawn on {}", sq
            );
        }

        // Black pawns on rank 7
        for file in 0..8u8 {
            let sq = Square::from_file_rank(file, 6).unwrap();
            assert_eq!(
                parsed.board[sq.index() as usize],
                Some(Piece::BLACK_PAWN),
                "Expected Black pawn on {}", sq
            );
        }

        assert_eq!(parsed.side_to_move, Color::White);
        assert!(parsed.castling.white_kingside);
        assert!(parsed.castling.white_queenside);
        assert!(parsed.castling.black_kingside);
        assert!(parsed.castling.black_queenside);
        assert_eq!(parsed.en_passant, None);
        assert_eq!(parsed.halfmove_clock, 0);
        assert_eq!(parsed.fullmove_number, 1);
    }

    #[test]
    fn test_parse_empty_squares() {
        // Middle ranks should be empty in standard start
        let parsed = parse_fen(STANDARD_START_FEN).unwrap();
        for rank in 2..6u8 {
            for file in 0..8u8 {
                let sq = Square::from_file_rank(file, rank).unwrap();
                assert_eq!(
                    parsed.board[sq.index() as usize], None,
                    "Expected empty square on {}", sq
                );
            }
        }
    }

    #[test]
    fn test_parse_side_to_move() {
        let w = parse_fen(STANDARD_START_FEN).unwrap();
        assert_eq!(w.side_to_move, Color::White);

        let fen_black =
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let b = parse_fen(fen_black).unwrap();
        assert_eq!(b.side_to_move, Color::Black);
    }

    #[test]
    fn test_parse_en_passant() {
        let fen =
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1";
        let parsed = parse_fen(fen).unwrap();
        assert_eq!(parsed.en_passant, Some(Square::E3));
    }

    #[test]
    fn test_parse_no_castling() {
        let fen = "4k3/8/8/8/8/8/8/4K3 w - - 0 1";
        let parsed = parse_fen(fen).unwrap();
        assert_eq!(parsed.castling, CastlingRights::NONE);
    }

    #[test]
    fn test_generate_fen_standard_start() {
        let parsed = parse_fen(STANDARD_START_FEN).unwrap();
        let generated = generate_fen(
            &parsed.board,
            parsed.side_to_move,
            parsed.castling,
            parsed.en_passant,
            parsed.halfmove_clock,
            parsed.fullmove_number,
            &parsed.pawn_starts,
            false, // no Pet Dragon extension
        );
        assert_eq!(generated, STANDARD_START_FEN);
    }

    #[test]
    fn test_fen_roundtrip() {
        // Parse then generate should give back the same FEN
        let test_fens = [
            STANDARD_START_FEN,
            "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
            "r1bqkb1r/pppp1ppp/2n2n2/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4",
        ];

        for &fen in &test_fens {
            let parsed = parse_fen(fen).unwrap();
            let generated = generate_fen(
                &parsed.board,
                parsed.side_to_move,
                parsed.castling,
                parsed.en_passant,
                parsed.halfmove_clock,
                parsed.fullmove_number,
                &parsed.pawn_starts,
                false,
            );
            assert_eq!(generated, fen, "FEN roundtrip failed for: {}", fen);
        }
    }

    #[test]
    fn test_pet_dragon_pawn_start_extension() {
        // Generate a FEN with Pet Dragon extension and parse it back
        let parsed_std = parse_fen(STANDARD_START_FEN).unwrap();

        // Generate with extension
        let fen_with_ext = generate_fen(
            &parsed_std.board,
            parsed_std.side_to_move,
            parsed_std.castling,
            parsed_std.en_passant,
            parsed_std.halfmove_clock,
            parsed_std.fullmove_number,
            &parsed_std.pawn_starts,
            true, // include Pet Dragon extension
        );

        // Should have 7 fields
        assert_eq!(fen_with_ext.split_whitespace().count(), 7,
            "Pet Dragon FEN should have 7 fields");

        // Parse the extended FEN back
        let parsed_ext = parse_fen(&fen_with_ext).unwrap();

        // Pawn starts should be preserved
        for file in 0..8u8 {
            let white_sq = Square::from_file_rank(file, 1).unwrap();
            let black_sq = Square::from_file_rank(file, 6).unwrap();
            assert!(
                parsed_ext.pawn_starts.started_here(white_sq, Color::White),
                "White pawn start should be preserved for {}",
                white_sq
            );
            assert!(
                parsed_ext.pawn_starts.started_here(black_sq, Color::Black),
                "Black pawn start should be preserved for {}",
                black_sq
            );
        }
    }

    #[test]
    fn test_infer_pawn_starts_standard() {
        // In standard position, all rank-2 White pawns and rank-7 Black
        // pawns should be inferred as start squares
        let parsed = parse_fen(STANDARD_START_FEN).unwrap();

        for file in 0..8u8 {
            let white_sq = Square::from_file_rank(file, 1).unwrap();
            assert!(
                parsed.pawn_starts.started_here(white_sq, Color::White),
                "Standard position: White pawn on {} should be inferred \
                 as start square", white_sq
            );
        }
    }

    #[test]
    fn test_pet_dragon_rank1_pawn_inferred() {
        // A White pawn on rank 1 should be inferred as a start square
        // This is the Pet Dragon specific case
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPP1/RNBQKBNP w KQkq - 0 1";
        let parsed = parse_fen(fen).unwrap();
        // White pawn on h1 (rank 1) should be inferred as start square
        assert!(
            parsed.pawn_starts.started_here(Square::H1, Color::White),
            "White pawn on rank 1 should be inferred as start square"
        );
    }

    #[test]
    fn test_invalid_fen_wrong_fields() {
        assert!(parse_fen("rnbqkbnr w KQkq - 0").is_err());
        assert!(parse_fen("").is_err());
    }

    #[test]
    fn test_invalid_side_to_move() {
        let fen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1";
        assert!(parse_fen(fen).is_err());
    }

    #[test]
    fn test_piece_counts_standard() {
        let parsed = parse_fen(STANDARD_START_FEN).unwrap();
        let mut white_pawns = 0;
        let mut black_pawns = 0;
        let mut white_pieces = 0;
        let mut black_pieces = 0;

        for sq in Square::all() {
            if let Some(piece) = parsed.board[sq.index() as usize] {
                match piece.color {
                    Color::White => {
                        white_pieces += 1;
                        if piece.kind == PieceKind::Pawn {
                            white_pawns += 1;
                        }
                    }
                    Color::Black => {
                        black_pieces += 1;
                        if piece.kind == PieceKind::Pawn {
                            black_pawns += 1;
                        }
                    }
                }
            }
        }

        assert_eq!(white_pieces, 16, "White should have 16 pieces");
        assert_eq!(black_pieces, 16, "Black should have 16 pieces");
        assert_eq!(white_pawns,   8, "White should have 8 pawns");
        assert_eq!(black_pawns,   8, "Black should have 8 pawns");
    }
}
