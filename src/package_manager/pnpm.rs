use std::path::Path;

use crate::config::Commands;
use crate::utils::run_command;

use super::{
    ApplyContext, ApplyDriver, ApplyError, ApplyResult, ApplyStatus, LockfileDriver,
    PackageInstance,
};

pub struct Pnpm {
    pub npmrc_template: Option<String>,
    pub registry: Option<String>,
}

impl Pnpm {
    pub fn name(&self) -> &str {
        "pnpm"
    }

    pub fn lockfile_name(&self) -> &str {
        "pnpm-lock.yaml"
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("pnpm").is_ok()
    }

    pub fn default_commands(&self) -> Commands {
        Commands {
            install: Some("pnpm install".into()),
            test: Some("pnpm test".into()),
            build: Some("pnpm run build".into()),
        }
    }
}

impl LockfileDriver for Pnpm {
    fn get_all_instances(&self, _project_path: &Path, _name: &str) -> Vec<PackageInstance> {
        todo!("pnpm lockfile parsing not yet implemented")
    }
}

impl ApplyDriver for Pnpm {
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

            let install_cmd = format!("pnpm add {}@{} {}", ctx.package, version_spec, flag);
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
                std::fs::remove_dir_all(&node_modules).ok();
                result.node_modules_deleted = true;
            }

            let lockfile_path = ctx.project_path.join("pnpm-lock.yaml");
            if lockfile_path.exists() {
                std::fs::remove_file(&lockfile_path).ok();
                result.lockfile_deleted = true;
            }

            run_command("pnpm update", ctx.project_path).ok();
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
            run_command("pnpm audit --fix", project_path).map_err(|e| ApplyError::CommandFailed {
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
