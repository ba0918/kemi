// エージェントとの往復（agent-channel.md）のブラウザ自動化。実際の kemi バイナリを worktree
// モードで起動し、`kemi wait` と `kemi reply` を別プロセスで呼びながら、画面を agent-browser で
// 確かめる。
//
//   node scripts/test-agent-channel.mjs <kemi-bin>
//
// 確かめること: kemi wait を呼ぶ前から返信の欄・解決・会話パネルの書く欄と未接続の状態が出て、
// 「Hand to agent」だけが無い、会話パネルの開閉と幅が読み込み直しても残る、ページの起動中に
// kemi wait と kemi reply の発言が来ても待機中と渡すと発言が出る、kemi wait を待たせると待機中、
// 返った後は作業中に変わり渡すが出る、畳んだまま返信が届くとパネルは開かず札と帯に新着が出て
// スレッドを開くと消える、一番下を見ているときだけ並びが新しいものについていき、上を見ている
// ときは届いた印が出て、どちらでも差分のスクロール位置が変わらない、
// kemi reply の発言が会話パネルに出る、一覧の項目からスレッドを開いて返信を書くとスレッドに出る、
// 「This file」の絞り込みが選んだファイルに合わせて変わる、
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
      replies: document.querySelectorAll('#cv-thread-body .cv-post').length,
      modal: document.querySelector('#modal-body')?.textContent,
    })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

const LINES = (count) => Array.from({ length: count }, (_, i) => `line ${i + 1}\n`);

/** worktree の変更。a.txt の 3 行目・60 行目・100 行目と、コメントの無い b.txt の 1 行目を書き換える。 */
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
  await writeFile(join(dir, 'b.txt'), LINES(5).join(''));
  await git('add', 'a.txt', 'b.txt');
  await git('commit', '-q', '-m', 'base');
  for (const index of [2, 59, 99]) {
    lines[index] = `changed ${index + 1}\n`;
  }
  await writeFile(join(dir, 'a.txt'), lines.join(''));
  await writeFile(join(dir, 'b.txt'), ['other\n', ...LINES(5).slice(1)].join(''));
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
const chipC1 = `document.querySelector('#diff-content .cchip[data-id="c1"]')`;
const threadOpen = (text) => `!document.querySelector('#cv-thread').hidden && document.querySelector('#cv-thread-body .cv-comment')?.textContent.includes(${JSON.stringify(text)})`;
const agentMessage = (text) => `Array.from(document.querySelectorAll('#cv-items .cv-msg[data-author="agent"]')).some(m => m.textContent.includes(${JSON.stringify(text)}))`;

