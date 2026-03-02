use std::path::Path;

use crate::config::Commands;

use super::{
    ApplyContext, ApplyDriver, ApplyError, ApplyResult, ApplyStatus, LockfileDriver,
    PackageInstance,
};

pub struct Yarn {
    pub yarnrc_template: Option<String>,
    pub registry: Option<String>,
}

impl Yarn {
    pub fn name(&self) -> &str {
        "yarn"
    }

    pub fn lockfile_name(&self) -> &str {
        "yarn.lock"
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        project_path.join(self.lockfile_name()).exists()
    }

    pub fn is_installed(&self) -> bool {
        which::which("yarn").is_ok()
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
    fn apply_update(&self, ctx: &ApplyContext) -> Result<ApplyResult, ApplyError> {
        Ok(ApplyResult {
            package: ctx.package.to_string(),
            target_version: ctx.target_version.to_string(),
            audit_fix_ran: false,
            audit_fix_success: false,
            version_matched: true,
            lockfile_deleted: false,
            node_modules_deleted: false,
            update_ran: false,
            final_status: ApplyStatus::Success,
            error_reason: None,
        })
    }

    fn audit_fix(&self, _project_path: &Path) -> Option<Result<(), ApplyError>> {
        None
    }

    fn supports_audit_fix(&self) -> bool {
        false
    }
}
