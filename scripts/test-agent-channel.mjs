// エージェントとの往復（agent-channel.md）のブラウザ自動化。実際の kemi バイナリを worktree
// モード（消えたコミットだけはコミット範囲）で起動し、`kemi wait` と `kemi reply` を別プロセスで
// 呼びながら、画面を agent-browser で確かめる。
//
//   node scripts/test-agent-channel.mjs <kemi-bin>
//
// 確かめること: kemi wait を呼ぶ前から返信の欄・解決・会話パネルの書く欄と未接続の状態が出て、
// 「Hand to agent」は押せないまま kemi wait <id> を写す操作が添えられる（畳んだ帯でも）、会話パネルの開閉と幅が読み込み直しても残る、ページの起動中に
// kemi wait と kemi reply の発言が来ても待機中と渡すと発言が出る、kemi wait を待たせると待機中、
// 返った後は作業中に変わり渡すが出る、畳んだまま返信が届くとパネルは開かず札と帯に新着が出て
// スレッドを開くと消える、一番下を見ているときだけ並びが新しいものについていき、上を見ている
// ときは届いた印が出て、どちらでも差分のスクロール位置が変わらない、
// kemi reply の発言が会話パネルに出る、一覧の項目からスレッドを開いて返信を書くとスレッドに出る、
// 「This file」の絞り込みが選んだファイルに合わせて変わる、
// 幅 390px でも会話パネルのシートを開いて閉じられる、未渡しを残して submit を押すと確認に件数が
// 出て、submit の JSON にそのコメントが入る、消えたコミットのスレッドが、会話パネルを開いたまま
// 読み直しても読み込み直しても「消えたコミット」と示される、「This file」で上のほうを見たまま
// 別のファイルを選んでも、別のファイルを読めなかった後に描き直しても届いた印が出ない、まだ読んで
// いないファイルのスレッドを開いても、開いたまま表示色の明暗を切り替えても対象の行の前後が見える、
// 畳んだ帯にも未渡しの件数つきで渡すが出る、渡した 1 回分の行が受け取り待ち → 作業中 → 返信で消える
// と変わり、状態が未接続 → 待機中 → 作業中 → 返事済みと変わる、渡した直後は渡すが押せず返信を書くと
// 押せる、kemi wait を一度も呼ばないレビューやエージェントがつながれないレビューでも渡すが押せないまま出て、
// submit は今どおり終わる、スレッドを開いている間に別のスレッドへ返信が届くと知らせと見出しの新着の数が出て、
// 知らせを押すとそのスレッドが開く。
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
      status: document.querySelector('#agent-status')?.dataset.kemiAgentState,
      conversation: document.querySelector('#conversation')?.dataset.open,
      thread: !document.querySelector('#cv-thread')?.hidden,
      replies: document.querySelectorAll('#cv-thread-body .cv-post').length,
      modal: document.querySelector('#modal-body')?.textContent,
    })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

const LINES = (count) => Array.from({ length: count }, (_, i) => `line ${i + 1}\n`);

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

/** worktree の変更。a.txt の 3 行目・60 行目・100 行目と、コメントの無い b.txt の 1 行目を書き換える。 */
async function makeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-agent-'));
  const git = gitIn(dir);
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

/** コミット範囲。base の後に、c.txt を変える first と、d.txt を変える second の 2 コミット。 */
async function makeRangeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-agent-range-'));
  const git = gitIn(dir);
  await git('init', '-q');
  await writeFile(join(dir, 'c.txt'), LINES(5).join(''));
  await writeFile(join(dir, 'd.txt'), LINES(5).join(''));
  await git('add', 'c.txt', 'd.txt');
  await git('commit', '-q', '-m', 'base');
  const from = (await git('rev-parse', 'HEAD')).stdout.trim();
  await writeFile(join(dir, 'c.txt'), ['first\n', ...LINES(5).slice(1)].join(''));
  await git('commit', '-q', '-am', 'first');
  await writeFile(join(dir, 'd.txt'), ['second\n', ...LINES(5).slice(1)].join(''));
  await git('commit', '-q', '-am', 'second');
  return { dir, from, git };
}

