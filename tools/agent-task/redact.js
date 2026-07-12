/**
 * Credential shapes that must never reach a log file, let alone a telemetry record.
 *
 * `bot-token.sh` mints a `ghs_…` installation token every run, and a provider that echoes
 * its environment or `cat`s a file would otherwise write one straight into `provider.log`.
 */
const PATTERNS = [
  /\bgh[posru]_[A-Za-z0-9]{20,}/g,
  /\bgithub_pat_[A-Za-z0-9_]{20,}/g,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g,
];

/**
 * Scrubs known credential shapes, plus any literal secrets the caller knows about.
 *
 * `extra` covers the values a pattern cannot know — the token minted for *this* run — so a
 * provider that prints it still cannot leak it.
 */
export function redact(text, extra = []) {
  let out = String(text);
  for (const pattern of PATTERNS) out = out.replace(pattern, "[redacted]");
  for (const secret of extra) {
    if (typeof secret === "string" && secret.length >= 8) out = out.split(secret).join("[redacted]");
  }
  return out;
}

/**
 * Line-buffered redaction for a stream.
 *
 * Scrubbing raw chunks would miss a secret split across a chunk boundary, so text is held
 * until a newline arrives; `flush()` drains the tail.
 */
export function redactor(write, extra = []) {
  let buffer = "";
  return {
    push(chunk) {
      buffer += chunk;
      const lines = buffer.split("\n");
      buffer = lines.pop();
      for (const line of lines) write(`${redact(line, extra)}\n`);
    },
    flush() {
      if (buffer) write(`${redact(buffer, extra)}\n`);
      buffer = "";
    },
  };
}
