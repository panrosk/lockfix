use serde::{Deserialize, Serialize};

use crate::config::{DependencyScope, DependencyType, UpdatePolicy};

/// Top-level plan produced before any changes are made.
/// Serialized to a JSON file that apply reads.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// ISO 8601 timestamp of when this plan was generated.
    pub generated_at: String,
    /// Path to the lockfix config file used to produce this plan.
    pub config_path: String,
    pub summary: PlanSummary,
    pub projects: Vec<ProjectPlan>,
}

/// High-level counts for quick inspection.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub total_projects: usize,
    pub total_packages: usize,
    /// Packages with required: true — failure here blocks the whole project.
    pub required_packages: usize,
}

/// Resolved plan for a single project.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPlan {
    pub name: String,
    pub path: String,
    /// Rendered fix branch name e.g. "fix/generic-login-api-vulnerabilities"
    pub fix_branch: String,
    /// Resolved base branch (project override or global fallback)
    pub base_branch: String,
    /// Resolved package manager name e.g. "npm"
    pub package_manager: String,
    pub packages: Vec<PackagePlan>,
}

/// Resolved plan for a single package within a project.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackagePlan {
    pub name: String,
    /// Top-level resolved version from the lockfile. None if not installed.
    pub current_version: Option<String>,
    pub target_version: String,
    pub update_policy: UpdatePolicy,
    pub action: PlannedAction,
    pub scope: DependencyScope,
    pub dependency_type: DependencyType,
    pub required: bool,
    /// Why this package was included. e.g. "security-fix", "manual-request"
    pub reason: String,
    /// All copies of this package found in the lockfile.
    /// More than one entry means the package is installed at multiple versions.
    pub instances: Vec<PackageInstance>,
}

/// A single installation of a package found in the lockfile.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PackageInstance {
    /// Lockfile key e.g. "node_modules/axios" or "node_modules/foo/node_modules/axios"
    pub path: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PlannedAction {
    Update,
    Add,
    Skip,
    Pending,
    Error { reason: String },
}
