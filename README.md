# lockfix

A CLI tool for auditing and applying dependency upgrades across multiple Node.js projects using their lockfiles. Define what packages need updating (and to what version) in a single JSON config file, generate a plan, and apply it.

## How it works

lockfix operates in two steps:

1. **`plan`** — reads your config, inspects each project's lockfile in parallel, and produces a JSON document describing what will change (current version, target version, action, scope, all lockfile instances).
2. **`apply`** — executes the upgrades. Accepts either a config file (generates a plan on the fly) or a pre-generated plan file.

## Installation

```bash
cargo install --path .
```

Requires Rust 2024 edition toolchain.

## Usage

```bash
# Generate a plan and print it as JSON
lockfix plan --config path/to/lockfix.json

# Apply from a config file (generates a plan on the fly)
lockfix apply --config path/to/lockfix.json

# Apply from a pre-generated plan file
lockfix apply --plan path/to/plan.json
```

## Config file format

```json
{
  "root": "/absolute/path/to/your/monorepo",
  "fixBranchTemplate": "fix/{projectName}-vulnerabilities",
  "baseBranch": "develop",
  "packageManager": { "npm": { "npmrcTemplate": null, "registry": null } },
  "gitUser": {
    "name": "Your Name",
    "email": "you@example.com",
    "username": "yourhandle"
  },
  "scmConfig": {
    "provider": "gitlab",
    "url": null,
    "token": null,
    "create_merge_request": true,
    "target_branch": "develop"
  },
  "projects": [
    {
      "name": "my-api",
      "path": "my-api",
      "baseBranch": null,
      "packageManager": null,
      "commands": null,
      "packages": [
        {
          "name": "lodash",
          "targetVersion": "4.17.23",
          "updatePolicy": "exact",
          "scope": "auto",
          "dependencyType": "dependency",
          "required": true,
          "reason": "security-fix"
        }
      ]
    }
  ]
}
```

### Top-level fields

| Field | Type | Description |
|---|---|---|
| `root` | string | Absolute path to the directory containing your projects. Project `path` values are resolved relative to this. |
| `fixBranchTemplate` | string | Branch name template. `{projectName}` is replaced with the project's `name`. |
| `baseBranch` | string | Default branch to branch from. Can be overridden per project. |
| `packageManager` | object | Global package manager config. Overridable per project. |
| `gitUser` | object \| null | Git identity used when committing. |
| `scmConfig` | object \| null | SCM integration config (`gitlab`, `github`, or `local`). |
| `projects` | array | List of projects to process. |

### Package manager variants

```json
{ "npm":  { "npmrcTemplate": null, "registry": null } }
{ "yarn": { "yarnrcTemplate": null, "registry": null } }
{ "pnpm": { "npmrcTemplate": null, "registry": null } }
```

### Project fields

| Field | Type | Description |
|---|---|---|
| `name` | string | Unique project name. Used in branch template substitution. |
| `path` | string | Path relative to `root` (or absolute). |
| `baseBranch` | string \| null | Overrides the global `baseBranch` for this project. |
| `packageManager` | object \| null | Overrides the global `packageManager` for this project. |
| `commands` | object \| null | Override individual commands (`install`, `test`, `build`). Any omitted field falls back to the package manager default. |
| `packages` | array | Packages to upgrade within this project. |

### Package target fields

| Field | Type | Values | Description |
|---|---|---|---|
| `name` | string | — | npm package name. |
| `targetVersion` | string | — | Version to upgrade to. |
| `updatePolicy` | string | `exact`, `minimum` | Whether to pin to exact version or use `>=` range. |
| `scope` | string | `direct`, `transitive`, `auto` | `auto` detects from the lockfile and manifest. |
| `dependencyType` | string | `dependency`, `devDependency`, `peerDependency`, `optionalDependency` | Where to write the dependency. |
| `required` | bool | — | If `true`, a failure on this package blocks the whole project. |
| `reason` | string | — | Human-readable reason e.g. `"security-fix"`. |

### SCM config variants

```json
{ "provider": "gitlab", "url": null, "token": null, "create_merge_request": true, "target_branch": "develop" }
{ "provider": "github", "url": null, "token": null, "create_pull_request": true, "target_branch": "main" }
{ "provider": "local" }
```

## Plan output

Running `lockfix plan` produces a JSON document:

```json
{
  "generatedAt": "2026-03-01T12:00:00Z",
  "configPath": "path/to/lockfix.json",
  "summary": {
    "totalProjects": 1,
    "totalPackages": 3,
    "requiredPackages": 2
  },
  "projects": [
    {
      "name": "my-api",
      "path": "my-api",
      "fixBranch": "fix/my-api-vulnerabilities",
      "baseBranch": "develop",
      "packageManager": "npm",
      "packages": [
        {
          "name": "lodash",
          "currentVersion": "4.17.21",
          "targetVersion": "4.17.23",
          "updatePolicy": "exact",
          "action": "update",
          "scope": "direct",
          "dependencyType": "dependency",
          "required": true,
          "reason": "security-fix",
          "instances": [
            { "path": "node_modules/lodash", "version": "4.17.21" }
          ]
        }
      ]
    }
  ]
}
```

`action` is one of `update` (package exists, will be upgraded), `add` (not yet installed), `skip` (already at target), or `pending`.

## Supported package managers

| Manager | Lockfile |
|---|---|
| npm | `package-lock.json` |
| yarn | `yarn.lock` |
| pnpm | `pnpm-lock.yaml` |

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run benchmarks
cargo bench

# Smoke-run benchmarks without timing
cargo bench -- --test
```

### Benchmarks

Benchmarks live in `benches/plan_runner.rs`. Two benchmarks are run: `plan_runner` (parallel, the default) and `plan_runner/sequential_baseline` (kept as a regression comparison). HTML reports are written to `target/criterion/`.

## Project structure

```
src/
  main.rs                    # CLI entry point (plan / apply subcommands)
  lib.rs                     # Library root (re-exports for bench/integration)
  config/                    # Config deserialization and validation
  commands/
    plan/
      runner_par.rs          # Parallel plan runner (rayon) — default
      runner.rs              # Sequential plan runner — regression baseline
      model.rs               # Plan data structures
    apply/
      runner.rs              # Apply runner (WIP)
  package_manager/
    mod.rs                   # LockfileDriver trait + PackageManagerKind enum
    npm.rs                   # npm lockfile parser
    yarn.rs                  # yarn lockfile parser
    pnpm.rs                  # pnpm lockfile parser
    package_json.rs          # package.json reader
  scm/
    git.rs                   # git2-based SCM operations
example.json                 # Minimal single-project example config
```

## License

MIT
