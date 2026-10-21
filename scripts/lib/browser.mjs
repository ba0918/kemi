// ブラウザ検査の agent-browser の呼び出しと、ページの中で読む式。

import assert from 'node:assert/strict';
import { execFile, spawnSync } from 'node:child_process';
import { promisify } from 'node:util';

import { onExit } from './exit.mjs';

const run = promisify(execFile);

/** 使ってから閉じていないセッション。検査がどこで落ちても、プロセスが終わるときに閉じる（ブラウザも止まる）。 */
const opened = new Set();
onExit('browser', () => {
  for (const session of opened) {
    spawnSync('agent-browser', ['--session', session, 'close'], { stdio: 'ignore', timeout: 15000 });
  }
});

/**
 * agent-browser のセッション 1 つの呼び出し。
 * @param {string} session
 * @param {{ timeout?: number, describe?: string }} [options] timeout は待ちの既定の期限（ms。省くと agent-browser の
 *   既定）。describe は待ちが尽きたときに添える、ページの状態を文字列で返す式。
 */
export function browserSession(session, { timeout, describe } = {}) {
  /** agent-browser を 1 回呼び、data を返す。失敗したら、agent-browser の出力を添える。 */
  const browser = async (...args) => {
    opened.add(session);
    const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 16 * 1024 * 1024 })
      .catch((error) => { throw new Error(`agent-browser ${args.join(' ')}: ${error.stdout ?? ''}`, { cause: error }); });
    const result = JSON.parse(stdout);
    assert.equal(result.success, true, JSON.stringify(result));
    return result.data;
  };
  /** ページの中で式を評価する（引用符の扱いに困らないよう base64 で渡す）。 */
  const evaluate = async (code) => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
  /** 式が真になるのを待つ。 */
  const waitFor = async (code, waitTimeout = timeout) => {
    try {
      await browser('wait', ...(waitTimeout === undefined ? [] : ['--timeout', String(waitTimeout)]), '--fn', code);
    } catch (error) {
      if (describe === undefined) {
        throw error;
      }
      const snapshot = await evaluate(describe).catch(() => 'no snapshot');
      throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
    }
  };
  /** セッションを閉じる（ブラウザも止まる）。閉じられなくても検査は落とさない。 */
  const close = async () => {
    await run('agent-browser', ['--session', session, 'close']).catch(() => {});
    opened.delete(session);
  };
  return { browser, evaluate, waitFor, close };
}

/**
 * 要素の式（null や undefined になりうる）が見えているかを読む式。箱があり（display: none の祖先の中に無い）、
 * visibility が hidden でない（hidden を継いだものも見えない）。
 * @param {string} element
 */
export const isVisible = (element) => `(() => { const e = ${element}; return e != null && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden'; })()`;

/**
 * セレクタの最初の要素が見えているかを読む式（isVisible）。
 * @param {string} selector
 */
export const visible = (selector) => isVisible(`document.querySelector(${JSON.stringify(selector)})`);

/**
 * CDP でページ（http://127.0.0.1: の最初のもの）につなぐ。agent-browser に無い操作（タッチ、ホイールの量）に使う。
 * @param {(...args: string[]) => Promise<any>} browser
 * @returns {Promise<{ cdp: (method: string, params?: object) => Promise<any>, close: () => void }>} cdp はつないだページに送る
 */
export async function attachToPage(browser) {
  const { cdpUrl } = await browser('get', 'cdp-url');
  const socket = new WebSocket(cdpUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let sequence = 0;
  const send = (method, params = {}, sessionId = undefined) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const receive = (event) => {
      const response = JSON.parse(event.data);
      if (response.id !== id) return;
      socket.removeEventListener('message', receive);
      response.error ? reject(new Error(JSON.stringify(response.error))) : resolve(response.result);
    };
    socket.addEventListener('message', receive);
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });
  try {
    const target = (await send('Target.getTargets')).targetInfos.find((t) => t.type === 'page' && t.url.startsWith('http://127.0.0.1:'));
    const { sessionId } = await send('Target.attachToTarget', { targetId: target.targetId, flatten: true });
    return { cdp: (method, params = {}) => send(method, params, sessionId), close: () => socket.close() };
  } catch (error) {
    socket.close();
    throw error;
  }
}
