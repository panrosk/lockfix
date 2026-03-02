use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("project '{0}' has no package_manager and no global package_manager is set")]
    MissingPackageManager(String),

    #[error("project path does not exist: {0}")]
    ProjectDoesNotExist(String),

    #[error("package manager '{0}' is not installed or not found on PATH")]
    PackageManagerNotInstalled(String),

    #[error("project '{project}' is missing lockfile '{lockfile}'")]
    LockfileNotFound { project: String, lockfile: String },

    #[error("failed to read manifest: {0}")]
    ManifestRead(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub root: String,
    pub fix_branch_template: String,
    pub projects: Vec<Project>,
    pub package_manager: Option<PackageManager>,
    pub base_branch: String,
    pub git_user: Option<GitUser>,
    pub scm_config: Option<ScmConfig>,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for project in &self.projects {
            if self.package_manager.is_none() && project.package_manager.is_none() {
                return Err(ConfigError::MissingPackageManager(project.name.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Project {
    pub name: String,
    pub path: String,
    /// Takes precedence over Config.base_branch when set
    pub base_branch: Option<String>,
    /// Takes precedence over Config.package_manager when set
    pub package_manager: Option<PackageManager>,
    /// Per-field command overrides. Falls back to package manager defaults.
    pub commands: Option<Commands>,
    pub packages: Vec<PackageTarget>,
}

impl Project {
    /// Resolves final commands merging project overrides over package manager defaults.
    /// Each field is resolved independently so you can override just test without
    /// having to respecify install and build.
    pub fn resolved_commands(&self, pm: &PackageManager) -> Commands {
        let defaults = pm.default_commands();
        let overrides = self.commands.as_ref();

        Commands {
            install: overrides
                .and_then(|c| c.install.clone())
                .or(defaults.install),
            test: overrides.and_then(|c| c.test.clone()).or(defaults.test),
            build: overrides.and_then(|c| c.build.clone()).or(defaults.build),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm {
        npmrc_template: Option<String>,
        registry: Option<String>,
    },
    Yarn {
        yarnrc_template: Option<String>,
        registry: Option<String>,
    },
    Pnpm {
        npmrc_template: Option<String>,
        registry: Option<String>,
    },
}

impl PackageManager {
    pub fn name(&self) -> &str {
        match self {
            PackageManager::Npm { .. } => "npm",
            PackageManager::Yarn { .. } => "yarn",
            PackageManager::Pnpm { .. } => "pnpm",
        }
    }

    /// Returns the default commands for this package manager.
    pub fn default_commands(&self) -> Commands {
        match self {
            PackageManager::Npm { .. } => Commands {
                install: Some("npm install".into()),
                test: Some("npm test".into()),
                build: Some("npm run build".into()),
            },
            PackageManager::Yarn { .. } => Commands {
                install: Some("yarn".into()),
                test: Some("yarn test".into()),
                build: Some("yarn build".into()),
            },
            PackageManager::Pnpm { .. } => Commands {
                install: Some("pnpm install".into()),
                test: Some("pnpm test".into()),
                build: Some("pnpm run build".into()),
            },
        }
    }
}

/// Per-project command overrides.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Commands {
    pub install: Option<String>,
    pub test: Option<String>,
    pub build: Option<String>,
}

/// Describes a single package update target within a project.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackageTarget {
    pub name: String,
    pub target_version: String,
    pub update_policy: UpdatePolicy,
    pub scope: DependencyScope,
    pub dependency_type: DependencyType,
    pub required: bool,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    Exact,
    Minimum,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DependencyScope {
    Direct,
    Transitive,
    Auto,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum DependencyType {
    Dependency,
    DevDependency,
    PeerDependency,
    OptionalDependency,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum ScmConfig {
    Gitlab {
        url: Option<String>,
        token: Option<String>,
        create_merge_request: bool,
        target_branch: String,
    },
    Github {
        url: Option<String>,
        token: Option<String>,
        create_pull_request: bool,
        target_branch: String,
    },
    Local,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitUser {
    pub name: String,
    pub email: String,
    pub username: String,
}

#[cfg(test)]
mod tests;
