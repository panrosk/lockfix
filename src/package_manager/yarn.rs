use std::path::Path;

use crate::config::Commands;

use super::{LockfileDriver, PackageInstance};

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
        // TODO: parse yarn.lock (custom format) and return all instances of the package
        todo!("yarn lockfile parsing not yet implemented")
    }
}
