//! Provider-agnostic topology model.
//!
//! Every cloud provider (Azure today, AWS/GCP later) normalizes its inventory
//! into these types. The UI knows nothing about any specific cloud — it only
//! understands this graph.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Normalized resource categories. These drive color coding and icons in the
/// UI. Provider mappers translate native types (e.g. ARM types) into one of
/// these buckets; `Scope` and `Group` are structural (subscription/account,
/// resource group/stack).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceCategory {
    Compute,
    Network,
    Storage,
    Database,
    Containers,
    Security,
    Integration,
    Web,
    Other,
    Scope,
    Group,
}

impl ResourceCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Compute => "Compute",
            Self::Network => "Networking",
            Self::Storage => "Storage",
            Self::Database => "Databases",
            Self::Containers => "Containers",
            Self::Security => "Security",
            Self::Integration => "Integration",
            Self::Web => "Web & apps",
            Self::Other => "Other",
            Self::Scope => "Scope",
            Self::Group => "Group",
        }
    }

    /// Fixed display/sort order for leaf categories (also the categorical
    /// color slot order — do not shuffle).
    pub fn sort_rank(self) -> u8 {
        match self {
            Self::Compute => 0,
            Self::Network => 1,
            Self::Storage => 2,
            Self::Database => 3,
            Self::Containers => 4,
            Self::Security => 5,
            Self::Integration => 6,
            Self::Web => 7,
            Self::Other => 8,
            Self::Scope => 9,
            Self::Group => 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    /// Stable unique id — for Azure this is the lowercased ARM resource id.
    pub id: String,
    pub name: String,
    /// Native type identifier, e.g. `Microsoft.Compute/virtualMachines`.
    pub kind: String,
    /// Human-friendly rendering of `kind`, e.g. `Virtual machine`.
    pub kind_label: String,
    pub category: ResourceCategory,
    /// Containment: id of the node this one lives inside.
    pub parent_id: Option<String>,
    /// True when the node renders as a container box holding its children.
    pub container: bool,
    /// Resource group / stack name, shown as card subtext. Purely
    /// informational — resource groups are no longer drawn as container boxes.
    /// `default` so older cached topology files still deserialize.
    #[serde(default)]
    pub group: Option<String>,
    /// Subsidiary resources folded into this node instead of drawn as their
    /// own nodes — e.g. a VM's attached managed disks, extensions, or SSH
    /// keys. Rendered as sub-cards on the owner's card and listed in full in
    /// the details panel.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// This resource's own accrued cost for the current billing month
    /// (month-to-date), in the topology's [`Topology::currency`]. `None`
    /// when the provider had no cost data (no permission, free resource…).
    #[serde(default)]
    pub cost: Option<f64>,
    pub region: Option<String>,
    /// Provider-specific extras surfaced in the details panel (tags, sku…).
    /// Kept ordered so the panel is stable between refreshes.
    pub metadata: Vec<(String, String)>,
}

impl TopologyNode {
    /// Month-to-date cost of this resource plus everything folded into its
    /// card (disks, public IPs…) — the number shown on the card. `None` when
    /// neither the resource nor any attachment has cost data.
    pub fn total_cost(&self) -> Option<f64> {
        let attached: Option<f64> = self
            .attachments
            .iter()
            .filter_map(|a| a.cost)
            .fold(None, |acc, c| Some(acc.unwrap_or(0.0) + c));
        match (self.cost, attached) {
            (None, None) => None,
            (own, att) => Some(own.unwrap_or(0.0) + att.unwrap_or(0.0)),
        }
    }
}

/// A subsidiary resource folded into its owner's card (see
/// [`TopologyNode::attachments`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// Short lowercase kind driving the row icon: "disk", "nic",
    /// "extension", "slot", "ssh key".
    pub kind: String,
    pub name: String,
    /// The folded resource's own id (for Azure the lowercased ARM id) —
    /// what a delete command must target. `None` when the provider didn't
    /// record one (older caches, purely synthetic attachments).
    #[serde(default)]
    pub id: Option<String>,
    /// True when the same underlying resource is folded into other nodes as
    /// well (e.g. an SSH key used by several VMs) — rendered with a link
    /// badge on the sub-card's corner.
    #[serde(default)]
    pub shared: bool,
    /// The folded resource's own month-to-date cost — counted into the
    /// owner card's [`TopologyNode::total_cost`].
    #[serde(default)]
    pub cost: Option<f64>,
}

impl Attachment {
    /// Secondary attachments — credentials and reachability (SSH keys,
    /// public IPs) — always render below their own separator on the card,
    /// after the hardware rows (disks, NICs, extensions, slots).
    pub fn secondary(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "ssh key" | "public ip" | "restore point"
        )
    }
}

/// One step of a [`DeletePlan`]: a CLI command plus what it removes.
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStep {
    /// Human label, e.g. `public ip pip-web` or `Virtual machine vm-web-01`.
    pub label: String,
    /// The command to run, e.g. `az resource delete --ids "…"`.
    pub command: String,
}

