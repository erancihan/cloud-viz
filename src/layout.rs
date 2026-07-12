//! Deterministic nested layout with deliberately relaxed spacing: leaves are
//! fixed-size cards, containers pack their children onto shelves (rows)
//! aiming at a roughly square footprint, and grow to fit. Positions are
//! absolute world coordinates, ready for painting.
//!
//! No GUI types here — the module is unit-testable headlessly and shared by
//! the egui canvas and the SVG exporter.

use crate::model::{Topology, TopologyNode};
use std::collections::{HashMap, HashSet};

// Spacing constants. Generous on purpose: cards and containers get room to
// breathe so relationship edges route loosely instead of hugging the nodes.
pub const LEAF_W: f32 = 240.0;
pub const LEAF_H: f32 = 68.0;
pub const GAP: f32 = 36.0;
pub const PAD: f32 = 28.0;
pub const HEADER: f32 = 52.0;
pub const ROOT_GAP: f32 = 120.0;
const EMPTY_W: f32 = 280.0;
const EMPTY_H: f32 = 116.0;
/// Wider-than-tall packing bias — screens are landscape.
const ASPECT_BIAS: f32 = 2.1;

/// Axis-aligned rectangle in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    #[cfg(test)]
    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.right() <= self.right()
            && other.bottom() <= self.bottom()
    }
    #[cfg(test)]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect {
            x,
            y,
            w: self.right().max(other.right()) - x,
            h: self.bottom().max(other.bottom()) - y,
        }
    }
}

/// One laid-out node. The paint order is the vector order, which is
/// guaranteed parents-before-children.
#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub index: usize,
    pub rect: Rect,
    pub is_container: bool,
    pub child_count: usize,
}

pub struct Layout {
    /// Paint order: depth-first, parents before children.
    pub placed: Vec<PlacedNode>,
    /// node id -> index into `placed`.
    pub by_id: HashMap<String, usize>,
    /// Bounds of the whole diagram.
    pub bounds: Rect,
}

struct Measured {
    node_index: usize,
    w: f32,
    h: f32,
    /// Children with positions relative to this box's origin.
    children: Vec<(f32, f32, Measured)>,
}

pub fn layout_topology(topology: &Topology) -> Layout {
    let ids: HashSet<&str> = topology.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut children_of: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();

    for (i, node) in topology.nodes.iter().enumerate() {
        match node.parent_id.as_deref().filter(|p| ids.contains(p)) {
            Some(parent) => children_of.entry(parent).or_default().push(i),
            None => roots.push(i),
        }
    }

    let nodes = &topology.nodes;

    fn subtree_size(
        i: usize,
        nodes: &[TopologyNode],
        children_of: &HashMap<&str, Vec<usize>>,
    ) -> usize {
        1 + children_of
            .get(nodes[i].id.as_str())
            .map(|kids| {
                kids.iter()
                    .map(|&k| subtree_size(k, nodes, children_of))
                    .sum()
            })
            .unwrap_or(0)
    }

    // Stable child ordering: containers first (largest subtree first), then
    // leaves grouped by category in fixed slot order, then by name.
    let sort_children = |list: &mut Vec<usize>| {
        list.sort_by(|&a, &b| {
            let na = &nodes[a];
            let nb = &nodes[b];
            let ca = na.container || children_of.contains_key(na.id.as_str());
            let cb = nb.container || children_of.contains_key(nb.id.as_str());
            cb.cmp(&ca)
                .then_with(|| {
                    if ca && cb {
                        subtree_size(b, nodes, &children_of).cmp(&subtree_size(
                            a,
                            nodes,
                            &children_of,
                        ))
                    } else {
                        na.category.sort_rank().cmp(&nb.category.sort_rank())
                    }
                })
                .then_with(|| na.name.cmp(&nb.name))
        });
    };

    fn measure(
        i: usize,
        nodes: &[TopologyNode],
        children_of: &HashMap<&str, Vec<usize>>,
        sort_children: &impl Fn(&mut Vec<usize>),
    ) -> Measured {
        let node = &nodes[i];
        let kids = children_of.get(node.id.as_str());
        match kids {
            None => {
                let (w, h) = if node.container {
                    (EMPTY_W, EMPTY_H)
                } else {
                    (LEAF_W, LEAF_H)
                };
                Measured {
                    node_index: i,
                    w,
                    h,
                    children: Vec::new(),
                }
            }
            Some(kids) => {
                let mut order = kids.clone();
                sort_children(&mut order);
                let boxes: Vec<Measured> = order
                    .iter()
                    .map(|&k| measure(k, nodes, children_of, sort_children))
                    .collect();
                let (placed, w, h) = shelf_pack(boxes, GAP);
                Measured {
                    node_index: i,
                    w: w + PAD * 2.0,
                    h: h + HEADER + PAD,
                    children: placed
                        .into_iter()
                        .map(|(x, y, m)| (x + PAD, y + HEADER, m))
                        .collect(),
                }
            }
        }
    }

    let root_boxes: Vec<Measured> = roots
        .iter()
        .map(|&r| measure(r, nodes, &children_of, &sort_children))
        .collect();
    let (placed_roots, _, _) = shelf_pack(root_boxes, ROOT_GAP);

    // Flatten depth-first so parents always precede their children.
    let mut placed: Vec<PlacedNode> = Vec::with_capacity(nodes.len());
    let mut by_id: HashMap<String, usize> = HashMap::with_capacity(nodes.len());

    fn emit(
        x: f32,
        y: f32,
        m: &Measured,
        nodes: &[TopologyNode],
        children_of: &HashMap<&str, Vec<usize>>,
        placed: &mut Vec<PlacedNode>,
        by_id: &mut HashMap<String, usize>,
    ) {
        let node = &nodes[m.node_index];
        by_id.insert(node.id.clone(), placed.len());
        placed.push(PlacedNode {
            index: m.node_index,
            rect: Rect {
                x,
                y,
                w: m.w,
                h: m.h,
            },
            is_container: !m.children.is_empty() || node.container,
            child_count: children_of.get(node.id.as_str()).map_or(0, |k| k.len()),
        });
        for (cx, cy, child) in &m.children {
            emit(x + cx, y + cy, child, nodes, children_of, placed, by_id);
        }
    }

    for (x, y, root) in &placed_roots {
        emit(*x, *y, root, nodes, &children_of, &mut placed, &mut by_id);
    }

    let bounds = placed
        .iter()
        .map(|p| p.rect)
        .reduce(|a, b| a.union(&b))
        .unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });

    Layout {
        placed,
        by_id,
        bounds,
    }
}

