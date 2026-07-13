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
- Caches the fetched topology on disk (`~/.cache/cloudviz`,
  `%LOCALAPPDATA%\cloudviz\cache`) and shows it instantly on the next
  start-up instead of re-running the inventory; the status bar shows the
  cache age and **⟳ Refresh** always fetches live.
- Normalizes it into a provider-agnostic topology graph.
- Lays out resources by dependency: a compound spring embedder clusters
  connected resources together and nests what belongs together — VMs render
  inside the subnet their NIC lives in (vnet ▸ subnet ▸ VMs), App Services
  inside their App Service plan, and an AKS cluster becomes a container
  holding everything in its `MC_...` node resource group (VM scale sets,
  load balancers, public IPs) so the Kubernetes infra reads as one unit.
  The resource group shows as card subtext instead of a container box (the
  subscription is already in the toolbar).
- Folds subsidiary resources into their owner's card instead of drawing
  them as nodes: attached managed disks (via ARM's `managedBy`), NICs, VM
  extensions, deployment slots, SSH keys, public IPs, and VM restore point
  collections (via each collection's `source.id`, read per-RG) render as
  icon rows on the owner's card, with the full list also in the details
  panel. Hardware rows come first; credentials / reachability / backups (SSH
  keys, public IPs, restore points) sit below their own separator, colored
  by category, with a link icon when shared across nodes (an SSH key on
  several VMs). Unused keys, unattached disks and IPs stay visible in dashed
  "Detached …" boxes; regional services (network watchers) group under a
  "Regional" box instead — both styled like a virtual network so they don't
  scatter. Remaining relationships (NSG → subnet/VM, app → db, …) route as
  relaxed beziers.
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
├── layout.rs         shelf-packed containers + top-level spring embedder ← unit tested
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
  cards, 28px padding) are deliberately generous, and edges are wide-swinging
  beziers anchored to whichever node side faces the other end — no tight
  right-angle routing. A layout test enforces the minimum clearance.
- **Colors**: node categories use a fixed-order categorical palette validated
  for color-vision deficiency and per-theme contrast (separately stepped for
  light and dark). Identity is never color-alone — every node also carries a
  glyph and text labels, and edge kinds are dash-pattern-coded.
- **Deterministic layout**: containers shelf-pack bottom-up and the top level
  runs a fixed-iteration, RNG-free spring embedder, so the same subscription
  always produces the same picture. Layout invariants (parents-before-children,
  containment, no overlap, minimum clearance) are unit-tested.

## Azure commands used

`az account show` · `az account list` · `az group list` ·
`az resource list` · `az network vnet list` · `az network nic list` ·
`az webapp list` · `az vm list` · `az sshkey list` · `az aks list`
(all with `--output json --only-show-errors`; everything after
`az resource list` is best-effort enrichment and only produces warnings on
failure).

Associations come from fields in those responses: `managedBy` on
`az resource list` (an attached managed disk points at its VM),
`networkSecurityGroup` on subnets and NICs, the NIC's `virtualMachine` /
`subnet` / `publicIPAddress` references, `appServicePlanId` on
`az webapp list`, child-resource ids (VM extensions, site slots), SSH key
material matched between `az sshkey list` and each VM's osProfile (ARM
copies the key text into the VM instead of referencing the key resource),
and the `nodeResourceGroup` from `az aks list` (everything in a cluster's
`MC_...` group belongs to it).

## Roadmap

- Richer Azure relationship harvesting (load balancers, private endpoints,
  peerings, app → database wiring from connection metadata).
- AWS provider via the `aws` CLI.
- Resource interactions: rename, tag, delete with confirmation flows.
- PNG export and saved snapshots for drift comparison.
