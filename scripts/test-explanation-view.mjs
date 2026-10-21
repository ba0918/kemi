// 保留して復元した会話（エージェントが parts で書いた返信と発言）と説明の見方を、実バイナリと実フォントで読む。
//
//   node scripts/test-explanation-view.mjs <kemi-bin>                     全部の節
//   node scripts/test-explanation-view.mjs <kemi-bin> --parts-only        復元した会話の parts（Markdown・コード・図・描けない表示）だけ
//   node scripts/test-explanation-view.mjs <kemi-bin> --navigation        説明の見方（目次・幅・版の更新・2 つの画面・保留と復元・参照）だけ
//   node scripts/test-explanation-view.mjs <kemi-bin> --live-navigation   --live の説明（ページの参照・保留と復元・渡す）だけ
//   node scripts/test-explanation-view.mjs <kemi-bin> --range-navigation  コミット範囲のグループ単位の参照だけ
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, readFile, writeFile, readdir, unlink } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { promisify } from 'node:util';
import { createServer } from 'node:http';

import { browserSession } from './lib/browser.mjs';
import { temporaryDirectory } from './lib/fixture.mjs';
import { getJson, isolatedEnvironment, kemiOutput, launchKemi, postJson, stopKemi } from './lib/kemi.mjs';

const run = promisify(execFile);
const binary = resolve(process.argv[2]);
let root = await temporaryDirectory('kemi-parts-');
const session = `kemi-parts-${process.pid}`;
const { browser, evaluate: evaluateCode, waitFor: wait, close } = browserSession(session);
const evaluate = async code => {
  if (code.includes('await import')) {
    code=`(async()=>{${/;\s*true\s*$/.test(code) ? code.replace(/;\s*true\s*$/, ';return true;') : `return (${code});`}})()`;
  }
  return evaluateCode(code);
};
const frames = () => evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
/** 文字のノード（式の文字列）の [start, end) を選ぶ。 */
async function selectText(node, start, end) {
  await evaluate(`(()=>{const n=${node};n.parentElement.scrollIntoView({block:'center'});const r=document.createRange();r.setStart(n,${start});r.setEnd(n,${end});const s=getSelection();s.removeAllRanges();s.addRange(r)})()`);
  await frames();
}
function start(args) {
  return launchKemi(binary, { cwd: root, state: root, args });
}
const bodyA = '**A begins**\n\n![external](https://example.invalid/a.png) ![relative](a.png) ![data](data:image/png;base64,AA) ![reference][r]\n\n[r]: https://example.invalid/ref.png\n\n<img src="html.png">\n\nA ends\n';
const codeB = { type: 'code', body: '<script>literal B</script>\n', language: 'pseudocode', role: 'example', title: 'Code B' };
const parts = [{ type: 'text', body: bodyA }, codeB, { type: 'text', body: '**C survives**' }];
// 描けないときの表示は、まだ描いていない本文で起こす（同じ本文を描き直させることに頼らない）。
const lateA = bodyA.replace('A ends', 'A ends late');
const lateReplyA = bodyA.replace('A ends', 'A ends in the reply');
/** 本文 body の text パーツが描けず、元の文のまま失敗の知らせと並んでいる所（無ければ undefined）を読む式。 */
const failedPart = body => `Array.from(document.querySelectorAll('pre')).find(e=>e.textContent===${JSON.stringify(body)} && e.previousElementSibling?.getAttribute('role')==='status' && e.previousElementSibling.textContent.includes('Network connection lost'))`;
/** text パーツの本文が bodies のどれかなら、描く頼みを通信の失敗にする（前に包んだものは外してから包む）。 */
const failRendering = bodies => `window.unwrappedFetch??=window.fetch; window.fetch=(input,init)=> { let part; try {part=JSON.parse(init?.body)} catch {} if(part?.type==='text' && ${JSON.stringify(bodies)}.includes(part.body)) return Promise.reject(new TypeError('Network connection lost')); return unwrappedFetch(input,init); }; true`;
/** 通信の失敗を外し、それから描く頼み（POST api/parts）の text パーツの本文を、頼んだ順に window.renderRequests に控える。 */
const countRendering = `window.unwrappedFetch??=window.fetch; window.renderRequests=[]; window.fetch=(input,init)=> { let part; try {part=JSON.parse(init?.body)} catch {} if(String(input).includes('api/parts') && part?.type==='text') renderRequests.push(part.body); return unwrappedFetch(input,init); }; true`;
const sequence = { type: 'sequence', title: 'Sequence operations', role: 'example', participants: [{ id: 'a', label: 'Reader' }, { id: 'b', label: 'Server' }], events: [{ type: 'message', from: 'a', to: 'b', label: 'Request', kind: 'call' }, { type: 'message', from: 'b', to: 'a', label: 'Response', kind: 'return' }, { type: 'message', from: 'a', to: 'a', label: 'Self', kind: 'call' }] };
const grid = { type: 'diagram', title: 'Grid example', role: 'example', source: `a = ${'長いラベル'.repeat(30)}\nb = ? Pending\n| a | b |\na -> b : next` };
const flow = { type: 'flow', title: 'Flow operations', role: 'proposal', nodes: [{id:'a',kind:'process',label:'First'},{id:'b',kind:'process',label:'Second'}], edges:[{from:'a',to:'b'}] };
const outside = { ...flow, title:'Outside canvas', canvas:{width:64,height:64} };
const collision = { ...flow, title:'Label collision', edges:[{from:'a',to:'b',label:{text:'one',position:{x:300,y:180}}},{from:'a',to:'b',label:{text:'two',position:{x:300,y:180}}}] };
const intrusion = { ...flow, title:'Route intrusion', nodes:[...flow.nodes,{id:'c',kind:'process',label:'Middle',position:{x:160,y:180}}], edges:[{from:'a',to:'b',via:[]}] };
const framed = {...sequence,title:'Alt and loop',events:[{type:'alt',branches:[{condition:'yes',messages:[sequence.events[0]]},{condition:'no',messages:[sequence.events[1]]}]},{type:'loop',condition:'repeat',messages:[sequence.events[0]]}]};
const decision = {type:'flow',title:'Decision and return',role:'confirmed',nodes:[{id:'a',kind:'start',label:'Begin',position:{x:160,y:100}},{id:'b',kind:'decision',label:'Ready?',position:{x:160,y:300}},{id:'c',kind:'end',label:'End',position:{x:160,y:520}}],edges:[{from:'a',to:'b'},{from:'b',to:'c',label:{text:'yes'}},{from:'b',to:'a',from_port:'east',to_port:'east',via:[{x:400,y:300},{x:400,y:100}],label:{text:'retry',position:{x:440,y:200}}}]};
const longEnd = {type:'flow',title:'Long end label',role:'example',nodes:[{id:'a',kind:'start',label:'Begin',position:{x:160,y:100}},{id:'b',kind:'end',label:'終わりのラベル'.repeat(6),position:{x:160,y:400}}],edges:[{from:'a',to:'b'}]};
const tallStart = {type:'flow',title:'Tall font start',role:'example',nodes:[{id:'a',kind:'start',label:'n'.repeat(40)}],edges:[]};
const touching = {type:'flow',title:'Allowed contact and crossing',role:'example',nodes:[{id:'a',kind:'process',label:'A',position:{x:160,y:100},width:180,height:64},{id:'b',kind:'process',label:'B',position:{x:340,y:100},width:180,height:64},{id:'c',kind:'process',label:'C',position:{x:160,y:400}},{id:'d',kind:'process',label:'D',position:{x:500,y:400}}],edges:[{from:'a',to:'d',from_port:'south',to_port:'north',via:[]},{from:'b',to:'c',from_port:'south',to_port:'north',via:[]}]};
let running;
if (!process.argv.includes('--navigation') && !process.argv.includes('--live-navigation') && !process.argv.includes('--range-navigation')) try {
  await writeFile(join(root, 'a.txt'), 'old\n');
  await run('git', ['init', '-q', root]);
  await run('git', ['-C', root, 'add', 'a.txt']);
  await run('git', ['-C', root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']);
  await writeFile(join(root, 'a.txt'), 'new\n');
  running = await start(['--worktree']);
  const post = (path, value) => postJson(running.url, path, value);
  const review = await getJson(running.url, 'api/review');
  const comment = await post('api/comment', {op:'add',file_id:review.groups[0].files[0].id,side:'new',body:'Comment stays'});
  await post('api/message', { body: '**old plain stays**' });
  // parts の返信と発言はエージェントが kemi reply で書く（人間の発言と返信は body だけ）。保留して、復元した会話を読む。
  const id = (await readdir(join(root, 'kemi', 'sessions'))).find(name => name.endsWith('.endpoint')).slice(0, -9);
  const writes = [{ type: 'reply', comment_id: comment.id, parts }, { type: 'message', parts: [...parts, sequence, grid, flow, outside, collision, intrusion, framed, decision, touching, longEnd, tallStart] }];
  await kemiOutput(binary, ['reply', id], { cwd: root, env: isolatedEnvironment(root), input: JSON.stringify({ writes }) });
  const paused = await stopKemi(running);
  assert.equal(paused.code, 130, paused.stderr);
  running = await start(['--resume', id]);
  await browser('open', running.url);
  await browser('network', 'har', 'start');
  await browser('find', 'role', 'button', 'click', '--name', 'Open the conversation');
  await wait(`document.body.textContent.includes('C survives')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='A begins')`), true);
  assert.equal(await evaluate(`document.body.textContent.includes('**old plain stays**')`), false);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='old plain stays')`), true);
  assert.equal(await evaluate(`document.body.textContent.includes('Image not loaded') && document.body.textContent.includes('https://example.invalid/ref.png')`), true);
  await browser('find', 'role', 'button', 'click', '--name', 'Copy code');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.getAttribute('aria-label')==='Copied')`);
  const compose = 'Write to the agent about the whole review (Cmd/Ctrl+Enter to post)';
  await browser('find', 'placeholder', compose, 'click');
  await browser('clipboard', 'paste');
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('textarea')).find(e=>e.placeholder===${JSON.stringify(compose)}).value`), codeB.body);
  await browser('find', 'placeholder', compose, 'fill', '');
  const har = join(root, 'images.har'); await browser('network', 'har', 'stop', har);
  const traffic = JSON.parse(await readFile(har, 'utf8'));
  assert.equal(traffic.log.entries.filter(e => /example\.invalid|\/a\.png|\/html\.png|\/api\/image\//.test(e.request.url)).length, 0);
  console.log('PASS: restored plain/Markdown/code order and no image requests');
  // 失敗を仕込んでから、エージェントが新しい発言を書く。届いた発言は通常のパーツの描画の経路で描く。
  await evaluate(failRendering([lateA]));
  const reply = writes => kemiOutput(binary, ['reply', id], { cwd: root, env: isolatedEnvironment(root), input: JSON.stringify({ writes }) });
  await reply([{ type: 'message', parts: [{ type: 'text', body: lateA }, codeB, { type: 'text', body: '**C survives**' }] }]);
  await wait(`${failedPart(lateA)} !== undefined`);
  // 隣のパーツは同じ発言の中で描ける。
  assert.equal(await evaluate(`(()=>{const post=${failedPart(lateA)}.closest('section').parentElement;return post.textContent.includes('literal B') && Array.from(post.querySelectorAll('strong')).some(e=>e.textContent==='C survives')})()`), true);
  console.log('PASS: ordinary fetch failure preserves the complete original A and adjacent B/C');
  // 描けなかった本文は、通信が戻れば描ける。会話を閉じて開き直すと、並びを全部作り直す。描けなかった本文だけを
  // 頼み直して Markdown で描き、描けた本文（ほかの発言とパーツ）は頼み直さない。
  await evaluate(countRendering);
  await browser('find', 'role', 'button', 'click', '--name', 'Close', '--exact');
  await browser('find', 'role', 'button', 'click', '--name', 'Open the conversation');
  await wait(`${failedPart(lateA)} === undefined && Array.from(document.querySelectorAll('p')).some(p=>p.textContent==='A ends late' && Array.from(p.parentElement.querySelectorAll('strong')).some(e=>e.textContent==='A begins'))`);
  assert.deepEqual(await evaluate(`window.renderRequests`), [lateA], 'reopening asks again only for the body that could not be drawn');
  console.log('PASS: reopening draws the failed A as Markdown and asks again for nothing already drawn');
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations')`);
   await evaluate(`document.body.style.zoom='1.5';true`);
  console.log('PASS: the sequence diagram is drawn');
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Grid example')`);
  assert.equal(await evaluate(`(()=>{const svg=Array.from(document.querySelectorAll('svg[role="img"]')).find(e=>e.getAttribute('aria-label')==='Grid example');const label=Array.from(svg.querySelectorAll('text')).find(e=>e.textContent==='next').getBBox();const y=Number(svg.querySelector('line').getAttribute('y1'));return label.y>y||label.y+label.height<y})()`), true, 'a left-to-right grid edge label stays off its line');
  await browser('find', 'role', 'button', 'click', '--name', '長いラベル'.repeat(30));
  assert.equal(await evaluate(`document.body.textContent.includes(${JSON.stringify('長いラベル'.repeat(30))})`), true);
  assert.equal(await evaluate(`document.body.textContent.includes('Comment stays')`), true);
  const failures = await evaluate(`Array.from(document.querySelectorAll('pre')).map(e=>{try{return JSON.parse(e.textContent).title}catch{return null}}).filter(Boolean)`);
  assert.deepEqual(failures.sort(), ['Label collision','Outside canvas','Route intrusion'].sort());
  // 描けなかった図の理由と元の JSON は、写せるようにドラッグで選べる。
  const original = `Array.from(document.querySelectorAll('pre')).find(e=>{try{return JSON.parse(e.textContent).title==='Label collision'}catch{return false}})`;
  const span = await evaluate(`(()=>{const pre=${original};pre.scrollIntoView({block:'start'});const r=document.createRange();r.setStart(pre.firstChild,0);r.setEnd(pre.firstChild,12);const b=r.getBoundingClientRect();return {x:Math.round(b.left+1),y:Math.round(b.top+b.height/2),end:Math.round(b.right-1)}})()`);
  await browser('mouse', 'move', String(span.x), String(span.y)); await browser('mouse', 'down'); await browser('mouse', 'move', String(span.end), String(span.y)); await browser('mouse', 'up');
  assert.ok((await evaluate(`getSelection().toString()`)).length > 0, 'the original JSON of a failed diagram can be selected by dragging');
  await evaluate(`getSelection().removeAllRanges();true`);
  for(const title of ['Alt and loop','Decision and return','Allowed contact and crossing','Long end label','Tall font start']) assert.equal(await evaluate(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')===${JSON.stringify(title)})`),true);
  await evaluate(`Array.from(document.querySelectorAll('[role="group"]')).find(e=>e.getAttribute('aria-label')==='Sequence operations diagram controls').focus();true`);
  const transform = () => evaluate(`Array.from(document.querySelectorAll('svg[role="img"]')).find(e=>e.getAttribute('aria-label')==='Sequence operations').style.transform`);
  const before = await transform(); await browser('press', 'ArrowRight'); assert.notEqual(await transform(), before);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).some(e=>(e.getAttribute('aria-label')??'').startsWith('Comment on ') && e.closest('.part-diagram, .part-grid'))`), false, 'reply diagrams have no comment button');
  assert.equal(await evaluate(`Boolean(document.querySelector('[aria-label="Sequence operations view tools"] [aria-label="Move right"], [aria-label="Sequence operations view tools"] [aria-label="Reset diagram"]'))`), false, 'inline reply diagrams have no pan or reset buttons');
   await browser('press', '+'); const zoomed = await transform();
  await browser('press', '0'); assert.notEqual(await transform(), zoomed);
  await evaluate(`window.diagramSource=Array.from(document.querySelectorAll('header')).find(e=>e.textContent.includes('Sequence operations')).parentElement.querySelector('[aria-label="Open diagram separately"]');window.diagramSource.click();true`);
  await wait(`document.querySelector('dialog')?.open`);
  await browser('find', 'role', 'button', 'click', '--name', 'Close diagram');
  assert.equal(await evaluate(`document.activeElement===window.diagramSource`), true);
  await evaluate(`const style=document.createElement('style');style.id='font-change';style.textContent='.explanation-diagram {font-size:180px!important;font-family:serif!important}';document.head.append(style);true`);
  await wait(`Array.from(document.querySelectorAll('pre')).some(e=>{try{return JSON.parse(e.textContent).title==='Sequence operations'}catch{return false}})`);
  assert.equal(await evaluate(`document.body.textContent.includes('C survives') && document.body.textContent.includes('Comment stays')`), true);
  await evaluate(`document.querySelector('#font-change').remove();document.body.style.zoom='1.25';true`);
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations')`);
  // 1 行の箱が行の送り 18 より高いフォントでも、長いラベルの Start/End が収まる。ascent/descent を上書きした
  // フォントで行を高くする。元のフォントが見つからないと素通りするので、先に行の箱が 19 より高いことを確かめる。
  const tallTitles = ['Long end label', 'Tall font start'];
  const failedTitles = `Array.from(document.querySelectorAll('pre')).map(e=>{try{return JSON.parse(e.textContent).title}catch{return null}})`;
  await evaluate(`(()=>{for(const title of ${JSON.stringify(tallTitles)}) Array.from(document.querySelectorAll('svg[role="img"]')).find(e=>e.getAttribute('aria-label')===title).firstElementChild.setAttribute('data-before','1');const tall=document.createElement('style');tall.id='tall-font';tall.textContent='@font-face{font-family:"Tall";src:local("IPAGothic"),local("Liberation Sans"),local("DejaVu Sans"),local("FreeSans");ascent-override:115%;descent-override:35%} .explanation-diagram {font-family:"Tall"!important}';document.head.append(tall);return true})()`);
  const tallLineBox = await evaluate(`(async()=>{await document.fonts.load('14px "Tall"');const ns='http://www.w3.org/2000/svg';const svg=document.createElementNS(ns,'svg');svg.setAttribute('class','explanation-diagram');const text=document.createElementNS(ns,'text');text.setAttribute('dominant-baseline','text-before-edge');text.textContent='n';svg.append(text);document.body.append(svg);const height=text.getBBox().height;svg.remove();return height})()`);
  assert.ok(tallLineBox > 19, `the tall font is available (line box ${tallLineBox})`);
  await wait(`${JSON.stringify(tallTitles)}.every(title=>${failedTitles}.includes(title) || Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')===title && e.isConnected && e.firstElementChild && !e.querySelector('[data-before]')))`);
  assert.deepEqual(await evaluate(`${failedTitles}.filter(title=>${JSON.stringify(tallTitles)}.includes(title))`), [], 'long Start/End labels fit with a font whose line box is taller than the line step');
  await evaluate(`document.querySelector('#tall-font').remove();true`);
  console.log('PASS: grid full labels, real layout rejection, keyboard zoom/pan/fit, separate view return, font/zoom revalidation');
  await evaluate(failRendering([lateReplyA]));
  await reply([{ type: 'reply', comment_id: comment.id, parts: [{ type: 'text', body: lateReplyA }, codeB, { type: 'text', body: '**C survives**' }] }]);
  await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Comment stays')).click();true`);
  await wait(`${failedPart(lateReplyA)} !== undefined`);
  assert.equal(await evaluate(`document.body.textContent.includes('Comment stays') && Array.from(${failedPart(lateReplyA)}.closest('section').parentElement.querySelectorAll('strong')).some(e=>e.textContent==='C survives')`),true);
  console.log('PASS: the same Part fallback preserves reply content and its comment');
  await browser('find', 'role', 'button', 'click', '--name', '‹ Conversation');
  await evaluate(`document.body.style.zoom='1';true`);
  await browser('set', 'viewport', '390', '900');
  await browser('find', 'role', 'button', 'click', '--name', 'Conversation (1 comment)');
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations' && e.getClientRects().length)`);
  await evaluate(`window.diagramSource=Array.from(document.querySelectorAll('header')).find(e=>e.textContent.includes('Sequence operations')).parentElement.querySelector('[aria-label="Open diagram separately"]');window.diagramSource.click();true`);
  await wait(`document.querySelector('dialog')?.open`);
  await browser('press', 'Escape');
  assert.equal(await evaluate(`!document.querySelector('dialog') && document.activeElement===window.diagramSource && document.activeElement.getClientRects().length>0`), true);
  console.log('PASS: Escape closes the separate diagram and returns to its source at 390px');
} finally {
  await close();
  if (running) await stopKemi(running);
}

