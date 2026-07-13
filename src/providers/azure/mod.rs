mod az_types;
mod cli;
mod mapper;

use crate::model::{ProviderError, ProviderInfo, ProviderStatus, ScopeOption, Topology};
use az_types::*;
use cli::{az_json, AzExecutor, RealAzExecutor};
use mapper::{build_topology, AzureInventory};

pub struct AzureProvider {
    exec: Box<dyn AzExecutor>,
}

impl AzureProvider {
    pub fn new() -> Self {
        Self {
            exec: Box::new(RealAzExecutor),
        }
    }

    /// Best-effort enrichment listing: failures become warnings, not errors
    /// (unregistered resource providers, missing permissions…).
    fn try_list<T: serde::de::DeserializeOwned>(
        &self,
        args: &[&str],
        warnings: &mut Vec<String>,
    ) -> Vec<T> {
        match az_json::<Vec<T>>(self.exec.as_ref(), args) {
            Ok(list) => list,
            Err(e) => {
                let cmd: Vec<&str> = args
                    .iter()
                    .copied()
                    .take_while(|a| !a.starts_with("--"))
                    .collect();
                warnings.push(format!("az {} failed: {}", cmd.join(" "), e.message));
                Vec::new()
            }
        }
    }
}

impl super::CloudProvider for AzureProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "azure",
            display_name: "Microsoft Azure",
            demo: false,
        }
    }

    fn check_status(&self) -> ProviderStatus {
        match az_json::<AzAccount>(self.exec.as_ref(), &["account", "show"]) {
            Ok(account) => ProviderStatus::Ok {
                account_label: account.name,
                detail: account.user.and_then(|u| u.name),
            },
            Err(e) => ProviderStatus::Failed(e),
        }
    }

    fn list_scopes(&self) -> Result<Vec<ScopeOption>, ProviderError> {
        let accounts: Vec<AzAccount> = az_json(self.exec.as_ref(), &["account", "list"])?;
        let mut scopes: Vec<ScopeOption> = accounts
            .into_iter()
            .map(|a| ScopeOption {
                id: a.id,
                label: a.name,
                is_default: a.is_default,
            })
            .collect();
        scopes.sort_by(|a, b| {
            b.is_default
                .cmp(&a.is_default)
                .then_with(|| a.label.cmp(&b.label))
        });
        Ok(scopes)
    }

    fn fetch_topology(&self, scope_id: Option<&str>) -> Result<Topology, ProviderError> {
        let mut scope_args: Vec<&str> = Vec::new();
        if let Some(id) = scope_id {
            scope_args.extend(["--subscription", id]);
        }
        fn with_scope<'a>(base: &[&'a str], scope: &[&'a str]) -> Vec<&'a str> {
            base.iter().chain(scope.iter()).copied().collect()
        }

        // Required listings — without these there is no topology.
        let account: AzAccount = az_json(
            self.exec.as_ref(),
            &with_scope(&["account", "show"], &scope_args),
        )?;
        let groups: Vec<AzGroup> = az_json(
            self.exec.as_ref(),
            &with_scope(&["group", "list"], &scope_args),
        )?;
        let resources: Vec<AzResource> = az_json(
            self.exec.as_ref(),
            &with_scope(&["resource", "list"], &scope_args),
        )?;

        // Enrichment listings — degrade gracefully.
        let mut warnings = Vec::new();
        let vnets: Vec<AzVnet> = self.try_list(
            &with_scope(&["network", "vnet", "list"], &scope_args),
            &mut warnings,
        );
        let nics: Vec<AzNic> = self.try_list(
            &with_scope(&["network", "nic", "list"], &scope_args),
            &mut warnings,
        );
        let webapps: Vec<AzWebApp> =
            self.try_list(&with_scope(&["webapp", "list"], &scope_args), &mut warnings);
        let vms: Vec<AzVm> =
            self.try_list(&with_scope(&["vm", "list"], &scope_args), &mut warnings);
        let sshkeys: Vec<AzSshKey> =
            self.try_list(&with_scope(&["sshkey", "list"], &scope_args), &mut warnings);
        let aks: Vec<AzAksCluster> =
            self.try_list(&with_scope(&["aks", "list"], &scope_args), &mut warnings);

        Ok(build_topology(AzureInventory {
            account,
            groups,
            resources,
            vnets,
            nics,
            webapps,
            vms,
            sshkeys,
            aks,
            warnings,
        }))
    }
}
