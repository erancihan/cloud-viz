//! Bundled sample topology — lets anyone explore the app without a cloud
//! account. Shaped like a typical small Azure estate so it exercises every
//! render path: the vnet ▸ subnet ▸ members nesting, resources grouped by
//! dependency, resource-group names as card subtext, and every category and
//! edge kind.

use crate::model::*;
use crate::providers::CloudProvider;

pub struct DemoProvider;

impl CloudProvider for DemoProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "demo",
            display_name: "Demo (sample data)",
            demo: true,
        }
    }

    fn check_status(&self) -> ProviderStatus {
        ProviderStatus::Ok {
            account_label: "Contoso — Production".into(),
            detail: Some("sample data".into()),
        }
    }

    fn list_scopes(&self) -> Result<Vec<ScopeOption>, ProviderError> {
        Ok(vec![ScopeOption {
            id: "demo".into(),
            label: "Contoso — Production (sample)".into(),
            is_default: true,
        }])
    }

    fn fetch_topology(&self, _scope_id: Option<&str>) -> Result<Topology, ProviderError> {
        Ok(demo_topology())
    }
}

const SUB: &str = "demo:/subscriptions/contoso-prod";

/// Stable, unique id namespace per resource group. Resource groups no longer
/// render as boxes, but keeping ids under their RG path keeps them unique.
fn id(rg: &str, name: &str) -> String {
    format!("{SUB}/resourcegroups/{rg}/{name}")
}

struct Def {
    id: String,
    name: &'static str,
    kind: &'static str,
    kind_label: &'static str,
    category: ResourceCategory,
    /// Containment parent: a subnet for network members, a vnet for subnets,
    /// otherwise `None` (a free, dependency-placed resource).
    parent: Option<String>,
    container: bool,
    /// Resource group name shown as card subtext.
    group: Option<&'static str>,
    metadata: Vec<(&'static str, &'static str)>,
}

/// A free resource card, placed by its dependencies (no containment parent).
fn leaf(
    rg: &'static str,
    name: &'static str,
    kind: &'static str,
    kind_label: &'static str,
    category: ResourceCategory,
    metadata: Vec<(&'static str, &'static str)>,
) -> Def {
    Def {
        id: id(rg, name),
        name,
        kind,
        kind_label,
        category,
        parent: None,
        container: false,
        group: Some(rg),
        metadata,
    }
}

