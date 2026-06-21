# 🐉 Pet Dragon

> *The world's first original chess variant engine built natively in Rust*

**Created by Gokul Chandar**
**Engine contributors: Claude (Anthropic)**
**Licensed under GPL v3**

---

## Play Now

🎮 **[Play Pet Dragon in your browser](https://g-c-3.github.io/pet-dragon)**

No download required. Play against Stockfish directly in your browser.

---

## What is Pet Dragon?

Pet Dragon is an original chess variant where every game starts from a
unique position. The King stays home — but everything else finds its
own place.

Built from the ground up in Rust. Not a fork. Not a port. Purpose-built
for Pet Dragon from day one.

---

## The Rules

### Starting Position

- The **White King always starts on e1** (its standard square)
- The remaining **15 White pieces** — 1 Queen, 2 Rooks, 2 Bishops,
  2 Knights, 8 Pawns — are placed **randomly across ranks 1 and 2**
- The two **Bishops must be on opposite coloured squares**
- **Black mirrors White exactly** — same file, opposite rank
  (rank 1 → rank 8, rank 2 → rank 7, same piece)
- The standard chess starting position is one valid Pet Dragon arrangement

### Pawns

- White pawns always move **toward rank 8**. Black toward **rank 1**.
  Direction never changes regardless of starting square.
- A pawn may **double-step on its very first move** from wherever it
  actually started:
  - White pawn on rank 1 → can jump to rank 3
  - White pawn on rank 2 → can jump to rank 4 (standard chess)
  - Same logic applies for Black from ranks 8 and 7
- **En passant** follows naturally from the double-step
- **Promotion** — White on rank 8, Black on rank 1

### Castling

Available **only** if the King and Rook happen to start on their
standard chess squares:

| Side | Kingside | Queenside |
|---|---|---|
| White | King e1 + Rook h1 | King e1 + Rook a1 |
| Black | King e8 + Rook h8 | King e8 + Rook a8 |

Since the King is always on e1/e8, castling depends entirely on
whether the randomly placed Rooks land on their standard squares.
All standard castling conditions apply.

### Everything Else

Identical to standard FIDE chess — checkmate, stalemate, draws,
promotion, en passant, insufficient material, threefold repetition,
fifty-move rule.

---

## The Engine

Pet Dragon is powered by a purpose-built Rust chess engine:

- **Pure Rust** — written from scratch, no forks, no ports
- **Bitboard representation** — industry-standard 64-bit board
- **Magic bitboards** — fastest known sliding piece attack generation
- **Alpha-beta search with PVS** — Principal Variation Search
- **Iterative deepening** — searches deeper within time limits
- **Lazy SMP** — multi-threaded parallel search
- **Handcrafted evaluation** — material, mobility, pawn structure,
  king safety, piece-square tables
- **Transposition table** — never evaluates the same position twice
- **UCI protocol** — works with any chess GUI
- **WebAssembly** — runs natively in any modern browser
- **Target: 3000+ Elo** without neural networks

### Evaluation draws from (GPL v3, with attribution)
- **Ethereal** (Andrew Grant) — piece-square tables, mobility weights
- **Stockfish** — king safety, search techniques
- **Reckless** — Rust engine architecture reference

---

## Download

Desktop binaries are available on the
**[Releases page](https://github.com/g-c-3/pet-dragon/releases)**
for Windows, macOS, and Linux.

The engine speaks UCI — connect it to any chess GUI such as
Arena, BanksiaGUI, or CuteChess.

---

## Build from Source

```bash
# Native binary
cargo build --release

# WebAssembly
wasm-pack build --target web --release
```

Requires Rust 1.75+ and wasm-pack for WASM builds.

---

## Project Status

| Phase | Status | Description |
|---|---|---|
| Scaffold & types | 🔄 In progress | Core data types |
| Bitboards | ⏳ Pending | Board representation |
| Position generator | ⏳ Pending | Pet Dragon setup |
| Move generation | ⏳ Pending | All legal moves |
| Search | ⏳ Pending | Alpha-beta + PVS |
| Evaluation | ⏳ Pending | HCE |
| UCI protocol | ⏳ Pending | GUI compatibility |
| Browser deployment | ⏳ Pending | WASM + GitHub Pages |
| Search improvements | ⏳ Pending | LMR, null move, etc. |
| Texel tuning | ⏳ Pending | 3000+ Elo target |

---

## License

Pet Dragon is free software licensed under the
[GNU General Public License v3](LICENSE).

Copyright © 2026 Gokul Chandar. All rights reserved.

---

## Contributing

Pet Dragon is an open source project. Contributions, issues, and
discussions are welcome.

---

*Pet Dragon — an original chess variant by Gokul Chandar*
*Engine built in Rust from the ground up*
*Contributors: Claude (Anthropic)*
