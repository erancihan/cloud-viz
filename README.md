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
- Lays out resources in ordered shelf rows: virtual networks lead on the
  top band, the other containers follow by size, then leaf cards grouped
  by category — a rectangular, scannable picture instead of a force blob.
  Nesting keeps what belongs together: VMs render inside the subnet their
  NIC lives in (vnet ▸ subnet ▸ VMs), App Services inside their App
  Service plan, and an AKS cluster becomes a container holding everything
  in its `MC_...` node resource group (VM scale sets, load balancers,
  public IPs) so the Kubernetes infra reads as one unit. The resource
  group shows as card subtext instead of a container box (the subscription
  is already in the toolbar).
- Folds subsidiary resources into their owner's card instead of drawing
  them as nodes: attached managed disks (via ARM's `managedBy`), NICs,
  public IPs, SSH keys, NSGs (onto every card they protect — nic-level
  refs plus subnet-level refs applied to each member, shared-flagged like
  a key on several VMs), VM restore point collections (via each
  collection's `source.id`, read per-RG), and — generically — any child
  resource whose parent is on the canvas (VM extensions, deployment slots,
  CDN endpoints, email domains, private DNS zone links…) render as icon
  rows on the owner's card, with the full list also in the details panel.
  Hardware rows come first; security / reachability / backups sit below
  their own separator, colored by category, with a link icon when shared
  across nodes. Unused keys, unattached disks and IPs, container
  registries, fully detached NSGs, and unwired container instances stay
  visible in dashed "Detached …" boxes; regional services (network
  watchers) group under a "Regional" box; monitoring debris (alert rules,
  action groups, dashboards, App Insights, Log Analytics workspaces)
  gathers into a "Monitoring" box; and whatever is still top-level with no
  edges at all parks in a per-category "Standalone" box (Databases,
  Storage, Security…) — all styled like a virtual network so nothing
  loiters. A single association keeps a card free. Cost rows for resources
  deleted during the billing period surface as a warning instead of
  vanishing silently. Remaining relationships (snapshot → disk, vault →
  VM, VM → diagnostics storage, …) route as relaxed beziers.
- Nests services into the subnet they live in even without a NIC: Bastion
  hosts (their AzureBastionSubnet), vnet-integrated PostgreSQL flexible
  servers (their delegated subnet), standalone VM scale sets — and function
  apps into their App Service plan (`az functionapp list`; `az webapp list`
  omits them).
- Shows cost on each card (top-right badge): one Cost Management query per
  subscription (`az rest` — actual cost grouped by ResourceId), so a VM's
  badge is the VM plus everything folded into its card (disks, public IPs,
  …); the details panel breaks it down per attachment. The toolbar's COSTS
  dropdown picks the billing window — this month to date (default) or any
  of the last 12 calendar months; each period caches separately, so
  revisiting a month is instant. Needs the Cost Management Reader role —
  without it the cards simply render without badges (a warning explains
  why). Container boxes (App Service plans, AKS clusters, vnets, subnets,
  the dashed group boxes) show their whole subtree's total — the plan's
  charge plus everything nested inside, the way the bill reads.
- Details panel has a collapsed **Delete** section: the `az` commands that
  remove the resource *and* everything folded into its card, in dependency
  order (restore points, then the resource — freeing its NICs and disks —
  then NICs, then the public IPs those NICs held, then disks, then SSH keys)
  so nothing is left behind. Edge-connected dependents whose *only*
  connection is the resource being deleted (an NSG protecting just this VM,
  a snapshot of just this disk) join the teardown; anything with more
  connections is excluded with a note, like shared attachments — and a
  vault backing the resource up becomes a "disable protection first"
  warning. Virtual networks and subnets get children-first plans: every
  member's block runs before the vnet delete, and subnets are noted as
  dying with their vnet. Child resources (extensions, slots) are noted as
  dying with their parent, and a copy button grabs the whole script.
  CloudViz itself never runs them — read-only stays read-only.
- Pan (drag), zoom (scroll, cursor-anchored; +/- buttons by the minimap,
  center-anchored), fit-to-view, clickable minimap, light/dark themes,
  fullscreen (F11), details panel per resource. The screen zoom (Ctrl +/-)
  persists across sessions (`~/.config/cloudviz/config.json`,
  `%APPDATA%\cloudviz\config.json`).
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
├── layout.rs         shelf-packed rows at every level, canonical order ← unit tested
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
- **Deterministic layout**: every level shelf-packs in a canonical order
  derived purely from node content (kind, category, name, id) — never from
  the CLI's listing order — so the same subscription always produces the
  same picture, even when `az` returns resources shuffled between
  refreshes. Layout invariants (parents-before-children, containment, no
  overlap, minimum clearance, stability under input reordering) are
  unit-tested.

## Azure commands used

`az account show` · `az account list` · `az group list` ·
`az resource list` · `az network vnet list` · `az network nic list` ·
`az webapp list` · `az functionapp list` · `az vm list` · `az sshkey list` ·
`az aks list` · `az snapshot list` · `az network bastion list` ·
`az vmss list` · `az postgres flexible-server list` ·
`az restore-point collection list` (per RG that has one) ·
`az backup item list` (per Recovery Services vault)
(all with `--output json --only-show-errors`; everything after
`az resource list` is best-effort enrichment and only produces warnings on
failure).

Associations come from fields in those responses: `managedBy` on
`az resource list` (an attached managed disk points at its VM),
`networkSecurityGroup` on subnets and NICs, the NIC's `virtualMachine` /
`subnet` / `publicIPAddress` references, `appServicePlanId` on the webapp /
functionapp listings, child-resource ids (VM extensions, site slots, CDN
endpoints, email domains, DNS zone links — any `parent/child` typed
resource whose parent is present), SSH key material matched between
`az sshkey list` and each VM's osProfile (ARM copies the key text into the
VM instead of referencing the key resource), the `nodeResourceGroup` from
`az aks list` (everything in a cluster's `MC_...` group belongs to it),
`creationData.sourceResourceId` on snapshots, the Bastion / VMSS ip
configurations and the PostgreSQL `network.delegatedSubnetResourceId` (all
three nest into their subnet), each VM's boot-diagnostics `storageUri`, and
the protected items of `az backup item list` (vault → VM "backs up"
edges). Resources with no cheap association source (container registries,
key vaults, certificates, Front Door, plain storage accounts…) stay
free-floating deliberately.

## Roadmap

- Richer Azure relationship harvesting (load balancers, private endpoints,
  peerings, app → database wiring from connection metadata).
- AWS provider via the `aws` CLI.
- Resource interactions: rename, tag, delete with confirmation flows.
- PNG export and saved snapshots for drift comparison.
