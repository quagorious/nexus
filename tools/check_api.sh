#!/usr/bin/env bash
# Contract check of the Quagorious API paths the addon depends on, against the real server.
# Reports one row per endpoint: new (api v2) vs legacy paths, expected vs actual, PASS/FAIL/PENDING.
#
#   PASS     as expected
#   PENDING  new path answers 404 (not deployed yet); FAIL instead when STRICT=1
#   FAIL     unexpected answer, network error, or a legacy path broke while the new one is missing
#   GONE     legacy path is 404 but the new one works: the legacy fallback code can be dropped
#   SKIP     precondition missing (e.g. no session token)
#
# Exit code is 1 if any row is FAIL. Authenticated checks run only with a subtoken:
# env QUAG_SUBTOKEN, or derived from ./.gw2key (gitignored) via createsubtoken like probe.sh.
# Tokens, subtokens and keys are never printed or written to disk.
#
# Env: STRICT=1 (new paths must exist), TIMEOUT=5 (seconds per request; SSE never ends, so it is the
#      stream sampling time), STREAM_TIMEOUT=30 (account streams only: they send headers with the first ping), API_BASE, LIVE_BASE, GITHUB_STEP_SUMMARY (markdown summary is appended).
set -euo pipefail
cd "$(dirname "$0")/.."

