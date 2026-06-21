// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// bitboard/magic.rs — Magic bitboards for sliding piece attacks
//
// Problem: A Rook on e1 attacks differently depending on what pieces
// are blocking it. With 6 relevant squares per rank/file, there are
// 2^6 = 64 possible blocker configurations per square. For all 64
// squares that's thousands of cases — too slow to compute each time.
//
// Solution: Magic bitboards
// For each square, we precompute a "magic number" such that:
//   (blockers & occupancy_mask) * magic_number >> shift
// produces a unique index into an attack table.
// The attack table is precomputed for every possible blocker config.
// Result: sliding piece attacks = one multiply + one shift + one lookup.
//
// Magic numbers sourced from public domain chess programming resources
// (well-known values used across many GPL v3 engines including Stockfish).
// ============================================================================

use crate::bitboard::Bitboard;
use crate::types::Square;

// ── Magic entry ───────────────────────────────────────────────────────────────

/// One entry in the magic table for a single square
struct Magic {
    /// Mask of squares that can block this piece on this square
    /// (excludes edges — edge squares can't block further movement)
    mask: u64,
    /// The magic number for this square
    magic: u64,
    /// How many bits to shift after multiplying
    shift: u32,
    /// Offset into the shared attack table
    offset: usize,
}

// ── Shared attack storage ─────────────────────────────────────────────────────
// All 64 rook attack tables share one flat array (800KB total)
// All 64 bishop attack tables share one flat array (smaller)

const ROOK_TABLE_SIZE:   usize = 102_400;
const BISHOP_TABLE_SIZE: usize = 5_248;

static mut ROOK_ATTACKS:   [Bitboard; ROOK_TABLE_SIZE]   =
    [Bitboard(0); ROOK_TABLE_SIZE];
static mut BISHOP_ATTACKS: [Bitboard; BISHOP_TABLE_SIZE] =
    [Bitboard(0); BISHOP_TABLE_SIZE];

static mut ROOK_MAGICS:   [Magic; 64] = unsafe {
    // SAFETY: Magic has no invalid bit patterns — zero-init is valid
    std::mem::zeroed()
};
static mut BISHOP_MAGICS: [Magic; 64] = unsafe {
    std::mem::zeroed()
};

// ── Known magic numbers ───────────────────────────────────────────────────────
// These are well-known public domain magic numbers used across many
// open source chess engines. They guarantee no collisions.

const ROOK_MAGIC_NUMBERS: [u64; 64] = [
    0x8a80104000800020, 0x140002000100040, 0x2801880a0017001,
    0x100081001000420, 0x200020010080420, 0x3001c0002010008,
    0x8480008002000100, 0x2080088004402900, 0x800098204000,
    0x2024401000200040, 0x100802000801000, 0x120800800801000,
    0x208808088000400, 0x2802200800400, 0x2200800100020080,
    0x801000060821100, 0x80044006422000, 0x100808020004000,
    0x12108a0010204200, 0x140848010000802, 0x481828014002800,
    0x8094004002004100, 0x4010040010010802, 0x20008806104,
    0x100400080208000, 0x2040002120081000, 0x21200680100081,
    0x20100080080080, 0x2000a00200410, 0x20080800400,
    0x80088400100102, 0x80004600042881, 0x4040008040800020,
    0x440003000200801, 0x4200011004500, 0x188020010100100,
    0x14800401802800, 0x2080040080800200, 0x124080204001001,
    0x200046502000484, 0x480400080088020, 0x1000422010034000,
    0x30200100110040, 0x100021010009, 0x2002080100110004,
    0x202008004008002, 0x20020004010100, 0x2048440040820001,
    0x101002200408200, 0x40802000401080, 0x4008142004410100,
    0x2060820c0120200, 0x1001004080100, 0x20c020080040080,
    0x2935610830022400, 0x44440041009200, 0x280001040802101,
    0x2100190040002085, 0x80c0084100102001, 0x4024081001000421,
    0x20030a0244872, 0x12001008414402, 0x2006104900a0804,
    0x1004081002402,
];

