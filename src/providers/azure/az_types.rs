//! Serde shapes for the Azure CLI JSON we consume (only the fields we read).
//! `alias` entries absorb the casing drift between ARM (`publicIPAddress`)
//! and az CLI output (`publicIpAddress`).

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct AzAccount {
    pub id: String,
    pub name: String,
    #[serde(default, rename = "tenantId")]
    pub tenant_id: Option<String>,
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
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub tags: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzResource {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    #[serde(default, rename = "resourceGroup")]
    pub resource_group: Option<String>,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct AzNic {
    pub id: String,
    #[serde(default, rename = "virtualMachine")]
    pub virtual_machine: Option<AzIdRef>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct AzIdRef {
    #[serde(default)]
    pub id: Option<String>,
}
