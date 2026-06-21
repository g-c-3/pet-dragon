// ============================================================================
// Pet Dragon Chess Engine
// Copyright (C) 2026 Gokul Chandar
// Licensed under GPL v3 — see LICENSE file
// Contributors: Claude (Anthropic)
//
// main.rs — Native binary entry point
//
// This is what runs when someone launches Pet Dragon on their desktop.
// Chess GUIs (Arena, BanksiaGUI, CuteChess) communicate with this binary
// via the UCI protocol over stdin/stdout.
//
// Phase 1: Minimal placeholder — prints engine info, compiles cleanly.
// Phase 9: Full UCI communication loop added here.
// ============================================================================

// Import our library crate — all engine logic lives in lib.rs and its modules
use pet_dragon_lib::types;

fn main() {
    // ── Engine banner ────────────────────────────────────────────────────────
    // Printed when the engine first starts.
    // UCI-compatible GUIs ignore non-UCI output before "uci" command,
    // so this is safe to print here.

    println!("Pet Dragon Chess Engine");
    println!("Copyright (C) 2026 Gokul Chandar");
    println!("Licensed under GPL v3");
    println!("Contributors: Claude (Anthropic)");
    println!();
    println!("Status: Initialising...");

    // ── Startup checks ───────────────────────────────────────────────────────
    // Confirm core types are accessible.
    // This will grow in later phases to initialise:
    //   - Attack tables (Phase 2)
    //   - Zobrist keys (Phase 3)
    //   - Transposition table (Phase 6)

    // Verify Color type works
    let white = types::Color::White;
    let black = types::Color::Black;
    println!("Colors loaded:  {:?} / {:?}", white, black);

    // Verify Square type works
    let e1 = types::Square::E1;
    let e8 = types::Square::E8;
    println!("King squares:   {:?} (White) / {:?} (Black)", e1, e8);

    // Verify PieceKind type works
    let king = types::PieceKind::King;
    println!("King piece:     {:?}", king);

    println!();
    println!("Engine ready.");
    println!();

    // ── UCI loop placeholder ─────────────────────────────────────────────────
    // Phase 9 replaces this with the full UCI protocol handler.
    // For now we just wait for "quit" so the binary doesn't immediately exit
    // (useful for testing that the binary runs correctly).

    println!("Type 'quit' to exit.");
    println!("Full UCI protocol coming in Phase 9.");
    println!();

    // Read lines from stdin until "quit"
    let mut input = String::new();
    loop {
        input.clear();
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => break,          // EOF — stdin closed
            Ok(_) => {
                let cmd = input.trim();
                match cmd {
                    "quit" | "exit" => {
                        println!("Pet Dragon exiting. Goodbye.");
                        break;
                    }
                    "uci" => {
                        // Minimal UCI response so GUIs don't hang
                        // Full implementation in Phase 9
                        println!("id name Pet Dragon");
                        println!("id author Gokul Chandar");
                        println!("uciok");
                    }
                    "isready" => {
                        println!("readyok");
                    }
                    "" => {}         // Ignore empty lines
                    other => {
                        println!(
                            "Unknown command: '{}' \
                             (full UCI support in Phase 9)",
                            other
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}
