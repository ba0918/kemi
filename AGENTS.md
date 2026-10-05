# Agent Instructions

## Core

- 目的に仕え、依頼された範囲を不必要に広げない。
- 確認済みの事実と、推測・未確認を区別する。
- 変更したら、その変更に適した方法で実際に検証する。
- 不可逆・破壊的・外部から見える操作は、承認なしに行わない。
- プロジェクト固有の指示がある場合は、それを適用する。

## Rule Routing

| When | Read |
|---|---|
| Always | ba0918-design, ba0918-placement, ba0918-readability, ba0918-secrets |
| ci | ba0918-ci |
| commit | ba0918-commit |
| delegate | ba0918-delegation |
| design | ba0918-reuse |
| diff-review | ba0918-diff-review |
| document | ba0918-documents |
| gui | ba0918-gui-structure |
| implement | ba0918-tdd |
| mutation | ba0918-mutation-testing |
| release | ba0918-release |
| review | ba0918-verification |
| rust | ba0918-rust |
| worktree | ba0918-worktree |

ルールはスキル名で参照する。該当する作業を始める前に、対応するルールをすべて読む。
一度読んだルールは、そのコンテキストの間は有効とする。読み直すのは、コンテキストが圧縮・
クリアされた後か、ルール自体が変わったときだけ。委譲された作業では、委譲プロンプトが
「埋め込み済み」と明示したルールはそのプロンプトから有効とし、読み直さない。それ以外の
ルールは、この表に従って通常どおり読む。

## Project Context

このリポジトリ固有の文脈（何のリポジトリか、ビルドとテストの方法、ここだけの規約）は
`PROJECT.md` にある。変更する前に読む。
