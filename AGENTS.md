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
| commit | ba0918-commit |
| delegate | ba0918-delegation |
| design | ba0918-reuse |
| diff-review | ba0918-diff-review |
| implement | ba0918-tdd |
| release | ba0918-release |
| review | ba0918-verification |

ルールはスキル名で参照する。該当する作業を始める前に、対応するルールをすべて読む。

## Project Context

このリポジトリ固有の文脈（何のリポジトリか、ビルドとテストの方法、ここだけの規約）は
`PROJECT.md` にある。変更する前に読む。
