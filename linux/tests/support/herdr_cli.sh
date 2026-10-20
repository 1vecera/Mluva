#!/usr/bin/env bash
# Run only through dev/verify.sh session with the production process.
set -euo pipefail
: "${MLUVA_VERIFY_CLI:?Use dev/verify.sh session}"
: "${OFFSCREEN_SESSION_ROOT:?Private session required}"
"$MLUVA_VERIFY_CLI" doctor >/dev/null
root="$OFFSCREEN_SESSION_ROOT"
app="$root/bundle/bin/mluva"
checkout="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd -P)"
socket="$XDG_RUNTIME_DIR/voice.sock"
settings="$XDG_CONFIG_HOME/mluva/agent-voice.json"
fixture="$root/herdr-state.json"
audit="$root/herdr-audit.jsonl"
jq -n '{agents:[
  {guard:("a"*64),agent:{name:"Synthetic team",pane_id:"w1:p1",agent:"codex",agent_status:"working",cwd:"/synthetic/product"}},
  {guard:("b"*64),agent:{name:"Synthetic team",pane_id:"w1:p2",agent:"claude",agent_status:"idle",cwd:"/synthetic/research"}}
]}' > "$fixture"
"$checkout/tmp/native-build/debug/herdr-voice-fixture-peer" "$socket" "$fixture" "$audit" > "$root/herdr-peer.log" 2>&1 &
peer=$!
trap 'kill "$peer" 2>/dev/null || true; wait "$peer" 2>/dev/null || true' EXIT
for _ in {1..100}; do [[ -S "$socket" ]] && break; sleep 0.05; done
test -S "$socket"
cp "$root/app-process.json" "$root/cli-owner-before.json"
"$app" herdr list --socket "$socket" | jq -e '.ok and (.recipients|length==2)' >/dev/null
"$app" herdr status | jq -e '.ok and (.connected|not)' >/dev/null
if "$app" herdr select 'Synthetic team' --socket "$socket" > "$root/ambiguous.json"; then
  echo 'Ambiguous name unexpectedly selected' >&2; exit 1
fi
jq -e '.ok == false' "$root/ambiguous.json" >/dev/null
for pane in w1:p1 w1:p2; do
  "$app" herdr select "$pane" --socket "$socket" | jq -e --arg pane "$pane" '.connected and .persisted and .selected.pane==$pane' >/dev/null
  "$app" herdr status | jq -e --arg pane "$pane" '.connected and .selected.pane==$pane and .hold_key=="F8" and .draft_key=="F7"' >/dev/null
  "$MLUVA_VERIFY_CLI" discover > "$root/cli-selected-${pane//:/-}.json"
  jq -e --arg pane "$pane" 'any(.data.nodes[]; .role=="combo box" and (.name|contains($pane)))' "$root/cli-selected-${pane//:/-}.json" >/dev/null
  jq -e --arg pane "$pane" '.selected.pane_id==$pane' "$settings" >/dev/null
done
cp "$settings" "$root/saved-choice.json"
jq '.agents[1].agent.agent_status="blocked"' "$fixture" > "$fixture.next"
mv "$fixture.next" "$fixture"
if "$app" herdr select w1:p2 --socket "$socket" > "$root/blocked.json"; then
  echo 'Blocked recipient unexpectedly selected' >&2; exit 1
fi
cmp "$settings" "$root/saved-choice.json"
"$app" herdr status | jq -e '.ok and (.connected|not)' >/dev/null
jq '.agents[1].agent.agent_status="idle" | .agents[1].guard=("c"*64)' "$fixture" > "$fixture.next"
mv "$fixture.next" "$fixture"
"$app" herdr status | jq -e '.ok and (.connected|not)' >/dev/null
"$app" herdr select w1:p2 --socket "$socket" | jq -e '.connected' >/dev/null
"$app" herdr clear | jq -e '.ok and .selected==null and (.connected|not)' >/dev/null
jq -e '.selected==null and .hold_key=="F8" and .draft_key=="F7"' "$settings" >/dev/null
cmp "$root/cli-owner-before.json" "$root/app-process.json"
"$MLUVA_VERIFY_CLI" doctor >/dev/null
"$app" herdr select w1:p1 --socket "$socket" >/dev/null
"$MLUVA_VERIFY_CLI" action quit >/dev/null
if "$app" herdr status > "$root/stopped-app.json"; then
  echo 'A stopped app unexpectedly answered' >&2; exit 1
fi
"$MLUVA_VERIFY_CLI" launch >/dev/null
"$app" herdr status | jq -e '.connected and .selected.pane=="w1:p1"' >/dev/null
# Incognito changes the live selection but never the saved choice.
"$MLUVA_VERIFY_CLI" action quit >/dev/null
cp "$settings" "$root/before-incognito.json"
config="$XDG_CONFIG_HOME/mluva/config.json"
jq '.incognito_mode=true' "$config" > "$config.next"
mv "$config.next" "$config"
"$MLUVA_VERIFY_CLI" launch >/dev/null
"$app" herdr select w1:p2 --socket "$socket" | jq -e '.connected and (.persisted|not)' >/dev/null
"$app" herdr status | jq -e '.incognito and .connected and .selected.pane=="w1:p2"' >/dev/null
cmp "$settings" "$root/before-incognito.json"
jq -se 'all(.[]; .method=="agent.guarded_list")' "$audit" >/dev/null
"$MLUVA_VERIFY_CLI" screenshot cli-selected >/dev/null
printf '%s\n' '{"ok":true,"switch_without_restart":true,"widget_matches":true,"ambiguity_and_blocked_rejected":true,"stale_guard_disconnected":true,"restart_persists":true,"incognito_not_saved":true,"shortcuts_preserved":true,"prompts_sent":0}' > "$root/cli-result.json"
cat "$root/cli-result.json"
