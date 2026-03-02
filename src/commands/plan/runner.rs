use std::path::PathBuf;

use chrono::Utc;

use super::model::{PackageInstance, PackagePlan, Plan, PlanSummary, PlannedAction, ProjectPlan};
use crate::config::{Config, ConfigError, DependencyScope, UpdatePolicy};
use crate::package_manager::package_json::PackageJson;
use crate::package_manager::{LockfileDriver, PackageManagerKind, Version};

pub fn run(config_path: &str) -> Result<Plan, ConfigError> {
    let config = Config::from_file(config_path)?;
    let root_path = PathBuf::from(&config.root);

    let projects: Vec<ProjectPlan> = config
        .projects
        .iter()
        .map(|project| {
            let config_pm = project
                .package_manager
                .as_ref()
                .or(config.package_manager.as_ref())
                .unwrap(); // safe — validate() already enforces this

            let pm = PackageManagerKind::from_config(config_pm);

            let base_branch = project
                .base_branch
                .clone()
                .unwrap_or_else(|| config.base_branch.clone());

            let fix_branch = config
                .fix_branch_template
                .replace("{projectName}", &project.name);

            let project_path = {
                let p = PathBuf::from(&project.path);
                if p.is_relative() {
                    root_path.join(&project.path)
                } else {
                    p
                }
            };

            if !project_path.exists() {
                return Err(ConfigError::ProjectDoesNotExist(
                    project_path.display().to_string(),
                ));
            }

            if !pm.is_installed() {
                return Err(ConfigError::PackageManagerNotInstalled(
                    pm.name().to_string(),
                ));
            }

            if !pm.has_lockfile(&project_path) {
                return Err(ConfigError::LockfileNotFound {
                    project: project.name.clone(),
                    lockfile: pm.lockfile_name().to_string(),
                });
            }

            let pkg_json = PackageJson::from_path(&project_path)
                .map_err(|e| ConfigError::ManifestRead(e.to_string()))?;

            let packages = project
                .packages
                .iter()
                .map(|pkg| {
                    let instances: Vec<PackageInstance> = pm
                        .get_all_instances(&project_path, &pkg.name)
                        .into_iter()
                        .map(|i| PackageInstance {
                            path: i.path,
                            version: i.version.to_string(),
                        })
                        .collect();

                    let current_version = pm
                        .get_version(&project_path, &pkg.name)
                        .map(|v| v.to_string());

                    let scope = if pkg_json.get_version(&pkg.name).is_some() {
                        DependencyScope::Direct
                    } else if !instances.is_empty() {
                        DependencyScope::Transitive
                    } else {
                        DependencyScope::Auto
                    };

                    let action = if instances.is_empty() {
                        PlannedAction::Add
                    } else if let Some(ref current) = current_version {
                        let current_ver = Version::from(current.as_str());
                        let target_ver = Version::from(pkg.target_version.as_str());

                        match pkg.update_policy {
                            UpdatePolicy::Minimum => {
                                if current_ver.satisfies(&target_ver, &UpdatePolicy::Minimum) {
                                    PlannedAction::Skip
                                } else {
                                    PlannedAction::Update
                                }
                            }
                            UpdatePolicy::Exact => {
                                if current_ver.satisfies(&target_ver, &UpdatePolicy::Exact) {
                                    PlannedAction::Skip
                                } else if current_ver.is_downgrade(&target_ver) {
                                    PlannedAction::Error {
                                        reason: format!(
                                            "downgrade not allowed for exact policy: {} -> {}",
                                            current, pkg.target_version
                                        ),
                                    }
                                } else {
                                    PlannedAction::Update
                                }
                            }
                        }
                    } else {
                        PlannedAction::Update
                    };

                    PackagePlan {
                        name: pkg.name.clone(),
                        current_version,
                        target_version: pkg.target_version.clone(),
                        update_policy: pkg.update_policy.clone(),
                        action,
                        scope,
                        dependency_type: pkg.dependency_type.clone(),
                        required: pkg.required,
                        reason: pkg.reason.clone(),
                        instances,
                    }
                })
                .collect::<Vec<_>>();

            Ok(ProjectPlan {
                name: project.name.clone(),
                path: project.path.clone(),
                fix_branch,
                base_branch,
                package_manager: pm.name().to_string(),
                packages,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let summary = PlanSummary {
        total_projects: projects.len(),
        total_packages: projects.iter().map(|p| p.packages.len()).sum(),
        required_packages: projects
            .iter()
            .flat_map(|p| &p.packages)
            .filter(|pkg| pkg.required)
            .count(),
    };

    Ok(Plan {
        generated_at: Utc::now().to_rfc3339(),
        config_path: config_path.to_string(),
        summary,
        projects,
    })
}
