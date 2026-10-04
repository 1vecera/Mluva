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

linux-text-target-test: linux-python-setup
	bash linux/tests/run_native_text_target_smoke.sh

linux-conversation-test: linux-python-setup
	MLUVA_SMOKE=conversation bash linux/tests/run_native_text_target_smoke.sh tmp/conversation-smoke

.PHONY: linux-compact-workspace-test
linux-compact-workspace-test:
	bash linux/tests/run_application_smoke.sh compact

.PHONY: linux-conversation-management-test
linux-conversation-management-test: linux-python-setup
	@mkdir -p tmp/chat-management
	@set -e; for spec in minimum:420:520 narrow:480:640 wide:1060:780; do \
		scenario=$${spec%%:*}; dimensions=$${spec#*:}; \
		OFFSCREEN_SCREEN_SPEC=1600x1000x24 OFFSCREEN_ENABLE_ATSPI=1 \
		bash dev/run-isolated.sh "tmp/chat-management/$$scenario" -- env \
			GDK_SCALE=1 GDK_DPI_SCALE=1 PYTHONPATH=linux:linux/tests ADW_DISABLE_PORTAL=1 GTK_A11Y=none GSK_RENDERER=cairo \
			MLUVA_UI_WIDTH="$${dimensions%:*}" MLUVA_UI_HEIGHT="$${dimensions#*:}" \
			uv run --project linux --locked python linux/tests/conversation_management_smoke.py \
			> "tmp/chat-management/$$scenario.log" 2>&1; \
	done

linux-live-rewrite-test: linux-live-workspace-test
	bash linux/tests/run_application_smoke.sh live-controllers

.PHONY: linux-provider-settings-test
linux-provider-settings-test:
	bash linux/tests/run_application_smoke.sh providers

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
linux-prompt-test: linux-python-setup
	@mkdir -p tmp/prompt-editor
	@set -e; for spec in minimum:420:520:dark narrow:480:640:light wide:1060:780:dark bad-config:480:640:dark; do \
		scenario=$${spec%%:*}; rest=$${spec#*:}; width=$${rest%%:*}; rest=$${rest#*:}; height=$${rest%%:*}; theme=$${rest#*:}; \
		OFFSCREEN_SCREEN_SPEC=1600x1000x24 OFFSCREEN_ENABLE_ATSPI=1 \
		bash dev/run-isolated.sh "tmp/prompt-editor/$$scenario" -- env \
			GDK_SCALE=1 GDK_DPI_SCALE=1 PYTHONPATH=linux:linux/tests ADW_DISABLE_PORTAL=1 GTK_A11Y=none GSK_RENDERER=cairo \
			MLUVA_PROMPT_SCENARIO="$$scenario" MLUVA_UI_WIDTH="$$width" MLUVA_UI_HEIGHT="$$height" MLUVA_UI_THEME="$$theme" \
			bash -c 'uv run --project linux --locked python linux/tests/prompt_editor_smoke.py && uv run --project linux --locked python linux/tests/prompt_editor_smoke.py --restart-check' \
			> "tmp/prompt-editor/$$scenario.log" 2>&1; \
	done
