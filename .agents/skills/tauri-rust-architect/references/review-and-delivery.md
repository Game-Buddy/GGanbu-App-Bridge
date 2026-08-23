# Review and Delivery Guide

## Contents

1. Review order
2. Severity model
3. Finding format
4. Patch workflow
5. Git and GitHub operations
6. Commit and pull-request quality
7. CI failure handling
8. Delivery summary

## 1. Review order

Review code in this order:

1. Security boundaries and data exposure
2. Correctness, data integrity, and failure behavior
3. Concurrency, lifecycle, and resource ownership
4. Performance and bounded resource use
5. Compatibility, migration, and cross-platform behavior
6. Test adequacy and observability
7. Maintainability and clarity
8. Style only when it creates real risk or contradicts repository rules

Trace important flows end to end. For a Tauri command, inspect the frontend caller, DTO, command, application service, infrastructure operation, capability permission, error mapping, tests, and shutdown behavior.

Do not approve a design solely because individual functions look idiomatic.

## 2. Severity model

Use severity based on realistic impact and reachability.

- **Critical:** Direct compromise, arbitrary code execution, signing-key exposure, broad secret theft, destructive unauthenticated action, or release-channel compromise with plausible reachability.
- **High:** Privilege escalation, unauthorized sensitive data access, durable data corruption, authentication bypass, exploitable update or local-service weakness, or reliable application-wide denial of service.
- **Medium:** Limited data exposure, meaningful reliability failure, race, resource leak, missing validation, migration hazard, or performance failure under plausible use.
- **Low:** Localized maintainability, diagnostics, test, or performance issue with limited immediate impact.
- **Suggestion:** Optional improvement with no demonstrated defect. Keep suggestions separate from findings.

Do not inflate severity to attract attention. State assumptions that affect severity.

## 3. Finding format

Present findings before general praise or summary.

Use:

```text
[Severity] Short title
Location: path/to/file.rs:line
Scenario: Concrete input, state, or attacker action that triggers the issue
Impact: User, security, correctness, or performance consequence
Why: Relevant invariant or boundary that is violated
Fix: Smallest robust remediation
Test: Regression or negative test that should prove the fix
Confidence: High, medium, or low; state missing evidence
```

Group duplicate instances under one root-cause finding. Include exact locations for representative examples.

For performance findings, include evidence or label the item as a hypothesis requiring measurement. For security findings, distinguish defense-in-depth from an exploitable path.

If no material findings exist, say so and list validation gaps or residual risks. Do not invent issues to fill a review.

## 4. Patch workflow

When authorized to edit:

1. Check status, branch, and repository instructions.
2. Reproduce or add a failing test where feasible.
3. Implement the smallest coherent change.
4. Add or update tests.
5. Run focused checks.
6. Run the full relevant suite.
7. Inspect the diff for unrelated edits, secrets, permissions, debug code, generated files, and lockfile changes.
8. Update proportional documentation.
9. Summarize evidence and limitations.

Preserve unrelated local modifications. Do not overwrite user work. Avoid broad automated formatting outside touched files when it creates review noise.

For a refactor, preserve behavior with characterization tests before moving code. Separate mechanical movement from behavior changes when that makes review safer.

## 5. Git and GitHub operations

Use connected GitHub tools when available for remote repositories, pull requests, review threads, issues, and CI. Use local Git for workspace inspection and code changes.

Read repository instructions and existing pull-request context before acting. Address review comments at their root cause, not only the exact line mentioned.

Require explicit authorization before:

- Pushing to a remote
- Opening or updating a pull request when not already requested
- Requesting reviewers
- Merging
- Closing issues or pull requests
- Creating a release or tag
- Modifying branch protection or repository settings
- Force-pushing, rebasing shared history, resetting, deleting branches, or reverting others' work

Never include credentials in remotes, commands, patches, commits, comments, or logs.

If the user authorizes repository editing and pull-request preparation, it is acceptable to create a focused branch, commit, push, and open the pull request as one workflow when the available tool's confirmation policy permits it. Report every externally visible action.

## 6. Commit and pull-request quality

Keep commits focused and buildable where practical.

A commit message should explain the outcome and reason, not implementation trivia. Follow repository convention; otherwise use a concise imperative subject and a body for security, compatibility, or migration context.

A pull request should contain:

- Problem and user impact
- Chosen design and important alternatives
- Security impact and capability changes
- Performance impact and measurement
- Compatibility, migration, and rollback
- Tests and platform validation
- Screenshots or recordings only when UI behavior changes
- Remaining limitations or follow-ups

Call out capability, CSP, updater, signing, local-port, shell, filesystem, or secret-storage changes explicitly. Reviewers should not have to discover privilege expansion in a JSON diff.

Do not bundle unrelated dependency upgrades. Explain every lockfile change.

## 7. CI failure handling

When CI fails:

1. Identify the first causal failure, not only the final canceled job.
2. Compare with the base branch or known existing failures.
3. Reproduce locally when feasible.
4. Fix code or configuration; do not weaken a gate without a justified decision.
5. Re-run the narrow failing command, then the relevant suite.
6. Document platform-only failures and evidence.

Do not suppress lints globally, skip tests, reduce permission checks, or ignore audit findings merely to obtain a green build.

For flaky tests, identify the race or environmental dependency. A blind retry can mask a correctness issue and is not the default fix.

## 8. Delivery summary

Use this compact structure for completed implementation work:

```text
Result
- What now works

Architecture
- Important boundaries and decisions

Security
- Controls added or changed
- Capability or trust-boundary impact

Performance
- Expected impact and measured evidence, or why no material impact is expected

Validation
- Commands and tests run
- Platforms validated

Limitations
- Checks not run, residual risks, or follow-up work
```

Keep the summary factual. Link to files, commits, pull requests, ADRs, and benchmark output when available.