/** ツリーからファイルを選び、その差分が出るのを待つ。 */
async function selectFile(name) {
  await evaluate(`Array.from(document.querySelectorAll('#tree button.file')).find(b => b.textContent.includes(${JSON.stringify(name)})).click(); true`);
  await waitFor(`document.querySelector('#file-header .path')?.textContent === ${JSON.stringify(name)} && document.querySelectorAll('[data-kemi-row]').length > 0`);
}

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
  // 「Hand to agent」だけが無い。会話パネルは畳んだ帯で始まり、差分の中の札を押すと開いて
  // そのスレッドになる。
  assert.equal(await evaluate(panelClosed), true);
  await evaluate(`${chipC1}.click(); true`);
  await waitFor(`${panelOpen} && ${threadOpen('rename this line')}`);
  assert.equal(await evaluate(shown('#agent-status')), true);
  assert.equal(await evaluate(statusIs('unconnected')), true);
  assert.equal(await evaluate(shown('#btn-hand')), false);
  assert.equal(await evaluate(shown('#cv-reply-text')), true);
  assert.equal(await evaluate(shown('#cv-thread-head .cv-resolve')), true);
  // 解決すると札は解決済みの印つきで 1 行に縮み、取り消すと戻る。
  await browser('click', '#cv-thread-head .cv-resolve');
  await waitFor(`${chipC1}?.classList.contains('folded') && ${chipC1}.querySelector('.t-resolved-mark') !== null`);
  await browser('click', '#cv-thread-head .cv-resolve');
  await waitFor(`${chipC1} !== null && !${chipC1}.classList.contains('folded') && ${chipC1}.querySelector('.t-resolved-mark') === null`);
  await browser('click', '#cv-thread-head .cv-back');
  await waitFor(`!document.querySelector('#cv-list').hidden`);
  assert.equal(await evaluate(shown('#cv-message')), true);
  console.log('PASS kemi wait を呼ぶ前から返信・解決・会話パネルと未接続の状態が出て、「Hand to agent」だけが無い。札を押すとパネルでスレッドが開き、解決できる');

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

  // (3) 新着と追従。差分のスクロール位置はどの場合も変わらない（R-LIVE）。
  // (3a) 会話パネルを畳んだまま kemi reply で返信が届くと、パネルは開かず、札に新着の印が付き、
  //      畳んだ帯に新着の数（新着のスレッドと、畳んでいる間に届いた発言）が出る。
  await browser('click', '#cv-close');
  await waitFor(panelClosed);
  await evaluate(`document.querySelector('#diff-viewport').scrollTop = 30; true`);
  await waitFor(`document.querySelector('#diff-viewport').scrollTop === 30`);
  const before = await evaluate(`document.querySelector('#diff-viewport').scrollTop`);
  const diffUnmoved = async () => assert.equal(await evaluate(`document.querySelector('#diff-viewport').scrollTop`), before, 'the diff must not move');
  const replied = await agentCommand(fixture, state, ['reply', kemi.id], JSON.stringify({
    writes: [
      { type: 'reply', comment_id: 'c1', body: 'Renamed it.' },
      { type: 'message', body: 'Both comments are addressed.' },
    ],
  }));
  assert.equal(replied.code, 0, replied.stderr);
  const { ids } = JSON.parse(replied.stdout);
  assert.equal(ids.length, 2);
  await waitFor(`${chipC1}?.querySelector('.unread-mark') !== null && document.querySelector('#cv-unread').textContent === '2' && !document.querySelector('#cv-unread').hidden`);
  assert.equal(await evaluate(panelClosed), true, 'the panel must not open by itself');
  await diffUnmoved();
  // (3b) 札からスレッドを開くと返信が見え、新着の印が消える。
  await evaluate(`${chipC1}.click(); true`);
  await waitFor(`${threadOpen('rename this line')} && Array.from(document.querySelectorAll('#cv-thread-body .cv-post[data-author="agent"]')).some(r => r.textContent.includes('Renamed it.'))`);
  await waitFor(`${chipC1}?.querySelector('.unread-mark') === null`);
  await diffUnmoved();
  console.log('PASS 畳んだまま返信が届くとパネルは開かず札と帯に新着が出て、スレッドを開くと消える');
  // (3c) 一覧の一番下を見ているときに発言が届くと、並びが新しいものまで進む。
  await browser('click', '#cv-thread-head .cv-back');
  await waitFor(`!document.querySelector('#cv-list').hidden`);
  const fillers = Array.from({ length: 30 }, (_, index) => `Note ${index + 1} about the review.`);
  const filled = await agentCommand(fixture, state, ['reply', kemi.id], JSON.stringify({
    writes: fillers.map((body) => ({ type: 'message', body })),
  }));
  assert.equal(filled.code, 0, filled.stderr);
  const atBottom = `(() => { const list = document.querySelector('#cv-items'); return list.scrollHeight > list.clientHeight + 100 && list.scrollHeight - list.scrollTop - list.clientHeight <= 12; })()`;
  await waitFor(`${agentMessage(fillers.at(-1))} && ${atBottom}`);
  assert.equal(await evaluate(`document.querySelector('#cv-newer').hidden`), true);
  await diffUnmoved();
  // (3d) 上のほうを見ているときに届くと、並びの位置は変わらず、届いたことを示す印が出る。
  await evaluate(`document.querySelector('#cv-items').scrollTop = 0; true`);
  await waitFor(`document.querySelector('#cv-items').scrollTop === 0`);
  const late = await agentCommand(fixture, state, ['reply', kemi.id], JSON.stringify({
    writes: [{ type: 'message', body: 'One more thing.' }],
  }));
  assert.equal(late.code, 0, late.stderr);
  await waitFor(`${agentMessage('One more thing.')} && !document.querySelector('#cv-newer').hidden`);
  assert.equal(await evaluate(`document.querySelector('#cv-items').scrollTop`), 0, 'the list must stay where it was read');
  await diffUnmoved();
  await browser('click', '#cv-newer');
  await waitFor(`${atBottom} && document.querySelector('#cv-newer').hidden`);
  console.log('PASS 一番下を見ているときだけ並びが新しいものについていき、上を見ているときは印が出て、差分は動かない');

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

  // (4b) 「This file」で絞り込んだまま別のファイルを選ぶと、一覧はそのファイルのスレッドに変わる。
  const cards = `Array.from(document.querySelectorAll('#cv-items .cv-card')).map(c => c.dataset.id).sort().join(',')`;
  await browser('click', '#cv-filter button[data-filter="file"]');
  await waitFor(`${cards} === 'c1,c2'`);
  await selectFile('b.txt');
  await waitFor(`${cards} === ''`);
  await selectFile('a.txt');
  await waitFor(`${cards} === 'c1,c2'`);
  await browser('click', '#cv-filter button[data-filter="all"]');
  console.log('PASS 「This file」の絞り込みは、選んだファイルに合わせて変わる');

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
  assert.deepEqual(result.messages.map((message) => message.body), ['Looking at it now.', 'Both comments are addressed.', ...fillers, 'One more thing.']);
  console.log('PASS 未渡しを残して submit を押すと確認に件数が出て、submit の JSON にそのコメントが入る');
} finally {
  kemi.child.kill('SIGTERM');
  await kemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
