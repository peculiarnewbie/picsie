#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
node scripts/check-architecture.mjs
exec cargo run --locked --release --manifest-path crates/picsie-desktop/Cargo.toml -- "$@"
