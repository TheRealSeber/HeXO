# HeXO

Native desktop client for **HeXO** — infinite hexagonal tic-tac-toe (6-in-a-row), with a Monte Carlo Tree Search AI opponent.

Built on top of the [`hexo-engine`](../hexo-engine) Rust crate (vendored as a sibling directory).

## Run

```sh
cargo run --release -p app
```

Opens a 1100×800 window. An orange X is placed automatically at the origin (forced opening); the AI plays blue (O) by default.

## Modes

- **Hotseat** — two humans share one mouse
- **You (X) vs AI** — you play X / orange
- **You (O) vs AI** — you play O / blue
- **AI vs AI** — watch two MCTS instances duel

## Controls

| Action | Input |
|---|---|
| Place a stone | Click on a legal hex |
| Pan the board | Drag with the mouse |
| Zoom | Scroll wheel |
| Pause / resume an AI game | "Pause" / "Resume" button in sidebar |
| Restart game | "Restart" button |
| Reset camera | "Reset view" button |
| Tune AI strength | "AI iterations" slider (1k–200k, default 50k) |

Pause cancels the AI's in-flight search and stops it from spawning new ones until you resume — useful in AI vs AI to freeze the position mid-game.

## Architecture

Cargo workspace, two crates:

- **`ai`** — Monte Carlo Tree Search.
  - Tactical preamble: a 1-ply scan that catches immediate wins / blocks before any tree search.
  - Root-parallel MCTS via [`rayon`](https://crates.io/crates/rayon): N independent trees, results merged by aggregating root-child visit counts.
  - Biased rollouts: in simulation, the active player picks an axis-adjacent extension of one of their own stones 80% of the time. This gives MCTS the signal that line-building correlates with wins; without it, random rollouts produce scattered noise.
- **`app`** — egui frontend. Hex grid renderer, pan/zoom camera, AI dispatched on a worker thread with a cancellable `AtomicBool`.

Game logic comes from `../hexo-engine/hexo-engine` (path dep, not modified).

See `docs/design.md` for the full design.

## Game rules (from the `FULL_HEXO` engine defaults)

- Win: 6 stones in a row along any of the 3 hex axes
- P1 (X) is forced to open at axial `(0,0)` — canonically equivalent to "X plays anywhere" by rotation/translation symmetry
- All subsequent turns: active player places exactly 2 stones; sequence is P2, P1, P2, P1, ...
- Legal moves: any empty hex within distance 8 of any placed stone
- Draw at the 200-move cap

## Tests

```sh
cargo test --release
```

6 tests pass: 4 in `ai/` (immediate win, immediate block, terminal-root handling, cancel handling), 2 in `app/` (coord round-trip and cube-round boundary).

## Lints

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --workspace -- -D warnings
```

Both clean.

## Build profile

`Cargo.toml` enables `lto = "thin"`, `codegen-units = 1`, and `strip = "symbols"` for release. Release binary is ~4.6 MB; first build takes ~25 s, incremental ~7 s.
