#!/usr/bin/env bash
set -euo pipefail

CONTROL_PLANE="${CONTROL_PLANE_URL:-http://127.0.0.1:8800}"

usage() {
  cat <<EOF
Usage: $0 <colony_id> <crab_id> <name> <role> [state]

Register a crab in an existing colony via the control-plane API.

Arguments:
  colony_id   Colony to join (must already exist)
  crab_id     Unique crab identifier (e.g. crab-1)
  name        Display name (e.g. Alice)
  role        Role in the colony (e.g. coder, reviewer)
  state       Initial state: idle (default), busy, offline

Environment:
  CONTROL_PLANE_URL  Base URL (default: http://127.0.0.1:8800)

Examples:
  # Create a colony first
  curl -s -X POST \$CONTROL_PLANE_URL/v1/colonies \\
    -H 'Content-Type: application/json' \\
    -d '{"name":"my-project","description":"My first colony"}'

  # Then onboard crabs
  $0 <colony_id> crab-1 Alice coder
  $0 <colony_id> crab-2 Bob reviewer idle
EOF
  exit 1
}

if [[ $# -lt 4 ]]; then
  usage
fi

COLONY_ID="$1"
CRAB_ID="$2"
NAME="$3"
ROLE="$4"
STATE="${5:-idle}"

echo "Registering crab: ${CRAB_ID} (${NAME}, ${ROLE}, ${STATE})"
echo "Colony: ${COLONY_ID}"
echo "Control-plane: ${CONTROL_PLANE}"
echo ""

RESPONSE=$(curl -s -w "\n%{http_code}" \
  -X POST "${CONTROL_PLANE}/v1/crabs/register" \
  -H "Content-Type: application/json" \
  -d "{\"crab_id\":\"${CRAB_ID}\",\"colony_id\":\"${COLONY_ID}\",\"name\":\"${NAME}\",\"role\":\"${ROLE}\",\"state\":\"${STATE}\"}")

HTTP_CODE=$(echo "$RESPONSE" | tail -1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [[ "$HTTP_CODE" -ge 200 && "$HTTP_CODE" -lt 300 ]]; then
  echo "Registration successful (HTTP ${HTTP_CODE}):"
  echo "$BODY" | python3 -m json.tool 2>/dev/null || echo "$BODY"
else
  echo "Registration failed (HTTP ${HTTP_CODE}):"
  echo "$BODY" | python3 -m json.tool 2>/dev/null || echo "$BODY"
  exit 1
fi

echo ""
echo "Current status snapshot:"
curl -s "${CONTROL_PLANE}/v1/status" | python3 -m json.tool 2>/dev/null || curl -s "${CONTROL_PLANE}/v1/status"
