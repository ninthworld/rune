import { createHash } from "node:crypto";

/**
 * Reads `docs/roadmap.md` — the document the whole stewardship cycle is an audit *of*.
 *
 * Everything here is quotation, not interpretation. ADR 0017 requires the Auditor to be
 * handed the exact sentence a human wrote, because a paraphrased exit criterion is a
 * criterion the model got to soften before anyone checked it: `raw` is the verbatim
 * roadmap text, and `text` exists only so a human reading the bundle is not fighting
 * markdown line-wrapping.
 */

/** A milestone heading: `### M3 — A real card pool` (with an optional `*(coarse)*` tag). */
const HEADING = /^###\s+(M\d+)\s+—\s+(.+?)\s*$/;

/** `- [ ]` / `- [x]`, at the start of a line, is a criterion. Continuation lines are indented. */
const CRITERION = /^-\s+\[([ xX])\]\s/;

/**
 * Splits the document into milestone sections.
 *
 * `## How this drives work` (or any other `##`) ends the milestone list, so prose that
 * merely mentions a milestone cannot be mistaken for one.
 */
function sections(text) {
  const lines = text.split("\n");
  const found = [];
  let current = null;

  for (const line of lines) {
    const heading = HEADING.exec(line);
    if (heading) {
      current = { id: heading[1], name: `${heading[1]} — ${heading[2].replace(/\s*\*\(coarse\)\*\s*$/, "")}`, lines: [] };
      found.push(current);
      continue;
    }
    if (/^##\s/.test(line)) current = null;
    if (current) current.lines.push(line);
  }
  return found;
}

/**
 * The criteria of one milestone, verbatim.
 *
 * A coarse milestone (M6/M7) has no checklist at all and a mid-range one (M4/M5) states its
 * exit as a single `**Exit:**` sentence. Both are returned as one unchecked, prose criterion
 * rather than as an empty list: a milestone whose exit condition is a sentence still has an
 * exit condition, and an empty list would read to every later stage as "nothing to audit".
 */
function criteria(id, lines) {
  const start = lines.findIndex((l) => /^\*\*Exit criteria:?\*\*/.test(l));
  if (start === -1) {
    const prose = lines.find((l) => /^\*\*Exit:?\*\*/.test(l));
    if (!prose) return [];
    const block = collectProse(lines, lines.indexOf(prose));
    return [criterion(id, block, { kind: "prose" })];
  }

  const out = [];
  let block = null;
  for (const line of lines.slice(start + 1)) {
    // The criteria list ends at the next bolded lead-in (`**Features …**`) or the next heading.
    if (/^\*\*/.test(line) || /^#{1,6}\s/.test(line)) break;
    if (CRITERION.test(line)) {
      if (block) out.push(criterion(id, block));
      block = [line];
    } else if (block && line.trim() !== "") {
      block.push(line);
    } else if (block && line.trim() === "") {
      out.push(criterion(id, block));
      block = null;
    }
  }
  if (block) out.push(criterion(id, block));
  return out;
}

function collectProse(lines, from) {
  const block = [lines[from]];
  for (const line of lines.slice(from + 1)) {
    if (line.trim() === "" || /^\*\*/.test(line)) break;
    block.push(line);
  }
  return block;
}

/**
 * `*Partial: …*` notes a human already wrote against a criterion.
 *
 * ADR 0017 carries these forward verbatim rather than re-deriving them: someone looked at
 * this criterion once, decided it was half-done, and said exactly how. Re-deriving that from
 * scratch every cycle is how a known gap quietly becomes an unknown one.
 */
function partialNotes(raw) {
  return [...raw.matchAll(/\*(Partial:[\s\S]*?)\*/g)].map((m) => m[1].replace(/\s+/g, " ").trim());
}

function criterion(milestoneId, block, { kind = "checkbox" } = {}) {
  const raw = block.join("\n");
  const checked = kind === "checkbox" && /^-\s+\[[xX]\]/.test(block[0]);
  const text = raw
    .replace(CRITERION, "")
    .replace(/^\*\*Exit:?\*\*\s*/, "")
    .replace(/\*Partial:[\s\S]*?\*/g, "")
    .replace(/\s+/g, " ")
    .trim();

  return {
    // Content-addressed, not positional: re-collecting the same roadmap yields the same id,
    // while *editing* a criterion yields a new one. That is the honest behavior — a reworded
    // exit condition is a different claim, and silently carrying an old audit verdict across
    // the rewording is exactly the kind of drift this cycle exists to catch.
    criterion_id: `${milestoneId}-${createHash("sha256").update(text).digest("hex").slice(0, 8)}`,
    kind,
    checked,
    text,
    raw,
    notes: partialNotes(raw),
    issue_refs: issueRefs(raw),
    cr_citations: crCitations(raw),
    adr_refs: adrRefs(raw),
    // Backticked tokens are how the roadmap names the concrete artifacts a criterion is
    // about (`GameView.result`, `data/oracle.json`, `docs/protocol.md`). They are what the
    // protocol/stub sweeps below are scoped by, so the sweep follows the criterion instead
    // of a hand-maintained path list that would rot.
    terms: [...new Set([...raw.matchAll(/`([^`]+)`/g)].map((m) => m[1]))],
  };
}

export function issueRefs(text) {
  return [...new Set([...String(text).matchAll(/#(\d+)/g)].map((m) => Number(m[1])))].sort((a, b) => a - b);
}

export function adrRefs(text) {
  return [...new Set([...String(text).matchAll(/ADR\s+(\d{4})/g)].map((m) => m[1]))].sort();
}

/**
 * The CR rules a criterion names, as written: `CR 502/504/514`, `CR 508–510, 704.5g`.
 *
 * `sections` is the top-level rule number (`704.5g` → `704`), which is what
 * `docs/rules-coverage.md` rows are matched on. Deliberately coarse: pulling in a
 * neighbouring rule from the same section shows the Auditor one row it did not need, while
 * missing one hides evidence — and only one of those two failures is recoverable by a
 * reader.
 */
export function crCitations(text) {
  const tokens = new Set();
  const sections = new Set();

  for (const match of String(text).matchAll(/CR\s+((?:\d[\d.]*[a-z]?)(?:\s*[/,–-]\s*(?:\d[\d.]*[a-z]?))*)/g)) {
    const list = match[1];
    for (const range of list.matchAll(/(\d+)\s*–\s*(\d+)/g)) {
      const [from, to] = [Number(range[1]), Number(range[2])];
      for (let n = from; n <= to && n - from < 50; n++) sections.add(n);
    }
    for (const token of list.split(/[/,–-]/)) {
      const trimmed = token.trim();
      if (!/^\d/.test(trimmed)) continue;
      tokens.add(trimmed);
      sections.add(Number(trimmed.split(".")[0]));
    }
  }
  return { tokens: [...tokens].sort(), sections: [...sections].sort((a, b) => a - b) };
}

/**
 * The "Where we are" prose, per bullet.
 *
 * Carried into the bundle as `documented_gaps` alongside the `*Partial:*` notes: this
 * section is where a maintainer writes down what is *known* to be missing, and an audit that
 * rediscovers it from scratch will sooner or later fail to rediscover one of them.
 */
export function whereWeAre(text) {
  const start = text.search(/^##\s+Where we are/m);
  if (start === -1) return [];
  const body = text.slice(start).split("\n").slice(1);
  const end = body.findIndex((l) => /^##\s/.test(l));
  const lines = end === -1 ? body : body.slice(0, end);

  const bullets = [];
  let block = null;
  for (const line of lines) {
    if (/^-\s/.test(line)) {
      if (block) bullets.push(block.join(" ").replace(/\s+/g, " ").trim());
      block = [line.replace(/^-\s+/, "")];
    } else if (block && line.trim() !== "") {
      block.push(line.trim());
    } else if (block) {
      bullets.push(block.join(" ").replace(/\s+/g, " ").trim());
      block = null;
    }
  }
  if (block) bullets.push(block.join(" ").replace(/\s+/g, " ").trim());
  return bullets;
}

export function lastReconciled(text) {
  return /^>\s*Last reconciled[^:]*:\s*(\S+)/m.exec(text)?.[1]?.replace(/[^\d-]/g, "") || null;
}

export function parseRoadmap(text) {
  return {
    last_reconciled: lastReconciled(text),
    where_we_are: whereWeAre(text),
    milestones: sections(text).map((section) => {
      const raw = section.lines.join("\n");
      return {
        id: section.id,
        name: section.name,
        exit_criteria: criteria(section.id, section.lines),
        // Every `#N` anywhere in the milestone's section — its criteria *and* its feature
        // table. GitHub's own milestone field is only assigned on the recent waves, so the
        // roadmap's own links are the durable mapping from milestone to issues; the bundle
        // records both and marks which source each issue came from, so the drift between
        // them is visible rather than resolved.
        issue_refs: issueRefs(raw),
      };
    }),
  };
}

/**
 * Finds a milestone by id (`M3`) or by name, case-insensitively.
 *
 * A typo'd milestone must fail loudly here rather than collect a bundle full of nothing:
 * "zero issues, zero criteria" is also what a genuinely finished milestone looks like.
 */
export function findMilestone(roadmap, wanted) {
  const needle = String(wanted).trim().toLowerCase();
  return (
    roadmap.milestones.find((m) => m.id.toLowerCase() === needle || m.name.toLowerCase() === needle) ||
    roadmap.milestones.find((m) => m.name.toLowerCase().startsWith(needle)) ||
    null
  );
}
