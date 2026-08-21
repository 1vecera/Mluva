#!/usr/bin/env bash
set -euo pipefail

secret_config_dir="${DAS_CONF_DIR:-${XDG_CONFIG_HOME:-${HOME}/.config}/daniel-ai-skills}"
environment_dir="${secret_config_dir}/env"
agent_references="${environment_dir}/agent.env"
profile_references="${environment_dir}/voice-scribe.env"
managed_launcher="${secret_config_dir}/bin/das-mcp-launch"
target_prefix="DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL="

if [[ ! -x "${managed_launcher}" ]]; then
    echo "Mluva requires the installed daniel-ai-skills managed launcher." >&2
    exit 1
fi
if [[ ! -f "${agent_references}" ]]; then
    echo "Mluva could not find the managed agent reference catalog." >&2
    exit 1
fi

install -d -m 0700 "${environment_dir}"
temporary_profile="$(mktemp "${profile_references}.tmp.XXXXXX")"
trap 'rm -f -- "${temporary_profile}"' EXIT HUP INT TERM

if ! awk -v target_prefix="${target_prefix}" '
    index($0, target_prefix) == 1 {
        reference = substr($0, length(target_prefix) + 1)
        if (reference !~ /^op:\/\/[^/]+\/[^/]+\/.+$/) {
            exit 2
        }
        print "ELEVENLABS_API_KEY=" reference
        matches++
    }
    END {
        if (matches != 1) {
            exit 3
        }
    }
' "${agent_references}" > "${temporary_profile}"; then
    echo "Mluva could not resolve one reviewed ElevenLabs credential reference." >&2
    exit 1
fi

chmod 0600 "${temporary_profile}"
mv -f -- "${temporary_profile}" "${profile_references}"
trap - EXIT HUP INT TERM
echo "Configured the scoped Mluva secret profile."
