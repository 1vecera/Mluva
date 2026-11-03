#!/usr/bin/env bash
set -euo pipefail
archive=$1; root=$2
inv="$root/.mluva-native.json"
files=0; bad=0
while IFS=$'\t' read -r path hash; do
    files=$((files+1))
    [ "$(sha256sum "$root/$path" | cut -d' ' -f1)" = "$hash" ] || { echo "MISMATCH $path"; bad=$((bad+1)); }
done < <(jq -r '.sha256 | to_entries[] | [.key, .value] | @tsv' "$inv")
links=0
while IFS=$'\t' read -r path target; do
    links=$((links+1))
    [ "$(readlink "$root/$path")" = "$target" ] || { echo "LINK MISMATCH $path"; bad=$((bad+1)); }
done < <(jq -r '.links | to_entries[] | [.key, .value] | @tsv' "$inv")
# Every shipped regular file is inventoried, and nothing else is.
shipped=$(cd "$root" && find . -type f ! -name .mluva-native.json | sed 's|^\./||' | sort | wc -l)
test "$shipped" = "$files" || { echo "inventory lists $files files but the archive holds $shipped"; bad=$((bad+1)); }
# Executables stay executable; app, bundle and widget report one version.
for exe in bin/mluva bin/mluva-install bin/mluva-uninstall bin/mluva-shell install.sh uninstall.sh; do
    test -x "$root/$exe" || { echo "NOT EXECUTABLE $exe"; bad=$((bad+1)); }
done
version=$(jq -r .version "$inv")
for manifest in manifest.json quickshell/mluva.dictation/manifest.json; do
    test "$(jq -r .version "$root/$manifest")" = "$version" || { echo "VERSION $manifest"; bad=$((bad+1)); }
done
test "$bad" = 0
jq -n --arg version "$version" --argjson files "$files" --argjson links "$links" \
    --arg native "$(sha256sum "$root/bin/mluva" | cut -d' ' -f1)" \
    --arg archive "$(sha256sum "$archive" | cut -d' ' -f1)" \
    '{version:$version, files_verified:$files, links_verified:$links, native_sha256:$native, archive_sha256:$archive}'
