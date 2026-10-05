#!/usr/bin/env bash
# ドメイン層が HTTP・非同期実行・プロセス起動に依存していないことを確かめる。
set -euo pipefail

if rg -n 'use (axum|hyper|tokio|std::process)' crates/kemi-core/src/domain; then
  echo "domain must not depend on infrastructure" >&2
  exit 1
fi
