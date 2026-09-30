#!/usr/bin/env bash
# Explicitly build and activate the narration extension for Omarchy's existing editor.
set -euo pipefail
source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
install_home="${MLUVA_INSTALL_HOME:-${HOME}}"
[[ "${install_home}" == /* && "${install_home}" != / ]] || exit 2
if [[ "${install_home}" == "${HOME}" ]]; then
    data_home="${XDG_DATA_HOME:-${install_home}/.local/share}"
else
    data_home="${install_home}/.local/share"
fi
[[ "${data_home}" == /* && "${data_home}" != / ]] || exit 2
bin_dir="${install_home}/.local/bin"
editor_launcher="${data_home}/mluva/app/mluva-screenshot-editor"
if [[ ! -x "${editor_launcher}" || ! -x "${bin_dir}/mluva-narrate" ]]; then
    echo "Install or upgrade Mluva before activating the narrated editor." >&2
    exit 3
fi
default_editor="${bin_dir}/tensaku-edit"
if [[ -e "${default_editor}" || -L "${default_editor}" ]]; then
    if [[ ! -L "${default_editor}" || "$(readlink -- "${default_editor}")" != "${editor_launcher}" ]]; then
        echo "Preserved an unrelated user editor: ${default_editor}" >&2
        exit 4
    fi
fi
bash "${source_dir}/build-narrated-editor.sh" "${data_home}/mluva/tensaku"
ln -sfn "${editor_launcher}" "${default_editor}"
echo "Narration enabled in the default Tensaku screenshot editor."