/// Ordered teardown of a node and everything folded into its card, shown in
/// the details panel's Delete section.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletePlan {
    /// Commands in dependency order — run top to bottom.
    pub steps: Vec<DeleteStep>,
    /// What is deliberately *not* a step and why: shared resources left in
    /// place, child resources that die with their parent, missing ids.
    pub notes: Vec<String>,
}

/// Position of an attachment kind in the teardown order; `None` for child
/// resources (extensions, slots) that are deleted together with their parent
/// and must not get their own command.
fn delete_rank(kind: &str) -> Option<u8> {
    match kind {
        // Backups referencing the resource go first, so nothing dangles.
        "restore point" => Some(0),
        // 1 is the resource itself: its deletion detaches NICs and disks.
        "nic" => Some(2),
        // A public IP can only be deleted once the NIC holding it is gone.
        "public ip" => Some(3),
        "disk" => Some(4),
        // Independent credential resources — pure cleanup, last.
        "ssh key" => Some(5),
        _ => None,
    }
}

/// Edge labels whose SOURCE exists purely for the sake of the TARGET, with
/// the step rank the pulled-in dependent gets when its only connection is
/// the resource being deleted. Deliberately excludes "manages"/"attached"
/// (deleting a disk must never pull in its VM), "backs up" (a vault is heavy
/// and multi-tenant — it becomes a note instead), and "boot diagnostics"
/// (the storage account holds unrelated data).
const PULL_IN: &[(&str, u8)] = &[("protects", 6), ("snapshot of", 0)];

/// Prebuilt lookups shared by every block of one delete plan.
struct PlanCtx<'a> {
    topology: &'a Topology,
    by_id: HashMap<&'a str, usize>,
    children: HashMap<&'a str, Vec<usize>>,
    /// Edge count per node id — a dependent with degree 1 exists only for
    /// the resource being deleted and may join its teardown.
    degree: HashMap<&'a str, usize>,
}

fn az_delete(id: &str) -> String {
    format!("az resource delete --ids \"{id}\"")
}

/// Builds the ordered CLI teardown for a node: its folded attachments
/// (dependents before dependencies — restore points, then the resource,
/// which frees its NICs and disks, then NICs, then the public IPs those
/// NICs held, then disks, then SSH keys), plus edge-connected dependents
/// (an NSG, a snapshot) whose ONLY connection is this resource — anything
/// with more connections is excluded with a note, like shared attachments.
/// Virtual networks and subnets get children-first plans; other containers
/// have nothing directly deletable and return `None`. Commands are Azure
/// CLI-shaped (the demo estate mimics Azure); when another provider lands,
/// branch on [`Topology::provider`] here.
pub fn delete_plan(topology: &Topology, index: usize) -> Option<DeletePlan> {
    let node = topology.nodes.get(index)?;
    if !node.id.contains("/subscriptions/") {
        return None;
    }
    let ctx = PlanCtx {
        topology,
        by_id: topology
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect(),
        children: children_index(topology),
        degree: {
            let mut degree: HashMap<&str, usize> = HashMap::new();
            for edge in &topology.edges {
                *degree.entry(edge.source.as_str()).or_default() += 1;
                *degree.entry(edge.target.as_str()).or_default() += 1;
            }
            degree
        },
    };

    let mut steps = Vec::new();
    let mut notes = Vec::new();
    let mut seen = HashSet::new();
    if node.container {
        let kind = node.kind.to_lowercase();
        let is_vnet = kind == "microsoft.network/virtualnetworks";
        let is_subnet = kind == "microsoft.network/virtualnetworks/subnets";
        if !is_vnet && !is_subnet {
            return None; // AKS/plans/synthetic boxes: nothing directly deletable
        }
        let mut visited = HashSet::from([index]);
        for kid in sorted_children(&ctx, &node.id) {
            let child = &topology.nodes[kid];
            if child.container {
                // A subnet inside the vnet: empty it, then note that the
                // subnet itself vanishes with the vnet delete.
                collect_children(&ctx, kid, &mut visited, &mut seen, &mut steps, &mut notes);
                notes.push(format!(
                    "subnet {} is deleted together with the virtual network",
                    child.name
                ));
            } else if visited.insert(kid) {
                leaf_block(&ctx, kid, &mut seen, &mut steps, &mut notes);
            }
        }
        if seen.insert(node.id.clone()) {
            steps.push(DeleteStep {
                label: format!("{} {}", node.kind_label, node.name),
                command: az_delete(&node.id),
            });
        }
        if is_subnet {
            notes.push(
                "the subnet is also removed automatically if the virtual network is deleted".into(),
            );
        }
    } else {
        leaf_block(&ctx, index, &mut seen, &mut steps, &mut notes);
    }

    let mut seen_notes = HashSet::new();
    notes.retain(|n| seen_notes.insert(n.clone()));
    Some(DeletePlan { steps, notes })
}

/// Children ordered deterministically: containers first, then by name.
fn sorted_children(ctx: &PlanCtx, id: &str) -> Vec<usize> {
    let mut kids = ctx.children.get(id).cloned().unwrap_or_default();
    kids.sort_by(|&a, &b| {
        let (na, nb) = (&ctx.topology.nodes[a], &ctx.topology.nodes[b]);
        nb.container
            .cmp(&na.container)
            .then_with(|| na.name.cmp(&nb.name))
    });
    kids
}

