import assert from "node:assert/strict";
import { test } from "node:test";

import { renderClaudeEvent, renderRaw } from "./events.js";

const event = (obj) => renderClaudeEvent(JSON.stringify(obj));

test("a tool call renders as one line naming what it is doing", () => {
  assert.equal(
    event({ type: "assistant", message: { content: [{ type: "tool_use", name: "Edit", input: { file_path: "crates/rune-engine/src/lib.rs" } }] } }),
    "▸ Edit: crates/rune-engine/src/lib.rs",
  );
  assert.equal(
    event({ type: "assistant", message: { content: [{ type: "tool_use", name: "Bash", input: { command: "make check" } }] } }),
    "▸ Bash: make check",
  );
});

test("assistant prose renders as its first line, not a wall of text", () => {
  const rendered = event({
    type: "assistant",
    message: { content: [{ type: "text", text: "I'll start by reading the engine.\n\nThen I will…" }] },
  });
  assert.equal(rendered, "  I'll start by reading the engine.");
});

test("the session start and the result are visible", () => {
  assert.match(event({ type: "system", subtype: "init", model: "claude-opus-4-8" }), /session started \(claude-opus-4-8\)/);
  assert.match(event({ type: "result", subtype: "success", num_turns: 12, total_cost_usd: 0.42 }), /finished: success, 12 turns, \$0\.42/);
});

test("tool results are not echoed — the interesting half was already shown", () => {
  // The tool's *output* is bulky; what it *ran* was rendered by the tool_use above it.
  assert.equal(event({ type: "user", message: { content: [{ type: "tool_result", content: "…10k of file…" }] } }), null);
});

test("an unrecognised event is skipped, never fatal", () => {
  // A provider's event schema is not a contract the runner controls, so it must not be able to
  // crash a run by adding a field.
  assert.equal(event({ type: "something_new_in_a_future_release" }), null);
  assert.equal(renderClaudeEvent(""), null);
});

test("non-JSON output is passed through, because that is where the crash will be", () => {
  assert.equal(renderClaudeEvent("Error: ENOSPC: no space left on device"), "Error: ENOSPC: no space left on device");
  assert.equal(renderClaudeEvent("{not json"), "{not json");
});

test("a very long line is truncated rather than flooding the terminal", () => {
  const rendered = event({ type: "assistant", message: { content: [{ type: "text", text: "x".repeat(500) }] } });
  assert.ok(rendered.length < 200, `was ${rendered.length}`);
  assert.match(rendered, /…$/);
});

test("providers without a structured stream just print", () => {
  assert.equal(renderRaw("running tests\n"), "running tests\n");
  assert.equal(renderRaw("   "), null);
});
