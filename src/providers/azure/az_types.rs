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

/// One entry of `az vm list` — only the SSH key material is read, to match
/// VMs to `Microsoft.Compute/sshPublicKeys` resources (ARM copies the key
/// text into the VM's osProfile instead of referencing the key resource).
#[derive(Debug, Clone, Deserialize)]
pub struct AzVm {
    pub id: String,
    #[serde(default, rename = "osProfile")]
    pub os_profile: Option<AzOsProfile>,
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