/// Recursively empty a container: leaf blocks child after child (each block
/// keeps its own internal dependency order), nested containers first.
/// `visited` guards against malformed parent cycles, `seen` against a
/// dependent reachable from two children getting two commands.
fn collect_children(
    ctx: &PlanCtx,
    index: usize,
    visited: &mut HashSet<usize>,
    seen: &mut HashSet<String>,
    steps: &mut Vec<DeleteStep>,
    notes: &mut Vec<String>,
) {
    if !visited.insert(index) {
        return;
    }
    for kid in sorted_children(ctx, &ctx.topology.nodes[index].id) {
        let child = &ctx.topology.nodes[kid];
        if child.container {
            collect_children(ctx, kid, visited, seen, steps, notes);
        } else if visited.insert(kid) {
            leaf_block(ctx, kid, seen, steps, notes);
        }
    }
}

/// One leaf's self-contained ordered block: the resource, its attachments,
/// and any single-connection edge dependents, appended to `steps` in
/// dependency order.
fn leaf_block(
    ctx: &PlanCtx,
    index: usize,
    seen: &mut HashSet<String>,
    steps: &mut Vec<DeleteStep>,
    notes: &mut Vec<String>,
) {
    let node = &ctx.topology.nodes[index];
    let mut ranked: Vec<(u8, DeleteStep)> = Vec::new();
    if seen.insert(node.id.clone()) {
        ranked.push((
            1,
            DeleteStep {
                label: format!("{} {}", node.kind_label, node.name),
                command: az_delete(&node.id),
            },
        ));
    }
    for att in &node.attachments {
        if att.shared {
            notes.push(format!(
                "{} {} is shared with other resources — left in place",
                att.kind, att.name
            ));
            continue;
        }
        let Some(rank) = delete_rank(&att.kind) else {
            notes.push(format!(
                "{} {} is a child resource — deleted together with {}",
                att.kind, att.name, node.name
            ));
            continue;
        };
        let Some(id) = att.id.as_deref() else {
            notes.push(format!(
                "{} {} has no recorded resource id — press ⟳ Refresh, or delete it manually",
                att.kind, att.name
            ));
            continue;
        };
        if seen.insert(id.to_string()) {
            ranked.push((
                rank,
                DeleteStep {
                    label: format!("{} {}", att.kind, att.name),
                    command: az_delete(id),
                },
            ));
        }
    }
    // Edge-connected dependents pointing AT this resource.
    for edge in &ctx.topology.edges {
        if edge.target != node.id {
            continue;
        }
        let label = edge.label.as_deref().unwrap_or("");
        let Some(&src) = ctx.by_id.get(edge.source.as_str()) else {
            continue;
        };
        let dep = &ctx.topology.nodes[src];
        if let Some(&(_, rank)) = PULL_IN.iter().find(|(l, _)| *l == label) {
            let degree = ctx.degree.get(dep.id.as_str()).copied().unwrap_or(0);
            if degree == 1 {
                if seen.insert(dep.id.clone()) {
                    ranked.push((
                        rank,
                        DeleteStep {
                            label: format!("{} {}", dep.kind_label, dep.name),
                            command: az_delete(&dep.id),
                        },
                    ));
                }
            } else {
                notes.push(format!(
                    "{} {} also connects to {} other resource(s) — left in place",
                    dep.kind_label,
                    dep.name,
                    degree.saturating_sub(1)
                ));
            }
        } else if label == "backs up" {
            notes.push(format!(
                "{} {} backs up this resource — disable its protection before deleting",
                dep.kind_label, dep.name
            ));
        }
    }
    // Stable sort: same-rank entries keep their card order.
    ranked.sort_by_key(|(rank, _)| *rank);
    steps.extend(ranked.into_iter().map(|(_, step)| step));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Association,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub provider: String,
    pub scope_id: String,
    pub scope_label: String,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
    /// Billing currency (ISO code, e.g. "USD") every node/attachment cost is
    /// denominated in. One per topology — a subscription bills in a single
    /// currency. `None` when no cost data was available.
    #[serde(default)]
    pub currency: Option<String>,
    /// Non-fatal problems encountered during the fetch.
    pub warnings: Vec<String>,
}

/// Billing window the per-resource cost query covers — what the card badges
/// show. Selected in the toolbar; a topology's costs always reflect the
/// period it was fetched with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostPeriod {
    /// The current billing month so far (the 1st through today).
    MonthToDate,
    /// A whole past calendar month (`month` is 1-based).
    Month { year: i32, month: u32 },
}

impl CostPeriod {
    pub fn label(self) -> String {
        match self {
            Self::MonthToDate => "This month (to date)".into(),
            Self::Month { year, month } => format!("{} {year}", month_name(month)),
        }
    }

