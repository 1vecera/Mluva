.DEFAULT_GOAL := linux-setup
export CARGO_TARGET_DIR ?= $(CURDIR)/tmp/native-build
.PHONY: test run install linux-setup linux-test linux-feature-maturity linux-feature-maturity-check linux-shortcut-test linux-overlay-test linux-text-target-test linux-conversation-test linux-run linux-install linux-uninstall linux-input-helper-install linux-input-helper-status linux-input-helper-remove linux-recording-overlay-install linux-recording-overlay-status linux-recording-overlay-remove

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

# Fast configuration, text, storage and migration checks; linux-test is the native gate.
.PHONY: linux-test-fast linux-application-test
linux-test-fast:
	cargo test --locked -p mluva-core --test reference_contracts --test persistence_contracts \
		--test prompt_and_draft_contracts --test legacy_recovery

linux-application-test:
	bash linux/tests/run_application_smoke.sh

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

.PHONY: linux-browser-target-test
linux-browser-target-test:
	bash linux/tests/run_application_smoke.sh browser-targets

.PHONY: linux-chromium-target-test
linux-chromium-target-test:
	bash linux/tests/run_application_smoke.sh browser-targets default chromium
	bash linux/tests/run_application_smoke.sh browser-targets default chromium native
	bash linux/tests/run_application_smoke.sh browser-targets default chromium native-late

.PHONY: linux-browser-application-test
linux-browser-application-test:
	bash linux/tests/run_application_smoke.sh browser-application

.PHONY: linux-chromium-application-test
linux-chromium-application-test:
	bash linux/tests/run_application_smoke.sh browser-application default chromium

linux-conversation-test:
	bash linux/tests/run_conversation_smoke.sh

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

.PHONY: linux-screenshot-test
linux-screenshot-test:
	bash linux/tests/run_application_smoke.sh screenshots

.PHONY: linux-account-test
linux-account-test:
	bash linux/tests/run_application_smoke.sh account

.PHONY: linux-agent-voice-test
linux-agent-voice-test:
	bash linux/tests/run_application_smoke.sh agent-voice
