/**
 * Provider adapters.
 *
 * The whole contract: given the brief, produce an argv that edits the working copy and
 * exits. No adapter gets to touch GitHub, and none reports its own outcome — the runner
 * observes that for itself. A provider that needs more than this is one RUNE does not
 * support, which is the point: the adapter is the only thing that changes between providers.
 */
/**
 * How much rope the provider gets, derived from how well it is contained.
 *
 * A provider has to run the build and the tests, and `acceptEdits` auto-approves file edits but
 * still asks before arbitrary `Bash` — in print mode there is nobody to ask, so those calls are
 * simply denied and the run limps to a failure. `bypassPermissions` is the mode that actually
 * works unattended, and it is only defensible *because* the run is contained: another UID or a
 * container, no credentials in the environment, no network remote (ADR 0016).
 *
 * Under `--unsafe-same-uid` there is no containment, so the provider is running as the
 * maintainer, in the maintainer's session. There it gets `acceptEdits` and may fail to run a
 * command — which is the correct trade, because the alternative is unattended `bypassPermissions`
 * as a user who can read the app's private key.
 */
export function permissionMode(isolation) {
  return isolation?.mode === "same-uid" ? "acceptEdits" : "bypassPermissions";
}

const ADAPTERS = {
  claude: {
    command: "claude",
    // Deliberately NOT `--bare`: bare mode skips auto-discovery of CLAUDE.md, which is how the
    // root and nested `AGENTS.md` reach the model at all. ADR 0016 requires that behaviour be
    // preserved, and the brief points at those files rather than inlining them.
    argv: (brief, { isolation } = {}) => ["claude", "-p", brief, "--permission-mode", permissionMode(isolation)],
  },
  codex: {
    command: "codex",
    argv: (brief) => ["codex", "exec", "--full-auto", brief],
  },
  local: {
    // No model, harness, or vendor prescribed (ADR 0016). The command reads the brief from
    // $RUNE_BRIEF, which is in the environment the runner builds.
    command: null,
    argv: () => {
      const cmd = process.env.RUNE_LOCAL_CMD;
      if (!cmd) {
        throw new Error(
          "the `local` provider needs RUNE_LOCAL_CMD, e.g.\n" +
            '  RUNE_LOCAL_CMD=\'opencode run --model my/model "$(cat "$RUNE_BRIEF")"\'',
        );
      }
      return ["bash", "-c", cmd];
    },
  },
};

export function adapterFor(provider) {
  const adapter = ADAPTERS[provider];
  if (!adapter) throw new Error(`unknown provider "${provider}"`);
  return adapter;
}
