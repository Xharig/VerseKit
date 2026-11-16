#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Starts the image named as $1 the way tools/orga-standard/docker-compose.yml does (hardened: read-only, no kernel
# rights, no new privileges; state in a volume, the configuration read-only, settings through the environment) and
# checks: it does not run as root; a request that did not come over HTTPS is refused, one through the proxy (with
# X-Forwarded-Proto) is answered with HSTS; the discovery answers with the name from the environment; the server made
# an admin password in /data, names in the log only where it lies, and shows it with --admin-password; a member logs
# in through the panel; the health check turns healthy; after the container is removed and started anew on the same
# volume the token, the refresh token and the password are still there.
#
#   bash crates/versekit-orgserver/container_check.sh versekit-orgserver:test
#
# Needs docker, curl and jq. PORT chooses the port on 127.0.0.1 (default 18080).

set -euo pipefail

image="${1:?usage: container_check.sh IMAGE}"
port="${PORT:-18080}"
base="http://127.0.0.1:$port"
api="$base/versekit/v1"
name="versekit-orgserver-check-$$"
volume="$name-data"
work="$(mktemp -d)"
key="schluessel-fuer-den-test-0001"

cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker volume rm "$volume" >/dev/null 2>&1 || true
  rm -rf "$work"
}
trap cleanup EXIT

fail() {
  echo "FAILED: $*" >&2
  docker logs "$name" >&2 2>&1 || true
  exit 1
}

# Every request as the reverse proxy in front hands it on.
c() {
  curl -H "X-Forwarded-Proto: https" "$@"
}

cat > "$work/config.json" <<EOF
{
  "public_url": "https://orga.example.org",
  "name": "Name aus der Datei",
  "tag": "PRF",
  "poll_interval": 1,
  "members": [{ "id": "m1", "name": "Pilot", "key": "$key" }]
}
EOF
chmod 644 "$work/config.json"

start() {
  docker run -d --name "$name" \
    --read-only --cap-drop ALL --security-opt no-new-privileges:true \
    -p "127.0.0.1:$port:8080" \
    -v "$volume:/data" \
    -v "$work/config.json:/etc/versekit-org/config.json:ro" \
    -e VERSEKIT_ORG_CONFIG=/etc/versekit-org/config.json \
    -e VERSEKIT_ORG_NAME="Prueforga" \
    -e VERSEKIT_ORG_RATE_LIMIT=60 \
    -e VERSEKIT_ORG_UPDATE_CHECK=0 \
    "$image" >/dev/null
  for _ in $(seq 1 100); do
    if c -fsS "$base/.well-known/versekit-org.json" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.2
  done
  fail "the server did not answer within 20 s"
}

start
echo "ok: the container started read-only, without kernel rights"

user="$(docker inspect -f '{{.Config.User}}' "$image")"
case "$user" in ""|root|0|0:0) fail "the image runs as root ($user)" ;; esac
echo "ok: the image runs as $user, not as root"

plain="$(curl -s -o /dev/null -w '%{http_code}' "$base/")"
[ "$plain" = 403 ] || fail "a request without HTTPS got $plain instead of 403"
hsts="$(c -fsS -D - -o /dev/null "$base/" | tr -d '\r' | grep -i '^strict-transport-security:' || true)"
[ -n "$hsts" ] || fail "no HSTS through the proxy"
echo "ok: without HTTPS refused, through the proxy answered with HSTS"

discovery="$(c -fsS "$base/.well-known/versekit-org.json")"
jq -e '.standard == "versekit-org" and .org.name == "Prueforga" and .org.tag == "PRF"' <<<"$discovery" >/dev/null \
  || fail "discovery: $discovery"
jq -e '.versions."1".verification_uri == "https://orga.example.org/device" and .server.name == "versekit-orgserver"' \
  <<<"$discovery" >/dev/null || fail "discovery: $discovery"
echo "ok: the discovery answers, the name comes from the environment"

password="$(docker exec "$name" /usr/local/bin/versekit-orgserver --admin-password --state /data/state.json)" # privacy-ok: reads the generated password of a throwaway check container, never printed
[ "${#password}" -ge 24 ] || fail "no generated admin password"
logs="$(docker logs "$name" 2>&1)"
grep -q "/data/admin-password.txt" <<<"$logs" || fail "the log does not name where the password lies"
if grep -qF "$password" <<<"$logs"; then fail "the log holds the password"; fi
echo "ok: the admin password was made in /data, the log names only where it lies"

code="$(c -fsS -X POST --data-urlencode "scope=stock:read orders:read" "$api/device/code")"
device="$(jq -r .device_code <<<"$code")"
user_code="$(jq -r .user_code <<<"$code")"
[ -n "$device" ] && [ "$device" != null ] || fail "device code: $code"

panel="$(c -fsS -X POST -H "Accept-Language: de-DE,de;q=0.9" --data-urlencode "user_code=$user_code" \
  --data-urlencode "key=$key" -d action=approve -d scope=stock:read "$base/device")"
grep -q "Freigegeben" <<<"$panel" || fail "panel: $panel"
echo "ok: the panel confirms the code in German for a German browser"

tokens="$(c -sS -X POST --data-urlencode "grant_type=urn:ietf:params:oauth:grant-type:device_code" \
  --data-urlencode "device_code=$device" "$api/token")"
access="$(jq -r .access_token <<<"$tokens")"
refresh="$(jq -r .refresh_token <<<"$tokens")"
jq -e '.scope == "stock:read"' <<<"$tokens" >/dev/null || fail "token: only the right ticked in the panel"
me() {
  c -fsS -H "Authorization: Bearer $1" "$api/me"
}
me "$access" | jq -e '.member.name == "Pilot"' >/dev/null || fail "/me before the restart"
echo "ok: a member logged in through the panel, the token works"

for _ in $(seq 1 30); do
  health="$(docker inspect -f '{{.State.Health.Status}}' "$name")"
  [ "$health" = healthy ] && break
  sleep 2
done
[ "$health" = healthy ] || fail "the health check is $health"
echo "ok: the health check is healthy"

docker rm -f "$name" >/dev/null
start
echo "ok: the container was removed and started anew on the same volume"

again="$(docker exec "$name" /usr/local/bin/versekit-orgserver --admin-password --state /data/state.json)"
[ "$again" = "$password" ] || fail "the admin password changed across the restart"
me "$access" | jq -e '.member.name == "Pilot"' >/dev/null || fail "/me with the old token after the restart"
renewed="$(c -sS -X POST --data-urlencode "grant_type=refresh_token" --data-urlencode "refresh_token=$refresh" "$api/token")"
fresh="$(jq -r .access_token <<<"$renewed")"
[ -n "$fresh" ] && [ "$fresh" != null ] || fail "refresh after the restart"
me "$fresh" | jq -e '.member.name == "Pilot"' >/dev/null || fail "/me with the renewed token"
echo "ok: after the restart the token, the refresh token and the password are still there"
echo "container check passed"