pub fn demo_topology() -> Topology {
    use ResourceCategory::*;

    let vnet = id("rg-network", "vnet-hub");
    let snet_web = format!("{vnet}/snet-web");
    let snet_app = format!("{vnet}/snet-app");
    let snet_data = format!("{vnet}/snet-data");

    let mut defs: Vec<Def> = vec![
        // Virtual network — a top-level container holding its subnets.
        Def {
            id: vnet.clone(),
            name: "vnet-hub",
            kind: "Microsoft.Network/virtualNetworks",
            kind_label: "Virtual network",
            category: Network,
            parent: None,
            container: true,
            group: Some("rg-network"),
            metadata: vec![("addressSpace", "10.0.0.0/16")],
        },
    ];

    // Subnets — containers nested in the vnet, holding their members.
    for (id, name, prefix) in [
        (&snet_web, "snet-web", "10.0.0.0/24"),
        (&snet_app, "snet-app", "10.0.1.0/24"),
        (&snet_data, "snet-data", "10.0.2.0/24"),
    ] {
        defs.push(Def {
            id: id.clone(),
            name,
            kind: "Microsoft.Network/virtualNetworks/subnets",
            kind_label: "Subnet",
            category: Network,
            parent: Some(vnet.clone()),
            container: true,
            group: None,
            metadata: vec![("addressPrefix", prefix)],
        });
    }

    // Subnet members: VMs render inside the subnet their NIC lives in (the
    // NIC itself rides on the VM's card, mirroring the Azure mapper), and
    // AKS sits in its delegated subnet.
    defs.extend([
        Def {
            id: id("rg-app", "vm-web-01"),
            name: "vm-web-01",
            kind: "Microsoft.Compute/virtualMachines",
            kind_label: "Virtual machine",
            category: Compute,
            parent: Some(snet_web.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")],
        },
        Def {
            id: id("rg-app", "vm-web-02"),
            name: "vm-web-02",
            kind: "Microsoft.Compute/virtualMachines",
            kind_label: "Virtual machine",
            category: Compute,
            parent: Some(snet_web.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")],
        },
        Def {
            id: id("rg-app", "aks-main"),
            name: "aks-main",
            kind: "Microsoft.ContainerService/managedClusters",
            kind_label: "AKS cluster",
            category: Containers,
            parent: Some(snet_app.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("nodeCount", "3"), ("version", "1.31")],
        },
    ]);

    // App Service plan hosts its App Service, like the Azure mapper builds
    // from `az webapp list`.
    let plan = id("rg-app", "plan-portal");
    defs.extend([
        Def {
            id: plan.clone(),
            name: "plan-portal",
            kind: "Microsoft.Web/serverfarms",
            kind_label: "App Service plan",
            category: Web,
            parent: None,
            container: true,
            group: Some("rg-app"),
            metadata: vec![("sku", "P1v3")],
        },
        Def {
            id: id("rg-app", "app-portal"),
            name: "app-portal",
            kind: "Microsoft.Web/sites",
            kind_label: "App Service",
            category: Web,
            parent: Some(plan.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("host", "app-portal.azurewebsites.net")],
        },
    ]);

    // Free resources — positioned by their dependency edges.
    defs.extend([
        leaf(
            "rg-network",
            "lb-web",
            "Microsoft.Network/loadBalancers",
            "Load balancer",
            Network,
            vec![],
        ),
        leaf(
            "rg-network",
            "nsg-web",
            "Microsoft.Network/networkSecurityGroups",
            "Network security group",
            Network,
            vec![],
        ),
        leaf(
            "rg-app",
            "acrcontoso",
            "Microsoft.ContainerRegistry/registries",
            "Container registry",
            Containers,
            vec![],
        ),
        leaf(
            "rg-data",
            "sqlsrv-main",
            "Microsoft.Sql/servers",
            "SQL server",
            Database,
            vec![],
        ),
        leaf(
            "rg-data",
            "sqldb-orders",
            "Microsoft.Sql/servers/databases",
            "SQL database",
            Database,
            vec![("tier", "GP_Gen5_2")],
        ),
        leaf(
            "rg-data",
            "cosmos-catalog",
            "Microsoft.DocumentDB/databaseAccounts",
            "Cosmos DB account",
            Database,
            vec![],
        ),
        leaf(
            "rg-data",
            "redis-session",
            "Microsoft.Cache/redis",
            "Redis cache",
            Database,
            vec![],
        ),
        leaf(
            "rg-data",
            "stcontosoprod",
            "Microsoft.Storage/storageAccounts",
            "Storage account",
            Storage,
            vec![("sku", "Standard_ZRS")],
        ),
        leaf(
            "rg-ops",
            "kv-secrets",
            "Microsoft.KeyVault/vaults",
            "Key vault",
            Security,
            vec![],
        ),
        // An SSH key no VM uses — only unattached keys stand alone.
        leaf(
            "rg-ops",
            "key-legacy",
            "Microsoft.Compute/sshPublicKeys",
            "SSH public key",
            Security,
            vec![],
        ),
        leaf(
            "rg-ops",
            "id-workload",
            "Microsoft.ManagedIdentity/userAssignedIdentities",
            "Managed identity",
            Security,
            vec![],
        ),
        leaf(
            "rg-ops",
            "sb-events",
            "Microsoft.ServiceBus/namespaces",
            "Service Bus namespace",
            Integration,
            vec![],
        ),
        leaf(
            "rg-ops",
            "log-contoso",
            "Microsoft.OperationalInsights/workspaces",
            "Log Analytics workspace",
            Other,
            vec![],
        ),
    ]);

    let mut nodes: Vec<TopologyNode> = defs
        .into_iter()
        .map(|d| TopologyNode {
            id: d.id,
            name: d.name.to_string(),
            kind: d.kind.to_string(),
            kind_label: d.kind_label.to_string(),
            category: d.category,
            parent_id: d.parent,
            container: d.container,
            group: d.group.map(str::to_string),
            attachments: Vec::new(),
            region: Some("westeurope".into()),
            metadata: d
                .metadata
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        })
        .collect();

    // Folded-in subsidiaries, as the Azure mapper produces them: disks,
    // extensions, NICs, and SSH keys ride on their VM's card; slots on their
    // site's. `ssh-admin` is used by both VMs, so it carries the shared/link
    // badge and has no standalone card.
    let mut attach = |name: &str, items: &[(&str, &str, bool)]| {
        if let Some(node) = nodes.iter_mut().find(|n| n.name == name) {
            node.attachments = items
                .iter()
                .map(|&(kind, name, shared)| Attachment {
                    kind: kind.to_string(),
                    name: name.to_string(),
                    shared,
                })
                .collect();
        }
    };
    attach(
        "vm-web-01",
        &[
            ("disk", "disk-web-01-data", false),
            ("extension", "AADSSHLoginForLinux", false),
            ("nic", "nic-web-01", false),
            ("ssh key", "ssh-admin", true),
        ],
    );
    attach(
        "vm-web-02",
        &[("nic", "nic-web-02", false), ("ssh key", "ssh-admin", true)],
    );
    attach("app-portal", &[("slot", "staging", false)]);
    // The load balancer's frontend IP folds in like a VM's public IP would.
    attach("lb-web", &[("public ip", "pip-gateway", false)]);

    // Only dependency edges remain; subnet, vnet, and plan membership is
    // shown by containment, and NICs/disks/extensions/slots by attachments.
    let edge_defs: Vec<(String, String, EdgeKind, &str)> = vec![
        (
            id("rg-network", "nsg-web"),
            snet_web.clone(),
            EdgeKind::Network,
            "protects",
        ),
        (
            id("rg-app", "aks-main"),
            id("rg-app", "acrcontoso"),
            EdgeKind::Association,
            "pulls from",
        ),
        (
            id("rg-data", "sqldb-orders"),
            id("rg-data", "sqlsrv-main"),
            EdgeKind::Association,
            "on server",
        ),
    ];

    let edges = edge_defs
        .into_iter()
        .enumerate()
        .map(|(i, (source, target, kind, label))| TopologyEdge {
            id: format!("demo-edge-{i}"),
            source,
            target,
            kind,
            label: Some(label.to_string()),
        })
        .collect();

    Topology {
        provider: "demo".into(),
        scope_id: "demo".into(),
        scope_label: "Contoso — Production (sample data)".into(),
        nodes,
        edges,
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn demo_data_is_internally_consistent() {
        let t = demo_topology();
        let ids: HashSet<&str> = t.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), t.nodes.len(), "duplicate node ids");

        for n in &t.nodes {
            if let Some(p) = n.parent_id.as_deref() {
                assert!(ids.contains(p), "missing parent for {}", n.id);
            }
        }

        let containers: HashSet<&str> = t
            .nodes
            .iter()
            .filter(|n| n.container)
            .map(|n| n.id.as_str())
            .collect();
        for e in &t.edges {
            assert!(
                ids.contains(e.source.as_str()),
                "missing source {}",
                e.source
            );
            assert!(
                ids.contains(e.target.as_str()),
                "missing target {}",
                e.target
            );
            // Edges may target containers (NSG -> subnet), but never start
            // from one.
            assert!(
                !containers.contains(e.source.as_str()),
                "edge from container {}",
                e.source
            );
        }
    }

    #[test]
    fn subscription_and_resource_groups_are_not_nodes() {
        let t = demo_topology();
        assert!(
            !t.nodes.iter().any(|n| matches!(
                n.category,
                ResourceCategory::Scope | ResourceCategory::Group
            )),
            "subscription/resource-group containers should no longer be nodes"
        );
        // Resource-group membership survives as card subtext instead.
        let vm = t.nodes.iter().find(|n| n.name == "vm-web-01").unwrap();
        assert_eq!(vm.group.as_deref(), Some("rg-app"));
    }

    #[test]
    fn vms_nest_under_their_subnet_with_folded_attachments() {
        let t = demo_topology();
        let by_name = |name: &str| t.nodes.iter().find(|n| n.name == name).unwrap();
        let snet_web = by_name("snet-web");
        assert!(snet_web.container);
        for vm in ["vm-web-01", "vm-web-02"] {
            assert_eq!(by_name(vm).parent_id.as_deref(), Some(snet_web.id.as_str()));
        }
        // NIC/disk/extension/ssh key ride on the VM card, not as nodes.
        assert!(!t.nodes.iter().any(|n| n.name.starts_with("nic-web")));
        assert_eq!(by_name("vm-web-01").attachments.len(), 4);
        // The shared SSH key is flagged on both VMs and has no node; the
        // unused key keeps its standalone card.
        for vm in ["vm-web-01", "vm-web-02"] {
            let key = by_name(vm)
                .attachments
                .iter()
                .find(|a| a.kind == "ssh key")
                .unwrap();
            assert_eq!(key.name, "ssh-admin");
            assert!(key.shared);
        }
        assert!(!t.nodes.iter().any(|n| n.name == "ssh-admin"));
        assert!(t.nodes.iter().any(|n| n.name == "key-legacy"));
        // subnets nest inside the vnet
        let vnet = by_name("vnet-hub");
        assert!(vnet.container);
        assert_eq!(snet_web.parent_id.as_deref(), Some(vnet.id.as_str()));
    }

    #[test]
    fn app_service_nests_in_its_plan() {
        let t = demo_topology();
        let by_name = |name: &str| t.nodes.iter().find(|n| n.name == name).unwrap();
        let plan = by_name("plan-portal");
        assert!(plan.container);
        assert_eq!(
            by_name("app-portal").parent_id.as_deref(),
            Some(plan.id.as_str())
        );
        assert_eq!(
            by_name("app-portal").attachments,
            vec![Attachment {
                kind: "slot".into(),
                name: "staging".into(),
                shared: false,
            }]
        );
    }

    #[test]
    fn demo_covers_every_leaf_category() {
        use ResourceCategory::*;
        let t = demo_topology();
        let present: HashSet<ResourceCategory> = t.nodes.iter().map(|n| n.category).collect();
        for cat in [
            Compute,
            Network,
            Storage,
            Database,
            Containers,
            Security,
            Integration,
            Web,
            Other,
        ] {
            assert!(present.contains(&cat), "demo lacks category {cat:?}");
        }
    }
}