    /// Cache-key suffix so each period caches separately. Month-to-date is
    /// `None` — the plain key — keeping existing cache files valid.
    pub fn cache_suffix(self) -> Option<String> {
        match self {
            Self::MonthToDate => None,
            Self::Month { year, month } => Some(format!("{year:04}-{month:02}")),
        }
    }
}

pub fn month_name(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    NAMES[(month.clamp(1, 12) - 1) as usize]
}

/// Days in a calendar month, leap-aware.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if leap {
                29
            } else {
                28
            }
        }
    }
}

/// (year, month, day) in UTC from unix seconds — Howard Hinnant's civil
/// calendar algorithm, so we don't need a date-time dependency.
pub fn civil_from_unix(secs: u64) -> (i32, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y as i32, m, d)
}

/// The calendar month `k` steps before (year, month).
pub fn month_minus(year: i32, month: u32, k: u32) -> (i32, u32) {
    let idx = i64::from(year) * 12 + i64::from(month) - 1 - i64::from(k);
    (idx.div_euclid(12) as i32, (idx.rem_euclid(12) + 1) as u32)
}

/// Renders a cost amount for display: known currencies get their symbol
/// (`$12.34`), anything else the ISO code (`12.34 SEK`); cents drop once the
/// amount reaches four digits so card badges stay short.
pub fn format_cost(amount: f64, currency: Option<&str>) -> String {
    let value = if amount.abs() >= 1000.0 {
        format!("{amount:.0}")
    } else {
        format!("{amount:.2}")
    };
    match currency {
        Some("USD") => format!("${value}"),
        Some("EUR") => format!("€{value}"),
        Some("GBP") => format!("£{value}"),
        Some(code) => format!("{value} {code}"),
        None => value,
    }
}

/// Presentation post-pass shared by all providers: gather detached leaves of
/// well-known kinds into dashed "Detached …" container boxes (styled like a
/// virtual network) so they don't scatter across the canvas. Two families
/// qualify: kinds whose attached instances fold into owner cards (so a
/// remaining top-level one is genuinely unused — an orphaned disk, an unused
/// SSH key, an unassociated public IP), and regional/management resources
/// that are never topologically connected (network watchers, VM restore
/// point collections).
pub fn group_detached(topology: &mut Topology) {
    // (member kind_label, box name, category, box header label). "Detached"
    // reads as orphaned/cleanup-candidate; regional services get "Regional".
    const GROUPS: &[(&str, &str, ResourceCategory, &str)] = &[
        (
            "SSH public key",
            "SSH public keys",
            ResourceCategory::Security,
            "Detached",
        ),
        (
            "Managed disk",
            "Managed disks",
            ResourceCategory::Compute,
            "Detached",
        ),
        (
            "Public IP address",
            "Public IP addresses",
            ResourceCategory::Network,
            "Detached",
        ),
        (
            "Network Watcher",
            "Network Watchers",
            ResourceCategory::Network,
            "Regional",
        ),
        (
            "Restore point collection",
            "Restore point collections",
            ResourceCategory::Compute,
            "Detached",
        ),
        // Registries serve pushes/pulls from outside the topology, so they
        // always read as detached — box them rather than scatter them.
        (
            "Container registry",
            "Container registries",
            ResourceCategory::Containers,
            "Detached",
        ),
    ];
    for (kind_label, plural, category, box_label) in GROUPS {
        let members: Vec<usize> = topology
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent_id.is_none() && !n.container && n.kind_label == *kind_label)
            .map(|(i, _)| i)
            .collect();
        let id = format!(
            "cloudviz:group:{}",
            kind_label.to_lowercase().replace(' ', "-")
        );
        gather_into_box(topology, members, id, plural, *category, box_label);
    }

    // Monitoring/management debris — alert rules, action groups, dashboards,
    // data collection rules, solutions, App Insights, Log Analytics — is
    // never topologically connected; one shared box keeps it off the canvas
    // floor. Matched by ARM type prefix (lowercased: real listings drift
    // between `microsoft.insights` and `Microsoft.Insights`).
    const MONITORING_TYPE_PREFIXES: &[&str] = &[
        "microsoft.insights/",
        "microsoft.alertsmanagement/",
        "microsoft.portal/",
        "microsoft.operationsmanagement/",
        "microsoft.operationalinsights/",
    ];
    let members: Vec<usize> = topology
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            n.parent_id.is_none() && !n.container && {
                let ty = n.kind.to_lowercase();
                MONITORING_TYPE_PREFIXES.iter().any(|p| ty.starts_with(p))
            }
        })
        .map(|(i, _)| i)
        .collect();
    gather_into_box(
        topology,
        members,
        "cloudviz:group:monitoring".into(),
        "Monitoring",
        ResourceCategory::Other,
        "Management",
    );
}

/// Parent the given nodes under a new synthetic dashed box (no-op when there
/// are no members).
fn gather_into_box(
    topology: &mut Topology,
    members: Vec<usize>,
    id: String,
    name: &str,
    category: ResourceCategory,
    box_label: &str,
) {
    if members.is_empty() {
        return;
    }
    for &i in &members {
        topology.nodes[i].parent_id = Some(id.clone());
    }
    topology.nodes.push(TopologyNode {
        id,
        name: name.to_string(),
        kind: "cloudviz/detachedGroup".into(),
        kind_label: box_label.to_string(),
        category,
        parent_id: None,
        container: true,
        group: None,
        attachments: Vec::new(),
        cost: None,
        region: None,
        metadata: Vec::new(),
    });
}

