// コードの見方の操作の細部を実バイナリで確かめる。行に乗せても本文が動かないこと、コメントを編集するときの
// suggestion の初めの文字列（R-COMMENT）、確認ダイアログが開いている間フォーカスがダイアログの中に留まること、
// `?` で開くキー操作の一覧（R-KEYS）、ファイルツリーの Tab の止まり先と矢印キー（R-VIEW）。
// node scripts/test-review-interactions.mjs <kemi-bin>
import assert from 'node:assert/strict';
import {writeFile} from 'node:fs/promises';
import {join, resolve} from 'node:path';

import {browserSession} from './lib/browser.mjs';
import {temporaryDirectory} from './lib/fixture.mjs';
import {getJson, launchKemi, postJson, stopKemi} from './lib/kemi.mjs';

assert.ok(process.argv[2], 'pass the kemi binary');
const binary = resolve(process.argv[2]);
const fixture = await temporaryDirectory('kemi-interactions-');
const state = await temporaryDirectory('kemi-interactions-state-');
const session = `kemi-interactions-${process.pid}`;
const {browser, evaluate, waitFor: wait, close} = browserSession(session);
const lines = Array.from({length: 20}, (_, i) => `const value${i + 1} = ${i + 1};\n`).join('');
const manifest = join(fixture, 'review.json');
await writeFile(manifest, JSON.stringify({title: 'Interactions', groups: [{id: 'group', title: 'Changes', diffs: [{path: 'sample.ts', old: lines, new: lines.replaceAll('const', 'let')}, {path: 'src/a/one.ts', old: 'a\n', new: 'b\n'}, {path: 'src/a/two.ts', old: 'a\n', new: 'c\n'}, {path: 'src/b/three.ts', old: 'a\n', new: 'd\n'}]}], approval: []}));
const kemi = await launchKemi(binary, {cwd: fixture, state, args: [manifest]});
try {
  const {url} = kemi;
  const review = await getJson(url, 'api/review');
  const file_id = review.groups[0].files[0].id;
  const plain = await postJson(url, 'api/comment', {op: 'add', file_id, side: 'new', start_line: 3, end_line: 4, body: 'no suggestion yet'});
  await browser('set', 'viewport', '1280', '900');
  await browser('open', url);
  await wait(`document.querySelectorAll('#diff-content .row').length > 0 && document.querySelector('#diff-content .cchip')`);
  await browser('press', 'u');

  // 行番号が 2 桁の行で確かめる。1 桁の行では、ボタンが行番号の欄の余白に収まってしまい、ずれが出ない。
  const row = `Array.from(document.querySelectorAll('#diff-content .row')).find(r => r.textContent.includes('value20 '))`;
  const codeLeft = `${row}.querySelector('.code').getBoundingClientRect().left`;
  const before = await evaluate(codeLeft);
  const {x, y} = await evaluate(`(() => { const box = ${row}.getBoundingClientRect(); return {x: Math.round(box.left + box.width / 2), y: Math.round(box.top + box.height / 2)}; })()`);
  await browser('mouse', 'move', String(x), String(y));
  await wait(`getComputedStyle(${row}.querySelector('.line-add-btn')).display !== 'none'`);
  const hovered = await evaluate(codeLeft);
  assert.equal(hovered, before, `the code of a hovered row stays in place (before ${before}, hovered ${hovered})`);
  console.log('PASS hovering a row shows the comment button without moving its code');

  await browser('click', `.cchip[data-id="${plain.id}"]`);
  await wait(`Array.from(document.querySelectorAll('#cv-thread-head button')).some(b => b.textContent === 'Edit')`);
  await evaluate(`Array.from(document.querySelectorAll('#cv-thread-head button')).find(b => b.textContent === 'Edit').click()`);
  await wait(`document.querySelector('#diff-content .editor textarea[data-editor-field="suggestion"]')`);
  const suggestion = await evaluate(`document.querySelector('#diff-content .editor textarea[data-editor-field="suggestion"]').value`);
  assert.equal(suggestion, 'let value3 = 3;\nlet value4 = 4;');
  console.log('PASS editing a comment without a suggestion starts the suggestion from the commented lines');
  await evaluate(`Array.from(document.querySelectorAll('#diff-content .editor button')).find(b => b.textContent === 'Cancel').click()`);
  await wait(`!document.querySelector('#diff-content .editor')`);

  // ほかの理由で操作できなくしてある要素は、ダイアログを閉じても操作できなくしたまま残す。
  await evaluate(`document.querySelector('#overlay').inert = true; true`);
  await browser('click', '#btn-changes');
  await wait(`!document.querySelector('#modal').hidden`);
  assert.equal(await evaluate(`document.activeElement?.id`), 'modal-cancel', 'opening the dialog focuses its safe choice');
  for (let i = 0; i < 4; i += 1) {
    await browser('press', 'Tab');
    assert.ok(await evaluate(`document.querySelector('#modal .box').contains(document.activeElement)`), `Tab ${i + 1} keeps the focus inside the dialog`);
  }
  console.log('PASS the confirmation dialog takes the focus and keeps Tab inside it');
  await browser('press', 'Escape');
  await wait(`document.querySelector('#modal').hidden`);
  assert.equal(await evaluate(`document.activeElement?.id`), 'btn-changes', 'closing the dialog returns the focus to the button that opened it');
  assert.equal(await evaluate(`Array.from(document.body.children).some(e => e.inert && e.id !== 'overlay')`), false, 'closing the dialog makes the page usable again');
  assert.equal(await evaluate(`document.querySelector('#overlay').inert`), true, 'closing the dialog keeps an element that was already inert');
  console.log('PASS closing the dialog returns the focus to its opener');

  await browser('press', 'Escape');
  await evaluate(`document.activeElement.blur(); true`);
  await browser('press', '?');
  await wait(`!document.querySelector('#modal').hidden`);
  // 閉じる操作は、どの画面でも見出しの行の右端の × にそろえる（R-VIEW）。
  const closeOf = sel => `(() => { const c = document.querySelector(${JSON.stringify(sel)}); if (!c) return null; const b = c.getBoundingClientRect(), box = c.closest('.box, .cv-panel, .sheet')?.getBoundingClientRect(); return {cls: c.classList.contains('dialog-close'), label: c.getAttribute('aria-label'), title: c.title, text: c.textContent.trim(), icon: !!c.querySelector('svg'), right: box ? Math.round(box.right - b.right) : null, top: box ? Math.round(b.top - box.top) : null}; })()`;
  const uniform = (c, what) => {
    assert.ok(c && c.cls && c.label === 'Close' && c.title === 'Close' && c.text === '' && c.icon, `${what}: the close control is the shared × (${JSON.stringify(c)})`);
  };
  const help = await evaluate(`({title: document.querySelector('#modal-title').textContent, body: document.querySelector('#modal-body').textContent, row: document.querySelector('#modal .row').checkVisibility(), focus: document.activeElement?.id})`);
  assert.equal(help.title, 'Keyboard shortcuts');
  for (const key of ['j', 'k', 'g', 'G', 'n', 'p', 'v', 's', 'u', 'w', 'r', '?']) assert.ok(help.body.includes(key), `the list names ${key}: ${help.body}`);
  assert.equal(help.row, false, 'the list has no row of choices, only the ×');
  assert.equal(help.focus, 'modal-close', 'the list focuses its ×');
  const helpClose = await evaluate(closeOf('#modal-close'));
  uniform(helpClose, 'key list');
  assert.ok(helpClose.right < 24 && helpClose.top < 24, `the × sits at the top right (${JSON.stringify(helpClose)})`);
  await browser('click', '#modal-close');
  await wait(`document.querySelector('#modal').hidden`);
  await browser('click', '#btn-changes');
  await wait(`!document.querySelector('#modal').hidden`);
  const confirm = await evaluate(`({ok: document.querySelector('#modal-ok').checkVisibility(), back: document.querySelector('#modal-cancel').textContent, focus: document.activeElement?.id})`);
  assert.deepEqual(confirm, {ok: true, back: 'Back', focus: 'modal-cancel'}, 'a confirmation keeps Back and its action below, and focuses Back');
  uniform(await evaluate(closeOf('#modal-close')), 'confirmation');
  await browser('mouse', 'move', '4', '4'); await browser('mouse', 'down'); await browser('mouse', 'up');
  await wait(`document.querySelector('#modal').hidden`);
  assert.equal(await evaluate(`document.activeElement?.id`), 'btn-changes', 'the backdrop closes the dialog and returns the focus');
  if (await evaluate(`document.querySelector('#conversation').dataset.open !== 'true'`)) await browser('click', '#btn-comments');
  await wait(`document.querySelector('#conversation').dataset.open === 'true'`);
  uniform(await evaluate(closeOf('#cv-close')), 'conversation panel');
  await browser('click', '#cv-close');
  await wait(`document.querySelector('#conversation').dataset.open !== 'true'`);
  console.log('PASS ? lists the keys, and every close control is the same × at the top right');

  // ファイルツリーは Tab の止まり先を 1 つだけ持ち、中は矢印キーで移る（R-VIEW）。
  const tabStops = `Array.from(document.querySelectorAll('#tree button')).filter(b => b.tabIndex >= 0)`;
  const stopState = async () => evaluate(`(() => { const stops = ${tabStops}; return {count: stops.length, label: stops[0]?.textContent.trim().replace(/\\s+/g, ' ').slice(0, 30), visible: stops[0]?.getClientRects().length > 0, focusedIsStop: stops[0] === document.activeElement}; })()`);
  const focused = () => evaluate(`document.activeElement.closest('#tree') ? document.activeElement.textContent.trim().replace(/\\s+/g, ' ').slice(0, 30) : null`);
  let stop = await stopState();
  assert.equal(stop.count, 1, `one tab stop at the start: ${JSON.stringify(stop)}`);
  assert.ok(stop.label.includes('sample.ts'), `the stop is the file on display: ${JSON.stringify(stop)}`);
  await evaluate(`${tabStops}[0].focus(); true`);
  await browser('press', 'ArrowUp');
  assert.ok((await focused())?.includes('Changes'), 'up from the first file reaches the group heading');
  await browser('press', 'ArrowDown');
  await browser('press', 'ArrowDown');
  assert.equal(await focused(), '▾src', 'down from the first file reaches the src heading');
  stop = await stopState();
  assert.ok(stop.count === 1 && stop.focusedIsStop, `the focused item becomes the only stop: ${JSON.stringify(stop)}`);
  await browser('press', 'ArrowLeft');
  assert.equal(await evaluate(`document.activeElement.getAttribute('aria-expanded')`), 'false', 'left folds an open heading');
  await browser('press', 'ArrowRight');
  assert.equal(await evaluate(`document.activeElement.getAttribute('aria-expanded')`), 'true', 'right opens a folded heading');
  await browser('press', 'ArrowRight');
  assert.equal(await focused(), '▾a', 'right on an open heading enters its first child');
  await browser('press', 'ArrowRight');
  await browser('press', 'Enter');
  await wait(`document.querySelector('#file-header')?.textContent.includes('src/a/one.ts')`);
  assert.ok((await focused())?.includes('one.ts'), 'Enter opens the file and keeps the focus on it');
  await browser('press', 'ArrowLeft');
  assert.equal(await focused(), '▾a', 'left on a file moves to its heading');
  await browser('press', 'ArrowLeft');
  assert.equal(await evaluate(`document.activeElement.getAttribute('aria-expanded')`), 'false', 'the heading folds by keyboard and keeps the focus');
  stop = await stopState();
  assert.ok(stop.count === 1 && stop.visible, `folding away the file on display still leaves one visible stop: ${JSON.stringify(stop)}`);
  await browser('press', 'End');
  assert.ok((await focused())?.includes('three.ts'), 'End reaches the last visible item');
  await browser('press', 'Home');
  assert.ok((await focused())?.includes('Changes'), 'Home reaches the first item');
  stop = await stopState();
  assert.ok(stop.count === 1 && stop.focusedIsStop, `the group heading becomes the stop too: ${JSON.stringify(stop)}`);
  await evaluate(`document.querySelector('#chip-sort').click(); document.querySelector('#chip-sort').click(); true`);
  assert.ok((await focused())?.includes('Changes'), 'rebuilding the tree keeps the focus on the group heading');
  // ツリーを作り直しても、ツリーの中のフォーカスは同じ項目に戻る。
  await browser('press', 'ArrowDown');
  await evaluate(`document.querySelector('#chip-sort').click(); true`);
  await wait(`document.querySelector('#chip-sort').getAttribute('aria-pressed') === 'true'`);
  assert.ok((await focused())?.includes('sample.ts'), 'rebuilding the tree keeps the focus on the same item');
  stop = await stopState();
  assert.equal(stop.count, 1, `one stop after the rebuild: ${JSON.stringify(stop)}`);
  await evaluate(`document.activeElement.blur(); true`);
  await browser('press', 'j');
  stop = await stopState();
  assert.equal(stop.count, 1, `one stop after moving with j: ${JSON.stringify(stop)}`);
  console.log('PASS the file tree is one tab stop, arrows move and fold inside it, and focus survives a rebuild');
} finally {
  await close();
  await stopKemi(kemi, 'SIGTERM');
}
