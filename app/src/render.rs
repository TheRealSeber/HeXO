//! Hex board renderer. Pure paint, no mutation.

use eframe::egui;
use hexo_engine::Player;

use crate::app_state::{App, HEX_SIZE_PX};
use crate::coords::{axial_to_pixel, hex_corners, pixel_to_axial};

const BG: egui::Color32 = egui::Color32::from_rgb(0x1A, 0x1A, 0x1F);
const EMPTY_OUTLINE: egui::Color32 = egui::Color32::from_rgb(0x2E, 0x2E, 0x36);
const LEGAL_OUTLINE: egui::Color32 = egui::Color32::from_rgb(0x42, 0x42, 0x4C);
const P1_FILL: egui::Color32 = egui::Color32::from_rgb(0xE0, 0x7A, 0x2A); // orange
const P2_FILL: egui::Color32 = egui::Color32::from_rgb(0x3F, 0xA0, 0xD6); // blue
const LAST_MOVE_BORDER: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF);

pub fn paint_board(app: &App, ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, BG);

    let size = HEX_SIZE_PX * app.camera.zoom;
    let origin = rect.center() + app.camera.offset;

    // Visible axial bounds via inverse-projecting the rect corners.
    let to_local = |p: egui::Pos2| -> egui::Vec2 { p - origin };
    let corners_local = [
        to_local(rect.left_top()),
        to_local(rect.right_top()),
        to_local(rect.left_bottom()),
        to_local(rect.right_bottom()),
    ];
    let mut min_q = i32::MAX;
    let mut max_q = i32::MIN;
    let mut min_r = i32::MAX;
    let mut max_r = i32::MIN;
    for c in corners_local {
        let (q, r) = pixel_to_axial(c, size);
        min_q = min_q.min(q);
        max_q = max_q.max(q);
        min_r = min_r.min(r);
        max_r = max_r.max(r);
    }
    let margin = 2;
    min_q -= margin;
    max_q += margin;
    min_r -= margin;
    max_r += margin;

    let legal = app.game.legal_moves_set();
    let occupied: std::collections::HashMap<(i32, i32), Player> =
        app.game.placed_stones().into_iter().collect();
    let current = app.game.current_player();

    // Pass 1: empty legal-move outlines + hover ghost. Skip non-legal empties (infinite grid).
    for q in min_q..=max_q {
        for r in min_r..=max_r {
            let coord = (q, r);
            if occupied.contains_key(&coord) {
                continue;
            }
            let is_legal = legal.contains(&coord);
            let is_hover = app.hover == Some(coord);
            if !is_legal && !is_hover {
                continue;
            }
            let center = origin + axial_to_pixel(q, r, size);
            let pts = hex_corners(center, size * 0.92);
            let fill = if is_hover && is_legal {
                ghost_color(current).unwrap_or(egui::Color32::TRANSPARENT)
            } else {
                egui::Color32::TRANSPARENT
            };
            painter.add(egui::Shape::convex_polygon(
                pts.to_vec(),
                fill,
                egui::Stroke::new(1.0, LEGAL_OUTLINE),
            ));
        }
    }

    // Pass 2: occupied cells (drawn on top so last-move border isn't covered).
    for (&(q, r), &p) in &occupied {
        let center = origin + axial_to_pixel(q, r, size);
        let pts = hex_corners(center, size * 0.92);
        let fill = match p {
            Player::P1 => P1_FILL,
            Player::P2 => P2_FILL,
        };
        let stroke = if app.last_move == Some((q, r)) {
            egui::Stroke::new(2.5, LAST_MOVE_BORDER)
        } else {
            egui::Stroke::new(1.0, EMPTY_OUTLINE)
        };
        painter.add(egui::Shape::convex_polygon(pts.to_vec(), fill, stroke));
    }

    response
}

fn ghost_color(p: Option<Player>) -> Option<egui::Color32> {
    let c = match p? {
        Player::P1 => P1_FILL,
        Player::P2 => P2_FILL,
    };
    Some(egui::Color32::from_rgba_unmultiplied(
        c.r(),
        c.g(),
        c.b(),
        90,
    ))
}

const WIN_LINE_COLOR: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xC8, 0x66);

fn winning_line(state: &hexo_engine::GameState) -> Option<Vec<(i32, i32)>> {
    let winner = state.winner()?;
    let stones = state.placed_stones();
    let by_player: std::collections::HashSet<(i32, i32)> = stones
        .iter()
        .filter(|(_, p)| *p == winner)
        .map(|(c, _)| *c)
        .collect();
    let win_length = state.config().win_length as usize;
    let axes: [(i32, i32); 3] = [(1, 0), (0, 1), (1, -1)];
    for &start in &by_player {
        for &(dq, dr) in &axes {
            let mut line = Vec::with_capacity(win_length);
            for i in 0..win_length {
                let step = i as i32;
                let c = (start.0 + dq * step, start.1 + dr * step);
                if by_player.contains(&c) {
                    line.push(c);
                } else {
                    break;
                }
            }
            if line.len() == win_length {
                return Some(line);
            }
        }
    }
    None
}

pub fn paint_win_overlay(app: &App, ui: &mut egui::Ui, rect: egui::Rect) {
    let Some(line) = winning_line(&app.game) else {
        return;
    };
    if line.len() < 2 {
        return;
    }
    let painter = ui.painter_at(rect);
    let size = HEX_SIZE_PX * app.camera.zoom;
    let origin = rect.center() + app.camera.offset;
    let a = origin + axial_to_pixel(line[0].0, line[0].1, size);
    let b = origin + axial_to_pixel(line[line.len() - 1].0, line[line.len() - 1].1, size);
    painter.line_segment([a, b], egui::Stroke::new(6.0, WIN_LINE_COLOR));
}