/// Containment lookup: parent node id → indices of its direct children.
/// Only parents that exist as nodes appear as keys.
pub fn children_index(topology: &Topology) -> HashMap<&str, Vec<usize>> {
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, node) in topology.nodes.iter().enumerate() {
        if let Some(parent) = node.parent_id.as_deref() {
            children.entry(parent).or_default().push(i);
        }
    }
    children
}

/// Per-node subtree cost: the node's own [`TopologyNode::total_cost`] plus
/// every descendant's — what a container box's badge shows (an App Service
/// plan with its sites, a vnet with everything inside). A shared attachment
/// (one SSH key folded into several VMs) counts once per subtree, deduped by
/// its resource id; shared attachments without an id can't be deduped and
/// count per occurrence. `None` where nothing in the subtree has cost data.
pub fn subtree_costs(topology: &Topology) -> Vec<Option<f64>> {
    let children = children_index(topology);
    (0..topology.nodes.len())
        .map(|root| {
            // Explicit DFS with a visited set so malformed parent chains
            // (cycles) terminate instead of hanging.
            let mut visited = vec![false; topology.nodes.len()];
            let mut stack = vec![root];
            let mut shared_seen: HashSet<&str> = HashSet::new();
            let mut sum = 0.0;
            let mut seen_any = false;
            while let Some(i) = stack.pop() {
                if std::mem::replace(&mut visited[i], true) {
                    continue;
                }
                let node = &topology.nodes[i];
                if let Some(cost) = node.cost {
                    sum += cost;
                    seen_any = true;
                }
                for att in &node.attachments {
                    let Some(cost) = att.cost else { continue };
                    if att.shared {
                        if let Some(id) = att.id.as_deref() {
                            if !shared_seen.insert(id) {
                                continue;
                            }
                        }
                    }
                    sum += cost;
                    seen_any = true;
                }
                if let Some(kids) = children.get(node.id.as_str()) {
                    stack.extend(kids.iter().copied());
                }
            }
            seen_any.then_some(sum)
        })
        .collect()
}

/// A selectable fetch scope within a provider — an Azure subscription today,
/// an AWS account/region pair later.
#[derive(Debug, Clone)]
pub struct ScopeOption {
    pub id: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorCode {
    CliMissing,
    NotAuthenticated,
    CommandFailed,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ProviderError {
    pub code: ProviderErrorCode,
    pub message: String,
    /// Actionable next step to show the user, e.g. `az login`.
    pub hint: Option<String>,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProviderError {}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    /// `true` when the provider serves bundled sample data.
    pub demo: bool,
}

#[derive(Debug, Clone)]
pub enum ProviderStatus {
    Ok {
        /// The identity/scope the CLI is signed in with, for the toolbar.
        account_label: String,
        detail: Option<String>,
    },
    Failed(ProviderError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, kind_label: &str) -> TopologyNode {
        TopologyNode {
            id: format!("test:{name}"),
            name: name.into(),
            kind: format!("Test/{kind_label}"),
            kind_label: kind_label.into(),
            category: ResourceCategory::Other,
            parent_id: None,
            container: false,
            group: None,
            attachments: Vec::new(),
            cost: None,
            region: None,
            metadata: Vec::new(),
        }
    }

    #[test]
    fn group_detached_boxes_known_kinds_and_leaves_the_rest() {
        let mut t = Topology {
            provider: "test".into(),
            scope_id: "test".into(),
            scope_label: "test".into(),
            nodes: vec![
                leaf("key-a", "SSH public key"),
                leaf("key-b", "SSH public key"),
                leaf("disk-a", "Managed disk"),
                leaf("vm-a", "Virtual machine"),
            ],
            edges: Vec::new(),
            currency: None,
            warnings: Vec::new(),
        };
        group_detached(&mut t);

        // Two containers appear (keys + disks), none for public IPs or VMs.
        let groups: Vec<&TopologyNode> = t
            .nodes
            .iter()
            .filter(|n| n.kind == "cloudviz/detachedGroup")
            .collect();
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.container));

        let by_name = |name: &str| t.nodes.iter().find(|n| n.name == name).unwrap();
        let key_group = by_name("SSH public keys");
        assert_eq!(key_group.category, ResourceCategory::Security);
        for key in ["key-a", "key-b"] {
            assert_eq!(
                by_name(key).parent_id.as_deref(),
                Some(key_group.id.as_str())
            );
        }
        let disk_group = by_name("Managed disks");
        assert_eq!(
            by_name("disk-a").parent_id.as_deref(),
            Some(disk_group.id.as_str())
        );
        // Unrelated leaves stay top-level.
        assert_eq!(by_name("vm-a").parent_id, None);
    }