BASE=${API_BASE:-https://quagorious.gg}
LIVE=${LIVE_BASE:-https://live.quagorious.gg}
TIMEOUT=${TIMEOUT:-5}
STREAM_TIMEOUT=${STREAM_TIMEOUT:-30}   # account streams send headers only with the first ping (~25s)
STRICT=${STRICT:-0}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT

ROWS=()          # name, kind, expected, actual, status joined by US (0x1f)
SEP=$(printf '\037')
GONE_NOTES=()
R_CODE=000; R_CT=""; R_OK=0; R_ACT=""; NEW_OK=0

# req METHOD URL [BEARER] [JSON_BODY] -> R_CODE, R_CT; body in $TMP/body.
# URL and bearer go through curl config files (private temp dir, deleted right after) so they never show in argv.
req() {
  local m=$1 u=$2 auth=${3:-} data=${4:-} out
  { printf 'url = "%s"\n' "$u"
    [ -n "$auth" ] && printf 'header = "Authorization: Bearer %s"\n' "$auth"
  } > "$TMP/cfg"
  local args=(-s -m "${REQ_TIMEOUT:-$TIMEOUT}" -o "$TMP/body" -w '%{http_code}|%{content_type}' -X "$m" -K "$TMP/cfg")
  if [ -n "$data" ]; then
    printf '%s' "$data" > "$TMP/req"
    args+=(-H 'Content-Type: application/json' --data-binary @"$TMP/req")
  fi
  : > "$TMP/body"
  out=$(curl "${args[@]}" 2>/dev/null) || true   # exit 28 (timeout) is normal for SSE
  rm -f "$TMP/cfg" "$TMP/req"
  R_CODE=${out%%|*}; R_CT=${out#*|}; R_CT=${R_CT%%;*}
  [ -n "$R_CODE" ] || R_CODE=000
}

# in_spec CODE "400 401 2xx" -> 0 if CODE matches any item
in_spec() { local c=$1 s; for s in $2; do case "$c" in ${s//x/[0-9]}) return 0;; esac; done; return 1; }

# check NAME KIND(new|legacy) EXPECT METHOD URL [BEARER] [BODY] [JQ_FILTER] [LABEL]
# EXPECT: "sse" (200 + text/event-stream), "codes:400 401 422", or "json:2xx" (+ JQ_FILTER must hold)
check() {
  local name=$1 kind=$2 expect=$3 m=$4 u=$5 auth=${6:-} body=${7:-} jqf=${8:-} label=${9:-} exp_txt status
  req "$m" "$u" "$auth" "$body"
  R_OK=0; R_ACT=$R_CODE
  case "$expect" in
    sse)    exp_txt="200 text/event-stream"; R_ACT="$R_CODE ${R_CT:--}"
            [ "$R_CODE" = 200 ] && [ "$R_CT" = text/event-stream ] && R_OK=1 ;;
    codes:*) exp_txt="${expect#codes:}"; in_spec "$R_CODE" "$exp_txt" && R_OK=1 ;;
    json:*) exp_txt="${expect#json:} + $label"
            if in_spec "$R_CODE" "${expect#json:}"; then
              if jq -e "$jqf" "$TMP/body" >/dev/null 2>&1; then R_OK=1; R_ACT="$R_CODE json ok"
              else R_ACT="$R_CODE json invalid"; fi
            fi ;;
  esac
  if [ "$R_CODE" = 000 ]; then R_ACT="no response"; R_OK=0; status=FAIL
  elif [ "$kind" = new ]; then
    if [ "$R_CODE" = 404 ]; then
      if [ "$STRICT" = 1 ]; then status=FAIL; else status=PENDING; fi
    elif [ $R_OK = 1 ]; then status=PASS; else status=FAIL; fi
    NEW_OK=$R_OK
  else
    if [ "$R_CODE" = 404 ]; then
      if [ "$NEW_OK" = 1 ]; then status=GONE; GONE_NOTES+=("$name: legacy path gone, new one works -> drop fallback code")
      else status=FAIL; R_ACT="404, new path missing too (addon broken)"; fi
    elif [ $R_OK = 1 ]; then status=PASS; else status=FAIL; fi
  fi
  ROWS+=("$name$SEP$kind$SEP$exp_txt$SEP$R_ACT$SEP$status")
}
skip() { R_OK=0; ROWS+=("$1$SEP$2$SEP$3$SEP$4$SEP${5:-SKIP}"); }

echo "Checking $BASE and $LIVE (timeout ${TIMEOUT}s, STRICT=$STRICT)" >&2

# ---- unauthenticated ----
INV='{"grant_type":"subtoken","client_id":"blish","subtoken":"invalid"}'
check "public stream"      new    sse "GET" "$LIVE/events"
check "public stream"      legacy sse "GET" "$LIVE/api/public/live"
# the legacy account-stream route without token: any defined answer (not 5xx, not 404) counts
check "account stream (no token)" legacy "codes:2xx 400 401 403" "GET" "$LIVE/api/blish/events"
check "token grant"        new    "codes:400 401 403 415 422" POST "$BASE/api/auth/token" "" "$INV"
check "token grant"        legacy "codes:400 401 403 415 422" POST "$BASE/api/blish/session" "" '{"subtoken":"invalid"}'
check "device code start"  new    "codes:2xx 400 401 403 415 422" POST "$BASE/api/auth/device" "" '{"client_id":"blish"}'
check "device code start"  legacy "codes:2xx 400 401 403 415 422" POST "$BASE/api/uploader/device" "" '{}'
check "live-token (no auth)" new    "codes:401 403" GET "$BASE/api/auth/live-token"
check "live-token (no auth)" legacy "codes:401 403" GET "$BASE/api/blish/live-token"
check "encounters (no auth)" legacy "codes:401 403" GET "$BASE/api/blish/encounters?limit=1"

# ---- authenticated ----
SUB=${QUAG_SUBTOKEN:-}
if [ -z "$SUB" ] && [ -f .gw2key ]; then
  KEY=$(tr -d '[:space:]' < .gw2key)
  EXP=$(date -u -d '+1 day' +%Y-%m-%dT00:00:00Z)
  req GET "https://api.guildwars2.com/v2/createsubtoken?permissions=account&expire=$EXP" "$KEY"
  SUB=$(jq -r '.subtoken // empty' "$TMP/body" 2>/dev/null || true)
  [ -n "$SUB" ] || echo "note: createsubtoken failed (HTTP $R_CODE), skipping authenticated checks" >&2
  unset KEY
fi

if [ -z "$SUB" ]; then
  echo "note: no QUAG_SUBTOKEN or .gw2key, authenticated checks skipped" >&2
else
  OKFILTER='(.token|type=="string" and length>0) and (.expiresAt!=null) and (.liveUrl|type=="string" and length>0)'
  NEW_S=""; LEG_S=""; NEW_LT=""; LEG_LT=""

  check "token grant (auth)" new    "json:2xx" POST "$BASE/api/auth/token" "" \
    "$(jq -nc --arg s "$SUB" '{grant_type:"subtoken",client_id:"blish",subtoken:$s}')" "$OKFILTER" "token, expiresAt, liveUrl"
  [ $R_OK = 1 ] && NEW_S=$(jq -r .token "$TMP/body")
  check "token grant (auth)" legacy "json:2xx" POST "$BASE/api/blish/session" "" \
    "$(jq -nc --arg s "$SUB" '{subtoken:$s}')" "$OKFILTER" "token, expiresAt, liveUrl"
  [ $R_OK = 1 ] && LEG_S=$(jq -r .token "$TMP/body")
  unset SUB

  tokcheck() { # kind session
    local kind=$1 sess=$2 path=$3
    if [ -z "$sess" ]; then
      if [ "$kind" = new ]; then
        [ "$STRICT" = 1 ] && st=FAIL || st=PENDING; skip "live-token (auth)" new "token" "no new session" "$st"; NEW_OK=0
      else skip "live-token (auth)" legacy "token" "no legacy session"; fi
      return
    fi
    check "live-token (auth)" "$kind" "json:2xx" GET "$BASE$path" "$sess" "" '(.token|type=="string" and length>0)' "token"
  }
  tokcheck new "$NEW_S" /api/auth/live-token
  [ $R_OK = 1 ] && NEW_LT=$(jq -r .token "$TMP/body")
  tokcheck legacy "$LEG_S" /api/blish/live-token
  [ $R_OK = 1 ] && LEG_LT=$(jq -r .token "$TMP/body")

  if [ -n "$NEW_LT" ]; then REQ_TIMEOUT=$STREAM_TIMEOUT check "account stream" new sse GET "$LIVE/events?token=$NEW_LT"
  else
    [ "$STRICT" = 1 ] && st=FAIL || st=PENDING; skip "account stream" new "200 text/event-stream" "no new live token" "$st"; NEW_OK=0
  fi
  if [ -n "$LEG_LT" ]; then REQ_TIMEOUT=$STREAM_TIMEOUT check "account stream" legacy sse GET "$LIVE/api/blish/events?token=$LEG_LT"
  else skip "account stream" legacy "200 text/event-stream" "no legacy live token"; fi

  S=${LEG_S:-$NEW_S}
  if [ -n "$S" ]; then
    NEW_OK=0 # no new equivalent: a 404 here is always a FAIL
    check "encounters?limit=1" legacy "json:2xx" GET "$BASE/api/blish/encounters?limit=1" "$S" "" '.tries|type=="array"' ".tries array"
  else skip "encounters?limit=1" legacy ".tries array" "no session"; fi
fi

# ---- report ----
fails=0; pend=0
printf '\n%-28s %-7s %-34s %-34s %s\n' ENDPOINT KIND EXPECTED ACTUAL RESULT
printf '%-28s %-7s %-34s %-34s %s\n' ---------------------------- ------- ---------------------------------- ---------------------------------- -------
for r in "${ROWS[@]}"; do
  IFS=$SEP read -r n k e a s <<<"$r"
  printf '%-28s %-7s %-34s %-34s %s\n' "$n" "$k" "$e" "$a" "$s"
  [ "$s" = FAIL ] && fails=$((fails+1))
  [ "$s" = PENDING ] && pend=$((pend+1))
done
echo
for g in ${GONE_NOTES[@]+"${GONE_NOTES[@]}"}; do echo "LEGACY GONE: $g"; done
[ "$pend" -gt 0 ] && echo "$pend new path(s) PENDING (404, not deployed yet; set STRICT=1 to fail on these)"
echo "$fails failure(s)"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Quagorious API contract check"
    echo
    echo "| Endpoint | Kind | Expected | Actual | Result |"
    echo "|---|---|---|---|---|"
    for r in "${ROWS[@]}"; do
      IFS=$SEP read -r n k e a s <<<"$r"
      case "$s" in PASS) i=":white_check_mark:";; FAIL) i=":x:";; PENDING) i=":hourglass_flowing_sand:";; GONE) i=":wastebasket:";; *) i=":fast_forward:";; esac
      echo "| $n | $k | $e | $a | $i $s |"
    done
    echo
    for g in ${GONE_NOTES[@]+"${GONE_NOTES[@]}"}; do echo "- **Legacy gone:** $g"; done
    [ "$pend" -gt 0 ] && echo "- $pend new path(s) pending (not deployed yet)"
    echo "- STRICT=$STRICT, failures: $fails"
  } >> "$GITHUB_STEP_SUMMARY"
fi
[ "$fails" -eq 0 ]
