/**
 * Provider adapters.
 *
 * The whole contract: given the brief, produce an argv that edits the working copy and
 * exits. No adapter gets to touch GitHub, and none reports its own outcome — the runner
 * observes that for itself. A provider that needs more than this is one RUNE does not
 * support, which is the point: the adapter is the only thing that changes between providers.
 */
const ADAPTERS = {
  claude: {
    command: "claude",
    argv: (brief) => ["claude", "-p", brief, "--permission-mode", "acceptEdits"],
  },
  codex: {
    command: "codex",
    argv: (brief) => ["codex", "exec", brief],
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
