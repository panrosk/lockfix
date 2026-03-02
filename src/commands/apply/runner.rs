use std::path::PathBuf;

use thiserror::Error;

use crate::commands::plan::model::PlannedAction;
use crate::config::{Config, ConfigError};
use crate::package_manager::{
    package_json::PackageJson, ApplyContext, ApplyDriver, ApplyResult, ApplyStatus,
    PackageManagerKind,
};
use crate::scm::{Git, GitError};

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("git error: {0}")]
    Git(#[from] GitError),

    #[error("package.json error: {0}")]
    PackageJsonRead(#[from] crate::package_manager::package_json::PackageJsonError),

    #[error("apply error: {0}")]
    Apply(#[from] crate::package_manager::ApplyError),

    #[error("no git user configured")]
    NoGitUser,

    #[error("version mismatch for required package '{package}' in project '{project}'")]
    VersionMismatch { project: String, package: String },

    #[error("failed to read plan file: {0}")]
    PlanRead(#[from] std::io::Error),

    #[error("failed to parse plan: {0}")]
    PlanParse(#[from] serde_json::Error),
}

#[derive(Debug)]
pub struct ProjectSummary {
    pub name: String,
    pub results: Vec<ApplyResult>,
    pub committed: bool,
}

impl ProjectSummary {
    pub fn success_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.final_status == ApplyStatus::Success)
            .count()
    }

    pub fn partial_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.final_status == ApplyStatus::PartialSuccess)
            .count()
    }

    pub fn mismatch_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.final_status == ApplyStatus::VersionMismatch)
            .count()
    }

    pub fn error_count(&self) -> usize {
        self.results
            .iter()
            .filter(|r| r.final_status == ApplyStatus::PlannedError)
            .count()
    }

    pub fn all_success(&self) -> bool {
        self.results
            .iter()
            .all(|r| r.final_status == ApplyStatus::Success)
    }
}

#[derive(Debug)]
pub struct ApplySummary {
    pub projects: Vec<ProjectSummary>,
}

impl ApplySummary {
    pub fn total_packages(&self) -> usize {
        self.projects.iter().map(|p| p.results.len()).sum()
    }

    pub fn total_success(&self) -> usize {
        self.projects.iter().map(|p| p.success_count()).sum()
    }

    pub fn total_partial(&self) -> usize {
        self.projects.iter().map(|p| p.partial_count()).sum()
    }

    pub fn total_mismatch(&self) -> usize {
        self.projects.iter().map(|p| p.mismatch_count()).sum()
    }

    pub fn total_errors(&self) -> usize {
        self.projects.iter().map(|p| p.error_count()).sum()
    }

    pub fn print(&self) {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("                      APPLY SUMMARY                         ");
        println!("═══════════════════════════════════════════════════════════");
        println!();

        for project in &self.projects {
            let status_icon = if project.all_success() { "✓" } else { "✗" };
            let committed = if project.committed {
                "committed"
            } else {
                "not committed"
            };

            println!(
                "{} {} ({}) - {} success, {} partial, {} mismatch, {} error",
                status_icon,
                project.name,
                committed,
                project.success_count(),
                project.partial_count(),
                project.mismatch_count(),
                project.error_count()
            );

            for result in &project.results {
                let icon = match result.final_status {
                    ApplyStatus::Success => "  ✓",
                    ApplyStatus::PartialSuccess => "  ⚠",
                    ApplyStatus::VersionMismatch => "  ✗",
                    ApplyStatus::PlannedError => "  !",
                };
                let status = match result.final_status {
                    ApplyStatus::Success => "success",
                    ApplyStatus::PartialSuccess => "partial",
                    ApplyStatus::VersionMismatch => "mismatch",
                    ApplyStatus::PlannedError => "error",
                };
                if let Some(ref reason) = result.error_reason {
                    println!(
                        "{} {} @ {} [{}]: {}",
                        icon, result.package, result.target_version, status, reason
                    );
                } else {
                    println!(
                        "{} {} @ {} [{}]",
                        icon, result.package, result.target_version, status
                    );
                }
            }
            println!();
        }

        println!("───────────────────────────────────────────────────────────");
        println!(
            "Total: {} packages | {} success | {} partial | {} mismatch | {} error",
            self.total_packages(),
            self.total_success(),
            self.total_partial(),
            self.total_mismatch(),
            self.total_errors()
        );
        println!("═══════════════════════════════════════════════════════════");
    }
}

pub fn run(config_path: Option<&str>, plan_path: Option<&str>) -> Result<(), ApplyError> {
    match (config_path, plan_path) {
        (Some(path), None) => run_from_config(path),
        (None, Some(path)) => run_from_plan(path),
        _ => Err(ApplyError::Config(ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "either config or plan must be provided",
        )))),
    }
}

