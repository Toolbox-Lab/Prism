#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo "$(dirname "$0")/..")"

DATA_DIR="crates/core/src/taxonomy/data"

echo "🔍 Linting taxonomy TOML files in ${DATA_DIR} ..."
cargo run --quiet -p grat-core --bin taxonomy_linter -- "${DATA_DIR}"
