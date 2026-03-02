# NPM Driver Documentation

The npm driver implements the `LockfileDriver` and `ApplyDriver` traits for npm projects. It handles lockfile parsing, version detection, and package installation.

## Overview

```rust
pub struct Npm {
    pub npmrc_template: Option<String>,
    pub registry: Option<String>,
}
```

The driver supports:
- Reading `package-lock.json` (lockfile versions 1, 2, and 3)
- Finding all instances of a package in the lockfile
- Running `npm install` with version constraints
- Running `npm audit fix`
- Recovery from version mismatches

## Lockfile Parsing

### Structure

The npm lockfile (`package-lock.json`) has this structure:

```json
{
  "lockfileVersion": 3,
  "packages": {
    "node_modules/lodash": {
      "version": "4.17.21"
    },
    "node_modules/axios": {
      "version": "1.6.0"
    },
    "node_modules/some-package/node_modules/lodash": {
      "version": "4.17.15"
    }
  }
}
```

### Parsing Implementation

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageLockJson {
    pub lockfile_version: u8,
    pub packages: HashMap<String, PackageLockEntry>,
}

#[derive(Debug, Deserialize)]
pub struct PackageLockEntry {
    pub version: Option<Version>,
}
```

The `packages` map uses the path as the key:
- `"node_modules/{name}"` — top-level dependency
- `"node_modules/{parent}/node_modules/{name}"` — nested dependency

### Finding All Instances

```rust
fn get_all_instances(&self, project_path: &Path, name: &str) -> Vec<PackageInstance> {
    let lock = PackageLockJson::from_path(project_path)?;
    let suffix = format!("node_modules/{name}");
    
    lock.packages
        .into_iter()
        .filter(|(key, _)| key == &suffix || key.ends_with(&format!("/{suffix}")))
        .filter_map(|(key, entry)| {
            entry.version.map(|v| PackageInstance {
                path: key,
                version: v,
            })
        })
        .collect()
}
```

This finds:
1. Top-level instances: `key == "node_modules/{name}"`
2. Nested instances: `key.ends_with("/node_modules/{name}")`

Example output for a package with multiple versions:

```rust
vec![
    PackageInstance { 
        path: "node_modules/lodash", 
        version: Version("4.17.21") 
    },
    PackageInstance { 
        path: "node_modules/some-package/node_modules/lodash", 
        version: Version("4.17.15") 
    },
]
```

### Getting Top-Level Version

The `LockfileDriver` trait provides a default implementation:

```rust
fn get_version(&self, project_path: &Path, name: &str) -> Option<Version> {
    let top_level = format!("node_modules/{name}");
    self.get_all_instances(project_path, name)
        .into_iter()
        .find(|i| i.path == top_level)
        .map(|i| i.version)
}
```

This returns the version of the top-level instance, or `None` if not found.

## Apply Process

### Apply Context

```rust
pub struct ApplyContext<'a> {
    pub project_path: &'a Path,
    pub package: &'a str,
    pub target_version: &'a str,
    pub dependency_type: DependencyType,
    pub update_policy: UpdatePolicy,
}
```

### Apply Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    START APPLY UPDATE                       │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. INITIALIZE RESULT                                        │
│    - Set all flags to false/default                         │
│    - Status = Success (optimistic)                          │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. RUN AUDIT FIX (if supported)                             │
│    - npm audit fix                                          │
│    - Record success/failure                                 │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. BUILD INSTALL COMMAND                                    │
│                                                             │
│    Determine flag based on dependency_type:                 │
│    - Dependency         → --save                            │
│    - DevDependency      → --save-dev                        │
│    - PeerDependency     → --save-peer                       │
│    - OptionalDependency → --save-optional                    │
│                                                             │
│    Determine version spec based on update_policy:           │
│    - Exact  → {version}                                     │
│    - Minimum → >={version}                                   │
│                                                             │
│    Result: npm install {package}@{spec} {flag}              │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. RUN INSTALL COMMAND                                      │
│    - Execute npm install command                            │
│    - On failure: return CommandFailed error                 │
└─────────────────────────┬───────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. VERIFY VERSION                                           │
│    - Get current version from lockfile                      │
│    - Check if matches target                                │
└─────────────────────────┬───────────────────────────────────┘
                          │
                    ┌─────┴─────┐
                    │           │
                Matches     Mismatch
                    │           │
                    ▼           ▼
┌──────────────────────┐  ┌─────────────────────────────────┐
│ Set final status:    │  │ 6. RECOVERY PROCESS             │
│ - Success            │  │                                 │
│                      │  │ a. Delete node_modules/         │
│ Return result        │  │ b. Delete package-lock.json     │
└──────────────────────┘  │ c. Run npm update               │
                          │ d. Re-verify version            │
                          └──────────────┬──────────────────┘
                                         │
                                   ┌─────┴─────┐
                                   │           │
                               Matches     Mismatch
                                   │           │
                                   ▼           ▼
                          ┌─────────────┐ ┌─────────────┐
                          │ PartialSuccess│ VersionMismatch
                          └─────────────┘ └─────────────┘
```

