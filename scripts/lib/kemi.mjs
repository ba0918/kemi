// ブラウザ検査が起動する kemi と、その API。

import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { join } from 'node:path';

import { onExit, onSignal } from './exit.mjs';
import { withTimeout } from './wait.mjs';

/** kemi が起動の行を出し終えるまで待つ上限。 */
const START_TIMEOUT = 30000;
/** 止める合図の後、終わるまで待つ上限。過ぎたら SIGKILL で止める。 */
const STOP_TIMEOUT = 10000;
/**
 * 検査が合図で止められたとき、kemi に止める合図を送ってから終わるのを待つ上限。過ぎたものは 'exit' の段で
 * SIGKILL で止める。人が Ctrl+C を押してから長く待たせないよう、ふだんの止め方より短くする。exit.mjs が合図の後に
 * 待つ上限（SIGNAL_HOOKS_TIMEOUT）より短くする（長いと、止まるのを待ち切る前に打ち切られる）。
 */
const SIGNAL_STOP_TIMEOUT = 3000;

/**
 * 起動した kemi（レビューとサブコマンド）のうち、まだ終わっていないものと、それぞれが終わると解決する Promise。
 * 検査がどこで落ちても、プロセスが終わるときに止める（検査の後に kemi が残り続けていたことがある）。
 * @type {Map<import('node:child_process').ChildProcess, Promise<void>>}
 */
const running = new Map();
onExit('processes', () => {
  for (const child of running.keys()) {
    console.error(`stopping kemi (pid ${child.pid}) left running`);
    child.kill('SIGKILL');
  }
});
// 合図で止められたときは、すぐに SIGKILL せず、まず止める合図を送る。端末の Ctrl+C を受けた kemi は、
// セッションの保留（状態の保存と resume の行）を始めていて、その途中で殺すと保存しかけの状態が残る。
// 送るのは SIGINT（kemi は SIGINT と SIGTERM を同じ保留として受ける）。受けた合図をそのまま送らないのは、
// kemi が SIGHUP を受けず、保留せずにすぐ終わるため。
onSignal(async () => {
  const ended = [...running].map(([child, exited]) => {
    child.kill('SIGINT');
    return exited;
  });
  await withTimeout(Promise.all(ended), SIGNAL_STOP_TIMEOUT, () => 'kemi did not stop').catch(() => {
    console.error(`kemi did not stop within ${SIGNAL_STOP_TIMEOUT} ms after SIGINT`);
  });
});

/**
 * kemi を起動し、終わるまで running に入れておく。
 * @param {string} binary
 * @param {string[]} args
 * @param {import('node:child_process').SpawnOptions} options
 */
function spawnKemi(binary, args, options) {
  const child = spawn(binary, args, options);
  running.set(
    child,
    new Promise((done) => {
      const ended = () => {
        running.delete(child);
        done();
      };
      child.on('exit', ended);
      child.on('error', ended);
    }),
  );
  return child;
}

/**
 * 状態の置き場所を state の下に向けた環境変数。利用者の本物のセッションと結果の置き場所に書かないよう、起動ごとに
 * 一時ディレクトリを渡す（XDG_STATE_HOME を検査全体にかけると、フィクスチャを作る git commit で利用者のフックが
 * 読む場所まで変わる）。
 * @param {string} state
 */
