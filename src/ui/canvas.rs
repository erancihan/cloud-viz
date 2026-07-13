//! The topology canvas: hand-painted nested containers, resource cards,
//! relaxed bezier edges, pan/zoom camera, hit-testing, and a minimap.

use crate::geom::{label_t, route_edge};
use crate::layout::Layout;
use crate::model::{EdgeKind, ResourceCategory, Topology};
use crate::theme::{mix, Theme};
use crate::ui::{c32, c32a, glyphs};
use eframe::egui::{
    Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
};

pub struct Camera {
    pub pan: Vec2,
    pub zoom: f32,
    pub needs_fit: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            needs_fit: true,
        }
    }
}

pub struct CanvasOutput {
    /// Some(index into topology.nodes) when a node was clicked; Some(None)
    /// is represented by `clicked_background`.
    pub clicked_node: Option<usize>,
    pub clicked_background: bool,
}

const MINIMAP_SIZE: Vec2 = Vec2::new(200.0, 140.0);
const MINIMAP_MARGIN: f32 = 14.0;

pub fn show(
    ui: &mut Ui,
    camera: &mut Camera,
    topology: &Topology,
    layout: &Layout,
    theme: &Theme,
    selected: Option<usize>,
) -> CanvasOutput {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 0.0, c32(theme.plane));
    draw_dot_grid(&painter, rect, camera, theme);

    if camera.needs_fit {
        fit(camera, rect, layout);
        camera.needs_fit = false;
    }

    let minimap_rect = Rect::from_min_size(
        Pos2::new(
            rect.right() - MINIMAP_SIZE.x - MINIMAP_MARGIN,
            rect.bottom() - MINIMAP_SIZE.y - MINIMAP_MARGIN,
        ),
        MINIMAP_SIZE,
    );
    let pointer_over_minimap = response
        .hover_pos()
        .is_some_and(|p| minimap_rect.contains(p));

    handle_camera_input(ui, camera, &response, rect, pointer_over_minimap);

    // Copy out the camera transform so the closures don't hold a borrow on
    // `camera` (the minimap interaction below mutates it for the next frame).
    let (pan, zoom) = (camera.pan, camera.zoom);
    let to_screen = move |x: f32, y: f32| -> Pos2 {
        Pos2::new(rect.min.x + pan.x + x * zoom, rect.min.y + pan.y + y * zoom)
    };
    let world_rect = |r: &crate::layout::Rect| -> Rect {
        Rect::from_min_max(to_screen(r.x, r.y), to_screen(r.right(), r.bottom()))
    };

    let hovered = if pointer_over_minimap {
        None
    } else {
        response
            .hover_pos()
            .and_then(|p| hit_test(p, layout, &world_rect))
    };
    if hovered.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    // Paint order: containers (parents already precede children), then edges,
    // then leaf cards, so relaxed curves never cover node content.
    for placed in &layout.placed {
        if placed.is_container {
            draw_container(
                &painter,
                topology,
                placed,
                &world_rect(&placed.rect),
                theme,
                camera.zoom,
                selected == Some(placed.index),
            );
        }
    }

    for (edge_index, edge) in topology.edges.iter().enumerate() {
        let (Some(&si), Some(&ti)) = (
            layout.by_id.get(&edge.source),
            layout.by_id.get(&edge.target),
        ) else {
            continue;
        };
        draw_edge(
            &painter,
            edge.kind,
            edge.label.as_deref(),
            edge_index,
            &layout.placed[si].rect,
            &layout.placed[ti].rect,
            theme,
            camera,
            &to_screen,
        );
    }

    for placed in &layout.placed {
        if !placed.is_container {
            let is_selected = selected == Some(placed.index);
            let is_hovered = hovered
                .map(|h| std::ptr::eq(&layout.placed[h], placed))
                .unwrap_or(false);
            draw_card(
                &painter,
                topology,
                placed,
                &world_rect(&placed.rect),
                theme,
                camera.zoom,
                is_selected,
                is_hovered,
            );
        }
    }

    draw_minimap(
        &painter,
        minimap_rect,
        rect,
        layout,
        topology,
        theme,
        camera,
    );
    if pointer_over_minimap && (response.dragged() || response.clicked()) {
        if let Some(pos) = response.interact_pointer_pos() {
            center_on_minimap_point(camera, pos, minimap_rect, rect, layout);
        }
    }

    let clicked_node = if response.clicked() && !pointer_over_minimap {
        response
            .interact_pointer_pos()
            .and_then(|p| hit_test(p, layout, &world_rect))
    } else {
        None
    };
    let clicked_background = response.clicked() && !pointer_over_minimap && clicked_node.is_none();

    CanvasOutput {
        clicked_node: clicked_node.map(|i| layout.placed[i].index),
        clicked_background,
    }
}

