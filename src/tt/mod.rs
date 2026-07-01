// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// tt/mod.rs — Transposition Table
//
// The transposition table (TT) is a hash table mapping position hashes
// to previously computed search results. When the search reaches a position
// it has seen before (via a different move order), it retrieves the cached
// result instead of re-searching from scratch.
//
// Why it matters:
//   Chess positions are reached via many different move orders.
//   Without TT: search the same position thousands of times.
//   With TT: search it once, retrieve result on all subsequent visits.
//   Effect: effectively multiplies search depth by ~2-3x.
//
// Design:
//   Fixed-size array indexed by (hash % size).
//   Size is always a power of 2 — enables fast indexing via bitwise AND.
//   Default size: 64MB (configurable via UCI Hash option).
//   Replacement policy: prefer deeper entries, replace on age.
//   Lock-free: Lazy SMP threads read/write without locking.
//   Benign races accepted — occasional hash corruption is tolerable.
//   This is the standard Stockfish approach.
//
// Entry structure:
//   key:   upper bits of Zobrist hash (verification)
//   depth: search depth this result came from
//   score: evaluation score
//   bound: Exact / LowerBound / UpperBound
//   mv:    best move found (for move ordering)
//   age:   search generation (for replacement)
// ============================================================================

use crate::types::Move;

// ── Bound type ────────────────────────────────────────────────────────────────

/// What the stored score represents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Bound {
    /// Score is exact — full window search confirmed this value
    Exact      = 0,
    /// Score is a lower bound — caused a beta cutoff (fail-high)
    LowerBound = 1,
    /// Score is an upper bound — all moves failed low (fail-low)
    UpperBound = 2,
}

// ── TT Entry ──────────────────────────────────────────────────────────────────

/// One entry in the transposition table
/// Packed to 16 bytes for cache efficiency
#[derive(Clone, Copy)]
pub struct TTEntry {
    /// Upper 32 bits of Zobrist hash — used to verify correct entry
    /// (lower bits used for table index, upper bits for verification)
    pub key:   u32,
    /// Search depth this result was computed at
    /// Higher depth = more reliable result
    pub depth: i8,
    /// Bound type for this entry
    pub bound: Bound,
    /// Search generation when this entry was written
    /// Used for aging/replacement — older entries replaced first
    pub age:   u8,
    /// Best move found at this position (for move ordering)
    /// Move::NULL if no best move stored
    pub mv:    Move,
    /// The score for this position
    /// Note: mate scores are stored relative to the position,
    /// adjusted when reading to be relative to root
    pub score: i32,
}

impl TTEntry {
    /// Empty/invalid entry
    pub const EMPTY: Self = Self {
        key:   0,
        depth: 0,
        bound: Bound::UpperBound,
        age:   0,
        mv:    Move::NULL,
        score: 0,
    };

    /// Is this entry valid? (has real data)
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.key != 0
    }
}

// ── Transposition Table ───────────────────────────────────────────────────────

pub struct TranspositionTable {
    /// The hash table entries
    entries: Vec<TTEntry>,
    /// Number of entries (always power of 2)
    size:    usize,
    /// Mask for fast indexing: index = hash & mask
    mask:    usize,
    /// Current search generation (incremented each search)
    /// Used to identify stale entries from previous searches
    age:     u8,
}

impl TranspositionTable {
    /// Create a TT with the given size in megabytes
    /// Size is rounded down to nearest power of 2 in entries
    pub fn new(size_mb: usize) -> Self {
        let bytes         = size_mb * 1024 * 1024;
        let entry_size    = std::mem::size_of::<TTEntry>();
        let num_entries   = (bytes / entry_size).next_power_of_two() / 2;
        let num_entries   = num_entries.max(1024); // minimum 1024 entries

        TranspositionTable {
            entries: vec![TTEntry::EMPTY; num_entries],
            size:    num_entries,
            mask:    num_entries - 1,
            age:     0,
        }
    }

    /// Default 64MB table
    pub fn default_size() -> Self {
        Self::new(64)
    }

