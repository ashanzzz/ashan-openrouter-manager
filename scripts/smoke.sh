#!/usr/bin/env sh
set -eu
BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"
echo "Checking ${BASE_URL}/api/status"
curl -fsS "${BASE_URL}/api/status"
echo
echo "Smoke test passed"