fn handle_camera_input(
    ui: &Ui,
    camera: &mut Camera,
    response: &Response,
    rect: Rect,
    over_minimap: bool,
) {
    if response.dragged() && !over_minimap {
        camera.pan += response.drag_delta();
    }
    if let Some(hover) = response.hover_pos() {
        if !over_minimap {
            let (scroll, zoom_gesture) = ui.input(|i| (i.raw_scroll_delta.y, i.zoom_delta()));
            let mut factor = zoom_gesture;
            if scroll.abs() > 0.0 {
                factor *= (scroll * 0.0015).exp();
            }
            if (factor - 1.0).abs() > f32::EPSILON {
                let new_zoom = (camera.zoom * factor).clamp(0.05, 2.5);
                let world = (hover - rect.min - camera.pan) / camera.zoom;
                camera.pan = hover - rect.min - world * new_zoom;
                camera.zoom = new_zoom;
            }
        }
    }
}

pub fn fit(camera: &mut Camera, rect: Rect, layout: &Layout) {
    let b = layout.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return;
    }
    let margin = 40.0;
    let zoom = ((rect.width() - margin) / b.w)
        .min((rect.height() - margin) / b.h)
        .clamp(0.05, 1.0);
    camera.zoom = zoom;
    camera.pan = Vec2::new(
        (rect.width() - b.w * zoom) / 2.0 - b.x * zoom,
        (rect.height() - b.h * zoom) / 2.0 - b.y * zoom,
    );
}

fn hit_test(
    pos: Pos2,
    layout: &Layout,
    world_rect: &impl Fn(&crate::layout::Rect) -> Rect,
) -> Option<usize> {
    // Children are painted after parents, so scan back-to-front.
    layout
        .placed
        .iter()
        .enumerate()
        .rev()
        .find(|(_, p)| world_rect(&p.rect).contains(pos))
        .map(|(i, _)| i)
}

fn draw_dot_grid(painter: &Painter, rect: Rect, camera: &Camera, theme: &Theme) {
    let spacing = 30.0 * camera.zoom.max(0.4);
    let color = c32(theme.hairline);
    let mut x = rect.min.x + camera.pan.x.rem_euclid(spacing);
    while x < rect.max.x {
        let mut y = rect.min.y + camera.pan.y.rem_euclid(spacing);
        while y < rect.max.y {
            painter.circle_filled(Pos2::new(x, y), 1.0, color);
            y += spacing;
        }
        x += spacing;
    }
}

