use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::config::Commands;
use crate::utils::run_command;

use super::{
    ApplyContext, ApplyDriver, ApplyError, ApplyResult, ApplyStatus, AuthConfig, LockfileDriver,
    PackageInstance, Version,
};

pub struct Npm {
    pub npmrc_template: Option<String>,
    pub registry: Option<String>,
}

#[derive(Debug, Error)]
pub enum PackageLockError {
    #[error("failed to read package-lock.json at '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse package-lock.json at '{path}': {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLockJson {
    pub lockfile_version: u8,
    pub packages: HashMap<String, PackageLockEntry>,
}

#[derive(Debug, Deserialize)]
pub struct PackageLockEntry {
    pub version: Option<Version>,
}

impl PackageLockJson {
    pub fn from_path(project_path: &Path) -> Result<Self, PackageLockError> {
        let path = project_path.join("package-lock.json");
        let content = std::fs::read_to_string(&path).map_err(|e| PackageLockError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_json::from_str(&content).map_err(|e| PackageLockError::Parse {
            path: path.display().to_string(),
            source: e,
        })
    }
}

impl Npm {
    pub fn name(&self) -> &str {
        "npm"
    }

    pub fn lockfile_name(&self) -> &str {
        "package-lock.json"
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("npm").is_ok()
    }

    pub fn default_commands(&self) -> Commands {
        Commands {
            install: Some("npm install".into()),
            test: Some("npm test".into()),
            build: Some("npm run build".into()),
        }
    }

    /// Renders the auth template with registry substitution and writes it to .npmrc
    pub fn setup_auth(
        &self,
        project_path: &Path,
        auth_config: &AuthConfig,
    ) -> Result<(), ApplyError> {
        if let Some(template) = &auth_config.auth_template {
            let rendered = if let Some(registry) = &auth_config.registry {
                template.replace("{registry}", registry)
            } else {
                template.clone()
            };

            let npmrc_path = project_path.join(".npmrc");
            fs::write(&npmrc_path, rendered).map_err(|e| ApplyError::CommandFailed {
                command: "write .npmrc".to_string(),
                path: npmrc_path.display().to_string(),
                message: e.to_string(),
            })?;
        }
        Ok(())
    }
}

impl LockfileDriver for Npm {
    fn get_all_instances(&self, project_path: &Path, name: &str) -> Vec<PackageInstance> {
        let lock = match PackageLockJson::from_path(project_path) {
            Ok(l) => l,
            Err(_) => return vec![],
        };

        let suffix = format!("node_modules/{name}");
        lock.packages
            .into_iter()
            .filter(|(key, _)| key == &suffix || key.ends_with(&format!("/{suffix}")))
            .filter_map(|(key, entry)| {
                entry.version.map(|v| PackageInstance {
                    path: key,
                    version: v,
                })
            })
            .collect()
    }
}

impl ApplyDriver for Npm {
    fn apply_update(&self, ctx: &ApplyContext) -> Result<ApplyResult, ApplyError> {
        let mut result = ApplyResult {
            package: ctx.package.to_string(),
            target_version: ctx.target_version.to_string(),
            audit_fix_ran: false,
            audit_fix_success: false,
            version_matched: false,
            lockfile_deleted: false,
            node_modules_deleted: false,
            update_ran: false,
            final_status: ApplyStatus::Success,
            error_reason: None,
        };

        // Setup auth configuration if provided
        if let Some(auth_config) = &ctx.auth_config {
            self.setup_auth(ctx.project_path, auth_config)?;
        }

        if self.supports_audit_fix() {
            result.audit_fix_ran = true;
            if let Some(audit_result) = self.audit_fix(ctx.project_path) {
                result.audit_fix_success = audit_result.is_ok();
            }
        }

        if ctx.scope != crate::config::DependencyScope::Transitive {
            let flag = match ctx.dependency_type {
                crate::config::DependencyType::DevDependency => "--save-dev",
                crate::config::DependencyType::PeerDependency => "--save-peer",
                crate::config::DependencyType::OptionalDependency => "--save-optional",
                crate::config::DependencyType::Dependency => "--save",
            };

            let version_spec = match ctx.update_policy {
                crate::config::UpdatePolicy::Exact => ctx.target_version.to_string(),
                crate::config::UpdatePolicy::Minimum => format!(">={}", ctx.target_version),
            };

            let install_cmd = format!("npm install {}@{} {}", ctx.package, version_spec, flag);
            run_command(&install_cmd, ctx.project_path).map_err(|e| ApplyError::CommandFailed {
                command: e.command,
                path: e.path,
                message: e.message,
            })?;
        }

        let current_version = self.get_version(ctx.project_path, ctx.package);
        result.version_matched = current_version
            .as_ref()
            .map(|v| v.0 == ctx.target_version)
            .unwrap_or(false);

        if !result.version_matched {
            let node_modules = ctx.project_path.join("node_modules");
            if node_modules.exists() {
                fs::remove_dir_all(&node_modules).ok();
                result.node_modules_deleted = true;
            }

            let lockfile_path = ctx.project_path.join("package-lock.json");
            if lockfile_path.exists() {
                fs::remove_file(&lockfile_path).ok();
                result.lockfile_deleted = true;
            }

            run_command("npm update", ctx.project_path).ok();
            result.update_ran = true;

            let current_version = self.get_version(ctx.project_path, ctx.package);
            result.version_matched = current_version
                .as_ref()
                .map(|v| v.0 == ctx.target_version)
                .unwrap_or(false);
        }

        if result.version_matched {
            if result.update_ran {
                result.final_status = ApplyStatus::PartialSuccess;
            } else {
                result.final_status = ApplyStatus::Success;
            }
        } else {
            result.final_status = ApplyStatus::VersionMismatch;
        }

        Ok(result)
    }

    fn audit_fix(&self, project_path: &Path) -> Option<Result<(), ApplyError>> {
        Some(
            run_command("npm audit fix", project_path).map_err(|e| ApplyError::CommandFailed {
                command: e.command,
                path: e.path,
                message: e.message,
            }),
        )
    }

    fn supports_audit_fix(&self) -> bool {
        true
    }
}
