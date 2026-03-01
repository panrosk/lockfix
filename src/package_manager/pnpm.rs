use std::path::Path;

use crate::config::Commands;

use super::{LockfileDriver, PackageInstance};

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
        // TODO: parse pnpm-lock.yaml and return all instances of the package
        todo!("pnpm lockfile parsing not yet implemented")
    }
}
