//! Pure translation from Azure CLI JSON to the provider-agnostic Topology
//! model. No I/O here — everything is unit-testable with fixtures.

use super::az_types::*;
use crate::model::{EdgeKind, ResourceCategory, Topology, TopologyEdge, TopologyNode};
use std::collections::{HashMap, HashSet};

pub struct AzureInventory {
    pub account: AzAccount,
    pub groups: Vec<AzGroup>,
    pub resources: Vec<AzResource>,
    pub vnets: Vec<AzVnet>,
    pub nics: Vec<AzNic>,
    pub warnings: Vec<String>,
}

const VNET_TYPE: &str = "microsoft.network/virtualnetworks";

/// Maps an ARM resource type (e.g. `Microsoft.Compute/virtualMachines`) to a
/// normalized category.
pub fn categorize(resource_type: &str) -> ResourceCategory {
    use ResourceCategory::*;
    const TABLE: &[(&str, ResourceCategory)] = &[
        ("microsoft.compute", Compute),
        ("microsoft.classiccompute", Compute),
        ("microsoft.network", Network),
        ("microsoft.cdn", Network),
        ("microsoft.storage", Storage),
        ("microsoft.storagecache", Storage),
        ("microsoft.sql", Database),
        ("microsoft.dbformysql", Database),
        ("microsoft.dbforpostgresql", Database),
        ("microsoft.dbformariadb", Database),
        ("microsoft.documentdb", Database),
        ("microsoft.cache", Database),
        ("microsoft.containerservice", Containers),
        ("microsoft.containerregistry", Containers),
        ("microsoft.containerinstance", Containers),
        ("microsoft.app", Containers),
        ("microsoft.keyvault", Security),
        ("microsoft.managedidentity", Security),
        ("microsoft.authorization", Security),
        ("microsoft.security", Security),
        ("microsoft.servicebus", Integration),
        ("microsoft.eventhub", Integration),
        ("microsoft.eventgrid", Integration),
        ("microsoft.logic", Integration),
        ("microsoft.apimanagement", Integration),
        ("microsoft.web", Web),
    ];
    let lower = resource_type.to_lowercase();
    TABLE
        .iter()
        .find(|(prefix, _)| lower.starts_with(prefix))
        .map(|(_, cat)| *cat)
        .unwrap_or(Other)
}

/// Human label for an ARM type; falls back to de-camel-casing the last path
/// segment.
pub fn type_label(resource_type: &str) -> String {
    const KNOWN: &[(&str, &str)] = &[
        ("microsoft.compute/virtualmachines", "Virtual machine"),
        ("microsoft.compute/virtualmachinescalesets", "VM scale set"),
        ("microsoft.compute/disks", "Managed disk"),
        ("microsoft.compute/snapshots", "Snapshot"),
        ("microsoft.compute/images", "Image"),
        ("microsoft.network/virtualnetworks", "Virtual network"),
        ("microsoft.network/virtualnetworks/subnets", "Subnet"),
        ("microsoft.network/networkinterfaces", "Network interface"),
        (
            "microsoft.network/networksecuritygroups",
            "Network security group",
        ),
        ("microsoft.network/publicipaddresses", "Public IP address"),
        ("microsoft.network/loadbalancers", "Load balancer"),
        (
            "microsoft.network/applicationgateways",
            "Application gateway",
        ),
        ("microsoft.network/dnszones", "DNS zone"),
        ("microsoft.network/privatednszones", "Private DNS zone"),
        ("microsoft.network/natgateways", "NAT gateway"),
        ("microsoft.network/routetables", "Route table"),
        ("microsoft.network/azurefirewalls", "Azure Firewall"),
        ("microsoft.network/bastionhosts", "Bastion host"),
        ("microsoft.network/privateendpoints", "Private endpoint"),
        ("microsoft.storage/storageaccounts", "Storage account"),
        ("microsoft.sql/servers", "SQL server"),
        ("microsoft.sql/servers/databases", "SQL database"),
        (
            "microsoft.dbforpostgresql/flexibleservers",
            "PostgreSQL server",
        ),
        ("microsoft.dbformysql/flexibleservers", "MySQL server"),
        ("microsoft.documentdb/databaseaccounts", "Cosmos DB account"),
        ("microsoft.cache/redis", "Redis cache"),
        ("microsoft.containerservice/managedclusters", "AKS cluster"),
        (
            "microsoft.containerregistry/registries",
            "Container registry",
        ),
        (
            "microsoft.containerinstance/containergroups",
            "Container instances",
        ),
        ("microsoft.app/containerapps", "Container app"),
        (
            "microsoft.app/managedenvironments",
            "Container Apps environment",
        ),
        ("microsoft.keyvault/vaults", "Key vault"),
        (
            "microsoft.managedidentity/userassignedidentities",
            "Managed identity",
        ),
        ("microsoft.servicebus/namespaces", "Service Bus namespace"),
        ("microsoft.eventhub/namespaces", "Event Hubs namespace"),
        ("microsoft.eventgrid/topics", "Event Grid topic"),
        ("microsoft.apimanagement/service", "API Management"),
        ("microsoft.web/sites", "App Service"),
        ("microsoft.web/serverfarms", "App Service plan"),
        ("microsoft.web/staticsites", "Static web app"),
        (
            "microsoft.operationalinsights/workspaces",
            "Log Analytics workspace",
        ),
        ("microsoft.insights/components", "Application Insights"),
        (
            "microsoft.recoveryservices/vaults",
            "Recovery Services vault",
        ),
    ];
    let lower = resource_type.to_lowercase();
    if let Some((_, label)) = KNOWN.iter().find(|(t, _)| *t == lower) {
        return (*label).to_string();
    }
    let segment = resource_type.rsplit('/').next().unwrap_or(resource_type);
    let mut out = String::with_capacity(segment.len() + 4);
    for (i, ch) in segment.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => out,
    }
}

