//! Edge routing geometry, shared by the egui canvas and the SVG exporter.
//!
//! Edges are relaxed cubic beziers: each endpoint anchors at the midpoint of
//! whichever rectangle side faces the other node, and the control points push
//! outward from that side, so curves swing wide of the cards instead of
//! hugging them.

use crate::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn dist(self, other: Vec2) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn anchor(self, rect: &Rect) -> Vec2 {
        match self {
            Side::Left => Vec2::new(rect.x, rect.y + rect.h / 2.0),
            Side::Right => Vec2::new(rect.right(), rect.y + rect.h / 2.0),
            Side::Top => Vec2::new(rect.x + rect.w / 2.0, rect.y),
            Side::Bottom => Vec2::new(rect.x + rect.w / 2.0, rect.bottom()),
        }
    }
    fn normal(self) -> Vec2 {
        match self {
            Side::Left => Vec2::new(-1.0, 0.0),
            Side::Right => Vec2::new(1.0, 0.0),
            Side::Top => Vec2::new(0.0, -1.0),
            Side::Bottom => Vec2::new(0.0, 1.0),
        }
    }
}

const SIDES: [Side; 4] = [Side::Left, Side::Right, Side::Top, Side::Bottom];

/// A routed edge: cubic bezier control polygon.
#[derive(Debug, Clone, Copy)]
pub struct EdgePath {
    pub p0: Vec2,
    pub c0: Vec2,
    pub c1: Vec2,
    pub p1: Vec2,
    /// True when this edge loops under two side-by-side cards; its label
    /// belongs at the deepest point (t = 0.5), not at a staggered position.
    pub loop_under: bool,
}

pub fn route_edge(source: &Rect, target: &Rect) -> EdgePath {
    // Pick the pair of side anchors with the shortest distance; ties resolve
    // in SIDES order, which keeps routing deterministic.
    let mut best = (f32::MAX, Side::Right, Side::Left);
    for s in SIDES {
        let pa = s.anchor(source);
        for t in SIDES {
            let d = pa.dist(t.anchor(target));
            if d < best.0 {
                best = (d, s, t);
            }
        }
    }
    let (dist, mut s_side, mut t_side) = best;

    // Side-by-side neighbors: a straight left/right connection leaves no room
    // for a label between the cards, so loop underneath instead — the curve
    // and its label land in the horizontal corridor below the pair. The dip
    // (~0.75 × reach) stays shallower than the inter-row gap so the loop
    // never reaches the next row of cards.
    let horizontal =
        matches!(s_side, Side::Left | Side::Right) && matches!(t_side, Side::Left | Side::Right);
    let loop_under = horizontal && dist < 80.0;
    if loop_under {
        s_side = Side::Bottom;
        t_side = Side::Bottom;
    }

    let p0 = s_side.anchor(source);
    let p1 = t_side.anchor(target);

    // Relaxed curvature: control points push well away from the node sides.
    let reach = if loop_under {
        28.0
    } else {
        (dist * 0.35).clamp(56.0, 280.0)
    };
    let c0 = Vec2::new(
        p0.x + s_side.normal().x * reach,
        p0.y + s_side.normal().y * reach,
    );
    let c1 = Vec2::new(
        p1.x + t_side.normal().x * reach,
        p1.y + t_side.normal().y * reach,
    );
    EdgePath {
        p0,
        c0,
        c1,
        p1,
        loop_under,
    }
}

/// Label position along an edge. Near-parallel edges (same target, similar
/// route) would stack their labels at the same midpoint; staggering by edge
/// index keeps them apart without any collision solver.
pub fn label_t(edge_index: usize) -> f32 {
    [0.5, 0.35, 0.65][edge_index % 3]
}

impl EdgePath {
    pub fn point_at(&self, t: f32) -> Vec2 {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        Vec2::new(
            a * self.p0.x + b * self.c0.x + c * self.c1.x + d * self.p1.x,
            a * self.p0.y + b * self.c0.y + c * self.c1.y + d * self.p1.y,
        )
    }

    /// Uniformly sampled polyline approximation (`segments + 1` points).
    pub fn flatten(&self, segments: usize) -> Vec<Vec2> {
        (0..=segments)
            .map(|i| self.point_at(i as f32 / segments as f32))
            .collect()
    }

    /// Arrowhead triangle at the target end, pointing along the curve.
    pub fn arrow_head(&self, size: f32) -> [Vec2; 3] {
        let tip = self.p1;
        let back = self.point_at(0.94);
        let (mut dx, mut dy) = (tip.x - back.x, tip.y - back.y);
        let len = (dx * dx + dy * dy).sqrt().max(0.0001);
        dx /= len;
        dy /= len;
        let (nx, ny) = (-dy, dx);
        let base_x = tip.x - dx * size;
        let base_y = tip.y - dy * size;
        let half = size * 0.45;
        [
            tip,
            Vec2::new(base_x + nx * half, base_y + ny * half),
            Vec2::new(base_x - nx * half, base_y - ny * half),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 50.0,
    };

    #[test]
    fn routes_to_facing_sides() {
        // Target directly to the right: leave from source's right, enter target's left.
        let b = Rect {
            x: 300.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        };
        let path = route_edge(&A, &b);
        assert_eq!(path.p0, Vec2::new(100.0, 25.0));
        assert_eq!(path.p1, Vec2::new(300.0, 25.0));

        // Target below: leave from bottom, enter top.
        let c = Rect {
            x: 0.0,
            y: 300.0,
            w: 100.0,
            h: 50.0,
        };
        let path = route_edge(&A, &c);
        assert_eq!(path.p0, Vec2::new(50.0, 50.0));
        assert_eq!(path.p1, Vec2::new(50.0, 300.0));
    }

    #[test]
    fn control_points_leave_room() {
        let b = Rect {
            x: 500.0,
            y: 400.0,
            w: 100.0,
            h: 50.0,
        };
        let path = route_edge(&A, &b);
        // Controls push outward from the chosen sides by at least the minimum reach.
        assert!((path.c0.x - path.p0.x).abs() + (path.c0.y - path.p0.y).abs() >= 56.0);
        assert!((path.c1.x - path.p1.x).abs() + (path.c1.y - path.p1.y).abs() >= 56.0);
    }

    #[test]
    fn adjacent_cards_loop_underneath() {
        // 36px apart side by side — no room for a label between them.
        let b = Rect {
            x: 136.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        };
        let path = route_edge(&A, &b);
        assert!(path.loop_under);
        assert_eq!(
            path.p0,
            Vec2::new(50.0, 50.0),
            "source anchor should be bottom"
        );
        assert_eq!(
            path.p1,
            Vec2::new(186.0, 50.0),
            "target anchor should be bottom"
        );
        // The curve dips below the cards but stays shallower than the 36px
        // inter-row gap, so it never touches the next row.
        let dip = path.point_at(0.5).y - 50.0;
        assert!(dip > 15.0 && dip < 30.0, "dip was {dip}");
    }

    #[test]
    fn flatten_spans_endpoints() {
        let b = Rect {
            x: 300.0,
            y: 200.0,
            w: 100.0,
            h: 50.0,
        };
        let path = route_edge(&A, &b);
        let pts = path.flatten(24);
        assert_eq!(pts.len(), 25);
        assert_eq!(pts[0], path.p0);
        assert_eq!(pts[24], path.p1);
    }
}