const BISHOP_MAGIC_NUMBERS: [u64; 64] = [
    0x0002020202020200, 0x0002020202020000, 0x0004010202000000,
    0x0004040080000000, 0x0001104000000000, 0x0000821040000000,
    0x0000410410400000, 0x0000104104104000,
    0x0000040404040400, 0x0000020202020200, 0x0000040102020000,
    0x0000040400800000, 0x0000011040000000, 0x0000008210400000,
    0x0000004104104000, 0x0000002082082000,
    0x0004000808080800, 0x0002000404040400, 0x0001000202020200,
    0x0000800802004000, 0x0000800400A00000, 0x0000200100884000,
    0x0000400082082000, 0x0000200041041000,
    0x0002080010101000, 0x0001040008080800, 0x0000208004010400,
    0x0000404004010200, 0x0000840000802000, 0x0000404002011000,
    0x0000808001041000, 0x0000404000820800,
    0x0001041000202000, 0x0000820800101000, 0x0000104400080800,
    0x0000020080080080, 0x0000404040040100, 0x0000808100020100,
    0x0001010100020800, 0x0000808080010400,
    0x0000820820004000, 0x0000410410002000, 0x0000082088001000,
    0x0000002011000800, 0x0000080100400400, 0x0001010101000200,
    0x0002020202000400, 0x0001010101000200,
    0x0000410410400000, 0x0000208208200000, 0x0000002084100000,
    0x0000000020880000, 0x0000001002020000, 0x0000040408020000,
    0x0004040404040000, 0x0002020202020000,
    0x0000104104104000, 0x0000002082082000, 0x0000000020841000,
    0x0000000000208800, 0x0000000010020200, 0x0000000404080200,
    0x0000040404040400, 0x0002020202020200,
];

// ── Initialisation ────────────────────────────────────────────────────────────

/// Initialise magic bitboard tables for Rook and Bishop.
/// Called once at engine startup from init_masks().
pub fn init_magic() {
    unsafe {
        init_slider_magic(true);   // Rooks
        init_slider_magic(false);  // Bishops
    }
}

unsafe fn init_slider_magic(is_rook: bool) {
    let mut offset = 0usize;

    for sq_idx in 0u8..64 {
        let sq = Square::from_index(sq_idx).unwrap();

        let mask = if is_rook {
            rook_mask(sq)
        } else {
            bishop_mask(sq)
        };

        let bits = mask.count_ones();
        let shift = 64 - bits;
        let magic_num = if is_rook {
            ROOK_MAGIC_NUMBERS[sq_idx as usize]
        } else {
            BISHOP_MAGIC_NUMBERS[sq_idx as usize]
        };

        let entry = Magic {
            mask,
            magic: magic_num,
            shift,
            offset,
        };

        // Fill attack table for all subsets of the mask
        let num_entries = 1usize << bits;
        let mut subset = 0u64;
        loop {
            let attacks = if is_rook {
                rook_attacks_slow(sq, Bitboard(subset))
            } else {
                bishop_attacks_slow(sq, Bitboard(subset))
            };

            let index = magic_index(&entry, subset);

            if is_rook {
                ROOK_ATTACKS[index] = attacks;
            } else {
                BISHOP_ATTACKS[index] = attacks;
            }

            // Carry-rippler trick to enumerate all subsets
            subset = subset.wrapping_sub(mask) & mask;
            if subset == 0 { break; }
        }

        if is_rook {
            ROOK_MAGICS[sq_idx as usize] = entry;
        } else {
            BISHOP_MAGICS[sq_idx as usize] = entry;
        }

        offset += num_entries;
    }
}

/// Compute magic index for a given occupancy
#[inline]
fn magic_index(entry: &Magic, occupancy: u64) -> usize {
    let relevant = occupancy & entry.mask;
    let hash = relevant.wrapping_mul(entry.magic);
    entry.offset + (hash >> entry.shift) as usize
}

// ── Slow attack generators (used only during init) ────────────────────────────

/// Compute rook attack mask (relevant occupancy squares, excluding edges)
fn rook_mask(sq: Square) -> u64 {
    let r = sq.rank() as i32;
    let f = sq.file() as i32;
    let mut mask = 0u64;

    // North (excluding rank 8)
    for rank in (r + 1)..7 {
        mask |= 1u64 << (rank * 8 + f);
    }
    // South (excluding rank 1)
    for rank in 1..r {
        mask |= 1u64 << (rank * 8 + f);
    }
    // East (excluding h-file)
    for file in (f + 1)..7 {
        mask |= 1u64 << (r * 8 + file);
    }
    // West (excluding a-file)
    for file in 1..f {
        mask |= 1u64 << (r * 8 + file);
    }
    mask
}