pub fn build_topology(inv: AzureInventory) -> Topology {
    let mut nodes: Vec<TopologyNode> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<TopologyEdge> = Vec::new();
    let mut edge_ids: HashSet<String> = HashSet::new();

    fn add_node(
        nodes: &mut Vec<TopologyNode>,
        index: &mut HashMap<String, usize>,
        node: TopologyNode,
    ) {
        if !index.contains_key(&node.id) {
            index.insert(node.id.clone(), nodes.len());
            nodes.push(node);
        }
    }

    let norm = |id: &str| id.to_lowercase();

    // Resource-group display names, keyed case-insensitively. Resource groups
    // are no longer container nodes — the name rides along as card subtext.
    let mut rg_display: HashMap<String, String> = HashMap::new();
    for group in &inv.groups {
        rg_display.insert(group.name.to_lowercase(), group.name.clone());
    }
    let group_label = |rg: &Option<String>| -> Option<String> {
        rg.as_ref().map(|name| {
            rg_display
                .get(&name.to_lowercase())
                .cloned()
                .unwrap_or_else(|| name.clone())
        })
    };

    // Flat resource inventory. Every resource is top-level — the layout places
    // it by its dependencies; only vnet ▸ subnet membership stays as nesting.
    for resource in &inv.resources {
        let id = norm(&resource.id);
        if index.contains_key(&id) {
            continue;
        }
        let mut metadata = Vec::new();
        if let Some(kind) = resource.kind.as_ref().filter(|k| !k.is_empty()) {
            metadata.push(("kind".into(), kind.clone()));
        }
        if let Some(sku) = &resource.sku {
            if !sku.is_null() {
                metadata.push(("sku".into(), compact_json(sku)));
            }
        }
        metadata.extend(tags_metadata(&resource.tags));
        add_node(
            &mut nodes,
            &mut index,
            TopologyNode {
                id,
                name: resource.name.clone(),
                kind: resource.resource_type.clone(),
                kind_label: type_label(&resource.resource_type),
                category: categorize(&resource.resource_type),
                parent_id: None,
                container: resource.resource_type.to_lowercase() == VNET_TYPE,
                group: group_label(&resource.resource_group),
                region: resource.location.clone(),
                metadata,
            },
        );
    }

    // Virtual networks (top-level containers) enriched with address space, each
    // holding its subnets, which in turn hold their member NICs.
    for vnet in &inv.vnets {
        let vnet_id = norm(&vnet.id);
        if !index.contains_key(&vnet_id) {
            add_node(
                &mut nodes,
                &mut index,
                TopologyNode {
                    id: vnet_id.clone(),
                    name: vnet.name.clone(),
                    kind: "Microsoft.Network/virtualNetworks".into(),
                    kind_label: "Virtual network".into(),
                    category: ResourceCategory::Network,
                    parent_id: None,
                    container: true,
                    group: group_label(&vnet.resource_group),
                    region: vnet.location.clone(),
                    metadata: Vec::new(),
                },
            );
        }
        if let Some(&i) = index.get(&vnet_id) {
            nodes[i].container = true;
            if let Some(space) = &vnet.address_space {
                if !space.address_prefixes.is_empty() {
                    nodes[i]
                        .metadata
                        .push(("addressSpace".into(), space.address_prefixes.join(", ")));
                }
            }
        }
        for subnet in &vnet.subnets {
            let prefix = subnet.address_prefix.clone().or_else(|| {
                (!subnet.address_prefixes.is_empty()).then(|| subnet.address_prefixes.join(", "))
            });
            add_node(
                &mut nodes,
                &mut index,
                TopologyNode {
                    id: norm(&subnet.id),
                    name: subnet.name.clone(),
                    kind: "Microsoft.Network/virtualNetworks/subnets".into(),
                    kind_label: "Subnet".into(),
                    category: ResourceCategory::Network,
                    parent_id: Some(vnet_id.clone()),
                    container: true,
                    group: None,
                    region: None,
                    metadata: prefix
                        .map(|p| ("addressPrefix".to_string(), p))
                        .into_iter()
                        .collect(),
                },
            );
        }
    }

    // NICs stitch the graph together: VM -> NIC -> subnet / public IP.
    let mut add_edge = |source: String,
                        target: String,
                        kind: EdgeKind,
                        label: &str,
                        edges: &mut Vec<TopologyEdge>| {
        if source == target || !index.contains_key(&source) || !index.contains_key(&target) {
            return;
        }
        let id = format!("{source}=>{target}:{label}");
        if edge_ids.insert(id.clone()) {
            edges.push(TopologyEdge {
                id,
                source,
                target,
                kind,
                label: Some(label.to_string()),
            });
        }
    };

    for nic in &inv.nics {
        let nic_id = norm(&nic.id);
        let Some(&nic_index) = index.get(&nic_id) else {
            continue; // NIC absent from resource listing; skip rather than invent
        };
        if let Some(vm) = nic.virtual_machine.as_ref().and_then(|v| v.id.as_deref()) {
            add_edge(
                norm(vm),
                nic_id.clone(),
                EdgeKind::Association,
                "attached",
                &mut edges,
            );
        }
        // A NIC lives in its subnet: nest it there instead of drawing an edge.
        // Fall back to leaving it top-level if the subnet wasn't discovered.
        let mut nested = false;
        for ip_config in &nic.ip_configurations {
            if !nested {
                if let Some(subnet) = ip_config.subnet.as_ref().and_then(|s| s.id.as_deref()) {
                    let subnet_id = norm(subnet);
                    if index.contains_key(&subnet_id) {
                        nodes[nic_index].parent_id = Some(subnet_id);
                        nested = true;
                    }
                }
            }
            if let Some(pip) = ip_config
                .public_ip_address
                .as_ref()
                .and_then(|p| p.id.as_deref())
            {
                add_edge(
                    nic_id.clone(),
                    norm(pip),
                    EdgeKind::Association,
                    "public IP",
                    &mut edges,
                );
            }
        }
    }

    Topology {
        provider: "azure".into(),
        scope_id: inv.account.id.clone(),
        scope_label: inv.account.name.clone(),
        nodes,
        edges,
        warnings: inv.warnings,
    }
}

