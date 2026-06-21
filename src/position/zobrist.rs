// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// position/zobrist.rs — Zobrist hashing
//
// A Zobrist hash is a 64-bit fingerprint of a chess position.
// It lets the transposition table identify positions it has seen before.
//
// How it works:
//   1. At startup, generate a random 64-bit number for every combination of:
//      - Piece type (6) × Color (2) × Square (64) = 768 numbers
//      - Side to move (1 number for "Black to move")
//      - Castling rights (16 combinations = 16 numbers)
//      - En passant file (8 numbers, one per file)
//   2. Hash of a position = XOR of all relevant random numbers
//   3. When a move is made, XOR out old values and XOR in new values
//      This is O(1) — just a few XOR operations
//
// Why XOR?
//   XOR is its own inverse: A ^ B ^ B = A
//   So adding and removing a piece use the exact same operation.
//   This makes incremental updates trivial.
//
// Pet Dragon note:
//   We also hash the pawn start configuration so that two positions
//   with identical piece placement but different pawn start squares
//   (theoretically possible across different games) get different hashes.
//   This prevents the TT from confusing positions from different games.
// ============================================================================

use crate::types::{Color, PieceKind, Square};

// ── Zobrist tables ────────────────────────────────────────────────────────────

/// Random numbers for piece-square combinations
/// [color][piece_kind][square]
pub static mut PIECE_KEYS:     [[[u64; 64]; 6]; 2] = [[[0; 64]; 6]; 2];

/// Random number XOR'd in when Black is to move
/// (nothing XOR'd for White to move — White is the "default")
pub static mut SIDE_KEY:       u64 = 0;

/// Random numbers for castling rights
/// Indexed by the 4-bit castling rights mask:
///   bit 0 = White kingside
///   bit 1 = White queenside
///   bit 2 = Black kingside
///   bit 3 = Black queenside
pub static mut CASTLING_KEYS:  [u64; 16] = [0; 16];

/// Random numbers for en passant file (0=a .. 7=h)
/// Only used when an en passant capture is actually possible
pub static mut EN_PASSANT_KEYS:[u64; 8]  = [0; 8];

/// Pet Dragon: random numbers for pawn start square configuration
/// [color][square] — XOR'd in for each pawn's actual starting square
pub static mut PAWN_START_KEYS:[[u64; 64]; 2] = [[0; 64]; 2];

// ── Pseudo-random number generator ───────────────────────────────────────────
// We use a simple xorshift64 PRNG seeded with a fixed value.
// Fixed seed = deterministic hashes across runs = reproducible TT behaviour.
// This is standard practice in chess engines.

struct Rng(u64);

impl Rng {
    const fn new() -> Self {
        // Fixed seed — any non-zero value works
        // This specific seed is chosen to produce good distribution
        Rng(0x246C_CB28_5410_8BA3)
    }

