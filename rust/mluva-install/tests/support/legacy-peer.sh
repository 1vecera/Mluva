#!/usr/bin/env bash
# Controlled external desktop/service endpoints. Unit writes are allowed only
# after checking the private mount, and never use the host's sudo or systemd.
set -euo pipefail
name=${0##*/}
pause_migration() {
    sleep 300 &
    printf '%s\n' "$!" > "$MLUVA_INSTALL_PEER/child"
    touch "$MLUVA_INSTALL_PEER/waiting"
    wait
}
printf '%s %s\n' "$name" "$*" >> "$MLUVA_INSTALL_PEER/commands"
case "$name" in
    gsettings)
        if [[ "$1" == get ]]; then jq -er --arg schema "$2" --arg key "$3" '.[$schema][$key]' "$MLUVA_INSTALL_PEER/settings.json"
        else
            [[ "$1" == set && "$#" == 4 ]] || exit 96
            if [[ "$MLUVA_INSTALL_CASE" == desktop-failure && "$3" == enabled-extensions && "$4" != *voicescribe* ]]; then exit 19; fi
            jq --arg schema "$2" --arg key "$3" --arg value "$4" '.[$schema][$key]=$value' "$MLUVA_INSTALL_PEER/settings.json" > "$MLUVA_INSTALL_PEER/settings.tmp"
            mv "$MLUVA_INSTALL_PEER/settings.tmp" "$MLUVA_INSTALL_PEER/settings.json"
        fi ;;
    systemctl)
        if [[ "$1" == list-units ]]; then
            owner=$(id -u)
            [[ "$MLUVA_INSTALL_CASE" != service-other-user ]] || owner=42424
            printf 'voice-scribe-input@%s.service loaded active running\n' "$owner"
        elif [[ "$1" == is-enabled ]]; then jq -e .old_enabled "$MLUVA_INSTALL_PEER/service.json" >/dev/null
        elif [[ "$1" == is-active ]]; then jq -e .old_active "$MLUVA_INSTALL_PEER/service.json" >/dev/null
        else exit 96; fi ;;
    sudo)
        [[ "$(stat -Lc %d:%i /etc/systemd/system)" == "$MLUVA_MIGRATION_UNIT_ID" ]] || exit 98
        case "$1" in
            -v) [[ "$MLUVA_INSTALL_CASE" != service-auth-failure ]] || exit 21 ;;
            systemctl)
                [[ "$2" != daemon-reload ]] || exit 0
                role=old; [[ "${!#}" != mluva-input@* ]] || role=new
                if [[ "$MLUVA_INSTALL_CASE" == service-restore-failure && "$2" == disable && "$role" == new ]]; then exit 19; fi
                if [[ "$MLUVA_INSTALL_CASE" == service-restore-* && "$2" == start && "$role" == new ]]; then
                    if [[ "$MLUVA_INSTALL_CASE" == service-restore-foreign ]]; then printf 'concurrent service owner\n' > /etc/systemd/system/mluva-input@.service; fi
                    exit 23
                fi
                if [[ "$MLUVA_INSTALL_CASE" == service-start-failure && "$2" == start && "$role" == new ]]; then exit 23; fi
                if [[ "$MLUVA_INSTALL_CASE" == service-signal && "$2" == start && "$role" == new ]]; then
                    pause_migration
                fi
                operation=$2
                jq --arg role "$role" --arg op "$operation" 'if $op=="disable" then .[$role+"_enabled"]=false | .[$role+"_active"]=false elif $op=="enable" then .[$role+"_enabled"]=true elif $op=="start" then .[$role+"_active"]=true else error("unexpected operation") end' "$MLUVA_INSTALL_PEER/service.json" > "$MLUVA_INSTALL_PEER/service.tmp"
                mv "$MLUVA_INSTALL_PEER/service.tmp" "$MLUVA_INSTALL_PEER/service.json" ;;
            mv)
                shift
                if [[ "$1" == --no-clobber ]]; then [[ "$2" == --no-target-directory ]] || exit 96; shift 2; fi
                [[ "$#" == 2 && "$1" == /etc/systemd/system/voice-scribe-input@.service && "$2" == /etc/systemd/system/mluva-input@.service ]] || exit 96
                if [[ "$MLUVA_INSTALL_CASE" == service-move-foreign ]]; then printf 'concurrent service owner\n' > "$2"; fi
                /usr/bin/mv --no-clobber --no-target-directory "$@"
                if [[ "$MLUVA_INSTALL_CASE" == service-move-signal ]]; then pause_migration; fi ;;
            rm) [[ "$#" == 3 && "$2" == -f && "$3" == /etc/systemd/system/mluva-input@.service ]] || exit 96; /usr/bin/rm -f -- "$3" ;;
            install) [[ "$#" == 5 && "$2" == -m && "$3" == 0644 && "$4" == "$MLUVA_MIGRATION_BACKUP_PREFIX/"* && "$5" == /etc/systemd/system/voice-scribe-input@.service ]] || exit 96; /usr/bin/install -m 0644 "$4" "$5" ;;
            *) exit 96 ;;
        esac ;;
    *) exit 96 ;;
esac
