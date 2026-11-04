TARGET  := wasm32-wasip1
ASSET   := zjhints.wasm
WASM    := target/$(TARGET)/release/$(ASSET)
PLUGINS := $(HOME)/.local/share/zellij/plugins
PLUGIN  := $(PLUGINS)/$(ASSET)
REPO    := myah-mitchell/zjhints

# The other plugins a release may carry next to zjhints.wasm. A release built
# from main has none yet; one built from the zjkit workspace with zjtabs or
# zjtaskbar in it has that plugin's .wasm. The download targets install each
# one a release has.
EXTRA_ASSETS := zjtabs.wasm zjtaskbar.wasm

# The asset name every release before 0.5.0 was published under. `latest` and
# `zellij` try the current name and fall back to this one, so the older
# `zellij-<line>` tags stay installable. Drop
# it once nothing worth fetching carries the old name.
LEGACY_ASSET := zjstatus-hints.wasm

.PHONY: build install dev test images check nightly latest zellij ea

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

# Redraw the SVGs in docs/img from the code that draws the bar and the
# settings screen, and remove the ones it no longer draws. SVGs it did not
# draw are left alone. Plain `make test` compares against them and fails when
# they are stale, so run this after a change that alters how either looks.
# builds.yml runs the same tests, so a stale image
# blocks a release as well as a pull request.
images:
	ZJHINTS_WRITE_IMAGES=1 cargo test --all-features images

# What CI runs, so a red build can be reproduced before pushing.
check: test
	cargo fmt --all --check
	cargo clippy --all-features --target $(TARGET) -- -D warnings
	cargo build --release --target $(TARGET)

# Prints the newest release tag on GitHub matching the extended regex $(1),
# or nothing when none does. Nightly and EA tags end in the build date, plus a
# .2, .3 for a second build that day, so version order is build order.
newest_release = curl -fsSL "https://api.github.com/repos/$(REPO)/releases?per_page=100" \
	| grep -Eo "\"tag_name\": *\"$(1)\"" | sed -E 's/.*"([^"]+)"$$/\1/' | sort -V | tail -n1

# Installs the plugins of the release whose download URL is $(1), in a recipe
# running under `set -e`. zjhints.wasm is required, and $(2), when given, is a
# second name to try for it.
#
# Each of EXTRA_ASSETS is fetched as well. A release made before that plugin
# existed answers 404 for it, which is not an error: the plugin is skipped,
# and a copy already installed is left as it is. Any other failure stops the
# install. Everything is downloaded before anything is moved into place, so a
# failed install never leaves plugins from two releases side by side.
fetch_plugins = \
	trap 'rm -f $(foreach a,$(ASSET) $(EXTRA_ASSETS),"$(PLUGINS)/$(a).tmp")' EXIT; \
	curl -fsSL -o "$(PLUGIN).tmp" "$(1)/$(ASSET)" \
		$(if $(2),|| curl -fsSL -o "$(PLUGIN).tmp" "$(1)/$(2)"); \
	fetched="$(ASSET)"; \
	for asset in $(EXTRA_ASSETS); do \
		status="$$(curl -fsL -o "$(PLUGINS)/$$asset.tmp" -w '%{http_code}' "$(1)/$$asset")" || true; \
		case "$$status" in \
			200) fetched="$$fetched $$asset" ;; \
			404) echo "This release has no $$asset, so it was skipped." ;; \
			*) echo "Fetching $$asset failed (HTTP status '$$status')" >&2; exit 1 ;; \
		esac; \
	done; \
	for asset in $$fetched; do \
		mv "$(PLUGINS)/$$asset.tmp" "$(PLUGINS)/$$asset"; \
		echo "Installed $$asset -> $(PLUGINS)/$$asset"; \
	done

# Install the newest nightly from GitHub.
#
# Zellij caches remote plugins by URL, and every nightly has a URL of its own,
# so a config pointing at one keeps loading that build. Fetching to the plugin
# path instead means the next session always picks up the new build.
nightly:
	@set -e; \
	tag="$$($(call newest_release,nightly-[0-9]{6}(\.[0-9]+)?))"; \
	if [ -z "$$tag" ]; then \
		echo "No nightly release found on $(REPO)" >&2; \
		exit 1; \
	fi; \
	echo "Fetching $$tag from $(REPO)…"; \
	$(call fetch_plugins,https://github.com/$(REPO)/releases/download/$$tag); \
	echo "Start a new Zellij session to load $$tag."

# Same, but the current stable release.
latest:
	@set -e; \
	echo "Fetching latest release from $(REPO)…"; \
	$(call fetch_plugins,https://github.com/$(REPO)/releases/latest/download,$(LEGACY_ASSET)); \
	echo "Start a new Zellij session to load it."

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
	@set -e; \
	echo "Fetching the newest release for Zellij $(VERSION).x from $(REPO)…"; \
	$(call fetch_plugins,https://github.com/$(REPO)/releases/download/zellij-$(VERSION),$(LEGACY_ASSET)); \
	echo "Start a new Zellij session to load it."

# Install the newest build of an EA channel: a [build.ea-<label>] table in
# builds.toml (see docs/automation.md). The channel can be rebuilt or removed
# at any time: treat this as trying out in-progress work, not as something to
# depend on.
#
# Usage: make ea LABEL=nested-sessions
#
# LABEL is lowercased and reduced to [a-z0-9-], the characters a build name
# may use, so a label typed with different casing or punctuation still finds
# its channel.
ea:
	@if [ -z "$(LABEL)" ]; then \
		echo "Usage: make ea LABEL=nested-sessions   (whatever channel was published)" >&2; \
		exit 1; \
	fi
	@set -e; \
	label="$$(printf '%s\n' '$(LABEL)' | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9-' '-' | sed -e 's/^-*//' -e 's/-*$$//')"; \
	if [ -z "$$label" ]; then \
		echo "LABEL '$(LABEL)' has no characters left after sanitizing to [a-z0-9-]" >&2; \
		exit 1; \
	fi; \
	tag="$$($(call newest_release,ea-$$label-[0-9]{6}(\.[0-9]+)?))"; \
	if [ -z "$$tag" ]; then \
		echo "No EA build '$$label' found on $(REPO)" >&2; \
		exit 1; \
	fi; \
	echo "Fetching $$tag from $(REPO)…"; \
	$(call fetch_plugins,https://github.com/$(REPO)/releases/download/$$tag); \
	echo "Start a new Zellij session to load $$tag."
