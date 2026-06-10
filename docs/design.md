# HeXO Desktop App — Design

Native Rust desktop client for **HeXO** — infinite hexagonal tic-tac-toe (6-in-a-row) — with a Monte Carlo Tree Search AI opponent. Built on top of the [`hexo-engine`](../../hexo-engine) Rust crate (vendored as a sibling directory), which provides game state, win detection, legal-move generation, and the sparse axial-coordinate board.

## Game rules

Default `FULL_HEXO` config from `hexo-engine`:

- **Win:** 6 stones in a row along any of the 3 hex axes `(1,0)`, `(0,1)`, `(-1,1)`.
- **Opening:** P1 (X) is forced at axial `(0,0)`. By rotation/translation symmetry this is canonically equivalent to "X plays anywhere" — matches BKE notation convention where the first X is always labelled `A0`.
- **Turns from t=2 onwards:** active player places exactly **2 stones** per turn, sequence P2, P1, P2, P1, ...
- **Legal moves:** any empty hex within distance 8 of any placed stone.
- **Draw:** 200-move cap with no winner.

Player colours match the community convention (HeXO Analysis, k.e.atawan):

- **P1 / X** = orange (`#E07A2A`)
- **P2 / O** = blue (`#3FA0D6`)

MVP renders solid colour hexes only (no X/O glyphs).

## Repo layout

The app sits in `HeXO/` next to the vendored engine. The engine is its own workspace; we consume it via a `path = "../hexo-engine/hexo-engine"` dep and never modify it.

```
HeXO/
├── Cargo.toml                   # workspace
├── README.md
├── app/                         # bin crate — egui frontend
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # eframe entry + App impl
│       ├── app_state.rs         # App struct, Mode, Camera, AiWorker
│       ├── controller.rs        # per-frame AI dispatch + game loop
│       ├── coords.rs            # axial<->pixel hex math
│       ├── render.rs            # board + stones + win line drawing
│       ├── input.rs             # pan / zoom / hover / click
│       └── sidebar.rs           # left-panel UI
├── ai/                          # lib crate — pure MCTS
│   ├── Cargo.toml
│   └── src/lib.rs               # MctsConfig, Mcts, choose_move
└── docs/
    └── design.md
```

## AI architecture

Two layers stacked inside `Mcts::choose_move`:

### 1. Tactical preamble (1-ply scan, ~µs)

Before any MCTS work, scan the legal moves once:

- If any move would *complete* a 6-in-a-row for the current player → return it.
- Otherwise, if any move would *complete* a 6-in-a-row for the opponent → block it.

This catches obvious tactics deterministically. Random rollouts miss them often, especially at low iteration budgets.

### 2. Root-parallel MCTS (rayon)

Iteration budget is split across `rayon::current_num_threads()` independent trees:

- Each thread has its own arena, RNG, and full game-state clones — no shared mutable state during search.
- The cancel flag is `&AtomicBool` (Sync), readable by all threads.
- After all threads finish (or cancel fires), root-child visit counts are summed across trees; the move with the most aggregate visits wins.

Each tree runs standard 4-phase MCTS: **select** (UCB1 down to a node with untried moves or terminal), **expand** (one untried move from the leaf), **simulate** (biased rollout to terminal), **backprop** (negamax-style sign flip based on which player chose the move into each node along the path).

### 3. Biased rollouts

Random uniform rollouts give MCTS no signal that line-building correlates with wins. Replaced with a soft heuristic:

- With probability **0.8**, the rollout's active player picks a move axis-adjacent to one of their own existing stones (extends a line).
- With probability **0.2**, picks uniformly at random over legal moves (keeps exploration).

This injects "players extend their own lines" into the simulation. Combined with backprop, MCTS now sees that early-game moves leading to clustered structures translate into downstream wins — and emergent play resembles real HeXO formations (Triangle, Arch, etc.).

## Threading model

AI search runs on a `std::thread::spawn` worker.

- The worker captures a *cloned* `GameState` (`~500 ns` per the engine's benchmark), an `Arc<AtomicBool>` cancel flag, and an `mpsc::sync_channel(1)` sender.
- The UI thread polls `rx.try_recv()` each frame. On receipt, applies the move and drops the worker.
- On Restart / mode change / **Pause**, the controller sets `cancel = true` and drops the handle. The worker exits cleanly at its next loop iteration.

Inside `choose_move`, rayon further parallelizes across the search trees within that single worker invocation.

## UI structure

Two egui panels:

- `SidePanel::left` (240px): mode selector dropdown, status line, iteration budget slider (1k–200k, log scale), AI thinking spinner with elapsed time, Pause/Resume, Restart, Reset view.
- `CentralPanel`: hex board with pan-by-drag, zoom-by-scroll, hover ghost on legal cells, last-move emphasis, and the winning 6-line highlighted on terminal.

## Modes

- **Hotseat** — two humans alternate on one mouse.
- **You (X) vs AI** — human plays X.
- **You (O) vs AI** — human plays O.
- **AI vs AI** — two MCTS instances duel hands-off.

Pause is exposed in all AI-involving modes; cancels the in-flight search and gates further spawns until resumed.

## Coordinate math

Pointy-top axial `(q, r)`. Pixel layout (size = base × camera zoom):

```
x = size * sqrt(3) * (q + r/2)
y = size * 1.5 * r
```

Inverse via standard cube-round. Camera transform is `screen_center + offset + pixel_from_axial`.

## Error handling

System-boundary defensive only:

- `apply_move` returns `Result<(), MoveError>`. The click handler gates on `legal_moves_set` so a `MoveError` shouldn't surface — if it does, log and ignore, never panic.
- Worker channel disconnect (e.g. UI dropped receiver on restart) is a clean exit for the worker.
- No `unwrap()` on user-driven paths. `expect()` is reserved for impossible-program-state asserts (UCB1-selected child must be a legal move).

## Tests

`cargo test --release`:

- 4 MCTS tests in `ai/`: finds immediate win, blocks opponent's immediate win, handles already-terminal root, respects cancel.
- 2 coord tests in `app/`: axial↔pixel round-trip, cube-round near grid boundaries.

UI verified manually.

## Non-goals

Out of scope for the MVP:

- BKE-notation move log in sidebar
- Ring/sector overlay around origin
- Formation auto-recognition (Triangle, Open Two, ...)
- Preset opening positions
- Multiplayer / save / load / undo
- X/O glyphs on hex tiles
- Localization toggle