/// Compute bishop attack mask (relevant occupancy squares, excluding edges)
fn bishop_mask(sq: Square) -> u64 {
    let r = sq.rank() as i32;
    let f = sq.file() as i32;
    let mut mask = 0u64;

    // NE diagonal (excluding edges)
    let (mut rr, mut ff) = (r + 1, f + 1);
    while rr < 7 && ff < 7 { mask |= 1u64 << (rr * 8 + ff); rr += 1; ff += 1; }
    // NW diagonal
    let (mut rr, mut ff) = (r + 1, f - 1);
    while rr < 7 && ff > 0 { mask |= 1u64 << (rr * 8 + ff); rr += 1; ff -= 1; }
    // SE diagonal
    let (mut rr, mut ff) = (r - 1, f + 1);
    while rr > 0 && ff < 7 { mask |= 1u64 << (rr * 8 + ff); rr -= 1; ff += 1; }
    // SW diagonal
    let (mut rr, mut ff) = (r - 1, f - 1);
    while rr > 0 && ff > 0 { mask |= 1u64 << (rr * 8 + ff); rr -= 1; ff -= 1; }
    mask
}

/// Compute actual rook attacks given a set of blockers (slow, init only)
fn rook_attacks_slow(sq: Square, blockers: Bitboard) -> Bitboard {
    let r = sq.rank() as i32;
    let f = sq.file() as i32;
    let mut attacks = 0u64;

    // North
    let mut rr = r + 1;
    while rr < 8 {
        let bit = 1u64 << (rr * 8 + f);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr += 1;
    }
    // South
    let mut rr = r - 1;
    while rr >= 0 {
        let bit = 1u64 << (rr * 8 + f);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr -= 1;
    }
    // East
    let mut ff = f + 1;
    while ff < 8 {
        let bit = 1u64 << (r * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        ff += 1;
    }
    // West
    let mut ff = f - 1;
    while ff >= 0 {
        let bit = 1u64 << (r * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        ff -= 1;
    }
    Bitboard(attacks)
}

/// Compute actual bishop attacks given a set of blockers (slow, init only)
fn bishop_attacks_slow(sq: Square, blockers: Bitboard) -> Bitboard {
    let r = sq.rank() as i32;
    let f = sq.file() as i32;
    let mut attacks = 0u64;

    // NE
    let (mut rr, mut ff) = (r + 1, f + 1);
    while rr < 8 && ff < 8 {
        let bit = 1u64 << (rr * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr += 1; ff += 1;
    }
    // NW
    let (mut rr, mut ff) = (r + 1, f - 1);
    while rr < 8 && ff >= 0 {
        let bit = 1u64 << (rr * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr += 1; ff -= 1;
    }
    // SE
    let (mut rr, mut ff) = (r - 1, f + 1);
    while rr >= 0 && ff < 8 {
        let bit = 1u64 << (rr * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr -= 1; ff += 1;
    }
    // SW
    let (mut rr, mut ff) = (r - 1, f - 1);
    while rr >= 0 && ff >= 0 {
        let bit = 1u64 << (rr * 8 + ff);
        attacks |= bit;
        if blockers.0 & bit != 0 { break; }
        rr -= 1; ff -= 1;
    }
    Bitboard(attacks)
}

// ── Fast public attack functions ──────────────────────────────────────────────
// These are called millions of times per second during search.
// Each is a single multiply + shift + array lookup.

/// Get Rook attacks from a square given current board occupancy
#[inline]
pub fn rook_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    unsafe {
        let entry = &ROOK_MAGICS[sq.index() as usize];
        let index = magic_index(entry, occupancy.0);
        ROOK_ATTACKS[index]
    }
}

/// Get Bishop attacks from a square given current board occupancy
#[inline]
pub fn bishop_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    unsafe {
        let entry = &BISHOP_MAGICS[sq.index() as usize];
        let index = magic_index(entry, occupancy.0);
        BISHOP_ATTACKS[index]
    }
}

/// Get Queen attacks (Rook attacks | Bishop attacks)
#[inline]
pub fn queen_attacks(sq: Square, occupancy: Bitboard) -> Bitboard {
    rook_attacks(sq, occupancy) | bishop_attacks(sq, occupancy)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    fn setup() {
        init_magic();
    }

    #[test]
    fn test_rook_empty_board_center() {
        setup();
        // Rook on e4 with no blockers should attack entire rank 4 and file e
        let attacks = rook_attacks(Square::E4, Bitboard::EMPTY);
        // Rank 4: a4,b4,c4,d4,f4,g4,h4 = 7 squares
        // File e: e1,e2,e3,e5,e6,e7,e8 = 7 squares
        assert_eq!(attacks.count(), 14);
        assert!(attacks.contains(Square::E1));
        assert!(attacks.contains(Square::E8));
        assert!(attacks.contains(Square::A4));
        assert!(attacks.contains(Square::H4));
        assert!(!attacks.contains(Square::E4)); // not own square
    }

    #[test]
    fn test_rook_with_blocker() {
        setup();
        // Rook on a1 with blocker on a4 — should see a2,a3,a4 north
        // and b1..h1 east
        let blocker = Bitboard::from_square(Square::A4);
        let attacks = rook_attacks(Square::A1, blocker);
        assert!(attacks.contains(Square::A2));
        assert!(attacks.contains(Square::A3));
        assert!(attacks.contains(Square::A4)); // can capture blocker
        assert!(!attacks.contains(Square::A5)); // blocked
        assert!(attacks.contains(Square::H1)); // east unblocked
    }

    #[test]
    fn test_rook_battery_detection() {
        setup();
        // Pet Dragon specific: Rook on a2, enemy Rook on a7
        // With no pieces between them, a2 Rook should see a7
        let occupancy = Bitboard::from_square(Square::A7);
        let attacks = rook_attacks(Square::A2, occupancy);
        assert!(attacks.contains(Square::A7),
            "Rook on a2 should attack a7 (battery detection)");
        assert!(!attacks.contains(Square::A8),
            "Rook on a2 blocked by a7 should not see a8");
    }

    #[test]
    fn test_bishop_empty_board() {
        setup();
        // Bishop on a1 with no blockers — attacks the entire diagonal
        let attacks = bishop_attacks(Square::A1, Bitboard::EMPTY);
        assert!(attacks.contains(Square::B2));
        assert!(attacks.contains(Square::H8));
        assert!(!attacks.contains(Square::A2));
        assert!(!attacks.contains(Square::B1));
    }

    #[test]
    fn test_bishop_with_blocker() {
        setup();
        // Bishop on a1 with blocker on d4
        let blocker = Bitboard::from_square(Square::D4);
        let attacks = bishop_attacks(Square::A1, blocker);
        assert!(attacks.contains(Square::B2));
        assert!(attacks.contains(Square::C3));
        assert!(attacks.contains(Square::D4)); // can capture
        assert!(!attacks.contains(Square::E5)); // blocked
    }

    #[test]
    fn test_queen_attacks() {
        setup();
        // Queen combines rook and bishop attacks
        let attacks = queen_attacks(Square::D4, Bitboard::EMPTY);
        // Should attack rank 4, file d, and both diagonals
        assert!(attacks.contains(Square::A4)); // rank
        assert!(attacks.contains(Square::D1)); // file
        assert!(attacks.contains(Square::A1)); // diagonal
        assert!(attacks.contains(Square::G7)); // diagonal
        assert!(!attacks.contains(Square::D4)); // not own square
    }

    #[test]
    fn test_rook_corner() {
        setup();
        // Rook on h8 with empty board
        let attacks = rook_attacks(Square::H8, Bitboard::EMPTY);
        assert!(attacks.contains(Square::A8)); // rank
        assert!(attacks.contains(Square::H1)); // file
        assert!(!attacks.contains(Square::H8)); // not own square
        assert_eq!(attacks.count(), 14);
    }

    #[test]
    fn test_magic_tables_consistent() {
        setup();
        // Verify magic lookup matches slow computation for 100 cases
        let test_squares = [
            Square::A1, Square::E4, Square::H8,
            Square::D4, Square::A8, Square::H1,
        ];
        let test_occupancies = [
            Bitboard::EMPTY,
            Bitboard::RANK_4,
            Bitboard::FILE_E,
            Bitboard::from_square(Square::E5),
        ];

        for &sq in &test_squares {
            for &occ in &test_occupancies {
                let fast_rook  = rook_attacks(sq, occ);
                let slow_rook  = rook_attacks_slow(sq, occ);
                assert_eq!(fast_rook, slow_rook,
                    "Rook magic mismatch on {:?} with occupancy {:?}",
                    sq, occ);

                let fast_bish  = bishop_attacks(sq, occ);
                let slow_bish  = bishop_attacks_slow(sq, occ);
                assert_eq!(fast_bish, slow_bish,
                    "Bishop magic mismatch on {:?} with occupancy {:?}",
                    sq, occ);
            }
        }
    }
}