export function isolatedEnvironment(state) {
  return { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home'), LOCALAPPDATA: join(state, 'localappdata') };
}

/**
 * @typedef {{ code: number | null, signal: NodeJS.Signals | null, stdout: string, stderr: string }} Exit
 * @typedef {{
 *   child: import('node:child_process').ChildProcess,
 *   url: string,
 *   id: string | undefined,
 *   live: string | undefined,
 *   exited: Promise<Exit>,
 *   stdout: () => string,
 *   stderr: () => string,
 * }} Kemi
 */

/**
 * kemi のレビューを起動し、起動の行（`kemi: <URL>`、求めたときは `kemi: review <id>` と `kemi: live <URL>`）が
 * そろうのを待つ。期限までにそろわなければ止めて失敗にする。
 * @param {string} binary
 * @param {{ cwd: string, args: string[], state?: string, env?: NodeJS.ProcessEnv, review?: boolean, live?: boolean }} options
 *   env を省くと isolatedEnvironment(state)
 * @returns {Promise<Kemi>}
 */
export async function launchKemi(binary, { cwd, args, state, env, review = false, live = false }) {
  const child = spawnKemi(binary, [...args, '--port', '0', '--no-open'], {
    cwd,
    env: env ?? isolatedEnvironment(/** @type {string} */ (state)),
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  // 文字列で受ける。塊のまま文字列につなぐと、塊の境目で割れた多バイト文字が壊れる。
  child.stdout.setEncoding('utf8');
  child.stderr.setEncoding('utf8');
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  // 'close' は標準出力を読み終えてから来る（'exit' の時点では submit の JSON がまだ途中のことがある）。
  const exited = new Promise((done) => {
    child.on('close', (code, signal) => done({ code, signal, stdout, stderr }));
    // 起動できなかったときは 'close' が来ないことがある。
    child.on('error', () => done({ code: null, signal: null, stdout, stderr }));
  });
  const started = new Promise((done, fail) => {
    child.stderr.on('data', () => {
      // 行が途中で切れて届くことがあるので、改行まで届いた行だけを読む。
      const url = stderr.match(/^kemi: (http:\/\/\S+)\r?\n/m);
      const id = stderr.match(/^kemi: review (\S+)\r?\n/m);
      const relay = stderr.match(/^kemi: live (\S+)\r?\n/m);
      if (url && (id || !review) && (relay || !live)) {
        done({ url: url[1], id: id?.[1], live: relay?.[1] });
      }
    });
    child.on('error', fail);
    child.on('exit', (code) => fail(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const kemi = { child, exited, stdout: () => stdout, stderr: () => stderr };
  try {
    const lines = await withTimeout(started, START_TIMEOUT, () => `kemi did not start within ${START_TIMEOUT} ms: ${stderr}`);
    return { ...kemi, ...lines };
  } catch (error) {
    await stopKemi(kemi, 'SIGKILL');
    throw error;
  }
}

/**
 * kemi を合図で止め、終わるのを待つ。もう終わっていればその結果を返す。期限を過ぎたら SIGKILL で止める。
 * @param {{ child: import('node:child_process').ChildProcess, exited: Promise<Exit> }} kemi
 * @param {NodeJS.Signals} [signal]
 * @returns {Promise<Exit>}
 */
export async function stopKemi(kemi, signal = 'SIGINT') {
  if (kemi.child.exitCode === null && kemi.child.signalCode === null) {
    kemi.child.kill(signal);
  }
  try {
    return await withTimeout(kemi.exited, STOP_TIMEOUT, () => 'kemi did not stop');
  } catch {
    console.error(`kemi (pid ${kemi.child.pid}) did not stop within ${STOP_TIMEOUT} ms after ${signal}; killing it`);
    kemi.child.kill('SIGKILL');
    return kemi.exited;
  }
}

/**
 * kemi のサブコマンド（`kemi wait`・`kemi reply` など）を走らせ、終わるのを待つ。
 * @param {string} binary
 * @param {string[]} args
 * @param {{ cwd: string, env: NodeJS.ProcessEnv, input?: string }} options
 * @returns {Promise<{ code: number | null, stdout: string, stderr: string }>}
 */
export function runKemi(binary, args, { cwd, env, input = '' }) {
  return new Promise((resolve, reject) => {
    const child = spawnKemi(binary, args, { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    // 文字列で受ける（launchKemi と同じ理由）。
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', reject);
    child.on('close', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input);
  });
}

/**
 * kemi のサブコマンドを走らせ、終了コードが 0 であることを確かめて stdout を返す。
 * @param {string} binary
 * @param {string[]} args
 * @param {{ cwd: string, env: NodeJS.ProcessEnv, input?: string }} options
 */
export async function kemiOutput(binary, args, options) {
  const { code, stdout, stderr } = await runKemi(binary, args, options);
  assert.equal(code, 0, `kemi ${args.join(' ')} exited with ${code}: ${stderr}`);
  return stdout;
}

/**
 * ページと同じオリジンから API に POST し、200 であることを確かめて JSON を返す。
 * @param {string} url レビューの URL
 * @param {string} path
 * @param {unknown} body
 */
export async function postJson(url, path, body) {
  const response = await fetch(new URL(path, url), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: new URL(url).origin },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, 200, `${path}: ${text}`);
  return JSON.parse(text);
}

/**
 * API を GET し、200 であることを確かめて JSON を返す。
 * @param {string} url レビューの URL
 * @param {string} path
 */
export async function getJson(url, path) {
  const response = await fetch(new URL(path, url));
  assert.equal(response.status, 200, path);
  return response.json();
}
