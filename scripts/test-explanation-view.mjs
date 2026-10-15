// 形式4の復元会話を実バイナリと実フォントで読む。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, readdir, unlink } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { promisify } from 'node:util';
import { createServer } from 'node:http';

const run = promisify(execFile);
const binary = resolve(process.argv[2]);
let root = await mkdtemp(join(tmpdir(), 'kemi-parts-'));
const session = `kemi-parts-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 8 * 1024 * 1024 });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async code => {
  if (code.includes('await import')) {
    code=`(async()=>{${/;\s*true\s*$/.test(code) ? code.replace(/;\s*true\s*$/, ';return true;') : `return (${code});`}})()`;
  }
  return (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
};
const wait = code => browser('wait', '--fn', code);
const frames = () => evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
/** 文字のノード（式の文字列）の [start, end) を選ぶ。 */
async function selectText(node, start, end) {
  await evaluate(`(()=>{const n=${node};n.parentElement.scrollIntoView({block:'center'});const r=document.createRange();r.setStart(n,${start});r.setEnd(n,${end});const s=getSelection();s.removeAllRanges();s.addRange(r)})()`);
  await frames();
}
async function start(args) {
  const child = spawn(binary, [...args, '--port', '0', '--no-open'], {
    cwd: root, env: { ...process.env, XDG_STATE_HOME: root, HOME: join(root, 'home') }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stderr = '';
  const exited = new Promise(resolve => child.on('exit', resolve));
  const url = await new Promise((resolve, reject) => {
    child.stderr.on('data', chunk => { stderr += chunk; const match = stderr.match(/^kemi: (http:\/\/\S+)/m); if (match) resolve(match[1]); });
    child.on('error', reject);
    child.on('exit', code => reject(new Error(`exit ${code}: ${stderr}`)));
  });
  return { child, url, exited };
}
const bodyA = '**A begins**\n\n![external](https://example.invalid/a.png) ![relative](a.png) ![data](data:image/png;base64,AA) ![reference][r]\n\n[r]: https://example.invalid/ref.png\n\n<img src="html.png">\n\nA ends\n';
const codeB = { type: 'code', body: '<script>literal B</script>\n', language: 'pseudocode', role: 'example', title: 'Code B' };
const parts = [{ type: 'text', body: bodyA }, codeB, { type: 'text', body: '**C survives**' }];
const sequence = { type: 'sequence', title: 'Sequence operations', role: 'example', participants: [{ id: 'a', label: 'Reader' }, { id: 'b', label: 'Server' }], events: [{ type: 'message', from: 'a', to: 'b', label: 'Request', kind: 'call' }, { type: 'message', from: 'b', to: 'a', label: 'Response', kind: 'return' }, { type: 'message', from: 'a', to: 'a', label: 'Self', kind: 'call' }] };
const grid = { type: 'diagram', title: 'Grid example', role: 'example', source: `a = ${'長いラベル'.repeat(30)}\nb = ? Pending\n| a | b |\na -> b : next` };
const flow = { type: 'flow', title: 'Flow operations', role: 'proposal', nodes: [{id:'a',kind:'process',label:'First'},{id:'b',kind:'process',label:'Second'}], edges:[{from:'a',to:'b'}] };
const outside = { ...flow, title:'Outside canvas', canvas:{width:64,height:64} };
const collision = { ...flow, title:'Label collision', edges:[{from:'a',to:'b',label:{text:'one',position:{x:300,y:180}}},{from:'a',to:'b',label:{text:'two',position:{x:300,y:180}}}] };
const intrusion = { ...flow, title:'Route intrusion', nodes:[...flow.nodes,{id:'c',kind:'process',label:'Middle',position:{x:160,y:180}}], edges:[{from:'a',to:'b',via:[]}] };
const framed = {...sequence,title:'Alt and loop',events:[{type:'alt',branches:[{condition:'yes',messages:[sequence.events[0]]},{condition:'no',messages:[sequence.events[1]]}]},{type:'loop',condition:'repeat',messages:[sequence.events[0]]}]};
const decision = {type:'flow',title:'Decision and return',role:'confirmed',nodes:[{id:'a',kind:'start',label:'Begin',position:{x:160,y:100}},{id:'b',kind:'decision',label:'Ready?',position:{x:160,y:300}},{id:'c',kind:'end',label:'End',position:{x:160,y:520}}],edges:[{from:'a',to:'b'},{from:'b',to:'c',label:{text:'yes'}},{from:'b',to:'a',from_port:'east',to_port:'east',via:[{x:400,y:300},{x:400,y:100}],label:{text:'retry',position:{x:440,y:200}}}]};
const touching = {type:'flow',title:'Allowed contact and crossing',role:'example',nodes:[{id:'a',kind:'process',label:'A',position:{x:160,y:100},width:180,height:64},{id:'b',kind:'process',label:'B',position:{x:340,y:100},width:180,height:64},{id:'c',kind:'process',label:'C',position:{x:160,y:400}},{id:'d',kind:'process',label:'D',position:{x:500,y:400}}],edges:[{from:'a',to:'d',from_port:'south',to_port:'north',via:[]},{from:'b',to:'c',from_port:'south',to_port:'north',via:[]}]};
let running;
if (!process.argv.includes('--navigation') && !process.argv.includes('--live-navigation') && !process.argv.includes('--range-navigation')) try {
  await writeFile(join(root, 'a.txt'), 'old\n');
  await run('git', ['init', '-q', root]);
  await run('git', ['-C', root, 'add', 'a.txt']);
  await run('git', ['-C', root, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']);
  await writeFile(join(root, 'a.txt'), 'new\n');
  running = await start(['--worktree']);
  const post = async (path, value) => {
    const response = await fetch(`${running.url}${path}`, { method: 'POST', headers: { Origin: new URL(running.url).origin, 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
    assert.equal(response.status, 200); return response.json();
  };
  const review = await (await fetch(`${running.url}api/review`)).json();
  const comment = await post('api/comment', {op:'add',file_id:review.groups[0].files[0].id,side:'new',body:'Comment stays'});
  await post('api/comment', {op:'reply',id:comment.id,body:'reply fixture'});
  await post('api/message', { body: '**old plain stays**' });
  await post('api/message', { body: 'parts fixture' });
  running.child.kill('SIGINT'); await running.exited;
  const dir = join(root, 'kemi', 'sessions');
  const name = (await readdir(dir)).find(name => name.endsWith('.session'));
  assert.ok(name);
  const file = join(dir, name);
  const raw = await readFile(file);
  assert.equal(raw[13], 4);
  const saved = JSON.parse(raw.subarray(18).toString());
  saved.state.messages[1].body = null;
  saved.state.messages[1].parts = [...parts, sequence, grid, flow, outside, collision, intrusion, framed, decision, touching];
  saved.state.comments[0].replies[0].body = null; saved.state.comments[0].replies[0].parts = parts;
  const json = Buffer.from(JSON.stringify(saved));
  const header = Buffer.from(raw.subarray(0, 18)); header.writeUInt32LE(json.length, 14);
  await writeFile(file, Buffer.concat([header, json]));
  running = await start(['--resume', name.slice(0, -8)]);
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
  await evaluate(`const original=window.fetch; window.fetch=(input,init)=> { let part; try {part=JSON.parse(init?.body)} catch {} if(part?.type==='text' && part.body===${JSON.stringify(bodyA)}) return Promise.reject(new TypeError('Network connection lost')); return original(input,init); }; true`);
  // 会話を閉じて開くと、通常のPart描画経路から再び取得する。
  await browser('find', 'role', 'button', 'click', '--name', 'Conversation (1 comment)');
  await browser('find', 'role', 'button', 'click', '--name', 'Open the conversation');
  await wait(`document.body.textContent.includes('Network connection lost')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('pre')).some(e=>e.textContent===${JSON.stringify(bodyA)})`), true);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='C survives') && document.body.textContent.includes('literal B')`), true);
  console.log('PASS: ordinary fetch failure preserves the complete original A and adjacent B/C');
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations')`);
   await evaluate(`document.body.style.zoom='1.5';true`);
  console.log('PASS: sequence measured with the actual browser font');
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Grid example')`);
  await browser('find', 'role', 'button', 'click', '--name', '長いラベル'.repeat(30));
  assert.equal(await evaluate(`document.body.textContent.includes(${JSON.stringify('長いラベル'.repeat(30))})`), true);
  assert.equal(await evaluate(`document.body.textContent.includes('Comment stays')`), true);
  const failures = await evaluate(`Array.from(document.querySelectorAll('pre')).map(e=>{try{return JSON.parse(e.textContent).title}catch{return null}}).filter(Boolean)`);
  assert.deepEqual(failures.sort(), ['Label collision','Outside canvas','Route intrusion'].sort());
  for(const title of ['Alt and loop','Decision and return','Allowed contact and crossing']) assert.equal(await evaluate(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')===${JSON.stringify(title)})`),true);
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
  console.log('PASS: grid full labels, real layout rejection, keyboard zoom/pan/fit, separate view return, font/zoom revalidation');
  await evaluate(`Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Comment stays')).click();true`);
  await wait(`document.body.textContent.includes('Network connection lost')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('pre')).some(e=>e.textContent===${JSON.stringify(bodyA)}) && document.body.textContent.includes('Comment stays') && Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='C survives')`),true);
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
  await browser('close').catch(() => {});
  if (running && running.child.exitCode === null) { running.child.kill('SIGINT'); await running.exited; }
}

