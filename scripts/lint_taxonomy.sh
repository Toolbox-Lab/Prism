#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo "$(dirname "$0")/..")"

DATA_DIR="crates/core/src/taxonomy/data"
ENHANCED_FILE="taxonomy/enhanced_error_taxonomy.toml"

echo "🔍 Linting taxonomy TOML files in ${DATA_DIR} and ${ENHANCED_FILE} ..."
cargo run --quiet -p grat-core --bin taxonomy_linter -- "${DATA_DIR}" "${ENHANCED_FILE}"
