//! The contract every cloud provider implements. Adding a new cloud (AWS,
//! GCP…) means implementing [`CloudProvider`] and registering it in
//! [`builtin_providers`] — nothing in the app state machine or the UI changes.

pub mod azure;
pub mod demo;

use crate::model::{
    CostPeriod, ProviderError, ProviderInfo, ProviderStatus, ScopeOption, Topology,
};
use std::sync::Arc;

pub trait CloudProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;

    /// Is the provider usable right now (CLI installed, user signed in)?
    fn check_status(&self) -> ProviderStatus;

    /// Selectable fetch scopes: Azure subscriptions, AWS account/region pairs…
    fn list_scopes(&self) -> Result<Vec<ScopeOption>, ProviderError>;

    /// Fetch the inventory for a scope and normalize it into a Topology,
    /// with per-resource costs covering `period`. Blocking — the app calls
    /// this from a worker thread.
    fn fetch_topology(
        &self,
        scope_id: Option<&str>,
        period: CostPeriod,
    ) -> Result<Topology, ProviderError>;
}

/// The built-in providers. New clouds get one line here.
pub fn builtin_providers() -> Vec<Arc<dyn CloudProvider>> {
    vec![
        Arc::new(azure::AzureProvider::new()),
        Arc::new(demo::DemoProvider),
    ]
}
