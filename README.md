# CloudViz

Native cross-platform desktop app (Windows / macOS / Linux) that visualizes
your cloud infrastructure as an interactive topology diagram. Written in Rust
with [egui](https://github.com/emilk/egui) — a single compiled binary, no
Electron, no web runtime, no JavaScript.

Azure is the first supported provider; the architecture is deliberately
provider-agnostic so AWS, GCP, etc. can be added without touching the UI.

**How it gets your data:** you sign in with the provider's own CLI in your own
terminal (`az login`). CloudViz shells out to that CLI, reads the inventory as
JSON, and renders it. The app never sees or stores credentials.

## Quick start

Requirements: a Rust toolchain, and for live Azure data the
[Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli) signed in
via `az login`. Without the CLI you can still explore the bundled **Demo**
provider.

```bash
cargo run --release            # launch the app
cargo test                     # mapper, layout, routing, demo-data tests
```

Headless SVG export (also handy for docs and snapshot diffing):

```bash
cargo run -- --export-svg topo.svg                       # demo data, dark
cargo run -- --export-svg topo.svg --light               # demo data, light
cargo run -- --export-svg topo.svg --provider azure      # your live estate
```

## What it does today

- Fetches an Azure subscription's inventory through `az` (subscriptions,
  resource groups, all resources, virtual networks/subnets, NICs).
- Normalizes it into a provider-agnostic topology graph.
- Renders nested containment — subscription ▸ resource groups ▸ resources,
  vnets ▸ subnets — plus relationship edges (VM → NIC → subnet, NIC → public
  IP) as relaxed bezier curves with generous spacing.
- Pan (drag), zoom (scroll, cursor-anchored), fit-to-view, clickable minimap,
  light/dark themes, fullscreen (F11), details panel per resource.
- Degrades gracefully: CLI missing → install guidance; signed out →
  `az login` guidance; individual enrichment listings failing → warning badge,
  not a broken screen.

Read-only by design for now. Interactions with resources (rename, delete, …)
are a later milestone — see Roadmap.

## Architecture

```
src/
├── model.rs          Topology / TopologyNode / TopologyEdge / categories
│                     — the ONLY vocabulary the UI understands
├── providers/
│   ├── mod.rs        CloudProvider trait + builtin_providers() registry
│   ├── azure/
│   │   ├── cli.rs    runs `az … --output json` (std::process, no shell)
│   │   ├── az_types  serde shapes for az output (casing-drift tolerant)
│   │   ├── mapper.rs pure: az JSON → Topology        ← unit tested
│   │   └── mod.rs    status / scopes / fetch orchestration
│   └── demo.rs       bundled sample estate (Demo provider)
├── layout.rs         nested shelf-packing, relaxed spacing ← unit tested
├── geom.rs           bezier edge routing (side-facing anchors) ← unit tested
├── theme.rs          light/dark tokens + CVD-validated categorical palette
├── ui/canvas.rs      hand-painted canvas: containers, cards, edges,
│                     pan/zoom camera, hit-testing, minimap
├── app.rs            state machine + background fetch workers (mpsc)
└── export.rs         SVG exporter (same layout/routing as the canvas)
```

Everything cloud-specific lives behind the `CloudProvider` trait; fetches run
on worker threads so the UI never blocks on the CLI. The layout, routing,
mapper, and export modules have no GUI dependency and are tested headlessly.

## Adding a new provider (e.g. AWS)

1. Create `src/providers/aws/` and implement the `CloudProvider` trait:
   - `check_status()` — is the `aws` CLI installed and authenticated?
   - `list_scopes()` — account/region pairs (Azure uses subscriptions).
   - `fetch_topology(scope)` — run the CLI listings, then map to `Topology`.
2. Write a pure mapper (CLI JSON → `Topology`) alongside it and unit-test it
   with fixture files, mirroring `azure/mapper.rs`. Map native resource types
   onto the shared `ResourceCategory` buckets and use `parent_id` for
   containment (account ▸ region ▸ VPC ▸ subnet …).
3. Register it — one line in `builtin_providers()`.

The provider then shows up in the toolbar dropdown automatically.

## Design notes

- **Relaxed by default**: spacing constants in `layout.rs` (36px between
  cards, 28px padding, 120px between top-level groups) are deliberately
  generous, and edges are wide-swinging beziers anchored to whichever node
  side faces the other end — no tight right-angle routing. A layout test
  enforces the minimum clearance.
- **Colors**: node categories use a fixed-order categorical palette validated
  for color-vision deficiency and per-theme contrast (separately stepped for
  light and dark). Identity is never color-alone — every node also carries a
  glyph and text labels, and edge kinds are dash-pattern-coded.
- **Deterministic layout**: bottom-up shelf packing; the same subscription
  always produces the same picture. Layout invariants (parents-before-children,
  containment, no overlap, minimum clearance) are unit-tested.

## Azure commands used

`az account show` · `az account list` · `az group list` ·
`az resource list` · `az network vnet list` · `az network nic list`
(all with `--output json --only-show-errors`; the last two are best-effort
enrichment and only produce warnings when they fail).

## Roadmap

- Richer Azure relationship harvesting (load balancers, private endpoints,
  peerings, app → database wiring from connection metadata).
- AWS provider via the `aws` CLI.
- Resource interactions: rename, tag, delete with confirmation flows.
- PNG export and saved snapshots for drift comparison.