### Install Command Construction

```rust
let flag = match ctx.dependency_type {
    DependencyType::Dependency => "--save",
    DependencyType::DevDependency => "--save-dev",
    DependencyType::PeerDependency => "--save-peer",
    DependencyType::OptionalDependency => "--save-optional",
};

let version_spec = match ctx.update_policy {
    UpdatePolicy::Exact => ctx.target_version.to_string(),
    UpdatePolicy::Minimum => format!(">={}", ctx.target_version),
};

let install_cmd = format!("npm install {}@{} {}", ctx.package, version_spec, flag);
```

Examples:

| Policy | Target | Command |
|--------|--------|---------|
| exact | 4.17.21 | `npm install lodash@4.17.21 --save` |
| minimum | 4.17.21 | `npm install lodash@>=4.17.21 --save` |
| exact | 1.6.0 | `npm install axios@1.6.0 --save-dev` |

### Audit Fix

```rust
fn audit_fix(&self, project_path: &Path) -> Option<Result<(), ApplyError>> {
    Some(run_command("npm audit fix", project_path).map_err(|e| ApplyError::CommandFailed {
        command: e.command,
        path: e.path,
        message: e.message,
    }))
}

fn supports_audit_fix(&self) -> bool {
    true
}
```

Npm's `audit fix` attempts to automatically fix vulnerabilities by upgrading to compatible versions.

### Recovery Process

When the installed version doesn't match the target:

```rust
if !result.version_matched {
    // 1. Delete node_modules
    let node_modules = ctx.project_path.join("node_modules");
    if node_modules.exists() {
        fs::remove_dir_all(&node_modules).ok();
        result.node_modules_deleted = true;
    }

    // 2. Delete lockfile
    let lockfile_path = ctx.project_path.join("package-lock.json");
    if lockfile_path.exists() {
        fs::remove_file(&lockfile_path).ok();
        result.lockfile_deleted = true;
    }

    // 3. Run npm update
    run_command("npm update", ctx.project_path).ok();
    result.update_ran = true;

    // 4. Re-check version
    let current_version = self.get_version(ctx.project_path, ctx.package);
    result.version_matched = current_version
        .as_ref()
        .map(|v| v.0 == ctx.target_version)
        .unwrap_or(false);
}
```

This recovery handles edge cases where npm's resolver chooses a different version due to:
- Conflicting peer dependencies
- Range restrictions from other packages
- Cached metadata

## Error Handling

### PackageLockError

```rust
pub enum PackageLockError {
    Io { path: String, source: std::io::Error },
    Parse { path: String, source: serde_json::Error },
}
```

| Error | Cause |
|-------|-------|
| `Io` | File doesn't exist or not readable |
| `Parse` | Invalid JSON or unexpected structure |

When lockfile parsing fails, `get_all_instances` returns an empty vector rather than propagating the error. This allows the system to handle missing/invalid lockfiles gracefully.

### ApplyError

```rust
pub enum ApplyError {
    CommandFailed { command: String, path: String, message: String },
    PackageJsonRead(PackageJsonError),
    PackageJsonWrite(String),
}
```

| Error | Cause |
|-------|-------|
| `CommandFailed` | npm command failed (install, audit fix, update) |
| `PackageJsonRead` | Failed to read package.json |
| `PackageJsonWrite` | Failed to write package.json |

## Utility Methods

### Checking Prerequisites

```rust
pub fn is_installed(&self) -> bool {
    which::which("npm").is_ok()
}

pub fn has_lockfile(&self, project_path: &Path) -> bool {
    project_path.join(self.lockfile_name()).exists()
}
```

These are checked during the plan phase to fail early if prerequisites aren't met.

### Default Commands

```rust
pub fn default_commands(&self) -> Commands {
    Commands {
        install: Some("npm install".into()),
        test: Some("npm test".into()),
        build: Some("npm run build".into()),
    }
}
```

These can be overridden per-project in the configuration.

## Version Handling

The `Version` type is a simple wrapper around a string:

```rust
pub struct Version(pub String);
```

Version comparison is implemented in the `mod.rs` file:

```rust
impl Version {
    fn parse_parts(&self) -> Vec<u64> {
        let clean = self.0.trim_start_matches(|c| c == 'v' || c == '^' || c == '~');
        clean.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    }

    pub fn cmp_versions(&self, other: &Version) -> std::cmp::Ordering {
        // Compare segment by segment
    }

    pub fn satisfies(&self, target: &Version, policy: &UpdatePolicy) -> bool {
        match policy {
            UpdatePolicy::Minimum => self.cmp_versions(target) != Ordering::Less,
            UpdatePolicy::Exact => self.cmp_versions(target) == Ordering::Equal,
        }
    }
}
```

This handles common version formats:
- `4.17.21` — clean semver
- `^4.17.21` — with caret
- `~4.17.21` — with tilde
- `v4.17.21` — with v prefix