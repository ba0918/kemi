// エージェントとの往復（agent-channel.md）のブラウザ自動化。実際の kemi バイナリを worktree
// モードで起動し、`kemi wait` と `kemi reply` を別プロセスで呼びながら、画面を agent-browser で
// 確かめる。
//
//   node scripts/test-agent-channel.mjs <kemi-bin>
//
// 確かめること: kemi wait を呼ぶ前から返信の欄・解決・会話パネルの書く欄と未接続の状態が出て、
// 「Hand to agent」だけが無い、会話パネルの開閉と幅が読み込み直しても残る、ページの起動中に
// kemi wait と kemi reply の発言が来ても待機中と渡すと発言が出る、kemi wait を待たせると待機中、
// 返った後は作業中に変わり渡すが出る、kemi reply の返信がスレッドに出てスクロール位置が変わらない、
// kemi reply の発言が会話パネルに出る、一覧の項目からスレッドを開いて返信を書くとスレッドに出る、
// 幅 390px でも会話パネルのシートを開いて閉じられる、未渡しを残して submit を押すと確認に件数が
// 出て、submit の JSON にそのコメントが入る。
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
      conversation: document.querySelector('#conversation')?.dataset.open,
      thread: !document.querySelector('#cv-thread')?.hidden,
      replies: document.querySelectorAll('.reply').length,
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
const panelOpen = `document.querySelector('#conversation').dataset.open === 'true'`;
const panelClosed = `document.querySelector('#conversation').dataset.open === 'false'`;
const agentMessage = (text) => `Array.from(document.querySelectorAll('#cv-items .cv-msg[data-author="agent"]')).some(m => m.textContent.includes(${JSON.stringify(text)}))`;