    /// Increment age — call at the start of each new search
    /// This marks all existing entries as "old"
    #[inline]
    pub fn new_search(&mut self) {
        self.age = self.age.wrapping_add(1);
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.fill(TTEntry::EMPTY);
        self.age = 0;
    }

    /// Resize the table (called when UCI Hash option changes)
    pub fn resize(&mut self, size_mb: usize) {
        *self = Self::new(size_mb);
    }

    /// Get the table index for a hash
    #[inline]
    fn index(&self, hash: u64) -> usize {
        (hash as usize) & self.mask
    }

    /// Get the verification key from a hash (upper 32 bits)
    #[inline]
    fn key_from_hash(hash: u64) -> u32 {
        (hash >> 32) as u32
    }

    // ── Store ─────────────────────────────────────────────────────────────────

    /// OLD: Store a result in the TT.
    /// Replacement policy:
    ///   - Always replace if entry is from an older search generation
    ///   - Always replace if same position (update with better info)
    ///   - Replace if new depth >= existing depth
    ///   - Otherwise keep existing (deeper = more reliable)
    /// NEW: Store a result in the TT.
    /// Takes `&self` (not `&mut self`) so it can be called from multiple
    /// threads sharing the same `Arc<TranspositionTable>`.
    /// Concurrent writes may race — this is accepted per D4 (benign races).
    pub fn store(
        &self,
        hash:  u64,
        depth: i8,
        score: i32,
        bound: Bound,
        mv:    Move,
    ) {
        let idx      = self.index(hash);
        let new_key  = Self::key_from_hash(hash);
        let existing = &self.entries[idx];

        // Replacement decision
        let should_replace =
            existing.key == 0
            || existing.age != self.age
            || existing.key == new_key
            || depth >= existing.depth;

        if should_replace {
            let best_mv = if mv == Move::NULL && existing.key == new_key {
                existing.mv
            } else {
                mv
            };
            let new_entry = TTEntry {
                key:   new_key,
                depth,
                bound,
                age:   self.age,
                mv:    best_mv,
                score,
            };
            // SAFETY: lock-free design per D4. Concurrent writes at most produce
            // a corrupted entry; probe() catches this via key verification.
            unsafe {
                let ptr = self.entries.as_ptr().add(idx) as *mut TTEntry;
                ptr.write(new_entry);
            }
        }
    }

    // ── Probe ─────────────────────────────────────────────────────────────────

    /// Look up a position in the TT.
    /// Returns Some(entry) if found and verified, None if miss.
    ///
    /// Usage in search:
    ///   if let Some(entry) = tt.probe(pos.hash) {
    ///       if entry.depth >= remaining_depth {
    ///           match entry.bound {
    ///               Exact      => return entry.score,
    ///               LowerBound => alpha = alpha.max(entry.score),
    ///               UpperBound => beta  = beta.min(entry.score),
    ///           }
    ///           if alpha >= beta { return entry.score; }
    ///       }
    ///       // Use entry.mv for move ordering even if score not usable
    ///   }
    #[inline]
    pub fn probe(&self, hash: u64) -> Option<TTEntry> {
        let idx     = self.index(hash);
        let entry   = self.entries[idx];
        let exp_key = Self::key_from_hash(hash);

        if entry.key == exp_key && entry.is_valid() {
            Some(entry)
        } else {
            None
        }
    }

    /// Probe for just the best move (for move ordering even on TT miss)
    /// Returns the stored move if the key matches, NULL otherwise
    #[inline]
    pub fn probe_move(&self, hash: u64) -> Move {
        let idx     = self.index(hash);
        let entry   = self.entries[idx];
        let exp_key = Self::key_from_hash(hash);

        if entry.key == exp_key {
            entry.mv
        } else {
            Move::NULL
        }
    }

    // ── Mate score adjustment ─────────────────────────────────────────────────
    // Mate scores must be adjusted when storing/retrieving from TT.
    // A mate in 3 from root is a mate in 2 from depth 1, etc.
    // We store mate scores relative to current ply, adjust on retrieval.

