//! Category glyphs painted with egui primitives. Each glyph is designed on a
//! 16x16 grid and scaled into the given rect. Icons + text labels are the
//! secondary encoding that keeps node identity from being color-alone.

use crate::model::ResourceCategory;
use eframe::egui::{epaint::EllipseShape, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

pub fn draw(painter: &Painter, rect: Rect, category: ResourceCategory, color: Color32) {
    let s = rect.width() / 16.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let stroke = Stroke::new((1.5 * s).max(1.0), color);
    let line =
        |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| {
        Shape::closed_line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke)
    };
    let dot = |x: f32, y: f32, r: f32| Shape::circle_filled(p(x, y), r * s, color);

    use ResourceCategory::*;
    match category {
        Compute => {
            painter.add(closed(&[
                (4.5, 4.5),
                (11.5, 4.5),
                (11.5, 11.5),
                (4.5, 11.5),
            ]));
            painter.add(Shape::rect_filled(
                Rect::from_min_max(p(7.0, 7.0), p(9.0, 9.0)),
                0.0,
                color,
            ));
            for (a, b) in [
                ((6.0, 4.5), (6.0, 2.5)),
                ((10.0, 4.5), (10.0, 2.5)),
                ((6.0, 11.5), (6.0, 13.5)),
                ((10.0, 11.5), (10.0, 13.5)),
                ((4.5, 6.0), (2.5, 6.0)),
                ((4.5, 10.0), (2.5, 10.0)),
                ((11.5, 6.0), (13.5, 6.0)),
                ((11.5, 10.0), (13.5, 10.0)),
            ] {
                painter.add(line(&[a, b]));
            }
        }
        Network => {
            for (x, y) in [(8.0, 3.5), (3.5, 12.0), (12.5, 12.0)] {
                painter.add(Shape::circle_stroke(p(x, y), 1.8 * s, stroke));
            }
            painter.add(line(&[(7.0, 5.1), (4.4, 10.4)]));
            painter.add(line(&[(9.0, 5.1), (11.6, 10.4)]));
            painter.add(line(&[(5.3, 12.0), (10.7, 12.0)]));
        }
        Storage => {
            painter.add(closed(&[
                (2.5, 4.0),
                (13.5, 4.0),
                (13.5, 12.0),
                (2.5, 12.0),
            ]));
            painter.add(line(&[(2.5, 8.5), (13.5, 8.5)]));
            painter.add(dot(5.0, 10.3, 0.8));
        }
        Database => {
            painter.add(Shape::Ellipse(EllipseShape {
                center: p(8.0, 4.0),
                radius: Vec2::new(4.8 * s, 1.9 * s),
                fill: Color32::TRANSPARENT,
                stroke,
            }));
            painter.add(line(&[(3.2, 4.0), (3.2, 12.0)]));
            painter.add(line(&[(12.8, 4.0), (12.8, 12.0)]));
            painter.add(arc(
                p(8.0, 12.0),
                4.8 * s,
                1.9 * s,
                0.0,
                std::f32::consts::PI,
                stroke,
            ));
            painter.add(arc(
                p(8.0, 8.0),
                4.8 * s,
                1.9 * s,
                0.0,
                std::f32::consts::PI,
                stroke,
            ));
        }
        Containers => {
            for (x, y) in [(2.8, 2.8), (8.6, 2.8), (2.8, 8.6), (8.6, 8.6)] {
                painter.add(closed(&[
                    (x, y),
                    (x + 4.6, y),
                    (x + 4.6, y + 4.6),
                    (x, y + 4.6),
                ]));
            }
        }
        Security => {
            painter.add(closed(&[
                (8.0, 2.3),
                (12.6, 4.0),
                (12.6, 8.0),
                (11.9, 10.7),
                (8.0, 13.5),
                (4.1, 10.7),
                (3.4, 8.0),
                (3.4, 4.0),
            ]));
            painter.add(line(&[(6.2, 8.0), (7.5, 9.3), (9.9, 6.7)]));
        }
        Integration => {
            painter.add(arc(
                p(8.0, 8.0),
                5.0 * s,
                5.0 * s,
                std::f32::consts::PI,
                0.15,
                stroke,
            ));
            painter.add(arc(
                p(8.0, 8.0),
                5.0 * s,
                5.0 * s,
                0.0,
                std::f32::consts::PI + 0.15,
                stroke,
            ));
            painter.add(line(&[(13.0, 1.6), (13.0, 4.0), (10.6, 4.0)]));
            painter.add(line(&[(3.0, 14.4), (3.0, 12.0), (5.4, 12.0)]));
        }
        Web => {
            painter.add(Shape::circle_stroke(p(8.0, 8.0), 5.4 * s, stroke));
            painter.add(Shape::Ellipse(EllipseShape {
                center: p(8.0, 8.0),
                radius: Vec2::new(2.4 * s, 5.4 * s),
                fill: Color32::TRANSPARENT,
                stroke,
            }));
            painter.add(line(&[(2.6, 8.0), (13.4, 8.0)]));
        }
        Other => {
            painter.add(dot(3.5, 8.0, 1.0));
            painter.add(dot(8.0, 8.0, 1.0));
            painter.add(dot(12.5, 8.0, 1.0));
        }
        Scope => {
            painter.add(closed(&[
                (4.6, 12.5),
                (3.0, 11.9),
                (2.2, 10.4),
                (2.5, 8.8),
                (3.7, 7.8),
                (4.5, 7.6),
                (4.9, 6.3),
                (6.2, 5.0),
                (8.1, 4.6),
                (9.9, 5.1),
                (11.2, 6.3),
                (11.7, 7.5),
                (12.8, 7.9),
                (13.5, 9.2),
                (13.3, 10.9),
                (12.3, 12.1),
                (11.0, 12.5),
            ]));
        }
        Group => {
            painter.add(closed(&[
                (2.5, 4.5),
                (6.5, 4.5),
                (7.9, 6.1),
                (13.5, 6.1),
                (13.5, 12.5),
                (2.5, 12.5),
            ]));
        }
    }
}

