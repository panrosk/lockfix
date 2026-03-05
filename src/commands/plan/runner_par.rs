use std::path::PathBuf;

use chrono::Utc;
use rayon::prelude::*;

use super::model::{PackageInstance, PackagePlan, Plan, PlanSummary, PlannedAction, ProjectPlan};
use crate::config::{Config, ConfigError, DependencyScope, DependencyType, UpdatePolicy};
use crate::package_manager::package_json::PackageJson;
use crate::package_manager::{LockfileDriver, PackageManagerKind, Version};

/// Parallel version of the plan runner.
///
/// Projects are processed concurrently using Rayon's work-stealing thread pool.
/// Each project reads its own package.json and lockfile independently, so there
/// is no shared mutable state and no locking required.
///
/// The output is semantically identical to `runner::run` — only the order of
/// projects in the returned `Vec` may differ, which is fine because consumers
/// identify projects by name, not position.
pub fn run(config_path: &str) -> Result<Plan, ConfigError> {
    let config = Config::from_file(config_path)?;
    let root_path = PathBuf::from(&config.root);

    // Collect into Vec<Result<…>> first so par_iter can drive the work,
    // then fold errors afterwards with a normal sequential iterator.
    let results: Vec<Result<ProjectPlan, ConfigError>> = config
        .projects
        .par_iter()
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

            if !pm.has_manifest(&project_path) {
                return Err(ConfigError::ManifestNotFound {
                    project: project.name.clone(),
                    manifest: pm.manifest_name().to_string(),
                });
            }

            pm.ensure_lockfile(&project_path)
                .map_err(|e| ConfigError::LockfileGenerate {
                    project: project.name.clone(),
                    lockfile: pm.lockfile_name().to_string(),
                    message: e.to_string(),
                })?;

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

                    let detected_scope = if pkg_json.get_version(&pkg.name).is_some() {
                        DependencyScope::Direct
                    } else if !instances.is_empty() {
                        DependencyScope::Transitive
                    } else {
                        DependencyScope::Auto
                    };

                    let scope = match pkg.scope {
                        DependencyScope::Auto => detected_scope,
                        _ => pkg.scope.clone(),
                    };

                    let dependency_type = pkg_json
                        .get_dependency_type(&pkg.name)
                        .unwrap_or(DependencyType::Dependency);

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
                        dependency_type,
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
        .collect();

    let projects = results.into_iter().collect::<Result<Vec<_>, _>>()?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Read the bench fixture and rewrite the CARGO_MANIFEST_DIR placeholder,
    /// same as the bench harness does, so tests are self-contained.
    fn resolved_config_path() -> tempfile::NamedTempFile {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let template = std::fs::read_to_string(manifest_dir.join("fixtures/bench.json")).unwrap();
        let content = template.replace("CARGO_MANIFEST_DIR", manifest_dir.to_str().unwrap());
        let tmp = tempfile::Builder::new().suffix(".json").tempfile().unwrap();
        std::fs::write(tmp.path(), content).unwrap();
        tmp
    }

    #[test]
    fn par_runner_matches_sequential_summary() {
        let tmp = resolved_config_path();
        let path = tmp.path().to_str().unwrap();

        let seq = crate::commands::plan::runner::run(path).unwrap();
        let par = run(path).unwrap();

        assert_eq!(seq.summary.total_projects, par.summary.total_projects);
        assert_eq!(seq.summary.total_packages, par.summary.total_packages);
        assert_eq!(seq.summary.required_packages, par.summary.required_packages);
    }

    #[test]
    fn par_runner_same_packages_per_project() {
        let tmp = resolved_config_path();
        let path = tmp.path().to_str().unwrap();

        let seq = crate::commands::plan::runner::run(path).unwrap();
        let par = run(path).unwrap();

        let mut seq_proj = seq.projects;
        let mut par_proj = par.projects;
        seq_proj.sort_by(|a, b| a.name.cmp(&b.name));
        par_proj.sort_by(|a, b| a.name.cmp(&b.name));

        for (s, p) in seq_proj.iter().zip(par_proj.iter()) {
            assert_eq!(s.name, p.name, "project name mismatch");
            assert_eq!(
                s.packages.len(),
                p.packages.len(),
                "package count mismatch for project {}",
                s.name
            );
        }
    }

    #[test]
    fn par_runner_package_versions_match_sequential() {
        let tmp = resolved_config_path();
        let path = tmp.path().to_str().unwrap();

        let seq = crate::commands::plan::runner::run(path).unwrap();
        let par = run(path).unwrap();

        let mut seq_proj = seq.projects;
        let mut par_proj = par.projects;
        seq_proj.sort_by(|a, b| a.name.cmp(&b.name));
        par_proj.sort_by(|a, b| a.name.cmp(&b.name));

        for (s, p) in seq_proj.iter().zip(par_proj.iter()) {
            let mut s_pkgs = s.packages.iter().collect::<Vec<_>>();
            let mut p_pkgs = p.packages.iter().collect::<Vec<_>>();
            s_pkgs.sort_by(|a, b| a.name.cmp(&b.name));
            p_pkgs.sort_by(|a, b| a.name.cmp(&b.name));

            for (sp, pp) in s_pkgs.iter().zip(p_pkgs.iter()) {
                assert_eq!(sp.name, pp.name, "pkg name mismatch in {}", s.name);
                assert_eq!(
                    sp.current_version, pp.current_version,
                    "version mismatch for {} in {}",
                    sp.name, s.name
                );
                assert_eq!(
                    sp.instances.len(),
                    pp.instances.len(),
                    "instance count mismatch for {} in {}",
                    sp.name,
                    s.name
                );
            }
        }
    }
}
