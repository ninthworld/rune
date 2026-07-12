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
# Built from the REPOSITORY ROOT, because it needs the client's lockfile to pin the browser:
#
#   docker build -t rune/provider -f tools/agent-task/Dockerfile .
#   export RUNE_PROVIDER_IMAGE=rune/provider
#   scripts/agent-task start 186 --provider claude
#
# Rebuild it when `clients/web/package-lock.json` changes, so the baked browser keeps matching
# the Playwright the repository pins.
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

# actionlint + shellcheck, for the workflow gate `make verify` now runs (#199). actionlint
# shells out to shellcheck for `run:` bodies when it is present, and the CI runners ship it —
# so the image needs it too, or the sandbox would run a *weaker* gate than CI and a run could
# pass here and fail there. Same pinned, checksum-verified script the `cargo-deny` job uses,
# so the version lives in exactly one place.
COPY scripts/install-actionlint.sh /tmp/install-actionlint.sh
RUN apt-get update \
 && apt-get install -y --no-install-recommends shellcheck \
 && rm -rf /var/lib/apt/lists/* \
 && sh /tmp/install-actionlint.sh /usr/local/bin \
 && rm -f /tmp/install-actionlint.sh

# The provider CLIs. Neither is authenticated here: the runner passes a token
# (CLAUDE_CODE_OAUTH_TOKEN / OPENAI_API_KEY) into the container's environment at run time.
# Baking a credential into an image is how credentials end up in a registry.
RUN npm install -g @anthropic-ai/claude-code @openai/codex && npm cache clean --force

# The E2E browser (ADR 0011), baked in — it cannot be installed at run time, because
# `playwright install --with-deps` shells out to apt-get as root and the sandbox is
# unprivileged by design.
#
# Installed via the repository's own lockfile rather than `playwright@latest`, so the browser
# revision matches the Playwright the client pins. A mismatched revision fails at run time with
# "Executable doesn't exist", which is a miserable thing to debug from inside a container.
ENV PLAYWRIGHT_BROWSERS_PATH=/ms-playwright
COPY clients/web/package.json clients/web/package-lock.json /tmp/client/
RUN cd /tmp/client \
 && npm ci --ignore-scripts \
 && npx playwright install --with-deps chromium \
 && chmod -R a+rX /ms-playwright \
 && rm -rf /tmp/client /root/.npm /var/lib/apt/lists/*

# The runner passes `--user $(id -u):$(id -g)`, so files written into the mounted workspace belong
# to the invoking human rather than to root. That UID has no /etc/passwd entry, which is fine here
# only because the runner also sets HOME explicitly — nothing has to call getpwuid to find it.
#
# CARGO_HOME and NPM_CONFIG_CACHE are set by the runner to point into a cache shared across runs,
# so a run does not re-download the crate registry every time. PLAYWRIGHT_BROWSERS_PATH is *not*
# overridden — the browser lives here, in the image.
ENV CARGO_TERM_COLOR=never \
    NPM_CONFIG_UPDATE_NOTIFIER=false \
    NPM_CONFIG_FUND=false
