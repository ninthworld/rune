/**
 * A structural scanner for the subset of YAML that GitHub workflow files are written in.
 *
 * This is deliberately *not* a YAML parser. It does not build values, resolve anchors, or
 * know that YAML 1.1 reads a bare `on` as the boolean `true`. Real YAML validity is
 * actionlint's job — `make ci-lint` runs it first, and this scanner never gets to speak
 * about a file actionlint has already rejected. What the policy rules below actually need
 * is structural: which key sits in which block, on which line, carrying which trailing
 * comment. A scanner that answers exactly that is a hundred lines we can reason about,
 * where a hand-rolled YAML parser would be a thousand we could not — and taking on a YAML
 * dependency is not open to us, because `tools/` is dependency-free by rule (`AGENTS.md`).
 *
 * It rejects what it cannot understand rather than guessing, so a workflow this scanner
 * cannot read fails the gate; it never silently passes.
 */

export class MalformedWorkflow extends Error {
  constructor(line, message) {
    super(`line ${line}: ${message}`);
    this.line = line;
  }
}

/** Splits a trailing `# comment` off a line, respecting quotes. → [content, comment|null] */
export function splitComment(text) {
  let quote = null;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) {
      if (c === quote) quote = null;
    } else if (c === "'" || c === '"') {
      quote = c;
    } else if (c === "#" && (i === 0 || /\s/.test(text[i - 1]))) {
      return [text.slice(0, i).trim(), text.slice(i + 1).trim()];
    }
  }
  return [text.trim(), null];
}

const KEY = /^([A-Za-z_][\w.-]*):(?:\s+(.*))?$/;
const BLOCK_SCALAR = /^[|>][+-]?$/;

/** The indentation of the next line carrying content, sequence dashes counted in. */
function nextContentIndent(lines, from) {
  for (let i = from; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    let indent = lines[i].length - lines[i].trimStart().length;
    if (trimmed.startsWith("- ")) indent += 2;
    return indent;
  }
  return null;
}

/**
 * Scans a workflow into a flat list of nodes, each carrying the ancestor key chain that
 * locates it. Block-scalar bodies (`key: |`) are skipped, not interpreted.
 *
 * @returns {Array<{key: string, value: string, comment: string|null, line: number,
 *                  indent: number, path: string[]}>}
 */
export function scan(text) {
  const nodes = [];
  const lines = text.split("\n");
  // Open blocks, outermost first. `indent` is the column this block's keys start at.
  const stack = [{ indent: 0, path: [], keys: new Set() }];
  let blockScalarIndent = null;

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const lineNo = i + 1;
    if (raw.trim() === "") continue;

    const leading = raw.length - raw.trimStart().length;
    if (raw.slice(0, leading).includes("\t")) {
      throw new MalformedWorkflow(lineNo, "tab in indentation (YAML forbids tabs)");
    }

    // Inside a block scalar, everything indented past its key is opaque body text.
    if (blockScalarIndent !== null) {
      if (leading > blockScalarIndent) continue;
      blockScalarIndent = null;
    }

    const [content, comment] = splitComment(raw.trim());
    if (content === "") continue; // comment-only line

    // A sequence item's inline key opens the item's mapping two columns in, which is
    // where the item's sibling keys align.
    const isItem = content.startsWith("- ");
    const indent = isItem ? leading + 2 : leading;
    const text_ = isItem ? content.slice(2).trim() : content;
    if (content === "-") continue; // an item whose mapping starts on the next line

    while (stack.length > 1 && indent < stack[stack.length - 1].indent) stack.pop();
    const top = stack[stack.length - 1];
    if (indent > top.indent) {
      throw new MalformedWorkflow(lineNo, `unexpected indentation (expected ${top.indent}, got ${indent})`);
    }
    // A new sequence item starts a fresh mapping: its keys are not duplicates of the
    // previous item's. Without this, two steps that both say `uses:` would collide.
    if (isItem) top.keys.clear();

    const m = KEY.exec(text_);
    if (!m) throw new MalformedWorkflow(lineNo, `not a key or sequence item: ${JSON.stringify(text_)}`);

    const key = m[1];
    const value = (m[2] ?? "").trim();
    if (top.keys.has(key)) throw new MalformedWorkflow(lineNo, `duplicate key '${key}' in the same block`);
    top.keys.add(key);

    const path = [...top.path, key];
    nodes.push({ key, value, comment, line: lineNo, indent, path });

    if (BLOCK_SCALAR.test(value)) {
      blockScalarIndent = indent;
    } else if (value === "") {
      // A key with no inline value opens a block. Adopt the indent its first child uses.
      const childIndent = nextContentIndent(lines, i + 1);
      if (childIndent !== null && childIndent > indent) {
        stack.push({ indent: childIndent, path, keys: new Set() });
      }
    }
  }
  return nodes;
}
