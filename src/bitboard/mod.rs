// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// bitboard/mod.rs — Bitboard type and operations
//
// A Bitboard is a 64-bit integer where each bit represents one square.
// Bit 0 = a1, Bit 1 = b1, ..., Bit 63 = h8.
//
// This is the core data structure of the entire engine.
// Every piece position, every attack set, every pin mask is a Bitboard.
//
// Why bitboards?
//   - "Are there any pieces on these squares?" = (bb & mask) != 0
//   - "How many pieces?" = bb.count_ones() — one CPU instruction
//   - "Where is the lowest piece?" = bb.trailing_zeros() — one instruction
//   - Move generation becomes bitwise AND/OR/shift operations
//   - Modern CPUs do these in a single clock cycle
// ============================================================================

pub mod masks;
pub mod magic;

pub use magic::{rook_attacks, bishop_attacks, queen_attacks};
pub use masks::{
    knight_attacks, king_attacks, pawn_attacks,
    between, line, pawn_double_push_mask, are_aligned,
};

use crate::types::Square;

// ── Bitboard type ─────────────────────────────────────────────────────────────

/// A set of squares represented as a 64-bit integer.
/// Each bit corresponds to one square (bit 0 = a1, bit 63 = h8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bitboard(pub u64);

impl Bitboard {
    // ── Constants ────────────────────────────────────────────────────────────

    /// No squares set
    pub const EMPTY: Bitboard = Bitboard(0);

    /// All 64 squares set
    pub const FULL: Bitboard = Bitboard(u64::MAX);

