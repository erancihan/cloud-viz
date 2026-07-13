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
    /// own nodes — e.g. a VM's attached managed disks and extensions. Pairs of
    /// (short kind, name), like ("disk", "DataDisk_1"). Summarized on the
    /// card, listed in full in the details panel.
    #[serde(default)]
    pub attachments: Vec<(String, String)>,
    pub region: Option<String>,
    /// Provider-specific extras surfaced in the details panel (tags, sku…).
    /// Kept ordered so the panel is stable between refreshes.
    pub metadata: Vec<(String, String)>,
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
