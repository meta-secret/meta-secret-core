#!/usr/bin/env bash
# Connectivity checks for MetaSecret public API (HTTP /meta_request and related routes).
# WebSocket (wss://.../meta_ws) uses the same host; see server logs for "WebSocket /meta_ws connected".
#
# Usage:
#   ./scripts/diagnose-meta-api.sh
#   ./scripts/diagnose-meta-api.sh "https://api.meta-secret.org:443"
#   ./scripts/diagnose-meta-api.sh "http://127.0.0.1:3000"

set -euo pipefail

BASE="${1:-https://api.meta-secret.org:443}"
RAW="${BASE#*://}"
RAW="${RAW%%/*}"
HOSTONLY="${RAW%%:*}"

echo "=== DNS: ${HOSTONLY} ==="
python3 -c "import socket,sys
h=sys.argv[1]
try:
  _, _, ips = socket.gethostbyname_ex(h)
  print('IPv4:', ', '.join(ips))
except Exception as e:
  print('resolve error:', e)
  sys.exit(1)
" "${HOSTONLY}"

echo ""
echo "Note: Public api.meta-secret.org often resolves to Cloudflare anycast (104.x / 172.67.x)."
echo "      kubectl pod IPs are the origin cluster; they will not match edge IPs."
echo ""

echo "=== GET ${BASE%/}/hi (max 15s) ==="
curl -sS -f -o /tmp/meta-secret-diagnose-hi.txt -w "http=%{http_code} time_total_s=%{time_total} connect_s=%{time_connect} tls_handshake_s=%{time_appconnect}\n" \
  --max-time 15 "${BASE%/}/hi" || { echo "GET /hi failed"; exit 1; }
echo "body (first 80 bytes):"
head -c 80 /tmp/meta-secret-diagnose-hi.txt || true
echo ""
echo ""

echo "=== POST ${BASE%/}/meta_request invalid JSON (max 15s; expect 4xx) ==="
curl -sS -w "\nhttp=%{http_code} time_total_s=%{time_total}\n" --max-time 15 \
  -X POST "${BASE%/}/meta_request" \
  -H "Content-Type: application/json" \
  -d 'null' || true
echo ""

echo "=== Server (cluster): grep WebSocket + HTTP sync in one window ==="
echo "kubectl logs -f meta-secret-server-0 2>/dev/null | rg 'WebSocket /meta_ws|Event processing|meta_ws:' || \\"
echo "kubectl logs -f statefulset/meta-secret-server -c meta-secret-server 2>/dev/null | rg 'WebSocket /meta_ws|Event processing|meta_ws:' || \\"
echo "kubectl logs -f deployment/meta-secret-server 2>/dev/null | rg 'WebSocket /meta_ws|Event processing|meta_ws:'"
echo ""

echo "=== Mobile: Xcode / device console ==="
echo "Filter for: [meta_secret_ws]  (Rust eprintln on WS connect failures)"
echo ""
