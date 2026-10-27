#!/usr/bin/env bash
# Take B, the dial: the production session wheel over a busy synthetic desktop.
#
#   wheel_take.sh BINARIES EVIDENCE_DIR
#
# Starts a private headless sway at 3840x2160, scale 2 (sway_session.sh), tiles three Ghostty
# windows with Omarchy's gaps, borders and Vantablack colours over an invented project (an editor,
# a git history and a test run, all synthetic), then runs launch_wheel_take, which opens the
# production SessionWheel as a layer-shell overlay over them and records it frame by frame.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)

if [[ ${1:-} == --inside ]]; then
    binaries=$2 ev=$TAKE_EVIDENCE
    theme=/usr/share/omarchy/themes/${MLUVA_TAKE_THEME:-vantablack}/colors.toml
    python3 - "$theme" /usr/share/omarchy/default/themed/ghostty.conf.tpl "$XDG_CONFIG_HOME/ghostty/theme.conf" <<'PY'
import pathlib, re, sys, tomllib
colors = tomllib.loads(pathlib.Path(sys.argv[1]).read_text())
colors.setdefault("selection_background", colors.get("selection", "#1a1a1a"))
colors.setdefault("selection_foreground", colors["foreground"])
text = re.sub(r"\{\{\s*(\w+)\s*\}\}", lambda m: colors[m.group(1)], pathlib.Path(sys.argv[2]).read_text())
out = pathlib.Path(sys.argv[3]); out.parent.mkdir(parents=True, exist_ok=True); out.write_text(text)
PY
    # Omarchy's own Ghostty defaults, with the blink off so a stepped take has no stray motion.
    cat > "$XDG_CONFIG_HOME/ghostty/config" <<EOF
config-file = $XDG_CONFIG_HOME/ghostty/theme.conf
font-family = "JetBrainsMono Nerd Font"
font-size = 9
window-padding-x = 14
window-padding-y = 14
confirm-close-surface = false
resize-overlay = never
cursor-style = block
cursor-style-blink = false
gtk-single-instance = false
window-decoration = false
EOF
    # An invented project: every file, commit and test below is synthetic.
    project=$HOME/tidepool
    mkdir -p "$project/src"
    cp "$here/fixtures/schedule.rs" "$project/src/schedule.rs"
    (
        cd "$project"
        export GIT_AUTHOR_NAME="Take Fixture" GIT_AUTHOR_EMAIL=take@example.invalid GIT_COMMITTER_NAME="Take Fixture" GIT_COMMITTER_EMAIL=take@example.invalid
        git init -q -b main
        i=0
        while IFS= read -r message; do
            i=$((i + 1))
            printf '%s\n' "$message" >> CHANGES
            git add -A
            GIT_AUTHOR_DATE="2026-10-$((10 + i))T09:$((10 + i)):00Z" GIT_COMMITTER_DATE="2026-10-$((10 + i))T09:$((10 + i)):00Z" git commit -q -m "$message"
        done < "$here/fixtures/commits.txt"
    )
    cat > "$HOME/prompt.sh" <<'EOF'
export PS1='\[\e[38;2;122;122;122m\]~/tidepool\[\e[0m\] \[\e[38;2;236;236;236m\]❯\[\e[0m\] ' PROMPT_COMMAND=
typed() { printf '\e[38;2;122;122;122m~/tidepool\e[0m \e[38;2;236;236;236m❯\e[0m %s\n' "$1"; }
cd ~/tidepool
EOF
    cat > "$HOME/history.sh" <<'EOF'
source ~/prompt.sh
typed "git log --graph --oneline --decorate -14"
git -c color.ui=always log --graph --oneline --decorate -14
exec bash --noprofile --rcfile ~/prompt.sh -i
EOF
    cat > "$HOME/tests.sh" <<EOF
source ~/prompt.sh
typed "cargo test --release schedule"
cat "$here/fixtures/test-run.txt"
exec bash --noprofile --rcfile ~/prompt.sh -i
EOF
    ghost() { ghostty --class="$1" -e bash "${@:2}" > "$ev/ghostty-$1.log" 2>&1 & }
    window() { for _ in {1..200}; do swaymsg -t get_tree | grep -q "\"app_id\": \"$1\"" && return; sleep 0.1; done; echo "no window $1" >&2; exit 5; }
    ghost take.editor -c "cd ~/tidepool && exec nvim -u NONE -c 'syntax on' -c 'set number termguicolors laststatus=2 noshowmode' -c 'colorscheme default' -c 'call cursor(24, 9)' src/schedule.rs"
    window take.editor
    ghost take.history "$HOME/history.sh"
    window take.history
    swaymsg '[app_id="take.history"] focus' > /dev/null
    swaymsg splitv > /dev/null
    ghost take.tests "$HOME/tests.sh"
    window take.tests
    swaymsg '[app_id="take.editor"] resize set width 54 ppt' > /dev/null
    swaymsg '[app_id="take.editor"] focus' > /dev/null
    sleep 4
    swaymsg -t get_tree > "$ev/sway-tree.json"
    "$binaries/examples/launch_wheel_take" "$ev"
    exit
fi

binaries=$(realpath "$1") evidence=$2
exec bash "$here/sway_session.sh" "$evidence" "$here/sway.config" -- bash "$0" --inside "$binaries"
