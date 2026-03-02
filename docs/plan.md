# Plan Command Documentation

The `plan` command generates a JSON document describing what changes will be made to each project's dependencies. It inspects lockfiles and determines the appropriate action for each package without making any modifications.

## Overview

```bash
lockfix plan --config path/to/lockfix.json
```

The plan command:
1. Reads the configuration file
2. For each project, reads the lockfile and `package.json`
3. Determines the current version of each package
4. Calculates the appropriate action based on version comparison and update policy
5. Outputs a JSON plan document

## Plan Output Structure

```json
{
  "generatedAt": "2026-03-02T09:06:23.721085+00:00",
  "configPath": "lockfix.json",
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

## Action Determination Logic

The `action` field is the core output of the plan. It determines what the apply command will do with each package.

### Action Types

| Action | Description |
|--------|-------------|
| `update` | Package exists and needs to be upgraded |
| `add` | Package is not installed, will be added |
| `skip` | Package already satisfies requirements, no action needed |
| `error` | Invalid configuration (e.g., downgrade with exact policy) |
| `pending` | Cannot determine action without inspecting project first |

### Decision Flow

```
┌─────────────────────────────────────┐
│ Are there any instances in lockfile? │
└─────────────────┬───────────────────┘
                  │
         ┌────────┴────────┐
         │                 │
        Yes               No
         │                 │
         ▼                 ▼
┌─────────────────┐  ┌─────────────┐
│ Check versions  │  │ action: add │
└────────┬────────┘  └─────────────┘
         │
         ▼
┌─────────────────────────────────────┐
│ What is the update policy?          │
└─────────────────┬───────────────────┘
                  │
         ┌────────┴────────┐
         │                 │
     minimum             exact
         │                 │
         ▼                 ▼
┌─────────────────┐  ┌─────────────────────┐
│ current >=      │  │ current == target?  │
│ target?         │  │                     │
└────────┬────────┘  └──────────┬──────────┘
         │                      │
    ┌────┴────┐           ┌─────┴─────┐
   Yes       No          Yes         No
    │         │           │           │
    ▼         ▼           ▼           ▼
┌───────┐ ┌───────┐  ┌───────┐  ┌─────────────┐
│ skip  │ │update │  │ skip  │  │ current >   │
└───────┘ └───────┘  └───────┘  │ target?     │
                               └──────┬──────┘
                                      │
                                 ┌────┴────┐
                                Yes       No
                                 │         │
                                 ▼         ▼
                           ┌─────────┐ ┌───────┐
                           │ error   │ │update │
                           └─────────┘ └───────┘
```

### Version Comparison

The version comparison uses semantic versioning principles:

1. **Parsing**: Version strings are cleaned of prefixes (`v`, `^`, `~`) and split by `.`
2. **Comparison**: Each numeric segment is compared left-to-right
3. **Missing segments**: Treated as `0`

Example comparisons:
- `4.17.21` < `4.17.23` → needs update
- `6.14.2` > `6.14.1` → downgrade scenario
- `2.0.0` == `2.0.0` → skip

### Update Policies

#### Minimum Policy

The `minimum` policy means "at least this version or higher". This is typically used for security fixes where any version at or above the target is acceptable.

```json
{
  "updatePolicy": "minimum",
  "targetVersion": "4.17.21"
}
```

- Current: `4.17.20` → Action: `update` (below target)
- Current: `4.17.21` → Action: `skip` (meets minimum)
- Current: `4.17.22` → Action: `skip` (above minimum, acceptable)

The apply command will use `npm install package@>=4.17.21` for minimum policy.

#### Exact Policy

The `exact` policy means "exactly this version". This is used when a specific version is required, often for reproducibility or when newer versions have breaking changes.

```json
{
  "updatePolicy": "exact",
  "targetVersion": "4.17.21"
}
```

- Current: `4.17.20` → Action: `update` (needs upgrade)
- Current: `4.17.21` → Action: `skip` (already correct)
- Current: `4.17.22` → Action: `error` (downgrade not allowed)

The apply command will use `npm install package@4.17.21` for exact policy.

### Scope Detection

The `scope` field indicates where the package is declared:

| Scope | Description |
|-------|-------------|
| `direct` | Listed in `package.json` dependencies/devDependencies |
| `transitive` | Not in manifest, but present in lockfile (dependency of dependency) |
| `auto` | Not found anywhere (will need to be added) |

Detection logic:
1. Check if package exists in `package.json` → `direct`
2. Check if package has instances in lockfile → `transitive`
3. Neither found → `auto`

### Instances Array

The `instances` array lists all copies of a package found in the lockfile:

```json
"instances": [
  { "path": "node_modules/lodash", "version": "4.17.21" },
  { "path": "node_modules/some-package/node_modules/lodash", "version": "4.17.15" }
]
```

Multiple instances indicate the package is installed at different versions (nested dependencies). The `currentVersion` field reflects the top-level instance (`node_modules/{name}`).

### Required Flag

The `required: true` flag marks packages that are critical. If a required package fails during apply:
- The entire project is marked as failed
- No commit is created
- The apply command returns an error

Non-required packages (`required: false`) can fail without blocking the project.

## Parallel Execution

The plan command processes projects in parallel using Rayon's work-stealing thread pool:

```rust
config.projects.par_iter().map(|project| { ... })
```

Each project is processed independently:
- No shared mutable state
- No locking required
- Output order may differ from input order

Projects are identified by name, not position, so order doesn't matter for consumers.

## Error Handling

The plan command can fail with:

| Error | Cause |
|-------|-------|
| `ProjectDoesNotExist` | Project path doesn't exist |
| `PackageManagerNotInstalled` | npm/yarn/pnpm not found on PATH |
| `LockfileNotFound` | Missing lockfile in project |
| `ManifestRead` | Failed to read `package.json` |
| `Parse` | Failed to parse config file |

## Summary Fields

| Field | Description |
|-------|-------------|
| `totalProjects` | Number of projects in plan |
| `totalPackages` | Total packages across all projects |
| `requiredPackages` | Packages marked as required |
