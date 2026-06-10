//! HeXO Monte Carlo Tree Search. Pure UCB1 + uniform-random rollouts.
//!
//! `choose_move` first runs a cheap tactical scan (catches 1-ply wins/blocks),
//! then dispatches root-parallelized MCTS across all rayon worker threads. Each
//! thread runs an independent tree; we aggregate root-child visit counts at the
//! end and pick the most-visited move.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::Rng;
use rand::SeedableRng;
use rand::seq::IteratorRandom;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;

use hexo_engine::{Coord, GameState, Player};

/// MCTS configuration knobs.
#[derive(Debug, Clone)]
pub struct MctsConfig {
    pub iterations: u32,
    pub exploration_c: f32,
    pub seed: Option<u64>,
}

impl Default for MctsConfig {
    fn default() -> Self {
        Self {
            iterations: 50_000,
            exploration_c: std::f32::consts::SQRT_2,
            seed: None,
        }
    }
}

/// Re-entrant Monte Carlo Tree Search engine.
pub struct Mcts {
    cfg: MctsConfig,
}

impl Mcts {
    pub fn new(cfg: MctsConfig) -> Self {
        Self { cfg }
    }

    /// Returns the highest-visit-count root child move after running up to
    /// `cfg.iterations` simulations, or `None` if the state is already terminal
    /// or the search was cancelled before completing a single iteration.
    pub fn choose_move(&mut self, state: &GameState, cancel: &AtomicBool) -> Option<Coord> {
        if state.is_terminal() {
            return None;
        }
        let root_player = state.current_player()?;

        // Tactical preamble: catches 1-ply wins and 1-ply blocks BEFORE MCTS.
        // Pure UCB1 + random rollouts misses these reliably at low iteration budgets and
        // sometimes even at high ones (HeXO's 2-stones-per-turn injects rollout noise).
        if let Some(mv) = find_immediate_tactical(state, root_player) {
            return Some(mv);
        }

        // Root parallelization: split iterations across N independent trees via rayon,
        // then merge by summing visits per root move. Each tree has its own arena and RNG;
        // no shared mutable state during search, just the `cancel` AtomicBool (Sync).
        let n_threads = rayon::current_num_threads().max(1);
        let per_thread = (self.cfg.iterations / n_threads as u32).max(1);
        let exploration_c = self.cfg.exploration_c;
        let seed = self.cfg.seed;

        let per_tree: Vec<Vec<(Coord, u32)>> = (0..n_threads)
            .into_par_iter()
            .map(|i| {
                let tree_seed = seed.map(|s| s.wrapping_add(i as u64));
                run_one_tree(
                    state,
                    root_player,
                    per_thread,
                    exploration_c,
                    tree_seed,
                    cancel,
                )
            })
            .collect();

        let mut totals: HashMap<Coord, u32> = HashMap::new();
        for tree in per_tree {
            for (mv, visits) in tree {
                *totals.entry(mv).or_insert(0) += visits;
            }
        }
        totals.into_iter().max_by_key(|&(_, v)| v).map(|(mv, _)| mv)
    }
}

