# Running the agent workflow with a local model

This guide shows how to reproduce the RUNE agent loop — **create an issue →
implement it on a branch → open a PR** — on your own machine, driven by a local
model served by [Ollama](https://ollama.com) instead of a cloud provider.

It documents the *developer-automation* workflow. It is unrelated to the in-game
LLM opponent described in `docs/brief.md`.

> **Reality check.** A ~30B local model (e.g. `qwen3-coder:30b`) can tool-call and
> drive this loop, but it will **not** reliably one-shot green CI on a strict
> Rust + TypeScript repo gated by `make check`. Treat it as a supervised draft
> generator: keep tasks tiny, review every diff, and expect to re-prompt. The
> manual-trigger setup below keeps you in control on purpose.

## The four layers

The cloud agents that open PRs on this repo are four independent layers stacked
together. Only the first is hard to run locally, and you already have it:

| Layer | What a cloud agent uses | Local equivalent |
| --- | --- | --- |
| **Model** | A frontier model over the network | `qwen3-coder:30b` via Ollama |
| **Agent harness** — clone, branch, edit, run `make check` / `make verify`, commit | A hosted agent runtime | A terminal agent such as [OpenCode](https://opencode.ai) pointed at Ollama |
| **GitHub glue** — create the issue, open the PR | GitHub API / MCP | the `gh` CLI |
| **Trigger** — decide *when* an agent runs | A webhook + runner | you run a command (`scripts/local-agent.sh`) |

Everything below wires up layers 1–4 for a **manual trigger**: you pick an issue
and run one command. Making it fully autonomous (a self-hosted Actions runner or a
daemon that watches for `agent-task` + `status:ready` issues) is a later step that
reuses the same harness — see [Going autonomous](#going-autonomous).

## Step 1 — Tune Ollama for agentic use

The biggest gotcha is the **context window**. Ollama defaults to a small
`num_ctx`; an agent that stuffs file contents, tool schemas, and history into the
prompt will silently truncate and behave erratically. Bake a larger context into a
named model:

```dockerfile
# Modelfile
FROM qwen3-coder:30b
PARAMETER num_ctx 32768        # 32k minimum; raise it if VRAM/RAM allows
PARAMETER temperature 0.1      # low temperature → more deterministic edits
```

```sh
ollama create qwen3-coder-agent -f Modelfile
ollama run qwen3-coder-agent   # sanity-check it responds
```

Ollama already exposes an OpenAI-compatible API at `http://localhost:11434/v1`,
which is what the harness in Step 3 talks to.

## Step 2 — Authenticate GitHub

```sh
gh auth login          # or export a PAT with the `repo` and `workflow` scopes
gh auth status
```

Running locally with **your own** `gh` credentials sidesteps a RUNE-specific
pitfall: `docs/agents/workflow.md` notes that PRs opened by the default
`GITHUB_TOKEN` / `github-actions[bot]` do **not** trigger CI (recursion
protection). Because your PRs are authored by you, the `Engine` and `Client`
checks run normally — no extra configuration needed.

## Step 3 — Point the harness at Ollama

Install OpenCode (see its docs), then register Ollama as a local provider.
`opencode.json`, roughly (verify field names against the current OpenCode docs —
the schema evolves):

```json
{
  "provider": {
    "ollama": {
      "npm": "@ai-sdk/openai-compatible",
      "options": { "baseURL": "http://localhost:11434/v1" },
      "models": { "qwen3-coder-agent": { "name": "Qwen3 Coder 30B (local)" } }
    }
  },
  "model": "ollama/qwen3-coder-agent"
}
```

Run `opencode` in the repo root and confirm it can read a file and run a shell
command (ask it to run `make check`). That proves the tool-calling round-trip
works with your model — the make-or-break test for any local model.

## Step 4 — The two flows

### Flow A — the model drafts an issue

RUNE forces a template (`.github/ISSUE_TEMPLATE/agent-task.yml`; blank issues are
disabled), so the model must produce real field values:

- **Goal** — one sentence: what exists after this task that does not now?
- **Area** — one of `engine, protocol, server, cli, client, docs, ci`.
- **Acceptance criteria** — checkable, CI-verifiable statements; include
  `- [ ] make check green`.
- **In scope / out of scope** — files expected to change, and what must not.

Have the model draft the body, review it, then:

```sh
gh issue create --template agent-task.yml   # or --body-file <model-output>
```

### Flow B — an agent implements the issue and opens a PR

This is exactly what `scripts/local-agent.sh.example` does. The shape:

1. Read the issue with `gh issue view`.
2. Branch `agent/<issue>-<slug>` off `main` (the convention in
   `docs/agents/workflow.md`).
3. Hand the issue to the harness (`opencode run …`) with instructions to make the
   smallest change that satisfies the acceptance criteria and to run `make check`
   (the fast inner-loop gate) as it works.
4. **Run the gates yourself** before pushing — never trust the model's word that it
   passed. `make check` mirrors the `Engine` and `Client` CI jobs; `make verify`
   additionally runs the `E2E` and `cargo-deny` jobs, so it reproduces the full
   required-check surface locally. Run `make verify` before opening the PR when your
   machine can run the browser suite (`scripts/bootstrap.sh` confirms the
   prerequisites).
5. Push and open a PR with `gh pr create --fill --body "Closes #<n>"`; the PR
   template applies automatically.

Copy the example, drop the `.example` suffix, make it executable, and run
`./scripts/local-agent.sh 98`.

## Where your other tools fit

- **Cline / Continue (VS Code)** — *interactive* implementers. You watch them work,
  which is more reliable per task than an unattended local model. Good for the
  implementation step when you want oversight; they are not the autonomous loop.
- **Open WebUI** — a chat surface over Ollama, handy for drafting an issue body or
  a commit message by hand. Not part of the automated loop.
- **Aider** — an alternative harness to OpenCode. It is git-native (every edit is a
  commit) and tends to be the most robust option with local models; swap the
  `opencode run …` line in the script for an `aider --model …` invocation and keep
  the rest identical.

## Going autonomous

When you trust the loop, replace the "you run the script" trigger with either:

- a **self-hosted GitHub Actions runner** on your machine, so a workflow can reach
  `http://localhost:11434`; or
- a small **polling daemon** that watches for issues labeled `agent-task` +
  `status:ready` and invokes the same script.

The harness, the `make check` gate, and the PR flow stay the same — only the
trigger changes. A human still reviews and merges every PR (branch protection on
`main` requires it).
