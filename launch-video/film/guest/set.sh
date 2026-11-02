#!/bin/bash
# Guest side: lay out the film's desktop. Everything here is synthetic: invented notes and projects, loopback models.
. ~/henv.sh
M=$HOME/film; R=$M/run
export PATH=$M/tools/codex-pkg/bin:$M/tools:$PATH
mkdir -p ~/.codex ~/work/release-notes ~/work/landing-page ~/work/portal ~/work/migration
for d in release-notes landing-page portal migration; do (cd ~/work/$d; [ -d .git ] || git init -q; printf '# %s\n' $d > README.md; printf '## Unreleased\n' > CHANGELOG.md); done
{ cat <<T
model = "gpt-5.5"
model_provider = "fixture"
approval_policy = "never"
sandbox_mode = "read-only"
[model_providers.fixture]
name = "Local loopback model"
base_url = "http://127.0.0.1:18765/v1"
wire_api = "responses"
[features]
responses_websockets = false
responses_websockets_v2 = false
remote_models = false
T
for d in release-notes landing-page portal migration; do printf '[projects."%s/work/%s"]\ntrust_level = "trusted"\n' "$HOME" "$d"; done; } > ~/.codex/config.toml
python3 - <<'PY'
import json, os
home = os.environ["HOME"]
projects = {f"{home}/work/{d}": {"hasTrustDialogAccepted": True, "hasCompletedProjectOnboarding": True} for d in ["release-notes", "landing-page", "portal", "migration"]}
json.dump({"hasCompletedOnboarding": True, "theme": "dark", "projects": projects, "customApiKeyResponses": {"approved": ["fixture-key"], "rejected": []}}, open(f"{home}/.claude.json", "w"))
PY
systemctl --user stop film-models 2>/dev/null; systemctl --user reset-failed film-models 2>/dev/null
systemd-run --user --quiet --unit=film-models -p StandardOutput=append:$R/models.log -p StandardError=append:$R/models.log python3 $M/models.py 18765 $R/model-requests.jsonl
A="ANTHROPIC_BASE_URL=http://127.0.0.1:18765 ANTHROPIC_API_KEY=fixture-key DISABLE_AUTOUPDATER=1 DISABLE_TELEMETRY=1 DISABLE_ERROR_REPORTING=1 CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 SHELL=/bin/bash PATH=$PATH"
run() { printf '%s\n' "$1" > "$R/launch.sh"; cp "$R/launch.sh" "$R/launch-$2.sh"; hyprctl eval "hl.exec_cmd('bash $R/launch-$2.sh')" >/dev/null; }
term() { # workspace-routed app id, cwd, command, wait
  run "cd $2; exec env $A foot --app-id $1 --override=pad=14x10 --override=font='JetBrainsMono Nerd Font:size=10' sh -c '$3'" "$1"; sleep "${4:-1.2}"
}
P=$M/firefox-profile; rm -rf "$P" "$P-2"; mkdir -p "$P" "$P-2"
cat > "$P/user.js" <<U
user_pref("browser.aboutwelcome.enabled", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("startup.homepage_welcome_url", "");
user_pref("startup.homepage_override_url", "");
user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);
user_pref("browser.vpn_promo.enabled", false);
user_pref("browser.ipProtection.enabled", false);
user_pref("browser.tabs.inTitlebar", 0);
user_pref("sidebar.revamp", false);
user_pref("accessibility.force_disabled", 0);
user_pref("ui.systemUsesDarkTheme", 1);
U
cp "$P/user.js" "$P-2/user.js"
FF() { run "exec env MOZ_ENABLE_WAYLAND=1 GTK_A11Y=atspi firefox --no-remote --profile $3 file://$M/$1" "firefox-$1"; sleep "$2"; }
FF notes.html 8 "$P"
term notes.editor ~/work/release-notes 'nvim README.md CHANGELOG.md'
go() { hyprctl dispatch "hl.dsp.focus({ workspace = $1 })" >/dev/null; sleep 0.4; }
focus() { hyprctl dispatch "hl.dsp.focus({ window = 'class:$1' })" >/dev/null; sleep 0.3; }
go 2
term agent.claude.portal ~/work/portal "claude -n \"Portal tests\" --model claude-sonnet-4-5 \"Run the portal tests and tell me what fails.\"; sleep 9000" 4
term agent.codex.notes ~/work/release-notes "codex --no-alt-screen \"Read CHANGELOG.md and wait for my next request.\"; sleep 9000" 4
focus agent.claude.portal
term agent.codex.migration ~/work/migration "codex --no-alt-screen \"Read README.md and wait for my next request.\"; sleep 9000" 4
focus agent.codex.notes
term agent.claude.docs ~/work/release-notes "claude -n \"Docs\" --model claude-sonnet-4-5 \"Read README.md and wait for my next request.\"; sleep 9000" 4
sleep 2
go 3
FF settings-page.html 7 "$P-2"
term agent3.claude.landing ~/work/landing-page "claude -n \"Landing page\" --model claude-sonnet-4-5 \"Read README.md and wait for my next request.\"; sleep 9000" 5
go 1
hyprctl clients -j | jq -c '.[]|[.class,.title,.workspace.id,.at,.size]'