fn tags_metadata(
    tags: &Option<std::collections::BTreeMap<String, String>>,
) -> Vec<(String, String)> {
    tags.iter()
        .flatten()
        .map(|(k, v)| (format!("tag:{k}"), v.clone()))
        .collect()
}

fn compact_json(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build() -> Topology {
        build_topology(AzureInventory {
            account: serde_json::from_str(include_str!("fixtures/account.json")).unwrap(),
            groups: serde_json::from_str(include_str!("fixtures/groups.json")).unwrap(),
            resources: serde_json::from_str(include_str!("fixtures/resources.json")).unwrap(),
            vnets: serde_json::from_str(include_str!("fixtures/vnets.json")).unwrap(),
            nics: serde_json::from_str(include_str!("fixtures/nics.json")).unwrap(),
            warnings: Vec::new(),
        })
    }

    fn find<'a>(t: &'a Topology, name: &str) -> &'a TopologyNode {
        t.nodes.iter().find(|n| n.name == name).unwrap()
    }

    #[test]
    fn no_subscription_or_resource_group_nodes() {
        let t = build();
        assert!(
            !t.nodes.iter().any(|n| matches!(
                n.category,
                ResourceCategory::Scope | ResourceCategory::Group
            )),
            "subscription/resource-group containers should no longer be nodes"
        );
        // Non-network resources sit at the top level (no containment parent).
        let vm = find(&t, "vm-web-01");
        assert_eq!(vm.parent_id, None);
        assert_eq!(vm.group.as_deref(), Some("rg-app"));
    }

    #[test]
    fn resource_group_label_uses_listing_casing() {
        let t = build();
        // fixture resource says "RG-App"; the group listing says "rg-app".
        let vm = find(&t, "vm-web-01");
        assert_eq!(vm.group.as_deref(), Some("rg-app"));
        assert_eq!(vm.category, ResourceCategory::Compute);
    }

    #[test]
    fn resource_group_not_in_listing_still_labels_card() {
        let t = build();
        // ststray is in rg-orphan, which the group listing didn't return.
        let stray = find(&t, "ststray");
        assert_eq!(stray.parent_id, None);
        assert_eq!(stray.group.as_deref(), Some("rg-orphan"));
    }

    #[test]
    fn nests_subnets_inside_container_vnets() {
        let t = build();
        let vnet = find(&t, "vnet-hub");
        assert!(vnet.container);
        assert!(vnet.parent_id.is_none(), "vnet should be top-level now");
        assert!(vnet
            .metadata
            .iter()
            .any(|(k, v)| k == "addressSpace" && v == "10.0.0.0/16"));

        let subnets: Vec<&TopologyNode> = t
            .nodes
            .iter()
            .filter(|n| n.kind_label == "Subnet")
            .collect();
        assert_eq!(subnets.len(), 2);
        for s in &subnets {
            assert_eq!(s.parent_id.as_deref(), Some(vnet.id.as_str()));
            assert!(s.container, "subnets hold their members now");
        }
        assert!(subnets[0]
            .metadata
            .iter()
            .any(|(k, v)| k == "addressPrefix" && v == "10.0.0.0/24"));
    }

    #[test]
    fn derives_vm_nic_subnet_and_public_ip_edges() {
        let t = build();
        let with_label = |label: &str| -> Vec<&TopologyEdge> {
            t.edges
                .iter()
                .filter(|e| e.label.as_deref() == Some(label))
                .collect()
        };

        let attached = with_label("attached");
        assert_eq!(attached.len(), 1);
        assert!(attached[0].source.contains("virtualmachines/vm-web-01"));
        assert!(attached[0].target.contains("networkinterfaces/nic-web-01"));

        // NIC nesting replaces the old "in subnet" edge; the ghost subnet in a
        // non-existent vnet is ignored, so the NIC lands in snet-a.
        assert!(with_label("in subnet").is_empty());
        let nic = find(&t, "nic-web-01");
        let nic_parent = nic.parent_id.as_deref().unwrap();
        assert!(nic_parent.contains("subnets/snet-a"));
        assert!(!nic_parent.contains("snet-missing"));

        let public_ip = with_label("public IP");
        assert_eq!(public_ip.len(), 1);
        assert!(public_ip[0].target.contains("publicipaddresses/pip-web"));
    }

    #[test]
    fn never_points_edges_at_unknown_nodes() {
        let t = build();
        let ids: HashSet<&str> = t.nodes.iter().map(|n| n.id.as_str()).collect();
        for e in &t.edges {
            assert!(
                ids.contains(e.source.as_str()),
                "unknown source {}",
                e.source
            );
            assert!(
                ids.contains(e.target.as_str()),
                "unknown target {}",
                e.target
            );
        }
        // the fixture NIC references a subnet in a vnet that does not exist
        assert!(!t.edges.iter().any(|e| e.target.contains("snet-missing")));
    }

    #[test]
    fn ids_are_lowercase_and_unique() {
        let t = build();
        let mut seen = HashSet::new();
        for n in &t.nodes {
            assert_eq!(n.id, n.id.to_lowercase());
            assert!(seen.insert(n.id.clone()), "duplicate id {}", n.id);
        }
    }

    #[test]
    fn categorize_maps_arm_type_prefixes() {
        use ResourceCategory::*;
        for (ty, expected) in [
            ("Microsoft.Compute/virtualMachines", Compute),
            ("Microsoft.Network/loadBalancers", Network),
            ("Microsoft.Storage/storageAccounts", Storage),
            ("Microsoft.Sql/servers/databases", Database),
            ("Microsoft.DocumentDB/databaseAccounts", Database),
            ("Microsoft.ContainerService/managedClusters", Containers),
            ("Microsoft.KeyVault/vaults", Security),
            ("Microsoft.ServiceBus/namespaces", Integration),
            ("Microsoft.Web/sites", Web),
            ("Some.Unknown/thing", Other),
        ] {
            assert_eq!(categorize(ty), expected, "{ty}");
        }
    }

    #[test]
    fn type_label_curated_and_fallback() {
        assert_eq!(
            type_label("Microsoft.Compute/virtualMachines"),
            "Virtual machine"
        );
        assert_eq!(type_label("microsoft.web/sites"), "App Service");
        assert_eq!(type_label("Vendor.Foo/widgetFactories"), "Widget factories");
    }
}
