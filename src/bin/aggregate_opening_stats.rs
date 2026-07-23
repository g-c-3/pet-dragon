// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// src/bin/aggregate_opening_stats.rs — Phase 23.4 step 3 (D67, D71)
//
// Reads one or more opening-stats data files produced by
// `src/bin/selfplay.rs`'s second output stream (one line per game:
// `starting_seed | rook_files | knight_files | root_move_uci |
// game_result`), aggregates by structural bucket, and generates
// `src/opening_stats.rs` — a static, compile-time-baked lookup table for
// D67 step 5's root-move-ordering bias.
//
// Bucket key: (sorted rook_files, sorted knight_files) — D67's Tier 1
// design. NOTE (D71, Session 84): the true bucket count is larger than
// D67's original "420" estimate — rook/knight files are NOT drawn from
// disjoint pools (a file can host both a rook and a knight, at its two
// different ranks), so this table will be sparser and take longer to
// populate than originally planned. That's an accepted, corrected
// expectation, not a bug — see DECISIONS.md D71.
//
// Per bucket, keeps only the single best-win-rate root move that clears
// MIN_SAMPLES — matching D67's usage design (root-only move-ordering bias
// needs one favored move per bucket, not a full ranking). Buckets with no
// qualifying move are OMITTED from the output, not zero-filled — an absent
// bucket (no data) must stay distinguishable from "we checked and nothing
// stood out" at lookup time.
//
// Usage (no terminal needed — triggered via GitHub Actions
// workflow_dispatch, see .github/workflows/aggregate_opening_stats.yml):
//   cargo run --release --bin aggregate_opening_stats -- <comma,separated,file,paths>
//
// Matches texel_tune.rs's argument convention exactly (a single
// comma-separated string, not multiple positional args) — the workflow
// locates files from up to three sources (Run ID / committed path / URL)
// and joins them into one comma-separated list before invoking this
// binary. Pass as many files as you have accumulated across separate
// selfplay.yml runs; this binary reads and combines them all before
// aggregating, so old runs' data is never thrown away.
// ============================================================================

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;

/// Minimum samples required for a (bucket, root move) pair to be trusted
/// enough to bake into the table (D67 — standard small-sample floor).
const MIN_SAMPLES: usize = 30;

/// One parsed line from an opening-stats data file.
struct GameRecord {
    rook_files: [u8; 2],
    knight_files: [u8; 2],
    root_move_uci: String,
    game_result: f32,
}

/// Running tally for one (bucket, root move) pair.
#[derive(Default)]
struct MoveTally {
    count: u32,
    result_sum: f64,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let paths_arg = args.first().unwrap_or_else(|| {
        eprintln!("usage: aggregate_opening_stats <comma_separated_data_files>");
        std::process::exit(1);
    });
    let paths: Vec<String> = paths_arg.split(',').map(|s| s.trim().to_string()).collect();
    if paths.is_empty() || paths.iter().all(|p| p.is_empty()) {
        eprintln!("usage: aggregate_opening_stats <comma_separated_data_files> — got no files");
        std::process::exit(1);
    }

    let mut records = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path, e));
        let mut file_count = 0usize;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match parse_line(line) {
                Some(rec) => {
                    records.push(rec);
                    file_count += 1;
                }
                None => eprintln!("warning: skipping malformed line in {}: {}", path, line),
            }
        }
        eprintln!("{}: {} games", path, file_count);
    }
    eprintln!("total games across all input files: {}", records.len());

    // bucket key -> move UCI -> tally
    let mut buckets: HashMap<([u8; 2], [u8; 2]), HashMap<String, MoveTally>> = HashMap::new();
    for rec in &records {
        let bucket_key = (rec.rook_files, rec.knight_files);
        let move_tally = buckets
            .entry(bucket_key)
            .or_default()
            .entry(rec.root_move_uci.clone())
            .or_default();
        move_tally.count += 1;
        move_tally.result_sum += rec.game_result as f64;
    }

    eprintln!("distinct buckets observed: {}", buckets.len());

    // For each bucket, keep only the best-win-rate move that clears
    // MIN_SAMPLES. Buckets with no qualifying move are omitted entirely.
    let mut entries: Vec<(u16, String, f32, u32)> = Vec::new();
    for ((rook_files, knight_files), moves) in &buckets {
        let mut best: Option<(&str, f32, u32)> = None;
        for (mv, tally) in moves {
            if (tally.count as usize) < MIN_SAMPLES {
                continue;
            }
            let win_rate = (tally.result_sum / tally.count as f64) as f32;
            if best.is_none() || win_rate > best.unwrap().1 {
                best = Some((mv.as_str(), win_rate, tally.count));
            }
        }
        if let Some((mv, win_rate, count)) = best {
            let key = pack_key(*rook_files, *knight_files);
            entries.push((key, mv.to_string(), win_rate, count));
        }
    }
    entries.sort_by_key(|e| e.0);

    eprintln!(
        "buckets with a qualifying move (>= {} samples): {}",
        MIN_SAMPLES,
        entries.len()
    );
    if entries.is_empty() {
        eprintln!(
            "warning: zero qualifying entries — output table will be empty. \
             This is expected with limited data (see DECISIONS.md D71), not a bug. \
             Still generating a valid (empty) src/opening_stats.rs rather than failing, \
             so the build stays green — accumulate more games and re-run."
        );
    }

    write_output(&entries);
}

/// Parse one line: `seed|rf0,rf1|nf0,nf1|move|result`. `seed` is read but not
/// retained — it's only useful for reproducing a specific game, not for
/// aggregation.
fn parse_line(line: &str) -> Option<GameRecord> {
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() != 5 {
        return None;
    }
    let rook_files = parse_file_pair(parts[1])?;
    let knight_files = parse_file_pair(parts[2])?;
    let root_move_uci = parts[3].to_string();
    let game_result: f32 = parts[4].parse().ok()?;
    Some(GameRecord { rook_files, knight_files, root_move_uci, game_result })
}

