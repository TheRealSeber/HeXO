//! Left sidebar: mode selector, status, AI knobs, restart.

use eframe::egui;
use hexo_engine::Player;

use crate::app_state::{App, Camera, Mode};

pub fn draw(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("HeXO");
    ui.add_space(8.0);

    ui.label("Mode:");
    let mut mode = app.mode;
    egui::ComboBox::from_id_salt("mode")
        .selected_text(label_for(mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut mode, Mode::Hotseat, "Hotseat (2 humans)");
            ui.selectable_value(
                &mut mode,
                Mode::HumanVsAi {
                    human_is: Player::P1,
                },
                "You (X) vs AI",
            );
            ui.selectable_value(
                &mut mode,
                Mode::HumanVsAi {
                    human_is: Player::P2,
                },
                "You (O) vs AI",
            );
            ui.selectable_value(&mut mode, Mode::AiVsAi, "AI vs AI");
        });
    if mode != app.mode {
        app.mode = mode;
        app.reset();
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(format!("Move #{}", app.game.move_count()));
    match app.game.current_player() {
        Some(p) => {
            let label = match p {
                Player::P1 => "X (orange)",
                Player::P2 => "O (blue)",
            };
            ui.label(format!(
                "{} to play ({} left)",
                label,
                app.game.moves_remaining_this_turn()
            ));
        }
        None => {
            let label = match app.game.winner() {
                Some(Player::P1) => "X (orange) wins!",
                Some(Player::P2) => "O (blue) wins!",
                None => "Draw — move limit reached",
            };
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xC8, 0x66), label);
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label("AI iterations:");
    ui.add(egui::Slider::new(&mut app.ai_iterations, 1_000..=200_000).logarithmic(true));

    if let Some(w) = &app.ai_worker {
        let secs = w.started_at.elapsed().as_secs_f32();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(format!("AI thinking… {:.1}s", secs));
        });
    }

    ui.add_space(8.0);

    // Pause control — only relevant when an AI plays.
    if !matches!(app.mode, Mode::Hotseat) {
        let label = if app.paused { "Resume" } else { "Pause" };
        if ui.button(label).clicked() {
            app.paused = !app.paused;
        }
        if app.paused {
            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xC8, 0x66), "Paused");
        }
        ui.add_space(4.0);
    }

    if ui.button("Restart").clicked() {
        app.reset();
    }
    if ui.button("Reset view").clicked() {
        app.camera = Camera::default();
    }
}

fn label_for(m: Mode) -> &'static str {
    match m {
        Mode::Hotseat => "Hotseat (2 humans)",
        Mode::HumanVsAi {
            human_is: Player::P1,
        } => "You (X) vs AI",
        Mode::HumanVsAi {
            human_is: Player::P2,
        } => "You (O) vs AI",
        Mode::AiVsAi => "AI vs AI",
    }
}
