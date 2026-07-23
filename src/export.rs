//! SVG export: renders a topology + layout to a standalone SVG document.
//! Shares layout and edge-routing code with the GUI canvas, so the exported
//! picture matches what the app shows. Also handy for docs and diffing
//! infrastructure snapshots.

use crate::geom::{label_t, route_edge};
use crate::layout::{
    layout_topology, secondary_split_index, Rect, ATTACH_PAD, ATTACH_ROW, ATTACH_SPLIT, LEAF_H,
};
use crate::model::{format_cost, subtree_costs, EdgeKind, ResourceCategory, Topology};
use crate::theme::{mix, Rgb, Theme};
use std::fmt::Write;

const MARGIN: f32 = 48.0;

pub fn to_svg(topology: &Topology, theme: &Theme) -> String {
    let layout = layout_topology(topology);
    let b = layout.bounds;
    let (w, h) = (b.w + MARGIN * 2.0, b.h + MARGIN * 2.0);
    let (ox, oy) = (MARGIN - b.x, MARGIN - b.y);

    let mut svg = String::with_capacity(64 * 1024);
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif">"#
    );
    let _ = write!(
        svg,
        r#"<rect width="{w:.0}" height="{h:.0}" fill="{}"/>"#,
        theme.plane.hex()
    );

    // Containers first, then edges, then cards on top so loose curves never
    // obscure node content.
    let subtree = subtree_costs(topology);
    for placed in &layout.placed {
        if !placed.is_container {
            continue;
        }
        let node = &topology.nodes[placed.index];
        let r = shift(placed.rect, ox, oy);
        let cat = theme.category_color(node.category);
        let is_scope = node.category == ResourceCategory::Scope;
        let fill = if is_scope {
            theme.surface
        } else {
            mix(cat, theme.surface, 0.05)
        };
        let stroke = if is_scope { theme.hairline } else { cat };
        let dash = if is_scope {
            ""
        } else {
            r#" stroke-dasharray="7 6""#
        };
        let stroke_opacity = if is_scope { 1.0 } else { 0.5 };
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="14" fill="{}" stroke="{}" stroke-opacity="{stroke_opacity}" stroke-width="1.5"{dash}/>"#,
            r.x,
            r.y,
            r.w,
            r.h,
            fill.hex(),
            stroke.hex()
        );
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="{:.1}" font-size="10.5" font-weight="700" letter-spacing="0.6" fill="{}">{}</text>"#,
            r.x + 18.0,
            r.y + 24.0,
            cat.hex(),
            escape(&node.kind_label.to_uppercase())
        );
        // ~8.4px per uppercase character at 10.5px bold with letter-spacing.
        let _ = write!(
            svg,
            r#"<text x="{:.1}" y="{:.1}" font-size="14" font-weight="650" fill="{}">{}</text>"#,
            r.x + 18.0 + node.kind_label.chars().count() as f32 * 8.4 + 14.0,
            r.y + 25.0,
            theme.ink.hex(),
            escape(&node.name)
        );
        // Right side of the header: subtree cost badge (own + everything
        // nested inside), then the child count to its left. Mirrors the
        // canvas painter. ~6.3px per character at font-size 10.5; the name
        // runs ~7.2px per character at 14px. A box too narrow to fit the
        // badge next to the name moves it to the header's second line.
        let name_end = r.x
            + 18.0
            + node.kind_label.chars().count() as f32 * 8.4
            + 14.0
            + node.name.chars().count() as f32 * 7.2;
        let mut right = r.right() - 16.0;
        if let Some(total) = subtree[placed.index] {
            let cost = format_cost(total, topology.currency.as_deref());
            let width = cost.chars().count() as f32 * 6.3;
            let fits_header = right - width - 8.0 > name_end;
            let y = if fits_header { r.y + 24.0 } else { r.y + 46.0 };
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" font-size="10.5" fill="{}" text-anchor="end">{}</text>"#,
                right,
                y,
                theme.ink_2.hex(),
                escape(&cost)
            );
            if fits_header {
                right -= width + 8.0;
            }
        }
        if placed.child_count > 0 {
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" font-size="11" fill="{}" text-anchor="end">{}</text>"#,
                right,
                r.y + 24.0,
                theme.ink_3.hex(),
                placed.child_count
            );
        } else {
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="{}">No resources</text>"#,
                r.x + 18.0,
                r.y + 46.0,
                theme.ink_3.hex()
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
        let path = route_edge(
            &shift(layout.placed[si].rect, ox, oy),
            &shift(layout.placed[ti].rect, ox, oy),
        );
        let dash = if edge.kind == EdgeKind::Network {
            r#" stroke-dasharray="7 5""#
        } else {
            ""
        };
        let _ = write!(
            svg,
            r#"<path d="M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}" fill="none" stroke="{}" stroke-width="1.6"{dash}/>"#,
            path.p0.x,
            path.p0.y,
            path.c0.x,
            path.c0.y,
            path.c1.x,
            path.c1.y,
            path.p1.x,
            path.p1.y,
            theme.edge.hex()
        );
        let head = path.arrow_head(11.0);
        let _ = write!(
            svg,
            r#"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="{}"/>"#,
            head[0].x,
            head[0].y,
            head[1].x,
            head[1].y,
            head[2].x,
            head[2].y,
            theme.edge.hex()
        );
        if let Some(label) = &edge.label {
            let t = if path.loop_under {
                0.5
            } else {
                label_t(edge_index)
            };
            let mid = path.point_at(t);
            let half_w = label.len() as f32 * 2.9 + 6.0;
            let _ = write!(
                svg,
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="15" rx="4" fill="{}" fill-opacity="0.92"/>"#,
                mid.x - half_w,
                mid.y - 7.5,
                half_w * 2.0,
                theme.surface.hex()
            );
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" font-size="10" fill="{}" text-anchor="middle">{}</text>"#,
                mid.x,
                mid.y + 3.5,
                theme.ink_3.hex(),
                escape(label)
            );
        }
    }

    for placed in &layout.placed {
        if placed.is_container {
            continue;
        }
        let node = &topology.nodes[placed.index];
        let r = shift(placed.rect, ox, oy);
        let cat = theme.category_color(node.category);
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="10" fill="{}" stroke="{}" stroke-width="1"/>"#,
            r.x,
            r.y,
            r.w,
            r.h,
            theme.card.hex(),
            theme.hairline.hex()
        );
        // category accent bar
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.1}" width="3.5" height="{:.1}" rx="1.75" fill="{}"/>"#,
            r.x + 1.0,
            r.y + 6.0,
            r.h - 12.0,
            cat.hex()
        );
        // icon chip + glyph, centered in the card head (attachment rows,
        // when present, live below LEAF_H).
        let chip = 32.0;
        let (cx, cy) = (r.x + 14.0, r.y + (LEAF_H - chip) / 2.0);
        let _ = write!(
            svg,
            r#"<rect x="{cx:.1}" y="{cy:.1}" width="{chip}" height="{chip}" rx="8" fill="{}" fill-opacity="0.14"/>"#,
            cat.hex()
        );
        let _ = write!(
            svg,
            r#"<g transform="translate({:.1},{:.1})" stroke="{}" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round">{}</g>"#,
            cx + 8.0,
            cy + 8.0,
            cat.hex(),
            glyph_svg(node.category, cat)
        );
        let tx = cx + chip + 12.0;
        let mid = r.y + LEAF_H / 2.0;
        // Month-to-date cost badge, right-aligned on the name line (mirrors
        // ui/canvas.rs); the name gives up truncation room for it.
        let cost_text = node
            .total_cost()
            .map(|total| format_cost(total, topology.currency.as_deref()));
        let name_max = if cost_text.is_some() { 16 } else { 24 };
        if let Some(cost) = &cost_text {
            let _ = write!(
                svg,
                r#"<text x="{:.1}" y="{:.1}" font-size="10.5" fill="{}" text-anchor="end">{}</text>"#,
                r.right() - 10.0,
                if node.group.is_some() {
                    mid - 9.0
                } else {
                    mid - 4.0
                },
                theme.ink_2.hex(),
                escape(cost)
            );
        }
        if let Some(group) = &node.group {
            let _ = write!(
                svg,
                r#"<text x="{tx:.1}" y="{:.1}" font-size="12.5" font-weight="600" fill="{}">{}</text>"#,
                mid - 9.0,
                theme.ink.hex(),
                escape(&truncate(&node.name, name_max))
            );
            let _ = write!(
                svg,
                r#"<text x="{tx:.1}" y="{:.1}" font-size="10.5" fill="{}">{}</text>"#,
                mid + 4.0,
                theme.ink_3.hex(),
                escape(&truncate(&node.kind_label, 26))
            );
            let _ = write!(
                svg,
                r#"<text x="{tx:.1}" y="{:.1}" font-size="10.5" fill="{}" fill-opacity="0.78">{}</text>"#,
                mid + 17.0,
                theme.ink_3.hex(),
                escape(&truncate(group, 26))
            );
        } else {
            let _ = write!(
                svg,
                r#"<text x="{tx:.1}" y="{:.1}" font-size="12.5" font-weight="600" fill="{}">{}</text>"#,
                mid - 4.0,
                theme.ink.hex(),
                escape(&truncate(&node.name, name_max))
            );
            let _ = write!(
                svg,
                r#"<text x="{tx:.1}" y="{:.1}" font-size="11" fill="{}">{}</text>"#,
                mid + 12.0,
                theme.ink_3.hex(),
                escape(&truncate(&node.kind_label, 26))
            );
        }

        // Attachment rows below the head: mini icon + name each, shared
        // items below their own separator with a link icon on the right
        // (mirrors ui/canvas.rs).
        if !node.attachments.is_empty() {
            let _ = write!(
                svg,
                r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1"/>"#,
                r.x + 12.0,
                r.y + LEAF_H,
                r.right() - 12.0,
                r.y + LEAF_H,
                theme.hairline.hex()
            );
            let split = secondary_split_index(&node.attachments);
            for (i, att) in node.attachments.iter().enumerate() {
                let mut offset = LEAF_H + ATTACH_PAD + (i as f32 + 0.5) * ATTACH_ROW;
                if let Some(s) = split {
                    if i >= s {
                        offset += ATTACH_SPLIT;
                    }
                    if i == s {
                        let sep_y =
                            r.y + LEAF_H + ATTACH_PAD + s as f32 * ATTACH_ROW + ATTACH_SPLIT / 2.0;
                        let _ = write!(
                            svg,
                            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1"/>"#,
                            r.x + 20.0,
                            sep_y,
                            r.right() - 20.0,
                            sep_y,
                            theme.hairline.hex()
                        );
                    }
                }
                let cy = r.y + offset;
                // Secondary glyphs carry their category color (public IP =
                // network green, SSH key = security red); hardware stays
                // muted.
                let icon_color = match att.kind.as_str() {
                    "public ip" | "private endpoint" => {
                        theme.category_color(ResourceCategory::Network)
                    }
                    "ssh key" | "nsg" => theme.category_color(ResourceCategory::Security),
                    "restore point" | "snapshot" => theme.category_color(ResourceCategory::Compute),
                    _ => theme.ink_3,
                };
                // 16-grid glyph scaled to 11px, centered on (x=27, cy).
                let _ = write!(
                    svg,
                    r#"<g transform="translate({:.1},{:.1}) scale(0.6875)" stroke="{}" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round">{}</g>"#,
                    r.x + 27.0 - 5.5,
                    cy - 5.5,
                    icon_color.hex(),
                    attachment_glyph_svg(&att.kind, icon_color)
                );
                let _ = write!(
                    svg,
                    r#"<text x="{:.1}" y="{:.1}" font-size="10.5" fill="{}">{}</text>"#,
                    r.x + 38.0,
                    cy + 3.5,
                    theme.ink_2.hex(),
                    escape(&truncate(&att.name, if att.shared { 20 } else { 24 }))
                );
                if att.shared {
                    let _ = write!(
                        svg,
                        r#"<g transform="translate({:.1},{:.1}) scale(0.625)" stroke="{}" stroke-width="1.6" fill="none" stroke-linecap="round">{}</g>"#,
                        r.right() - 16.0 - 5.0,
                        cy - 5.0,
                        theme.accent.hex(),
                        link_glyph_svg()
                    );
                }
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

fn shift(r: Rect, ox: f32, oy: f32) -> Rect {
    Rect {
        x: r.x + ox,
        y: r.y + oy,
        ..r
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// 16x16 mini glyphs for card attachment rows (mirrors
/// `ui/glyphs.rs::draw_attachment` — keep the two in sync).
fn attachment_glyph_svg(kind: &str, color: Rgb) -> String {
    match kind {
        "disk" => concat!(
            r#"<rect x="2.5" y="4.5" width="11" height="7" rx="1"/>"#,
            r#"<path d="M2.5 8.7h11"/>"#
        )
        .to_string()
            + &format!(
                r#"<circle cx="11.4" cy="10.1" r="0.8" fill="{}" stroke="none"/>"#,
                color.hex()
            ),
        "nic" => concat!(
            r#"<circle cx="8" cy="3.5" r="1.8"/><circle cx="3.5" cy="12" r="1.8"/><circle cx="12.5" cy="12" r="1.8"/>"#,
            r#"<path d="M7 5.1 4.4 10.4M9 5.1l2.6 5.3M5.3 12h5.4"/>"#
        )
        .to_string(),
        "extension" => {
            r#"<path d="M2.8 7h3.4V5.4l.9-1h1.8l.9 1V7h3.4v6.2H2.8Z"/>"#.to_string()
        }
        "public ip" => concat!(
            r#"<circle cx="8" cy="8" r="5.4"/>"#,
            r#"<ellipse cx="8" cy="8" rx="2.4" ry="5.4"/>"#,
            r#"<path d="M2.6 8h10.8"/>"#
        )
        .to_string(),
        "restore point" => concat!(
            r#"<circle cx="8" cy="8" r="5.5"/>"#,
            r#"<path d="M8 8V4.4M8 8l2.8 1.4"/>"#
        )
        .to_string(),
        "ssh key" => concat!(
            r#"<circle cx="4.8" cy="8" r="2.4"/>"#,
            r#"<path d="M7.2 8h6.2M10.8 8v2.6M13.4 8v2.6"/>"#
        )
        .to_string(),
        "snapshot" => concat!(
            r#"<rect x="2.5" y="5" width="11" height="8" rx="1"/>"#,
            r#"<circle cx="8" cy="9" r="2.4"/>"#,
            r#"<path d="M6 5l1-1.6h2L10 5"/>"#
        )
        .to_string(),
        "private endpoint" => format!(
            r#"<circle cx="3.5" cy="8" r="1.6" fill="{}" stroke="none"/><path d="M5.1 8h4.2"/><circle cx="11.6" cy="8" r="2.3"/>"#,
            color.hex()
        ),
        "nsg" => {
            r#"<path d="M8 2.6 13 4.6 12.4 9.4 8 13.4 3.6 9.4 3 4.6Z"/>"#.to_string()
        }
        "slot" => concat!(
            r#"<rect x="3" y="3" width="7.4" height="7.4" rx="1"/>"#,
            r#"<rect x="5.6" y="5.6" width="7.4" height="7.4" rx="1"/>"#
        )
        .to_string(),
        _ => format!(
            r#"<g fill="{}" stroke="none"><circle cx="4" cy="8" r="1"/><circle cx="8" cy="8" r="1"/><circle cx="12" cy="8" r="1"/></g>"#,
            color.hex()
        ),
    }
}

/// Link badge for shared attachments (mirrors `glyphs::draw_link`).
fn link_glyph_svg() -> &'static str {
    r#"<circle cx="5" cy="11" r="2.6"/><circle cx="11" cy="5" r="2.6"/><path d="M6.8 9.2l2.4-2.4"/>"#
}

/// 16x16 line glyphs, one per category (mirrors the in-app painter glyphs).
fn glyph_svg(category: ResourceCategory, color: Rgb) -> String {
    use ResourceCategory::*;
    let filled = |body: &str| format!(r#"<g fill="{}" stroke="none">{body}</g>"#, color.hex());
    match category {
        Compute => concat!(
            r#"<rect x="4.5" y="4.5" width="7" height="7" rx="1"/>"#,
            r#"<path d="M6 4.5V2.5M10 4.5V2.5M6 13.5v-2M10 13.5v-2M4.5 6h-2M4.5 10h-2M13.5 6h-2M13.5 10h-2"/>"#
        )
        .to_string()
            + &filled(r#"<rect x="7" y="7" width="2" height="2"/>"#),
        Network => concat!(
            r#"<circle cx="8" cy="3.5" r="1.8"/><circle cx="3.5" cy="12" r="1.8"/><circle cx="12.5" cy="12" r="1.8"/>"#,
            r#"<path d="M7 5.1 4.4 10.4M9 5.1l2.6 5.3M5.3 12h5.4"/>"#
        )
        .to_string(),
        Storage => r#"<rect x="2.5" y="4" width="11" height="8" rx="1.2"/><path d="M2.5 8.5h11"/>"#.to_string()
            + &filled(r#"<circle cx="5" cy="10.3" r="0.8"/>"#),
        Database => concat!(
            r#"<ellipse cx="8" cy="4" rx="4.8" ry="1.9"/>"#,
            r#"<path d="M3.2 4v8c0 1 2.1 1.9 4.8 1.9s4.8-.9 4.8-1.9V4"/>"#,
            r#"<path d="M3.2 8c0 1 2.1 1.9 4.8 1.9S12.8 9 12.8 8"/>"#
        )
        .to_string(),
        Containers => concat!(
            r#"<rect x="2.8" y="2.8" width="4.6" height="4.6" rx="0.8"/>"#,
            r#"<rect x="8.6" y="2.8" width="4.6" height="4.6" rx="0.8"/>"#,
            r#"<rect x="2.8" y="8.6" width="4.6" height="4.6" rx="0.8"/>"#,
            r#"<rect x="8.6" y="8.6" width="4.6" height="4.6" rx="0.8"/>"#
        )
        .to_string(),
        Security => concat!(
            r#"<path d="M8 2.3 12.6 4v4c0 2.8-1.9 4.7-4.6 5.7C5.3 12.7 3.4 10.8 3.4 8V4Z"/>"#,
            r#"<path d="M6.2 8l1.3 1.3 2.4-2.6"/>"#
        )
        .to_string(),
        Integration => concat!(
            r#"<path d="M3 6a5 5 0 0 1 9-1.6"/><path d="M12.3 2.2v2.4H9.9"/>"#,
            r#"<path d="M13 10a5 5 0 0 1-9 1.6"/><path d="M3.7 13.8v-2.4h2.4"/>"#
        )
        .to_string(),
        Web => r#"<circle cx="8" cy="8" r="5.4"/><ellipse cx="8" cy="8" rx="2.4" ry="5.4"/><path d="M2.6 8h10.8"/>"#
            .to_string(),
        Other => filled(r#"<circle cx="3.5" cy="8" r="1"/><circle cx="8" cy="8" r="1"/><circle cx="12.5" cy="8" r="1"/>"#),
        Scope => r#"<path d="M4.6 12.5a3 3 0 0 1-.3-6A4 4 0 0 1 12 5.8a2.9 2.9 0 0 1-.5 6.7Z"/>"#.to_string(),
        Group => r#"<path d="M2.5 4.5h4l1.4 1.6h5.6v6.4a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1Z"/>"#.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::demo::demo_topology;
    use crate::theme;

    #[test]
    fn export_produces_wellformed_svg_with_all_nodes() {
        let topo = demo_topology();
        let svg = to_svg(&topo, &theme::DARK);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        for node in &topo.nodes {
            assert!(svg.contains(&node.name), "missing node {}", node.name);
        }
        // one bezier path per edge
        assert_eq!(svg.matches("<path d=\"M ").count(), topo.edges.len());
    }

    #[test]
    fn export_shows_cost_badges() {
        let topo = demo_topology();
        let svg = to_svg(&topo, &theme::DARK);
        // vm-web-01: own 33.58 + attached disk 3.20; lb-web: 18.26 + 2.92.
        assert!(svg.contains(">$36.78<"), "vm total cost badge missing");
        assert!(svg.contains(">$21.18<"), "lb total cost badge missing");
        // Containers roll up their subtree: snet-web covers the two web VMs
        // (36.78 + 33.58); vnet-hub adds the delegated Postgres in snet-data
        // (+42.00); the AKS box its node pool + lb (61.44 + 2.92); each plan
        // box its app; the "Managed disks" detached box and the Monitoring
        // box their members (badge on the leaf AND the box).
        assert!(svg.contains(">$70.36<"), "subnet rollup missing");
        assert!(svg.contains(">$112.36<"), "vnet rollup missing");
        assert!(svg.contains(">$64.36<"), "AKS box rollup missing");
        assert_eq!(svg.matches(">$12.41<").count(), 2, "plan box rollup");
        assert_eq!(svg.matches(">$3.10<").count(), 2, "function plan rollup");
        assert_eq!(svg.matches(">$42.00<").count(), 2, "data subnet rollup");
        assert_eq!(svg.matches(">$5.63<").count(), 2, "detached disk box");
        assert_eq!(svg.matches(">$7.75<").count(), 2, "monitoring box");
        // The category sweep's standalone boxes roll up too: Databases =
        // cosmos 31.07 + redis 15.23.
        assert!(svg.contains(">$46.30<"), "databases box rollup missing");
        assert_eq!(svg.matches(">$21.18<").count(), 2, "networking box");
        // A card or box without cost data shows no zero badge.
        assert!(!svg.contains(">$0.00<"));
    }
}
