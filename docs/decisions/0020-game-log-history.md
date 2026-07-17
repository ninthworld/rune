# ADR 0020: Structured game-log history

- Status: accepted
- Date: 2026-07-17
- Issue: #259

## Context

Game views are full snapshots. A client-side accumulated activity history would make
a reconnect or fresh mount depend on messages it did not receive.

## Decision

The pure engine appends sequence-numbered, structured public facts to `GameState`.
It retains the latest 200 entries. The server projects that bounded window into every
`GameView`, applying the same hidden-information policy as other view data. Event
payloads contain typed entity references and data, never pre-rendered prose; clients
compose their own readable text.

## Consequences

Fresh clients render exactly the history carried by their first view, with no
load-bearing local accumulation. The bounded window limits snapshot size but does not
provide a complete match transcript. Adding an event requires engine emission,
receiver-safe projection, and protocol documentation.