    /// Adjust score before storing in TT (convert from root-relative to ply-relative)
    #[inline]
    pub fn score_to_tt(score: i32, ply: i32) -> i32 {
        if score >= Self::MATE_THRESHOLD {
            score + ply
        } else if score <= -Self::MATE_THRESHOLD {
            score - ply
        } else {
            score
        }
    }

    /// Adjust score after retrieving from TT (convert from ply-relative to root-relative)
    #[inline]
    pub fn score_from_tt(score: i32, ply: i32) -> i32 {
        if score >= Self::MATE_THRESHOLD {
            score - ply
        } else if score <= -Self::MATE_THRESHOLD {
            score + ply
        } else {
            score
        }
    }

    /// Threshold above which a score is considered a mate score
    pub const MATE_THRESHOLD: i32 = 30_000;

    // ── Statistics ────────────────────────────────────────────────────────────

    /// Estimate TT fill percentage (0-100)
    /// Samples first 1000 entries for performance
    pub fn fill_permille(&self) -> u32 {
        let sample = self.size.min(1000);
        let filled = self.entries[..sample]
            .iter()
            .filter(|e| e.age == self.age && e.is_valid())
            .count();
        (filled * 1000 / sample) as u32
    }

    /// Number of entries in the table
    pub fn capacity(&self) -> usize {
        self.size
    }

    /// Size in megabytes (approximate)
    pub fn size_mb(&self) -> usize {
        self.size * std::mem::size_of::<TTEntry>() / (1024 * 1024)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

// SAFETY: TranspositionTable uses lock-free design with benign races (D4).
// Multiple threads may call store() concurrently; at worst an entry is
// partially overwritten, which probe() detects via Zobrist key mismatch.
// This is identical to Stockfish's approach — no crashes, at most one
// slightly suboptimal move per race event.
unsafe impl Send for TranspositionTable {}
unsafe impl Sync for TranspositionTable {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Move, MoveKind, Square};

    fn test_move() -> Move {
        Move::new(Square::E2, Square::E4, MoveKind::DoublePush)
    }

    #[test]
    fn test_tt_basic_store_probe() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;
        let mv     = test_move();

        tt.store(hash, 5, 100, Bound::Exact, mv);

        let entry = tt.probe(hash).expect("Should find stored entry");
        assert_eq!(entry.depth, 5);
        assert_eq!(entry.score, 100);
        assert_eq!(entry.bound, Bound::Exact);
        assert_eq!(entry.mv,    mv);
    }

    #[test]
    fn test_tt_miss_on_wrong_hash() {
        let mut tt   = TranspositionTable::new(1);
        let hash     = 0x1234_5678_9ABC_DEF0u64;
        let bad_hash = 0xDEAD_BEEF_CAFE_1234u64;

        tt.store(hash, 5, 100, Bound::Exact, Move::NULL);

        // Different hash should not match
        assert!(tt.probe(bad_hash).is_none(),
            "Different hash should not match");
    }

    #[test]
    fn test_tt_replacement_deeper_wins() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;

        // Store shallow entry
        tt.store(hash, 3, 50, Bound::Exact, Move::NULL);
        // Store deeper entry — should replace
        tt.store(hash, 7, 200, Bound::Exact, Move::NULL);

