pub mod npm;
pub mod package_json;
mod pnpm;
mod yarn;

#[cfg(test)]
mod tests;

use std::path::Path;

pub use npm::Npm;
pub use pnpm::Pnpm;
pub use yarn::Yarn;

use crate::config::{Commands, PackageManager as ConfigPackageManager};

/// Newtype representing a package version string.
/// Kept as a newtype so version-comparison and semver logic can be added later.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Version(pub String);

impl Version {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Version {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Version {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A single installation of a package found in the lockfile.
pub struct PackageInstance {
    pub path: String,
    pub version: Version,
}

/// Every package manager must implement this trait so the plan runner can
/// resolve lockfile information uniformly regardless of the PM.
pub trait LockfileDriver {
    /// Returns all instances (path + version) of a package found anywhere in
    /// the lockfile, including nested copies under other packages.
    fn get_all_instances(&self, project_path: &Path, name: &str) -> Vec<PackageInstance>;

    /// Returns the top-level resolved version of a package derived from
    /// get_all_instances — no need to override this in each implementation.
    fn get_version(&self, project_path: &Path, name: &str) -> Option<Version> {
        let top_level = format!("node_modules/{name}");
        self.get_all_instances(project_path, name)
            .into_iter()
            .find(|i| i.path == top_level)
            .map(|i| i.version)
    }
}

/// Runtime wrapper enum that bridges config::PackageManager (serializable)
/// to actual package manager behaviour (lockfile detection, binary checks, commands).
pub enum PackageManagerKind {
    Npm(Npm),
    Yarn(Yarn),
    Pnpm(Pnpm),
}

impl PackageManagerKind {
    pub fn from_config(config: &ConfigPackageManager) -> Self {
        match config {
            ConfigPackageManager::Npm {
                npmrc_template,
                registry,
            } => Self::Npm(Npm {
                npmrc_template: npmrc_template.clone(),
                registry: registry.clone(),
            }),
            ConfigPackageManager::Yarn {
                yarnrc_template,
                registry,
            } => Self::Yarn(Yarn {
                yarnrc_template: yarnrc_template.clone(),
                registry: registry.clone(),
            }),
            ConfigPackageManager::Pnpm {
                npmrc_template,
                registry,
            } => Self::Pnpm(Pnpm {
                npmrc_template: npmrc_template.clone(),
                registry: registry.clone(),
            }),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Npm(n) => n.name(),
            Self::Yarn(y) => y.name(),
            Self::Pnpm(p) => p.name(),
        }
    }

    pub fn lockfile_name(&self) -> &str {
        match self {
            Self::Npm(n) => n.lockfile_name(),
            Self::Yarn(y) => y.lockfile_name(),
            Self::Pnpm(p) => p.lockfile_name(),
        }
    }

    pub fn has_lockfile(&self, project_path: &Path) -> bool {
        match self {
            Self::Npm(n) => n.has_lockfile(project_path),
            Self::Yarn(y) => y.has_lockfile(project_path),
            Self::Pnpm(p) => p.has_lockfile(project_path),
        }
    }

    pub fn is_installed(&self) -> bool {
        match self {
            Self::Npm(n) => n.is_installed(),
            Self::Yarn(y) => y.is_installed(),
            Self::Pnpm(p) => p.is_installed(),
        }
    }

    pub fn default_commands(&self) -> Commands {
        match self {
            Self::Npm(n) => n.default_commands(),
            Self::Yarn(y) => y.default_commands(),
            Self::Pnpm(p) => p.default_commands(),
        }
    }
}

impl LockfileDriver for PackageManagerKind {
    fn get_all_instances(&self, project_path: &Path, name: &str) -> Vec<PackageInstance> {
        match self {
            Self::Npm(n) => n.get_all_instances(project_path, name),
            Self::Yarn(y) => y.get_all_instances(project_path, name),
            Self::Pnpm(p) => p.get_all_instances(project_path, name),
        }
    }
}
