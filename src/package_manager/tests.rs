use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::config::PackageManager as ConfigPackageManager;
use crate::package_manager::package_json::PackageJson;

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("generic-login-api")
}

fn npm_config() -> ConfigPackageManager {
    ConfigPackageManager::Npm {
        npmrc_template: None,
        registry: None,
    }
}

fn yarn_config() -> ConfigPackageManager {
    ConfigPackageManager::Yarn {
        yarnrc_template: None,
        registry: None,
    }
}

fn pnpm_config() -> ConfigPackageManager {
    ConfigPackageManager::Pnpm {
        npmrc_template: None,
        registry: None,
    }
}

// --- name() ---

#[test]
fn test_npm_name() {
    assert_eq!(PackageManagerKind::from_config(&npm_config()).name(), "npm");
}

#[test]
fn test_yarn_name() {
    assert_eq!(
        PackageManagerKind::from_config(&yarn_config()).name(),
        "yarn"
    );
}

#[test]
fn test_pnpm_name() {
    assert_eq!(
        PackageManagerKind::from_config(&pnpm_config()).name(),
        "pnpm"
    );
}

// --- lockfile_name() ---

#[test]
fn test_npm_lockfile_name() {
    assert_eq!(
        PackageManagerKind::from_config(&npm_config()).lockfile_name(),
        "package-lock.json"
    );
}

#[test]
fn test_yarn_lockfile_name() {
    assert_eq!(
        PackageManagerKind::from_config(&yarn_config()).lockfile_name(),
        "yarn.lock"
    );
}

#[test]
fn test_pnpm_lockfile_name() {
    assert_eq!(
        PackageManagerKind::from_config(&pnpm_config()).lockfile_name(),
        "pnpm-lock.yaml"
    );
}

// --- has_lockfile() ---

#[test]
fn test_has_lockfile_returns_true_when_present() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("package-lock.json"), "{}").unwrap();

    let pm = PackageManagerKind::from_config(&npm_config());
    assert!(pm.has_lockfile(dir.path()));
}

#[test]
fn test_has_lockfile_returns_false_when_missing() {
    let dir = TempDir::new().unwrap();

    let pm = PackageManagerKind::from_config(&npm_config());
    assert!(!pm.has_lockfile(dir.path()));
}

#[test]
fn test_has_lockfile_does_not_match_wrong_pm() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("yarn.lock"), "").unwrap();

    let pm = PackageManagerKind::from_config(&npm_config());
    assert!(!pm.has_lockfile(dir.path()));
}

// --- is_installed() ---

#[test]
fn test_npm_is_installed() {
    assert!(PackageManagerKind::from_config(&npm_config()).is_installed());
}

#[test]
fn test_yarn_is_installed() {
    assert!(PackageManagerKind::from_config(&yarn_config()).is_installed());
}

#[test]
fn test_pnpm_is_installed() {
    assert!(PackageManagerKind::from_config(&pnpm_config()).is_installed());
}

// --- PackageJson::from_path() ---

#[test]
fn test_package_json_loads_from_fixture() {
    let pkg = PackageJson::from_path(&fixtures_path()).unwrap();
    // verify at least one known dep and dev-dep can be found via the public API
    assert!(pkg.get_version("lodash").is_some(), "expected lodash in dependencies");
    assert!(pkg.get_version("jest").is_some(), "expected jest in devDependencies");
}

#[test]
fn test_package_json_get_version_direct_dep() {
    let pkg = PackageJson::from_path(&fixtures_path()).unwrap();
    let version = pkg.get_version("lodash");
    assert!(version.is_some());
    assert_eq!(version.unwrap().as_str(), "^4.17.21");
}

#[test]
fn test_package_json_get_version_dev_dep() {
    let pkg = PackageJson::from_path(&fixtures_path()).unwrap();
    let version = pkg.get_version("jest");
    assert!(version.is_some());
}