fn parse_file_pair(s: &str) -> Option<[u8; 2]> {
    let vals: Vec<u8> = s.split(',').filter_map(|x| x.parse().ok()).collect();
    if vals.len() != 2 || vals.iter().any(|&f| f > 7) {
        return None;
    }
    let mut arr = [vals[0], vals[1]];
    arr.sort_unstable();
    Some(arr)
}

/// Pack a bucket key into 12 bits: 3 bits each for rook_file_0, rook_file_1,
/// knight_file_0, knight_file_1 (each 0..8, so 3 bits is exactly enough).
/// Kept as a plain u16 (not a struct) so the generated table can be a flat
/// `[(u16, ...); N]` sorted array with a cheap binary-search lookup — no
/// new dependency (e.g. `phf`) needed, matching this project's existing
/// zero-added-dependency pattern for every other generated/tuned table.
fn pack_key(rook_files: [u8; 2], knight_files: [u8; 2]) -> u16 {
    ((rook_files[0] as u16) << 9)
        | ((rook_files[1] as u16) << 6)
        | ((knight_files[0] as u16) << 3)
        | (knight_files[1] as u16)
}

fn write_output(entries: &[(u16, String, f32, u32)]) {
    let mut out = String::new();
    out.push_str(
        "// ============================================================================\n\
         // Pet Dragon Chess Engine\n\
         // Copyright (C) 2026 Gokul Chandar\n\
         // Licensed under GPL v3 — see LICENSE file\n\
         // Contributors: Claude (Anthropic)\n\
         //\n\
         // src/opening_stats.rs — GENERATED FILE, DO NOT HAND-EDIT.\n\
         // Produced by src/bin/aggregate_opening_stats.rs (Phase 23.4, D67/D71).\n\
         // Regenerate by re-running that binary against accumulated\n\
         // opening-stats data and committing the new output.\n\
         //\n\
         // Bucket key: 12-bit packed (rook_file_0<<9 | rook_file_1<<6 |\n\
         // knight_file_0<<3 | knight_file_1), each file 0..8, both pairs\n\
         // sorted ascending. Table is sorted by key for binary search.\n\
         // Every entry cleared a 30-sample minimum (see aggregate_opening_stats.rs);\n\
         // an ABSENT bucket means \"no data\", not \"no edge found\" — callers must\n\
         // not treat a missing lookup as a negative signal.\n\
         // ============================================================================\n\n",
    );
    out.push_str(&format!(
        "/// (packed_key, best_move_uci, win_rate, sample_count)\n\
         pub static OPENING_STATS: [(u16, &str, f32, u32); {}] = [\n",
        entries.len()
    ));
    for (key, mv, win_rate, count) in entries {
        out.push_str(&format!(
            "    ({}, \"{}\", {:.4}, {}),\n",
            key, mv, win_rate, count
        ));
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// Binary search lookup by packed bucket key. Returns `None` if this\n\
         /// bucket has no entry that cleared the sample threshold — treat as\n\
         /// \"no data\", not as a signal to avoid every move (D67's usage design:\n\
         /// degrade gracefully to normal search on a miss).\n\
         pub fn lookup(rook_files: [u8; 2], knight_files: [u8; 2]) -> Option<(&'static str, f32, u32)> {\n\
         \u{20}   let mut rf = rook_files;\n\
         \u{20}   rf.sort_unstable();\n\
         \u{20}   let mut nf = knight_files;\n\
         \u{20}   nf.sort_unstable();\n\
         \u{20}   let key = ((rf[0] as u16) << 9) | ((rf[1] as u16) << 6) | ((nf[0] as u16) << 3) | (nf[1] as u16);\n\
         \u{20}   OPENING_STATS\n\
         \u{20}       .binary_search_by_key(&key, |e| e.0)\n\
         \u{20}       .ok()\n\
         \u{20}       .map(|i| (OPENING_STATS[i].1, OPENING_STATS[i].2, OPENING_STATS[i].3))\n\
         }\n\n\
         #[cfg(test)]\n\
         mod tests {\n\
         \u{20}   use super::*;\n\n\
         \u{20}   #[test]\n\
         \u{20}   fn test_table_sorted_by_key() {\n\
         \u{20}       for w in OPENING_STATS.windows(2) {\n\
         \u{20}           assert!(w[0].0 < w[1].0, \"table must be strictly sorted and deduplicated by key for binary_search_by_key to work\");\n\
         \u{20}       }\n\
         \u{20}   }\n\n\
         \u{20}   #[test]\n\
         \u{20}   fn test_lookup_miss_returns_none() {\n\
         \u{20}       // 0/1 vs 6/7 is an unlikely-to-collide probe key; if this ever\n\
         \u{20}       // starts failing because the table grew to include it, that's\n\
         \u{20}       // fine — swap to a different obviously-absent key.\n\
         \u{20}       if !OPENING_STATS.iter().any(|e| e.0 == 0b000_001_110_111) {\n\
         \u{20}           assert_eq!(lookup([0, 1], [6, 7]), None);\n\
         \u{20}       }\n\
         \u{20}   }\n\
         }\n",
    );

    let path = "src/opening_stats.rs";
    let mut f = fs::File::create(path).unwrap_or_else(|e| panic!("failed to create {}: {}", path, e));
    f.write_all(out.as_bytes()).expect("failed to write output");
    eprintln!("wrote {} ({} entries)", path, entries.len());
}