fn run_from_config(config_path: &str) -> Result<(), ApplyError> {
    let config = Config::from_file(config_path)?;
    let git_user = config.git_user.as_ref().ok_or(ApplyError::NoGitUser)?;
    let root_path = PathBuf::from(&config.root);
    let mut summary = ApplySummary {
        projects: Vec::new(),
    };

    for project in &config.projects {
        let config_pm = project
            .package_manager
            .as_ref()
            .or(config.package_manager.as_ref())
            .unwrap();

        let pm = PackageManagerKind::from_config(config_pm);
        let project_path = resolve_project_path(&root_path, &project.path);

        let base_branch = project
            .base_branch
            .clone()
            .unwrap_or_else(|| config.base_branch.clone());

        let fix_branch = config
            .fix_branch_template
            .replace("{projectName}", &project.name);

        let git = Git::open(&project_path, &git_user.name, &git_user.email)?;
        git.fetch(&base_branch)?;
        git.checkout_and_reset(&base_branch)?;
        git.create_and_checkout_branch(&fix_branch)?;

        let mut pkg_json = PackageJson::from_path(&project_path)?;

        for pkg in &project.packages {
            pkg_json.set_version(&pkg.name, &pkg.target_version);
        }

        pkg_json.write(&project_path)?;

        let mut results = Vec::new();

        for pkg in &project.packages {
            let ctx = ApplyContext {
                project_path: &project_path,
                package: &pkg.name,
                target_version: &pkg.target_version,
                dependency_type: pkg.dependency_type.clone(),
                update_policy: pkg.update_policy.clone(),
                scope: pkg.scope.clone(),
                auth_config: None,
            };

            let result = pm.apply_update(&ctx)?;

            if result.final_status == ApplyStatus::VersionMismatch && pkg.required {
                summary.projects.push(ProjectSummary {
                    name: project.name.clone(),
                    results: results.clone(),
                    committed: false,
                });
                summary.print();
                return Err(ApplyError::VersionMismatch {
                    project: project.name.clone(),
                    package: pkg.name.clone(),
                });
            }

            results.push(result);
        }

        let project_summary = ProjectSummary {
            name: project.name.clone(),
            results: results.clone(),
            committed: false,
        };

        let all_success = project_summary.all_success();

        if all_success {
            let commit_message = format!("fix: update dependencies for {}", project.name);
            git.stage_and_commit(&commit_message)?;
            git.push(&fix_branch)?;
        }

        summary.projects.push(ProjectSummary {
            committed: all_success,
            ..project_summary
        });
    }

    summary.print();
    Ok(())
}

fn run_from_plan(plan_path: &str) -> Result<(), ApplyError> {
    let content = std::fs::read_to_string(plan_path)?;
    let plan: crate::commands::plan::model::Plan = serde_json::from_str(&content)?;

    let config = Config::from_file(&plan.config_path)?;
    let git_user = config.git_user.as_ref().ok_or(ApplyError::NoGitUser)?;
    let root_path = PathBuf::from(&config.root);
    let mut summary = ApplySummary {
        projects: Vec::new(),
    };

    for project_plan in &plan.projects {
        let project_path = resolve_project_path(&root_path, &project_plan.path);

        let config_pm = config
            .package_manager
            .as_ref()
            .or_else(|| {
                config
                    .projects
                    .iter()
                    .find(|p| p.name == project_plan.name)
                    .and_then(|p| p.package_manager.as_ref())
            })
            .unwrap();

        let pm = PackageManagerKind::from_config(config_pm);

        let git = Git::open(&project_path, &git_user.name, &git_user.email)?;
        git.fetch(&project_plan.base_branch)?;
        git.checkout_and_reset(&project_plan.base_branch)?;
        git.create_and_checkout_branch(&project_plan.fix_branch)?;

        let mut pkg_json = PackageJson::from_path(&project_path)?;
        let mut results = Vec::new();
        let mut needs_commit = false;

        for pkg in &project_plan.packages {
            match &pkg.action {
                PlannedAction::Skip => {
                    results.push(ApplyResult {
                        package: pkg.name.clone(),
                        target_version: pkg.target_version.clone(),
                        audit_fix_ran: false,
                        audit_fix_success: false,
                        version_matched: true,
                        lockfile_deleted: false,
                        node_modules_deleted: false,
                        update_ran: false,
                        final_status: ApplyStatus::Success,
                        error_reason: None,
                    });
                }
                PlannedAction::Error { reason } => {
                    results.push(ApplyResult {
                        package: pkg.name.clone(),
                        target_version: pkg.target_version.clone(),
                        audit_fix_ran: false,
                        audit_fix_success: false,
                        version_matched: false,
                        lockfile_deleted: false,
                        node_modules_deleted: false,
                        update_ran: false,
                        final_status: ApplyStatus::PlannedError,
                        error_reason: Some(reason.clone()),
                    });
                }
                PlannedAction::Update | PlannedAction::Add | PlannedAction::Pending => {
                    pkg_json.set_version(&pkg.name, &pkg.target_version);
                    needs_commit = true;

                    let ctx = ApplyContext {
                        project_path: &project_path,
                        package: &pkg.name,
                        target_version: &pkg.target_version,
                        dependency_type: pkg.dependency_type.clone(),
                        update_policy: pkg.update_policy.clone(),
                        scope: pkg.scope.clone(),
                        auth_config: None,
                    };

                    let result = pm.apply_update(&ctx)?;

                    if result.final_status == ApplyStatus::VersionMismatch && pkg.required {
                        summary.projects.push(ProjectSummary {
                            name: project_plan.name.clone(),
                            results: results.clone(),
                            committed: false,
                        });
                        summary.print();
                        return Err(ApplyError::VersionMismatch {
                            project: project_plan.name.clone(),
                            package: pkg.name.clone(),
                        });
                    }

                    results.push(result);
                }
            }
        }

        if needs_commit {
            pkg_json.write(&project_path)?;
        }

        let project_summary = ProjectSummary {
            name: project_plan.name.clone(),
            results: results.clone(),
            committed: false,
        };

        let all_success = project_summary.all_success();

        if all_success && needs_commit {
            let commit_message = format!("fix: update dependencies for {}", project_plan.name);
            git.stage_and_commit(&commit_message)?;
            git.push(&project_plan.fix_branch)?;
        }

        summary.projects.push(ProjectSummary {
            committed: all_success && needs_commit,
            ..project_summary
        });
    }

    summary.print();
    Ok(())
}

fn resolve_project_path(root: &PathBuf, project_path: &str) -> PathBuf {
    let p = PathBuf::from(project_path);
    if p.is_relative() {
        root.join(project_path)
    } else {
        p
    }
}
