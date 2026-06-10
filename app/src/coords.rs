//! Pointy-top axial hex coordinate math.
//!
//! Reference: https://www.redblobgames.com/grids/hexagons/#hex-to-pixel-axial

use eframe::egui;

pub fn axial_to_pixel(q: i32, r: i32, size: f32) -> egui::Vec2 {
    let sqrt3 = 3.0_f32.sqrt();
    let x = size * sqrt3 * (q as f32 + (r as f32) / 2.0);
    let y = size * 1.5 * (r as f32);
    egui::vec2(x, y)
}

pub fn pixel_to_axial(p: egui::Vec2, size: f32) -> (i32, i32) {
    let sqrt3 = 3.0_f32.sqrt();
    let qf = (sqrt3 / 3.0 * p.x - 1.0 / 3.0 * p.y) / size;
    let rf = (2.0 / 3.0 * p.y) / size;
    cube_round(qf, rf)
}

fn cube_round(qf: f32, rf: f32) -> (i32, i32) {
    let xf = qf;
    let zf = rf;
    let yf = -xf - zf;

    let mut x = xf.round();
    let mut y = yf.round();
    let z = zf.round();

    let dx = (x - xf).abs();
    let dy = (y - yf).abs();
    let dz = (z - zf).abs();

    if dx > dy && dx > dz {
        x = -y - z;
    } else if dy > dz {
        y = -x - z;
    }
    let _ = y; // keep variable live for the dy > dz branch correction
    (x as i32, z as i32)
}

pub fn hex_corners(center: egui::Pos2, size: f32) -> [egui::Pos2; 6] {
    let mut pts = [egui::Pos2::ZERO; 6];
    for (i, pt) in pts.iter_mut().enumerate() {
        let angle = std::f32::consts::PI / 180.0 * (60.0 * i as f32 - 30.0);
        *pt = egui::pos2(center.x + size * angle.cos(), center.y + size * angle.sin());
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_axial_pixel() {
        let size = 28.0;
        for q in -10..=10 {
            for r in -10..=10 {
                let p = axial_to_pixel(q, r, size);
                let back = pixel_to_axial(p, size);
                assert_eq!(back, (q, r), "round-trip failed at ({},{})", q, r);
            }
        }
    }

    #[test]
    fn cube_round_handles_near_boundaries() {
        let size = 28.0;
        let center = axial_to_pixel(0, 0, size);
        let nudged = center + egui::vec2(0.5, -0.5);
        assert_eq!(pixel_to_axial(nudged, size), (0, 0));
    }
}