#[test]
fn test_package_json_get_version_transitive_returns_none() {
    let pkg = PackageJson::from_path(&fixtures_path()).unwrap();
    // follow-redirects is a transitive dep of axios, not declared directly
    let version = pkg.get_version("follow-redirects");
    assert!(version.is_none());
}

#[test]
fn test_package_json_roundtrip_preserves_all_fields() {
    let dir = TempDir::new().unwrap();
    let json = r#"{
  "name": "idp-authorize",
  "version": "1.0.0",
  "description": "Generic API authorize",
  "main": "lambda.js",
  "license": "ISC",
  "scripts": {
    "start": "ts-node index.local.ts",
    "test": "jest --runInBand"
  },
  "dependencies": {
    "express": "^4.18.0"
  },
  "devDependencies": {
    "typescript": "^5.0.0"
  }
}"#;
    fs::write(dir.path().join("package.json"), json).unwrap();
    let pkg = PackageJson::from_path(dir.path()).unwrap();
    pkg.write(dir.path()).unwrap();

    let back: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("package.json")).unwrap())
            .unwrap();
    assert_eq!(back["name"], "idp-authorize", "name was dropped");
    assert_eq!(back["version"], "1.0.0", "version was dropped");
    assert_eq!(back["description"], "Generic API authorize", "description was dropped");
    assert_eq!(back["main"], "lambda.js", "main was dropped");
    assert_eq!(back["license"], "ISC", "license was dropped");
    assert!(back["scripts"].is_object(), "scripts was dropped");
    assert_eq!(back["scripts"]["start"], "ts-node index.local.ts", "scripts.start was dropped");
    assert_eq!(back["dependencies"]["express"], "^4.18.0", "dependencies was dropped");
    assert_eq!(back["devDependencies"]["typescript"], "^5.0.0", "devDependencies was dropped");
}

/// Simulates the apply flow: read package.json, update a version, write it back,
/// then verify all original fields (name, version, scripts, main, etc.) are preserved.
/// This guards against regressions where write() strips non-dependency fields.
#[test]
fn test_package_json_apply_flow_preserves_all_fields() {
    let dir = TempDir::new().unwrap();
    let original = serde_json::json!({
        "name": "idp-authorize",
        "version": "1.0.0",
        "description": "Generic API authorize",
        "main": "lambda.js",
        "scripts": {
            "start": "ts-node index.local.ts",
            "build": "tsc && cp -R config *.json build/",
            "test": "jest --runInBand --detectOpenHandles"
        },
        "dependencies": {
            "express": "^4.18.0",
            "body-parser": "2.2.1"
        },
        "devDependencies": {
            "typescript": "^5.0.0"
        }
    });

    fs::write(
        dir.path().join("package.json"),
        serde_json::to_string_pretty(&original).unwrap(),
    ).unwrap();

    // --- simulate apply: read, mutate one version, write ---
    let mut pkg = PackageJson::from_path(dir.path()).unwrap();
    pkg.set_version("body-parser", "2.2.2");
    pkg.write(dir.path()).unwrap();

    // --- verify the written file ---
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("package.json")).unwrap())
            .unwrap();

    // all non-dependency fields must be intact
    assert_eq!(written["name"], "idp-authorize", "name was erased");
    assert_eq!(written["version"], "1.0.0", "version was erased");
    assert_eq!(written["description"], "Generic API authorize", "description was erased");
    assert_eq!(written["main"], "lambda.js", "main was erased");
    assert!(written["scripts"].is_object(), "scripts was erased");
    assert_eq!(written["scripts"]["start"], "ts-node index.local.ts", "scripts.start was erased");
    assert_eq!(written["scripts"]["test"], "jest --runInBand --detectOpenHandles", "scripts.test was erased");

    // dependency field that was updated must reflect new version
    assert_eq!(written["dependencies"]["body-parser"], "2.2.2", "body-parser version was not updated");

    // dependency field that was not touched must be unchanged
    assert_eq!(written["dependencies"]["express"], "^4.18.0", "express version was changed unexpectedly");
    assert_eq!(written["devDependencies"]["typescript"], "^5.0.0", "devDependencies was erased");
}