    fn next(&mut self) -> u64 {
        // Xorshift64 — fast, good statistical properties
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ── Initialisation ────────────────────────────────────────────────────────────

/// Initialise all Zobrist random number tables.
/// Must be called once at engine startup before any position is created.
pub fn init_zobrist() {
    let mut rng = Rng::new();

    unsafe {
        // Piece-square keys
        for color in 0..2 {
            for piece in 0..6 {
                for sq in 0..64 {
                    PIECE_KEYS[color][piece][sq] = rng.next();
                }
            }
        }

        // Side to move key
        SIDE_KEY = rng.next();

        // Castling keys (one per combination of rights)
        for i in 0..16 {
            CASTLING_KEYS[i] = rng.next();
        }

        // En passant file keys
        for i in 0..8 {
            EN_PASSANT_KEYS[i] = rng.next();
        }

        // Pet Dragon: pawn start keys
        for color in 0..2 {
            for sq in 0..64 {
                PAWN_START_KEYS[color][sq] = rng.next();
            }
        }
    }
}

// ── Key accessors ─────────────────────────────────────────────────────────────

/// Get the Zobrist key for a piece on a square
#[inline]
pub fn piece_key(color: Color, kind: PieceKind, sq: Square) -> u64 {
    unsafe {
        PIECE_KEYS[color as usize][kind as usize][sq.index() as usize]
    }
}

/// Get the side-to-move key (XOR this in when Black is to move)
#[inline]
pub fn side_key() -> u64 {
    unsafe { SIDE_KEY }
}

/// Get the castling rights key for a given rights bitmask
#[inline]
pub fn castling_key(rights_mask: u8) -> u64 {
    unsafe { CASTLING_KEYS[(rights_mask & 0xF) as usize] }
}

/// Get the en passant key for a given file (0=a .. 7=h)
#[inline]
pub fn ep_key(file: u8) -> u64 {
    unsafe { EN_PASSANT_KEYS[(file & 7) as usize] }
}

/// Pet Dragon: get the pawn start key for a pawn of given color on given square
#[inline]
pub fn pawn_start_key(color: Color, sq: Square) -> u64 {
    unsafe {
        PAWN_START_KEYS[color as usize][sq.index() as usize]
    }
}

// ── CastlingRights bitmask helper ─────────────────────────────────────────────
// Converts CastlingRights struct to a 4-bit mask for array indexing

use crate::types::CastlingRights;

impl CastlingRights {
    /// Convert to a 4-bit mask for Zobrist key lookup
    /// bit 0 = white kingside, bit 1 = white queenside
    /// bit 2 = black kingside, bit 3 = black queenside
    #[inline]
    pub fn to_mask(self) -> u8 {
        let mut mask = 0u8;
        if self.white_kingside  { mask |= 1; }
        if self.white_queenside { mask |= 2; }
        if self.black_kingside  { mask |= 4; }
        if self.black_queenside { mask |= 8; }
        mask
    }

    /// Create CastlingRights from a 4-bit mask
    #[inline]
    pub fn from_mask(mask: u8) -> Self {
        CastlingRights {
            white_kingside:  mask & 1 != 0,
            white_queenside: mask & 2 != 0,
            black_kingside:  mask & 4 != 0,
            black_queenside: mask & 8 != 0,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CastlingRights, Color, PieceKind, Square};

    fn setup() {
        init_zobrist();
    }

    #[test]
    fn test_keys_are_nonzero() {
        setup();
        // Every key should be non-zero (probability of collision is negligible)
        assert_ne!(piece_key(Color::White, PieceKind::King, Square::E1), 0);
        assert_ne!(piece_key(Color::Black, PieceKind::Pawn, Square::A7), 0);
        assert_ne!(side_key(), 0);
        assert_ne!(ep_key(4), 0);
    }

    #[test]
    fn test_keys_are_unique() {
        setup();
        // Different positions should have different keys
        let k1 = piece_key(Color::White, PieceKind::King,  Square::E1);
        let k2 = piece_key(Color::White, PieceKind::Queen, Square::E1);
        let k3 = piece_key(Color::White, PieceKind::King,  Square::E2);
        let k4 = piece_key(Color::Black, PieceKind::King,  Square::E1);
        assert_ne!(k1, k2); // same square, different piece
        assert_ne!(k1, k3); // same piece, different square
        assert_ne!(k1, k4); // same piece+square, different color
    }

    #[test]
    fn test_xor_symmetry() {
        setup();
        // XOR symmetry: adding and removing a piece gives back original hash
        let base_hash = 0xDEAD_BEEF_CAFE_1234u64;
        let key = piece_key(Color::White, PieceKind::Rook, Square::A1);

        let after_add    = base_hash ^ key;
        let after_remove = after_add ^ key;
        assert_eq!(after_remove, base_hash,
            "XOR symmetry failed — hash should return to original");
    }

    #[test]
    fn test_castling_mask_roundtrip() {
        // All castling combinations
        let all = CastlingRights::ALL;
        assert_eq!(all.to_mask(), 0b1111);
        assert_eq!(CastlingRights::from_mask(0b1111), all);

        let none = CastlingRights::NONE;
        assert_eq!(none.to_mask(), 0b0000);
        assert_eq!(CastlingRights::from_mask(0b0000), none);

        let white_only = CastlingRights {
            white_kingside:  true,
            white_queenside: true,
            black_kingside:  false,
            black_queenside: false,
        };
        assert_eq!(white_only.to_mask(), 0b0011);
        assert_eq!(CastlingRights::from_mask(0b0011), white_only);
    }

    #[test]
    fn test_castling_keys_unique() {
        setup();
        // Each castling combination should have a unique key
        let k0 = castling_key(0b0000);
        let k1 = castling_key(0b0001);
        let k15 = castling_key(0b1111);
        assert_ne!(k0, k1);
        assert_ne!(k1, k15);
    }

    #[test]
    fn test_pawn_start_keys_unique() {
        setup();
        // Pet Dragon: pawn start keys should be unique per color+square
        let k1 = pawn_start_key(Color::White, Square::E1);
        let k2 = pawn_start_key(Color::White, Square::E2);
        let k3 = pawn_start_key(Color::Black, Square::E1);
        assert_ne!(k1, k2); // same color, different square
        assert_ne!(k1, k3); // same square, different color
    }

    #[test]
    fn test_deterministic() {
        // Calling init multiple times should give same keys (fixed seed)
        init_zobrist();
        let key1 = piece_key(Color::White, PieceKind::Queen, Square::D1);
        init_zobrist();
        let key2 = piece_key(Color::White, PieceKind::Queen, Square::D1);
        assert_eq!(key1, key2,
            "Zobrist keys must be deterministic across init calls");
    }
}
