# ADR 0005: License selection

- Status: accepted
- Date: 2026-07-10

## Context
The brief requires the project remain free and non-monetized (WotC fan content
policy). Prior art: Forge is GPL; XMage is MIT. Copyleft (GPL/AGPL) structurally
prevents closed monetized forks; permissive (MIT/Apache-2.0) maximizes adoption
including in the planned Tauri/mobile embeddings.

## Decision
The project is licensed under the **MIT License**.

MIT is permissive: it does not itself forbid commercial or closed-source
redistribution. That is intentional and does not conflict with the brief's
"must remain free" constraint — that constraint is a WotC fan-content posture
governing how *this* project is distributed (no monetization, no card images, no
official branding), not a property of the source-code license. The rules
implementation is our own copyright, and MIT matches the long-standing prior art
(XMage) that has operated in this grey zone for 15+ years. MIT was chosen over
Apache-2.0 for brevity and over GPL to keep the planned embeddings (Tauri,
mobile) frictionless.

The copyright line reads "the RUNE authors" so contributions accrue to the
project collectively; a maintainer may substitute a legal name if preferred.

## Consequences
- `LICENSE` carries the standard MIT text; `Cargo.toml` sets `license = "MIT"`
  (inherited by every crate via `license.workspace = true`).
- The earlier hold on accepting third-party contributions is lifted: inbound
  contributions are under MIT (inbound=outbound), and any vendored third-party
  code must be MIT-compatible.
- The non-monetization / no-card-images / no-WotC-branding rules from the brief
  still apply to distribution regardless of the code license.
