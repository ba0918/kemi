// `--live`（docs/spec/live.md、live-compare.md）のブラウザ自動化。実際の kemi バイナリを、
// scripts/live-dev-server.mjs の試験用の開発サーバを相手に起動し、画面を agent-browser で確かめる。
//
//   node scripts/test-live.mjs <kemi-bin>
//
// 確かめること:
// - 中継: HMR が中継越しに効く。中継用の cookie が document.cookie に出ない。開発サーバを止めた
//   まま新しく起動しても、止めたまま復元しても、つながらない旨が出て、開発サーバを起動すると
//   つながる。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

import { startDevServer } from './live-dev-server.mjs';

const run = promisify(execFile);
if (!process.argv[2]) {
  console.error('usage: node scripts/test-live.mjs <kemi-bin>');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const session = `kemi-live-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 16 * 1024 * 1024 })
    .catch((error) => { throw new Error(`agent-browser ${args.join(' ')}: ${error.stdout ?? ''}`, { cause: error }); });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async (code) => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
const waitFor = async (code, timeout = 15000) => {
  try {
    await browser('wait', '--timeout', String(timeout), '--fn', code);
  } catch (error) {
    const snapshot = await evaluate(`JSON.stringify({ url: location.href, text: document.body?.innerText.slice(0, 400) })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

/** 利用者の git の設定に左右されない git。 */
function gitIn(dir) {
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

/** 変更のある作業ツリー。 */
async function makeRepository() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-live-repo-'));
  const git = gitIn(dir);
  await git('init', '-q');
  await writeFile(join(dir, 'a.txt'), 'one\ntwo\n');
  await git('add', 'a.txt');
  await git('commit', '-q', '-m', 'base');
  await writeFile(join(dir, 'a.txt'), 'one\nTWO\n');
  return dir;
}

function environment(state) {
  return { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home'), LOCALAPPDATA: join(state, 'localappdata') };
}

/** 空いているポートを 1 つ選ぶ（開発サーバを後から同じポートで起動するため）。 */
async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', () => done(undefined)));
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((done) => server.close(() => done(undefined)));
  return port;
}

/** kemi を起動し、URL・review の id・中継の URL がそろうのを待つ。 */
async function startKemi(dir, state, args) {
  const child = spawn(binary, [...args, '--port', '0', '--no-open'], { cwd: dir, env: environment(state), stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = '';
  let stdout = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  const started = await new Promise((done, fail) => {
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
      const url = stderr.match(/^kemi: (http:\/\/\S+)/m);
      const review = stderr.match(/^kemi: review (\S+)/m);
      const live = stderr.match(/^kemi: live (\S+)/m);
      if (url && review && live) done({ url: url[1], id: review[1], live: live[1] });
    });
    child.on('error', fail);
    child.on('exit', (code) => fail(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const exited = new Promise((done) => child.on('exit', (code) => done({ code, stdout, stderr })));
  return { child, ...started, exited, stderr: () => stderr };
}

/** ページと同じオリジンから API を呼ぶ。 */
async function post(url, path, body) {
  const response = await fetch(new URL(path, url), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: new URL(url).origin },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, 200, `${path}: ${text}`);
  return JSON.parse(text);
}

async function stop(kemi) {
  kemi.child.kill('SIGINT');
  return kemi.exited;
}

/** 中継（R-PAGE-PROXY）: HMR が中継越しに効き、中継用の cookie がページから読めない。 */
async function relayCarriesHmrAndHidesTheCookie(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', dev.url]);
  try {
    await browser('open', kemi.url);
    await browser('open', kemi.live);
    await waitFor(`document.documentElement.dataset.hmrConnected === 'true'`);
    const box = `getComputedStyle(document.querySelector('.box')).backgroundColor`;
    assert.equal(await evaluate(box), 'rgb(255, 0, 0)');
    await writeFile(join(dev.dir, 'style.css'), '.box { background: rgb(0, 0, 255); width: 200px; height: 80px; }\n');
    await waitFor(`${box} === 'rgb(0, 0, 255)'`);
    assert.equal(await evaluate(`document.cookie.includes('kemi_live')`), false, 'the relay cookie must be HttpOnly');
    console.log('PASS 中継越しに HMR が効き、中継用の cookie がページの document.cookie に出ない');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 開発サーバにつながらないときは出して待ち、起動したらつながる（R-PAGE-MODE、R-PAGE-SESSION）。 */
async function waitsForTheDevServer(repository) {
  const port = await freePort();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const url = `http://127.0.0.1:${port}/`;
  const unreachable = `document.querySelector('#kemi-unreachable')?.textContent.includes('kemi cannot reach')`;
  const loaded = `document.querySelector('#title')?.textContent === 'Dev server'`;

  // 新しく起動したとき。
  const first = await startKemi(repository, state, ['--live', url]);
  let dev;
  try {
    await browser('open', first.url);
    await browser('open', first.live);
    await waitFor(unreachable);
    dev = await startDevServer({ port });
    await waitFor(loaded);
    console.log('PASS 開発サーバを止めたまま起動すると、つながらない旨が出て、起動するとつながる');
    await post(first.url, 'api/message', { body: 'keep this review' });
  } finally {
    await stop(first);
    await dev?.close();
  }

  // 止めたまま復元したとき。
  const resumed = await startKemi(repository, state, ['--resume', first.id]);
  try {
    await browser('open', resumed.url);
    await browser('open', resumed.live);
    await waitFor(unreachable);
    dev = await startDevServer({ port });
    await waitFor(loaded);
    console.log('PASS 開発サーバを止めたまま復元すると、つながらない旨が出て、起動するとつながる');
  } finally {
    await stop(resumed);
    await dev?.close();
  }
}

const repository = await makeRepository();
try {
  await relayCarriesHmrAndHidesTheCookie(repository);
  await waitsForTheDevServer(repository);
} finally {
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