    #[test]
    fn total_cost_sums_own_and_attachment_costs() {
        let mut vm = leaf("vm-a", "Virtual machine");
        assert_eq!(vm.total_cost(), None);

        vm.attachments = vec![
            Attachment {
                kind: "disk".into(),
                name: "data".into(),
                id: None,
                shared: false,
                cost: Some(3.25),
            },
            Attachment {
                kind: "nic".into(),
                name: "nic-a".into(),
                id: None,
                shared: false,
                cost: None,
            },
        ];
        // Attachment cost alone counts even when the owner has none…
        assert_eq!(vm.total_cost(), Some(3.25));
        // …and adds onto the owner's own cost when both are known.
        vm.cost = Some(40.0);
        assert_eq!(vm.total_cost(), Some(43.25));
    }

    #[test]
    fn subtree_costs_roll_up_children_and_dedupe_shared_attachments() {
        let att = |kind: &str, id: &str, shared: bool, cost: f64| Attachment {
            kind: kind.into(),
            name: kind.into(),
            id: Some(id.into()),
            shared,
            cost: Some(cost),
        };
        let mut vnet = leaf("vnet-a", "Virtual network");
        vnet.container = true;
        let mut subnet = leaf("snet-a", "Subnet");
        subnet.container = true;
        subnet.parent_id = Some("test:vnet-a".into());
        let mut vm1 = leaf("vm-1", "Virtual machine");
        vm1.parent_id = Some("test:snet-a".into());
        vm1.cost = Some(10.0);
        vm1.attachments = vec![
            att("disk", "id-disk", false, 2.5),
            att("ssh key", "id-key", true, 1.25),
        ];
        let mut vm2 = leaf("vm-2", "Virtual machine");
        vm2.parent_id = Some("test:snet-a".into());
        vm2.cost = Some(20.0);
        // The same shared key folded into both VMs — counts once per subtree.
        vm2.attachments = vec![att("ssh key", "id-key", true, 1.25)];
        let costless = leaf("kv-a", "Key vault");

        let t = Topology {
            provider: "test".into(),
            scope_id: "test".into(),
            scope_label: "test".into(),
            nodes: vec![vnet, subnet, vm1, vm2, costless],
            edges: Vec::new(),
            currency: None,
            warnings: Vec::new(),
        };
        let costs = subtree_costs(&t);
        // Leaves match total_cost(); the shared key still counts per card.
        assert_eq!(costs[2], t.nodes[2].total_cost());
        assert_eq!(costs[2], Some(13.75));
        assert_eq!(costs[3], Some(21.25));
        // Containers roll up children with the shared key counted once:
        // 10 + 2.5 + 20 + 1.25.
        assert_eq!(costs[1], Some(33.75));
        assert_eq!(costs[0], Some(33.75));
        // Nothing costed in the subtree — no badge, not zero.
        assert_eq!(costs[4], None);
    }

    #[test]
    fn subtree_costs_terminate_on_parent_cycles() {
        let mut a = leaf("a", "Thing");
        a.parent_id = Some("test:b".into());
        a.cost = Some(1.0);
        let mut b = leaf("b", "Thing");
        b.parent_id = Some("test:a".into());
        b.cost = Some(2.0);
        let t = Topology {
            provider: "test".into(),
            scope_id: "test".into(),
            scope_label: "test".into(),
            nodes: vec![a, b],
            edges: Vec::new(),
            currency: None,
            warnings: Vec::new(),
        };
        // A malformed mutual-parent chain must terminate, each subtree seeing
        // both nodes exactly once.
        assert_eq!(subtree_costs(&t), vec![Some(3.0), Some(3.0)]);
    }

