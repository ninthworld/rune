# ADR 0001: Record architecture decisions

- Status: accepted
- Date: 2026-07-10

## Context
This repository is developed primarily by AI agents across many short-lived
sessions. Decisions that live in chat history or reviewers' heads are lost to
the next session; agents then re-litigate or silently violate them.

## Decision
Every architectural decision is recorded as a numbered ADR in this directory
using 0000-template.md. AGENTS.md files state rules; ADRs state why. PRs that
change architecture must add or update an ADR in the same PR.

## Consequences
Slight overhead per decision; in exchange, agents can cite and follow decisions
deterministically, and humans can audit why the codebase is shaped as it is.
