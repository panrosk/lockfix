use std::path::PathBuf;

use thiserror::Error;

use crate::commands::plan::model::PlannedAction;
use crate::config::{Config, ConfigError, ScmConfig};
use crate::package_manager::{
    package_json::PackageJson, ApplyDriver, ApplyResult, ApplyStatus, PackageManagerKind,
    PackageUpdateRequest,
};
use crate::scm::{Git, GitError, GitLabClient, GitLabConfig};

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("git error: {0}")]
    Git(#[from] GitError),

    #[error("gitlab error: {0}")]
    GitLab(#[from] crate::scm::GitLabError),

    #[error("package.json error: {0}")]
    PackageJsonRead(#[from] crate::package_manager::package_json::PackageJsonError),

    #[error("apply error: {0}")]
    Apply(#[from] crate::package_manager::ApplyError),

    #[error("no git user configured")]
    NoGitUser,

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

fn is_local_mode(config: &Config) -> bool {
    matches!(config.scm_config, Some(ScmConfig::Local))
}

fn get_token(config: &Config) -> Option<String> {
    match &config.scm_config {
        Some(ScmConfig::Gitlab { token, .. }) => {
            token.clone().or_else(|| std::env::var("GITLAB_TOKEN").ok())
        }
        Some(ScmConfig::Github { token, .. }) => {
            token.clone().or_else(|| std::env::var("GITHUB_TOKEN").ok())
        }
        _ => None,
    }
}

fn get_gitlab_config(config: &Config) -> Option<GitLabConfig> {
    match &config.scm_config {
        Some(ScmConfig::Gitlab {
            url,
            token,
            create_merge_request,
            target_branch,
        }) if *create_merge_request => {
            let base_url = url
                .clone()
                .unwrap_or_else(|| "https://gitlab.com".to_string());
            let resolved_token = token
                .clone()
                .or_else(|| std::env::var("GITLAB_TOKEN").ok())
                .unwrap_or_default();
            Some(GitLabConfig {
                base_url,
                token: resolved_token,
                target_branch: target_branch.clone(),
            })
        }
        _ => None,
    }
}

fn run_from_config(config_path: &str) -> Result<(), ApplyError> {
    eprintln!("[plan] generating plan from config: {}", config_path);
    let plan = crate::commands::plan::runner::run(config_path)?;
    eprintln!("[plan] found {} projects", plan.projects.len());
    apply_plan(&plan, config_path)
}

fn apply_plan(
    plan: &crate::commands::plan::model::Plan,
    config_path: &str,
) -> Result<(), ApplyError> {
    eprintln!("[apply] loading config from: {}", config_path);
    let config = Config::from_file(config_path)?;
    let git_user = config.git_user.as_ref().ok_or(ApplyError::NoGitUser)?;
    let root_path = PathBuf::from(&config.root);
    let local_mode = is_local_mode(&config);
    let token = get_token(&config);
    let gitlab_config = get_gitlab_config(&config);
    eprintln!("[apply] root: {}", root_path.display());
    eprintln!("[apply] local_mode: {}", local_mode);
    let mut summary = ApplySummary {
        projects: Vec::new(),
    };

    for (idx, project_plan) in plan.projects.iter().enumerate() {
        eprintln!();
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        eprintln!(
            "[{}/{}] {}",
            idx + 1,
            plan.projects.len(),
            project_plan.name
        );
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let project_path = resolve_project_path(&root_path, &project_plan.path);
        eprintln!("[git] project path: {}", project_path.display());

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
        eprintln!("[git] package manager: {}", pm.name());

        eprintln!("[git] opening repository...");
        let git = Git::open(
            &project_path,
            &git_user.name,
            &git_user.email,
            token.as_deref(),
        )?;
        if !local_mode {
            eprintln!("[git] fetching base branch: {}", project_plan.base_branch);
            git.fetch(&project_plan.base_branch)?;
        }
        eprintln!(
            "[git] checking out and resetting: {}",
            project_plan.base_branch
        );
        git.checkout_and_reset(&project_plan.base_branch)?;
        eprintln!("[git] creating or resetting branch: {}", project_plan.fix_branch);
        git.create_or_reset_branch(&project_plan.fix_branch)?;

        let mut results = Vec::new();
        let mut packages_to_update: Vec<PackageUpdateRequest> = Vec::new();
        let mut pkg_json = PackageJson::from_path(&project_path)?;

        for pkg in &project_plan.packages {
            match &pkg.action {
                PlannedAction::Skip => {
                    eprintln!("[apply] {} - skipping (already satisfied)", pkg.name);
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
                    eprintln!("[apply] {} - error: {}", pkg.name, reason);
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
                    eprintln!(
                        "[apply] {} {} -> {} ({:?})",
                        pkg.name,
                        pkg.current_version.as_deref().unwrap_or("none"),
                        pkg.target_version,
                        pkg.action
                    );
                    pkg_json.set_version(&pkg.name, &pkg.target_version);
                    packages_to_update.push(PackageUpdateRequest {
                        package: pkg.name.clone(),
                        target_version: pkg.target_version.clone(),
                        dependency_type: pkg.dependency_type.clone(),
                        update_policy: pkg.update_policy.clone(),
                        scope: pkg.scope.clone(),
                    });
                }
            }
        }

        if !packages_to_update.is_empty() {
            // Write package.json with updated versions before running the package manager.
            // This ensures npm sees the correct versions without using --save flags,
            // which would cause npm to normalize and potentially strip fields from package.json.
            eprintln!("[apply] writing package.json with updated versions...");
            pkg_json.write(&project_path)?;

            eprintln!(
                "[apply] applying {} package updates in batch...",
                packages_to_update.len()
            );
            let batch_result =
                pm.apply_project_updates(&project_path, &packages_to_update, None)?;
            results.extend(batch_result.results);
        }

        let project_summary = ProjectSummary {
            name: project_plan.name.clone(),
            results: results.clone(),
            committed: false,
        };

        let all_success = project_summary.all_success();

        if all_success && !packages_to_update.is_empty() {
            let commit_message = format!("fix: update dependencies for {}", project_plan.name);
            eprintln!("[git] staging and committing...");
            git.stage_and_commit(&commit_message)?;
            if !local_mode {
                eprintln!(
                    "[git] pushing branch '{}' to origin",
                    project_plan.fix_branch
                );
                git.push(&project_plan.fix_branch)?;
                eprintln!("[git] push completed successfully");

                if let Some(ref gl_config) = gitlab_config {
                    eprintln!("[gitlab] creating merge request...");
                    let remote_url = git.get_remote_url()?;
                    let project_path = GitLabClient::extract_project_path(&remote_url)?;
                    let client = GitLabClient::new(gl_config.clone());
                    let mr_title = format!("fix: update dependencies for {}", project_plan.name);
                    match client.create_merge_request(
                        &project_path,
                        &project_plan.fix_branch,
                        &mr_title,
                    ) {
                        Ok(mr_url) => {
                            eprintln!("[gitlab] merge request created: {}", mr_url);
                        }
                        Err(e) => {
                            eprintln!("[gitlab] failed to create merge request: {}", e);
                        }
                    }
                }
            } else {
                eprintln!("[git] skipping push (local mode)");
            }
        } else if !all_success {
            eprintln!("[git] skipping commit and push (not all packages succeeded)");
        } else {
            eprintln!("[git] skipping commit and push (no changes needed)");
        }

        summary.projects.push(ProjectSummary {
            committed: all_success && !packages_to_update.is_empty(),
            ..project_summary
        });
    }

    summary.print();
    Ok(())
}

fn run_from_plan(plan_path: &str) -> Result<(), ApplyError> {
    let content = std::fs::read_to_string(plan_path)?;
    let plan: crate::commands::plan::model::Plan = serde_json::from_str(&content)?;
    apply_plan(&plan, &plan.config_path)
}

fn resolve_project_path(root: &PathBuf, project_path: &str) -> PathBuf {
    let p = PathBuf::from(project_path);
    if p.is_relative() {
        root.join(project_path)
    } else {
        p
    }
}
