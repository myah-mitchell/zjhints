TARGET  := wasm32-wasip1
WASM    := target/$(TARGET)/release/zjstatus-hints.wasm
PLUGIN  := $(HOME)/.local/share/zellij/plugins/zjstatus-hints.wasm
REPO    := myah-mitchell/zjstatus-hints

.PHONY: build install dev test check nightly latest zellij ea

# Build the release wasm.
build:
	cargo build --release --target $(TARGET)

# Copy the built wasm into the local Zellij plugin dir.
# Writes through the symlink, so the dotfiles setup stays intact.
install: build
	install -m 644 $(WASM) $(PLUGIN)
	@echo "Installed -> $(PLUGIN)"

# Build + install in one step. Then start a fresh Zellij session to load it.
dev: install

# Tests build for the host, not wasm, and need OpenSSL headers
# (libssl-dev). See docs/AUTOMATION.md if this fails to link.
test:
	cargo test --all-features

# What CI runs, so a red build can be reproduced before pushing.
check: test
	cargo fmt --all --check
	cargo clippy --all-features --target $(TARGET) -- -D warnings
	cargo build --release --target $(TARGET)

# Install the newest nightly from GitHub.
#
# Zellij caches remote plugins by URL, so pointing the config at a rolling
# release URL will keep serving whatever it downloaded first. Fetching to the
# plugin path instead means the next session always picks up the new build.
nightly:
	@echo "Fetching nightly from $(REPO)…"
	@curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/nightly/zjstatus-hints.wasm"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed nightly -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# Same, but the current stable release.
latest:
	@echo "Fetching latest release from $(REPO)…"
	@curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/latest/download/zjstatus-hints.wasm"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed latest -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# `latest` tracks the newest zjstatus-hints, which is not always the newest
# release built for the Zellij you actually run — zellij-tile only moves past
# a minor deliberately (see docs/AUTOMATION.md). `zellij-<line>` is a tag that
# always points at the newest release built for that Zellij minor, so this
# fetches the right one regardless of what `latest` currently is.
#
# Usage: make zellij VERSION=0.44
zellij:
	@if [ -z "$(VERSION)" ]; then \
		echo "Usage: make zellij VERSION=0.44   (your Zellij's major.minor)" >&2; \
		exit 1; \
	fi
	@echo "Fetching the newest release for Zellij $(VERSION).x from $(REPO)…"
	@curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/zellij-$(VERSION)/zjstatus-hints.wasm"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# Install an on-demand EA/beta build (see docs/AUTOMATION.md). These come
# from the beta.yml workflow, not a tagged release, and the channel can be
# replaced at any time — treat this as trying out in-progress work, not as
# something to depend on.
#
# Usage: make ea LABEL=nested-sessions
ea:
	@if [ -z "$(LABEL)" ]; then \
		echo "Usage: make ea LABEL=nested-sessions   (whatever channel was published)" >&2; \
		exit 1; \
	fi
	@echo "Fetching the EA build '$(LABEL)' from $(REPO)…"
	@curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/ea-$(LABEL)/zjstatus-hints.wasm"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."
