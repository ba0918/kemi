#!/usr/bin/env bash
# 起動から api/review の初回応答までの時間を計測し、3 回の中央値を出す（R-VERIFY）。
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "usage: measure-startup.sh <fixture> <kemi-bin>" >&2
  exit 2
fi

fixture=$1
binary=$(realpath "$2")
shift 2

cd "$fixture"

times=()
for run in 1 2 3; do
  stderr_file=$(mktemp)
  start=$(date +%s%3N)
  "$binary" --worktree --port 0 --no-open >/dev/null 2>"$stderr_file" &
  pid=$!
  url=""
  while [ -z "$url" ]; do
    url=$(grep -o 'http://[^ ]*' "$stderr_file" | head -1 || true)
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "kemi exited before serving:" >&2
      cat "$stderr_file" >&2
      exit 1
    fi
    sleep 0.01
  done
  curl -s -o /dev/null "${url}api/review"
  end=$(date +%s%3N)
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -f "$stderr_file"
  ms=$((end - start))
  times+=("$ms")
  echo "run $run: ${ms} ms" >&2
done

median=$(printf '%s\n' "${times[@]}" | sort -n | sed -n 2p)
echo "$median"
