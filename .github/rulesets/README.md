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
- Required status checks `Engine`, `Client`, `E2E`, `cargo-deny`, with **strict**
  (branch-up-to-date) enforcement.
- Linear history; squash is the only allowed merge method.
- No force pushes and no deletion of `main`.

Only the repository **Admin** role may bypass (`bypass_actors`).

If a status check is ever renamed in `.github/workflows/`, update the matching
`context` here and re-import.
