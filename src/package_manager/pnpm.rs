use std::fs;
use std::path::Path;

use crate::config::{Commands, DependencyScope, DependencyType, UpdatePolicy};
use crate::utils::run_command;

use super::{
    ApplyDriver, ApplyError, ApplyResult, ApplyStatus, AuthConfig, BatchApplyResult,
    LockfileDriver, PackageInstance, PackageUpdateRequest, Version,
};

pub struct Pnpm {
    pub npmrc_template: Option<String>,
    pub registry: Option<String>,
}

impl Pnpm {
    pub fn name(&self) -> &str {
        "pnpm"
    }

    pub fn manifest_name(&self) -> &str {
        "package.json"
    }

    pub fn lockfile_name(&self) -> &str {
        "pnpm-lock.yaml"
    }

    pub fn has_manifest(&self, project_path: &Path) -> bool {
        project_path.join(self.manifest_name()).exists()
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("pnpm").is_ok()
    }

    pub fn ensure_lockfile(&self, project_path: &Path) -> Result<(), ApplyError> {
        if self.has_lockfile(project_path) {
            return Ok(());
        }

        if let Some(npmrc_path) = &self.npmrc_template {
            let content =
                fs::read_to_string(npmrc_path).map_err(|e| ApplyError::CommandFailed {
                    command: "read .npmrc template".to_string(),
                    path: npmrc_path.clone(),
                    message: e.to_string(),
                })?;
            let dest_path = project_path.join(".npmrc");
            fs::write(&dest_path, content).map_err(|e| ApplyError::CommandFailed {
                command: "write .npmrc".to_string(),
                path: dest_path.display().to_string(),
                message: e.to_string(),
            })?;
        }

        eprintln!("[pnpm] running pnpm install to generate lockfile...");
        run_command("pnpm install", project_path).map_err(|e| ApplyError::CommandFailed {
            command: e.command,
            path: e.path,
            message: e.message,
        })?;

        if !self.has_lockfile(project_path) {
            return Err(ApplyError::CommandFailed {
                command: "pnpm install".to_string(),
                path: project_path.display().to_string(),
                message: "lockfile was not generated".to_string(),
            });
        }

        Ok(())
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
    fn apply_project_updates(
        &self,
        project_path: &Path,
        packages: &[PackageUpdateRequest],
        _auth_config: Option<&AuthConfig>,
    ) -> Result<BatchApplyResult, ApplyError> {
        let mut results = Vec::new();
        let mut audit_fix_ran = false;
        let mut audit_fix_success = false;

        if self.run_audit_fix(project_path) {
            audit_fix_ran = true;
            audit_fix_success = true;
        }

        let direct_packages: Vec<&PackageUpdateRequest> = packages
            .iter()
            .filter(|p| p.scope != DependencyScope::Transitive)
            .collect();

        for pkg in &direct_packages {
            let flag = match pkg.dependency_type {
                DependencyType::DevDependency => "--save-dev",
                DependencyType::PeerDependency => "--save-peer",
                DependencyType::OptionalDependency => "--save-optional",
                DependencyType::Dependency => "--save",
            };

            let version_spec = match pkg.update_policy {
                UpdatePolicy::Exact => pkg.target_version.clone(),
                UpdatePolicy::Minimum => format!(">={}", pkg.target_version),
            };

            let install_cmd = format!("pnpm add {}@{} {}", pkg.package, version_spec, flag);
            run_command(&install_cmd, project_path).map_err(|e| ApplyError::CommandFailed {
                command: e.command,
                path: e.path,
                message: e.message,
            })?;
        }

        for pkg in packages {
            let current_version = self.get_version(project_path, &pkg.package);
            let target = Version::from(pkg.target_version.as_str());
            let version_matched = current_version
                .as_ref()
                .map(|v| v.satisfies(&target, &pkg.update_policy))
                .unwrap_or(false);

            results.push(ApplyResult {
                package: pkg.package.clone(),
                target_version: pkg.target_version.clone(),
                audit_fix_ran,
                audit_fix_success,
                version_matched,
                lockfile_deleted: false,
                node_modules_deleted: false,
                update_ran: false,
                final_status: if version_matched {
                    ApplyStatus::Success
                } else {
                    ApplyStatus::VersionMismatch
                },
                error_reason: if version_matched {
                    None
                } else {
                    Some("Version mismatch".to_string())
                },
            });
        }

        Ok(BatchApplyResult {
            results,
            audit_fix_ran,
            audit_fix_success,
            direct_installs_ran: !direct_packages.is_empty(),
            recovery_ran: false,
            node_modules_deleted: false,
            lockfile_deleted: false,
        })
    }
}

impl Pnpm {
    fn run_audit_fix(&self, project_path: &Path) -> bool {
        run_command("pnpm audit --fix", project_path).is_ok()
    }
}
