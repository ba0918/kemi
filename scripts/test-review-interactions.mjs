// コードの見方の操作の細部を実バイナリで確かめる。行に乗せても本文が動かないこと、コメントを編集するときの
// suggestion の初めの文字列（R-COMMENT）、確認ダイアログが開いている間フォーカスがダイアログの中に留まること。
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
await writeFile(manifest, JSON.stringify({title: 'Interactions', groups: [{id: 'group', title: 'Changes', diffs: [{path: 'sample.ts', old: lines, new: lines.replaceAll('const', 'let')}]}], approval: []}));
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
} finally {
  await close();
  await stopKemi(kemi, 'SIGTERM');
}
