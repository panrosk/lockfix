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

use crate::config::{
    Commands, DependencyScope, DependencyType, PackageManager as ConfigPackageManager, UpdatePolicy,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Version(pub String);

impl Version {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn parse_parts(&self) -> Vec<u64> {
        let clean = self
            .0
            .trim_start_matches(|c| c == 'v' || c == '^' || c == '~');
        clean.split('.').filter_map(|s| s.parse().ok()).collect()
    }

    pub fn cmp_versions(&self, other: &Version) -> std::cmp::Ordering {
        let self_parts = self.parse_parts();
        let other_parts = other.parse_parts();

        let max_len = self_parts.len().max(other_parts.len());
        for i in 0..max_len {
            let a = self_parts.get(i).copied().unwrap_or(0);
            let b = other_parts.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                std::cmp::Ordering::Equal => continue,
                other => return other,
            }
        }
        std::cmp::Ordering::Equal
    }

    pub fn satisfies(&self, target: &Version, policy: &UpdatePolicy) -> bool {
        match policy {
            UpdatePolicy::Minimum => self.cmp_versions(target) != std::cmp::Ordering::Less,
            UpdatePolicy::Exact => self.cmp_versions(target) == std::cmp::Ordering::Equal,
        }
    }

    pub fn is_downgrade(&self, target: &Version) -> bool {
        self.cmp_versions(target) == std::cmp::Ordering::Greater
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

pub struct PackageInstance {
    pub path: String,
    pub version: Version,
}

pub trait LockfileDriver {
    fn get_all_instances(&self, project_path: &Path, name: &str) -> Vec<PackageInstance>;

    fn get_version(&self, project_path: &Path, name: &str) -> Option<Version> {
        let top_level = format!("node_modules/{name}");
        self.get_all_instances(project_path, name)
            .into_iter()
            .find(|i| i.path == top_level)
            .map(|i| i.version)
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub auth_template: Option<String>,
    pub registry: Option<String>,
    pub auth_type: String,
}

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub package: String,
    pub target_version: String,
    pub audit_fix_ran: bool,
    pub audit_fix_success: bool,
    pub version_matched: bool,
    pub lockfile_deleted: bool,
    pub node_modules_deleted: bool,
    pub update_ran: bool,
    pub final_status: ApplyStatus,
    pub error_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageUpdateRequest {
    pub package: String,
    pub target_version: String,
    pub dependency_type: DependencyType,
    pub update_policy: UpdatePolicy,
    pub scope: DependencyScope,
}

#[derive(Debug, Clone)]
pub struct BatchApplyResult {
    pub results: Vec<ApplyResult>,
    pub audit_fix_ran: bool,
    pub audit_fix_success: bool,
    pub direct_installs_ran: bool,
    pub recovery_ran: bool,
    pub node_modules_deleted: bool,
    pub lockfile_deleted: bool,
}

impl BatchApplyResult {
    pub fn all_success(&self) -> bool {
        self.results
            .iter()
            .all(|r| r.final_status == ApplyStatus::Success)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyStatus {
    Success,
    VersionMismatch,
    PartialSuccess,
    PlannedError,
}

pub trait ApplyDriver {
    fn apply_project_updates(
        &self,
        project_path: &Path,
        packages: &[PackageUpdateRequest],
        auth_config: Option<&AuthConfig>,
    ) -> Result<BatchApplyResult, ApplyError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("failed to run command '{command}' in '{path}': {message}")]
    CommandFailed {
        command: String,
        path: String,
        message: String,
    },

    #[error("failed to read package.json: {0}")]
    PackageJsonRead(#[from] package_json::PackageJsonError),

    #[error("failed to write package.json: {0}")]
    PackageJsonWrite(String),
}

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

    pub fn get_auth_config(&self) -> Option<AuthConfig> {
        match self {
            Self::Npm(npm) => Some(AuthConfig {
                auth_template: npm.npmrc_template.clone(),
                registry: npm.registry.clone(),
                auth_type: "npmrc".to_string(),
            }),
            Self::Yarn(yarn) => Some(AuthConfig {
                auth_template: yarn.yarnrc_template.clone(),
                registry: yarn.registry.clone(),
                auth_type: "yarnrc".to_string(),
            }),
            Self::Pnpm(pnpm) => Some(AuthConfig {
                auth_template: pnpm.npmrc_template.clone(),
                registry: pnpm.registry.clone(),
                auth_type: "npmrc".to_string(),
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

    pub fn manifest_name(&self) -> &str {
        match self {
            Self::Npm(n) => n.manifest_name(),
            Self::Yarn(y) => y.manifest_name(),
            Self::Pnpm(p) => p.manifest_name(),
        }
    }

    pub fn lockfile_name(&self) -> &str {
        match self {
            Self::Npm(n) => n.lockfile_name(),
            Self::Yarn(y) => y.lockfile_name(),
            Self::Pnpm(p) => p.lockfile_name(),
        }
    }

    pub fn has_manifest(&self, project_path: &Path) -> bool {
        match self {
            Self::Npm(n) => n.has_manifest(project_path),
            Self::Yarn(y) => y.has_manifest(project_path),
            Self::Pnpm(p) => p.has_manifest(project_path),
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

    pub fn ensure_lockfile(&self, project_path: &Path) -> Result<(), ApplyError> {
        match self {
            Self::Npm(n) => n.ensure_lockfile(project_path),
            Self::Yarn(y) => y.ensure_lockfile(project_path),
            Self::Pnpm(p) => p.ensure_lockfile(project_path),
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

impl ApplyDriver for PackageManagerKind {
    fn apply_project_updates(
        &self,
        project_path: &Path,
        packages: &[PackageUpdateRequest],
        auth_config: Option<&AuthConfig>,
    ) -> Result<BatchApplyResult, ApplyError> {
        match self {
            Self::Npm(n) => n.apply_project_updates(project_path, packages, auth_config),
            Self::Yarn(y) => y.apply_project_updates(project_path, packages, auth_config),
            Self::Pnpm(p) => p.apply_project_updates(project_path, packages, auth_config),
        }
    }
}
