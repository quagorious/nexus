#!/usr/bin/env bash
# Records the Quagorious live feed (raw SSE lines, timestamped) to tools/samples/live_capture.log,
# reconnecting on drop. After each non-empty data line it also snapshots the newest try's detail JSON.
# Reads the GW2 key from ./.gw2key. Tokens are never written to the log.
set -uo pipefail
cd "$(dirname "$0")/.."
OUT=tools/samples; LOG=$OUT/live_capture.log; mkdir -p "$OUT"
END=$(( $(date +%s) + ${CAPTURE_SECS:-14400} ))
ts() { date '+%F %T'; }
KEY=$(tr -d '[:space:]' < .gw2key)
EXP=$(date -u -d '+30 days' +%Y-%m-%dT00:00:00Z)
SUB=$(curl -sf -H "Authorization: Bearer $KEY" "https://api.guildwars2.com/v2/createsubtoken?permissions=account&expire=$EXP" | jq -r .subtoken)
[ -n "$SUB" ] && [ "$SUB" != null ] || { echo "$(ts) subtoken failed" | tee -a "$LOG"; exit 1; }
while [ "$(date +%s)" -lt "$END" ]; do
  SESSION=$(curl -sf -X POST https://quagorious.gg/api/blish/session -H 'Content-Type: application/json' -d "{\"subtoken\":\"$SUB\"}")
  L=$(jq -r .token <<<"$SESSION"); LIVEURL=$(jq -r .liveUrl <<<"$SESSION")
  LT=$(curl -sf -H "Authorization: Bearer $L" https://quagorious.gg/api/blish/live-token | jq -r .token)
  if [ -z "$LT" ] || [ "$LT" = null ]; then echo "$(ts) # auth failed, retry in 15s" >> "$LOG"; sleep 15; continue; fi
  echo "$(ts) # connected" >> "$LOG"
  remaining=$(( END - $(date +%s) ))
  timeout "$remaining" curl -sN -H 'Accept: text/event-stream' "$LIVEURL/api/blish/events?token=$LT" 2>&1 |
  while IFS= read -r line; do
    echo "$(ts) $line" >> "$LOG"
    if [[ "$line" == data:* && ${#line} -gt 6 ]]; then
      id=$(curl -sf -H "Authorization: Bearer $L" "https://quagorious.gg/api/blish/encounters?limit=1" | jq -r '.tries[0].id')
      [ -n "$id" ] && [ "$id" != null ] && [ ! -f "$OUT/live_enc_$id.json" ] && \
        curl -sf -H "Authorization: Bearer $L" "https://quagorious.gg/api/blish/encounters/$id?locale=de" > "$OUT/live_enc_$id.json" && \
        echo "$(ts) # saved newest try $id" >> "$LOG"
    fi
  done
  echo "$(ts) # stream ended (exit ${PIPESTATUS[0]}), reconnect in 5s" >> "$LOG"; sleep 5
done
echo "$(ts) # capture finished" >> "$LOG"
