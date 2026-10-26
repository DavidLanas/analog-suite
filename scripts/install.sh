#!/usr/bin/env bash
# Compila los plugins indicados (o todos) y los instala en ~/.clap.
# Uso: scripts/install.sh [funkcomp pultecmaster tapemaster]
set -euo pipefail

cd "$(dirname "$0")/.."

PLUGINS=("$@")
if [ ${#PLUGINS[@]} -eq 0 ]; then
    for dir in plugins/*/; do
        PLUGINS+=("$(basename "$dir")")
    done
fi

mkdir -p "$HOME/.clap"
for p in "${PLUGINS[@]}"; do
    echo ">> bundle $p"
    cargo xtask bundle "$p" --release
    cp -v "target/bundled/$p.clap" "$HOME/.clap/"
done
echo "Instalado en $HOME/.clap"
