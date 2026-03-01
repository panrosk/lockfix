# Vulnerability Update CLI - Product Summary

## Overview

This tool is a CLI designed to execute a predefined remediation plan for vulnerable dependencies across multiple npm projects. The initial goal is not to discover vulnerabilities by itself, but to take an explicit input file that lists projects and package versions to update, then apply those changes safely and consistently.

The planned workflow is:

1. Read a configuration JSON file.
2. Iterate through the listed projects.
3. Prepare the project environment, including `.npmrc` if needed.
4. Create a dedicated fix branch.
5. Update the requested packages to the exact target versions.
6. Regenerate lockfiles and run verification steps.
7. Commit and push changes.
8. Open a PR or MR.
9. Generate a final summary of what succeeded, failed, or was skipped.

## Why this approach is strong

This design is strong because it separates discovery from execution. Instead of letting the CLI guess what to update, the user provides an explicit plan. That gives you:

- Predictability
- Repeatability
- Better auditability
- Easier review before execution
- Lower risk in corporate environments

It also makes the tool a good fit for batch execution across many repositories.

## Suggested scope for v1

The first version should stay narrow and reliable.

Recommended v1 scope:

- npm only
- Direct dependencies only
- Exact version updates only
- Standard git repositories only
- One SCM provider first, ideally GitLab or GitHub
- Summary output in terminal and JSON
- Dry-run support

Things to avoid in v1:

- Auto-discovery of vulnerabilities
- Multi-ecosystem support
- Complex monorepo logic
- Transitive dependency remediation through overrides
- Major upgrade migration logic
- Automatic rollback logic beyond basic git safety checks

## Recommended input schema direction

The original JSON idea is good, but it should be extended to make behavior explicit.

Suggested fields:

- `root`: root folder containing projects
- `npmrcTemplate`: template path for private registry config
- `fixBranchTemplate`: branch naming template
- `gitUserName` and `gitUserEmail`
- `scm`: provider, target branch, labels, create PR or MR toggle
- `defaults`: behavior flags and default commands
- `projects`: project list

Each project should ideally include:

- `id`
- `path`
- `packageManager`
- `baseBranch`
- `packages`
- `commands.install`
- `commands.test`
- `commands.build`

## Important design decisions

### 1. Exact version mode first

For the first version, the JSON file should be treated as an exact instruction set.

Example:

- `lodash -> 4.17.23`

The CLI should attempt that exact version and report failure if it cannot apply it. This is much simpler and safer than letting the tool resolve versions heuristically.

### 2. Only direct dependencies in v1

If a package is only present as a transitive dependency, the CLI should not try to fix it automatically in v1. It should report something like:

- package not found in direct dependencies
- manual remediation required

This avoids introducing unsafe behavior too early.

### 3. Use the package manager to apply updates

For npm projects, prefer using npm commands instead of editing `package.json` manually. This reduces the risk of desynchronizing the manifest and the lockfile.

### 4. `.npmrc` handling must be safe

Since the tool may depend on a corporate registry setup:

- never commit `.npmrc`
- avoid logging secrets
- back up existing `.npmrc` if needed
- restore project state after execution if appropriate

### 5. Git flow should be explicit

Recommended sequence:

1. Verify the directory is a git repository.
2. Verify working tree state.
3. Checkout base branch.
4. Pull or rebase if desired.
5. Create fix branch.
6. Apply package updates.
7. Run verification.
8. Commit if there are real changes.
9. Push.
10. Create PR or MR.

## Failure modes to account for

The CLI should not think only in terms of success or failure. It should support:

- `success`
- `partial_success`
- `failed`
- `skipped`

Common failure cases:

- repo is dirty
- branch already exists
- package not found
- package is transitive only
- npm install fails
- tests fail
- build fails
- push fails
- PR creation fails
- version already applied
- no changes detected after attempted update

## Verification strategy

A good dependency update tool must verify that the repo still works.

Recommended verification layers:

- install
- test
- build

These should be configurable per project, because not all repos expose the same scripts.

## Reporting and summary output

The summary is one of the most valuable outputs of the tool.

For each project, the report should include:

- requested package changes
- successfully applied package changes
- failed package changes
- skipped changes
- verification results
- whether a branch was created
- whether a commit was created
- whether push succeeded
- whether PR or MR creation succeeded
- PR or MR URL if available

Recommended outputs:

- terminal summary for humans
- JSON summary for automation
- optional Markdown report later

## Dry-run is essential

Before touching repositories, the tool should support dry-run mode. It should show:

- projects detected
- package changes planned
- commands that would run
- git actions planned
- warnings such as dirty repos or missing direct dependencies

This will make the tool far safer and easier to trust.

## Proposed internal architecture

The CLI should be organized as a pipeline, not a monolith.

Suggested phases:

1. Parse input
2. Validate config
3. Build execution plan
4. Prepare repo
5. Apply dependency updates
6. Verify project
7. Commit and push changes
8. Create PR or MR
9. Produce summary

Suggested domain objects:

- `ExecutionConfig`
- `ProjectPlan`
- `PackageUpdateRequest`
- `PackageUpdateResult`
- `VerificationResult`
- `GitResult`
- `PullRequestResult`
- `ProjectExecutionResult`
- `RunSummary`

## Adapter-based extensibility

If you want the tool to become extensible later, structure it around adapters.

Suggested adapters:

- `ProjectAdapter` for npm now, more ecosystems later
- `GitAdapter` for branch, commit, push operations
- `ScmAdapter` for GitLab or GitHub PR or MR creation
- `Reporter` for terminal, JSON, and later Markdown or SARIF

This would let you start with:

- `NpmProjectAdapter`
- `GitCliAdapter`
- `GitLabMergeRequestAdapter`

And later add:

- `PnpmProjectAdapter`
- `CargoProjectAdapter`
- `GitHubPullRequestAdapter`

## Recommended CLI command structure

A clean command structure could be:

- `validate`
- `plan`
- `apply`
- `report`

Example intent:

- `validate` checks config and repo readiness
- `plan` builds a detailed execution plan
- `apply` performs the updates
- `report` renders stored run results

## Product positioning

It is better to describe this tool as:

"A CLI that executes planned dependency vulnerability remediations across npm repositories, including git automation, PR creation, and reporting."

That is more precise and trustworthy than saying it "automatically fixes vulnerabilities".

## Recommended build order

A practical implementation order would be:

1. Config parser and validator
2. Single-project executor
3. npm direct dependency updater
4. Git branch and commit support
5. Summary JSON output
6. Push support
7. PR or MR creation
8. Multi-project execution
9. Dry-run improvements
10. Ecosystem adapters

## Main risk to avoid

The biggest risk is false confidence. A tool like this can look powerful while silently producing broken branches or meaningless PRs.

To avoid that, the tool should:

- only do what it clearly understands
- report what it could not do
- fail safely
- validate before mutation
- verify after mutation
- avoid touching transitives in v1

## Final recommendation

Build v1 as a reliable plan executor for npm repositories. Make it deterministic, auditable, and safe. Once that works well, expand toward:

- plan generation from vulnerability scanners
- transitive dependency strategies
- additional package managers
- richer reporting formats
- monorepo support

That path gives you a strong internal tool without overcomplicating the first release.