    // Rank masks — all squares on a given rank
    pub const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00FF);
    pub const RANK_2: Bitboard = Bitboard(0x0000_0000_0000_FF00);
    pub const RANK_3: Bitboard = Bitboard(0x0000_0000_00FF_0000);
    pub const RANK_4: Bitboard = Bitboard(0x0000_0000_FF00_0000);
    pub const RANK_5: Bitboard = Bitboard(0x0000_00FF_0000_0000);
    pub const RANK_6: Bitboard = Bitboard(0x0000_FF00_0000_0000);
    pub const RANK_7: Bitboard = Bitboard(0x00FF_0000_0000_0000);
    pub const RANK_8: Bitboard = Bitboard(0xFF00_0000_0000_0000);

    // File masks — all squares on a given file
    pub const FILE_A: Bitboard = Bitboard(0x0101_0101_0101_0101);
    pub const FILE_B: Bitboard = Bitboard(0x0202_0202_0202_0202);
    pub const FILE_C: Bitboard = Bitboard(0x0404_0404_0404_0404);
    pub const FILE_D: Bitboard = Bitboard(0x0808_0808_0808_0808);
    pub const FILE_E: Bitboard = Bitboard(0x1010_1010_1010_1010);
    pub const FILE_F: Bitboard = Bitboard(0x2020_2020_2020_2020);
    pub const FILE_G: Bitboard = Bitboard(0x4040_4040_4040_4040);
    pub const FILE_H: Bitboard = Bitboard(0x8080_8080_8080_8080);

    // Not-file masks — useful for preventing wrap-around in shift operations
    pub const NOT_FILE_A: Bitboard = Bitboard(!0x0101_0101_0101_0101);
    pub const NOT_FILE_H: Bitboard = Bitboard(!0x8080_8080_8080_8080);
    pub const NOT_FILE_AB: Bitboard = Bitboard(!0x0303_0303_0303_0303);
    pub const NOT_FILE_GH: Bitboard = Bitboard(!0xC0C0_C0C0_C0C0_C0C0);

    // Square color masks
    pub const LIGHT_SQUARES: Bitboard = Bitboard(0x55AA_55AA_55AA_55AA);
    pub const DARK_SQUARES:  Bitboard = Bitboard(0xAA55_AA55_AA55_AA55);

    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a Bitboard with a single square set
    #[inline]
    pub fn from_square(sq: Square) -> Self {
        Bitboard(1u64 << sq.index())
    }

    /// Create a Bitboard from a raw u64
    #[inline]
    pub const fn from_u64(val: u64) -> Self {
        Bitboard(val)
    }

    // ── Basic queries ─────────────────────────────────────────────────────────

    /// Is this bitboard empty? (no squares set)
    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Is at least one square set?
    #[inline]
    pub fn is_not_empty(self) -> bool {
        self.0 != 0
    }

    /// How many squares are set?
    /// Uses a single POPCNT CPU instruction when available.
    #[inline]
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// Is exactly one square set?
    #[inline]
    pub fn is_single(self) -> bool {
        self.0 != 0 && (self.0 & self.0.wrapping_sub(1)) == 0
    }

    /// Is a specific square set in this bitboard?
    #[inline]
    pub fn contains(self, sq: Square) -> bool {
        (self.0 >> sq.index()) & 1 == 1
    }

    // ── Bit manipulation ──────────────────────────────────────────────────────

    /// Set a square (turn its bit on)
    #[inline]
    pub fn set(&mut self, sq: Square) {
        self.0 |= 1u64 << sq.index();
    }

    /// Clear a square (turn its bit off)
    #[inline]
    pub fn clear(&mut self, sq: Square) {
        self.0 &= !(1u64 << sq.index());
    }

    /// Toggle a square (flip its bit)
    #[inline]
    pub fn toggle(&mut self, sq: Square) {
        self.0 ^= 1u64 << sq.index();
    }

    // ── Square extraction ─────────────────────────────────────────────────────

    /// Get the lowest set square (a1 side) without removing it.
    /// Uses BSF (Bit Scan Forward) — one CPU instruction.
    /// Returns None if empty.
    #[inline]
    pub fn lsb(self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            Square::from_index(self.0.trailing_zeros() as u8)
        }
    }

    /// Get the highest set square (h8 side) without removing it.
    /// Uses BSR (Bit Scan Reverse) — one CPU instruction.
    #[inline]
    pub fn msb(self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            Square::from_index(63 - self.0.leading_zeros() as u8)
        }
    }

    /// Remove and return the lowest set square.
    /// This is the core of our move generation loops:
    ///   while let Some(sq) = bb.pop_lsb() { ... }
    #[inline]
    pub fn pop_lsb(&mut self) -> Option<Square> {
        if self.0 == 0 {
            None
        } else {
            let sq = Square::from_index(self.0.trailing_zeros() as u8);
            // Clear the lowest set bit: x & (x-1)
            self.0 &= self.0 - 1;
            sq
        }
    }

    // ── Shift operations ──────────────────────────────────────────────────────
    // Used for pawn move generation.
    // "North" = toward rank 8 (White's forward direction)
    // "South" = toward rank 1 (Black's forward direction)

    /// Shift all squares one rank toward rank 8 (White pawn push direction)
    #[inline]
    pub fn shift_north(self) -> Self {
        Bitboard(self.0 << 8)
    }

    /// Shift all squares one rank toward rank 1 (Black pawn push direction)
    #[inline]
    pub fn shift_south(self) -> Self {
        Bitboard(self.0 >> 8)
    }

    /// Shift east (toward h-file), masking off h-file to prevent wrap
    #[inline]
    pub fn shift_east(self) -> Self {
        Bitboard((self.0 << 1) & Self::NOT_FILE_A.0)
    }

    /// Shift west (toward a-file), masking off a-file to prevent wrap
    #[inline]
    pub fn shift_west(self) -> Self {
        Bitboard((self.0 >> 1) & Self::NOT_FILE_H.0)
    }

    /// Northeast (toward rank 8, h-file)
    #[inline]
    pub fn shift_north_east(self) -> Self {
        Bitboard((self.0 << 9) & Self::NOT_FILE_A.0)
    }

    /// Northwest (toward rank 8, a-file)
    #[inline]
    pub fn shift_north_west(self) -> Self {
        Bitboard((self.0 << 7) & Self::NOT_FILE_H.0)
    }

    /// Southeast (toward rank 1, h-file)
    #[inline]
    pub fn shift_south_east(self) -> Self {
        Bitboard((self.0 >> 7) & Self::NOT_FILE_A.0)
    }

    /// Southwest (toward rank 1, a-file)
    #[inline]
    pub fn shift_south_west(self) -> Self {
        Bitboard((self.0 >> 9) & Self::NOT_FILE_H.0)
    }

    // ── Rank and file helpers ─────────────────────────────────────────────────

    /// Get the rank mask for a given rank index (0=rank1 .. 7=rank8)
    #[inline]
    pub fn rank_mask(rank: u8) -> Self {
        Bitboard(0xFF << (rank * 8))
    }

    /// Get the file mask for a given file index (0=a .. 7=h)
    #[inline]
    pub fn file_mask(file: u8) -> Self {
        Bitboard(0x0101_0101_0101_0101 << file)
    }

    /// Get raw u64 value
    #[inline]
    pub fn as_u64(self) -> u64 {
        self.0
    }

    // ── Pet Dragon specific helpers ───────────────────────────────────────────

    /// Is this square on a light square?
    /// Used in bishop placement validation during Pet Dragon setup.
    #[inline]
    pub fn is_light_square(sq: Square) -> bool {
        Self::LIGHT_SQUARES.contains(sq)
    }

    /// Is this square on a dark square?
    #[inline]
    pub fn is_dark_square(sq: Square) -> bool {
        Self::DARK_SQUARES.contains(sq)
    }

    /// Get all squares in ranks 1 and 2 combined
    /// (White's territory in Pet Dragon setup)
    pub const WHITE_SETUP_RANKS: Bitboard =
        Bitboard(Self::RANK_1.0 | Self::RANK_2.0);

    /// Get all squares in ranks 7 and 8 combined
    /// (Black's territory in Pet Dragon setup)
    pub const BLACK_SETUP_RANKS: Bitboard =
        Bitboard(Self::RANK_7.0 | Self::RANK_8.0);
}

