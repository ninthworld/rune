import assert from "node:assert/strict";
import { test } from "node:test";

import { redact, redactor } from "./redact.js";

test("installation tokens and PATs are scrubbed", () => {
  const text = [
    "token: ghs_abcdefghijklmnopqrstuvwxyz0123456789",
    "pat:   github_pat_11ABCDEFG0123456789_abcdefghijklmnop",
    "old:   ghp_abcdefghijklmnopqrstuvwxyz0123456789",
  ].join("\n");

  const out = redact(text);
  assert.doesNotMatch(out, /ghs_|ghp_|github_pat_/);
  assert.equal(out.match(/\[redacted\]/g).length, 3);
});

test("a private key is scrubbed whole, not line by line", () => {
  const key = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow...\nlines\n-----END RSA PRIVATE KEY-----";
  const out = redact(`before\n${key}\nafter`);

  assert.doesNotMatch(out, /MIIEow/);
  assert.match(out, /before\n\[redacted\]\nafter/);
});

test("secrets the caller knows about are scrubbed even if they match no pattern", () => {
  // The token minted for *this* run: a pattern cannot know it, but the runner can.
  assert.match(redact("value=s3cret-opaque-token", ["s3cret-opaque-token"]), /value=\[redacted\]/);
});

test("a short 'secret' is ignored, so a stray value cannot redact everything", () => {
  assert.equal(redact("a b c", ["a"]), "a b c");
});

test("the streaming redactor catches a secret split across chunk boundaries", () => {
  // The reason it is line-buffered: scrubbing raw chunks would let a token through whenever
  // it happened to straddle a read.
  const written = [];
  const sink = redactor((line) => written.push(line));

  sink.push("leaked: ghs_abcdefghij");
  sink.push("klmnopqrstuvwxyz0123456789\n");
  sink.flush();

  assert.doesNotMatch(written.join(""), /ghs_/);
  assert.match(written.join(""), /leaked: \[redacted\]/);
});

test("the redactor flushes a trailing line that never got a newline", () => {
  const written = [];
  const sink = redactor((line) => written.push(line));
  sink.push("no newline here");
  assert.deepEqual(written, []);
  sink.flush();
  assert.deepEqual(written, ["no newline here\n"]);
});
