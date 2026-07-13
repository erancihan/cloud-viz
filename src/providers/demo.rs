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

    // Network members live inside their subnet.
    let member = |snet: &str,
                  name: &'static str,
                  kind: &'static str,
                  kind_label: &'static str,
                  metadata: Vec<(&'static str, &'static str)>|
     -> Def {
        Def {
            id: id("rg-network", name),
            name,
            kind,
            kind_label,
            category: Network,
            parent: Some(snet.to_string()),
            container: false,
            group: Some("rg-network"),
            metadata,
        }
    };

    defs.extend([
        member(
            &snet_web,
            "nsg-web",
            "Microsoft.Network/networkSecurityGroups",
            "Network security group",
            vec![],
        ),
        Def {
            // NICs belong to rg-app but sit inside the web subnet.
            id: id("rg-app", "nic-web-01"),
            name: "nic-web-01",
            kind: "Microsoft.Network/networkInterfaces",
            kind_label: "Network interface",
            category: Network,
            parent: Some(snet_web.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("privateIp", "10.0.0.4")],
        },
        Def {
            id: id("rg-app", "nic-web-02"),
            name: "nic-web-02",
            kind: "Microsoft.Network/networkInterfaces",
            kind_label: "Network interface",
            category: Network,
            parent: Some(snet_web.clone()),
            container: false,
            group: Some("rg-app"),
            metadata: vec![("privateIp", "10.0.0.5")],
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

    // Free resources — positioned by their dependency edges.
    defs.extend([
        leaf(
            "rg-network",
            "pip-gateway",
            "Microsoft.Network/publicIPAddresses",
            "Public IP address",
            Network,
            vec![("ipAddress", "20.86.14.7")],
        ),
        leaf(
            "rg-network",
            "lb-web",
            "Microsoft.Network/loadBalancers",
            "Load balancer",
            Network,
            vec![],
        ),
        leaf(
            "rg-app",
            "vm-web-01",
            "Microsoft.Compute/virtualMachines",
            "Virtual machine",
            Compute,
            vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")],
        ),
        leaf(
            "rg-app",
            "vm-web-02",
            "Microsoft.Compute/virtualMachines",
            "Virtual machine",
            Compute,
            vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")],
        ),
        leaf(
            "rg-app",
            "disk-web-01-data",
            "Microsoft.Compute/disks",
            "Managed disk",
            Compute,
            vec![("sizeGb", "256"), ("sku", "Premium_LRS")],
        ),
        leaf(
            "rg-app",
            "plan-portal",
            "Microsoft.Web/serverfarms",
            "App Service plan",
            Web,
            vec![("sku", "P1v3")],
        ),
        leaf(
            "rg-app",
            "app-portal",
            "Microsoft.Web/sites",
            "App Service",
            Web,
            vec![("host", "app-portal.azurewebsites.net")],
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

    let nodes: Vec<TopologyNode> = defs
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
            region: Some("westeurope".into()),
            metadata: d
                .metadata
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        })
        .collect();

    // Only dependency edges remain; subnet membership is shown by containment.
    let edge_defs: Vec<(String, String, EdgeKind, &str)> = vec![
        (
            id("rg-app", "vm-web-01"),
            id("rg-app", "nic-web-01"),
            EdgeKind::Association,
            "attached",
        ),
        (
            id("rg-app", "vm-web-02"),
            id("rg-app", "nic-web-02"),
            EdgeKind::Association,
            "attached",
        ),
        // Mirrors Azure's managedBy: a data disk attached to its VM.
        (
            id("rg-app", "vm-web-01"),
            id("rg-app", "disk-web-01-data"),
            EdgeKind::Association,
            "attached",
        ),
        (
            id("rg-network", "lb-web"),
            id("rg-network", "pip-gateway"),
            EdgeKind::Association,
            "frontend",
        ),
        (
            id("rg-app", "app-portal"),
            id("rg-app", "plan-portal"),
            EdgeKind::Association,
            "hosted on",
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
            assert!(
                !containers.contains(e.source.as_str()),
                "edge from container {}",
                e.source
            );
            assert!(
                !containers.contains(e.target.as_str()),
                "edge into container {}",
                e.target
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
    fn network_members_nest_under_their_subnet() {
        let t = demo_topology();
        let by_name = |name: &str| t.nodes.iter().find(|n| n.name == name).unwrap();
        let snet_web = by_name("snet-web");
        assert!(snet_web.container);
        for nic in ["nic-web-01", "nic-web-02", "nsg-web"] {
            assert_eq!(
                by_name(nic).parent_id.as_deref(),
                Some(snet_web.id.as_str())
            );
        }
        // subnets nest inside the vnet
        let vnet = by_name("vnet-hub");
        assert!(vnet.container);
        assert_eq!(snet_web.parent_id.as_deref(), Some(vnet.id.as_str()));
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
