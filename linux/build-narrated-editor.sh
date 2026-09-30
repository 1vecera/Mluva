#!/usr/bin/env bash
# Build the pinned Tensaku narration extension into a reviewable output directory.
set -euo pipefail

source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
integration_dir="${source_dir}/integrations/tensaku"
scratch_root="${source_dir}/../tmp"
output_dir="${1:-${scratch_root}/narrated-editor}"
if (( $# > 1 )) || [[ "${output_dir}" != /* || "${output_dir}" == / ]]; then
    echo "Usage: build-narrated-editor.sh [absolute output directory]" >&2
    exit 2
fi
for prerequisite in git cargo pkg-config install rg; do
    command -v "${prerequisite}" >/dev/null || {
        echo "Missing build prerequisite: ${prerequisite}" >&2
        exit 3
    }
done
pkg-config --exists gtk4 libadwaita-1 fontconfig epoxy gtk4-layer-shell-0 wayland-client
commit="$(cat "${integration_dir}/upstream-commit")"
[[ "${commit}" =~ ^[a-f0-9]{40}$ ]]
mkdir -p "${scratch_root}"
build_root="$(mktemp -d "${scratch_root}/tensaku-build.XXXXXXXX")"
trap 'rm -rf -- "${build_root}"' EXIT
git clone --quiet --no-checkout https://github.com/jondkinney/tensaku.git "${build_root}/source"
git -C "${build_root}/source" checkout --quiet --detach "${commit}"
git -C "${build_root}/source" apply --check "${integration_dir}/narration.patch"
git -C "${build_root}/source" apply "${integration_dir}/narration.patch"
target_dir="${CARGO_TARGET_DIR:-${build_root}/source/target}"
if [[ "${target_dir}" != /* ]]; then
    target_dir="${build_root}/source/${target_dir}"
fi
built_editor="${target_dir}/release/tensaku"
(
    cd "${build_root}/source"
    cargo build --locked --release
    "${built_editor}" --help | rg -q -- --narration-command
)
mkdir -p "${output_dir}"
install -m 755 "${built_editor}" "${output_dir}/tensaku"
install -m 644 "${integration_dir}/LICENSE" "${integration_dir}/NOTICE" \
    "${integration_dir}/upstream-commit" "${integration_dir}/narration.patch" "${output_dir}/"
"${output_dir}/tensaku" --version
echo "Built editor: ${output_dir}/tensaku"
