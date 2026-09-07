TARGET  := wasm32-wasip1
ASSET   := zjhints.wasm
WASM    := target/$(TARGET)/release/$(ASSET)
PLUGIN  := $(HOME)/.local/share/zellij/plugins/$(ASSET)
REPO    := myah-mitchell/zjhints

# The asset name every release before 0.5.0 was published under. The fetch
# targets below try the current name and fall back to this one, so the older
# `zellij-<line>` tags and any EA channel carrying it stay installable. Drop
# it once nothing worth fetching carries the old name.
LEGACY_ASSET := zjstatus-hints.wasm

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
# (libssl-dev). See docs/automation.md if this fails to link.
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
		"https://github.com/$(REPO)/releases/download/nightly/$(ASSET)" \
		|| curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/nightly/$(LEGACY_ASSET)"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed nightly -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# Same, but the current stable release.
latest:
	@echo "Fetching latest release from $(REPO)…"
	@curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/latest/download/$(ASSET)" \
		|| curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/latest/download/$(LEGACY_ASSET)"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed latest -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# `latest` tracks the newest zjhints, which is not always the newest
# release built for the Zellij you actually run — zellij-tile only moves past
# a minor deliberately (see docs/automation.md). `zellij-<line>` is a tag that
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
		"https://github.com/$(REPO)/releases/download/zellij-$(VERSION)/$(ASSET)" \
		|| curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/zellij-$(VERSION)/$(LEGACY_ASSET)"
	@mv "$(PLUGIN).tmp" "$(PLUGIN)"
	@echo "Installed -> $(PLUGIN)"
	@echo "Start a new Zellij session to load it."

# Install an on-demand EA/beta build (see docs/automation.md). These come
# from the beta.yml workflow, not a tagged release, and the channel can be
# replaced at any time: treat this as trying out in-progress work, not as
# something to depend on.
#
# Usage: make ea LABEL=nested-sessions
#
# beta.yml lowercases the label and reduces it to [a-z0-9-] before building
# the ea-<label> tag, so LABEL is sanitized the same way here. Otherwise a
# label typed back with different casing or punctuation than what was
# published (e.g. LABEL=Nested-Sessions for a tag actually named
# ea-nested-sessions) would 404 with no hint why.
ea:
	@if [ -z "$(LABEL)" ]; then \
		echo "Usage: make ea LABEL=nested-sessions   (whatever channel was published)" >&2; \
		exit 1; \
	fi
	@label="$$(printf '%s\n' '$(LABEL)' | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9-' '-' | sed -e 's/^-*//' -e 's/-*$$//')"; \
	if [ -z "$$label" ]; then \
		echo "LABEL '$(LABEL)' has no characters left after sanitizing to [a-z0-9-]" >&2; \
		exit 1; \
	fi; \
	echo "Fetching the EA build '$$label' from $(REPO)…"; \
	curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/ea-$$label/$(ASSET)" \
		|| curl -fsSL -o "$(PLUGIN).tmp" \
		"https://github.com/$(REPO)/releases/download/ea-$$label/$(LEGACY_ASSET)"; \
	mv "$(PLUGIN).tmp" "$(PLUGIN)"; \
	echo "Installed -> $(PLUGIN)"; \
	echo "Start a new Zellij session to load it."
