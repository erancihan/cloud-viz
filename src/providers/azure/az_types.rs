//! Serde shapes for the Azure CLI JSON we consume (only the fields we read).
//! `alias` entries absorb the casing drift between ARM (`publicIPAddress`)
//! and az CLI output (`publicIpAddress`).

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct AzAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub user: Option<AzAccountUser>,
    #[serde(default, rename = "isDefault")]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzAccountUser {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzGroup {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzResource {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    #[serde(default, rename = "resourceGroup")]
    pub resource_group: Option<String>,
    /// ARM's generic ownership pointer — for an attached managed disk this is
    /// the owning VM's resource id.
    #[serde(default, rename = "managedBy")]
    pub managed_by: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub sku: Option<serde_json::Value>,
    #[serde(default)]
    pub tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzVnet {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "resourceGroup")]
    pub resource_group: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default, rename = "addressSpace")]
    pub address_space: Option<AzAddressSpace>,
    #[serde(default)]
    pub subnets: Vec<AzSubnet>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzAddressSpace {
    #[serde(default, rename = "addressPrefixes")]
    pub address_prefixes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzSubnet {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "addressPrefix")]
    pub address_prefix: Option<String>,
    #[serde(default, rename = "addressPrefixes")]
    pub address_prefixes: Vec<String>,
    #[serde(default, rename = "networkSecurityGroup")]
    pub network_security_group: Option<AzIdRef>,
    /// Who the subnet is handed over to (App Service vnet integration, ACI,
    /// delegated PostgreSQL…) — explains subnets with no member cards.
    #[serde(default)]
    pub delegations: Vec<AzDelegation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzDelegation {
    #[serde(default, rename = "serviceName")]
    pub service_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzNic {
    pub id: String,
    #[serde(default, rename = "virtualMachine")]
    pub virtual_machine: Option<AzIdRef>,
    #[serde(default, rename = "networkSecurityGroup")]
    pub network_security_group: Option<AzIdRef>,
    #[serde(default, rename = "ipConfigurations")]
    pub ip_configurations: Vec<AzNicIpConfiguration>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzNicIpConfiguration {
    #[serde(default)]
    pub subnet: Option<AzIdRef>,
    #[serde(default, rename = "publicIpAddress", alias = "publicIPAddress")]
    pub public_ip_address: Option<AzIdRef>,
}

/// One entry of `az webapp list` — only the plan linkage is read.
#[derive(Debug, Clone, Deserialize)]
pub struct AzWebApp {
    pub id: String,
    #[serde(default, rename = "appServicePlanId", alias = "serverFarmId")]
    pub app_service_plan_id: Option<String>,
}

/// One entry of `az vm list` — the SSH key material (to match VMs to
/// `Microsoft.Compute/sshPublicKeys` resources; ARM copies the key text into
/// the VM's osProfile instead of referencing the key resource) and the boot
/// diagnostics storage URI (to associate the VM with its storage account).
#[derive(Debug, Clone, Deserialize)]
pub struct AzVm {
    pub id: String,
    #[serde(default, rename = "osProfile")]
    pub os_profile: Option<AzOsProfile>,
    #[serde(default, rename = "diagnosticsProfile")]
    pub diagnostics_profile: Option<AzDiagnosticsProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzDiagnosticsProfile {
    #[serde(default, rename = "bootDiagnostics")]
    pub boot_diagnostics: Option<AzBootDiagnostics>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzBootDiagnostics {
    /// `https://<account>.blob.core.windows.net/` — absent (managed storage)
    /// on most modern VMs.
    #[serde(default, rename = "storageUri")]
    pub storage_uri: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzOsProfile {
    #[serde(default, rename = "linuxConfiguration")]
    pub linux_configuration: Option<AzLinuxConfiguration>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzLinuxConfiguration {
    #[serde(default)]
    pub ssh: Option<AzSshConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzSshConfig {
    #[serde(default, rename = "publicKeys")]
    pub public_keys: Vec<AzSshPublicKeyRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzSshPublicKeyRef {
    #[serde(default, rename = "keyData")]
    pub key_data: Option<String>,
}

/// One entry of `az sshkey list` (Microsoft.Compute/sshPublicKeys).
#[derive(Debug, Clone, Deserialize)]
pub struct AzSshKey {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "publicKey")]
    pub public_key: Option<String>,
}

/// One entry of `az restore-point collection list` — its `source.id` is the
/// backed-up VM. az flattens `properties`, but we accept it nested too.
#[derive(Debug, Clone, Deserialize)]
pub struct AzRestorePointCollection {
    pub id: String,
    #[serde(default)]
    pub source: Option<AzIdRef>,
    #[serde(default)]
    pub properties: Option<AzRpcProperties>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzRpcProperties {
    #[serde(default)]
    pub source: Option<AzIdRef>,
}

impl AzRestorePointCollection {
    /// The backed-up VM's ARM id, whether `source` is flattened or nested.
    pub fn source_vm(&self) -> Option<&str> {
        self.source
            .as_ref()
            .or_else(|| self.properties.as_ref().and_then(|p| p.source.as_ref()))
            .and_then(|s| s.id.as_deref())
    }
}

/// One entry of `az aks list` (Microsoft.ContainerService/managedClusters).
/// Only the node resource group is read — that `MC_...` group holds all the
/// cluster's managed infrastructure (VM scale sets, load balancers, IPs).
#[derive(Debug, Clone, Deserialize)]
pub struct AzAksCluster {
    pub id: String,
    #[serde(default, rename = "nodeResourceGroup")]
    pub node_resource_group: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzIdRef {
    #[serde(default)]
    pub id: Option<String>,
}

/// One entry of `az snapshot list` — the source disk it was taken from.
#[derive(Debug, Clone, Deserialize)]
pub struct AzSnapshot {
    pub id: String,
    #[serde(default, rename = "creationData")]
    pub creation_data: Option<AzCreationData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzCreationData {
    #[serde(default, rename = "sourceResourceId")]
    pub source_resource_id: Option<String>,
}

/// One entry of `az network bastion list` — the AzureBastionSubnet it sits
/// in ties the host to its virtual network.
#[derive(Debug, Clone, Deserialize)]
pub struct AzBastion {
    pub id: String,
    #[serde(default, rename = "ipConfigurations")]
    pub ip_configurations: Vec<AzNicIpConfiguration>,
}

/// One entry of `az vmss list` — the subnet its instances' NICs attach to.
#[derive(Debug, Clone, Deserialize)]
pub struct AzVmss {
    pub id: String,
    #[serde(default, rename = "virtualMachineProfile")]
    pub virtual_machine_profile: Option<AzVmssProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzVmssProfile {
    #[serde(default, rename = "networkProfile")]
    pub network_profile: Option<AzVmssNetProfile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzVmssNetProfile {
    #[serde(default, rename = "networkInterfaceConfigurations")]
    pub nic_configs: Vec<AzVmssNicConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzVmssNicConfig {
    #[serde(default, rename = "ipConfigurations")]
    pub ip_configurations: Vec<AzNicIpConfiguration>,
}

impl AzVmss {
    /// The first subnet its NIC configurations reference.
    pub fn subnet_id(&self) -> Option<&str> {
        self.virtual_machine_profile
            .as_ref()?
            .network_profile
            .as_ref()?
            .nic_configs
            .iter()
            .flat_map(|c| &c.ip_configurations)
            .find_map(|ip| ip.subnet.as_ref()?.id.as_deref())
    }
}

/// One entry of `az network private-endpoint list` — the private-link
/// service connection names the resource the endpoint fronts.
#[derive(Debug, Clone, Deserialize)]
pub struct AzPrivateEndpoint {
    pub id: String,
    #[serde(default, rename = "privateLinkServiceConnections")]
    pub connections: Vec<AzPeConnection>,
    #[serde(default, rename = "manualPrivateLinkServiceConnections")]
    pub manual_connections: Vec<AzPeConnection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzPeConnection {
    #[serde(default, rename = "privateLinkServiceId")]
    pub private_link_service_id: Option<String>,
}

impl AzPrivateEndpoint {
    /// The fronted resource's ARM id (first connection, auto or manual).
    pub fn target_id(&self) -> Option<&str> {
        self.connections
            .iter()
            .chain(self.manual_connections.iter())
            .find_map(|c| c.private_link_service_id.as_deref())
    }
}

/// One entry of `az postgres flexible-server list` — vnet-integrated servers
/// carry the delegated subnet they live in.
#[derive(Debug, Clone, Deserialize)]
pub struct AzPgFlexServer {
    pub id: String,
    #[serde(default)]
    pub network: Option<AzPgNetwork>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzPgNetwork {
    #[serde(default, rename = "delegatedSubnetResourceId")]
    pub delegated_subnet_resource_id: Option<String>,
}

/// One entry of `az backup item list` (per Recovery Services vault). The
/// protected resource sits under `properties`, but we accept the flattened
/// shape too, mirroring [`AzRestorePointCollection`].
#[derive(Debug, Clone, Deserialize)]
pub struct AzBackupItem {
    #[serde(default, rename = "virtualMachineId")]
    pub virtual_machine_id: Option<String>,
    #[serde(default, rename = "sourceResourceId")]
    pub source_resource_id: Option<String>,
    #[serde(default)]
    pub properties: Option<AzBackupItemProps>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzBackupItemProps {
    #[serde(default, rename = "virtualMachineId")]
    pub virtual_machine_id: Option<String>,
    #[serde(default, rename = "sourceResourceId")]
    pub source_resource_id: Option<String>,
}

impl AzBackupItem {
    /// The protected resource's ARM id, flattened or nested.
    pub fn protected_id(&self) -> Option<&str> {
        self.virtual_machine_id
            .as_deref()
            .or(self.source_resource_id.as_deref())
            .or_else(|| {
                let p = self.properties.as_ref()?;
                p.virtual_machine_id
                    .as_deref()
                    .or(p.source_resource_id.as_deref())
            })
    }
}

/// Response of the Cost Management query API (`az rest --method post …/
/// Microsoft.CostManagement/query`), aggregating month-to-date actual cost
/// grouped by ResourceId. Columnar: a `columns` legend plus untyped `rows`.
#[derive(Debug, Clone, Deserialize)]
pub struct AzCostQueryResponse {
    #[serde(default)]
    pub properties: Option<AzCostQueryProperties>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzCostQueryProperties {
    #[serde(default)]
    pub columns: Vec<AzCostColumn>,
    #[serde(default)]
    pub rows: Vec<Vec<serde_json::Value>>,
    #[serde(default, rename = "nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzCostColumn {
    pub name: String,
}

/// One resource's month-to-date cost, decoded from the columnar response.
#[derive(Debug, Clone)]
pub struct AzCostRow {
    pub resource_id: String,
    pub cost: f64,
    pub currency: Option<String>,
}

impl AzCostQueryResponse {
    /// True when the API returned only a partial page (we don't paginate —
    /// callers surface a warning instead of silently missing rows).
    pub fn truncated(&self) -> bool {
        self.properties
            .as_ref()
            .and_then(|p| p.next_link.as_deref())
            .is_some_and(|l| !l.is_empty())
    }

    /// Decodes the rows by looking the columns up by name (the API does not
    /// guarantee an order): the cost measure ("Cost"/"PreTaxCost"), the
    /// "ResourceId" dimension, and the "Currency" tag-along column.
    pub fn resource_costs(&self) -> Vec<AzCostRow> {
        let Some(props) = &self.properties else {
            return Vec::new();
        };
        let col = |names: &[&str]| -> Option<usize> {
            props
                .columns
                .iter()
                .position(|c| names.iter().any(|n| c.name.eq_ignore_ascii_case(n)))
        };
        let (Some(cost_col), Some(id_col)) = (col(&["Cost", "PreTaxCost"]), col(&["ResourceId"]))
        else {
            return Vec::new();
        };
        let currency_col = col(&["Currency"]);
        props
            .rows
            .iter()
            .filter_map(|row| {
                let cost = match row.get(cost_col)? {
                    serde_json::Value::Number(n) => n.as_f64()?,
                    serde_json::Value::String(s) => s.parse().ok()?,
                    _ => return None,
                };
                let resource_id = row.get(id_col)?.as_str()?.to_string();
                if resource_id.is_empty() {
                    return None;
                }
                let currency = currency_col
                    .and_then(|c| row.get(c))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                Some(AzCostRow {
                    resource_id,
                    cost,
                    currency,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_query_response_decodes_rows_by_column_name() {
        let r: AzCostQueryResponse =
            serde_json::from_str(include_str!("fixtures/costs.json")).unwrap();
        assert!(!r.truncated());
        let rows = r.resource_costs();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].cost, 30.0);
        assert!(rows[0].resource_id.ends_with("vm-web-01"));
        assert_eq!(rows[0].currency.as_deref(), Some("USD"));

        // Shuffled columns, the PreTaxCost measure, a string-typed amount,
        // and a nextLink (partial page) still decode.
        let shuffled = serde_json::json!({"properties": {
            "columns": [
                {"name": "ResourceId", "type": "String"},
                {"name": "PreTaxCost", "type": "Number"},
                {"name": "Currency", "type": "String"}
            ],
            "rows": [["/id-a", "1.5", "EUR"]],
            "nextLink": "https://example.invalid/next-page"
        }});
        let r: AzCostQueryResponse = serde_json::from_value(shuffled).unwrap();
        assert!(r.truncated());
        let rows = r.resource_costs();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].resource_id, "/id-a");
        assert_eq!(rows[0].cost, 1.5);
        assert_eq!(rows[0].currency.as_deref(), Some("EUR"));
    }
}
