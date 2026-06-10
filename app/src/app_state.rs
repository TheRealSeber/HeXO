//! Centralized App state. No logic — just data + reset helper.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;
use hexo_engine::{Coord, GameState, Player};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Hotseat,
    HumanVsAi { human_is: Player },
    AiVsAi,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::HumanVsAi {
            human_is: Player::P1,
        }
    }
}

pub struct AiWorker {
    pub rx: mpsc::Receiver<Coord>,
    pub cancel: Arc<AtomicBool>,
    pub started_at: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub offset: egui::Vec2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset: egui::Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

pub const HEX_SIZE_PX: f32 = 28.0;

pub struct App {
    pub game: GameState,
    pub mode: Mode,
    pub camera: Camera,
    pub hover: Option<Coord>,
    pub ai_worker: Option<AiWorker>,
    pub last_move: Option<Coord>,
    pub ai_iterations: u32,
    pub paused: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            game: GameState::new(),
            mode: Mode::default(),
            camera: Camera::default(),
            hover: None,
            ai_worker: None,
            last_move: None,
            ai_iterations: 50_000,
            paused: false,
        }
    }
}

impl App {
    pub fn reset(&mut self) {
        if let Some(w) = &self.ai_worker {
            w.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.ai_worker = None;
        self.game = GameState::new();
        self.hover = None;
        self.last_move = None;
        self.paused = false;
    }
}
