# The provider sandbox for `scripts/agent-task` (ADR 0016).
#
# This image is what makes real isolation available: the provider runs in here as an
# unprivileged user, with no access to the host's home — so it cannot read
# ~/.config/rune/rune-agent.pem and cannot mint a token for itself.
#
# It has to carry the *whole* toolchain, not just the model CLI, because verification runs
# inside the same boundary: `make verify` executes provider-controlled code by construction
# (a doctored Makefile or build.rs runs right here), which is exactly why it must not run in
# the runner's own context.
#
#   docker build -t rune/provider tools/agent-task
#   export RUNE_PROVIDER_IMAGE=rune/provider
#   scripts/agent-task start 186 --provider claude
#
# The container gets no credentials except the provider's own model token — the runner builds
# its environment from an allowlist — and its `origin` is a local path, so it has no remote to
# push to even if it tried.

FROM rust:1-bookworm

# Node 20: the client toolchain, and what the provider CLIs are distributed on.
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
 && apt-get install -y --no-install-recommends nodejs make git ca-certificates \
 && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy

# The `cargo-deny` gate. Installed at build time so a run never pays for it.
RUN cargo install --locked cargo-deny && rm -rf "$CARGO_HOME"/registry

# The provider CLIs. Neither is authenticated here: the runner passes a token
# (CLAUDE_CODE_OAUTH_TOKEN / OPENAI_API_KEY) into the container's environment at run time.
# Baking a credential into an image is how credentials end up in a registry.
RUN npm install -g @anthropic-ai/claude-code @openai/codex && npm cache clean --force

# System libraries Chromium needs for the E2E gate. The browser itself is *not* baked in — the
# runner points PLAYWRIGHT_BROWSERS_PATH at a cache shared across runs, so it is downloaded once
# on the host rather than once per image build.
RUN npx --yes playwright@latest install-deps chromium \
 && rm -rf /var/lib/apt/lists/* /root/.npm

# The runner passes `--user $(id -u):$(id -g)`, so files written into the mounted workspace belong
# to the invoking human rather than to root. That UID has no /etc/passwd entry, which is fine here
# only because the runner also sets HOME explicitly — nothing has to call getpwuid to find it.
#
# CARGO_HOME, NPM_CONFIG_CACHE, and PLAYWRIGHT_BROWSERS_PATH are all set by the runner to point
# into the shared cache mount, so a run does not re-download the crate registry every time.
ENV CARGO_TERM_COLOR=never \
    NPM_CONFIG_UPDATE_NOTIFIER=false \
    NPM_CONFIG_FUND=false
