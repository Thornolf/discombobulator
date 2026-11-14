#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
found=$(git ls-files '*.rs' | xargs grep -nE '^\s*(//|/\*)|[;{})]\s*//' | grep -vE '^[^:]+:[0-9]+:\s*#!' || true)
if [ -n "$found" ]; then
  echo "Comments are not allowed in code:"
  echo "$found"
  exit 1
fi
