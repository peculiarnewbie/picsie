#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
exec bash crates/picsie-desktop/run.sh "$@"
