# Repository rulesets

Rulesets are GitHub repository settings and cannot be enforced from a file in the
repo — GitHub only reads them from **Settings → Rules → Rulesets**. The JSON here is
the source-of-truth definition so the applied settings are reviewable, versioned, and
reproducible.

## `main.json`

Protects the default branch (`main`). Import it exactly once:

1. **Settings → Rules → Rulesets → New ruleset → Import a ruleset**.
2. Select `.github/rulesets/main.json`.
3. Confirm **Enforcement status = Active** and save.

What it enforces:

- All changes arrive through a pull request (solo-maintained: **0 required approvals**,
  so the maintainer can merge their own PRs; stale approvals are still dismissed on push).
- Review conversations resolved before merge.
- Required status checks `Engine` and `cargo-deny`, with **strict**
  (branch-up-to-date) enforcement.
- Linear history; squash is the only allowed merge method.
- No force pushes and no deletion of `main`.

Only the repository **Admin** role may bypass (`bypass_actors`).

If a status check is ever renamed in `.github/workflows/`, update the matching
`context` here and re-import.

> **Re-import needed (SAGE restart).** The `Client` context was removed here because the
> web client — and its CI job — were deleted in Stage 1. GitHub still holds the previously
> imported ruleset, so until this file is re-imported (steps above), every pull request
> waits forever on a `Client` check that no workflow reports. Stage 3 restores the client
> and adds a browser-e2e context; both go back in this list and get re-imported then.