if (!process.argv.includes('--parts-only') && !process.argv.includes('--live-navigation') && !process.argv.includes('--range-navigation')) try {
  root = await temporaryDirectory('kemi-explanation-');
  await writeFile(join(root, 'a.txt'), 'old\n');
  await run('git', ['init', '-q', root]);
  await run('git', ['-C', root, 'add', 'a.txt']);
  await run('git', ['-C', root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']);
  await writeFile(join(root, 'a.txt'), 'new\n');
  const ref = {type:'code',unit:null,group_id:'worktree',path:'a.txt',side:'new',start_line:null,end_line:null};
  await writeFile(join(root,'b.txt'),'another file\n');
  await writeFile(join(root,'skip.txt'),'x'.repeat(1_048_577));
  const sections = [{id:'intro',title:'Reading order',blocks:[{id:'first',part:{type:'text',body:bodyA},references:[]},...Array.from({length:12},(_,i)=>({id:`context-${i}`,part:{type:'text',body:`Context paragraph ${i}\n\n${'Reader context. '.repeat(40)}`},references:[]}))]},
    {id:'middle',title:'Middle section',blocks:[{id:'headings',part:{type:'text',body:'## Sub heading one\n\nUnder the first sub heading.\n\n### Sub heading two\n\nUnder the second sub heading.'},references:[]},{id:'code',part:codeB,references:[ref,{...ref,path:'b.txt'}]},{id:'last',part:{type:'text',body:'**C survives**'},references:[]},{id:'diagram',part:sequence,references:[]}]}];
  await writeFile(join(root, 'explanation.json'), JSON.stringify({sections}));
  running = await start(['--worktree','--explanation',join(root,'explanation.json')]);
  await browser('set','viewport','1280','900');
  await browser('open','about:blank');await browser('network','har','start');
  await browser('open',running.url);
  await wait(`document.body.textContent.includes('C survives')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent==='Explanation' && e.getAttribute('aria-pressed')==='true')`),true);
  const initialHar=join(root,'initial.har');await browser('network','har','stop',initialHar);
  const initialRequests=JSON.parse(await readFile(initialHar,'utf8')).log.entries;
  assert.equal(initialRequests.filter(e=>e.request.url.includes('/api/file/')).length,0,'opening Explanation does not precompute Code content');
  // 人間の追加要件: 説明だけの幅調整。内部DOM名ではなく操作と実幅を観測する。
  const contentsWidth = () => evaluate(`Array.from(document.querySelectorAll('aside')).find(e=>e.getAttribute('aria-label')==='Explanation contents').getBoundingClientRect().width`);
  const separator = `Array.from(document.querySelectorAll('[role="separator"]')).find(e=>e.getClientRects().length)`;
  assert.equal(await evaluate(`Boolean(${separator})`), true, 'Explanation contents can be resized');
  const initialContentsWidth = await contentsWidth();
  await evaluate(`${separator}.focus();true`);await browser('press','ArrowRight');
  assert.ok(await contentsWidth() > initialContentsWidth, 'keyboard expands the contents');
  await browser('press','End');const maxContentsWidth = await contentsWidth();
  await browser('press','ArrowRight');assert.equal(await contentsWidth(), maxContentsWidth, 'maximum width is clamped');
  await browser('press','Home');const minContentsWidth = await contentsWidth();
  await browser('press','ArrowLeft');assert.equal(await contentsWidth(), minContentsWidth, 'minimum width is clamped');
  assert.ok(minContentsWidth < initialContentsWidth && maxContentsWidth > initialContentsWidth);
  const resizeBox = await evaluate(`(()=>{const r=${separator}.getBoundingClientRect();return {x:Math.round(r.x+r.width/2),y:Math.round(r.y+r.height/2)}})()`);
  await browser('mouse','move',String(resizeBox.x),String(resizeBox.y));await browser('mouse','down');
  await browser('mouse','move',String(resizeBox.x+60),String(resizeBox.y));await browser('mouse','up');
  assert.ok(await contentsWidth() > minContentsWidth, 'pointer expands the contents');
  await browser('find','role','button','click','--name','Code','--exact');
  const codeTreeWidth = await evaluate(`Array.from(document.querySelectorAll('aside')).find(e=>e.getAttribute('aria-label')==='File tree').getBoundingClientRect().width`);
  await browser('find','role','button','click','--name','Explanation','--exact');
  await evaluate(`${separator}.focus();true`);await browser('press','End');
  await browser('find','role','button','click','--name','Code','--exact');
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('aside')).find(e=>e.getAttribute('aria-label')==='File tree').getBoundingClientRect().width`), codeTreeWidth);
  await browser('find','role','button','click','--name','Explanation','--exact');
  await evaluate(`${separator}.focus();true`);await browser('press','Home');
  console.log('PASS: contents resize by keyboard/pointer, both clamps, Code width unchanged');
  // 畳まない文書と目次を、1280px・390px と light・dark の 4 通りで見る。テーマは画面のボタンで切り替える。
  const artifacts = resolve('.agents/artifacts'); await mkdir(artifacts, { recursive: true });
  const readingRegion = `document.querySelector('[aria-label="Explanation"][role="region"]')`;
  const setTheme = async name => {
    for (let attempt = 0; attempt < 6; attempt++) {
      if ((await evaluate(`document.querySelector('#btn-theme').title`)).startsWith(`Theme: ${name} `)) return;
      await browser('click', '#btn-theme');
    }
    throw new Error(`theme ${name} not reached`);
  };
  const goTo = async label => {
    if (await evaluate(`innerWidth<720`)) {
      await browser('find','role','button','click','--name','Open file tree');
      await wait(`document.querySelector('[aria-label="Explanation contents"]').getBoundingClientRect().left>=-1`);
    }
    await browser('find','role','button','click','--name',`Go to ${label}`,'--exact');
    await frames();
  };
  const readingLayout = async (width, theme) => {
    assert.equal(await evaluate(`${readingRegion}.querySelectorAll('details, summary').length`), 0, `${width}/${theme}: sections cannot be collapsed`);
    assert.equal(await evaluate(`Array.from(${readingRegion}.querySelectorAll('section[aria-labelledby]')).every(e=>{const c=getComputedStyle(e);return ['Top','Right','Bottom','Left'].every(side=>c['border'+side+'Width']==='0px') && c.outlineStyle==='none' && c.backgroundColor==='rgba(0, 0, 0, 0)'})`), true, `${width}/${theme}: sections have no frame`);
    assert.equal(await evaluate(`Array.from(${readingRegion}.querySelectorAll('section[aria-labelledby] > header')).every(e=>getComputedStyle(e).borderBottomWidth!=='0px' && /^\\d+$/.test(e.querySelector('span').textContent))`), true, `${width}/${theme}: numbered headings are underlined`);
    await goTo('Middle section');
    assert.ok(await evaluate(`Math.abs(Array.from(${readingRegion}.querySelectorAll('section[aria-labelledby]')).find(e=>e.querySelector('h2')?.textContent==='Middle section').getBoundingClientRect().top-${readingRegion}.getBoundingClientRect().top)<60`), `${width}/${theme}: contents move to the section`);
    await evaluate(`${readingRegion}.scrollTop=0;true`); await frames();
    await browser('screenshot', join(artifacts, `explanation-reading-${width}-${theme}.png`));
  };
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.getAttribute('aria-label')==='Go to Sub heading one')`);
  await setTheme('light'); await readingLayout(1280, 'light');
  await setTheme('dark'); await readingLayout(1280, 'dark');
  await browser('set','viewport','390','900'); await readingLayout(390, 'dark');
  await browser('set','viewport','1280','900'); await setTheme('light');
  await browser('set','viewport','390','900'); await readingLayout(390, 'light');
  await browser('set','viewport','1280','900');
  const indent = await evaluate(`(()=>{const item=label=>Array.from(document.querySelectorAll('button')).find(e=>e.getAttribute('aria-label')==='Go to '+label);return [item('Middle section'),item('Sub heading one'),item('Sub heading two')].map(e=>parseFloat(getComputedStyle(e).paddingLeft))})()`);
  assert.ok(indent[1] > indent[0] && indent[1] === indent[2], `Markdown headings are listed one step indented at every level (${indent})`);
  await goTo('Sub heading two');
  assert.ok(await evaluate(`(()=>{const h=Array.from(document.querySelectorAll('h3')).find(e=>e.textContent==='Sub heading two');return Math.abs(h.getBoundingClientRect().top-${readingRegion}.getBoundingClientRect().top)<60})()`), 'a sub heading item moves to the heading');
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('button[aria-current="location"]')).map(e=>e.getAttribute('aria-label')).join()`), 'Go to Sub heading two', 'the item at the reading position is shown');
  console.log('PASS: 1280/390 light/dark sections without collapse or frame, numbered underlined headings, contents navigation, indented Markdown headings and the current item');
  const review = await getJson(running.url, 'api/review');
  const id = (await readdir(join(root,'kemi','sessions'))).find(name=>name.endsWith('.endpoint')).slice(0,-9);
  const cli = (...args)=>kemiOutput(binary,args,{cwd:root,env:isolatedEnvironment(root)});
  const context=async()=>JSON.parse(await cli('explain',id,'--context'));
  const update=async(sections,base=review.diff_display_version)=>{
    const current=await context();
    await writeFile(join(root,'update.json'),JSON.stringify({expected_revision:current.explanation_revision,base_display_version:base,sections}));
    return JSON.parse(await cli('explain',id,'--file',join(root,'update.json')));
  };
  // 画面に出る版で確かめる。読んでいる版は説明の見方の状態の行（Revision n）に、コードの表示の版は、説明の基点と
  // 食い違うときだけ知らせ（更新のボタンと状態の行）に Code <版> として出る（合っていれば知らせは無い）。
  const d1=review.diff_display_version;
  const statusText=`document.querySelector('#explanation-status').textContent`;
  const updateText=`document.querySelector('#explanation-update').textContent`;
  const shownCode=version=>`${statusText}.includes(${JSON.stringify(`/ Code ${version}`)})`;
  const reading=revision=>`${statusText}.includes(${JSON.stringify(`Revision ${revision}`)})`;
  const noMismatch=`!/different display version|not loaded/.test(${statusText})`;
  const comments=async()=>(await getJson(running.url,'api/review')).comments;
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('p')).some(e=>e.checkVisibility() && /fixed file contents|proof/.test(e.textContent))`),false);
  await browser('find','text','About the explanation base','click');
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('p')).some(e=>e.checkVisibility() && /not fixed file contents/.test(e.textContent) && /proof/.test(e.textContent))`),true);
  await browser('find','role','button','click','--name','Go to Middle section','--exact');
  await selectText(`document.querySelector('[data-explanation-block="first"] .part-markdown strong').firstChild`, 0, 1);
  await browser('press','c');
  await browser('find','label','Comment on Reading order','fill','Question before correction');
  await browser('find','role','button','click','--name','Add comment');
  await wait(`document.body.textContent.includes('Question before correction')`);
  assert.deepEqual(await evaluate(`(()=>{const item=Array.from(document.querySelectorAll('button')).find(e=>e.getAttribute('aria-label')==='Go to Reading order');return {shown:item.innerText.trim().split(/\\s+/).at(-1),described:/^1 \\S/.test(document.getElementById(item.getAttribute('aria-describedby')).textContent)}})()`), {shown:'1',described:true}, 'the commented section shows its count and describes it in words');
  assert.deepEqual(await evaluate(`(()=>{const item=Array.from(document.querySelectorAll('button')).find(e=>e.getAttribute('aria-label')==='Go to Middle section');return {shown:item.innerText.trim().split(/\\s+/).at(-1),described:document.getElementById(item.getAttribute('aria-describedby')).textContent}})()`), {shown:'section',described:''}, 'a section without comments shows and describes no count');
  await browser('find','role','button','click','--name','Open the conversation');
  await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Question before correction')).click();true`);
  await browser('find','placeholder','Reply (Cmd/Ctrl+Enter to send)','fill','Existing reply stays');
  await browser('find','role','button','click','--name','Reply','--exact');
  await wait(`document.body.textContent.includes('Existing reply stays')`);
  const beforeEdit=(await getJson(running.url, 'api/review')).comments[0];
  await browser('find','role','button','click','--name','Edit','--exact');
  await browser('find','label','Comment','fill','Question remains','--exact');
  await browser('find','role','button','click','--name','Save','--exact');
  await wait(`!Array.from(document.querySelectorAll('textarea')).some(e=>e.getAttribute('aria-label')==='Comment') && document.body.textContent.includes('Question remains')`);
  const afterEdit=(await getJson(running.url, 'api/review')).comments[0];
  assert.equal(afterEdit.body,'Question remains');
  assert.deepEqual(afterEdit,{...beforeEdit,body:'Question remains'});
  assert.equal(await evaluate(`document.body.textContent.includes('Existing reply stays')`),true);
  console.log('PASS: UI correction keeps the same comment ID, original revision/block/full quote and existing replies');
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Go to Middle section','--exact');
  await evaluate(`window.readingDocument=document.querySelector('[aria-label="Explanation"][role="region"]');window.originalTop=readingDocument.scrollTop;true`);
  assert.ok(await evaluate('originalTop>0'));
  // 版 1 の基点（D1）とコードの表示の版が同じ。
  assert.equal(await evaluate(`${reading(1)} && ${noMismatch}`),true);
  await browser('find','role','button','click','--name','Code a.txt (new)','--exact');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent==='Code' && e.getAttribute('aria-pressed')==='true')`);
  // 説明へのコメントはファイルに属さない（第三の対象）ので、コードの見方のファイルの札にも行の札にも出ない。
  await wait(`document.querySelector('#file-header')?.textContent.includes('a.txt') && Array.from(document.querySelectorAll('[data-kemi-row]')).some(e=>e.textContent.includes('new'))`);
  assert.deepEqual(await evaluate(`['#floating-threads','#diff-area'].filter(s=>document.querySelector(s).textContent.includes('Question remains'))`),[],'a comment on the explanation is not shown on the file in Code');
  await update([{...sections[0],title:'Renamed heading'},sections[1]]);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 2'))`);
  // 知らせに食い違いが出ない: 説明を更新してもコードの表示の版は D1 のまま。
  assert.equal(await evaluate(updateText),'Open explanation revision 2');
  await browser('find','role','button','click','--name','Back to explanation');
  assert.equal(await evaluate(`readingDocument.scrollTop===window.originalTop && ${reading(1)} && readingDocument.textContent.includes('Reading order') && !readingDocument.textContent.includes('Renamed heading')`),true);
  await browser('find','role','button','click','--name','Open explanation revision 2');
  await wait(`document.body.textContent.includes('Renamed heading')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('article')).some(e=>e.textContent.includes('Question remains'))`),true);
  // 版 3 は、まだ描いていない本文のブロックを 1 つ足したもの（ほかのブロックは変えないので、コメントは outdated にならない）。
  await evaluate(failRendering([lateA]));
  await update([{...sections[0],blocks:[sections[0].blocks[0],{id:'late',part:{type:'text',body:lateA},references:[]},...sections[0].blocks.slice(1)]},sections[1]]);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 3'))`);
  await browser('find','role','button','click','--name','Open explanation revision 3');
  await wait(`${failedPart(lateA)} !== undefined`);
  assert.equal(await evaluate(`document.body.textContent.includes('Question remains') && document.body.textContent.includes('literal B') && Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='C survives')`),true);
  // AだけがCodeをD2へ取り込む。別tab BはD1の一覧を持ち続ける。
  await browser('tab','new',running.url);
  await wait(`document.body.textContent.includes('C survives')`);
  assert.equal(await evaluate(`${reading(3)} && ${noMismatch}`),true);
  await browser('tab','t1');
  await browser('find','role','button','click','--name','Code','--exact');
  await writeFile(join(root,'a.txt'),'new\nfrom the agent\n');
  await wait(`document.querySelector('#update-badge')?.checkVisibility()`);
  await browser('find','role','button','click','--name','Update available','--exact');
  await wait(`Array.from(document.querySelectorAll('[data-kemi-row]')).some(e=>e.textContent.includes('from the agent'))`);
  const d2=(await context()).diff_display_version;
  assert.notEqual(d2,review.diff_display_version);
  await update(sections,d2);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 4'))`);
  // A のコードは D2 を取り込んだので、D2 を基点にした版 4 の知らせに食い違いが無い。
  assert.equal(await evaluate(updateText),'Open explanation revision 4');
  await browser('tab','t2');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 4') && e.textContent.includes('different display version'))`);
  assert.equal(await evaluate(`${updateText}.includes(${JSON.stringify(`(${d2} / Code ${d1})`)}) && ${reading(3)}`),true);
  await browser('find','role','button','click','--name','Open explanation revision 4');
  // 通知の取りこぼしからの読み直しには画面の入口が無いので、受け口（app.js の onMissed と同じ 2 つ）を直接呼ぶ。
  await evaluate(`window.fetchCount=0;const original=window.fetch;window.fetch=(...args)=>{if(String(args[0]).includes('api/review'))fetchCount++;return original(...args)};const app=await import(new URL('assets/features/explanation.js',location.href));await app.receiveExplanation(null);const agent=await import(new URL('assets/features/agent.js',location.href));agent.receiveMissed();true`);
  await wait(`window.fetchCount>0`);
  assert.equal(await evaluate(`${reading(4)} && ${shownCode(d1)}`),true);
  await evaluate(`document.body.style.zoom='1.25';true`);
  assert.equal(await evaluate(`${shownCode(d1)} && Array.from(document.querySelectorAll('p')).some(e=>e.textContent.includes('Explanation for a different display version'))`),true);
  await evaluate(`document.body.style.zoom='1';true`);
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations')`);
  await evaluate(`const style=document.createElement('style');style.id='explanation-font-probe';style.textContent='.explanation-diagram {font-size:180px!important;font-family:serif!important}';document.head.append(style);true`);
  await wait(`Array.from(document.querySelectorAll('pre')).some(e=>{try{return JSON.parse(e.textContent).title==='Sequence operations'}catch{return false}})`);
  assert.equal(await evaluate(`${shownCode(d1)} && ${reading(4)} && Array.from(document.querySelectorAll('p')).some(e=>e.textContent.includes('Explanation for a different display version'))`),true);
  await evaluate(`document.querySelector('#explanation-font-probe').remove();true`);
  await browser('set','viewport','390','900');
  await browser('find','role','button','click','--name','Code a.txt (new)','--exact');
  await browser('find','role','button','click','--name','Back to explanation');
  assert.equal(await evaluate(`${reading(4)} && document.querySelector('[aria-label="Explanation"][role="region"]').checkVisibility() && document.querySelector('#explanation-status').checkVisibility()`),true);
  await browser('find','role','button','click','--name','Open file tree');
  await wait(`document.querySelector('[aria-label="Explanation contents"]').getBoundingClientRect().left>=-1`);
  await browser('find','role','button','click','--name','Go to Middle section','--exact');
  // Conversationの下書きと旧引用は、説明更新と見方の変更をまたいで保持する。
  await browser('find','role','button','click','--name','Conversation (1 comment)');
  await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Question remains')).click();true`);
  await browser('find','placeholder','Reply (Cmd/Ctrl+Enter to send)','fill','Draft across views');
  await update([sections[1]],d2);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 5'))`);
  await wait(`document.body.textContent.includes('Outdated explanation block')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('textarea')).some(e=>e.value==='Draft across views') && document.querySelector('#progress-text').textContent.startsWith('Seen 0 /')`),true);
  assert.equal((await comments())[0].explanation.revision,1);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Open explanation revision 5');
  await update(sections,d2);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 6'))`);
  await browser('find','role','button','click','--name','Open explanation revision 6');
  await wait(`Array.from(document.querySelectorAll('article')).some(e=>e.textContent.includes('Question remains'))`);
  assert.equal((await comments())[0].outdated,false);
  await unlink(join(root,'b.txt'));
  await browser('find','role','button','click','--name','Code b.txt (new)','--exact');
  await wait(`document.querySelector('#overlay-card')?.textContent.includes('Reference unavailable for this display version')`);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Back to explanation');
  await unlink(join(root,'explanation.json'));
  await stopKemi(running);
  running=await start(['--resume',id]);await browser('open',running.url);
  await wait(`document.querySelector('[aria-label="Explanation"][role="region"]')?.checkVisibility()`);
  const frozen=await context();
  assert.equal(frozen.diff_display_version,review.diff_display_version);
  assert.equal(frozen.explanation.base_display_version,d2);
  assert.equal(frozen.explanation_revision,6);
  await wait(`document.body.textContent.includes('Explanation for a different display version')`);
  const restored=(await comments())[0].explanation;
  assert.equal(restored.revision,1);assert.equal(restored.quote.block.part.body,bodyA);
  // 既知の旧基点で受理した未知対象・未知行数・消えた行は画面の一覧で理由を出す。
  await update([{id:'unavailable',title:'Unavailable targets',blocks:[{id:'targets',part:{type:'text',body:'Unavailable links'},references:[
    {...ref,path:'missing.txt'}, {...ref,start_line:2,end_line:3}, {...ref,path:'skip.txt',start_line:1,end_line:1}
  ]}]}],d2);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 7'))`);
  await browser('find','role','button','click','--name','Open explanation revision 7');
  await browser('find','role','button','click','--name','Code missing.txt (new)','--exact');
  await wait(`document.body.textContent.includes('Reference unavailable for this display version')`);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Code skip.txt:1–1 (new)','--exact');
  await wait(`document.body.textContent.includes('line count is unknown')`);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Code','--exact');
  await browser('find','role','button','click','--name','Explanation','--exact');
  await browser('find','role','button','click','--name','Code a.txt:2–3 (new)','--exact');
  await wait(`document.body.textContent.includes('requested lines are no longer present')`);
  assert.equal(await evaluate(`document.querySelector('[aria-label="Explanation"][role="region"]').checkVisibility() && ${shownCode(d1)}`),true);
  console.log('PASS: initial Explanation, base details, old reading position, unchanged comments, no explanation comment on the Code file, isolated Part fallback, two-screen Code tokens, missed notification recovery and 390px reference return');
  console.log('PASS: non-live process resume without the explanation input restores frozen Code, unchanged full quote and explanation base; unavailable/unknown/out-of-range references stay on the explanation with a reason');
} finally {
  await close();
  if (running) await stopKemi(running);
}

