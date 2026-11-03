#!/bin/bash
# shellcheck source=/dev/null
. ~/henv.sh; ~/start.sh peer; sleep 1; ~/start.sh mluva; sleep 2; bash ~/film/set.sh 2>&1 | tail -10; sleep 3; ~/.local/bin/mluva agents list | jq -c ".recipients[]|[.session,.name,.route,.delivery]"; curl -s -m 3 -o /dev/null -w "outbound %{http_code}\n" https://example.com; 