// ── Bitwise operator overloads ────────────────────────────────────────────────
// These let us write: bb1 & bb2, bb1 | bb2, !bb1, etc.

impl std::ops::BitAnd for Bitboard {
    type Output = Self;
    #[inline]
    fn bitand(self, rhs: Self) -> Self { Bitboard(self.0 & rhs.0) }
}

impl std::ops::BitOr for Bitboard {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self { Bitboard(self.0 | rhs.0) }
}

impl std::ops::BitXor for Bitboard {
    type Output = Self;
    #[inline]
    fn bitxor(self, rhs: Self) -> Self { Bitboard(self.0 ^ rhs.0) }
}

impl std::ops::Not for Bitboard {
    type Output = Self;
    #[inline]
    fn not(self) -> Self { Bitboard(!self.0) }
}

impl std::ops::BitAndAssign for Bitboard {
    #[inline]
    fn bitand_assign(&mut self, rhs: Self) { self.0 &= rhs.0; }
}

impl std::ops::BitOrAssign for Bitboard {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) { self.0 |= rhs.0; }
}

impl std::ops::BitXorAssign for Bitboard {
    #[inline]
    fn bitxor_assign(&mut self, rhs: Self) { self.0 ^= rhs.0; }
}

impl std::ops::Shl<u32> for Bitboard {
    type Output = Self;
    #[inline]
    fn shl(self, rhs: u32) -> Self { Bitboard(self.0 << rhs) }
}

impl std::ops::Shr<u32> for Bitboard {
    type Output = Self;
    #[inline]
    fn shr(self, rhs: u32) -> Self { Bitboard(self.0 >> rhs) }
}

// ── Iterator ──────────────────────────────────────────────────────────────────
// Lets us write: for sq in bitboard { ... }
// Each iteration yields the next set square and removes it.

impl Iterator for Bitboard {
    type Item = Square;

    #[inline]
    fn next(&mut self) -> Option<Square> {
        self.pop_lsb()
    }
}

// ── Display ───────────────────────────────────────────────────────────────────
// Prints the board as an 8x8 grid — useful for debugging.
// Rank 8 at top (as you'd see on a real board), rank 1 at bottom.