if (!process.argv.includes('--parts-only') && !process.argv.includes('--navigation') && !process.argv.includes('--range-navigation')) {
  root=await temporaryDirectory('kemi-explanation-live-');
  const dev=createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end(`<html><body><h1>${req.url}</h1><a href="/other?one=1">Other page</a></body></html>`)});
  await new Promise(resolve=>dev.listen(0,'127.0.0.1',resolve));
  const url=`http://127.0.0.1:${dev.address().port}/`;
  try {
    await writeFile(join(root,'a.txt'),'old\n');
    await run('git',['init','-q',root]);await run('git',['-C',root,'add','a.txt']);
    await run('git',['-C',root,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);
    await writeFile(join(root,'a.txt'),'new\n');
    running=await start(['--live',url]);
    await browser('set','viewport','1280','900');await browser('open',running.url);
    await wait(`document.querySelector('#live-stage')?.checkVisibility()`);
    assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent==='Explanation').disabled`),true);
    const id=(await readdir(join(root,'kemi','sessions'))).find(name=>name.endsWith('.endpoint')).slice(0,-9);
    const cli=(...args)=>kemiOutput(binary,args,{cwd:root,env:isolatedEnvironment(root)});
    const context=async()=>JSON.parse(await cli('explain',id,'--context'));
    const initial=await context();
    // 通常のPage操作で辿ったページを登録し、Code世代は変えない。
    // ページの中で別のページへ移る（ページのリンクを辿るのと同じ）。
    await evaluate(`(()=>{const frame=document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-frame');frame.src=new URL('/other?one=1',frame.src).href;return true})()`);
    await wait(`document.querySelector('#live-stage')?.textContent.includes('/other?one=1')`);
    let map;
    for(let attempt=0;attempt<50;attempt++) {map=await context();if(map.references.pages.some(p=>p.url==='/other?one=1'))break;await new Promise(r=>setTimeout(r,50));}
    assert.ok(map.references.pages.some(p=>p.url==='/other?one=1'));assert.equal(map.diff_display_version,initial.diff_display_version);
    const sections=[{id:'live',title:'Page explanation',blocks:[{id:'page',part:{type:'text',body:'Page link'},references:[{type:'page',url:'/other?one=1'}]}]}];
    await writeFile(join(root,'update.json'),JSON.stringify({expected_revision:0,base_display_version:initial.diff_display_version,sections}));
    await cli('explain',id,'--file',join(root,'update.json'));
    await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 1'))`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').checkVisibility()`),true);
    await browser('find','role','button','click','--name','Open explanation revision 1');
    await wait(`document.querySelector('[aria-label="Explanation"][role="region"]').checkVisibility()`);
    assert.equal(await evaluate(`document.querySelector('[aria-label="Review view"]').querySelectorAll('[aria-pressed="true"]').length`),1);
    await browser('find','role','button','click','--name','Page /other?one=1','--exact');
    await wait(`document.querySelector('#live-stage').checkVisibility()`);
    await browser('find','role','button','click','--name','Back to explanation');
    await browser('set','viewport','390','900');
    await browser('find','role','button','click','--name','Page /other?one=1','--exact');
    await browser('find','role','button','click','--name','Back to explanation');
    assert.equal(await evaluate(`document.querySelector('[aria-label="Explanation"][role="region"]').checkVisibility()`),true);
    console.log('PASS: live starts on Page without explanation, mid-review addition does not steal the view, Page registration keeps the Code generation and reference return works at 1280/390px');
    await stopKemi(running);
    // liveの保留は説明だけでも残り、新しいscan後も旧基点を保持する。
    running=await start(['--resume',id]);await browser('open',running.url);
    await wait(`document.querySelector('[aria-label="Explanation"][role="region"]')?.checkVisibility()`);
    const restored=await context();
    assert.equal(restored.explanation_revision,1);assert.notEqual(restored.diff_display_version,initial.diff_display_version);
    assert.equal(restored.explanation.base_display_version,initial.diff_display_version);
    await wait(`document.body.textContent.includes('Explanation for a different display version')`);
    console.log('PASS: actual live process suspend/resume keeps explanation revision/base and displays the new scan mismatch');
    // 説明の初期表示と Page/Code 往復でも、会話を閉じたまま一つの手渡し操作を使える。
    await stopKemi(running);
    const handSections=[{id:'handoff',title:'Handoff explanation',blocks:[{id:'question',part:{type:'text',body:'Explain before handing'},references:[]}]}];
    await writeFile(join(root,'initial-explanation.json'),JSON.stringify({sections:handSections}));
    running=await start(['--live',url,'--explanation',join(root,'initial-explanation.json')]);
    await browser('open',running.url);
    await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent==='Explanation' && e.getAttribute('aria-pressed')==='true')`);
    const visibleHand=`Array.from(document.querySelectorAll('button')).filter(e=>e.textContent.includes('Hand to agent') && e.checkVisibility())`;
    assert.equal(await evaluate(`${visibleHand}.length`),1,'initial live Explanation has one visible hand control with Conversation closed');
    for(const view of ['Page','Explanation','Code','Explanation']) {
      await browser('find','role','button','click','--name',view,...(view==='Code'?[]:['--exact']));
      assert.equal(await evaluate(`${visibleHand}.length`),1,`${view} has exactly one visible hand control`);
    }
    await selectText(`document.querySelector('[data-explanation-block="question"] .part-markdown p').firstChild`, 0, 7);
    await browser('press','c');
    await browser('find','label','Comment on Handoff explanation','fill','A question to hand from Explanation');
    await browser('find','role','button','click','--name','Add comment');
    await wait(`document.body.textContent.includes('A question to hand from Explanation')`);
    const handId=(await readdir(join(root,'kemi','sessions'))).find(name=>name.endsWith('.endpoint') && name.slice(0,-9)!==id).slice(0,-9);
    const waiting=cli('wait',handId,'--timeout','30');
    await wait(`${visibleHand}.some(e=>!e.disabled)`);
    await browser('find','role','button','click','--name','Hand to agent');
    const handed=JSON.parse(await waiting).events.filter(e=>e.type==='handed').flatMap(e=>e.comments.map(c=>c.comment));
    assert.equal(handed.length,1);
    assert.equal(handed[0].target_type,'explanation');
    assert.equal(handed[0].body,'A question to hand from Explanation');
    console.log('PASS: 390px initial live Explanation and Page/Code switches keep one Hand with Conversation closed; real wait receives the explanation comment');
  } finally {
    await close();
    if (running) await stopKemi(running);
    await new Promise(resolve=>dev.close(resolve));
  }
}