        let entry = tt.probe(hash).unwrap();
        assert_eq!(entry.depth, 7,
            "Deeper entry should replace shallower");
        assert_eq!(entry.score, 200);
    }

    #[test]
    fn test_tt_age_replacement() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;

        // Store in generation 0
        tt.store(hash, 10, 999, Bound::Exact, Move::NULL);

        // New search — increment age
        tt.new_search();

        // Store shallow entry in new generation — should replace old
        tt.store(hash, 2, 42, Bound::Exact, Move::NULL);

        let entry = tt.probe(hash).unwrap();
        assert_eq!(entry.score, 42,
            "New generation entry should replace old regardless of depth");
    }

    #[test]
    fn test_tt_clear() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;

        tt.store(hash, 5, 100, Bound::Exact, Move::NULL);
        assert!(tt.probe(hash).is_some());

        tt.clear();
        assert!(tt.probe(hash).is_none(),
            "Entry should be gone after clear");
    }

    #[test]
    fn test_tt_probe_move() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;
        let mv     = test_move();

        tt.store(hash, 5, 100, Bound::Exact, mv);

        assert_eq!(tt.probe_move(hash), mv,
            "probe_move should return stored move");
        assert_eq!(tt.probe_move(0xDEAD_BEEF_CAFE_1234u64), Move::NULL,
            "probe_move should return NULL on miss");
    }

    #[test]
    fn test_tt_bound_types() {
        let mut tt = TranspositionTable::new(1);

        let hash1 = 0x1111_1111_1111_1111u64;
        let hash2 = 0x2222_2222_2222_2222u64;
        let hash3 = 0x3333_3333_3333_3333u64;

        tt.store(hash1, 5, 100, Bound::Exact,      Move::NULL);
        tt.store(hash2, 5, 200, Bound::LowerBound, Move::NULL);
        tt.store(hash3, 5, 300, Bound::UpperBound, Move::NULL);

        assert_eq!(tt.probe(hash1).unwrap().bound, Bound::Exact);
        assert_eq!(tt.probe(hash2).unwrap().bound, Bound::LowerBound);
        assert_eq!(tt.probe(hash3).unwrap().bound, Bound::UpperBound);
    }

    #[test]
    fn test_mate_score_adjustment() {
        // Mate in 3 from root (score = MATE - 3)
        let mate_score = TranspositionTable::MATE_THRESHOLD + 100;

        // Store: adjust for ply 2
        let stored = TranspositionTable::score_to_tt(mate_score, 2);
        assert_eq!(stored, mate_score + 2);

        // Retrieve: adjust back for ply 2
        let retrieved = TranspositionTable::score_from_tt(stored, 2);
        assert_eq!(retrieved, mate_score,
            "Mate score should round-trip through TT adjustment");
    }

    #[test]
    fn test_normal_score_no_adjustment() {
        let score = 250i32;
        assert_eq!(TranspositionTable::score_to_tt(score, 5), score);
        assert_eq!(TranspositionTable::score_from_tt(score, 5), score,
            "Normal scores should not be adjusted");
    }

    #[test]
    fn test_tt_size() {
        let tt = TranspositionTable::new(64);
        assert!(tt.capacity() > 0);
        assert!(tt.size_mb() <= 64,
            "TT should not exceed requested size");
    }

    #[test]
    fn test_tt_fill_permille_empty() {
        let tt = TranspositionTable::new(1);
        assert_eq!(tt.fill_permille(), 0,
            "Empty TT should have 0 fill");
    }

    #[test]
    fn test_tt_preserve_move_on_update() {
        let mut tt = TranspositionTable::new(1);
        let hash   = 0x1234_5678_9ABC_DEF0u64;
        let mv     = test_move();

        // Store with a good move
        tt.store(hash, 5, 100, Bound::Exact, mv);

        // Update same position with NULL move — should preserve original move
        tt.store(hash, 5, 150, Bound::Exact, Move::NULL);

        let entry = tt.probe(hash).unwrap();
        assert_eq!(entry.mv, mv,
            "Good move should be preserved when updating with NULL move");
    }

    #[test]
    fn test_tt_multiple_hashes() {
        let mut tt = TranspositionTable::new(4);

        // Store many different positions
        for i in 0u64..100 {
            let hash = i.wrapping_mul(0x517C_C1B7_2722_0A95);
            tt.store(hash, 5, i as i32, Bound::Exact, Move::NULL);
        }

        // Verify as many as possible still retrievable
        // (some may have been overwritten due to hash collisions)
        let mut found = 0u32;
        for i in 0u64..100 {
            let hash = i.wrapping_mul(0x517C_C1B7_2722_0A95);
            if let Some(entry) = tt.probe(hash) {
                assert_eq!(entry.score, i as i32);
                found += 1;
            }
        }
        // Should find most of them
        assert!(found > 50,
            "Should retrieve most stored entries, found {}/100", found);
    }
}
