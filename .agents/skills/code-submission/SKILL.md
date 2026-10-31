---
name: code-submission
description: Prepare RedAlert2 branches, commit messages and pull requests when the user requests a submission or Git operation.
---

# Code submission

The project repository is `ConchFeng/RedAlert2`. Check the checkout, branch, remotes
and staged/unstaged changes before a Git mutation. Preserve other work, including
its staging. Editing files does not authorize a commit, push, PR or merge; perform
only the publication actions the user authorized. An explicit no-commit request
continues to apply until the user changes it.

Use `feature/<topic>` for project branches, or the user's requested name. Start
new work from current `origin/main`; continue an existing task branch when suitable.
Reuse the current clean checkout or an appropriate worktree. Create isolation only
when needed, using the host's worktree tools when available.

When commits are authorized:

- Stage only the task's changes after reviewing their diff. Never use blanket
  staging when unrelated edits exist. Inspect the complete commit candidate;
  exclude unrelated staged content with a path-limited commit or isolated index
  while preserving its staging. Never include retail assets, credentials,
  generated builds or scratch reports.
- Use `type(scope): summary`, with an optional scope. Types are `feat`, `fix`,
  `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore` and `revert`.
- Write an imperative American English summary, preferably within 72 characters.
  Add a body only for the reason, important tradeoffs, native evidence or validation
  that a reviewer needs. Include an issue number only when the task has one.
- Keep a commit cohesive. Do not combine unrelated cleanup with a behavior change.
  Preserve contributor credit; use verified GitHub identities and never invent
  names, email addresses or co-author trailers.

For a requested PR, target `ConchFeng/RedAlert2` and `main` explicitly. Lead with the
problem and resulting behavior, then the reason and actual validation. Mention
native addresses for gamemd-derived behavior and material residuals. Keep simple
PRs to a few sentences; do not paste run logs or speculative completion claims.
Use `Fixes #N` only when the change actually closes that fork's issue. Historical
upstream issue numbers do not identify issues in this repository.

For authored changes or an authorized submission, run the checks required in
AGENTS.md and inspect their exit status.
Do not repeat passing checks without a new reason. If the user requests a review,
read the diff and its consumers and report concrete defects; an independent critic
is optional unless requested or justified by the risk. Do not run a mandatory
security scanner, build matrix or integration-slot procedure for every PR.

Before an authorized merge, verify the final PR head, checks, merge method and
contributor attribution. Read back the resulting commit. Never force-push or
rewrite shared history without explicit authorization.