if (process.argv.includes('--range-navigation') || process.argv.length===3) {
  root=await temporaryDirectory('kemi-explanation-range-');
  await writeFile(join(root,'a.txt'),'old\n');await run('git',['init','-q',root]);await run('git',['-C',root,'add','a.txt']);
  const commit=()=>run('git',['-C',root,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qam','fixture']);
  await commit();const base=(await run('git',['-C',root,'rev-parse','HEAD'])).stdout.trim();
  await writeFile(join(root,'a.txt'),'new\n');await commit();const sha=(await run('git',['-C',root,'rev-parse','HEAD'])).stdout.trim();
  const ref=unit=>({type:'code',unit,group_id:unit==='file'?'all':sha,path:'a.txt',side:'new',start_line:null,end_line:null});
  const refs=[ref('file'),ref('commit')];
  await writeFile(join(root,'explanation.json'),JSON.stringify({sections:[{id:'range',title:'Range overview',blocks:[{id:'entry',part:{type:'text',body:'Both group units'},references:refs}]}]}));
  for(const unit of ['file','commit']) try {
    running=await start(['--from',base,'--group-by',unit]);await browser('set','viewport','1280','900');await browser('open',running.url);
    await wait(`document.querySelector('[aria-label="Review view"]')?.querySelector('[aria-pressed="true"]')?.textContent==='Code'`);
    assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent==='Explanation').disabled`),true);
    await stopKemi(running);
    running=await start(['--from',base,'--group-by',unit,'--explanation',join(root,'explanation.json')]);await browser('open',running.url);
    await wait(`document.body.textContent.includes('Both group units')`);
    assert.equal(await evaluate(`document.querySelector('[aria-label="Group unit"]').checkVisibility()`),false);
    const current=`Code a.txt (new) · ${unit==='file'?'Final state / all':`Per commit / ${sha}`}`;
    assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent===${JSON.stringify(current)})`),true);
    const other=unit==='file'?'commit':'file';
    const otherName=`Code a.txt (new) · ${other==='file'?'Final state / all':`Per commit / ${sha}`}`;
    await browser('find','role','button','click','--name',otherName,'--exact');
    await wait(`document.body.textContent.includes('Reference unavailable for this display version')`);
    await browser('find','role','button','click','--name','Close','--exact');
    await browser('set','viewport','390','900');
    assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).filter(e=>e.textContent.startsWith('Code a.txt')).every(e=>e.getBoundingClientRect().right<=innerWidth && e.getBoundingClientRect().left>=0)`),true);
    await browser('find','role','button','click','--name',current,'--exact');
    await wait(`document.querySelector('[aria-label="Group unit"]').checkVisibility()`);
    await browser('find','role','button','click','--name','Back to explanation');
    assert.equal(await evaluate(`document.querySelector('[aria-label="Group unit"]').checkVisibility()`),false);
    console.log(`PASS: range ${unit} starts on Code without explanation and Explanation with one, distinguishes reference group units, refuses an unloaded unit and returns without changing the reading revision`);
  } finally {
    await close();
    if (running) await stopKemi(running);
  }
}
