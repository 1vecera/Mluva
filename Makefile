.PHONY: build test run setup-signing release distribution ci-package open install clean smoke linux-setup linux-test linux-feature-maturity linux-feature-maturity-check linux-shortcut-test linux-overlay-test linux-text-target-test linux-run linux-install linux-uninstall linux-input-helper-install linux-input-helper-status linux-input-helper-remove linux-recording-overlay-install linux-recording-overlay-status linux-recording-overlay-remove

# Debug build
build:
	swift build

# Run all tests
test:
	swift test

# Run debug binary
run: build
	.build/debug/VoiceScribeMac

# Verify or guide creation of the stable, local-only signing identity
setup-signing:
	scripts/create-local-signing-identity.sh

# Release build + locally stable .app bundle
release:
	SIGNING_MODE=local scripts/build.sh

# Developer ID release, notarized and stapled for distribution
distribution:
	SIGNING_MODE=distribution scripts/build.sh

# Ad-hoc bundle used only to validate packaging on CI runners
ci-package:
	CI=true SIGNING_MODE=ci scripts/build.sh

# Open the app
open: release
	open "build/Mluva.app"

# Install to /Applications
install: release
	@pkill -x VoiceScribeMac 2>/dev/null || true
	@staging="/Applications/.Mluva.installing.$$$$.app"; \
	backup="$$HOME/.Trash/Mluva previous $$(date '+%Y-%m-%d %H.%M.%S').app"; \
	legacy_backup="$$HOME/.Trash/Voice Scribe previous $$(date '+%Y-%m-%d %H.%M.%S').app"; \
	ditto "build/Mluva.app" "$$staging"; \
	codesign --verify --deep --strict "$$staging"; \
	if [ -e "/Applications/Mluva.app" ]; then \
		mv "/Applications/Mluva.app" "$$backup"; \
		echo "Previous app moved to $$backup"; \
	fi; \
	if [ -e "/Applications/Voice Scribe.app" ]; then \
		mv "/Applications/Voice Scribe.app" "$$legacy_backup"; \
		echo "Legacy app moved to $$legacy_backup"; \
	fi; \
	mv "$$staging" "/Applications/Mluva.app"
	@echo "Installed to /Applications/Mluva.app"

# Full smoke test: unit tests + build + launch + verify + shutdown
smoke:
	@bash scripts/smoke-test.sh

# Clean build artifacts
clean:
	swift package clean
	rm -rf build/ .build/

linux-setup:
	@cd linux && if ! test -x .venv/bin/python \
		|| ! uv run --no-sync python -c 'import gi; gi.require_version("Gtk", "4.0"); gi.require_version("Adw", "1"); gi.require_version("Atspi", "2.0"); gi.require_version("DBus", "1.0"); gi.require_version("cairo", "1.0"); from gi.repository import Adw, Atspi, DBus, Gtk, cairo' >/dev/null 2>&1; then \
		uv venv --clear --system-site-packages --python /usr/bin/python3; \
	fi
	cd linux && uv sync --locked

linux-test: linux-setup
	cd linux && uv run --locked python ../scripts/render_feature_maturity.py --check
	cd linux && uv run --locked pytest -q
	cd linux && uv run --locked ruff check .
	cd linux && uv run --locked ruff format --check .

linux-feature-maturity: linux-setup
	cd linux && uv run --locked python ../scripts/render_feature_maturity.py --write

linux-feature-maturity-check: linux-setup
	cd linux && uv run --locked python ../scripts/render_feature_maturity.py --check

linux-shortcut-test: linux-setup
	bash linux/tests/run_global_shortcut_portal_smoke.sh

linux-overlay-test:
	bash linux/tests/run_recording_overlay_smoke.sh

linux-text-target-test: linux-setup
	bash linux/tests/run_native_text_target_smoke.sh

linux-run: linux-setup
	cd linux && uv run --locked python -m voice_scribe_linux.app

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
