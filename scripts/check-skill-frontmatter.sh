#!/usr/bin/env bash
# skills/kemi/SKILL.md の frontmatter が ASCII だけで、項目がちょうど 3 つであることを確かめる。
set -euo pipefail

fm=$(awk 'NR>1 && /^---$/{exit} NR>1' skills/kemi/SKILL.md)
if printf '%s\n' "$fm" | grep -nP '[^\x00-\x7F]'; then
  echo "frontmatter must be ASCII only" >&2
  exit 1
fi
keys=$(printf '%s\n' "$fm" | grep -cE '^[a-z][a-z0-9_-]*:') || true
if [ "$keys" != 3 ]; then
  echo "frontmatter must have exactly 3 keys, found $keys" >&2
  exit 1
fi