/// Mini glyphs for attachment rows on a card (disk / nic / extension / slot),
/// same 16x16 grid as the category glyphs. Mirrored by
/// `export.rs::attachment_glyph_svg` — keep the two in sync.
pub fn draw_attachment(painter: &Painter, rect: Rect, kind: &str, color: Color32) {
    let s = rect.width() / 16.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let stroke = Stroke::new((1.5 * s).max(1.0), color);
    let line =
        |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| {
        Shape::closed_line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke)
    };

    match kind {
        // Drive: flat box with a status dot.
        "disk" => {
            painter.add(closed(&[
                (2.5, 4.5),
                (13.5, 4.5),
                (13.5, 11.5),
                (2.5, 11.5),
            ]));
            painter.add(line(&[(2.5, 8.7), (13.5, 8.7)]));
            painter.add(Shape::circle_filled(p(11.4, 10.1), 0.8 * s, color));
        }
        // Reuse the network triangle for NICs.
        "nic" => draw(painter, rect, ResourceCategory::Network, color),
        // Puzzle piece with a top tab.
        "extension" => {
            painter.add(closed(&[
                (2.8, 7.0),
                (6.2, 7.0),
                (6.2, 5.4),
                (7.1, 4.4),
                (8.9, 4.4),
                (9.8, 5.4),
                (9.8, 7.0),
                (13.2, 7.0),
                (13.2, 13.2),
                (2.8, 13.2),
            ]));
        }
        // Globe for public IPs (internet reachability).
        "public ip" => {
            painter.add(Shape::circle_stroke(p(8.0, 8.0), 5.4 * s, stroke));
            painter.add(Shape::Ellipse(EllipseShape {
                center: p(8.0, 8.0),
                radius: Vec2::new(2.4 * s, 5.4 * s),
                fill: Color32::TRANSPARENT,
                stroke,
            }));
            painter.add(line(&[(2.6, 8.0), (13.4, 8.0)]));
        }
        // Clock for restore points (point-in-time backups).
        "restore point" => {
            painter.add(Shape::circle_stroke(p(8.0, 8.0), 5.5 * s, stroke));
            painter.add(line(&[(8.0, 8.0), (8.0, 4.4)]));
            painter.add(line(&[(8.0, 8.0), (10.8, 9.4)]));
        }
        // Key: ring head, shaft, two teeth.
        "ssh key" => {
            painter.add(Shape::circle_stroke(p(4.8, 8.0), 2.4 * s, stroke));
            painter.add(line(&[(7.2, 8.0), (13.4, 8.0)]));
            painter.add(line(&[(10.8, 8.0), (10.8, 10.6)]));
            painter.add(line(&[(13.4, 8.0), (13.4, 10.6)]));
        }
        // Camera for snapshots (a picture of a disk at a moment).
        "snapshot" => {
            painter.add(closed(&[
                (2.5, 5.0),
                (13.5, 5.0),
                (13.5, 13.0),
                (2.5, 13.0),
            ]));
            painter.add(Shape::circle_stroke(p(8.0, 9.0), 2.4 * s, stroke));
            painter.add(line(&[(6.0, 5.0), (7.0, 3.4), (9.0, 3.4), (10.0, 5.0)]));
        }
        // A dot plugged into a service ring for private endpoints.
        "private endpoint" => {
            painter.add(Shape::circle_filled(p(3.5, 8.0), 1.6 * s, color));
            painter.add(line(&[(5.1, 8.0), (9.3, 8.0)]));
            painter.add(Shape::circle_stroke(p(11.6, 8.0), 2.3 * s, stroke));
        }
        // Shield for network security groups.
        "nsg" => {
            painter.add(closed(&[
                (8.0, 2.6),
                (13.0, 4.6),
                (12.4, 9.4),
                (8.0, 13.4),
                (3.6, 9.4),
                (3.0, 4.6),
            ]));
        }
        // Two stacked layers for deployment slots.
        "slot" => {
            painter.add(closed(&[
                (3.0, 3.0),
                (10.4, 3.0),
                (10.4, 10.4),
                (3.0, 10.4),
            ]));
            painter.add(closed(&[
                (5.6, 5.6),
                (13.0, 5.6),
                (13.0, 13.0),
                (5.6, 13.0),
            ]));
        }
        _ => {
            for x in [4.0, 8.0, 12.0] {
                painter.add(Shape::circle_filled(p(x, 8.0), 1.0 * s, color));
            }
        }
    }
}

/// Link badge for shared attachments (an SSH key used by several VMs): two
/// rings joined by a bar. Mirrored by `export.rs::link_glyph_svg`.
pub fn draw_link(painter: &Painter, rect: Rect, color: Color32) {
    let s = rect.width() / 16.0;
    let o = rect.min;
    let p = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let stroke = Stroke::new((1.6 * s).max(1.0), color);
    painter.add(Shape::circle_stroke(p(5.0, 11.0), 2.6 * s, stroke));
    painter.add(Shape::circle_stroke(p(11.0, 5.0), 2.6 * s, stroke));
    painter.add(Shape::line(vec![p(6.8, 9.2), p(9.2, 6.8)], stroke));
}

/// Elliptical arc approximated by a polyline (egui has no arc primitive).
fn arc(center: Pos2, rx: f32, ry: f32, from: f32, to: f32, stroke: Stroke) -> Shape {
    const STEPS: usize = 20;
    let pts = (0..=STEPS)
        .map(|i| {
            let t = from + (to - from) * (i as f32 / STEPS as f32);
            Pos2::new(center.x + rx * t.cos(), center.y + ry * t.sin())
        })
        .collect();
    Shape::line(pts, stroke)
}
