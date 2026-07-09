//! Bundled sample topology — lets anyone explore the app without a cloud
//! account. Shaped like a typical small Azure estate so it exercises every
//! render path: nested containers (subscription > resource groups > vnet >
//! subnets), every resource category, and cross-container edges.

use crate::model::*;
use crate::providers::CloudProvider;

pub struct DemoProvider;

impl CloudProvider for DemoProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo { id: "demo", display_name: "Demo (sample data)", demo: true }
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

fn rg(name: &str) -> String {
    format!("{SUB}/resourceGroups/{name}")
}

struct Def {
    id: String,
    name: &'static str,
    kind: &'static str,
    kind_label: &'static str,
    category: ResourceCategory,
    parent: Option<String>,
    container: bool,
    metadata: Vec<(&'static str, &'static str)>,
}

fn leaf(
    parent: &str,
    name: &'static str,
    kind: &'static str,
    kind_label: &'static str,
    category: ResourceCategory,
    metadata: Vec<(&'static str, &'static str)>,
) -> Def {
    Def {
        id: format!("{parent}/{name}"),
        name,
        kind,
        kind_label,
        category,
        parent: Some(parent.to_string()),
        container: false,
        metadata,
    }
}

pub fn demo_topology() -> Topology {
    use ResourceCategory::*;

    let vnet = format!("{}/vnet-hub", rg("rg-network"));

    let mut defs: Vec<Def> = vec![
        Def {
            id: SUB.into(),
            name: "Contoso — Production",
            kind: "azure/subscription",
            kind_label: "Subscription",
            category: Scope,
            parent: None,
            container: true,
            metadata: vec![("tenant", "contoso.com")],
        },
        Def {
            id: vnet.clone(),
            name: "vnet-hub",
            kind: "Microsoft.Network/virtualNetworks",
            kind_label: "Virtual network",
            category: Network,
            parent: Some(rg("rg-network")),
            container: true,
            metadata: vec![("addressSpace", "10.0.0.0/16")],
        },
    ];

    for name in ["rg-network", "rg-app", "rg-data", "rg-ops"] {
        defs.push(Def {
            id: rg(name),
            name: match name {
                "rg-network" => "rg-network",
                "rg-app" => "rg-app",
                "rg-data" => "rg-data",
                _ => "rg-ops",
            },
            kind: "azure/resourceGroup",
            kind_label: "Resource group",
            category: Group,
            parent: Some(SUB.into()),
            container: true,
            metadata: vec![("region", "westeurope")],
        });
    }

    for (name, prefix) in [("snet-web", "10.0.0.0/24"), ("snet-app", "10.0.1.0/24"), ("snet-data", "10.0.2.0/24")] {
        defs.push(Def {
            id: format!("{vnet}/{name}"),
            name: match name {
                "snet-web" => "snet-web",
                "snet-app" => "snet-app",
                _ => "snet-data",
            },
            kind: "Microsoft.Network/virtualNetworks/subnets",
            kind_label: "Subnet",
            category: Network,
            parent: Some(vnet.clone()),
            container: false,
            metadata: vec![("addressPrefix", prefix)],
        });
    }

    let rg_network = rg("rg-network");
    let rg_app = rg("rg-app");
    let rg_data = rg("rg-data");
    let rg_ops = rg("rg-ops");

    defs.extend([
        leaf(&rg_network, "nsg-web", "Microsoft.Network/networkSecurityGroups", "Network security group", Network, vec![]),
        leaf(&rg_network, "pip-gateway", "Microsoft.Network/publicIPAddresses", "Public IP address", Network, vec![("ipAddress", "20.86.14.7")]),
        leaf(&rg_network, "lb-web", "Microsoft.Network/loadBalancers", "Load balancer", Network, vec![]),
        leaf(&rg_app, "vm-web-01", "Microsoft.Compute/virtualMachines", "Virtual machine", Compute, vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")]),
        leaf(&rg_app, "vm-web-02", "Microsoft.Compute/virtualMachines", "Virtual machine", Compute, vec![("size", "Standard_D2s_v5"), ("os", "Ubuntu 24.04")]),
        leaf(&rg_app, "nic-web-01", "Microsoft.Network/networkInterfaces", "Network interface", Network, vec![("privateIp", "10.0.0.4")]),
        leaf(&rg_app, "nic-web-02", "Microsoft.Network/networkInterfaces", "Network interface", Network, vec![("privateIp", "10.0.0.5")]),
        leaf(&rg_app, "plan-portal", "Microsoft.Web/serverfarms", "App Service plan", Web, vec![("sku", "P1v3")]),
        leaf(&rg_app, "app-portal", "Microsoft.Web/sites", "App Service", Web, vec![("host", "app-portal.azurewebsites.net")]),
        leaf(&rg_app, "aks-main", "Microsoft.ContainerService/managedClusters", "AKS cluster", Containers, vec![("nodeCount", "3"), ("version", "1.31")]),
        leaf(&rg_app, "acrcontoso", "Microsoft.ContainerRegistry/registries", "Container registry", Containers, vec![]),
        leaf(&rg_data, "sqlsrv-main", "Microsoft.Sql/servers", "SQL server", Database, vec![]),
        leaf(&rg_data, "sqldb-orders", "Microsoft.Sql/servers/databases", "SQL database", Database, vec![("tier", "GP_Gen5_2")]),
        leaf(&rg_data, "cosmos-catalog", "Microsoft.DocumentDB/databaseAccounts", "Cosmos DB account", Database, vec![]),
        leaf(&rg_data, "redis-session", "Microsoft.Cache/redis", "Redis cache", Database, vec![]),
        leaf(&rg_data, "stcontosoprod", "Microsoft.Storage/storageAccounts", "Storage account", Storage, vec![("sku", "Standard_ZRS")]),
        leaf(&rg_ops, "kv-secrets", "Microsoft.KeyVault/vaults", "Key vault", Security, vec![]),
        leaf(&rg_ops, "id-workload", "Microsoft.ManagedIdentity/userAssignedIdentities", "Managed identity", Security, vec![]),
        leaf(&rg_ops, "sb-events", "Microsoft.ServiceBus/namespaces", "Service Bus namespace", Integration, vec![]),
        leaf(&rg_ops, "log-contoso", "Microsoft.OperationalInsights/workspaces", "Log Analytics workspace", Other, vec![]),
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
            region: Some("westeurope".into()).filter(|_| d.category != Scope),
            metadata: d.metadata.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        })
        .collect();

    let edge_defs: Vec<(String, String, EdgeKind, &str)> = vec![
        // VM -> NIC -> subnet chains
        (format!("{rg_app}/vm-web-01"), format!("{rg_app}/nic-web-01"), EdgeKind::Association, "attached"),
        (format!("{rg_app}/vm-web-02"), format!("{rg_app}/nic-web-02"), EdgeKind::Association, "attached"),
        (format!("{rg_app}/nic-web-01"), format!("{vnet}/snet-web"), EdgeKind::Network, "in subnet"),
        (format!("{rg_app}/nic-web-02"), format!("{vnet}/snet-web"), EdgeKind::Network, "in subnet"),
        // network plumbing
        (format!("{rg_network}/nsg-web"), format!("{vnet}/snet-web"), EdgeKind::Network, "protects"),
        (format!("{rg_network}/lb-web"), format!("{rg_network}/pip-gateway"), EdgeKind::Association, "frontend"),
        (format!("{rg_app}/aks-main"), format!("{vnet}/snet-app"), EdgeKind::Network, "in subnet"),
        // app-tier associations
        (format!("{rg_app}/app-portal"), format!("{rg_app}/plan-portal"), EdgeKind::Association, "hosted on"),
        (format!("{rg_app}/aks-main"), format!("{rg_app}/acrcontoso"), EdgeKind::Association, "pulls from"),
        (format!("{rg_data}/sqldb-orders"), format!("{rg_data}/sqlsrv-main"), EdgeKind::Association, "on server"),
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

        let containers: HashSet<&str> =
            t.nodes.iter().filter(|n| n.container).map(|n| n.id.as_str()).collect();
        for e in &t.edges {
            assert!(ids.contains(e.source.as_str()), "missing source {}", e.source);
            assert!(ids.contains(e.target.as_str()), "missing target {}", e.target);
            assert!(!containers.contains(e.source.as_str()), "edge from container {}", e.source);
            assert!(!containers.contains(e.target.as_str()), "edge into container {}", e.target);
        }
    }

    #[test]
    fn demo_covers_every_category() {
        use ResourceCategory::*;
        let t = demo_topology();
        let present: HashSet<ResourceCategory> = t.nodes.iter().map(|n| n.category).collect();
        for cat in [Compute, Network, Storage, Database, Containers, Security, Integration, Web, Other, Scope, Group] {
            assert!(present.contains(&cat), "demo lacks category {cat:?}");
        }
    }
}
