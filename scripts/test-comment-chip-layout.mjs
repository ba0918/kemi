// コメント札が本文だけを省略し、場所と印を 1 行で表示することを実バイナリで確かめる（R-VIEW）。
// node scripts/test-comment-chip-layout.mjs <kemi-bin>
import assert from 'node:assert/strict';
import {mkdtemp, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';

import {browserSession} from './lib/browser.mjs';
import {getJson, launchKemi, postJson, stopKemi} from './lib/kemi.mjs';

assert.ok(process.argv[2], 'pass the kemi binary');
const binary = resolve(process.argv[2]);
const fixture = await mkdtemp(join(tmpdir(), 'kemi-chip-layout-'));
const state = await mkdtemp(join(tmpdir(), 'kemi-chip-layout-state-'));
const session = `kemi-chip-layout-${process.pid}`;
const {browser, evaluate, waitFor: wait, close} = browserSession(session);
const lines = Array.from({length: 20}, (_, i) => `const value${i + 1} = ${i + 1};\n`).join('');
const manifest = join(fixture, 'review.json');
await writeFile(manifest, JSON.stringify({title: 'Comment layout', groups: [{id: 'group', title: 'Changes', diffs: [{path: 'sample.ts', old: lines, new: lines.replaceAll('const', 'let')}]}], approval: []}));
const kemi = await launchKemi(binary, {cwd: fixture, state, args: [manifest]});
try {
  const {url} = kemi;
  const post = body => postJson(url, 'api/comment', body);
  const review = await getJson(url, 'api/review');
  const file_id = review.groups[0].files[0].id;
  const body = 'Keep the location readable and show the complete comment when its thread is opened. '.repeat(5);
  const first = await post({op: 'add', file_id, side: 'new', start_line: 4, end_line: 4, body});
  await post({op: 'reply', id: first.id, body: 'A reply in the thread.'});
  await post({op: 'add', file_id, side: 'new', start_line: 8, end_line: 10, body, suggestion: 'replacement'});
  const old = await post({op: 'add', file_id, side: 'old', start_line: 12, end_line: 13, body});
  await post({op: 'resolve', id: old.id, resolved: true});
  const cases = [[1440, 'unified', true], [1280, 'unified', false], [1280, 'split', false], [390, 'unified', false], [320, 'unified', false]];
  for (const [width, mode, conversation] of cases) {
    await browser('set', 'viewport', String(width), '900');
    await browser('open', url);
    await wait(`document.querySelectorAll('#diff-content .cchip').length === 3`);
    if (width >= 720) await browser('press', mode === 'split' ? 's' : 'u');
    const open = await evaluate(`document.querySelector('#conversation').dataset.open === 'true'`);
    if (open !== conversation) await browser('click', '#btn-comments');
    await wait(`document.querySelector('#conversation').dataset.open === '${conversation}'`);
    await wait(`document.fonts.status === 'loaded'`);
    const geometry = await evaluate(`Array.from(document.querySelectorAll('#diff-content .cchip')).map(chip => {
      const box = chip.getBoundingClientRect(), row = chip.parentElement.getBoundingClientRect();
      const labels = Array.from(chip.children).filter(e => !e.classList.contains('tx')).map(e => {
        const range = document.createRange(); range.selectNodeContents(e); const lines = Array.from(range.getClientRects());
        return {text:e.textContent, lines:lines.length, top:Math.min(...lines.map(r=>r.top)), bottom:Math.max(...lines.map(r=>r.bottom)),left:Math.min(...lines.map(r=>r.left)),right:Math.max(...lines.map(r=>r.right))};
      });
      const text = chip.querySelector('.tx');
      return {id:chip.dataset.id, top:box.top,bottom:box.bottom,left:box.left,right:box.right,rowRight:row.right,labels,body:text.textContent,truncated:text.scrollWidth>text.clientWidth};
    })`);
    for (const chip of geometry) {
      assert.ok(chip.right <= chip.rowRight + 1, `${width}/${mode}: the chip must fit its row: ${JSON.stringify(chip)}`);
      for (const label of chip.labels) {
        assert.equal(label.lines, 1, `${width}/${mode}: ${label.text} must stay on one line`);
        assert.ok(label.top >= chip.top - 1 && label.bottom <= chip.bottom + 1, `${width}/${mode}: ${label.text} must stay inside its chip`);
        assert.ok(label.left >= chip.left - 1 && label.right <= chip.right + 1, `${width}/${mode}: ${label.text} must not be cut off horizontally`);
      }
      assert.equal(chip.body, body, 'ellipsizing must not remove the stored comment text');
      assert.equal(chip.truncated, true, 'long comment bodies are truncated');
    }
    console.log(`PASS ${width}px ${mode}${conversation ? ' with conversation' : ''}: labels and badges fit one row; only the body is truncated`);
  }
  await browser('set', 'viewport', '1280', '900');
  await browser('open', url);
  await wait(`document.querySelectorAll('#diff-content .cchip').length === 3`);
  await browser('click', `.cchip[data-id="${first.id}"]`);
  await wait(`document.querySelector('#cv-thread-body')?.textContent.includes(${JSON.stringify(body)}) && document.querySelector('#cv-thread-body').textContent.includes('A reply in the thread.')`);
  console.log('PASS opening the chip shows the complete comment and its reply');
} finally {
  await close();
  await stopKemi(kemi, 'SIGTERM');
}