function environment(state) {
  return { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home'), LOCALAPPDATA: join(state, 'localappdata') };
}

async function startKemi(dir, state, source = ['--worktree']) {
  const child = spawn(binary, [...source, '--port', '0', '--no-open'], {
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
const statusIs = (status) => `document.querySelector('#agent-status').dataset.kemiAgentState === ${JSON.stringify(status)}`;
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
  // 「Hand to agent」は出ているが押せず、そばに kemi wait を始めると渡せることと、kemi wait <id> を
  // 写す操作が出る（畳んだ帯でも）。会話パネルは畳んだ帯で始まり、差分の中の札を押すと開いてそのスレッドになる。
  await evaluate(`window.__kemiCopied = []; navigator.clipboard.writeText = async (text) => { window.__kemiCopied.push(text); }; true`);
  assert.equal(await evaluate(panelClosed), true);
  assert.equal(await evaluate(`${shown('#rail-hand')} && document.querySelector('#rail-hand').disabled`), true, 'the folded rail shows Hand to agent, not pressable');
  // 畳んだ帯でも、案内と写す操作はボタンのそばに出たまま（ポインタを乗せなくても）。
  await waitFor(`${shown('#rail-hand-note')} && document.querySelector('#rail-hand-note .hand-command')?.textContent === ${JSON.stringify(`kemi wait ${kemi.id}`)}`);
  await browser('click', '#rail-hand-note .hand-copy');
  await waitFor(`window.__kemiCopied.length === 1`);
  assert.deepEqual(JSON.parse(await evaluate(`JSON.stringify(window.__kemiCopied)`)), [`kemi wait ${kemi.id}`]);
  console.log('PASS kemi wait の前は、畳んだ帯に「Hand to agent」が押せないまま出て、そばの kemi wait <id> を写す操作で写したものがこのレビューの id のコマンド');
  await evaluate(`${chipC1}.click(); true`);
  await waitFor(`${panelOpen} && ${threadOpen('rename this line')}`);
  assert.equal(await evaluate(shown('#agent-status')), true);
  assert.equal(await evaluate(statusIs('not-connected')), true);
  assert.equal(await evaluate(`${shown('#btn-hand')} && document.querySelector('#btn-hand').disabled`), true);
  assert.equal(await evaluate(`${shown('#hand-note .hand-command')} && document.querySelector('#hand-note .hand-command').textContent`), `kemi wait ${kemi.id}`);
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
  console.log('PASS kemi wait を呼ぶ前から返信・解決・会話パネルと未接続の状態が出て、「Hand to agent」は押せないまま案内とコマンドが出る。札を押すとパネルでスレッドが開き、解決できる');

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
  await waitFor(`!document.querySelector('#btn-hand').disabled && document.querySelector('#hand-note').hidden`);
  await waitFor(`${panelOpen} && ${agentMessage('Looking at it now.')}`);
  const loadingWaited = await loadingWait;
  assert.equal(loadingWaited.code, 3, `the wait should time out: ${JSON.stringify(loadingWaited)}`);
  await waitFor(statusIs('working'));
  console.log('PASS ページの起動中に kemi wait と kemi reply の発言が来ても、待機中と押せる「Hand to agent」と発言が出る');

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
  // 畳んだ帯からも、未渡しの件数つきで「Hand to agent」を押せる。
  const { unhanded } = (await (await fetch(new URL('api/review', kemi.url))).json()).agent;
  assert.ok(unhanded > 0);
  assert.equal(await evaluate(shown('#rail-hand')), true);
  assert.equal(await evaluate(`document.querySelector('#rail-hand-count').textContent.trim()`), String(unhanded));
  assert.equal(await evaluate(`document.querySelector('#rail-hand').disabled`), false);
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

// (7) 履歴の書き換えで消えたコミットのスレッドは、会話パネルを開いたまま更新バッジで読み直しても、
// 開いたまま読み込み直しても「消えたコミット」と示され、その行へ移る操作が出ない（R-VIEW、R-LIVE）。
const range = await makeRangeFixture();
const rangeState = await mkdtemp(join(tmpdir(), 'kemi-agent-range-state-'));
const rangeKemi = await startKemi(range.dir, rangeState, ['--from', range.from]);
try {
  // コミットごとの単位は、最初の api/review の後に裏で作られる。
  for (;;) {
    const current = await (await fetch(new URL('api/review', rangeKemi.url))).json();
    if (current.units?.find((unit) => unit.unit === 'commit')?.state === 'ready') break;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const commitReview = await (await fetch(new URL('api/review?unit=commit', rangeKemi.url))).json();
  const second = commitReview.groups.find((group) => group.title === 'second');
  await post(rangeKemi.url, 'api/comment', { op: 'add', file_id: second.files[0].id, side: 'new', start_line: 1, end_line: 1, body: 'about the second commit' });
  const card = `document.querySelector('#cv-items .cv-card[data-id="c1"]')`;
  const vanished = `${card}?.querySelector('.cv-unit.vanished') !== null`;
  await browser('open', rangeKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await browser('click', '#btn-comments');
  await waitFor(`${panelOpen} && ${card} !== null`);
  assert.equal(await evaluate(vanished), false);
  await range.git('commit', '-q', '--amend', '-m', 'second, rewritten');
  await waitFor(`!document.querySelector('#update-badge').hidden`);
  await browser('click', '#update-badge');
  await waitFor(vanished);
  await browser('reload');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${panelOpen}`);
  await waitFor(vanished);
  await evaluate(`${card}.click(); true`);
  await waitFor(threadOpen('about the second commit'));
  assert.equal(await evaluate(`document.querySelector('#cv-thread-head .cv-go') === null`), true);
  console.log('PASS 消えたコミットのスレッドは、開いたまま読み直しても読み込み直しても「消えたコミット」と示される');
} finally {
  rangeKemi.child.kill('SIGTERM');
  await rangeKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (8) 「This file」で絞り込んだ一覧の上のほうを見ているまま別のファイルを選んでも、何も届いて
// いないので、届いたことを示す印は出ない（R-AGENT-HAND）。前のファイルのスレッドより後に付いた
// スレッドが、選んだファイルにある場合。
const fileFixture = await makeFixture();
const fileState = await mkdtemp(join(tmpdir(), 'kemi-agent-file-state-'));
const fileKemi = await startKemi(fileFixture, fileState);
try {
  const files = (await (await fetch(new URL('api/review', fileKemi.url))).json()).groups[0].files;
  const fileIdOf = (path) => files.find((file) => file.path === path).id;
  for (let index = 0; index < 20; index += 1) {
    await post(fileKemi.url, 'api/comment', { op: 'add', file_id: fileIdOf('a.txt'), side: 'new', start_line: 3, end_line: 3, body: `note ${index + 1}` });
  }
  await post(fileKemi.url, 'api/comment', { op: 'add', file_id: fileIdOf('b.txt'), side: 'new', start_line: 1, end_line: 1, body: 'about b' });
  await browser('set', 'viewport', '1280', '800');
  await browser('open', fileKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await selectFile('a.txt');
  await browser('click', '#btn-comments');
  await waitFor(panelOpen);
  await browser('click', '#cv-filter button[data-filter="file"]');
  const list = `document.querySelector('#cv-items')`;
  await waitFor(`${list}.querySelectorAll('.cv-card').length === 20 && ${list}.scrollHeight > ${list}.clientHeight + 100`);
  await evaluate(`${list}.scrollTop = 0; true`);
  await waitFor(`${list}.scrollTop === 0`);
  await selectFile('b.txt');
  await waitFor(`Array.from(${list}.querySelectorAll('.cv-card')).map(c => c.dataset.id).join(',') === 'c21'`);
  assert.equal(await evaluate(`document.querySelector('#cv-newer').hidden`), true, 'nothing arrived, so no arrival mark');
  console.log('PASS 「This file」で上のほうを見たまま別のファイルを選んでも、届いた印は出ない');

  // (9) まだ読んでいないファイルのスレッドを開いても、対象の行の前後が見える（R-VIEW）。
  // b.txt の 3 行目にスレッドを足して読み込み直し、a.txt を表示したまま一覧から開く。
  await post(fileKemi.url, 'api/comment', { op: 'add', file_id: fileIdOf('b.txt'), side: 'new', start_line: 3, end_line: 3, body: 'about line 3 of b' });
  await browser('reload');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${panelOpen}`);
  await selectFile('a.txt');
  await evaluate(`${list}.querySelector('.cv-card[data-id="c22"]').click(); true`);
  await waitFor(threadOpen('about line 3 of b'));
  const contextLines = `Array.from(document.querySelectorAll('#cv-thread-body .cv-context .cv-line'))`;
  await waitFor(`${contextLines}.some(l => l.textContent === 'line 2') && ${contextLines}.some(l => l.textContent === 'line 4')`);
  assert.deepEqual(await evaluate(`${contextLines}.filter(l => l.classList.contains('hit')).map(l => l.textContent)`), ['line 3']);
  assert.equal(await evaluate(`document.querySelector('#file-header .path')?.textContent`), 'a.txt', 'the shown file stays');
  // 表示色の明暗を切り替えて行を読み直しても、開いたままのスレッドの対象の行の前後が見える。
  // 描き直した後の並びだと分かるよう、切り替える前の並びに印を付けておく。
  await evaluate(`document.querySelector('#cv-thread-body .cv-context').dataset.before = '1'; true`);
  await browser('click', '#btn-theme');
  await browser('click', '#btn-theme');
  await waitFor(`document.documentElement.dataset.theme === 'dark'`);
  const freshLines = `Array.from(document.querySelectorAll('#cv-thread-body .cv-context:not([data-before]) .cv-line'))`;
  await waitFor(`${freshLines}.some(l => l.textContent === 'line 2') && ${freshLines}.some(l => l.textContent === 'line 4')`);
  await browser('click', '#cv-thread-head .cv-back');
  await waitFor(`!document.querySelector('#cv-list').hidden`);
  console.log('PASS まだ読んでいないファイルのスレッドを開いても、表示色を切り替えても、対象の行の前後が見える');

  // (10) 別のファイルを読めなかった後に、エージェントの状態が変わって描き直しても、届いた印は
  // 出ない。読み込み直して b.txt をまだ読んでいない状態にし、b.txt の行データの取得を失敗させる。
  await browser('reload');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${panelOpen}`);
  await selectFile('a.txt');
  await browser('click', '#cv-filter button[data-filter="file"]');
  await waitFor(`${list}.querySelectorAll('.cv-card').length === 20 && ${list}.scrollHeight > ${list}.clientHeight + 100`);
  await evaluate(`${list}.scrollTop = 0; true`);
  await waitFor(`${list}.scrollTop === 0`);
  await evaluate(`(() => {
    const original = window.fetch.bind(window);
    window.fetch = (input, init) => String(input).startsWith(${JSON.stringify(`api/file/${encodeURIComponent(fileIdOf('b.txt'))}`)})
      ? Promise.reject(new TypeError('Failed to fetch'))
      : original(input, init);
    return true;
  })()`);
  await evaluate(`Array.from(document.querySelectorAll('#tree button.file')).find(b => b.textContent.includes('b.txt')).click(); true`);
  await waitFor(`!document.querySelector('#overlay').hidden`);
  const failedWait = agentCommand(fileFixture, fileState, ['wait', fileKemi.id, '--timeout', '1']);
  await waitFor(statusIs('waiting'));
  await waitFor(`Array.from(${list}.querySelectorAll('.cv-card')).map(c => c.dataset.id).join(',') === 'c21,c22'`);
  assert.equal(await evaluate(`document.querySelector('#cv-newer').hidden`), true, 'nothing arrived, so no arrival mark');
  const failedWaited = await failedWait;
  assert.equal(failedWaited.code, 3, `the wait should time out: ${JSON.stringify(failedWaited)}`);
  console.log('PASS 別のファイルを読めなかった後に描き直しても、届いた印は出ない');
} finally {
  fileKemi.child.kill('SIGTERM');
  await fileKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (11) 渡した 1 回分の行と状態（R-AGENT-HAND、R-AGENT-STATE）。kemi wait が待っていない間に c1 を渡すと
// c1 のスレッドの末尾に受け取り待ちの行が出て、kemi wait が返すと作業中、kemi reply で返信すると消えて
// 返事済みになり、次の kemi wait で待機中になる。c2 と c3 を 1 回分で渡して c2 にだけ返信すると、c2 の
// 行だけが消えて作業中のまま、c3 にも返信すると返事済み。返事済みの間に渡すと返事済みのまま受け取り
// 待ちの行が出る。発言だけを渡すと並びの末尾に行が出て、エージェントの発言で消える。
const handFixture = await makeFixture();
const handState = await mkdtemp(join(tmpdir(), 'kemi-agent-hand-state-'));
const handKemi = await startKemi(handFixture, handState);
try {
  const handFile = (await (await fetch(new URL('api/review', handKemi.url))).json()).groups[0].files[0].id;
  // API で足したコメントは開いている画面には届かない（コメントの増減は更新バッジに任せる）ので、読み込み直す。
  const addComment = async (line, body) => {
    const comment = await post(handKemi.url, 'api/comment', { op: 'add', file_id: handFile, side: 'new', start_line: line, end_line: line, body });
    await browser('reload');
    await waitFor(`${panelOpen} && document.querySelector('#cv-items .cv-card[data-id="${comment.id}"]') !== null`);
    return comment;
  };
  const reply = (writes) => agentCommand(handFixture, handState, ['reply', handKemi.id], JSON.stringify({ writes }));
  const waitOnce = (timeout) => agentCommand(handFixture, handState, ['wait', handKemi.id, '--timeout', String(timeout)]);
  const lineOf = (id) => `(document.querySelector('#cv-items .cv-card[data-id="${id}"] [data-kemi-hand-line]')?.dataset.kemiHandLine ?? null)`;
  const endLine = `(() => { const last = document.querySelector('#cv-items > li:last-child'); return last && !last.querySelector('.cv-card, .cv-msg') ? last.querySelector('[data-kemi-hand-line]')?.dataset.kemiHandLine ?? null : null; })()`;
  const pressHand = async () => {
    const button = `(${shown('#btn-hand')} ? document.querySelector('#btn-hand') : ${shown('#rail-hand')} ? document.querySelector('#rail-hand') : null)`;
    await waitFor(`${button} !== null && !${button}.disabled`);
    await evaluate(`${button}.click(); true`);
  };
  await browser('set', 'viewport', '1280', '800');
  await browser('open', handKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await browser('click', '#cv-rail');
  await waitFor(`${panelOpen} && ${statusIs('not-connected')}`);
  const first = waitOnce(30);
  await waitFor(statusIs('waiting'));
  await post(handKemi.url, 'api/message', { body: 'starting' });
  await pressHand();
  assert.equal((await first).code, 0);
  await waitFor(statusIs('working'));
  await reply([{ type: 'message', body: 'ok' }]);
  await waitFor(statusIs('replied'));
  console.log('PASS 状態が未接続 → 待機中 → 作業中 → 返事済みと変わる');

  await addComment(3, 'first thread');
  await pressHand();
  await waitFor(`${lineOf('c1')} === 'pending'`);
  await waitFor(`document.querySelector('#btn-hand').disabled && !document.querySelector('#hand-note').hidden`);
  await post(handKemi.url, 'api/comment', { op: 'reply', id: 'c1', body: 'also the caller' });
  await waitFor(`!document.querySelector('#btn-hand').disabled && document.querySelector('#hand-note').hidden`);
  console.log('PASS 渡した直後は「Hand to agent」が押せず渡すものが無いと出て、返信を 1 つ書くと押せる');
  await pressHand();
  const pickedUp = waitOnce(30);
  await waitFor(`${lineOf('c1')} === 'working'`);
  assert.equal((await pickedUp).code, 0);
  await waitFor(statusIs('working'));
  await reply([{ type: 'reply', comment_id: 'c1', body: 'done' }]);
  await waitFor(`${lineOf('c1')} === null && ${statusIs('replied')}`);
  console.log('PASS kemi wait が待っていない間に c1 を渡すと受け取り待ちの行が出て、kemi wait が返すと作業中、返信すると消えて返事済みになる');

  const next = waitOnce(30);
  await waitFor(statusIs('waiting'));
  console.log('PASS 返事済みの後に kemi wait を呼ぶと待機中になる');
  await addComment(60, 'second thread');
  await addComment(100, 'third thread');
  await pressHand();
  assert.equal((await next).code, 0);
  await waitFor(`${statusIs('working')} && ${lineOf('c2')} === 'working' && ${lineOf('c3')} === 'working'`);
  await reply([{ type: 'reply', comment_id: 'c2', body: 'done' }]);
  await waitFor(`${lineOf('c2')} === null && ${lineOf('c3')} === 'working'`);
  assert.equal(await evaluate(statusIs('working')), true, 'one thread is still waiting for an answer');
  console.log('PASS c2 と c3 を 1 回分で渡して c2 にだけ返信すると、c2 の行だけが消えて c3 の行と作業中が残る');
  await reply([{ type: 'reply', comment_id: 'c3', body: 'done too' }]);
  await waitFor(`${lineOf('c3')} === null && ${statusIs('replied')}`);
  console.log('PASS c3 にも返信すると返事済みになる');

  await addComment(4, 'fourth thread');
  await pressHand();
  await waitFor(`${lineOf('c4')} === 'pending'`);
  assert.equal(await evaluate(statusIs('replied')), true, 'handing does not change the status');
  console.log('PASS 返事済みの間に渡すと、返事済みのまま受け取り待ちの行が出る');

  const opened = `(document.querySelector('#cv-thread-body [data-kemi-hand-line]')?.dataset.kemiHandLine ?? null)`;
  await browser('click', '#cv-items .cv-card[data-id="c4"]');
  await waitFor(`${threadOpen('fourth thread')} && ${opened} === 'pending'`);
  console.log('PASS 開いたスレッドの末尾にも、そのスレッドの行が出る');
  await browser('click', '#cv-thread-head .cv-back');
  await waitFor(`!document.querySelector('#cv-list').hidden`);
  const fourth = waitOnce(30);
  assert.equal((await fourth).code, 0);
  await reply([{ type: 'reply', comment_id: 'c4', body: 'done' }]);
  await waitFor(`${lineOf('c4')} === null`);

  await post(handKemi.url, 'api/message', { body: 'one more thing' });
  await pressHand();
  await waitFor(`${endLine} === 'pending'`);
  const messages = waitOnce(30);
  await waitFor(`${endLine} === 'working'`);
  assert.equal((await messages).code, 0);
  await reply([{ type: 'message', body: 'noted' }]);
  await waitFor(`${endLine} === null && ${agentMessage('noted')}`);
  console.log('PASS 発言だけを渡すと並びの末尾に行が出て、エージェントの発言で消える');
} finally {
  handKemi.child.kill('SIGTERM');
  await handKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (12) kemi wait を一度も呼ばずに起動したレビュー（R-AGENT-FLOW）: 「Hand to agent」は出ていて押せず、
// submit すると今どおり stdout に JSON が出て終わる。未渡しのコメントがあっても、確認に未渡しの件数は出ない（R-SUBMIT）。
const quietFixture = await makeFixture();
const quietState = await mkdtemp(join(tmpdir(), 'kemi-agent-quiet-state-'));
const quietKemi = await startKemi(quietFixture, quietState);
try {
  const quietFile = (await (await fetch(new URL('api/review', quietKemi.url))).json()).groups[0].files[0].id;
  await post(quietKemi.url, 'api/comment', { op: 'add', file_id: quietFile, side: 'new', start_line: 3, end_line: 3, body: 'never handed' });
  await browser('set', 'viewport', '1280', '800');
  await browser('open', quietKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${statusIs('not-connected')}`);
  const pressable = `(${shown('#rail-hand')} && !document.querySelector('#rail-hand').disabled) || (${shown('#btn-hand')} && !document.querySelector('#btn-hand').disabled)`;
  assert.equal(await evaluate(`${shown('#rail-hand')} || ${shown('#btn-hand')}`), true, 'Hand to agent is shown');
  assert.equal(await evaluate(pressable), false, 'Hand to agent cannot be pressed');
  await browser('click', '#btn-approve');
  await waitFor(`!document.querySelector('#modal').hidden`);
  assert.equal(await evaluate(`document.querySelector('#unhanded-notice') === null`), true, 'no unhanded count without kemi wait');
  await browser('click', '#modal-ok');
  const { code, stdout } = await quietKemi.exited;
  assert.equal(code, 0);
  assert.equal(JSON.parse(stdout).comments[0].body, 'never handed');
  console.log('PASS kemi wait を一度も呼ばないレビューでは「Hand to agent」が出ていて押せず、submit すると stdout に JSON が出て終わる');
} finally {
  quietKemi.child.kill('SIGTERM');
  await quietKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (13) エージェントがつながれないレビュー（状態の置き場所が決まらず、review の行もエンドポイントも無い）でも、
// 「Hand to agent」は押せないまま出て、写すコマンドは無く、つながれないことを言う。
const aloneFixture = await makeFixture();
const aloneEnv = { ...process.env };
for (const name of ['XDG_STATE_HOME', 'HOME', 'LOCALAPPDATA']) delete aloneEnv[name];
const alone = spawn(binary, ['--worktree', '--port', '0', '--no-open'], { cwd: aloneFixture, env: aloneEnv, stdio: ['ignore', 'pipe', 'pipe'] });
let aloneStderr = '';
alone.stdout.resume();
const aloneUrl = await new Promise((resolve, reject) => {
  alone.stderr.on('data', (chunk) => {
    aloneStderr += chunk;
    const url = aloneStderr.match(/^kemi: (http:\/\/\S+)/m);
    if (url) resolve(url[1]);
  });
  alone.on('exit', (code) => reject(new Error(`kemi exited before serving (${code}): ${aloneStderr}`)));
});
const aloneExited = new Promise((resolve) => alone.on('exit', resolve));
try {
  assert.equal(/^kemi: review /m.test(aloneStderr), false, 'no review line without a state location');
  await browser('set', 'viewport', '1280', '800');
  await browser('open', aloneUrl);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await waitFor(`${shown('#rail-hand')} && document.querySelector('#rail-hand').disabled`);
  await waitFor(shown('#rail-hand-note'));
  assert.equal(await evaluate(`document.querySelector('#rail-hand-note .hand-command') === null`), true, 'no command to copy');
  console.log('PASS エージェントがつながれないレビューでも「Hand to agent」は押せないまま出て、写すコマンドは無く、つながれないことを言う');
} finally {
  alone.kill('SIGTERM');
  await aloneExited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (14) 別のスレッドの新着（R-AGENT-HAND）: c1 を開いている間に kemi reply で c2 に返信を書くと、開いている
// スレッドの上に c2 に届いた知らせと、会話パネルの見出しに新着の数が出る。知らせを押すと c2 が開く。
const newsFixture = await makeFixture();
const newsState = await mkdtemp(join(tmpdir(), 'kemi-agent-news-state-'));
const newsKemi = await startKemi(newsFixture, newsState);
try {
  const newsFile = (await (await fetch(new URL('api/review', newsKemi.url))).json()).groups[0].files[0].id;
  await post(newsKemi.url, 'api/comment', { op: 'add', file_id: newsFile, side: 'new', start_line: 3, end_line: 3, body: 'first thread' });
  await post(newsKemi.url, 'api/comment', { op: 'add', file_id: newsFile, side: 'new', start_line: 60, end_line: 60, body: 'second thread' });
  await browser('set', 'viewport', '1280', '800');
  await browser('open', newsKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  await browser('click', '#cv-rail');
  await waitFor(`${panelOpen} && document.querySelector('#cv-items .cv-card[data-id="c1"]') !== null`);
  await browser('click', '#cv-items .cv-card[data-id="c1"]');
  await waitFor(threadOpen('first thread'));
  assert.equal(await evaluate(shown('#cv-other-new')), false);
  assert.equal(await evaluate(shown('#cv-head-unread')), false);
  const written = await agentCommand(newsFixture, newsState, ['reply', newsKemi.id], JSON.stringify({ writes: [{ type: 'reply', comment_id: 'c2', body: 'about the second' }] }));
  assert.equal(written.code, 0, written.stderr);
  await waitFor(`${shown('#cv-other-new')} && document.querySelector('#cv-other-new').dataset.id === 'c2' && document.querySelector('#cv-other-new').textContent.includes('second thread')`);
  await waitFor(shown('#cv-head-unread'));
  assert.equal(await evaluate(threadOpen('first thread')), true, 'the open thread stays open');
  console.log('PASS c1 を開いている間に c2 に返信が届くと、開いているスレッドの上に c2 の知らせと、パネルの見出しに新着の数が出る');
  await browser('click', '#cv-other-new');
  await waitFor(`${threadOpen('second thread')} && !${shown('#cv-other-new')} && !${shown('#cv-head-unread')}`);
  console.log('PASS 知らせを押すと c2 のスレッドが開く');
} finally {
  newsKemi.child.kill('SIGTERM');
  await newsKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}

// (15) 狭い画面の浮かぶ「Hand to agent」（R-AGENT-STATE、R-NARROW）: 幅 390px の worktree のレビューで、会話パネルの
// シートを閉じたまま、画面の下に浮かぶ「Hand to agent」で渡せ、kemi wait がその 1 回分を返す。
const floatFixture = await makeFixture();
const floatState = await mkdtemp(join(tmpdir(), 'kemi-agent-float-state-'));
const floatKemi = await startKemi(floatFixture, floatState);
try {
  const floatFile = (await (await fetch(new URL('api/review', floatKemi.url))).json()).groups[0].files[0].id;
  await post(floatKemi.url, 'api/comment', { op: 'add', file_id: floatFile, side: 'new', start_line: 3, end_line: 3, body: 'from the phone' });
  await browser('set', 'viewport', '390', '844');
  await browser('open', floatKemi.url);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0 && ${panelClosed}`);
  await waitFor(`${shown('#hand-float')} && document.querySelector('#hand-float').disabled`);
  const waiting = agentCommand(floatFixture, floatState, ['wait', floatKemi.id, '--timeout', '30']);
  await waitFor(`${statusIs('waiting')} && !document.querySelector('#hand-float').disabled`);
  const rect = JSON.parse(await evaluate(`JSON.stringify(document.querySelector('#hand-float').getBoundingClientRect())`));
  assert.ok(rect.bottom > 844 - 80 && rect.bottom <= 844, `the button floats at the bottom: ${JSON.stringify(rect)}`);
  await browser('click', '#hand-float');
  const waited = await waiting;
  assert.equal(waited.code, 0, waited.stderr);
  const handed = JSON.parse(waited.stdout).events.filter((event) => event.type === 'handed');
  assert.equal(handed[0].comments[0].comment.body, 'from the phone');
  assert.equal(await evaluate(panelClosed), true, 'the sheet stays closed');
  console.log('PASS 幅 390px で会話パネルのシートを閉じたまま、画面の下に浮かぶ「Hand to agent」で渡すと kemi wait がその 1 回分を返す');
} finally {
  await browser('set', 'viewport', '1280', '800').catch(() => {});
  floatKemi.child.kill('SIGTERM');
  await floatKemi.exited;
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
