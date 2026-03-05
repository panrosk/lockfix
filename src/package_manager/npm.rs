use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::config::{Commands, DependencyScope, DependencyType, UpdatePolicy};
use crate::utils::run_command;

use super::{
    ApplyDriver, ApplyError, ApplyResult, ApplyStatus, AuthConfig, BatchApplyResult,
    LockfileDriver, PackageInstance, PackageUpdateRequest, Version,
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

    pub fn manifest_name(&self) -> &str {
        "package.json"
    }

    pub fn lockfile_name(&self) -> &str {
        "package-lock.json"
    }

    pub fn has_manifest(&self, project_path: &Path) -> bool {
        project_path.join(self.manifest_name()).exists()
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("npm").is_ok()
    }

    pub fn ensure_lockfile(&self, project_path: &Path) -> Result<(), ApplyError> {
        if self.has_lockfile(project_path) {
            return Ok(());
        }

        if let Some(npmrc_path) = &self.npmrc_template {
            eprintln!("[npm] copying .npmrc from: {}", npmrc_path);
            let content =
                fs::read_to_string(npmrc_path).map_err(|e| ApplyError::CommandFailed {
                    command: "read .npmrc template".to_string(),
                    path: npmrc_path.clone(),
                    message: e.to_string(),
                })?;
            let dest_path = project_path.join(".npmrc");
            eprintln!("[npm] writing .npmrc to: {}", dest_path.display());
            fs::write(&dest_path, content).map_err(|e| ApplyError::CommandFailed {
                command: "write .npmrc".to_string(),
                path: dest_path.display().to_string(),
                message: e.to_string(),
            })?;
        } else {
            eprintln!("[npm] no .npmrc template configured");
        }

        eprintln!("[npm] running npm install to generate lockfile...");
        run_command("npm install", project_path).map_err(|e| ApplyError::CommandFailed {
            command: e.command,
            path: e.path,
            message: e.message,
        })?;

        if !self.has_lockfile(project_path) {
            return Err(ApplyError::CommandFailed {
                command: "npm install".to_string(),
                path: project_path.display().to_string(),
                message: "lockfile was not generated".to_string(),
            });
        }

        Ok(())
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
    fn apply_project_updates(
        &self,
        project_path: &Path,
        packages: &[PackageUpdateRequest],
        auth_config: Option<&AuthConfig>,
    ) -> Result<BatchApplyResult, ApplyError> {
        eprintln!(
            "      [npm] starting batch apply for {} packages",
            packages.len()
        );

        let mut batch_result = BatchApplyResult {
            results: Vec::new(),
            audit_fix_ran: false,
            audit_fix_success: false,
            direct_installs_ran: false,
            recovery_ran: false,
            node_modules_deleted: false,
            lockfile_deleted: false,
        };

        if let Some(auth) = auth_config {
            eprintln!("      [npm] setting up auth...");
            self.setup_auth(project_path, auth)?;
        }

        let direct_packages: Vec<&PackageUpdateRequest> = packages
            .iter()
            .filter(|p| p.scope != DependencyScope::Transitive)
            .collect();

        let transitive_packages: Vec<&PackageUpdateRequest> = packages
            .iter()
            .filter(|p| p.scope == DependencyScope::Transitive)
            .collect();

        eprintln!(
            "      [npm] direct: {}, transitive: {}",
            direct_packages.len(),
            transitive_packages.len()
        );

        for pkg in &direct_packages {
            eprintln!(
                "      [npm] direct: {} @ {}",
                pkg.package, pkg.target_version
            );
        }
        for pkg in &transitive_packages {
            eprintln!(
                "      [npm] transitive: {} @ {}",
                pkg.package, pkg.target_version
            );
        }

        eprintln!("      [npm] === PHASE 1: audit fix ===");
        batch_result.audit_fix_ran = true;
        if let Some(audit_result) = self.run_audit_fix(project_path) {
            batch_result.audit_fix_success = audit_result.is_ok();
            eprintln!(
                "      [npm] audit fix: {}",
                if batch_result.audit_fix_success {
                    "success"
                } else {
                    "failed"
                }
            );
        }

        eprintln!("      [npm] === PHASE 2: check all versions ===");
        let all_matched = self.check_all_versions(project_path, packages);
        eprintln!("      [npm] all versions matched: {}", all_matched);

        if all_matched {
            batch_result.results = self.build_success_results(packages);
            return Ok(batch_result);
        }

        eprintln!("      [npm] === PHASE 3: install direct dependencies ===");
        batch_result.direct_installs_ran = true;
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

            let install_cmd = format!("npm install {}@{} {}", pkg.package, version_spec, flag);
            eprintln!("      [npm] running: {}", install_cmd);
            run_command(&install_cmd, project_path).map_err(|e| ApplyError::CommandFailed {
                command: e.command,
                path: e.path,
                message: e.message,
            })?;
        }

        eprintln!("      [npm] === PHASE 4: check all versions ===");
        let all_matched = self.check_all_versions(project_path, packages);
        eprintln!("      [npm] all versions matched: {}", all_matched);

        if all_matched {
            batch_result.results = self.build_partial_success_results(packages);
            return Ok(batch_result);
        }

        eprintln!("      [npm] === PHASE 5: recovery - delete node_modules and lockfile ===");
        batch_result.recovery_ran = true;

        let node_modules = project_path.join("node_modules");
        if node_modules.exists() {
            eprintln!("      [npm] deleting node_modules...");
            fs::remove_dir_all(&node_modules).ok();
            batch_result.node_modules_deleted = true;
        }

        let lockfile_path = project_path.join("package-lock.json");
        if lockfile_path.exists() {
            eprintln!("      [npm] deleting package-lock.json...");
            fs::remove_file(&lockfile_path).ok();
            batch_result.lockfile_deleted = true;
        }

        eprintln!("      [npm] === PHASE 6: npm install && npm update ===");
        eprintln!("      [npm] running npm install...");
        run_command("npm install", project_path).ok();
        eprintln!("      [npm] running npm update...");
        run_command("npm update", project_path).ok();

        eprintln!("      [npm] === PHASE 7: final version check ===");
        batch_result.results = self.build_final_results(project_path, packages);

        eprintln!("      [npm] batch apply complete");
        Ok(batch_result)
    }
}

