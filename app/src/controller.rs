//! AI dispatch + per-frame game loop.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use ai::{Mcts, MctsConfig};

use crate::app_state::{AiWorker, App, Mode};

pub fn tick(app: &mut App, ctx: &egui::Context) {
    if app.game.is_terminal() {
        if let Some(w) = &app.ai_worker {
            w.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        app.ai_worker = None;
        return;
    }

    // Paused: cancel any running AI worker and don't spawn new ones until resumed.
    if app.paused {
        if let Some(w) = &app.ai_worker {
            w.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        app.ai_worker = None;
        return;
    }

    // Poll worker channel.
    if let Some(worker) = &app.ai_worker {
        match worker.rx.try_recv() {
            Ok(coord) => {
                if app.game.apply_move(coord).is_ok() {
                    app.last_move = Some(coord);
                }
                app.ai_worker = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                app.ai_worker = None;
            }
        }
    }

    // Spawn a worker if AI's turn and none running.
    if app.ai_worker.is_none() && is_ai_turn(app) {
        spawn(app);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn is_ai_turn(app: &App) -> bool {
    let Some(p) = app.game.current_player() else {
        return false;
    };
    match app.mode {
        Mode::Hotseat => false,
        Mode::HumanVsAi { human_is } => p != human_is,
        Mode::AiVsAi => true,
    }
}

fn spawn(app: &mut App) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::sync_channel::<hexo_engine::Coord>(1);
    let state = app.game.clone();
    let iterations = app.ai_iterations;
    let cancel_thread = Arc::clone(&cancel);
    thread::spawn(move || {
        let mut mcts = Mcts::new(MctsConfig {
            iterations,
            exploration_c: std::f32::consts::SQRT_2,
            seed: None,
        });
        if let Some(mv) = mcts.choose_move(&state, &cancel_thread) {
            let _ = tx.send(mv);
        }
    });
    app.ai_worker = Some(AiWorker {
        rx,
        cancel,
        started_at: Instant::now(),
    });
}
