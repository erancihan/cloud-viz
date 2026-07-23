mod az_types;
mod cli;
mod mapper;

use crate::model::{
    days_in_month, CostPeriod, ProviderError, ProviderInfo, ProviderStatus, ScopeOption, Topology,
};
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

    fn fetch_topology(
        &self,
        scope_id: Option<&str>,
        period: CostPeriod,
    ) -> Result<Topology, ProviderError> {
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

        // Restore point collections expose their source VM only in the per-RG
        // listing, so query just the resource groups that actually contain one
        // (usually one or two) rather than the whole subscription.
        let mut rpc_rgs: Vec<String> = resources
            .iter()
            .filter(|r| {
                r.resource_type
                    .eq_ignore_ascii_case("Microsoft.Compute/restorePointCollections")
            })
            .filter_map(|r| r.resource_group.clone())
            .collect();
        rpc_rgs.sort();
        rpc_rgs.dedup();
        let mut restore_points: Vec<AzRestorePointCollection> = Vec::new();
        for rg in &rpc_rgs {
            let base = [
                "restore-point",
                "collection",
                "list",
                "--resource-group",
                rg,
            ];
            restore_points.extend(self.try_list::<AzRestorePointCollection>(
                &with_scope(&base, &scope_args),
                &mut warnings,
            ));
        }

        // `az webapp list` omits function apps — they have their own listing
        // but the same shape (only the plan linkage is read).
        let functionapps: Vec<AzWebApp> = self.try_list(
            &with_scope(&["functionapp", "list"], &scope_args),
            &mut warnings,
        );
        let snapshots: Vec<AzSnapshot> = self.try_list(
            &with_scope(&["snapshot", "list"], &scope_args),
            &mut warnings,
        );
        let bastions: Vec<AzBastion> = self.try_list(
            &with_scope(&["network", "bastion", "list"], &scope_args),
            &mut warnings,
        );
        let vmss: Vec<AzVmss> =
            self.try_list(&with_scope(&["vmss", "list"], &scope_args), &mut warnings);
        let postgres: Vec<AzPgFlexServer> = self.try_list(
            &with_scope(&["postgres", "flexible-server", "list"], &scope_args),
            &mut warnings,
        );
        let private_endpoints: Vec<AzPrivateEndpoint> = self.try_list(
            &with_scope(&["network", "private-endpoint", "list"], &scope_args),
            &mut warnings,
        );

        // Backup items only list per vault, so query each Recovery Services
        // vault (usually a handful) like the restore-point per-RG loop.
        let mut backup_items: Vec<(String, AzBackupItem)> = Vec::new();
        for vault in resources.iter().filter(|r| {
            r.resource_type
                .eq_ignore_ascii_case("Microsoft.RecoveryServices/vaults")
        }) {
            let Some(rg) = vault.resource_group.as_deref() else {
                continue;
            };
            let base = [
                "backup",
                "item",
                "list",
                "--resource-group",
                rg,
                "--vault-name",
                &vault.name,
            ];
            for item in
                self.try_list::<AzBackupItem>(&with_scope(&base, &scope_args), &mut warnings)
            {
                backup_items.push((vault.id.clone(), item));
            }
        }

        let costs = self.fetch_costs(&account.id, period, &mut warnings);

        Ok(build_topology(AzureInventory {
            account,
            groups,
            resources,
            vnets,
            nics,
            webapps,
            functionapps,
            vms,
            sshkeys,
            aks,
            restore_points,
            snapshots,
            bastions,
            vmss,
            postgres,
            private_endpoints,
            backup_items,
            costs,
            warnings,
        }))
    }
}

/// Cost Management query body: actual cost per resource over the selected
/// period — the current month so far, or one whole past calendar month.
fn cost_query_body(period: CostPeriod) -> String {
    const DATASET: &str = r#""dataset":{"granularity":"None","aggregation":{"totalCost":{"name":"Cost","function":"Sum"}},"grouping":[{"type":"Dimension","name":"ResourceId"}]}"#;
    match period {
        CostPeriod::MonthToDate => {
            format!(r#"{{"type":"ActualCost","timeframe":"MonthToDate",{DATASET}}}"#)
        }
        CostPeriod::Month { year, month } => {
            let last = days_in_month(year, month);
            format!(
                r#"{{"type":"ActualCost","timeframe":"Custom","timePeriod":{{"from":"{year:04}-{month:02}-01T00:00:00Z","to":"{year:04}-{month:02}-{last:02}T23:59:59Z"}},{DATASET}}}"#
            )
        }
    }
}

impl AzureProvider {
    /// One Cost Management POST for the whole subscription. `az` has no
    /// built-in command for this API (`az costmanagement` is an extension),
    /// so go through `az rest`, which reuses the CLI's login. Best-effort:
    /// needs the Cost Management Reader role and the API throttles
    /// aggressively, so failures become a warning and the topology simply
    /// renders without cost badges.
    fn fetch_costs(
        &self,
        subscription_id: &str,
        period: CostPeriod,
        warnings: &mut Vec<String>,
    ) -> Vec<AzCostRow> {
        let url = format!(
            "https://management.azure.com/subscriptions/{subscription_id}/providers/Microsoft.CostManagement/query?api-version=2024-08-01"
        );
        let body = cost_query_body(period);
        let args = [
            "rest",
            "--method",
            "post",
            "--url",
            &url,
            "--headers",
            "Content-Type=application/json",
            "--body",
            &body,
        ];
        match az_json::<AzCostQueryResponse>(self.exec.as_ref(), &args) {
            Ok(response) => {
                if response.truncated() {
                    warnings.push(
                        "cost query returned more resources than one page; some cards may miss cost data".into(),
                    );
                }
                response.resource_costs()
            }
            Err(e) => {
                warnings.push(format!("cost query failed: {}", e.message));
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_query_body_covers_both_timeframes() {
        let mtd = cost_query_body(CostPeriod::MonthToDate);
        assert!(mtd.contains(r#""timeframe":"MonthToDate""#));
        assert!(!mtd.contains("timePeriod"));

        // A whole past month becomes a Custom window over its exact days —
        // February 2024 is a leap month.
        let feb = cost_query_body(CostPeriod::Month {
            year: 2024,
            month: 2,
        });
        assert!(feb.contains(r#""timeframe":"Custom""#));
        assert!(feb.contains(r#""from":"2024-02-01T00:00:00Z""#));
        assert!(feb.contains(r#""to":"2024-02-29T23:59:59Z""#));

        // Both carry the same per-resource aggregation.
        for body in [&mtd, &feb] {
            assert!(body.contains(r#""grouping":[{"type":"Dimension","name":"ResourceId"}]"#));
            serde_json::from_str::<serde_json::Value>(body).expect("body is valid JSON");
        }
    }
}
