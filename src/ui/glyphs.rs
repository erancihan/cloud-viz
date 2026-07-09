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
    let line = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(pts.iter().map(|&(x, y)| p(x, y)).collect(), stroke);
    let dot = |x: f32, y: f32, r: f32| Shape::circle_filled(p(x, y), r * s, color);

    use ResourceCategory::*;
    match category {
        Compute => {
            painter.add(closed(&[(4.5, 4.5), (11.5, 4.5), (11.5, 11.5), (4.5, 11.5)]));
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
            painter.add(closed(&[(2.5, 4.0), (13.5, 4.0), (13.5, 12.0), (2.5, 12.0)]));
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
            painter.add(arc(p(8.0, 12.0), 4.8 * s, 1.9 * s, 0.0, std::f32::consts::PI, stroke));
            painter.add(arc(p(8.0, 8.0), 4.8 * s, 1.9 * s, 0.0, std::f32::consts::PI, stroke));
        }
        Containers => {
            for (x, y) in [(2.8, 2.8), (8.6, 2.8), (2.8, 8.6), (8.6, 8.6)] {
                painter.add(closed(&[(x, y), (x + 4.6, y), (x + 4.6, y + 4.6), (x, y + 4.6)]));
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
            painter.add(arc(p(8.0, 8.0), 5.0 * s, 5.0 * s, std::f32::consts::PI, 0.15, stroke));
            painter.add(arc(p(8.0, 8.0), 5.0 * s, 5.0 * s, 0.0, std::f32::consts::PI + 0.15, stroke));
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
