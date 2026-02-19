#!/usr/bin/env bash
# spawn-crab.sh — build and launch a crab agent
#
# Usage:
#   ./scripts/spawn-crab.sh <colony_id> <crab_name> <role> [repo_path]
#
# Examples:
#   ./scripts/spawn-crab.sh abc-123 Alice coder
#   ./scripts/spawn-crab.sh abc-123 Bob reviewer /path/to/repo

set -euo pipefail

COLONY_ID="${1:?Usage: spawn-crab.sh <colony_id> <crab_name> <role> [repo_path]}"
CRAB_NAME="${2:?missing crab_name}"
ROLE="${3:?missing role}"
REPO="${4:-.}"
CONTROL_PLANE="${CONTROL_PLANE:-http://127.0.0.1:8800}"

echo "==> building crabitat-crab"
cargo build -p crabitat-crab

echo "==> spawning crab: name=${CRAB_NAME} role=${ROLE} colony=${COLONY_ID}"
exec cargo run -p crabitat-crab -- connect \
  --control-plane "${CONTROL_PLANE}" \
  --colony-id "${COLONY_ID}" \
  --name "${CRAB_NAME}" \
  --role "${ROLE}" \
  --repo "${REPO}"
