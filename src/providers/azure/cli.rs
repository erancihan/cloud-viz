//! Thin wrapper around the Azure CLI. All Azure data enters the app through
//! here — the user signs in with `az login` in their own terminal and we
//! reuse that session; no credentials are stored by this app.

use crate::model::{ProviderError, ProviderErrorCode};
use std::process::Command;

pub const INSTALL_HINT: &str =
    "Install it from https://learn.microsoft.com/cli/azure/install-azure-cli and restart the app.";
pub const LOGIN_HINT: &str = "Run `az login` in a terminal, then refresh.";

/// Runs `az <args>` and returns raw stdout. Injectable for tests.
pub trait AzExecutor: Send + Sync {
    fn run(&self, args: &[&str]) -> Result<String, ProviderError>;
}

pub struct RealAzExecutor;

impl AzExecutor for RealAzExecutor {
    fn run(&self, args: &[&str]) -> Result<String, ProviderError> {
        let output = new_az_command()
            .args(args)
            .args(["--output", "json", "--only-show-errors"])
            .env("AZURE_CORE_NO_COLOR", "1")
            .output();

        let output = match output {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(ProviderError {
                    code: ProviderErrorCode::CliMissing,
                    message: "Azure CLI (az) was not found on PATH.".into(),
                    hint: Some(INSTALL_HINT.into()),
                });
            }
            Err(e) => {
                return Err(ProviderError {
                    code: ProviderErrorCode::Unknown,
                    message: format!("failed to start az: {e}"),
                    hint: None,
                });
            }
        };

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if looks_signed_out(&stderr) {
            return Err(ProviderError {
                code: ProviderErrorCode::NotAuthenticated,
                message: "Azure CLI is not signed in.".into(),
                hint: Some(LOGIN_HINT.into()),
            });
        }
        Err(ProviderError {
            code: ProviderErrorCode::CommandFailed,
            message: format!(
                "az {} failed: {}",
                args.join(" "),
                stderr.lines().next().unwrap_or("unknown error")
            ),
            hint: None,
        })
    }
}

fn new_az_command() -> Command {
    // On Windows `az` is a .cmd shim which CreateProcess cannot start
    // directly; go through cmd.exe. Arguments are fixed flags plus
    // subscription GUIDs, so shell interpolation is not a concern.
    if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "az"]);
        cmd
    } else {
        Command::new("az")
    }
}

fn looks_signed_out(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    [
        "az login",
        "aadsts",
        "refresh token",
        "re-authenticate",
        "reauthenticate",
        "no subscriptions found",
        "credentials have expired",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Runs an az command and parses its JSON output.
pub fn az_json<T: serde::de::DeserializeOwned>(
    exec: &dyn AzExecutor,
    args: &[&str],
) -> Result<T, ProviderError> {
    let stdout = exec.run(args)?;
    serde_json::from_str(&stdout).map_err(|e| ProviderError {
        code: ProviderErrorCode::CommandFailed,
        message: format!("az {} returned unexpected JSON: {e}", args.join(" ")),
        hint: None,
    })
}