if (!process.argv.includes('--parts-only') && !process.argv.includes('--live-navigation') && !process.argv.includes('--range-navigation')) try {
  root = await mkdtemp(join(tmpdir(), 'kemi-explanation-'));
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
  const review = await (await fetch(`${running.url}api/review`)).json();
  const id = (await readdir(join(root,'kemi','sessions'))).find(name=>name.endsWith('.endpoint')).slice(0,-9);
  const cli = async (...args)=>(await run(binary,args,{cwd:root,env:{...process.env,XDG_STATE_HOME:root,HOME:join(root,'home')}})).stdout;
  const context=async()=>JSON.parse(await cli('explain',id,'--context'));
  const update=async(sections,base=review.diff_display_version)=>{
    const current=await context();
    await writeFile(join(root,'update.json'),JSON.stringify({expected_revision:current.explanation_revision,base_display_version:base,sections}));
    return JSON.parse(await cli('explain',id,'--file',join(root,'update.json')));
  };
  const moduleState=`(await import(new URL('assets/state.js',location.href))).state`;
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
  const beforeEdit=(await (await fetch(`${running.url}api/review`)).json()).comments[0];
  await browser('find','role','button','click','--name','Edit','--exact');
  await browser('find','label','Comment','fill','Question remains','--exact');
  await browser('find','role','button','click','--name','Save','--exact');
  await wait(`!Array.from(document.querySelectorAll('textarea')).some(e=>e.getAttribute('aria-label')==='Comment') && document.body.textContent.includes('Question remains')`);
  const afterEdit=(await (await fetch(`${running.url}api/review`)).json()).comments[0];
  assert.equal(afterEdit.body,'Question remains');
  assert.deepEqual(afterEdit,{...beforeEdit,body:'Question remains'});
  assert.equal(await evaluate(`document.body.textContent.includes('Existing reply stays')`),true);
  console.log('PASS: UI correction keeps the same comment ID, original revision/block/full quote and existing replies');
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Go to Middle section','--exact');
   await evaluate(`window.readingDocument=document.querySelector('[aria-label="Explanation"][role="region"]');window.originalTop=readingDocument.scrollTop;window.originalCode=(${moduleState}).codeToken;true`);
  assert.ok(await evaluate('originalTop>0'));
  await browser('find','role','button','click','--name','Code a.txt (new)','--exact');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent==='Code' && e.getAttribute('aria-pressed')==='true')`);
  await update([{...sections[0],title:'Renamed heading'},sections[1]]);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 2'))`);
   assert.equal(await evaluate(`(${moduleState}).explanation.reading.revision===1 && (${moduleState}).codeToken===window.originalCode`),true);
  await browser('find','role','button','click','--name','Back to explanation');
  assert.equal(await evaluate(`readingDocument.scrollTop===window.originalTop && (${moduleState}).explanation.reading.revision===1`),true);
  await browser('find','role','button','click','--name','Open explanation revision 2');
  await wait(`document.body.textContent.includes('Renamed heading')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('article')).some(e=>e.textContent.includes('Question remains'))`),true);
  await evaluate(`const original=window.fetch; window.fetch=(input,init)=>{let part;try{part=JSON.parse(init?.body)}catch{};if(part?.type==='text' && part.body===${JSON.stringify(bodyA)}) return Promise.reject(new TypeError('Network connection lost'));return original(input,init)};true`);
  await update(sections);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 3'))`);
  await browser('find','role','button','click','--name','Open explanation revision 3');
  await wait(`document.body.textContent.includes('Network connection lost')`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('pre')).some(e=>e.textContent===${JSON.stringify(bodyA)}) && document.body.textContent.includes('Question remains') && document.body.textContent.includes('literal B') && Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='C survives')`),true);
  // AだけがCodeをD2へ取り込む。別tab BはD1の一覧を持ち続ける。
  await browser('tab','new',running.url);
  await wait(`document.body.textContent.includes('C survives')`);
   await evaluate(`window.bToken=(${moduleState}).codeToken;true`);
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
  assert.equal(await evaluate(`(${moduleState}).codeToken===${JSON.stringify(d2)}`),true);
  await browser('tab','t2');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 4') && e.textContent.includes('different display version'))`);
   assert.equal(await evaluate(`(${moduleState}).codeToken===window.bToken && (${moduleState}).explanation.reading.revision===3`),true);
  await browser('find','role','button','click','--name','Open explanation revision 4');
  await evaluate(`window.fetchCount=0;const original=window.fetch;window.fetch=(...args)=>{if(String(args[0]).includes('api/review'))fetchCount++;return original(...args)};const app=await import(new URL('assets/features/explanation.js',location.href));await app.receiveExplanation(null);const agent=await import(new URL('assets/features/agent.js',location.href));agent.receiveMissed();true`);
  await wait(`window.fetchCount>0`);
   assert.equal(await evaluate(`(${moduleState}).codeToken===window.bToken`),true);
  await evaluate(`document.body.style.zoom='1.25';true`);
  assert.equal(await evaluate(`(${moduleState}).codeToken===window.bToken && Array.from(document.querySelectorAll('p')).some(e=>e.textContent.includes('Explanation for a different display version'))`),true);
  await evaluate(`document.body.style.zoom='1';true`);
  await wait(`Array.from(document.querySelectorAll('svg[role="img"]')).some(e=>e.getAttribute('aria-label')==='Sequence operations')`);
  await evaluate(`const style=document.createElement('style');style.id='explanation-font-probe';style.textContent='.explanation-diagram {font-size:180px!important;font-family:serif!important}';document.head.append(style);true`);
  await wait(`Array.from(document.querySelectorAll('pre')).some(e=>{try{return JSON.parse(e.textContent).title==='Sequence operations'}catch{return false}})`);
  assert.equal(await evaluate(`(${moduleState}).codeToken===window.bToken && (${moduleState}).explanation.reading.revision===4 && Array.from(document.querySelectorAll('p')).some(e=>e.textContent.includes('Explanation for a different display version'))`),true);
  await evaluate(`document.querySelector('#explanation-font-probe').remove();true`);
  await browser('set','viewport','390','900');
  await browser('find','role','button','click','--name','Code a.txt (new)','--exact');
  await browser('find','role','button','click','--name','Back to explanation');
  assert.equal(await evaluate(`(${moduleState}).explanation.reading.revision===4 && (${moduleState}).view==='explanation'`),true);
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
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('textarea')).some(e=>e.value==='Draft across views') && (${moduleState}).allComments[0].explanation.revision===1 && (${moduleState}).entries.every(e=>!e.file.seen)`),true);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Open explanation revision 5');
  await update(sections,d2);
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent.includes('Open explanation revision 6'))`);
  await browser('find','role','button','click','--name','Open explanation revision 6');
  await wait(`Array.from(document.querySelectorAll('article')).some(e=>e.textContent.includes('Question remains'))`);
  assert.equal(await evaluate(`(${moduleState}).allComments[0].outdated`),false);
  await unlink(join(root,'b.txt'));
  await browser('find','role','button','click','--name','Code b.txt (new)','--exact');
  await wait(`document.querySelector('#overlay-card')?.textContent.includes('Reference unavailable for this display version')`);
  await browser('find','role','button','click','--name','Close','--exact');
  await browser('find','role','button','click','--name','Back to explanation');
  await unlink(join(root,'explanation.json'));
  running.child.kill('SIGINT');await running.exited;
  running=await start(['--resume',id]);await browser('open',running.url);
  await wait(`document.querySelector('[aria-label="Explanation"][role="region"]')?.checkVisibility()`);
  const frozen=await context();
  assert.equal(frozen.diff_display_version,review.diff_display_version);
  assert.equal(frozen.explanation.base_display_version,d2);
  assert.equal(frozen.explanation_revision,6);
  await wait(`document.body.textContent.includes('Explanation for a different display version')`);
  assert.equal(await evaluate(`(${moduleState}).allComments[0].explanation.revision===1 && (${moduleState}).allComments[0].explanation.quote.block.part.body===${JSON.stringify(bodyA)}`),true);
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
  assert.equal(await evaluate(`(${moduleState}).view==='explanation' && (${moduleState}).codeToken===${JSON.stringify(review.diff_display_version)}`),true);
  console.log('PASS: initial Explanation, base details, old reading position, unchanged comments, isolated Part fallback, two-screen Code tokens, missed notification recovery and 390px reference return');
  console.log('PASS: non-live process resume without the explanation input restores frozen Code, unchanged full quote and explanation base; unavailable/unknown/out-of-range references stay on the explanation with a reason');
} finally {
  await browser('close').catch(()=>{});
  if (running && running.child.exitCode===null) {running.child.kill('SIGINT');await running.exited;}
}

