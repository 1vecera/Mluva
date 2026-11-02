. ~/henv.sh
for c in agent.codex.notes agent.codex.migration; do hyprctl dispatch "hl.dsp.focus({ workspace = 2 })" >/dev/null; hyprctl dispatch "hl.dsp.focus({ window = 'class:$c' })" >/dev/null; sleep 0.3; wtype 2; sleep 0.1; wtype -k Return; sleep 2.5; done
hyprctl dispatch "hl.dsp.focus({ window = 'class:agent.codex.notes' })" >/dev/null; sleep .3; wtype '/rename Release notes'; sleep .3; wtype -k Return; sleep 1.2
hyprctl dispatch "hl.dsp.focus({ window = 'class:agent.codex.migration' })" >/dev/null; sleep .3; wtype '/rename Migration'; sleep .3; wtype -k Return; sleep 3
~/.local/bin/mluva agents list | jq -c ".recipients[]|[.session,.name,.title,.label,.route,.delivery]"; ~/.local/bin/mluva agents list | jq -c ".recipients[0]" | cut -c1-400
grim /tmp/g2.png