const fixture = await makeFixture();
const state = await mkdtemp(join(tmpdir(), 'kemi-agent-state-'));
const kemi = await startKemi(fixture, state);
try {
  const review = await (await fetch(new URL('api/review', kemi.url))).json();
  const fileId = review.groups[0].files[0].id;
  // 3 行目と 100 行目にコメントを付けておく。どちらも渡さないまま submit まで残す。
  await post(kemi.url, 'api/comment', { op: 'add', file_id: fileId, side: 'new', start_line: 3, end_line: 3, body: 'rename this line' });
  await post(kemi.url, 'api/comment', { op: 'add', file_id: fileId, side: 'new', start_line: 100, end_line: 100, body: 'and this one' });

  // (1b) で使う、起動の api/review の応答を止める仕掛け。sessionStorage に印があるときの
  // 読み込みでだけ、最初の 1 回を、サーバが返した後にページへ渡すのを止める。
  const holdScript = join(state, 'hold-review.js');
  await writeFile(holdScript, `(() => {
    if (sessionStorage.getItem('kemi-test-hold-review') !== '1') return;
    sessionStorage.removeItem('kemi-test-hold-review');
    const original = window.fetch.bind(window);
    let held = false;
    window.fetch = async (input, init) => {
      const response = await original(input, init);
      if (!held && String(input).startsWith('api/review')) {
        held = true;
        await new Promise((release) => { window.__kemiRelease = release; window.__kemiHeld = true; });
      }
      return response;
    };
  })();`);
  await browser('--init-script', holdScript, 'set', 'viewport', '1280', '800');
  await browser('open', kemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);

  // (1) kemi wait を呼ぶ前から、返信の欄・解決・会話パネルの書く欄と状態（未接続）は出る。
  // 「Hand to agent」だけが無い。会話パネルは畳んだ帯で始まり、上部の入口で開く。
  assert.equal(await evaluate(panelClosed), true);
  await browser('click', '#btn-comments');
  await waitFor(panelOpen);
  assert.equal(await evaluate(shown('#agent-status')), true);
  assert.equal(await evaluate(statusIs('unconnected')), true);
  assert.equal(await evaluate(shown('#btn-hand')), false);
  await evaluate(`Array.from(document.querySelectorAll('#diff-content .cchip')).find(c => c.textContent.includes('rename this line')).click(); true`);
  await waitFor(`document.querySelector('#diff-content .bal .reply-box') !== null`);
  assert.equal(await evaluate(shown('#diff-content .bal [data-focus-key="resolve:c1"]')), true);
  assert.equal(await evaluate(shown('#cv-message')), true);
  console.log('PASS kemi wait を呼ぶ前から返信・解決・会話パネルと未接続の状態が出て、「Hand to agent」だけが無い');

  // (1a) 会話パネルの幅は左の縁を掴んで変えられ、開閉と幅は読み込み直しても残る。
  const widthBefore = await evaluate(`document.querySelector('#conversation').getBoundingClientRect().width`);
  const edge = await evaluate(`JSON.stringify(document.querySelector('#cv-resizer').getBoundingClientRect())`).then(JSON.parse);
  const edgeX = Math.round(edge.left + edge.width / 2);
  const edgeY = Math.round(edge.top + edge.height / 2);
  await browser('mouse', 'move', String(edgeX), String(edgeY));
  await browser('mouse', 'down');
  await browser('mouse', 'move', String(edgeX - 60), String(edgeY));
  await browser('mouse', 'move', String(edgeX - 120), String(edgeY));
  await browser('mouse', 'up');
  await waitFor(`Math.abs(document.querySelector('#conversation').getBoundingClientRect().width - ${widthBefore + 120}) <= 2`);
  await browser('reload');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${panelOpen}`);
  assert.ok(Math.abs(await evaluate(`document.querySelector('#conversation').getBoundingClientRect().width`) - (widthBefore + 120)) <= 2, 'the width should be kept');
  await browser('click', '#cv-close');
  await waitFor(panelClosed);
  await browser('reload');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  assert.equal(await evaluate(panelClosed), true, 'the folded panel should stay folded');
  await browser('click', '#btn-comments');
  await waitFor(panelOpen);
  console.log('PASS 会話パネルの開閉と幅は読み込み直しても残る');

  // (1b) ページが起動のために読んだ中身より後、通知につながるより前に kemi wait が呼ばれ、
  // kemi reply で発言されても、待機中と「Hand to agent」とその発言が出る。起動の api/review の応答を、サーバが返した後にページへ
  // 渡すのを止めておき、その間に kemi wait を待たせてから渡す。
  await evaluate(`sessionStorage.setItem('kemi-test-hold-review', '1'); true`);
  await browser('reload');
  await waitFor(`window.__kemiHeld === true`);
  const loadingWait = agentCommand(fixture, state, ['wait', kemi.id, '--timeout', '10']);
  for (;;) {
    const current = await (await fetch(new URL('api/review', kemi.url))).json();
    if (current.agent?.status === 'waiting') break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  // 状態はサーバが時々知らせ直すことがあるが、発言は一度しか届かない。
  const early = await agentCommand(fixture, state, ['reply', kemi.id], JSON.stringify({
    writes: [{ type: 'message', body: 'Looking at it now.' }],
  }));
  assert.equal(early.code, 0, early.stderr);
  await evaluate(`window.__kemiRelease(); true`);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await waitFor(statusIs('waiting'));
  assert.equal(await evaluate(shown('#btn-hand')), true);
  await waitFor(`${panelOpen} && ${agentMessage('Looking at it now.')}`);
  const loadingWaited = await loadingWait;
  assert.equal(loadingWaited.code, 3, `the wait should time out: ${JSON.stringify(loadingWaited)}`);
  await waitFor(statusIs('working'));
  // 読み込み直しで閉じた、3 行目のコメントを開き直す（(3) が使う）。
  await evaluate(`Array.from(document.querySelectorAll('#diff-content .cchip')).find(c => c.textContent.includes('rename this line')).click(); true`);
  await waitFor(`document.querySelector('#diff-content .bal .reply-box') !== null`);
  console.log('PASS ページの起動中に kemi wait と kemi reply の発言が来ても、待機中と「Hand to agent」と発言が出る');

  // (2) kemi wait を待たせると待機中になり、渡すが出る。返った後は作業中。
  const waiting = agentCommand(fixture, state, ['wait', kemi.id, '--timeout', '2']);
  await waitFor(statusIs('waiting'));
  assert.equal(await evaluate(shown('#btn-hand')), true);
  const waited = await waiting;
  assert.equal(waited.code, 3, `the wait should time out: ${JSON.stringify(waited)}`);
  await waitFor(statusIs('working'));
  assert.equal(await evaluate(shown('#btn-hand')), true);
  console.log('PASS kemi wait を待たせると待機中、返った後は作業中に変わり、渡すが出る');

  // (3) kemi reply の返信がスレッドに出て、スクロール位置が変わらない（コメントは (1) で開いた）。
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
  const { ids } = JSON.parse(replied.stdout);
  assert.equal(ids.length, 2);
  await waitFor(`Array.from(document.querySelectorAll('#diff-content .reply[data-author="agent"]')).some(r => r.textContent.includes('Renamed it.'))`);
  const after = await evaluate(`document.querySelector('#diff-viewport').scrollTop`);
  assert.equal(after, before, 'the scroll position must not move');
  console.log('PASS kemi reply の返信がスレッドに出て、スクロール位置が変わらない');

  // (4) kemi reply の発言が会話パネルに出る。
  await waitFor(agentMessage('Both comments are addressed.'));
  console.log('PASS kemi reply の発言が会話パネルに出る');

  // (4a) 一覧の項目からスレッドを開くとパネル全体がそのスレッドになり、返信を書くとスレッドに出る。
  await evaluate(`document.querySelector('#cv-items .cv-card[data-id="c1"]').click(); true`);
  await waitFor(`!document.querySelector('#cv-thread').hidden && document.querySelector('#cv-list').hidden && document.querySelector('#cv-thread-body .cv-comment').textContent.includes('rename this line')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('#cv-thread-body .cv-post[data-author="agent"]')).some(p => p.textContent.includes('Renamed it.'))`), true);
  await browser('fill', '#cv-reply-text', 'Thanks, that works.');
  await browser('click', '#cv-reply button[type="submit"]');
  await waitFor(`Array.from(document.querySelectorAll('#cv-thread-body .cv-post[data-author="reviewer"]')).some(p => p.textContent.includes('Thanks, that works.')) && document.querySelector('#cv-reply-text').value === ''`);
  await browser('click', '#cv-thread-head .cv-back');
  await waitFor(`!document.querySelector('#cv-list').hidden`);
  console.log('PASS 一覧の項目からスレッドを開き、返信を書くとスレッドに出る');

  // (5) 幅 390px でも、会話パネルを画面いっぱいのシートで開いて閉じられる。
  await browser('set', 'viewport', '390', '844');
  await waitFor(`getComputedStyle(document.querySelector('#tree')).position === 'fixed' && ${panelClosed}`);
  await browser('click', '#btn-comments');
  await waitFor(panelOpen);
  const sheet = await evaluate(`JSON.stringify(document.querySelector('#conversation').getBoundingClientRect())`).then(JSON.parse);
  assert.deepEqual([sheet.left, sheet.top, sheet.width, sheet.height].map(Math.round), [0, 0, 390, 844]);
  await browser('click', '#cv-close');
  await waitFor(panelClosed);
  await browser('set', 'viewport', '1280', '800');
  await waitFor(`getComputedStyle(document.querySelector('#tree')).position !== 'fixed' && ${panelOpen}`);
  console.log('PASS 幅 390px でも会話パネルのシートを開いて閉じられる');

  // (6) 未渡しを残して submit を押すと、確認に件数が出て、submit の JSON にそのコメントが入る。
  await browser('click', '#btn-approve');
  await waitFor(`!document.querySelector('#modal').hidden && document.querySelector('#unhanded-notice') !== null`);
  const notice = await evaluate(`document.querySelector('#unhanded-notice').textContent`);
  // 渡していないのは、画面で付けた 2 つのコメントと (4a) の返信。
  assert.match(notice, /\b3\b/);
  await browser('click', '#modal-ok');
  const { code, stdout } = await kemi.exited;
  assert.equal(code, 0);
  const result = JSON.parse(stdout);
  assert.deepEqual(result.comments.map((comment) => comment.body), ['rename this line', 'and this one']);
  assert.equal(result.comments[0].replies[0].author, 'agent');
  assert.deepEqual(result.messages.map((message) => message.body), ['Looking at it now.', 'Both comments are addressed.']);
  console.log('PASS 未渡しを残して submit を押すと確認に件数が出て、submit の JSON にそのコメントが入る');
} finally {
  kemi.child.kill('SIGTERM');
  await kemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