if (!process.argv.includes('--parts-only') && !process.argv.includes('--navigation') && !process.argv.includes('--range-navigation')) {
  root=await mkdtemp(join(tmpdir(),'kemi-explanation-live-'));
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
    const cli=async(...args)=>(await run(binary,args,{cwd:root,env:{...process.env,XDG_STATE_HOME:root,HOME:join(root,'home')}})).stdout;
    const context=async()=>JSON.parse(await cli('explain',id,'--context'));
    const initial=await context();
    // 通常のPage操作で辿ったページを登録し、Code世代は変えない。
    await evaluate(`const live=await import(new URL('assets/features/live.js',location.href));live.setLiveView('page','/other?one=1');true`);
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
    running.child.kill('SIGINT');await running.exited;
    // liveの保留は説明だけでも残り、新しいscan後も旧基点を保持する。
    running=await start(['--resume',id]);await browser('open',running.url);
    await wait(`document.querySelector('[aria-label="Explanation"][role="region"]')?.checkVisibility()`);
    const restored=await context();
    assert.equal(restored.explanation_revision,1);assert.notEqual(restored.diff_display_version,initial.diff_display_version);
    assert.equal(restored.explanation.base_display_version,initial.diff_display_version);
    await wait(`document.body.textContent.includes('Explanation for a different display version')`);
    console.log('PASS: actual live process suspend/resume keeps explanation revision/base and displays the new scan mismatch');
    // 説明の初期表示と Page/Code 往復でも、会話を閉じたまま一つの手渡し操作を使える。
    running.child.kill('SIGINT');await running.exited;
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
    await browser('close').catch(()=>{});
    if(running && running.child.exitCode===null){running.child.kill('SIGINT');await running.exited;}
    await new Promise(resolve=>dev.close(resolve));
  }
}

if (process.argv.includes('--range-navigation') || process.argv.length===3) {
  root=await mkdtemp(join(tmpdir(),'kemi-explanation-range-'));
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
    running.child.kill('SIGINT');await running.exited;
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
    await browser('close').catch(()=>{});
    if(running && running.child.exitCode===null){running.child.kill('SIGINT');await running.exited;}
  }
}
