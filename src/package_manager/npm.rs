use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use crate::config::Commands;

use super::{LockfileDriver, PackageInstance, Version};

pub struct Npm {
    pub npmrc_template: Option<String>,
    pub registry: Option<String>,
}

// --- lockfile structs ---

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

// --- driver ---

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
