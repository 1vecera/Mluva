.DEFAULT_GOAL := linux-setup
export CARGO_TARGET_DIR ?= $(CURDIR)/tmp/native-build
.PHONY: test run install linux-setup linux-test linux-feature-maturity linux-feature-maturity-check linux-shortcut-test linux-overlay-test linux-text-target-test linux-conversation-test linux-live-rewrite-test linux-run linux-install linux-uninstall linux-input-helper-install linux-input-helper-status linux-input-helper-remove linux-recording-overlay-install linux-recording-overlay-status linux-recording-overlay-remove

test: linux-test

run: linux-run

install:
	bash install.sh

linux-setup:
	bash linux/native-source-command.sh build

linux-test: linux-setup
	cargo test --locked --workspace
	cargo clippy --locked --workspace --all-targets -- -D warnings
	cargo clippy --locked -p mluva-install --no-default-features --all-targets -- -D warnings
	cargo fmt --all -- --check
	$(MAKE) linux-feature-maturity-check

# Temporary gate for the remaining Python implementation during the full port.
.PHONY: linux-python-setup linux-python-test
linux-python-setup:
	@cd linux && if ! test -x .venv/bin/python \
		|| ! uv run --no-sync python -c 'import gi; gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1"); gi.require_version("Atspi", "2.0"); gi.require_version("DBus", "1.0"); gi.require_version("cairo", "1.0"); from gi.repository import Adw, Atspi, DBus, Gtk, cairo' >/dev/null 2>&1; then \
		uv venv --clear --system-site-packages --python /usr/bin/python3; \
	fi
	cd linux && uv sync --locked

linux-python-test: linux-python-setup
	$(MAKE) linux-feature-maturity-check
	cd linux && uv run --locked pytest -q
	cd linux && uv run --locked ruff check .
	cd linux && uv run --locked ruff format --check .

.PHONY: linux-codex-isolation-test
linux-codex-isolation-test: linux-python-setup
	cd linux && PYTHONPATH=. uv run --locked python tests/codex_isolation_smoke.py ../tmp/codex-isolation

# Fast deterministic config/text/storage/Live feedback; linux-test is the native gate.
.PHONY: linux-test-fast linux-application-test linux-command-test linux-continuation-test linux-live-workspace-test linux-fluid-workspace-test linux-live-stability-test
linux-test-fast:
	cargo test --locked -p mluva-core --test reference_contracts --test persistence_contracts \
		--test prompt_and_draft_contracts --test live_policy

linux-command-test linux-continuation-test: linux-application-test

linux-application-test:
	bash linux/tests/run_application_smoke.sh

linux-fluid-workspace-test linux-live-stability-test: linux-live-workspace-test

linux-live-workspace-test: linux-application-test
	bash linux/tests/run_application_smoke.sh live-components

.PHONY: linux-omarchy-test
linux-omarchy-test:
	bash linux/tests/run_omarchy_widget_smoke.sh

linux-feature-maturity:
	cargo run --locked -q -p mluva-core --bin mluva-feature-maturity -- --write

linux-feature-maturity-check:
	cargo run --locked -q -p mluva-core --bin mluva-feature-maturity -- --check

linux-shortcut-test:
	bash linux/tests/run_global_shortcut_portal_smoke.sh

linux-overlay-test:
	bash linux/tests/run_recording_overlay_smoke.sh

linux-text-target-test:
	bash linux/tests/run_application_smoke.sh text-targets

linux-conversation-test: linux-python-setup
	bash linux/tests/run_conversation_smoke.sh

.PHONY: linux-compact-workspace-test
linux-compact-workspace-test:
	bash linux/tests/run_application_smoke.sh compact

.PHONY: linux-conversation-management-test
linux-conversation-management-test:
	bash linux/tests/run_application_smoke.sh management

linux-live-rewrite-test: linux-live-workspace-test
	bash linux/tests/run_application_smoke.sh live-controllers

.PHONY: linux-provider-settings-test
linux-provider-settings-test:
	bash linux/tests/run_application_smoke.sh providers

.PHONY: linux-onboarding-test
linux-onboarding-test:
	bash linux/tests/run_application_smoke.sh onboarding

linux-run:
	bash linux/native-source-command.sh run

linux-install:
	bash linux/install.sh

linux-uninstall:
	bash linux/uninstall.sh

linux-input-helper-install:
	bash linux/configure-input-helper.sh install

linux-input-helper-status:
	bash linux/configure-input-helper.sh status

linux-input-helper-remove:
	bash linux/configure-input-helper.sh remove

linux-recording-overlay-install:
	bash linux/configure-recording-overlay.sh install

linux-recording-overlay-status:
	bash linux/configure-recording-overlay.sh status

linux-recording-overlay-remove:
	bash linux/configure-recording-overlay.sh remove

.PHONY: linux-prompt-test
linux-prompt-test:
	bash linux/tests/run_application_smoke.sh prompts
	bash linux/tests/run_application_smoke.sh prompt-editor

.PHONY: linux-screenshot-test
linux-screenshot-test:
	bash linux/tests/run_application_smoke.sh images
	bash linux/tests/run_application_smoke.sh screenshots
