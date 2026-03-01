use super::*;

fn valid_config_json() -> &'static str {
    r#"{
        "root": "/projects/myapp",
        "fixBranchTemplate": "fix/{projectId}-vulnerabilities",
        "baseBranch": "main",
        "packageManager": { "npm": { "npmrcTemplate": null, "registry": null } },
        "gitUser": {
            "name": "Oscar Fuentes",
            "email": "oscar@example.com",
            "username": "ofuentes"
        },
        "scmConfig": {
            "provider": "gitlab",
            "url": null,
            "token": "$GITLAB_TOKEN",
            "create_merge_request": true,
            "target_branch": "main"
        },
        "projects": [
            {
                "name": "generic-login-api",
                "path": "services/generic-login-api",
                "baseBranch": null,
                "packageManager": null,
                "commands": null,
                "packages": [
                    {
                        "name": "lodash",
                        "targetVersion": "4.17.23",
                        "updatePolicy": "exact",
                        "scope": "direct",
                        "dependencyType": "dependency",
                        "required": true,
                        "reason": "security-fix"
                    }
                ]
            }
        ]
    }"#
}

#[test]
fn test_deserializes_valid_config() {
    let config: Config = serde_json::from_str(valid_config_json()).unwrap();

    assert_eq!(config.root, "/projects/myapp");
    assert_eq!(config.base_branch, "main");
    assert!(config.package_manager.is_some());
    assert!(config.git_user.is_some());
    assert_eq!(config.projects.len(), 1);

    let project = &config.projects[0];
    assert_eq!(project.name, "generic-login-api");
    assert_eq!(project.packages.len(), 1);
    assert_eq!(project.packages[0].name, "lodash");
    assert_eq!(project.packages[0].target_version, "4.17.23");
}

#[test]
fn test_validate_passes_when_global_package_manager_set() {
    let config: Config = serde_json::from_str(valid_config_json()).unwrap();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_fails_when_no_package_manager_on_project_or_global() {
    let json = r#"{
        "root": "/projects/myapp",
        "fixBranchTemplate": "fix/{projectId}-vulnerabilities",
        "baseBranch": "main",
        "packageManager": null,
        "gitUser": null,
        "scmConfig": null,
        "projects": [
            {
                "name": "orphan-project",
                "path": "services/orphan",
                "baseBranch": null,
                "packageManager": null,
                "commands": null,
                "packages": []
            }
        ]
    }"#;

    let config: Config = serde_json::from_str(json).unwrap();
    let err = config.validate().unwrap_err();

    assert!(
        matches!(&err, ConfigError::MissingPackageManager(name) if name == "orphan-project"),
        "expected MissingPackageManager error"
    );
    assert_eq!(
        err.to_string(),
        "project 'orphan-project' has no package_manager and no global package_manager is set"
    );
}

#[test]
fn test_parse_error_on_unknown_package_manager() {
    let json = r#"{
        "root": "/projects/myapp",
        "fixBranchTemplate": "fix/{projectId}-vulnerabilities",
        "baseBranch": "main",
        "packageManager": { "bun": {} },
        "gitUser": null,
        "scmConfig": null,
        "projects": []
    }"#;

    let err = serde_json::from_str::<Config>(json).unwrap_err();
    let config_err = ConfigError::Parse(err);

    assert!(config_err.to_string().contains("failed to parse config"));
}

#[test]
fn test_resolved_commands_uses_pm_defaults_when_no_override() {
    let pm = PackageManager::Npm {
        npmrc_template: None,
        registry: None,
    };
    let project = Project {
        name: "api".into(),
        path: "services/api".into(),
        base_branch: None,
        package_manager: None,
        commands: None,
        packages: vec![],
    };

    let resolved = project.resolved_commands(&pm);

    assert_eq!(resolved.install.as_deref(), Some("npm install"));
    assert_eq!(resolved.test.as_deref(), Some("npm test"));
    assert_eq!(resolved.build.as_deref(), Some("npm run build"));
}

#[test]
fn test_resolved_commands_project_override_wins_per_field() {
    let pm = PackageManager::Npm {
        npmrc_template: None,
        registry: None,
    };
    let project = Project {
        name: "api".into(),
        path: "services/api".into(),
        base_branch: None,
        package_manager: None,
        // only override test, install and build should fall back to npm defaults
        commands: Some(Commands {
            install: None,
            test: Some("npm run test:ci".into()),
            build: None,
        }),
        packages: vec![],
    };

    let resolved = project.resolved_commands(&pm);

    assert_eq!(resolved.install.as_deref(), Some("npm install"));
    assert_eq!(resolved.test.as_deref(), Some("npm run test:ci"));
    assert_eq!(resolved.build.as_deref(), Some("npm run build"));
}

#[test]
fn test_parse_error_on_malformed_json() {
    let json = r#"{ "root": "/projects/myapp", INVALID }"#;

    let err = serde_json::from_str::<Config>(json).unwrap_err();
    let config_err = ConfigError::Parse(err);

    assert!(config_err.to_string().contains("failed to parse config"));
}
