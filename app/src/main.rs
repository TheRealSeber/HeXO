mod app_state;
mod controller;
mod coords;
mod input;
mod render;
mod sidebar;

use eframe::egui;

use app_state::App;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 800.0])
            .with_title("HeXO"),
        ..Default::default()
    };
    eframe::run_native(
        "HeXO",
        options,
        Box::new(|_cc| Ok(Box::new(App::default()))),
    )
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        controller::tick(self, ctx);

        egui::SidePanel::left("controls")
            .resizable(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                sidebar::draw(self, ui);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let response = render::paint_board(self, ui);
            render::paint_win_overlay(self, ui, response.rect);
            input::handle(self, ui, &response);
        });
    }
}