fn draw_container(
    painter: &Painter,
    topology: &Topology,
    placed: &crate::layout::PlacedNode,
    rect: &Rect,
    theme: &Theme,
    zoom: f32,
    selected: bool,
) {
    let node = &topology.nodes[placed.index];
    let cat = theme.category_color(node.category);
    let is_scope = node.category == ResourceCategory::Scope;
    let rounding = 12.0 * zoom;

    let fill = if is_scope {
        theme.surface
    } else {
        mix(cat, theme.surface, 0.05)
    };
    painter.rect_filled(*rect, rounding, c32(fill));

    let stroke = Stroke::new(
        1.5,
        if is_scope {
            c32(theme.hairline)
        } else {
            c32a(cat, 128)
        },
    );
    if is_scope {
        painter.rect_stroke(*rect, rounding, stroke, StrokeKind::Inside);
    } else {
        dashed_rect(painter, rect, stroke, 7.0, 6.0);
    }
    if selected {
        painter.rect_stroke(
            rect.expand(2.0),
            rounding,
            Stroke::new(2.0, c32(theme.accent)),
            StrokeKind::Outside,
        );
    }

    // Header: KIND · name · count. Skip text when zoomed out too far to read.
    if rect.height() * (1.0 / zoom.max(0.0001)) > 0.0 && 11.0 * zoom >= 5.0 {
        let kind_font = FontId::proportional((10.5 * zoom).max(5.0));
        let name_font = FontId::proportional((13.5 * zoom).max(6.0));
        let pad = 18.0 * zoom;
        let baseline = rect.min + Vec2::new(pad, 14.0 * zoom);
        let kind_rect = painter.text(
            baseline,
            Align2::LEFT_TOP,
            node.kind_label.to_uppercase(),
            kind_font,
            c32(cat),
        );
        painter.text(
            Pos2::new(kind_rect.right() + 10.0 * zoom, baseline.y - 1.5 * zoom),
            Align2::LEFT_TOP,
            &node.name,
            name_font,
            c32(theme.ink),
        );
        if placed.child_count > 0 {
            painter.text(
                Pos2::new(rect.right() - pad, baseline.y),
                Align2::RIGHT_TOP,
                placed.child_count.to_string(),
                FontId::proportional((11.0 * zoom).max(5.0)),
                c32(theme.ink_3),
            );
        } else {
            painter.text(
                rect.min + Vec2::new(pad, 34.0 * zoom),
                Align2::LEFT_TOP,
                "No resources",
                FontId::proportional((11.5 * zoom).max(5.0)),
                c32(theme.ink_3),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_card(
    painter: &Painter,
    topology: &Topology,
    placed: &crate::layout::PlacedNode,
    rect: &Rect,
    theme: &Theme,
    zoom: f32,
    selected: bool,
    hovered: bool,
) {
    let node = &topology.nodes[placed.index];
    let cat = theme.category_color(node.category);
    let rounding = 10.0 * zoom;

    painter.rect_filled(*rect, rounding, c32(theme.card));
    let border = if hovered {
        c32(theme.ink_3)
    } else {
        c32(theme.hairline)
    };
    painter.rect_stroke(
        *rect,
        rounding,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    if selected {
        painter.rect_stroke(
            rect.expand(2.0),
            rounding,
            Stroke::new(2.0, c32(theme.accent)),
            StrokeKind::Outside,
        );
    }

    // Category accent bar on the left edge.
    let bar = Rect::from_min_size(
        rect.min + Vec2::new(1.5 * zoom, 6.0 * zoom),
        Vec2::new(3.5 * zoom, rect.height() - 12.0 * zoom),
    );
    painter.rect_filled(bar, 2.0 * zoom, c32(cat));

    // Below this size text/icons are unreadable smudges — cards read via the
    // accent color plus the minimap instead.
    if rect.height() < 18.0 {
        return;
    }

    // The head (chip + text lines) occupies the top LEAF_H of the card;
    // attachment rows, when present, live below it.
    let head_h = crate::layout::LEAF_H * zoom;
    let chip_size = 32.0 * zoom;
    let chip = Rect::from_min_size(
        rect.min + Vec2::new(14.0 * zoom, (head_h - chip_size) / 2.0),
        Vec2::splat(chip_size),
    );
    painter.rect_filled(chip, 8.0 * zoom, c32a(cat, 36));
    glyphs::draw(
        painter,
        chip.shrink(chip_size * 0.25),
        node.category,
        c32(cat),
    );

    let tx = chip.right() + 12.0 * zoom;
    let max_w = rect.right() - 10.0 * zoom - tx;
    let name_font = FontId::proportional((12.5 * zoom).max(6.0));
    let sub_font = FontId::proportional((10.5 * zoom).max(5.0));
    let cy = rect.min.y + head_h / 2.0;
    if let Some(group) = &node.group {
        let line = 13.0 * zoom;
        draw_truncated(
            painter,
            Pos2::new(tx, cy - line),
            Align2::LEFT_CENTER,
            &node.name,
            name_font,
            c32(theme.ink),
            max_w,
        );
        draw_truncated(
            painter,
            Pos2::new(tx, cy),
            Align2::LEFT_CENTER,
            &node.kind_label,
            sub_font.clone(),
            c32(theme.ink_3),
            max_w,
        );
        draw_truncated(
            painter,
            Pos2::new(tx, cy + line),
            Align2::LEFT_CENTER,
            group,
            sub_font.clone(),
            c32a(theme.ink_3, 200),
            max_w,
        );
    } else {
        draw_truncated(
            painter,
            Pos2::new(tx, cy - 2.0 * zoom),
            Align2::LEFT_BOTTOM,
            &node.name,
            name_font,
            c32(theme.ink),
            max_w,
        );
        draw_truncated(
            painter,
            Pos2::new(tx, cy + 2.0 * zoom),
            Align2::LEFT_TOP,
            &node.kind_label,
            sub_font.clone(),
            c32(theme.ink_3),
            max_w,
        );
    }

    // Attachment rows: a mini icon + name per folded-in subsidiary. The
    // hardware rows (disks, NICs, extensions, slots) come first; secondary
    // items (SSH keys, public IPs) sit below their own separator, with a
    // link icon on the right when shared across nodes.
    if !node.attachments.is_empty() {
        use crate::layout::{secondary_split_index, ATTACH_PAD, ATTACH_ROW, ATTACH_SPLIT, LEAF_H};
        let divider_y = rect.min.y + LEAF_H * zoom;
        painter.line_segment(
            [
                Pos2::new(rect.min.x + 12.0 * zoom, divider_y),
                Pos2::new(rect.max.x - 12.0 * zoom, divider_y),
            ],
            Stroke::new(1.0, c32(theme.hairline)),
        );
        let split = secondary_split_index(&node.attachments);
        for (i, att) in node.attachments.iter().enumerate() {
            let mut offset = LEAF_H + ATTACH_PAD + (i as f32 + 0.5) * ATTACH_ROW;
            if let Some(s) = split {
                if i >= s {
                    offset += ATTACH_SPLIT;
                }
                if i == s {
                    let sep_y = rect.min.y
                        + (LEAF_H + ATTACH_PAD + s as f32 * ATTACH_ROW + ATTACH_SPLIT / 2.0) * zoom;
                    painter.line_segment(
                        [
                            Pos2::new(rect.min.x + 20.0 * zoom, sep_y),
                            Pos2::new(rect.max.x - 20.0 * zoom, sep_y),
                        ],
                        Stroke::new(1.0, c32(theme.hairline)),
                    );
                }
            }
            let cy = rect.min.y + offset * zoom;
            let icon = Rect::from_center_size(
                Pos2::new(rect.min.x + 27.0 * zoom, cy),
                Vec2::splat(11.0 * zoom),
            );
            // Secondary glyphs carry their category color (public IP =
            // network green, SSH key = security red); hardware stays muted.
            let icon_color = match att.kind.as_str() {
                "public ip" => c32(theme.category_color(ResourceCategory::Network)),
                "ssh key" => c32(theme.category_color(ResourceCategory::Security)),
                _ => c32(theme.ink_3),
            };
            glyphs::draw_attachment(painter, icon, &att.kind, icon_color);
            let link_room = if att.shared { 26.0 * zoom } else { 10.0 * zoom };
            draw_truncated(
                painter,
                Pos2::new(rect.min.x + 38.0 * zoom, cy),
                Align2::LEFT_CENTER,
                &att.name,
                sub_font.clone(),
                c32(theme.ink_2),
                rect.max.x - link_room - (rect.min.x + 38.0 * zoom),
            );
            if att.shared {
                let badge = Rect::from_center_size(
                    Pos2::new(rect.max.x - 16.0 * zoom, cy),
                    Vec2::splat(10.0 * zoom),
                );
                glyphs::draw_link(painter, badge, c32(theme.accent));
            }
        }
    }
}

fn draw_truncated(
    painter: &Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) {
    let mut shown = text.to_string();
    let galley = painter.layout_no_wrap(shown.clone(), font.clone(), color);
    if galley.rect.width() > max_width {
        let keep = ((max_width / galley.rect.width()) * text.chars().count() as f32) as usize;
        shown = text
            .chars()
            .take(keep.saturating_sub(1).max(1))
            .collect::<String>()
            + "…";
    }
    painter.text(pos, anchor, shown, font, color);
}

#[allow(clippy::too_many_arguments)]
fn draw_edge(
    painter: &Painter,
    kind: EdgeKind,
    label: Option<&str>,
    edge_index: usize,
    source: &crate::layout::Rect,
    target: &crate::layout::Rect,
    theme: &Theme,
    camera: &Camera,
    to_screen: &impl Fn(f32, f32) -> Pos2,
) {
    let path = route_edge(source, target);
    let stroke = Stroke::new(1.6 * camera.zoom.max(0.5), c32(theme.edge));
    let points: Vec<Pos2> = path
        .flatten(36)
        .iter()
        .map(|v| to_screen(v.x, v.y))
        .collect();

    if kind == EdgeKind::Network {
        painter.extend(Shape::dashed_line(&points, stroke, 7.0, 5.0));
    } else {
        painter.add(Shape::line(points, stroke));
    }

    let head = path.arrow_head(11.0);
    painter.add(Shape::convex_polygon(
        head.iter().map(|v| to_screen(v.x, v.y)).collect(),
        c32(theme.edge),
        Stroke::NONE,
    ));

    // Labels earn their pixels only when readable.
    if let Some(label) = label {
        if camera.zoom >= 0.45 {
            let t = if path.loop_under {
                0.5
            } else {
                label_t(edge_index)
            };
            let mid = path.point_at(t);
            let pos = to_screen(mid.x, mid.y);
            let font = FontId::proportional((10.0 * camera.zoom).max(6.0));
            let galley = painter.layout_no_wrap(label.to_string(), font.clone(), c32(theme.ink_3));
            let bg = Rect::from_center_size(pos, galley.rect.size() + Vec2::new(10.0, 5.0));
            painter.rect_filled(bg, 4.0, c32a(theme.surface, 235));
            painter.text(pos, Align2::CENTER_CENTER, label, font, c32(theme.ink_3));
        }
    }
}

fn dashed_rect(painter: &Painter, rect: &Rect, stroke: Stroke, dash: f32, gap: f32) {
    let corners = [
        rect.min,
        Pos2::new(rect.max.x, rect.min.y),
        rect.max,
        Pos2::new(rect.min.x, rect.max.y),
        rect.min,
    ];
    for pair in corners.windows(2) {
        painter.extend(Shape::dashed_line(pair, stroke, dash, gap));
    }
}

fn draw_minimap(
    painter: &Painter,
    map: Rect,
    canvas: Rect,
    layout: &Layout,
    topology: &Topology,
    theme: &Theme,
    camera: &Camera,
) {
    painter.rect_filled(map, 8.0, c32(theme.surface));
    painter.rect_stroke(
        map,
        8.0,
        Stroke::new(1.0, c32(theme.hairline)),
        StrokeKind::Inside,
    );

    let b = layout.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return;
    }
    let inner = map.shrink(8.0);
    let scale = (inner.width() / b.w).min(inner.height() / b.h);
    let offset = inner.min.to_vec2()
        + Vec2::new(
            (inner.width() - b.w * scale) / 2.0 - b.x * scale,
            (inner.height() - b.h * scale) / 2.0 - b.y * scale,
        );
    let project = |r: &crate::layout::Rect| {
        Rect::from_min_size(
            Pos2::new(r.x * scale, r.y * scale) + offset,
            Vec2::new(r.w * scale, r.h * scale),
        )
    };

    for placed in &layout.placed {
        let node = &topology.nodes[placed.index];
        let r = project(&placed.rect);
        if placed.is_container {
            painter.rect_stroke(
                r,
                2.0,
                Stroke::new(0.6, c32a(theme.ink_3, 90)),
                StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(r, 1.0, c32(theme.category_color(node.category)));
        }
    }

    // Current viewport in world coords -> minimap coords.
    let view = crate::layout::Rect {
        x: -camera.pan.x / camera.zoom,
        y: -camera.pan.y / camera.zoom,
        w: canvas.width() / camera.zoom,
        h: canvas.height() / camera.zoom,
    };
    let view_r = project(&view).intersect(map.shrink(2.0));
    if view_r.is_positive() {
        painter.rect_stroke(
            view_r,
            2.0,
            Stroke::new(1.2, c32(theme.accent)),
            StrokeKind::Inside,
        );
    }
}

fn center_on_minimap_point(
    camera: &mut Camera,
    pos: Pos2,
    map: Rect,
    canvas: Rect,
    layout: &Layout,
) {
    let b = layout.bounds;
    if b.w <= 0.0 || b.h <= 0.0 {
        return;
    }
    let inner = map.shrink(8.0);
    let scale = (inner.width() / b.w).min(inner.height() / b.h);
    let offset = inner.min.to_vec2()
        + Vec2::new(
            (inner.width() - b.w * scale) / 2.0 - b.x * scale,
            (inner.height() - b.h * scale) / 2.0 - b.y * scale,
        );
    let world = Pos2::new((pos.x - offset.x) / scale, (pos.y - offset.y) / scale);
    camera.pan = Vec2::new(
        canvas.width() / 2.0 - world.x * camera.zoom,
        canvas.height() / 2.0 - world.y * camera.zoom,
    );
}