/// Run a single MCTS tree to `iterations` simulations (or until `cancel` is set).
/// Returns the visit count of each root child move so multiple trees can be merged.
fn run_one_tree(
    state: &GameState,
    root_player: Player,
    iterations: u32,
    exploration_c: f32,
    seed: Option<u64>,
    cancel: &AtomicBool,
) -> Vec<(Coord, u32)> {
    let mut rng = match seed {
        Some(s) => ChaCha8Rng::seed_from_u64(s),
        None => ChaCha8Rng::from_os_rng(),
    };

    let mut nodes: Vec<Node> = Vec::with_capacity((iterations as usize).min(1 << 18));
    nodes.push(Node::new(state, None));
    const ROOT: usize = 0;

    for _ in 0..iterations {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // -- SELECT --
        let mut path: Vec<usize> = vec![ROOT];
        let mut sim_state = state.clone();
        loop {
            let cur = *path.last().unwrap();
            if nodes[cur].terminal {
                break;
            }
            if !nodes[cur].untried.is_empty() {
                break;
            }
            if nodes[cur].children.is_empty() {
                nodes[cur].terminal = true;
                break;
            }
            let parent_visits = nodes[cur].visits.max(1);
            let ln_n = f32::ln(parent_visits as f32);
            let mut best = nodes[cur].children[0];
            let mut best_score = f32::NEG_INFINITY;
            for &(mv, child) in &nodes[cur].children {
                let n = &nodes[child];
                let q = if n.visits == 0 {
                    0.0
                } else {
                    n.total_score / n.visits as f32
                };
                let u = exploration_c * (ln_n / (n.visits.max(1) as f32)).sqrt();
                let score = q + u;
                if score > best_score {
                    best_score = score;
                    best = (mv, child);
                }
            }
            let (mv, child) = best;
            sim_state
                .apply_move(mv)
                .expect("UCB1-selected child move was illegal");
            path.push(child);
        }

        // -- EXPAND --
        let leaf = *path.last().unwrap();
        if !nodes[leaf].terminal && !nodes[leaf].untried.is_empty() {
            let idx = rng.random_range(0..nodes[leaf].untried.len());
            let mv = nodes[leaf].untried.swap_remove(idx);
            let mover = nodes[leaf].player_to_move;
            sim_state.apply_move(mv).expect("untried move was illegal");
            let new_node = Node::new(&sim_state, mover);
            let child_idx = nodes.len();
            nodes.push(new_node);
            nodes[leaf].children.push((mv, child_idx));
            path.push(child_idx);
        }

        // -- SIMULATE --
        let outcome = simulate(&mut sim_state, &mut rng, root_player);

        // -- BACKPROP --
        for &node_idx in &path {
            let pov = nodes[node_idx].chosen_by;
            let signed = match pov {
                Some(p) if p == root_player => outcome,
                Some(_) => -outcome,
                None => outcome,
            };
            nodes[node_idx].visits += 1;
            nodes[node_idx].total_score += signed;
        }
    }

    nodes[ROOT]
        .children
        .iter()
        .map(|&(mv, idx)| (mv, nodes[idx].visits))
        .collect()
}

struct Node {
    player_to_move: Option<Player>,
    chosen_by: Option<Player>, // player who made the move into this node (None for root)
    visits: u32,
    total_score: f32,
    untried: Vec<Coord>,
    children: Vec<(Coord, usize)>,
    terminal: bool,
}

impl Node {
    fn new(state: &GameState, chosen_by: Option<Player>) -> Self {
        let terminal = state.is_terminal();
        let untried = if terminal {
            Vec::new()
        } else {
            state.legal_moves()
        };
        Self {
            player_to_move: state.current_player(),
            chosen_by,
            visits: 0,
            total_score: 0.0,
            untried,
            children: Vec::new(),
            terminal,
        }
    }
}

/// Returns a tactically-forced move if one exists at the current state:
///
/// 1. A move that wins immediately for the player to move.
/// 2. Otherwise, a move that blocks an opponent's 1-ply forced win.
///
/// This is a cheap scan over legal moves (~µs per move) that runs once per
/// `choose_move` call to catch tactics MCTS rollouts miss at low iteration budgets.
fn find_immediate_tactical(state: &GameState, me: Player) -> Option<Coord> {
    let opp = me.opponent();
    let stones: std::collections::HashMap<Coord, Player> =
        state.placed_stones().into_iter().collect();
    let win_len = state.config().win_length as usize;
    let legal = state.legal_moves_set();

    if let Some(&c) = legal
        .iter()
        .find(|&&c| is_immediate_win_at(&stones, c, me, win_len))
    {
        return Some(c);
    }
    legal
        .iter()
        .find(|&&c| is_immediate_win_at(&stones, c, opp, win_len))
        .copied()
}

/// Would placing `player`'s stone at `c` complete a `win_len`-in-a-row along any of the 3 axes?
fn is_immediate_win_at(
    stones: &std::collections::HashMap<Coord, Player>,
    c: Coord,
    player: Player,
    win_len: usize,
) -> bool {
    let axes: [(i32, i32); 3] = [(1, 0), (0, 1), (1, -1)];
    for &(dq, dr) in &axes {
        let mut count = 1;
        let mut p = (c.0 + dq, c.1 + dr);
        while stones.get(&p) == Some(&player) {
            count += 1;
            p = (p.0 + dq, p.1 + dr);
        }
        let mut p = (c.0 - dq, c.1 - dr);
        while stones.get(&p) == Some(&player) {
            count += 1;
            p = (p.0 - dq, p.1 - dr);
        }
        if count >= win_len {
            return true;
        }
    }
    false
}

