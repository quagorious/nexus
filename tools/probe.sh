#!/usr/bin/env bash
# Walks the Quagorious auth flow and dumps sample responses to tools/samples/.
# Reads the GW2 API key from ./.gw2key (gitignored). Tokens are not printed.
set -euo pipefail
cd "$(dirname "$0")/.."
KEY=$(tr -d '[:space:]' < .gw2key)
OUT=tools/samples; mkdir -p "$OUT"
EXP=$(date -u -d '+180 days' +%Y-%m-%dT00:00:00Z)
SUB=$(curl -sf -H "Authorization: Bearer $KEY" "https://api.guildwars2.com/v2/createsubtoken?permissions=account&expire=$EXP" | jq -r .subtoken)
# New paths first; production may still answer 404 for /api/auth/*, so fall back to the legacy ones.
curl -sf -X POST https://quagorious.gg/api/auth/token -H 'Content-Type: application/json' \
  -d "{\"grant_type\":\"subtoken\",\"client_id\":\"blish\",\"subtoken\":\"$SUB\"}" > "$OUT/session.json" ||
curl -sf -X POST https://quagorious.gg/api/blish/session -H 'Content-Type: application/json' \
  -d "{\"subtoken\":\"$SUB\"}" > "$OUT/session.json"
LUSHM=$(jq -r .token "$OUT/session.json")
jq '.token="<redacted>"' "$OUT/session.json" > "$OUT/session.redacted.json"; rm "$OUT/session.json"
LIVE=$(curl -sf -H "Authorization: Bearer $LUSHM" https://quagorious.gg/api/auth/live-token ||
  curl -sf -H "Authorization: Bearer $LUSHM" https://quagorious.gg/api/blish/live-token)
LIVE=$(jq -r .token <<<"$LIVE")
for p in encounters "encounters?limit=5" recent history me latest; do
  code=$(curl -s -o "$OUT/try_${p//[?=&]/_}.json" -w '%{http_code}' -H "Authorization: Bearer $LUSHM" "https://quagorious.gg/api/blish/$p")
  echo "$p -> $code"
done
echo "listening to SSE for ${SSE_SECS:-20}s ..."
timeout "${SSE_SECS:-20}" curl -sfN "https://live.quagorious.gg/events?token=$LIVE" > "$OUT/events.txt" ||
  timeout "${SSE_SECS:-20}" curl -sN "https://live.quagorious.gg/api/blish/events?token=$LIVE" > "$OUT/events.txt" || true
head -c 3000 "$OUT/events.txt"; echo
for id in $(grep -oE '"(id|encounterId|uploadId)":"?[A-Za-z0-9_-]+' "$OUT/events.txt" | sed -E 's/.*:"?//' | sort -u | head -3) ${ENC_ID:-}; do
  curl -s -H "Authorization: Bearer $LUSHM" "https://quagorious.gg/api/blish/encounters/$id?locale=de" > "$OUT/encounter_$id.json"
  echo "encounter $id: $(wc -c < "$OUT/encounter_$id.json") bytes"
done
