#!/usr/bin/env bash
# 決定的な git リポジトリのフィクスチャを作る（R-VERIFY）。
# 同じ引数なら同じ内容（コミット日時・作者・内容を固定）になる。
set -euo pipefail

usage() {
  echo "usage: gen-fixture.sh <dir> --files N --lines M [--commits K]" >&2
  exit 2
}

dir=""
files=""
lines=""
commits=1
while [ $# -gt 0 ]; do
  case "$1" in
    --files)
      files=$2
      shift 2
      ;;
    --lines)
      lines=$2
      shift 2
      ;;
    --commits)
      commits=$2
      shift 2
      ;;
    -*)
      usage
      ;;
    *)
      dir=$1
      shift
      ;;
  esac
done
if [ -z "$dir" ] || [ -z "$files" ] || [ -z "$lines" ]; then
  usage
fi

rm -rf "$dir"
mkdir -p "$dir/src"
cd "$dir"
git init -q
git config user.email fixture@example.com
git config user.name fixture
git config core.hooksPath /dev/null
git config gc.auto 0
rm -f .git/gc.log
export GIT_AUTHOR_DATE="2026-01-01T00:00:00+00:00"
export GIT_COMMITTER_DATE="2026-01-01T00:00:00+00:00"

# 100 個のディレクトリを先に作り、ファイル数が多くても起動を抑える。
index=0
while [ "$index" -lt 100 ]; do
  mkdir -p "src/dir$index"
  index=$((index + 1))
done

# 決定的な内容で N ファイル作る。
index=0
while [ "$index" -lt "$files" ]; do
  sub=$((index % 100))
  path="src/dir$sub/file$index.txt"
  {
    line=1
    while [ "$line" -le "$lines" ]; do
      printf 'file %s line %s\n' "$index" "$line"
      line=$((line + 1))
    done
  } > "$path"
  index=$((index + 1))
done
git add -A
git commit -q -m "base"

# K コミット分の決定的な変更を積む。
commit=2
while [ "$commit" -le "$commits" ]; do
  index=0
  while [ "$index" -lt "$files" ]; do
    sub=$((index % 100))
    printf 'file %s commit %s\n' "$index" "$commit" >> "src/dir$sub/file$index.txt"
    index=$((index + 1))
  done
  git add -A
  git commit -q -m "commit $commit"
  commit=$((commit + 1))
done

# 未コミットの変更を残し、worktree モードでも全ファイルが見えるようにする。
index=0
while [ "$index" -lt "$files" ]; do
  sub=$((index % 100))
  printf 'file %s worktree change\n' "$index" >> "src/dir$sub/file$index.txt"
  index=$((index + 1))
done

tree=$(git rev-parse "HEAD^{tree}")
echo "fixture ready: $dir files=$files lines=$lines commits=$commits tree=$tree"
