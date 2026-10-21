// ブラウザ検査のフィクスチャを作る道具。

import { execFile } from 'node:child_process';
import { rmSync } from 'node:fs';
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

import { onExit } from './exit.mjs';

const run = promisify(execFile);

/** temporaryDirectory で作ったディレクトリ。 */
const created = [];
onExit('files', (code) => {
  if (created.length === 0) {
    return;
  }
  if (code !== 0) {
    console.error(`kept the temporary directories for inspection:\n${created.map((dir) => `  ${dir}`).join('\n')}`);
    return;
  }
  for (const dir of created) {
    try {
      rmSync(dir, { recursive: true, force: true, maxRetries: 3 });
    } catch (error) {
      console.error(`could not remove ${dir}: ${error}`);
    }
  }
});

/**
 * フィクスチャや状態の置き場所にする一時ディレクトリを作る。検査が通って終わったら消し、落ちたら調べられるよう
 * 場所を出して残す。人が見る画像の置き場所（出力に場所を出すもの）には使わない。
 * @param {string} prefix
 */
export async function temporaryDirectory(prefix) {
  const dir = await mkdtemp(join(tmpdir(), prefix));
  created.push(dir);
  return dir;
}

/**
 * 利用者の git の設定に左右されない git。
 * @param {string} dir
 */
export function gitIn(dir) {
  return (...args) => run('git', ['-C', dir, ...args], {
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: join(dir, '.git-test-global'),
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_AUTHOR_NAME: 'kemi',
      GIT_AUTHOR_EMAIL: 'kemi@example.com',
      GIT_COMMITTER_NAME: 'kemi',
      GIT_COMMITTER_EMAIL: 'kemi@example.com',
    },
  });
}
