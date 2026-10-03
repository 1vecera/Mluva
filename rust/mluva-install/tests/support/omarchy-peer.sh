#!/usr/bin/env bash
# Separate private command peer. Validation uses the installed schema checker;
# shell registration/failures are synthetic and never contact a session bus.
set -euo pipefail
: "${MLUVA_WIDGET_PEER_DIR:?}"
printf '%s\t' "${0##*/}" "$@" >> "$MLUVA_WIDGET_PEER_DIR/calls.tsv"
printf '\n' >> "$MLUVA_WIDGET_PEER_DIR/calls.tsv"
operation="${1}-${2}"
counter="$MLUVA_WIDGET_PEER_DIR/${operation}.count"
count=0
if [[ -f "$counter" ]]; then read -r count < "$counter"; fi
count=$((count + 1))
printf '%s\n' "$count" > "$counter"
if [[ "${MLUVA_WIDGET_FAILURE:-}" == "$operation-$count" ]]; then exit 7; fi
case "$operation" in
    plugin-validate)
        /usr/share/omarchy/bin/omarchy-plugin-validate "$3"
        if [[ "${MLUVA_WIDGET_FAILURE:-}" == promotion-conflict && "$count" == 2 ]]; then
            mkdir "$HOME/.config/omarchy/plugins/mluva.dictation"
            printf 'unrelated widget\n' > "$HOME/.config/omarchy/plugins/mluva.dictation/foreign.qml"
        fi
        ;;
    shell-rescanPlugins)
        if [[ "${MLUVA_WIDGET_FAILURE:-}" == replaced-target && "$count" == 1 ]]; then
            mv "$HOME/.config/omarchy/plugins/mluva.dictation" "$MLUVA_WIDGET_PEER_DIR/promoted-widget"
            mkdir "$HOME/.config/omarchy/plugins/mluva.dictation"
            printf 'unrelated widget\n' > "$HOME/.config/omarchy/plugins/mluva.dictation/foreign.qml"
            exit 7
        fi
        ;;
    plugin-list)
        if [[ "${MLUVA_WIDGET_FAILURE:-}" == malformed ]]; then printf 'private-invalid-json\n'
        elif (( count <= ${MLUVA_WIDGET_DELAY:-0} )); then printf '[]\n'
        else printf '[{"id":"mluva.dictation"}]\n'; fi
        ;;
    plugin-enable) ;;
    *) exit 91 ;;
esac
