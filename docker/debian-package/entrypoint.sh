#!/usr/bin/env bash

set -euo pipefail

if [[ ! -f "debian/control" ]]; then
  echo "[debian-package] run this container with the repository mounted at /work" >&2
  exit 1
fi

exec "$@"
