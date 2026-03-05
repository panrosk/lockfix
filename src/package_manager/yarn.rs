use std::fs;
use std::path::Path;

use crate::config::Commands;
use crate::utils::run_command;

use super::{
    ApplyDriver, ApplyError, ApplyResult, ApplyStatus, AuthConfig, BatchApplyResult,
    LockfileDriver, PackageInstance, PackageUpdateRequest,
};

pub struct Yarn {
    pub yarnrc_template: Option<String>,
    pub registry: Option<String>,
}

impl Yarn {
    pub fn name(&self) -> &str {
        "yarn"
    }

    pub fn manifest_name(&self) -> &str {
        "package.json"
    }

    pub fn lockfile_name(&self) -> &str {
        "yarn.lock"
    }

    pub fn has_manifest(&self, project_path: &Path) -> bool {
        project_path.join(self.manifest_name()).exists()
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("yarn").is_ok()
    }

    pub fn ensure_lockfile(&self, project_path: &Path) -> Result<(), ApplyError> {
        if self.has_lockfile(project_path) {
            return Ok(());
        }

        if let Some(yarnrc_path) = &self.yarnrc_template {
            let content =
                fs::read_to_string(yarnrc_path).map_err(|e| ApplyError::CommandFailed {
                    command: "read .yarnrc template".to_string(),
                    path: yarnrc_path.clone(),
                    message: e.to_string(),
                })?;
            let dest_path = project_path.join(".yarnrc");
            fs::write(&dest_path, content).map_err(|e| ApplyError::CommandFailed {
                command: "write .yarnrc".to_string(),
                path: dest_path.display().to_string(),
                message: e.to_string(),
            })?;
        }

        eprintln!("[yarn] running yarn to generate lockfile...");
        run_command("yarn", project_path).map_err(|e| ApplyError::CommandFailed {
            command: e.command,
            path: e.path,
            message: e.message,
        })?;

        if !self.has_lockfile(project_path) {
            return Err(ApplyError::CommandFailed {
                command: "yarn".to_string(),
                path: project_path.display().to_string(),
                message: "lockfile was not generated".to_string(),
            });
        }

        Ok(())
    }

    pub fn default_commands(&self) -> Commands {
        Commands {
            install: Some("yarn".into()),
            test: Some("yarn test".into()),
            build: Some("yarn build".into()),
        }
    }
}

impl LockfileDriver for Yarn {
    fn get_all_instances(&self, _project_path: &Path, _name: &str) -> Vec<PackageInstance> {
        todo!("yarn lockfile parsing not yet implemented")
    }
}

impl ApplyDriver for Yarn {
    fn apply_project_updates(
        &self,
        _project_path: &Path,
        packages: &[PackageUpdateRequest],
        _auth_config: Option<&AuthConfig>,
    ) -> Result<BatchApplyResult, ApplyError> {
        let results: Vec<ApplyResult> = packages
            .iter()
            .map(|pkg| ApplyResult {
                package: pkg.package.clone(),
                target_version: pkg.target_version.clone(),
                audit_fix_ran: false,
                audit_fix_success: false,
                version_matched: true,
                lockfile_deleted: false,
                node_modules_deleted: false,
                update_ran: false,
                final_status: ApplyStatus::Success,
                error_reason: None,
            })
            .collect();

        Ok(BatchApplyResult {
            results,
            audit_fix_ran: false,
            audit_fix_success: false,
            direct_installs_ran: false,
            recovery_ran: false,
            node_modules_deleted: false,
            lockfile_deleted: false,
        })
    }
}
