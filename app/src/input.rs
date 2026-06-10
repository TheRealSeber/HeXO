//! Pointer + keyboard handling. Mutates camera, hover; applies clicked moves.

use eframe::egui;

use crate::app_state::{App, HEX_SIZE_PX, Mode};
use crate::coords::pixel_to_axial;

pub fn handle(app: &mut App, ui: &egui::Ui, response: &egui::Response) {
    // Pan: drag with primary button.
    if response.dragged_by(egui::PointerButton::Primary) {
        app.camera.offset += response.drag_delta();
    }

    // Zoom: scroll wheel while hovering the board.
    if response.hovered() {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = (scroll * 0.005).exp();
            app.camera.zoom = (app.camera.zoom * factor).clamp(0.3, 3.0);
        }
    }

    // Hover: only when pointer is inside the rect and not currently dragging.
    let size = HEX_SIZE_PX * app.camera.zoom;
    let origin = response.rect.center() + app.camera.offset;
    let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
    app.hover = match pointer {
        Some(p) if response.rect.contains(p) && !response.dragged() => {
            let coord = pixel_to_axial(p - origin, size);
            if app.game.legal_moves_set().contains(&coord) {
                Some(coord)
            } else {
                None
            }
        }
        _ => None,
    };

    // Click: apply move if it's a human's turn.
    if response.clicked()
        && !response.dragged()
        && let Some(coord) = app.hover
        && is_human_turn(app)
        && app.game.apply_move(coord).is_ok()
    {
        app.last_move = Some(coord);
    }
}

fn is_human_turn(app: &App) -> bool {
    let Some(player) = app.game.current_player() else {
        return false;
    };
    match app.mode {
        Mode::Hotseat => true,
        Mode::HumanVsAi { human_is } => player == human_is,
        Mode::AiVsAi => false,
    }
}