/// Biased rollout to terminal. Returns +1 if root_player wins, 0 draw, -1 loss.
///
/// Each move: with probability `LINE_BIAS`, the active player plays an empty cell that
/// is axis-adjacent to one of their own stones (i.e. extends a line). Otherwise picks
/// uniformly at random over legal moves. This injects "players extend their own lines"
/// into rollouts, which is the strategic signal pure random rollouts lack — MCTS now
/// sees that early-game moves leading to 2-in-row / 3-in-row positions translate into
/// downstream wins.
fn simulate(state: &mut GameState, rng: &mut ChaCha8Rng, root_player: Player) -> f32 {
    const LINE_BIAS: f32 = 0.8;
    let axes: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

    while !state.is_terminal() {
        let active = match state.current_player() {
            Some(p) => p,
            None => break,
        };
        let legal = state.legal_moves_set();
        if legal.is_empty() {
            break;
        }

        let pick: Option<Coord> = if rng.random::<f32>() < LINE_BIAS {
            // Collect axis-adjacent empty cells around the active player's stones.
            let mut candidates: Vec<Coord> = Vec::new();
            for (c, p) in state.placed_stones() {
                if p != active {
                    continue;
                }
                for &(dq, dr) in &axes {
                    let n = (c.0 + dq, c.1 + dr);
                    if legal.contains(&n) {
                        candidates.push(n);
                    }
                }
            }
            if candidates.is_empty() {
                legal.iter().copied().choose(rng)
            } else {
                candidates.into_iter().choose(rng)
            }
        } else {
            legal.iter().copied().choose(rng)
        };

        match pick {
            Some(c) => {
                if state.apply_move(c).is_err() {
                    break;
                }
            }
            None => break,
        }
    }
    match state.winner() {
        Some(p) if p == root_player => 1.0,
        Some(_) => -1.0,
        None => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use hexo_engine::{Coord, GameConfig, GameState, Player};

    use super::*;

    fn play(cfg: GameConfig, coords: &[Coord]) -> GameState {
        let mut g = GameState::with_config(cfg);
        for &c in coords {
            g.apply_move(c)
                .unwrap_or_else(|e| panic!("test setup move {:?} illegal: {:?}", c, e));
        }
        g
    }

    fn cancel_off() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// Small-config helper so tests can build positions quickly with tight branching.
    fn small() -> GameConfig {
        GameConfig {
            win_length: 4,
            placement_radius: 3,
            max_moves: 80,
        }
    }

    #[test]
    fn mcts_finds_immediate_win() {
        // P1 has 3-in-a-row along q-axis at r=2 (q = -2, -1, 0... wait P1 forced at (0,0)).
        // Build line at r=2 by P1: (-2,2), (-1,2), (0,2)... actually let me make it
        // (0,0) (forced) + later P1 plays at (-2,2), (-1,2), (0,2), (1,2): then 4-in-row.
        // Need a setup where P1 has 3 stones in a row (not 4) and one move left.
        // win_length=4. Build: (0,0) forced, then sequence that gives P1 3-in-a-row r=2
        // with one stone left to play this turn.
        //
        // The structure of turns: P1(1), P2(2), P1(2), P2(2), P1(2), ...
        // After move 1: P1 has 1 stone, P2's turn (2 left)
        // After move 3: P2 has 2 stones, P1's turn (2 left)
        // After move 5: P1 has 3 stones, P2's turn (2 left)
        // After move 7: P2 has 4 stones, P1's turn (2 left)
        // After move 9: P1 has 5 stones, P2's turn (2 left)
        //
        // Plan: P1 stones (0,0), (-2,2), (-1,2), and one more from later turn.
        // Make line at r=2: (-2,2),(-1,2),(0,2) — 3-in-row needs P1 to play (1,2) to win.
        //
        // Sequence:
        // 1. P1 (0,0)            — forced
        // 2. P2 (3,0)
        // 3. P2 (3,1)
        // 4. P1 (-2,2)
        // 5. P1 (-1,2)           — P1 has (0,0),(-2,2),(-1,2)
        // 6. P2 (4,0)
        // 7. P2 (4,1)
        // 8. P1 (0,2)            — P1 has 3-in-row at r=2: (-2,2),(-1,2),(0,2). One move left.
        let g = play(
            small(),
            &[(3, 0), (3, 1), (-2, 2), (-1, 2), (4, 0), (4, 1), (0, 2)],
        );
        assert_eq!(g.current_player(), Some(Player::P1));
        assert_eq!(g.moves_remaining_this_turn(), 1);
        assert!(!g.is_terminal());

        let mut mcts = Mcts::new(MctsConfig {
            iterations: 5_000,
            exploration_c: std::f32::consts::SQRT_2,
            seed: Some(42),
        });
        let mv = mcts
            .choose_move(&g, &cancel_off())
            .expect("non-terminal -> Some");
        assert!(
            mv == (-3, 2) || mv == (1, 2),
            "expected immediate winning move at (-3,2) or (1,2), got {:?}",
            mv
        );
    }

    #[test]
    fn mcts_blocks_opponent_immediate_win() {
        // P2 has 3-in-row, it's P1's turn with 2 stones to play. P1 must block.
        //
        // Sequence (win_length=4):
        // 1. P1 (0,0) forced
        // 2. P2 (-1,-2)
        // 3. P2 (0,-2)            — P2 has 2-in-row at r=-2
        // 4. P1 (3,0)
        // 5. P1 (3,1)
        // 6. P2 (1,-2)            — P2 has 3-in-row at r=-2: (-1,-2),(0,-2),(1,-2). One move left this turn.
        //                           Need P2 to NOT yet win. Open ends: (-2,-2) and (2,-2).
        //                           So P2 places its second move elsewhere.
        // 7. P2 (4,0)             — P2 done. Now P1's turn.
        let g = play(
            small(),
            &[(-1, -2), (0, -2), (3, 0), (3, 1), (1, -2), (4, 0)],
        );
        assert!(
            !g.is_terminal(),
            "setup terminated early; rewrite play sequence"
        );
        assert_eq!(g.current_player(), Some(Player::P1));

        // Pure UCB1 + uniform-random rollouts has known limits on 2-ply tactical threats
        // when combined with HeXO's 2-stones-per-turn rule (the same player gets to recover
        // on their second stone, so first-stone choice has low signal in random rollouts).
        // We bump iterations high here; if this still fails the test is marked as a known
        // weakness rather than a bug.
        let mut mcts = Mcts::new(MctsConfig {
            iterations: 100_000,
            exploration_c: std::f32::consts::SQRT_2,
            seed: Some(7),
        });
        let mv = mcts
            .choose_move(&g, &cancel_off())
            .expect("non-terminal -> Some");
        let blocks = [(-2, -2), (2, -2)];
        assert!(
            blocks.contains(&mv),
            "expected MCTS to block at one of {:?}, got {:?}",
            blocks,
            mv
        );
    }

    #[test]
    fn mcts_handles_terminal_root() {
        // P1 wins with 4-in-row at r=0. P2 stones scattered to avoid accidental line.
        // 1. P1 (0,0) forced
        // 2. P2 (3,-1), (3,1)     — different axes, no line
        // 3. P1 (1,0), (2,0)      — P1 has 3-in-row at r=0
        // 4. P2 (-1,3), (1,-3)    — scattered
        // 5. P1 (3,0)             — 4-in-row at r=0. Terminal.
        let g = play(
            small(),
            &[(3, -1), (3, 1), (1, 0), (2, 0), (-1, 3), (1, -3), (3, 0)],
        );
        assert!(g.is_terminal(), "setup did not terminate; check sequence");

        let mut mcts = Mcts::new(MctsConfig::default());
        assert_eq!(mcts.choose_move(&g, &cancel_off()), None);
    }

    #[test]
    fn mcts_respects_cancel() {
        let g = GameState::new();
        let cancel = AtomicBool::new(true);
        let mut mcts = Mcts::new(MctsConfig {
            iterations: u32::MAX,
            exploration_c: 1.4,
            seed: Some(1),
        });
        let start = std::time::Instant::now();
        let _ = mcts.choose_move(&g, &cancel);
        assert!(
            start.elapsed().as_secs() < 5,
            "choose_move did not honor cancel"
        );
        cancel.store(false, Ordering::Relaxed);
    }
}
