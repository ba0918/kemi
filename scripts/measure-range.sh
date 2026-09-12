#!/usr/bin/env bash
# コミット範囲のグループ単位を作る時間を計測する（R-VERIFY, R-UNIT）。
#
#   commit / file: --group-by <commit|file> で起動し、api/review を取得するまでの
#                  時間の 3 回の中央値（ミリ秒）を出す。
#   busy:          最終形（--group-by file）で起動し、api/review の直後から、裏で
#                  コミットごとの単位を作っている間に最初のファイルの api/file を
#                  10 回取得し、最も遅い応答時間（ミリ秒）を出す。
#
# 範囲はフィクスチャの最初のコミットからの --from。
set -euo pipefail

if [ $# -lt 3 ]; then
  echo "usage: measure-range.sh <fixture> <kemi-bin> <commit|file|busy>" >&2
  exit 2
fi

fixture=$1
binary=$(realpath "$2")
mode=$3
case "$mode" in
  commit | file | busy) ;;
  *)
    echo "mode は commit / file / busy のどれかです: $mode" >&2
    exit 2
    ;;
esac

cd "$fixture"
first=$(git rev-list --max-parents=0 HEAD | tail -1)

# 壁時計（date）は時刻合わせで前後に飛ぶことがあり、計測値が負や数十秒になる。
# 単調増加の時計でミリ秒を取る。
now_ms() {
  perl -MTime::HiRes=clock_gettime,CLOCK_MONOTONIC \
    -e 'printf "%d\n", clock_gettime(CLOCK_MONOTONIC) * 1000'
}

stderr_file=""
pid=""
cleanup() {
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  if [ -n "$stderr_file" ]; then
    rm -f "$stderr_file"
  fi
}
trap cleanup EXIT

# kemi を起動し、stderr の `kemi: <url>` 行が出るまで待って URL を返す。
start_kemi() {
  local group_by=$1
  stderr_file=$(mktemp)
  "$binary" --from "$first" --group-by "$group_by" --port 0 --no-open \
    >/dev/null 2>"$stderr_file" &
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
}

stop_kemi() {
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  pid=""
  rm -f "$stderr_file"
  stderr_file=""
}

if [ "$mode" = busy ]; then
  start_kemi file
  review=$(curl -s "${url}api/review")
  # ファイルの項目は入れ子のオブジェクトを持たないので、最初の files 配列の
  # 最初の要素から id を取り出せる。
  file_id=$(printf '%s' "$review" | grep -o '"files":\[{[^{}]*' | head -1 \
    | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
  if [ -z "$file_id" ]; then
    echo "api/review にファイルがありません" >&2
    exit 1
  fi
  slowest=0
  for request in $(seq 1 10); do
    seconds=$(curl -s -o /dev/null -w '%{time_total}' "${url}api/file/${file_id}")
    ms=$(awk -v s="$seconds" 'BEGIN { printf "%d", s * 1000 + 0.5 }')
    echo "request $request: ${ms} ms" >&2
    if [ "$ms" -gt "$slowest" ]; then
      slowest=$ms
    fi
  done
  stop_kemi
  echo "$slowest"
  exit 0
fi

times=()
for run in 1 2 3; do
  start=$(now_ms)
  start_kemi "$mode"
  curl -s -o /dev/null "${url}api/review"
  end=$(now_ms)
  stop_kemi
  ms=$((end - start))
  times+=("$ms")
  echo "run $run: ${ms} ms" >&2
done

median=$(printf '%s\n' "${times[@]}" | sort -n | sed -n 2p)
echo "$median"
