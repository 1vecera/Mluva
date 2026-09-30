#!/usr/bin/env bash
# Explicitly build and activate the narration extension for Omarchy's existing editor.
set -euo pipefail
source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
prebuilt_dir=""
if (( $# == 2 )) && [[ "$1" == --prebuilt-dir && "$2" == /* ]]; then
    prebuilt_dir="$2"
elif (( $# != 0 )); then
    echo "Usage: install-narrated-editor.sh [--prebuilt-dir /absolute/editor-directory]" >&2
    exit 2
fi
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
if [[ -z "${prebuilt_dir}" ]]; then
    bash "${source_dir}/build-narrated-editor.sh" "${data_home}/mluva/tensaku"
else
    integration_dir="${source_dir}/integrations/tensaku"
    for source_file in LICENSE NOTICE upstream-commit narration.patch; do
        if [[ ! -f "${prebuilt_dir}/${source_file}" || -L "${prebuilt_dir}/${source_file}" ]] \
            || ! cmp -s "${prebuilt_dir}/${source_file}" "${integration_dir}/${source_file}"; then
            echo "Editor source metadata does not match this Mluva release: ${source_file}" >&2
            exit 5
        fi
    done
    editor="${prebuilt_dir}/tensaku"
    if [[ ! -f "${editor}" || ! -x "${editor}" || -L "${editor}" ]]; then
        echo "Missing prebuilt Tensaku executable." >&2
        exit 5
    fi
    "${editor}" --help | rg -q -- --narration-command
    destination="${data_home}/mluva/tensaku"
    mkdir -p "${destination}"
    install -m 755 "${editor}" "${destination}/tensaku"
    install -m 644 "${prebuilt_dir}/LICENSE" "${prebuilt_dir}/NOTICE" \
        "${prebuilt_dir}/upstream-commit" "${prebuilt_dir}/narration.patch" "${destination}/"
    for extra in RUST-DEPENDENCIES.json BUILD-INFO.json README.txt; do
        if [[ -f "${prebuilt_dir}/${extra}" && ! -L "${prebuilt_dir}/${extra}" ]]; then
            install -m 644 "${prebuilt_dir}/${extra}" "${destination}/"
        fi
    done
    if [[ -d "${prebuilt_dir}/third-party-licenses" && ! -L "${prebuilt_dir}/third-party-licenses" ]]; then
        cp -R --no-dereference "${prebuilt_dir}/third-party-licenses" "${destination}/"
    fi
fi
ln -sfn "${editor_launcher}" "${default_editor}"
echo "Narration enabled in the default Tensaku screenshot editor."
