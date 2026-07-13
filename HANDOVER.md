# CloudViz — Handover

A working reference for picking this project up. Pair it with `README.md`
(user-facing overview) — this file is the builder's-eye view: how it's wired,
how to verify changes, where to extend, and what's deliberately unfinished.

## 1. Where things stand

A functional read-only MVP:

- **Two providers**: `azure` (live, via the `az` CLI) and `demo` (bundled
  sample estate, no account needed).
- **Interactive native canvas** (egui): pan, zoom, fit-to-view, clickable
  minimap, per-resource details panel, light/dark themes.
- **Headless SVG export** sharing the exact layout/routing code as the GUI.
- **21 unit tests pass; `cargo clippy` is clean.**

The repo is standalone — it was moved out of the `erancihan/erancihan`
monorepo (previously at `projects/cloudviz`), now living at the root here.

## 2. Architecture at a glance

The load-bearing idea: **everything cloud-specific lives behind a trait; the
UI only ever sees the provider-agnostic `Topology`.** Adding a cloud never
touches rendering.

```
az CLI (JSON)
   │   providers/azure/cli.rs         run `az … --output json`, classify errors
   ▼
az_types.rs (serde structs)           only the fields we read; casing-drift tolerant
   │   providers/azure/mapper.rs      PURE: az JSON → Topology   ← unit tested
   ▼
model.rs  ── Topology { nodes, edges }   the ONLY vocabulary the UI understands
   │
   ├─► layout.rs   nested shelf-packing → absolute Rects   ← unit tested
   │        │
   │        ├─► ui/canvas.rs   hand-painted egui canvas (+ ui/glyphs.rs icons)
   │        └─► export.rs      SVG document (same layout + geom)
   │
   └─► geom.rs     bezier edge routing (side-facing anchors)  ← unit tested

app.rs      eframe App: provider/scope state machine + background fetch threads
main.rs     entry point; GUI mode, and headless `--export-svg` mode
theme.rs    light/dark tokens + CVD-validated categorical palette
```

File-by-file:

| File | Responsibility |
|------|----------------|
| `src/model.rs` | `Topology`, `TopologyNode`, `TopologyEdge`, `ResourceCategory`, provider result/status types. No I/O, no GUI. |
| `src/providers/mod.rs` | `CloudProvider` trait + `builtin_providers()` registry. |
| `src/providers/azure/cli.rs` | `AzExecutor` (injectable), runs `az`, maps failures → `ProviderError` codes. |
| `src/providers/azure/az_types.rs` | serde shapes for `az` output. |
| `src/providers/azure/mapper.rs` | Pure `az JSON → Topology`: categorization, type labels, containment, NIC-derived edges. |
| `src/providers/azure/mod.rs` | `AzureProvider`: status / scopes / fetch orchestration; required vs best-effort listings. |
| `src/providers/azure/fixtures/*.json` | Recorded `az` output; drives the mapper tests. |
| `src/providers/demo.rs` | `DemoProvider` + `demo_topology()` sample estate (covers every category + edge kind). |
| `src/layout.rs` | Deterministic layout: shelf-packed containers (vnet ▸ subnet ▸ members) + a top-level compound spring embedder that clusters connected resources; `Rect` helpers; spacing constants. |
| `src/geom.rs` | Edge routing (`route_edge`, `EdgePath`, arrowheads, `label_t` stagger). |
| `src/theme.rs` | `Theme` (`LIGHT`/`DARK`), `Rgb`, `mix()`, per-category colors. |
| `src/ui/canvas.rs` | Camera (pan/zoom), hit-testing, painting containers/cards/edges, minimap. |
| `src/ui/glyphs.rs` | One line-glyph per category, drawn with egui primitives. |
| `src/app.rs` | State machine, `mpsc` worker threads, toolbar, details panel, error screens. |
| `src/export.rs` | SVG exporter. |
| `src/main.rs` | Entry point + `--export-svg` headless mode. |

## 3. Build, run, test

```bash
cargo run --release        # launch the desktop app
cargo test                 # 21 tests: mapper, layout, geom, demo data, export
cargo clippy --all-targets # keep this clean (0 warnings today)
cargo fmt                  # not yet enforced by CI — run it before pushing
```

Dependencies are intentionally few: `eframe`/`egui`, `serde`, `serde_json`.
`Cargo.lock` **is committed** (this is a binary crate).

## 4. Verifying visual changes without a display

The dev container here has no display, so the GUI can't be launched or
screenshotted directly. The `--export-svg` mode is the proxy — it runs the
**same** `layout.rs` + `geom.rs` code the canvas uses, so if the SVG looks
right, the canvas geometry is right:

