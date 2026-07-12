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

    // Subscription root.
    let subscription_id = format!("/subscriptions/{}", inv.account.id).to_lowercase();
    let mut sub_meta = vec![("subscriptionId".to_string(), inv.account.id.clone())];
    if let Some(tenant) = &inv.account.tenant_id {
        sub_meta.push(("tenantId".into(), tenant.clone()));
    }
    if let Some(user) = inv.account.user.as_ref().and_then(|u| u.name.clone()) {
        sub_meta.push(("signedInAs".into(), user));
    }
    add_node(
        &mut nodes,
        &mut index,
        TopologyNode {
            id: subscription_id.clone(),
            name: inv.account.name.clone(),
            kind: "azure/subscription".into(),
            kind_label: "Subscription".into(),
            category: ResourceCategory::Scope,
            parent_id: None,
            container: true,
            region: None,
            metadata: sub_meta,
        },
    );

    // Resource groups. ARM treats RG names case-insensitively → key by lowercase name.
    let mut rg_id_by_name: HashMap<String, String> = HashMap::new();
    for group in &inv.groups {
        let id = norm(&group.id);
        rg_id_by_name.insert(group.name.to_lowercase(), id.clone());
        add_node(
            &mut nodes,
            &mut index,
            TopologyNode {
                id,
                name: group.name.clone(),
                kind: "azure/resourceGroup".into(),
                kind_label: "Resource group".into(),
                category: ResourceCategory::Group,
                parent_id: Some(subscription_id.clone()),
                container: true,
                region: group.location.clone(),
                metadata: tags_metadata(&group.tags),
            },
        );
    }

    // Resources can reference an RG the group listing didn't return — synthesize it.
    let mut ensure_rg =
        |name: &str, nodes: &mut Vec<TopologyNode>, index: &mut HashMap<String, usize>| -> String {
            let key = name.to_lowercase();
            if let Some(id) = rg_id_by_name.get(&key) {
                return id.clone();
            }
            let id = format!("{subscription_id}/resourcegroups/{key}");
            rg_id_by_name.insert(key, id.clone());
            add_node(
                nodes,
                index,
                TopologyNode {
                    id: id.clone(),
                    name: name.to_string(),
                    kind: "azure/resourceGroup".into(),
                    kind_label: "Resource group".into(),
                    category: ResourceCategory::Group,
                    parent_id: Some(subscription_id.clone()),
                    container: true,
                    region: None,
                    metadata: Vec::new(),
                },
            );
            id
        };

    // Flat resource inventory.
    for resource in &inv.resources {
        let id = norm(&resource.id);
        if index.contains_key(&id) {
            continue;
        }
        let parent = match &resource.resource_group {
            Some(rg) => ensure_rg(rg, &mut nodes, &mut index),
            None => subscription_id.clone(),
        };
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
                parent_id: Some(parent),
                container: resource.resource_type.to_lowercase() == VNET_TYPE,
                region: resource.location.clone(),
                metadata,
            },
        );
    }

    // Virtual networks: enrich with address space and hang subnets inside.
    for vnet in &inv.vnets {
        let vnet_id = norm(&vnet.id);
        if !index.contains_key(&vnet_id) {
            let parent = match &vnet.resource_group {
                Some(rg) => ensure_rg(rg, &mut nodes, &mut index),
                None => subscription_id.clone(),
            };
            add_node(
                &mut nodes,
                &mut index,
                TopologyNode {
                    id: vnet_id.clone(),
                    name: vnet.name.clone(),
                    kind: "Microsoft.Network/virtualNetworks".into(),
                    kind_label: "Virtual network".into(),
                    category: ResourceCategory::Network,
                    parent_id: Some(parent),
                    container: true,
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
                    container: false,
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
        if !index.contains_key(&nic_id) {
            continue; // NIC absent from resource listing; skip rather than invent
        }
        if let Some(vm) = nic.virtual_machine.as_ref().and_then(|v| v.id.as_deref()) {
            add_edge(
                norm(vm),
                nic_id.clone(),
                EdgeKind::Association,
                "attached",
                &mut edges,
            );
        }
        for ip_config in &nic.ip_configurations {
            if let Some(subnet) = ip_config.subnet.as_ref().and_then(|s| s.id.as_deref()) {
                add_edge(
                    nic_id.clone(),
                    norm(subnet),
                    EdgeKind::Network,
                    "in subnet",
                    &mut edges,
                );
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

    const SUB: &str = "/subscriptions/00000000-0000-0000-0000-000000000001";

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
    fn creates_subscription_root_with_resource_groups() {
        let t = build();
        let sub = t
            .nodes
            .iter()
            .find(|n| n.kind == "azure/subscription")
            .unwrap();
        assert_eq!(sub.id, SUB);
        assert!(sub.container);

        let mut rgs: Vec<&str> = t
            .nodes
            .iter()
            .filter(|n| n.kind == "azure/resourceGroup")
            .map(|n| n.name.as_str())
            .collect();
        rgs.sort();
        assert_eq!(rgs, ["rg-app", "rg-net", "rg-orphan"]);
        for n in t.nodes.iter().filter(|n| n.kind == "azure/resourceGroup") {
            assert_eq!(n.parent_id.as_deref(), Some(SUB));
        }
    }

    #[test]
    fn matches_resource_group_names_case_insensitively() {
        let t = build();
        // fixture uses resourceGroup "RG-App" while the group listing says "rg-app"
        let vm = find(&t, "vm-web-01");
        assert_eq!(
            vm.parent_id.as_deref(),
            Some(&format!("{SUB}/resourcegroups/rg-app")[..])
        );
        assert_eq!(vm.category, ResourceCategory::Compute);
    }

    #[test]
    fn synthesizes_unlisted_resource_groups() {
        let t = build();
        let stray = find(&t, "ststray");
        let parent_id = stray.parent_id.clone().unwrap();
        let rg = t.nodes.iter().find(|n| n.id == parent_id).unwrap();
        assert_eq!(rg.kind, "azure/resourceGroup");
        assert_eq!(rg.parent_id.as_deref(), Some(SUB));
    }

    #[test]
    fn nests_subnets_inside_container_vnets() {
        let t = build();
        let vnet = find(&t, "vnet-hub");
        assert!(vnet.container);
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

        let in_subnet = with_label("in subnet");
        assert_eq!(in_subnet.len(), 1);
        assert!(in_subnet[0].target.contains("subnets/snet-a"));
        assert_eq!(in_subnet[0].kind, EdgeKind::Network);

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
