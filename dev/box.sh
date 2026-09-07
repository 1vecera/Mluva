#!/usr/bin/env bash
# Own one small Linux workspace without mounting a desktop, secret store or Docker socket.
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
name="${MLUVA_DEV_NAME:-mluva-dev}"
image="${MLUVA_DEV_IMAGE:-mluva-dev:0.1.1}"

require_owned_box() {
    local owner
    owner="$(docker inspect --format '{{ index .Config.Labels "dev.mluva.root" }}' "$name")"
    [[ "$owner" == "$root" ]] || {
        echo "Refusing to operate on a container owned by another checkout: $name" >&2
        exit 1
    }
}

case "${1:-help}" in
    build)
        docker build -t "$image" "$root/dev"
        ;;
    up)
        if docker container inspect "$name" >/dev/null 2>&1; then
            require_owned_box
            docker start "$name"
        else
            docker run -d --name "$name" --label "dev.mluva.root=$root" \
                --cpus=2 --memory=3g --pids-limit=256 \
                --security-opt=no-new-privileges --cap-drop=ALL \
                --mount "type=bind,source=$root,target=/workspace" "$image"
        fi
        ;;
    exec)
        require_owned_box
        shift
        docker exec "$name" "$@"
        ;;
    shell)
        require_owned_box
        docker exec -it "$name" bash
        ;;
    check)
        require_owned_box
        docker exec "$name" bash -lc \
            'make linux-test linux-shortcut-test && shellcheck linux/*.sh linux/tests/*.sh scripts/*.sh dev/*.sh'
        ;;
    capture)
        require_owned_box
        shift
        docker exec -e "MLUVA_SOURCE_COMMIT=$(git -C "$root" rev-parse HEAD)" "$name" bash dev/capture.sh "$@"
        ;;
    stop)
        require_owned_box
        docker stop "$name"
        ;;
    remove)
        require_owned_box
        docker stop "$name" >/dev/null
        docker rm "$name"
        ;;
    *)
        echo "Usage: dev/box.sh {build|up|exec <command>...|shell|check|capture [tmp/path]|stop|remove}"
        ;;
esac