impl Npm {
    fn run_audit_fix(&self, project_path: &Path) -> Option<Result<(), ApplyError>> {
        Some(
            run_command("npm audit fix", project_path).map_err(|e| ApplyError::CommandFailed {
                command: e.command,
                path: e.path,
                message: e.message,
            }),
        )
    }

    fn check_all_versions(&self, project_path: &Path, packages: &[PackageUpdateRequest]) -> bool {
        packages.iter().all(|pkg| {
            let current = self.get_version(project_path, &pkg.package);
            let target = Version::from(pkg.target_version.as_str());
            let matched = current
                .as_ref()
                .map(|v| v.satisfies(&target, &pkg.update_policy))
                .unwrap_or(false);
            if !matched {
                eprintln!(
                    "      [npm] {} mismatch: current={:?}, target={}, policy={:?}",
                    pkg.package,
                    current.as_ref().map(|v| v.as_str()),
                    pkg.target_version,
                    pkg.update_policy
                );
            }
            matched
        })
    }

    fn build_success_results(&self, packages: &[PackageUpdateRequest]) -> Vec<ApplyResult> {
        packages
            .iter()
            .map(|pkg| ApplyResult {
                package: pkg.package.clone(),
                target_version: pkg.target_version.clone(),
                audit_fix_ran: true,
                audit_fix_success: true,
                version_matched: true,
                lockfile_deleted: false,
                node_modules_deleted: false,
                update_ran: false,
                final_status: ApplyStatus::Success,
                error_reason: None,
            })
            .collect()
    }

    fn build_partial_success_results(&self, packages: &[PackageUpdateRequest]) -> Vec<ApplyResult> {
        packages
            .iter()
            .map(|pkg| ApplyResult {
                package: pkg.package.clone(),
                target_version: pkg.target_version.clone(),
                audit_fix_ran: true,
                audit_fix_success: true,
                version_matched: true,
                lockfile_deleted: false,
                node_modules_deleted: false,
                update_ran: false,
                final_status: ApplyStatus::PartialSuccess,
                error_reason: None,
            })
            .collect()
    }

    fn build_final_results(
        &self,
        project_path: &Path,
        packages: &[PackageUpdateRequest],
    ) -> Vec<ApplyResult> {
        packages
            .iter()
            .map(|pkg| {
                let current = self.get_version(project_path, &pkg.package);
                let target = Version::from(pkg.target_version.as_str());
                let matched = current
                    .as_ref()
                    .map(|v| v.satisfies(&target, &pkg.update_policy))
                    .unwrap_or(false);
                eprintln!(
                    "      [npm] {} final check: current={:?}, target={}, policy={:?}, matched={}",
                    pkg.package,
                    current.as_ref().map(|v| v.as_str()),
                    pkg.target_version,
                    pkg.update_policy,
                    matched
                );
                ApplyResult {
                    package: pkg.package.clone(),
                    target_version: pkg.target_version.clone(),
                    audit_fix_ran: true,
                    audit_fix_success: true,
                    version_matched: matched,
                    lockfile_deleted: true,
                    node_modules_deleted: true,
                    update_ran: true,
                    final_status: if matched {
                        ApplyStatus::PartialSuccess
                    } else {
                        ApplyStatus::VersionMismatch
                    },
                    error_reason: if matched {
                        None
                    } else {
                        Some("Version mismatch after recovery".to_string())
                    },
                }
            })
            .collect()
    }
}