/// Places boxes onto left-to-right shelves, wrapping at a target width chosen
/// to keep the overall footprint near-square (with a landscape bias).
/// Returns (placed boxes with positions, tight width, tight height).
fn shelf_pack(boxes: Vec<Measured>, gap: f32) -> (Vec<(f32, f32, Measured)>, f32, f32) {
    if boxes.is_empty() {
        return (Vec::new(), 0.0, 0.0);
    }
    let total_area: f32 = boxes.iter().map(|b| (b.w + gap) * (b.h + gap)).sum();
    let widest = boxes.iter().map(|b| b.w).fold(0.0f32, f32::max);
    let target_w = widest.max((total_area * ASPECT_BIAS).sqrt());

    let mut placed = Vec::with_capacity(boxes.len());
    let (mut x, mut y, mut row_h, mut max_right) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

    for b in boxes {
        if x > 0.0 && x + b.w > target_w {
            x = 0.0;
            y += row_h + gap;
            row_h = 0.0;
        }
        max_right = max_right.max(x + b.w);
        row_h = row_h.max(b.h);
        let bw = b.w;
        placed.push((x, y, b));
        x += bw + gap;
    }

    (placed, max_right, y + row_h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_topology;

    #[test]
    fn places_every_node_exactly_once() {
        let topo = demo_topology();
        let layout = layout_topology(&topo);
        assert_eq!(layout.placed.len(), topo.nodes.len());
        assert_eq!(layout.by_id.len(), topo.nodes.len());
    }

    #[test]
    fn parents_precede_children() {
        let topo = demo_topology();
        let layout = layout_topology(&topo);
        let mut seen: HashSet<&str> = HashSet::new();
        for p in &layout.placed {
            let node = &topo.nodes[p.index];
            if let Some(parent) = node.parent_id.as_deref() {
                assert!(
                    seen.contains(parent),
                    "{} painted before its parent",
                    node.id
                );
            }
            seen.insert(node.id.as_str());
        }
    }

    #[test]
    fn children_stay_inside_their_parent() {
        let topo = demo_topology();
        let layout = layout_topology(&topo);
        for p in &layout.placed {
            let node = &topo.nodes[p.index];
            if let Some(parent_id) = node.parent_id.as_deref() {
                let parent = &layout.placed[layout.by_id[parent_id]];
                assert!(
                    parent.rect.contains_rect(&p.rect),
                    "{} escapes parent {}",
                    node.id,
                    parent_id
                );
            }
        }
    }

    #[test]
    fn siblings_never_overlap() {
        let topo = demo_topology();
        let layout = layout_topology(&topo);
        let mut by_parent: HashMap<Option<&str>, Vec<&PlacedNode>> = HashMap::new();
        for p in &layout.placed {
            by_parent
                .entry(topo.nodes[p.index].parent_id.as_deref())
                .or_default()
                .push(p);
        }
        for group in by_parent.values() {
            for (i, a) in group.iter().enumerate() {
                for b in &group[i + 1..] {
                    assert!(
                        !a.rect.intersects(&b.rect),
                        "{} overlaps {}",
                        topo.nodes[a.index].id,
                        topo.nodes[b.index].id
                    );
                }
            }
        }
    }

    #[test]
    fn spacing_is_relaxed_between_siblings() {
        // Guard the "relaxed view" requirement: sibling leaves keep >= GAP
        // clearance so edges have room to route.
        let topo = demo_topology();
        let layout = layout_topology(&topo);
        let mut leaves_by_parent: HashMap<&str, Vec<&PlacedNode>> = HashMap::new();
        for p in &layout.placed {
            if p.is_container {
                continue;
            }
            if let Some(parent) = topo.nodes[p.index].parent_id.as_deref() {
                leaves_by_parent.entry(parent).or_default().push(p);
            }
        }
        for group in leaves_by_parent.values() {
            for (i, a) in group.iter().enumerate() {
                for b in &group[i + 1..] {
                    let dx = (b.rect.x - a.rect.right()).max(a.rect.x - b.rect.right());
                    let dy = (b.rect.y - a.rect.bottom()).max(a.rect.y - b.rect.bottom());
                    assert!(
                        dx >= GAP - 0.001 || dy >= GAP - 0.001,
                        "cards too close: {} / {}",
                        topo.nodes[a.index].id,
                        topo.nodes[b.index].id
                    );
                }
            }
        }
    }
}
