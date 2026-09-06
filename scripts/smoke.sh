#!/usr/bin/env bash
#
# End-to-end check against a running API. Covers what unit tests cannot: the middleware
# pipeline, real HTTP status codes, and cross-tenant isolation through the actual router.
#
# Against a host-run dev server:
#   ./scripts/smoke.sh
# Against the Docker stack:
#   API=http://localhost:8080/api/v1 EXEC="docker compose exec -T api" \
#     DATA_DIR=/app/var/data ./scripts/smoke.sh
#
# Run it against a throwaway database — it registers accounts and creates workspaces.
set -uo pipefail
API=${API:-http://127.0.0.1:8123/api/v1}
BACKEND=${BACKEND:-backend}

# Where the SQLite files actually live, and how to reach them. Under Docker the databases are
# in a named volume rather than the bind mount, so the few checks that inspect files directly
# have to run inside the container.
DATA_DIR=${DATA_DIR:-$BACKEND/var/data}
EXEC=${EXEC:-}
PHP_BIN=${PHP_BIN:-php}
SUFFIX=$(date +%s)

run_php() { $EXEC $PHP_BIN -r "$1"; }

# UUIDv7 without the autoloader, so the script does not care where vendor/ lives.
new_uuid() {
  python3 -c '
import os, time
b = bytearray(os.urandom(16))
ms = int(time.time() * 1000)
b[0:6] = ms.to_bytes(6, "big")
b[6] = (b[6] & 0x0F) | 0x70
b[8] = (b[8] & 0x3F) | 0x80
h = b.hex()
print(f"{h[:8]}-{h[8:12]}-{h[12:16]}-{h[16:20]}-{h[20:]}")
'
}
file_exists() { if $EXEC test -f "$1" 2>/dev/null; then echo yes; else echo no; fi; }
J='Content-Type: application/json'
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  ok   $1"; pass=$((pass+1)); else echo "  FAIL $1 (expected $3, got $2)"; fail=$((fail+1)); fi; }

echo "== register =="
R=$(curl -sS -X POST "$API/auth/register" -H "$J" \
  -d '{"email":"tito+'"$SUFFIX"'@example.test","display_name":"Tito","password":"correct-horse-battery"}')
TOK=$(echo "$R" | php -r 'echo json_decode(stream_get_contents(STDIN),true)["access_token"] ?? "";')
check "register returns access token" "$([ -n "$TOK" ] && echo yes || echo no)" "yes"

echo "== account =="
ME=$(curl -sS "$API/account" -H "Authorization: Bearer $TOK")
WS=$(echo "$ME" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["workspaces"][0]["id"] ?? "";')
ROLE=$(echo "$ME" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["workspaces"][0]["role"] ?? "";')
KIND=$(echo "$ME" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["workspaces"][0]["kind"] ?? "";')
check "personal workspace auto-created" "$KIND" "personal"
check "creator is owner" "$ROLE" "owner"
check "TOTP not forced for personal-only owner" \
  "$(echo "$ME" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["totp"]["required"]?"yes":"no";')" "no"

echo "== unauthenticated =="
check "no token -> 401" "$(curl -sS -o /dev/null -w '%{http_code}' "$API/account")" "401"

echo "== workspace file created =="
check "workspace db file exists" "$(file_exists "$DATA_DIR/workspace/$WS.sqlite")" "yes"

echo "== sync push =="
SONG=$(new_uuid)
OP1=$(new_uuid)
PUSH=$(curl -sS -X POST "$API/workspaces/$WS/sync/push" -H "$J" -H "Authorization: Bearer $TOK" \
  -d "{\"ops\":[{\"op_id\":\"$OP1\",\"table\":\"songs\",\"record_id\":\"$SONG\",\"op\":\"upsert\",\"payload\":{\"title\":\"Be Thou My Vision\",\"original_key\":\"D\"}}]}")
check "push applied" "$(echo "$PUSH" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["results"][0]["status"] ?? "none";')" "applied"
check "change_seq advanced to 1" "$(echo "$PUSH" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["change_seq"] ?? 0;')" "1"

echo "== idempotency =="
PUSH2=$(curl -sS -X POST "$API/workspaces/$WS/sync/push" -H "$J" -H "Authorization: Bearer $TOK" \
  -d "{\"ops\":[{\"op_id\":\"$OP1\",\"table\":\"songs\",\"record_id\":\"$SONG\",\"op\":\"upsert\",\"payload\":{\"title\":\"Should Not Apply\"}}]}")
check "replayed op is a duplicate" "$(echo "$PUSH2" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["results"][0]["status"] ?? "none";')" "duplicate"

echo "== sync pull =="
PULL=$(curl -sS "$API/workspaces/$WS/sync/pull?since=0" -H "Authorization: Bearer $TOK")
check "pull returns the song" "$(echo "$PULL" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["tables"]["songs"][0]["title"] ?? "none";')" "Be Thou My Vision"
check "title not overwritten by duplicate" "$(echo "$PULL" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo count($d["tables"]["songs"]);')" "1"
check "watermark pull returns nothing" "$(curl -sS "$API/workspaces/$WS/sync/pull?since=1" -H "Authorization: Bearer $TOK" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo count($d["tables"]["songs"]);')" "0"

echo "== no workspace_id column anywhere =="
LEAK=$(run_php '$p = new PDO("sqlite:'"$DATA_DIR"'/workspace/'"$WS"'.sqlite"); $hits = 0; foreach ($p->query("SELECT name FROM sqlite_master WHERE type=\"table\"") as $t) { foreach ($p->query("PRAGMA table_info(".$t["name"].")") as $c) { if ($c["name"] === "workspace_id") { $hits++; } } } echo $hits;')
check "content schema has no workspace_id" "$LEAK" "0"

echo "== cross-tenant isolation =="
R2=$(curl -sS -X POST "$API/auth/register" -H "$J" \
  -d '{"email":"mallory+'"$SUFFIX"'@example.test","display_name":"Mallory","password":"correct-horse-battery"}')
TOK2=$(echo "$R2" | php -r 'echo json_decode(stream_get_contents(STDIN),true)["access_token"] ?? "";')
check "non-member pull -> 404" "$(curl -sS -o /dev/null -w '%{http_code}' "$API/workspaces/$WS/sync/pull?since=0" -H "Authorization: Bearer $TOK2")" "404"
check "non-member push -> 404" "$(curl -sS -o /dev/null -w '%{http_code}' -X POST "$API/workspaces/$WS/sync/push" -H "$J" -H "Authorization: Bearer $TOK2" -d '{"ops":[]}')" "404"
check "non-member members list -> 404" "$(curl -sS -o /dev/null -w '%{http_code}' "$API/workspaces/$WS/members" -H "Authorization: Bearer $TOK2")" "404"

echo "== band workspace + owner TOTP requirement =="
BAND=$(curl -sS -X POST "$API/workspaces" -H "$J" -H "Authorization: Bearer $TOK" -d '{"name":"Sunday Band"}')
BANDID=$(echo "$BAND" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["workspace"]["id"] ?? "";')
check "band workspace created" "$([ -n "$BANDID" ] && echo yes || echo no)" "yes"
check "band workspace db file exists" "$(file_exists "$DATA_DIR/workspace/$BANDID.sqlite")" "yes"
ME2=$(curl -sS "$API/account" -H "Authorization: Bearer $TOK")
check "owning a band workspace makes TOTP required" \
  "$(echo "$ME2" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["totp"]["required"]?"yes":"no";')" "yes"

echo "== unknown table rejected =="
OP3=$(new_uuid)
BAD=$(curl -sS -X POST "$API/workspaces/$WS/sync/push" -H "$J" -H "Authorization: Bearer $TOK" \
  -d "{\"ops\":[{\"op_id\":\"$OP3\",\"table\":\"users\",\"record_id\":\"$SONG\",\"op\":\"upsert\",\"payload\":{\"email\":\"x@y.z\"}}]}")
check "unknown table rejected" "$(echo "$BAD" | php -r '$d=json_decode(stream_get_contents(STDIN),true); echo $d["results"][0]["code"] ?? "none";')" "unknown_table"

echo
echo "passed: $pass   failed: $fail"
[ "$fail" -eq 0 ]
