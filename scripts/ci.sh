#!/usr/bin/env bash
# CI local: formato, clippy y tests de todo el workspace.
# Uso: scripts/ci.sh            (rápido)
#      scripts/ci.sh --validate (además empaqueta y pasa clap-validator)
set -euo pipefail

cd "$(dirname "$0")/.."

echo ">> cargo fmt --check"
cargo fmt --all -- --check

echo ">> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo ">> cargo test"
cargo test --workspace --release

if [ "${1:-}" = "--validate" ]; then
    if ! command -v clap-validator >/dev/null; then
        echo "clap-validator no está instalado: cargo install --git https://github.com/free-audio/clap-validator" >&2
        exit 1
    fi
    for dir in plugins/*/; do
        p="$(basename "$dir")"
        echo ">> bundle + validate $p"
        cargo xtask bundle "$p" --release
        clap-validator validate "target/bundled/$p.clap"
    done
fi

echo "CI OK"
