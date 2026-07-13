//! Pure translation from Azure CLI JSON to the provider-agnostic Topology
//! model. No I/O here — everything is unit-testable with fixtures.

use super::az_types::*;
use crate::model::{Attachment, EdgeKind, ResourceCategory, Topology, TopologyEdge, TopologyNode};
use std::collections::{HashMap, HashSet};

pub struct AzureInventory {
    pub account: AzAccount,
    pub groups: Vec<AzGroup>,
    pub resources: Vec<AzResource>,
    pub vnets: Vec<AzVnet>,
    pub nics: Vec<AzNic>,
    pub webapps: Vec<AzWebApp>,
    pub vms: Vec<AzVm>,
    pub sshkeys: Vec<AzSshKey>,
    pub aks: Vec<AzAksCluster>,
    pub restore_points: Vec<AzRestorePointCollection>,
    pub warnings: Vec<String>,
}

const VNET_TYPE: &str = "microsoft.network/virtualnetworks";

/// Maps an ARM resource type (e.g. `Microsoft.Compute/virtualMachines`) to a
/// normalized category.
pub fn categorize(resource_type: &str) -> ResourceCategory {
    use ResourceCategory::*;
    const TABLE: &[(&str, ResourceCategory)] = &[
        // Specific types first — the table matches by prefix, in order.
        ("microsoft.compute/sshpublickeys", Security),
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
        ("microsoft.compute/sshpublickeys", "SSH public key"),
        (
            "microsoft.compute/restorepointcollections",
            "Restore point collection",
        ),
        ("microsoft.network/networkwatchers", "Network Watcher"),
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

    // NIC → owning VM, from the nic listing. Used to fold NICs into their
    // VM's card and to re-anchor NIC-derived edges/nesting onto the VM.
    let nic_vm: HashMap<String, String> = inv
        .nics
        .iter()
        .filter_map(|nic| {
            let vm = nic.virtual_machine.as_ref()?.id.as_deref()?;
            Some((norm(&nic.id), norm(vm)))
        })
        .collect();

    // Subsidiary resources fold into their owner's card instead of standing
    // as nodes: attached managed disks (`managedBy` → the VM), NICs wired to
    // a VM, and child resources living under their owner's id (VM extensions,
    // App Service deployment slots). Collected first so the node loop can
    // skip them; applied once every owner exists.
    let resource_ids: HashSet<String> = inv.resources.iter().map(|r| norm(&r.id)).collect();

    // Public IPs referenced by a NIC fold into the NIC's anchor — the VM
    // when the NIC has one, otherwise the NIC itself. Unreferenced public
    // IPs keep their own card.
    let mut pip_anchors: HashMap<String, Vec<String>> = HashMap::new();
    for nic in &inv.nics {
        let nic_id = norm(&nic.id);
        let anchor = match nic_vm.get(&nic_id) {
            Some(vm) if resource_ids.contains(vm) => vm.clone(),
            _ => nic_id.clone(),
        };
        if !resource_ids.contains(&anchor) {
            continue;
        }
        for ip_config in &nic.ip_configurations {
            if let Some(pip) = ip_config
                .public_ip_address
                .as_ref()
                .and_then(|p| p.id.as_deref())
            {
                let anchors = pip_anchors.entry(norm(pip)).or_default();
                if !anchors.contains(&anchor) {
                    anchors.push(anchor.clone());
                }
            }
        }
    }

    // Restore point collection -> its source VM (from the per-RG listing).
    let rpc_source: HashMap<String, String> = inv
        .restore_points
        .iter()
        .filter_map(|rpc| Some((norm(&rpc.id), norm(rpc.source_vm()?))))
        .collect();

    let mut folded: HashMap<String, Vec<Attachment>> = HashMap::new();
    let mut fold_away: HashSet<String> = HashSet::new();
    for resource in &inv.resources {
        let ty = resource.resource_type.to_lowercase();
        let id = norm(&resource.id);
        if ty == "microsoft.network/publicipaddresses" {
            let Some(anchors) = pip_anchors.get(&id) else {
                continue; // unattached public IP keeps its own card
            };
            for owner in anchors {
                folded.entry(owner.clone()).or_default().push(Attachment {
                    kind: "public ip".into(),
                    name: resource.name.clone(),
                    shared: anchors.len() > 1,
                });
            }
            fold_away.insert(id);
            continue;
        }
        let (owner, kind) = if ty == "microsoft.compute/disks" {
            let Some(owner) = resource.managed_by.as_deref().filter(|m| !m.is_empty()) else {
                continue; // unattached disk: keep it visible as its own node
            };
            (norm(owner), "disk")
        } else if ty == "microsoft.network/networkinterfaces" {
            let Some(owner) = nic_vm.get(&id) else {
                continue; // NIC without a VM stays a node (in its subnet)
            };
            (owner.clone(), "nic")
        } else if ty.ends_with("/extensions") || ty.ends_with("/slots") {
            // Child resources, e.g. Microsoft.Compute/virtualMachines/extensions
            // (…/virtualMachines/gitlab/extensions/AADSSHLoginForLinux) or
            // Microsoft.Web/sites/slots (…/sites/app/slots/staging).
            let (marker, kind) = if ty.ends_with("/extensions") {
                ("/extensions/", "extension")
            } else {
                ("/slots/", "slot")
            };
            let Some(pos) = id.rfind(marker) else {
                continue;
            };
            (id[..pos].to_string(), kind)
        } else if ty == "microsoft.compute/restorepointcollections" {
            // A VM backup: fold onto its source VM. If the source is unknown
            // or gone, it stays a node and gathers into the detached box.
            let Some(vm) = rpc_source.get(&id) else {
                continue;
            };
            (vm.clone(), "restore point")
        } else {
            continue;
        };
        if resource_ids.contains(&owner) {
            let name = resource.name.rsplit('/').next().unwrap_or(&resource.name);
            folded.entry(owner).or_default().push(Attachment {
                kind: kind.to_string(),
                name: name.to_string(),
                shared: false,
            });
            fold_away.insert(id);
        }
    }

    // SSH public keys: ARM copies the key material into the VM's osProfile
    // instead of referencing the sshPublicKeys resource, so match `az sshkey
    // list` key text against `az vm list` osProfile keys. A key in use folds
    // into every VM using it (marked shared when that's more than one);
    // only unattached keys remain standalone cards.
    let key_by_material: HashMap<&str, &AzSshKey> = inv
        .sshkeys
        .iter()
        .filter_map(|k| k.public_key.as_deref().map(|m| (m.trim(), k)))
        .collect();
    let mut ssh_users: HashMap<String, Vec<String>> = HashMap::new(); // key id -> VM ids
    for vm in &inv.vms {
        let vm_id = norm(&vm.id);
        if !resource_ids.contains(&vm_id) {
            continue;
        }
        let keys = vm
            .os_profile
            .iter()
            .filter_map(|p| p.linux_configuration.as_ref())
            .filter_map(|l| l.ssh.as_ref())
            .flat_map(|s| &s.public_keys);
        for key in keys {
            let Some(material) = key.key_data.as_deref().map(str::trim) else {
                continue;
            };
            if let Some(ssh_key) = key_by_material.get(material) {
                let users = ssh_users.entry(norm(&ssh_key.id)).or_default();
                if !users.contains(&vm_id) {
                    users.push(vm_id.clone());
                }
            }
        }
    }
    for ssh_key in &inv.sshkeys {
        let key_id = norm(&ssh_key.id);
        let Some(users) = ssh_users.get(&key_id) else {
            continue; // unattached key: keep it visible as its own node
        };
        for vm_id in users {
            folded.entry(vm_id.clone()).or_default().push(Attachment {
                kind: "ssh key".into(),
                name: ssh_key.name.clone(),
                shared: users.len() > 1,
            });
        }
        fold_away.insert(key_id);
    }

    // Flat resource inventory. Every resource is top-level — the layout places
    // it by its dependencies; only vnet ▸ subnet membership stays as nesting.
    for resource in &inv.resources {
        let id = norm(&resource.id);
        if index.contains_key(&id) || fold_away.contains(&id) {
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
                attachments: Vec::new(),
                region: resource.location.clone(),
                metadata,
            },
        );
    }

    // Virtual networks (top-level containers) enriched with address space, each
    // holding its subnets, which in turn hold their member NICs.
    let mut subnet_nsgs: Vec<(String, String)> = Vec::new(); // (nsg id, subnet id)
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
                    attachments: Vec::new(),
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
            if let Some(nsg) = subnet
                .network_security_group
                .as_ref()
                .and_then(|r| r.id.as_deref())
            {
                subnet_nsgs.push((norm(nsg), norm(&subnet.id)));
            }
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
                    attachments: Vec::new(),
                    region: None,
                    metadata: prefix
                        .map(|p| ("addressPrefix".to_string(), p))
                        .into_iter()
                        .collect(),
                },
            );
        }
    }

    // App Services nest inside their App Service plan (`az webapp list`
    // carries appServicePlanId); a hosting plan renders as a container box.
    for app in &inv.webapps {
        let app_id = norm(&app.id);
        let Some(plan) = app.app_service_plan_id.as_deref().filter(|p| !p.is_empty()) else {
            continue;
        };
        let plan_id = norm(plan);
        let (Some(&ai), Some(&pi)) = (index.get(&app_id), index.get(&plan_id)) else {
            continue;
        };
        if nodes[ai].parent_id.is_none() {
            nodes[ai].parent_id = Some(plan_id);
            nodes[pi].container = true;
        }
    }

    // Hand each owner its folded-in subsidiaries, sorted for determinism
    // with secondary items (SSH keys, public IPs) last — they render below
    // a separator on the card.
    for (owner, mut items) in folded {
        if let Some(&i) = index.get(&owner) {
            items.sort_by(|a, b| {
                a.secondary()
                    .cmp(&b.secondary())
                    .then_with(|| a.kind.cmp(&b.kind))
                    .then_with(|| a.name.cmp(&b.name))
            });
            nodes[i].attachments.extend(items);
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

    // `managedBy` is ARM's generic ownership pointer: an attached managed
    // disk points at its VM, a managed application at its appliance, etc.
    // Surface it as an association edge from the manager to the resource.
    for resource in &inv.resources {
        let Some(manager) = resource.managed_by.as_deref().filter(|m| !m.is_empty()) else {
            continue;
        };
        let label = if resource
            .resource_type
            .to_lowercase()
            .starts_with("microsoft.compute/disks")
        {
            "attached"
        } else {
            "manages"
        };
        add_edge(
            norm(manager),
            norm(&resource.id),
            EdgeKind::Association,
            label,
            &mut edges,
        );
    }

    // NSG associations declared on subnets (from the vnet listing).
    for (nsg, subnet) in subnet_nsgs {
        add_edge(nsg, subnet, EdgeKind::Network, "protects", &mut edges);
    }

    // NICs anchor the network wiring. A NIC owned by a VM has been folded
    // into that VM's card, so everything the NIC implies — subnet membership,
    // public IPs, NSG protection — re-anchors onto the VM itself: the VM
    // renders inside its subnet, with the NIC as card subtext.
    for nic in &inv.nics {
        let nic_id = norm(&nic.id);
        let anchor_id = match nic_vm.get(&nic_id).filter(|vm| index.contains_key(*vm)) {
            Some(vm) => vm.clone(),
            None => nic_id.clone(),
        };
        let Some(&anchor_index) = index.get(&anchor_id) else {
            continue; // absent from the resource listing; skip rather than invent
        };
        // NSG association from the nic listing.
        if let Some(nsg) = nic
            .network_security_group
            .as_ref()
            .and_then(|r| r.id.as_deref())
        {
            add_edge(
                norm(nsg),
                anchor_id.clone(),
                EdgeKind::Network,
                "protects",
                &mut edges,
            );
        }
        // Nest the anchor into the NIC's subnet (first one wins). Public IPs
        // were folded into the anchor's card during the pre-pass.
        for ip_config in &nic.ip_configurations {
            if nodes[anchor_index].parent_id.is_none() {
                if let Some(subnet) = ip_config.subnet.as_ref().and_then(|s| s.id.as_deref()) {
                    let subnet_id = norm(subnet);
                    if index.contains_key(&subnet_id) {
                        nodes[anchor_index].parent_id = Some(subnet_id);
                    }
                }
            }
        }
    }

    // AKS node resource groups: a cluster's `MC_...` node RG holds all its
    // managed infrastructure — VM scale sets, load balancers, public IPs,
    // NSGs. Nest every resource in that RG inside the cluster, which becomes
    // a container box, so the Kubernetes infra reads as one unit instead of
    // scattering across the canvas (and out of the detached boxes).
    let node_rg_to_aks: HashMap<String, String> = inv
        .aks
        .iter()
        .filter_map(|c| {
            let nrg = c.node_resource_group.as_deref().filter(|g| !g.is_empty())?;
            let id = norm(&c.id);
            index.contains_key(&id).then(|| (nrg.to_lowercase(), id))
        })
        .collect();
    if !node_rg_to_aks.is_empty() {
        let mut nested: Vec<(usize, String)> = Vec::new();
        for (i, node) in nodes.iter().enumerate() {
            if node.parent_id.is_some() {
                continue;
            }
            let Some(rg) = node.group.as_deref() else {
                continue;
            };
            if let Some(aks_id) = node_rg_to_aks.get(&rg.to_lowercase()) {
                if node.id != *aks_id {
                    nested.push((i, aks_id.clone()));
                }
            }
        }
        for (i, aks_id) in nested {
            nodes[i].parent_id = Some(aks_id.clone());
            if let Some(&ai) = index.get(&aks_id) {
                nodes[ai].container = true;
            }
        }
    }

    let mut topology = Topology {
        provider: "azure".into(),
        scope_id: inv.account.id.clone(),
        scope_label: inv.account.name.clone(),
        nodes,
        edges,
        warnings: inv.warnings,
    };
    crate::model::group_detached(&mut topology);
    topology
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
            webapps: serde_json::from_str(include_str!("fixtures/webapps.json")).unwrap(),
            vms: serde_json::from_str(include_str!("fixtures/vms.json")).unwrap(),
            sshkeys: serde_json::from_str(include_str!("fixtures/sshkeys.json")).unwrap(),
            aks: serde_json::from_str(include_str!("fixtures/aks.json")).unwrap(),
            restore_points: serde_json::from_str(include_str!("fixtures/restore-points.json"))
                .unwrap(),
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
        // Resources without a network/hosting home sit at the top level.
        let storage = find(&t, "ststray");
        assert_eq!(storage.parent_id, None);
        let vm = find(&t, "vm-web-01");
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
    fn vm_nests_into_its_subnet_with_nic_folded() {
        let t = build();
        // The NIC is owned by vm-web-01, so it folds into the VM's card and
        // the VM itself renders inside the NIC's subnet. The ghost subnet in
        // a non-existent vnet is ignored, so snet-a wins.
        assert!(!t.nodes.iter().any(|n| n.name == "nic-web-01"));
        let vm = find(&t, "vm-web-01");
        let vm_parent = vm.parent_id.as_deref().unwrap();
        assert!(vm_parent.contains("subnets/snet-a"));
        assert!(!vm_parent.contains("snet-missing"));

        // The NIC's public IP folds into the VM's card, not an edge or node.
        assert!(!t.nodes.iter().any(|n| n.name == "pip-web"));
        let vm = find(&t, "vm-web-01");
        assert!(vm
            .attachments
            .iter()
            .any(|a| a.kind == "public ip" && a.name == "pip-web" && !a.shared));

        // No leftover free-standing wiring edges.
        assert!(!t.edges.iter().any(|e| {
            matches!(
                e.label.as_deref(),
                Some("attached") | Some("in subnet") | Some("public IP")
            )
        }));
    }

    #[test]
    fn disks_extensions_and_nics_fold_into_their_vm() {
        // Attached managed disks (managedBy = the VM), VM extensions, and
        // VM-owned NICs are not free-standing nodes — they ride on the card.
        let t = build();
        assert!(!t.nodes.iter().any(|n| n.name == "DataDisk_1"));
        assert!(!t
            .nodes
            .iter()
            .any(|n| n.name.contains("AADSSHLoginForLinux")));

        let vm = find(&t, "vm-web-01");
        let short: Vec<(&str, &str)> = vm
            .attachments
            .iter()
            .map(|a| (a.kind.as_str(), a.name.as_str()))
            .collect();
        // Hardware first, then the secondary group (public IP, restore point,
        // SSH key), each alphabetical by kind.
        assert_eq!(
            short,
            vec![
                ("disk", "DataDisk_1"),
                ("extension", "AADSSHLoginForLinux"),
                ("nic", "nic-web-01"),
                ("public ip", "pip-web"),
                ("restore point", "rpc-vm-web-01"),
                ("ssh key", "key-admin"),
            ]
        );
        // An unattached disk stays visible, gathered in the detached box.
        let orphan = find(&t, "disk-orphan");
        assert!(orphan.attachments.is_empty());
        let group = find(&t, "Managed disks");
        assert!(group.container);
        assert_eq!(orphan.parent_id.as_deref(), Some(group.id.as_str()));
    }

    #[test]
    fn ssh_keys_fold_into_vms_by_key_material() {
        let t = build();
        // key-admin's material matches both VMs' osProfile keys (one carries
        // a trailing newline — matching trims whitespace), so it folds into
        // both, flagged shared, with no standalone card.
        assert!(!t.nodes.iter().any(|n| n.name == "key-admin"));
        for vm_name in ["vm-web-01", "vm-web-02"] {
            let vm = find(&t, vm_name);
            let key = vm
                .attachments
                .iter()
                .find(|a| a.kind == "ssh key")
                .unwrap_or_else(|| panic!("{vm_name} missing ssh key attachment"));
            assert_eq!(key.name, "key-admin");
            assert!(key.shared, "{vm_name}'s key should be flagged shared");
        }

        // A key no VM uses stays visible in the detached box, as Security.
        let orphan = find(&t, "key-orphan");
        assert_eq!(orphan.category, ResourceCategory::Security);
        assert_eq!(orphan.kind_label, "SSH public key");
        let group = find(&t, "SSH public keys");
        assert!(group.container);
        assert_eq!(orphan.parent_id.as_deref(), Some(group.id.as_str()));
        // Every fixture public IP is attached, so no detached IP box exists.
        assert!(!t.nodes.iter().any(|n| n.name == "Public IP addresses"));
    }

    #[test]
    fn app_service_nests_in_its_plan_and_slot_folds_in() {
        let t = build();
        let app = find(&t, "app-portal");
        let plan = find(&t, "plan-portal");
        assert_eq!(app.parent_id.as_deref(), Some(plan.id.as_str()));
        assert!(plan.container, "a hosting plan renders as a container");
        // The staging slot rides on the site's card.
        assert!(!t.nodes.iter().any(|n| n.name == "app-portal/staging"));
        assert_eq!(
            app.attachments,
            vec![Attachment {
                kind: "slot".into(),
                name: "staging".into(),
                shared: false,
            }]
        );
        // A webapp whose site/plan never appeared in the listings is skipped.
        assert!(!t.nodes.iter().any(|n| n.name == "app-ghost"));
    }

    #[test]
    fn aks_node_resource_group_nests_in_the_cluster() {
        let t = build();
        let aks = find(&t, "aks-rex");
        assert!(aks.container, "AKS cluster should become a container");
        assert_eq!(aks.kind_label, "AKS cluster");
        // Everything in MC_rg-app_aks-rex_eastus2 nests inside the cluster.
        for name in ["aks-nodepool1-vmss", "kubernetes-lb-ip"] {
            assert_eq!(
                find(&t, name).parent_id.as_deref(),
                Some(aks.id.as_str()),
                "{name} should nest in the AKS cluster"
            );
        }
        let vmss = find(&t, "aks-nodepool1-vmss");
        assert_eq!(vmss.category, ResourceCategory::Compute);
        assert_eq!(vmss.kind_label, "VM scale set");
        // The AKS load-balancer public IP nested here, not in a detached box.
        assert!(!t.nodes.iter().any(|n| n.name == "Public IP addresses"));
    }

    #[test]
    fn network_watchers_gather_into_a_regional_box() {
        let t = build();
        let nw = find(&t, "NetworkWatcher_westeurope");
        assert_eq!(nw.kind_label, "Network Watcher"); // curated, not de-camel-cased
        let g = find(&t, "Network Watchers");
        assert!(g.container);
        assert_eq!(g.kind_label, "Regional"); // not "Detached"
        assert_eq!(nw.parent_id.as_deref(), Some(g.id.as_str()));
    }

    #[test]
    fn restore_point_collections_fold_into_their_source_vm() {
        let t = build();
        // The source VM (from the per-RG listing) matches vm-web-01, so the
        // collection folds onto its card instead of standing as a node.
        assert!(!t.nodes.iter().any(|n| n.name == "rpc-vm-web-01"));
        let vm = find(&t, "vm-web-01");
        assert!(vm
            .attachments
            .iter()
            .any(|a| a.kind == "restore point" && a.name == "rpc-vm-web-01"));
        // No detached restore-point box, since the only one resolved.
        assert!(!t
            .nodes
            .iter()
            .any(|n| n.name == "Restore point collections"));
    }

    #[test]
    fn nsg_references_become_protects_edges() {
        let t = build();
        let protects: Vec<&TopologyEdge> = t
            .edges
            .iter()
            .filter(|e| e.label.as_deref() == Some("protects"))
            .collect();
        // subnet-level (vnet listing) + nic-level (nic listing, re-anchored
        // onto the NIC's VM).
        assert_eq!(protects.len(), 2);
        for e in &protects {
            assert!(e.source.contains("networksecuritygroups/nsg-web"));
            assert_eq!(e.kind, EdgeKind::Network);
        }
        assert!(protects.iter().any(|e| e.target.contains("subnets/snet-a")));
        assert!(protects
            .iter()
            .any(|e| e.target.contains("virtualmachines/vm-web-01")));
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