```bash
cargo run -- --export-svg /tmp/topo-dark.svg
cargo run -- --export-svg /tmp/topo-light.svg --light
# then render to PNG to eyeball (headless chromium is available):
node -e "const{chromium}=require('/opt/node22/lib/node_modules/playwright/index.js');(async()=>{const b=await chromium.launch();const p=await b.newPage({viewport:{width:1400,height:1260}});await p.goto('file:///tmp/topo-dark.svg');await p.screenshot({path:'/tmp/topo-dark.png'});await b.close();})()"
```

What the SVG **can't** verify: live interaction (pan/zoom/hit-testing/minimap
dragging) and egui-specific painting. Those need a real run on your machine.
When you touch `ui/canvas.rs` interaction code, run the app locally.

## 5. How to extend

### Add a cloud provider (e.g. AWS)
1. `src/providers/aws/` implementing `CloudProvider` (`check_status`,
   `list_scopes`, `fetch_topology`).
2. A **pure** `mapper.rs` (`CLI JSON → Topology`) with `az_types`-style serde
   structs, unit-tested against recorded-output fixtures (copy the Azure
   pattern exactly). Map native types → `ResourceCategory`; use `parent_id`
   for containment (account ▸ region ▸ VPC ▸ subnet …).
3. One line in `builtin_providers()`. It appears in the toolbar automatically.

### Add / adjust a resource type
- New category → add a variant in `ResourceCategory` (model.rs), then fill in
  `label()`, `sort_rank()`, a color in `theme.rs::category_color`, and a glyph
  in `ui/glyphs.rs` **and** `export.rs::glyph_svg` (two painters, keep in sync).
- New Azure type mapping → `mapper.rs`: `CATEGORY_BY_PREFIX` and the
  `KNOWN_TYPE_LABELS`/`type_label` table.

### Tune the "relaxed" look
- Spacing: constants at the top of `layout.rs` (`GAP`, `PAD`, `ROOT_GAP`, …).
- Curve shape / anchoring / loop-under threshold: `geom.rs::route_edge`.
- Label placement stagger: `geom.rs::label_t`.

## 6. Invariants worth preserving (tests guard these)

- **Layout**: parents emitted before children; children stay inside parents;
  no sibling overlap; a minimum clearance between cards (the "relaxed" rule).
- **Mapper**: node ids are lowercased and unique; edges only ever connect
  nodes that exist (dangling references are dropped, not invented).
- **Palette**: categorical colors are CVD-validated and identity is never
  color-alone (glyph + text always present); edge kinds are dash-coded.
- **Credentials**: never stored — the app shells out to the user's own `az`
  session and reads JSON. Keep it that way.

## 7. Known limitations / gaps

- **Read-only.** No create/rename/tag/delete yet (deliberate; see roadmap).
- **Edge coverage is shallow.** Only NIC-derived relationships
  (VM→NIC→subnet, NIC→public IP) plus the demo's hand-authored edges. No load
  balancer wiring, private endpoints, VNet peering, or app→database links.
- **Azure is the only live provider.**
- **No CI.** fmt/clippy/test are manual. First recommended task below.
- **No PNG export, no saved snapshots / drift comparison.**
- **Layout is fully automatic** — no manual node dragging or persisted
  positions.
- Large estates are untested for performance; layout is O(n) shelf-packing but
  the canvas repaints everything each frame.

## 8. Suggested next steps (roughly ordered)

1. **CI** — a GitHub Actions workflow running `cargo fmt --check`,
   `cargo clippy -- -D warnings`, `cargo test`. Cheapest guardrail; do it first.
2. **Richer Azure edges** — harvest LB→backend, private endpoints, peerings,
   and app→DB from connection strings/settings. Pure additions to the mapper
   with new fixtures.
3. **AWS provider** — per section 5; proves the abstraction.
4. **Resource interactions** — rename/tag/delete behind explicit confirmation
   flows. This is the first place the app stops being read-only, so design the
   safety/confirmation UX deliberately.
5. **PNG export + snapshots** — reuse `export.rs`; add a rasterize step and a
   diff-two-snapshots mode.

## 9. Gotchas

- **Two glyph painters** exist (`ui/glyphs.rs` for egui, `export.rs::glyph_svg`
  for SVG). Changing an icon means changing both.
- **Windows `az`** is a `.cmd` shim — `cli.rs` already routes through `cmd /C`;
  don't "simplify" that away.
- **Best-effort listings**: `az network vnet/nic list` failures become
  `Topology.warnings` (shown in the status bar), not hard errors. `account
  show` / `group list` / `resource list` are required and do fail the fetch.
- **Fetch runs off-thread** (`app.rs`) with a generation counter; stale
  results from a superseded provider/scope switch are discarded. Preserve that
  when adding async work.
- **Determinism**: same input → same picture (no physics sim, no RNG). Keep
  new layout logic deterministic so the SVG-diff workflow stays meaningful.
