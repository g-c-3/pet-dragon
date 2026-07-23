// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/opening_stats.rs — GENERATED FILE, DO NOT HAND-EDIT.
// Produced by src/bin/aggregate_opening_stats.rs (Phase 23.4, D67/D71).
// Regenerate by re-running that binary against accumulated
// opening-stats data and committing the new output.
//
// Bucket key: 12-bit packed (rook_file_0<<9 | rook_file_1<<6 |
// knight_file_0<<3 | knight_file_1), each file 0..8, both pairs
// sorted ascending. Table is sorted by key for binary search.
// Every entry cleared a 30-sample minimum (see aggregate_opening_stats.rs);
// an ABSENT bucket means "no data", not "no edge found" — callers must
// not treat a missing lookup as a negative signal.
// ============================================================================

/// (packed_key, best_move_uci, win_rate, sample_count)
pub static OPENING_STATS: [(u16, &str, f32, u32); 2] = [
    (207, "a2a7", 0.9677, 31),
    (399, "a2a7", 0.9, 30),
];

/// Binary search lookup by packed bucket key. Returns `None` if this
/// bucket has no entry that cleared the sample threshold — treat as
/// "no data", not as a signal to avoid every move (D67's usage design:
/// degrade gracefully to normal search on a miss).
pub fn lookup(rook_files: [u8; 2], knight_files: [u8; 2]) -> Option<(&'static str, f32, u32)> {
    let mut rf = rook_files;
    rf.sort_unstable();
    let mut nf = knight_files;
    nf.sort_unstable();
    let key = ((rf[0] as u16) << 9) | ((rf[1] as u16) << 6) | ((nf[0] as u16) << 3) | (nf[1] as u16);
    OPENING_STATS
        .binary_search_by_key(&key, |e| e.0)
        .ok()
        .map(|i| (OPENING_STATS[i].1, OPENING_STATS[i].2, OPENING_STATS[i].3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_sorted_by_key() {
        for w in OPENING_STATS.windows(2) {
            assert!(w[0].0 < w[1].0, "table must be strictly sorted and deduplicated by key for binary_search_by_key to work");
        }
    }

    #[test]
    fn test_lookup_miss_returns_none() {
        // 0/1 vs 6/7 is an unlikely-to-collide probe key; if this ever
        // starts failing because the table grew to include it, that's
        // fine — swap to a different obviously-absent key.
        if !OPENING_STATS.iter().any(|e| e.0 == 0b000_001_110_111) {
            assert_eq!(lookup([0, 1], [6, 7]), None);
        }
    }
}
