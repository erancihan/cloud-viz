//! Provider-agnostic topology model.
//!
//! Every cloud provider (Azure today, AWS/GCP later) normalizes its inventory
//! into these types. The UI knows nothing about any specific cloud — it only
//! understands this graph.

use serde::{Deserialize, Serialize};

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
    pub region: Option<String>,
    /// Provider-specific extras surfaced in the details panel (tags, sku…).
    /// Kept ordered so the panel is stable between refreshes.
    pub metadata: Vec<(String, String)>,
}

/// A subsidiary resource folded into its owner's card (see
/// [`TopologyNode::attachments`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Attachment {
    /// Short lowercase kind driving the row icon: "disk", "nic",
    /// "extension", "slot", "ssh key".
    pub kind: String,
    pub name: String,
    /// True when the same underlying resource is folded into other nodes as
    /// well (e.g. an SSH key used by several VMs) — rendered with a link
    /// badge on the sub-card's corner.
    #[serde(default)]
    pub shared: bool,
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
    /// Non-fatal problems encountered during the fetch.
    pub warnings: Vec<String>,
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
    ];
    for (kind_label, plural, category, box_label) in GROUPS {
        let members: Vec<usize> = topology
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent_id.is_none() && !n.container && n.kind_label == *kind_label)
            .map(|(i, _)| i)
            .collect();
        if members.is_empty() {
            continue;
        }
        let id = format!(
            "cloudviz:group:{}",
            kind_label.to_lowercase().replace(' ', "-")
        );
        for &i in &members {
            topology.nodes[i].parent_id = Some(id.clone());
        }
        topology.nodes.push(TopologyNode {
            id,
            name: (*plural).to_string(),
            kind: "cloudviz/detachedGroup".into(),
            kind_label: (*box_label).to_string(),
            category: *category,
            parent_id: None,
            container: true,
            group: None,
            attachments: Vec::new(),
            region: None,
            metadata: Vec::new(),
        });
    }
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
}
