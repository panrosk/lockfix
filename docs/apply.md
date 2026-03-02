# Apply Command Documentation

The `apply` command executes the dependency upgrades described in a plan. It can accept either a config file (generates a plan internally) or a pre-generated plan file.

## Overview

```bash
# Apply from config (generates plan on the fly)
lockfix apply --config path/to/lockfix.json

# Apply from pre-generated plan
lockfix apply --plan path/to/plan.json
```

## Execution Flow

```
┌──────────────────────────────────────────────────────────────┐
│                     START APPLY                              │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│ Load config or plan                                          │
│ - Read config file OR read plan JSON                         │
│ - Resolve git user configuration                             │
│ - Resolve root path                                          │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│ FOR EACH PROJECT:                                            │
│                                                              │
│  1. Resolve package manager                                  │
│  2. Resolve project path                                     │
│  3. Open Git repository                                      │
│  4. Fetch base branch                                        │
│  5. Checkout and reset to base branch                        │
│  6. Create fix branch                                        │
│                                                              │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│ FOR EACH PACKAGE:                                            │
│                                                              │
│  Process action:                                             │
│  - skip    → Record success, no changes                      │
│  - error   → Record error with reason, continue              │
│  - update  → Apply the update                                │
│  - add     → Apply the update                                │
│  - pending → Apply the update                                │
│                                                              │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│ POST-PROCESSING:                                             │
│                                                              │
│  - Write package.json if any updates were made               │
│  - Check if all packages succeeded                           │
│  - If success AND changes made:                              │
│    - Stage all changes                                       │
│    - Commit with message                                     │
│    - Push fix branch to remote                               │
│                                                              │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│ OUTPUT:                                                      │
│                                                              │
│  Print summary with:                                         │
│  - Per-project status (success/partial/mismatch/error)       │
│  - Commit status                                             │
│  - Per-package results                                       │
│  - Totals                                                    │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

## Action Handling

### Skip Action

When `action: "skip"`, the package already satisfies the requirements:

```json
{
  "action": "skip",
  "currentVersion": "4.17.23",
  "targetVersion": "4.17.21",
  "updatePolicy": "minimum"
}
```

The apply command:
1. Does NOT modify `package.json`
2. Does NOT run npm install
3. Records a successful result with `version_matched: true`
4. No changes to the repository

### Error Action

When `action: { "error": { "reason": "..." } }`, the plan detected an invalid configuration:

```json
{
  "action": { "error": { "reason": "downgrade not allowed for exact policy: 6.14.2 -> 6.14.1" } },
  "currentVersion": "6.14.2",
  "targetVersion": "6.14.1",
  "updatePolicy": "exact"
}
```

The apply command:
1. Does NOT modify `package.json`
2. Does NOT run npm install
3. Records an error result with the reason
4. Continues processing other packages
5. Does NOT commit the project

### Update/Add/Pending Actions

These actions trigger the actual update process:

```json
{
  "action": "update",
  "currentVersion": "4.17.20",
  "targetVersion": "4.17.23",
  "updatePolicy": "exact"
}
```

The apply command:
1. Updates `package.json` with the target version
2. Runs the package manager's apply logic
3. Verifies the installed version
4. Handles version mismatches if necessary

## Git Operations

### Branch Management

For each project:

1. **Fetch**: `git fetch origin {base_branch}`
2. **Reset**: `git checkout {base_branch} && git reset --hard origin/{base_branch}`
3. **Create branch**: `git checkout -b {fix_branch}`

This ensures a clean starting point for changes.

### Commit and Push

After processing all packages in a project:

```
IF all packages succeeded AND at least one package needed changes:
    stage all changes
    commit: "fix: update dependencies for {project_name}"
    push to remote
```

If any package failed or was skipped (no changes), no commit is created.

## Apply Result Structure

Each package produces an `ApplyResult`:

```rust
struct ApplyResult {
    package: String,           // Package name
    target_version: String,    // Target version
    audit_fix_ran: bool,       // Was npm audit fix executed?
    audit_fix_success: bool,   // Did audit fix succeed?
    version_matched: bool,     // Does installed version match target?
    lockfile_deleted: bool,    // Was lockfile deleted during recovery?
    node_modules_deleted: bool,// Was node_modules deleted during recovery?
    update_ran: bool,          // Was npm update run during recovery?
    final_status: ApplyStatus, // Final outcome
    error_reason: Option<String>, // Reason for planned error
}
```

### Apply Status Values

| Status | Description |
|--------|-------------|
| `Success` | Version matched on first try |
| `PartialSuccess` | Version matched after recovery (delete + reinstall) |
| `VersionMismatch` | Version still doesn't match after all attempts |
| `PlannedError` | Plan action was error (invalid configuration) |

## Required Package Handling

When a package has `required: true` and results in `VersionMismatch`:

```
1. Add project summary to output
2. Print summary so far
3. Return error immediately
4. Stop processing remaining projects
```

This ensures critical packages don't silently fail.

## Running from Config vs Plan

### From Config

```rust
fn run_from_config(config_path: &str) -> Result<(), ApplyError>
```

1. Load config file
2. Get git user from config
3. For each project in config:
   - Process each package directly
   - No plan action checking (assumes update needed)

### From Plan

```rust
fn run_from_plan(plan_path: &str) -> Result<(), ApplyError>
```

1. Load plan JSON
2. Load original config (for git user, package manager settings)
3. For each project in plan:
   - Process each package based on its `action`
   - Skip packages with `action: "skip"`
   - Report errors for `action: "error"`

## Summary Output

```
═══════════════════════════════════════════════════════════
                      APPLY SUMMARY                         
═══════════════════════════════════════════════════════════

✓ my-project (committed) - 2 success, 0 partial, 0 mismatch, 0 error
  ✓ lodash @ 4.17.23 [success]
  ✓ axios @ 1.6.0 [success]

✗ other-project (not committed) - 1 success, 1 partial, 1 mismatch, 1 error
  ✓ lodash @ 4.17.21 [success]
  ⚠ react @ 18.2.0 [partial]
  ✗ vue @ 3.4.0 [mismatch]
  ! qs @ 6.14.1 [error]: downgrade not allowed for exact policy: 6.14.2 -> 6.14.1

───────────────────────────────────────────────────────────
Total: 5 packages | 3 success | 1 partial | 1 mismatch | 1 error
═══════════════════════════════════════════════════════════
```

### Status Icons

| Icon | Status | Meaning |
|------|--------|---------|
| ✓ | Success | Package updated correctly |
| ⚠ | Partial | Needed recovery but succeeded |
| ✗ | Mismatch | Version doesn't match after all attempts |
| ! | Error | Plan indicated invalid configuration |

## Error Types

| Error | Cause |
|-------|-------|
| `Config` | Failed to read/parse config file |
| `Git` | Git operations failed (fetch, checkout, commit, push) |
| `PackageJsonRead` | Failed to read `package.json` |
| `Apply` | Package manager command failed |
| `NoGitUser` | Missing git user configuration |
| `VersionMismatch` | Required package has version mismatch |
| `PlanRead` | Failed to read plan file |
| `PlanParse` | Failed to parse plan JSON |

## Recovery Process

When version doesn't match after initial install:

```
1. Delete node_modules directory
2. Delete package-lock.json
3. Run npm update
4. Check version again
5. If matches: PartialSuccess
   If still mismatch: VersionMismatch
```

This handles cases where npm's resolution logic installs a different version than requested.