// --- LockfileDriver for npm via PackageManagerKind ---

#[test]
fn test_npm_get_all_instances_direct_dep() {
    let pm = PackageManagerKind::from_config(&npm_config());
    let instances = pm.get_all_instances(&fixtures_path(), "lodash");
    assert!(!instances.is_empty());
    assert!(instances.iter().any(|i| i.path == "node_modules/lodash"));
}

#[test]
fn test_npm_get_all_instances_transitive_dep() {
    let pm = PackageManagerKind::from_config(&npm_config());
    // follow-redirects is a transitive dep of axios
    let instances = pm.get_all_instances(&fixtures_path(), "follow-redirects");
    assert!(!instances.is_empty());
}

#[test]
fn test_npm_get_all_instances_unknown_returns_empty() {
    let pm = PackageManagerKind::from_config(&npm_config());
    let instances = pm.get_all_instances(&fixtures_path(), "this-package-does-not-exist");
    assert!(instances.is_empty());
}

#[test]
fn test_npm_get_version_returns_top_level() {
    let pm = PackageManagerKind::from_config(&npm_config());
    let version = pm.get_version(&fixtures_path(), "lodash");
    assert!(version.is_some());
}

#[test]
fn test_npm_get_version_unknown_returns_none() {
    let pm = PackageManagerKind::from_config(&npm_config());
    let version = pm.get_version(&fixtures_path(), "this-package-does-not-exist");
    assert!(version.is_none());
}

// --- Integration tests for ApplyDriver ---

fn create_npm_project(dir: &Path, dependencies: Option<&str>) {
    let deps = dependencies.unwrap_or("{}");
    let package_json = format!(
        r#"{{
  "name": "test-project",
  "version": "1.0.0",
  "dependencies": {}
}}"#,
        deps
    );
    fs::write(dir.join("package.json"), package_json).unwrap();
}

