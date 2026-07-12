/**
 * Turns a provider's machine-readable event stream into something a human can watch.
 *
 * Claude Code's print mode emits nothing at all until the run is over — so a 40-minute run looks
 * exactly like a hung one. `--output-format stream-json` fixes that, at the cost of emitting JSON
 * nobody wants to read. This renders each event as one short line: what it is doing, right now.
 *
 * Rendering is best-effort by design. A provider's event schema is not a contract the runner
 * controls, so an event shape this does not recognise is *skipped*, never fatal — the raw stream is
 * kept alongside in `provider.jsonl` for anyone who needs the detail.
 */

/** A one-line summary of a tool call: the file, the command, the pattern — whatever identifies it. */
function toolSummary(name, input = {}) {
  const target =
    input.file_path ??
    input.path ??
    input.command ??
    input.pattern ??
    input.url ??
    input.prompt ??
    input.description ??
    "";
  const short = String(target).replace(/\s+/g, " ").trim();
  return short ? `${name}: ${short.slice(0, 100)}` : name;
}

function firstLine(text) {
  const line = String(text).trim().split("\n")[0];
  return line.length > 160 ? `${line.slice(0, 160)}…` : line;
}

/**
 * Renders one line of Claude Code's `stream-json` output, or null if there is nothing worth
 * showing. Non-JSON lines are passed through: a crash or a CLI warning is not JSON, and is
 * precisely the thing you most need to see.
 */
export function renderClaudeEvent(line) {
  const trimmed = line.trim();
  if (!trimmed) return null;
  if (!trimmed.startsWith("{")) return trimmed;

  let event;
  try {
    event = JSON.parse(trimmed);
  } catch {
    return trimmed;
  }

  if (event.type === "system" && event.subtype === "init") {
    return `▸ session started${event.model ? ` (${event.model})` : ""}`;
  }

  if (event.type === "assistant") {
    const parts = event.message?.content ?? [];
    const lines = [];
    for (const part of parts) {
      if (part.type === "text" && part.text?.trim()) lines.push(`  ${firstLine(part.text)}`);
      if (part.type === "tool_use") lines.push(`▸ ${toolSummary(part.name, part.input)}`);
    }
    return lines.length > 0 ? lines.join("\n") : null;
  }

  if (event.type === "result") {
    const cost = typeof event.total_cost_usd === "number" ? `, $${event.total_cost_usd.toFixed(2)}` : "";
    const turns = typeof event.num_turns === "number" ? `, ${event.num_turns} turns` : "";
    return `▸ finished: ${event.subtype ?? "done"}${turns}${cost}`;
  }

  // tool_result events (type: "user") are the tool's output echoed back. They are bulky and the
  // interesting half — what was run — was already shown by the tool_use above.
  return null;
}

/** Providers without a structured stream just print. Their output is already the human-readable form. */
export function renderRaw(line) {
  return line.trim() ? line : null;
}
