#!/usr/bin/env bash
# skills/kemi を skills-ref の公式バリデータで検査する。バリデータと依存はハッシュで固定した
# scripts/skills-ref-requirements.txt から、使い捨ての仮想環境に入れる。
set -euo pipefail

venv=$(mktemp -d)
trap 'rm -rf "$venv"' EXIT
uv venv --quiet "$venv"
uv pip install --quiet --python "$venv" --require-hashes -r scripts/skills-ref-requirements.txt
"$venv/bin/agentskills" validate ./skills/kemi