impl std::fmt::Display for Bitboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "  a b c d e f g h")?;
        writeln!(f, "  ───────────────")?;
        for rank in (0..8).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in 0..8 {
                let sq = Square::from_file_rank(file, rank).unwrap();
                if self.contains(sq) {
                    write!(f, "■ ")?;
                } else {
                    write!(f, "· ")?;
                }
            }
            writeln!(f, "{}", rank + 1)?;
        }
        writeln!(f, "  ───────────────")?;
        writeln!(f, "  a b c d e f g h")?;
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    #[test]
    fn test_empty_and_full() {
        assert!(Bitboard::EMPTY.is_empty());
        assert!(!Bitboard::FULL.is_empty());
        assert_eq!(Bitboard::FULL.count(), 64);
    }

    #[test]
    fn test_from_square() {
        let bb = Bitboard::from_square(Square::E1);
        assert!(bb.contains(Square::E1));
        assert!(!bb.contains(Square::E2));
        assert_eq!(bb.count(), 1);
    }

    #[test]
    fn test_set_clear_toggle() {
        let mut bb = Bitboard::EMPTY;
        bb.set(Square::A1);
        assert!(bb.contains(Square::A1));
        bb.clear(Square::A1);
        assert!(!bb.contains(Square::A1));
        bb.toggle(Square::H8);
        assert!(bb.contains(Square::H8));
        bb.toggle(Square::H8);
        assert!(!bb.contains(Square::H8));
    }

    #[test]
    fn test_pop_lsb() {
        let mut bb = Bitboard::from_square(Square::A1)
                   | Bitboard::from_square(Square::E4)
                   | Bitboard::from_square(Square::H8);
        assert_eq!(bb.count(), 3);
        assert_eq!(bb.pop_lsb(), Some(Square::A1));
        assert_eq!(bb.pop_lsb(), Some(Square::E4));
        assert_eq!(bb.pop_lsb(), Some(Square::H8));
        assert_eq!(bb.pop_lsb(), None);
    }

    #[test]
    fn test_rank_masks() {
        assert_eq!(Bitboard::RANK_1.count(), 8);
        assert_eq!(Bitboard::RANK_8.count(), 8);
        assert!(Bitboard::RANK_1.contains(Square::A1));
        assert!(Bitboard::RANK_1.contains(Square::H1));
        assert!(!Bitboard::RANK_1.contains(Square::A2));
        assert!(Bitboard::RANK_8.contains(Square::E8));
    }

    #[test]
    fn test_file_masks() {
        assert_eq!(Bitboard::FILE_A.count(), 8);
        assert!(Bitboard::FILE_A.contains(Square::A1));
        assert!(Bitboard::FILE_A.contains(Square::A8));
        assert!(!Bitboard::FILE_A.contains(Square::B1));
        assert!(Bitboard::FILE_H.contains(Square::H1));
        assert!(Bitboard::FILE_H.contains(Square::H8));
    }

    #[test]
    fn test_shift_north() {
        // A pawn on e2 pushed north should land on e3
        let e2 = Bitboard::from_square(Square::E2);
        let e3 = Bitboard::from_square(Square::E3);
        assert_eq!(e2.shift_north(), e3);
    }

    #[test]
    fn test_shift_south() {
        // A pawn on e7 pushed south should land on e6
        let e7 = Bitboard::from_square(Square::E7);
        let e6 = Bitboard::from_square(Square::E6);
        assert_eq!(e7.shift_south(), e6);
    }

    #[test]
    fn test_no_wraparound_east() {
        // Shifting h-file east should not wrap to a-file
        let h_file = Bitboard::FILE_H;
        assert!(h_file.shift_east().is_empty());
    }

    #[test]
    fn test_no_wraparound_west() {
        // Shifting a-file west should not wrap to h-file
        let a_file = Bitboard::FILE_A;
        assert!(a_file.shift_west().is_empty());
    }

    #[test]
    fn test_iterator() {
        let squares = [Square::A1, Square::D4, Square::H8];
        let mut bb = Bitboard::EMPTY;
        for &sq in &squares {
            bb.set(sq);
        }
        let collected: Vec<Square> = bb.collect();
        assert_eq!(collected, squares);
    }

    #[test]
    fn test_light_dark_squares() {
        // a1 is dark in standard chess orientation
        assert!(Bitboard::is_dark_square(Square::A1));
        assert!(Bitboard::is_light_square(Square::B1));
        assert_eq!(Bitboard::LIGHT_SQUARES.count(), 32);
        assert_eq!(Bitboard::DARK_SQUARES.count(), 32);
    }

    #[test]
    fn test_pet_dragon_setup_ranks() {
        // White setup territory: ranks 1 and 2 = 16 squares
        assert_eq!(Bitboard::WHITE_SETUP_RANKS.count(), 16);
        // Black setup territory: ranks 7 and 8 = 16 squares
        assert_eq!(Bitboard::BLACK_SETUP_RANKS.count(), 16);
        // They should not overlap
        assert!((Bitboard::WHITE_SETUP_RANKS
               & Bitboard::BLACK_SETUP_RANKS).is_empty());
    }

    #[test]
    fn test_bitwise_ops() {
        let a = Bitboard::from_square(Square::A1)
              | Bitboard::from_square(Square::B1);
        let b = Bitboard::from_square(Square::B1)
              | Bitboard::from_square(Square::C1);
        assert_eq!((a & b).count(), 1); // Only B1 in both
        assert_eq!((a | b).count(), 3); // A1, B1, C1
        assert_eq!((a ^ b).count(), 2); // A1, C1 (not both)
    }

    #[test]
    fn test_lsb_msb() {
        let bb = Bitboard::from_square(Square::C3)
               | Bitboard::from_square(Square::G7);
        assert_eq!(bb.lsb(), Some(Square::C3));
        assert_eq!(bb.msb(), Some(Square::G7));
    }

    #[test]
    fn test_is_single() {
        assert!(Bitboard::from_square(Square::E4).is_single());
        assert!(!Bitboard::EMPTY.is_single());
        let two = Bitboard::from_square(Square::A1)
                | Bitboard::from_square(Square::B1);
        assert!(!two.is_single());
    }
}