    #[test]
    fn delete_plan_orders_dependents_before_dependencies() {
        let att = |kind: &str, name: &str, shared: bool, id: Option<&str>| Attachment {
            kind: kind.into(),
            name: name.into(),
            id: id.map(String::from),
            shared,
            cost: None,
        };
        let mut vm = leaf("vm-a", "Virtual machine");
        vm.id = "/subscriptions/s/resourcegroups/rg/vm-a".into();
        // Deliberately shuffled: the plan must impose the safe order itself.
        vm.attachments = vec![
            att(
                "ssh key",
                "key-solo",
                false,
                Some("/subscriptions/s/key-solo"),
            ),
            att("disk", "disk-a", false, Some("/subscriptions/s/disk-a")),
            att("extension", "AADLogin", false, Some("/subscriptions/s/ext")),
            att("public ip", "pip-a", false, Some("/subscriptions/s/pip-a")),
            att(
                "ssh key",
                "key-shared",
                true,
                Some("/subscriptions/s/key-shared"),
            ),
            att("nic", "nic-a", false, Some("/subscriptions/s/nic-a")),
            att(
                "restore point",
                "rpc-a",
                false,
                Some("/subscriptions/s/rpc-a"),
            ),
            att("disk", "disk-no-id", false, None),
        ];

        let plan = delete_plan(&t_with(vec![vm], vec![]), 0).expect("a leaf resource gets a plan");
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "restore point rpc-a",
                "Virtual machine vm-a",
                "nic nic-a",
                "public ip pip-a",
                "disk disk-a",
                "ssh key key-solo",
            ]
        );
        assert_eq!(
            plan.steps[1].command,
            "az resource delete --ids \"/subscriptions/s/resourcegroups/rg/vm-a\""
        );
        assert_eq!(
            plan.steps[3].command,
            "az resource delete --ids \"/subscriptions/s/pip-a\""
        );
        // Shared key kept, extension dies with the VM, id-less disk flagged.
        assert_eq!(plan.notes.len(), 3);
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("key-shared") && n.contains("left in place")));
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("AADLogin") && n.contains("child resource")));
        assert!(plan.notes.iter().any(|n| n.contains("disk-no-id")));

        // Non-vnet containers and synthetic nodes have no plan.
        let mut boxed = leaf("Managed disks", "Detached");
        boxed.container = true;
        boxed.id = "/subscriptions/s/whatever".into();
        assert_eq!(delete_plan(&t_with(vec![boxed], vec![]), 0), None);
        // id "test:group" — synthetic, no ARM id.
        assert_eq!(
            delete_plan(&t_with(vec![leaf("group", "Group")], vec![]), 0),
            None
        );
    }

    /// Tiny topology builder: `edges` as (source id, target id, label).
    fn t_with(nodes: Vec<TopologyNode>, edges: Vec<(&str, &str, &str)>) -> Topology {
        let edges = edges
            .iter()
            .enumerate()
            .map(|(i, (source, target, label))| TopologyEdge {
                id: format!("e{i}"),
                source: source.to_string(),
                target: target.to_string(),
                kind: EdgeKind::Association,
                label: Some(label.to_string()),
            })
            .collect();
        Topology {
            provider: "test".into(),
            scope_id: "test".into(),
            scope_label: "test".into(),
            nodes,
            edges,
            currency: None,
            warnings: Vec::new(),
        }
    }

    /// A leaf with a real-looking ARM id.
    fn arm_leaf(name: &str, kind_label: &str) -> TopologyNode {
        let mut n = leaf(name, kind_label);
        n.id = format!("/subscriptions/s/{name}");
        n
    }

    #[test]
    fn delete_plan_pulls_in_single_connection_dependents_only() {
        let t = t_with(
            vec![
                arm_leaf("vm-a", "Virtual machine"),
                arm_leaf("vm-b", "Virtual machine"),
                arm_leaf("nsg-solo", "Network security group"),
                arm_leaf("nsg-shared", "Network security group"),
                arm_leaf("snap-a", "Snapshot"),
                arm_leaf("rsv-a", "Recovery Services vault"),
            ],
            vec![
                (
                    "/subscriptions/s/nsg-solo",
                    "/subscriptions/s/vm-a",
                    "protects",
                ),
                (
                    "/subscriptions/s/nsg-shared",
                    "/subscriptions/s/vm-a",
                    "protects",
                ),
                (
                    "/subscriptions/s/nsg-shared",
                    "/subscriptions/s/vm-b",
                    "protects",
                ),
                (
                    "/subscriptions/s/snap-a",
                    "/subscriptions/s/vm-a",
                    "snapshot of",
                ),
                (
                    "/subscriptions/s/rsv-a",
                    "/subscriptions/s/vm-a",
                    "backs up",
                ),
            ],
        );
        let plan = delete_plan(&t, 0).unwrap();
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        // The snapshot (only connection: vm-a) leads; the solo NSG trails;
        // the shared NSG and the vault never become steps.
        assert_eq!(
            labels,
            vec![
                "Snapshot snap-a",
                "Virtual machine vm-a",
                "Network security group nsg-solo",
            ]
        );
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("nsg-shared") && n.contains("left in place")));
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("rsv-a") && n.contains("disable its protection")));
    }

    #[test]
    fn delete_plan_never_pulls_in_managers() {
        // A disk's inbound "attached" edge comes from its VM — deleting the
        // disk must never drag the VM into the teardown, even at degree 1.
        let t = t_with(
            vec![
                arm_leaf("disk-a", "Managed disk"),
                arm_leaf("vm-a", "Virtual machine"),
            ],
            vec![(
                "/subscriptions/s/vm-a",
                "/subscriptions/s/disk-a",
                "attached",
            )],
        );
        let plan = delete_plan(&t, 0).unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].label, "Managed disk disk-a");
        assert!(plan.notes.is_empty());
    }

    #[test]
    fn container_plans_empty_children_first() {
        let mut vnet = arm_leaf("vnet-a", "Virtual network");
        vnet.kind = "Microsoft.Network/virtualNetworks".into();
        vnet.container = true;
        let mut subnet = arm_leaf("snet-a", "Subnet");
        subnet.kind = "Microsoft.Network/virtualNetworks/subnets".into();
        subnet.container = true;
        subnet.parent_id = Some("/subscriptions/s/vnet-a".into());
        let mut vm = arm_leaf("vm-a", "Virtual machine");
        vm.parent_id = Some("/subscriptions/s/snet-a".into());
        vm.attachments = vec![Attachment {
            kind: "disk".into(),
            name: "data".into(),
            id: Some("/subscriptions/s/disk-a".into()),
            shared: false,
            cost: None,
        }];
        let nsg = arm_leaf("nsg-a", "Network security group");
        let nodes = vec![vnet, subnet, vm, nsg];
        let edges = vec![(
            "/subscriptions/s/nsg-a",
            "/subscriptions/s/vm-a",
            "protects",
        )];

        // VNet plan: empty the subnet's members first (each leaf block in
        // its own order), no subnet step (it dies with the vnet), vnet last.
        let plan = delete_plan(&t_with(nodes.clone(), edges.clone()), 0).unwrap();
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Virtual machine vm-a",
                "disk data",
                "Network security group nsg-a",
                "Virtual network vnet-a",
            ]
        );
        assert!(plan.notes.iter().any(
            |n| n.contains("snet-a") && n.contains("deleted together with the virtual network")
        ));

        // Subnet plan: members first, then the subnet itself.
        let plan = delete_plan(&t_with(nodes, edges), 1).unwrap();
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Virtual machine vm-a",
                "disk data",
                "Network security group nsg-a",
                "Subnet snet-a",
            ]
        );
        assert!(plan
            .notes
            .iter()
            .any(|n| n.contains("removed automatically if the virtual network is deleted")));
    }

    #[test]
    fn container_plans_survive_cycles_and_never_duplicate() {
        // Malformed mutual parents…
        let mut a = arm_leaf("vnet-x", "Virtual network");
        a.kind = "Microsoft.Network/virtualNetworks".into();
        a.container = true;
        a.parent_id = Some("/subscriptions/s/vnet-y".into());
        let mut b = arm_leaf("vnet-y", "Virtual network");
        b.kind = "Microsoft.Network/virtualNetworks".into();
        b.container = true;
        b.parent_id = Some("/subscriptions/s/vnet-x".into());
        // …plus one NSG protecting two VMs in the same box: excluded (its
        // connections are 2) and noted once thanks to note-dedup.
        let mut vm1 = arm_leaf("vm-1", "Virtual machine");
        vm1.parent_id = Some("/subscriptions/s/vnet-x".into());
        let mut vm2 = arm_leaf("vm-2", "Virtual machine");
        vm2.parent_id = Some("/subscriptions/s/vnet-x".into());
        let nsg = arm_leaf("nsg-a", "Network security group");
        let t = t_with(
            vec![a, b, vm1, vm2, nsg],
            vec![
                (
                    "/subscriptions/s/nsg-a",
                    "/subscriptions/s/vm-1",
                    "protects",
                ),
                (
                    "/subscriptions/s/nsg-a",
                    "/subscriptions/s/vm-2",
                    "protects",
                ),
            ],
        );
        let plan = delete_plan(&t, 0).unwrap();
        let labels: Vec<&str> = plan.steps.iter().map(|s| s.label.as_str()).collect();
        // Terminates; every leaf exactly once; the cyclic container child
        // contributes no step; the shared NSG only a note.
        assert_eq!(
            labels,
            vec![
                "Virtual machine vm-1",
                "Virtual machine vm-2",
                "Virtual network vnet-x",
            ]
        );
        assert_eq!(plan.notes.iter().filter(|n| n.contains("nsg-a")).count(), 1);
    }

    #[test]
    fn civil_date_math_is_correct() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1));
        // 2001-09-09T01:46:40Z
        assert_eq!(civil_from_unix(1_000_000_000), (2001, 9, 9));
        // 2020-02-29T12:00:00Z — leap day decodes correctly.
        assert_eq!(civil_from_unix(1_582_977_600), (2020, 2, 29));

        assert_eq!(days_in_month(2024, 2), 29); // leap
        assert_eq!(days_in_month(2100, 2), 28); // century, not leap
        assert_eq!(days_in_month(2000, 2), 29); // 400-year leap
        assert_eq!(days_in_month(2026, 7), 31);
        assert_eq!(days_in_month(2026, 9), 30);

        assert_eq!(month_minus(2026, 7, 1), (2026, 6));
        assert_eq!(month_minus(2026, 1, 1), (2025, 12));
        assert_eq!(month_minus(2026, 3, 15), (2024, 12));

        assert_eq!(
            CostPeriod::Month {
                year: 2026,
                month: 6
            }
            .label(),
            "June 2026"
        );
        assert_eq!(
            CostPeriod::Month {
                year: 2026,
                month: 6
            }
            .cache_suffix()
            .as_deref(),
            Some("2026-06")
        );
        assert_eq!(CostPeriod::MonthToDate.cache_suffix(), None);
    }

    #[test]
    fn format_cost_symbols_codes_and_precision() {
        assert_eq!(format_cost(12.345, Some("USD")), "$12.35");
        assert_eq!(format_cost(0.0, Some("EUR")), "€0.00");
        assert_eq!(format_cost(1234.56, Some("GBP")), "£1235");
        assert_eq!(format_cost(9.9, Some("SEK")), "9.90 SEK");
        assert_eq!(format_cost(5.0, None), "5.00");
    }
}
