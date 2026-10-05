// エージェントとの往復（agent-channel.md）のブラウザ自動化。実際の kemi バイナリを worktree
// モードで起動し、`kemi wait` と `kemi reply` を別プロセスで呼びながら、画面を agent-browser で
// 確かめる。
//
//   node scripts/test-agent-channel.mjs <kemi-bin>
//
// 確かめること: kemi wait を呼ぶ前は「Hand to agent」が無く状態が未接続、kemi wait を待たせると
// 待機中、返った後は作業中に変わり渡すが出る、kemi reply の返信がスレッドに出てスクロール位置が
// 変わらない、kemi reply の発言がチャット欄に出る、幅 390px でチャット欄がシートで開く、未渡しを
// 残して submit を押すと確認に件数が出て、submit の JSON にそのコメントが入る。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

const run = promisify(execFile);
if (!process.argv[2]) {
  console.error('usage: node scripts/test-agent-channel.mjs <kemi-bin>');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const session = `kemi-agent-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 8 * 1024 * 1024 });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async (code) => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
/** 待ちが尽きたら、ページの状態を添えて失敗にする。 */
const waitFor = async (code) => {
  try {
    await browser('wait', '--fn', code);
  } catch (error) {
    const snapshot = await evaluate(`JSON.stringify({
      status: document.querySelector('#agent-status')?.dataset.status,
      dock: document.querySelector('#agent-dock')?.hidden,
      replies: document.querySelectorAll('.reply').length,
      chat: document.querySelector('#chat')?.hidden,
      modal: document.querySelector('#modal-body')?.textContent,
    })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

const LINES = (count) => Array.from({ length: count }, (_, i) => `line ${i + 1}\n`);

/** 1 ファイルの worktree の変更。3 行目・60 行目・100 行目を書き換える。 */
async function makeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-agent-'));
  const git = (...args) => run('git', ['-C', dir, ...args], {
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
  await git('init', '-q');
  const lines = LINES(120);
  await writeFile(join(dir, 'a.txt'), lines.join(''));
  await git('add', 'a.txt');
  await git('commit', '-q', '-m', 'base');
  for (const index of [2, 59, 99]) {
    lines[index] = `changed ${index + 1}\n`;
  }
  await writeFile(join(dir, 'a.txt'), lines.join(''));
  return dir;
}

function environment(state) {
  return { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home'), LOCALAPPDATA: join(state, 'localappdata') };
}

async function startKemi(dir, state) {
  const child = spawn(binary, ['--worktree', '--port', '0', '--no-open'], {
    cwd: dir,
    env: environment(state),
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const started = await new Promise((resolve, reject) => {
    child.stderr.on('data', () => {
      const url = stderr.match(/^kemi: (http:\/\/\S+)/m);
      const review = stderr.match(/^kemi: review (\S+)/m);
      if (url && review) resolve({ url: url[1], id: review[1] });
    });
    child.on('error', reject);
    child.on('exit', (code) => reject(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const exited = new Promise((resolve) => child.on('exit', (code) => resolve({ code, stdout })));
  return { child, ...started, exited };
}

/** `kemi wait` / `kemi reply` を走らせ、(終了コード, stdout, stderr) を返す。 */
function agentCommand(dir, state, args, input = '') {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { cwd: dir, env: environment(state), stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.on('error', reject);
    child.on('exit', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input);
  });
}

/** ページと同じオリジンから API を呼ぶ（画面を開く前のコメント）。 */
async function post(url, path, body) {
  const origin = new URL(url).origin;
  const response = await fetch(new URL(path, url), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: origin },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, 200, `${path}: ${text}`);
  return JSON.parse(text);
}

const shown = (selector) => `(() => { const e = document.querySelector(${JSON.stringify(selector)}); return e !== null && !e.closest('[hidden]') && getComputedStyle(e).display !== 'none'; })()`;
const statusIs = (status) => `document.querySelector('#agent-status').dataset.status === ${JSON.stringify(status)}`;

const fixture = await makeFixture();
const state = await mkdtemp(join(tmpdir(), 'kemi-agent-state-'));
const kemi = await startKemi(fixture, state);
try {
  const review = await (await fetch(new URL('api/review', kemi.url))).json();
  const fileId = review.groups[0].files[0].id;
  // 3 行目と 100 行目にコメントを付けておく。どちらも渡さないまま submit まで残す。
  await post(kemi.url, 'api/comment', { op: 'add', file_id: fileId, side: 'new', start_line: 3, end_line: 3, body: 'rename this line' });
  await post(kemi.url, 'api/comment', { op: 'add', file_id: fileId, side: 'new', start_line: 100, end_line: 100, body: 'and this one' });

  await browser('set', 'viewport', '1280', '800');
  await browser('open', kemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);

  // (1) kemi wait を呼ぶ前は、渡すも状態もチャット欄の入口も出ず、状態は未接続。
  assert.equal(await evaluate(statusIs('unconnected')), true);
  assert.equal(await evaluate(shown('#btn-hand')), false);
  assert.equal(await evaluate(shown('#btn-dock-chat')), false);
  assert.equal(await evaluate(`document.querySelectorAll('.reply-box, .thread').length`), 0);
  console.log('PASS kemi wait を呼ぶ前は「Hand to agent」が無く、状態が未接続');

  // (2) kemi wait を待たせると待機中になり、渡すが出る。返った後は作業中。
  const waiting = agentCommand(fixture, state, ['wait', kemi.id, '--timeout', '2']);
  await waitFor(statusIs('waiting'));
  assert.equal(await evaluate(shown('#btn-hand')), true);
  const waited = await waiting;
  assert.equal(waited.code, 3, `the wait should time out: ${JSON.stringify(waited)}`);
  await waitFor(statusIs('working'));
  assert.equal(await evaluate(shown('#btn-hand')), true);
  assert.equal(await evaluate(`document.querySelector('#hand-count').textContent.trim()`), '2');
  console.log('PASS kemi wait を待たせると待機中、返った後は作業中に変わり、渡すが出る');

  // (3) kemi reply の返信がスレッドに出て、スクロール位置が変わらない。
  await evaluate(`Array.from(document.querySelectorAll('#diff-content .cchip')).find(c => c.textContent.includes('rename this line')).click(); true`);
  await waitFor(`document.querySelector('#diff-content .bal .reply-box') !== null`);
  await evaluate(`document.querySelector('#diff-viewport').scrollTop = 30; true`);
  await waitFor(`document.querySelector('#diff-viewport').scrollTop === 30`);
  const before = await evaluate(`document.querySelector('#diff-viewport').scrollTop`);
  const replied = await agentCommand(fixture, state, ['reply', kemi.id], JSON.stringify({
    writes: [
      { type: 'reply', comment_id: 'c1', body: 'Renamed it.' },
      { type: 'message', body: 'Both comments are addressed.' },
    ],
  }));
  assert.equal(replied.code, 0, replied.stderr);
  assert.deepEqual(JSON.parse(replied.stdout), { ids: ['r1', 'm1'] });
  await waitFor(`Array.from(document.querySelectorAll('#diff-content .reply[data-author="agent"]')).some(r => r.textContent.includes('Renamed it.'))`);
  const after = await evaluate(`document.querySelector('#diff-viewport').scrollTop`);
  assert.equal(after, before, 'the scroll position must not move');
  console.log('PASS kemi reply の返信がスレッドに出て、スクロール位置が変わらない');

  // (4) kemi reply の発言がチャット欄に出る。
  await browser('click', '#btn-dock-chat');
  await waitFor(`Array.from(document.querySelectorAll('#chat .chat-item[data-author="agent"]')).some(m => m.textContent.includes('Both comments are addressed.'))`);
  await browser('click', '#chat .cl-close');
  await waitFor(`document.querySelector('#chat').hidden`);
  console.log('PASS kemi reply の発言がチャット欄に出る');

  // (5) 幅 390px では、チャット欄が下から出るシートで開く。
  await browser('set', 'viewport', '390', '844');
  await waitFor(`getComputedStyle(document.querySelector('#tree')).position === 'fixed'`);
  await browser('click', '#btn-chat');
  await waitFor(`!document.querySelector('#chat').hidden`);
  const sheet = await evaluate(`JSON.stringify(document.querySelector('#chat').getBoundingClientRect())`).then(JSON.parse);
  assert.equal(Math.round(sheet.left), 0);
  assert.equal(Math.round(sheet.width), 390);
  assert.equal(Math.round(sheet.bottom), 844);
  await browser('click', '#chat .cl-close');
  await waitFor(`document.querySelector('#chat').hidden`);
  await browser('set', 'viewport', '1280', '800');
  await waitFor(`getComputedStyle(document.querySelector('#tree')).position !== 'fixed'`);
  console.log('PASS 幅 390px でチャット欄がシートで開く');

  // (6) 未渡しを残して submit を押すと、確認に件数が出て、submit の JSON にそのコメントが入る。
  await browser('click', '#btn-approve');
  await waitFor(`!document.querySelector('#modal').hidden && document.querySelector('#unhanded-notice') !== null`);
  const notice = await evaluate(`document.querySelector('#unhanded-notice').textContent`);
  assert.match(notice, /\b2\b/);
  await browser('click', '#modal-ok');
  const { code, stdout } = await kemi.exited;
  assert.equal(code, 0);
  const result = JSON.parse(stdout);
  assert.deepEqual(result.comments.map((comment) => comment.body), ['rename this line', 'and this one']);
  assert.equal(result.comments[0].replies[0].author, 'agent');
  assert.equal(result.messages[0].body, 'Both comments are addressed.');
  console.log('PASS 未渡しを残して submit を押すと確認に件数が出て、submit の JSON にそのコメントが入る');
} finally {
  kemi.child.kill('SIGTERM');
  await kemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