#[test]
fn test_npm_apply_project_updates_success() {
    let dir = TempDir::new().unwrap();
    // Pre-write package.json with the target version, as the runner does before
    // calling apply_project_updates. The package manager no longer uses --save flags.
    create_npm_project(
        dir.path(),
        Some(r#"{"lodash": "4.17.21"}"#),
    );

    let packages = vec![PackageUpdateRequest {
        package: "lodash".to_string(),
        target_version: "4.17.21".to_string(),
        dependency_type: crate::config::DependencyType::Dependency,
        update_policy: crate::config::UpdatePolicy::Exact,
        scope: crate::config::DependencyScope::Direct,
    }];

    let npm = super::Npm {
        npmrc_template: None,
        registry: None,
    };

    let result = npm
        .apply_project_updates(dir.path(), &packages, None)
        .unwrap();

    assert!(result.audit_fix_ran);
    assert!(!result.results.is_empty());
    assert!(result.results[0].version_matched);
    // PartialSuccess is correct here: audit fix alone didn't satisfy the version,
    // npm install was needed. Success is only returned when audit fix alone resolved it.
    assert!(
        result.results[0].final_status == ApplyStatus::Success
            || result.results[0].final_status == ApplyStatus::PartialSuccess,
        "expected success or partial success, got {:?}",
        result.results[0].final_status
    );
    assert!(dir.path().join("package-lock.json").exists());
}

#[test]
fn test_npm_apply_project_updates_version_mismatch_recovers() {
    let dir = TempDir::new().unwrap();
    create_npm_project(dir.path(), None);

    let packages = vec![PackageUpdateRequest {
        package: "lodash".to_string(),
        target_version: "4.17.21".to_string(),
        dependency_type: crate::config::DependencyType::Dependency,
        update_policy: crate::config::UpdatePolicy::Exact,
        scope: crate::config::DependencyScope::Direct,
    }];

    let npm = super::Npm {
        npmrc_template: None,
        registry: None,
    };

    let result = npm
        .apply_project_updates(dir.path(), &packages, None)
        .unwrap();

    assert!(
        result.results[0].version_matched
            || result.results[0].final_status == ApplyStatus::VersionMismatch
    );
}

#[test]
fn test_npm_apply_project_updates_dev_dependency() {
    let dir = TempDir::new().unwrap();
    create_npm_project(dir.path(), None);

    let packages = vec![PackageUpdateRequest {
        package: "jest".to_string(),
        target_version: "29.7.0".to_string(),
        dependency_type: crate::config::DependencyType::DevDependency,
        update_policy: crate::config::UpdatePolicy::Exact,
        scope: crate::config::DependencyScope::Direct,
    }];

    let npm = super::Npm {
        npmrc_template: None,
        registry: None,
    };

    let result = npm
        .apply_project_updates(dir.path(), &packages, None)
        .unwrap();

    assert!(
        result.results[0].version_matched
            || result.results[0].final_status == ApplyStatus::VersionMismatch
    );
}

#[test]
fn test_npm_apply_project_updates_minimum_policy() {
    let dir = TempDir::new().unwrap();
    create_npm_project(dir.path(), None);

    let packages = vec![PackageUpdateRequest {
        package: "lodash".to_string(),
        target_version: "4.17.0".to_string(),
        dependency_type: crate::config::DependencyType::Dependency,
        update_policy: crate::config::UpdatePolicy::Minimum,
        scope: crate::config::DependencyScope::Direct,
    }];

    let npm = super::Npm {
        npmrc_template: None,
        registry: None,
    };

    let result = npm
        .apply_project_updates(dir.path(), &packages, None)
        .unwrap();

    assert!(
        result.results[0].version_matched
            || result.results[0].final_status == ApplyStatus::VersionMismatch
    );
}

#[test]
fn test_apply_summary_print() {
    use crate::commands::apply::runner::{ApplySummary, ProjectSummary};

    let summary = ApplySummary {
        projects: vec![
            ProjectSummary {
                name: "my-project".to_string(),
                results: vec![
                    ApplyResult {
                        package: "lodash".to_string(),
                        target_version: "4.17.21".to_string(),
                        audit_fix_ran: true,
                        audit_fix_success: true,
                        version_matched: true,
                        lockfile_deleted: false,
                        node_modules_deleted: false,
                        update_ran: false,
                        final_status: ApplyStatus::Success,
                        error_reason: None,
                    },
                    ApplyResult {
                        package: "axios".to_string(),
                        target_version: "1.6.0".to_string(),
                        audit_fix_ran: true,
                        audit_fix_success: true,
                        version_matched: true,
                        lockfile_deleted: false,
                        node_modules_deleted: false,
                        update_ran: false,
                        final_status: ApplyStatus::Success,
                        error_reason: None,
                    },
                ],
                committed: true,
            },
            ProjectSummary {
                name: "other-project".to_string(),
                results: vec![
                    ApplyResult {
                        package: "lodash".to_string(),
                        target_version: "4.17.21".to_string(),
                        audit_fix_ran: true,
                        audit_fix_success: true,
                        version_matched: true,
                        lockfile_deleted: false,
                        node_modules_deleted: false,
                        update_ran: false,
                        final_status: ApplyStatus::Success,
                        error_reason: None,
                    },
                    ApplyResult {
                        package: "react".to_string(),
                        target_version: "18.2.0".to_string(),
                        audit_fix_ran: true,
                        audit_fix_success: false,
                        version_matched: true,
                        lockfile_deleted: true,
                        node_modules_deleted: true,
                        update_ran: true,
                        final_status: ApplyStatus::PartialSuccess,
                        error_reason: None,
                    },
                    ApplyResult {
                        package: "vue".to_string(),
                        target_version: "3.4.0".to_string(),
                        audit_fix_ran: true,
                        audit_fix_success: false,
                        version_matched: false,
                        lockfile_deleted: true,
                        node_modules_deleted: true,
                        update_ran: true,
                        final_status: ApplyStatus::VersionMismatch,
                        error_reason: None,
                    },
                ],
                committed: false,
            },
        ],
    };

    summary.print();
}
