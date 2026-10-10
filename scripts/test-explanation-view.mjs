// 形式4の復元会話を実バイナリと実フォントで読む。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, readFile, writeFile, readdir } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { promisify } from 'node:util';

const run = promisify(execFile);
const binary = resolve(process.argv[2]);
const root = await mkdtemp(join(tmpdir(), 'kemi-parts-'));
const session = `kemi-parts-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 8 * 1024 * 1024 });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async code => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
const wait = code => browser('wait', '--fn', code);
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
try {
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
  assert.equal(await evaluate(`document.body.textContent.includes('**old plain stays**')`), true);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('strong')).some(e=>e.textContent==='old plain stays')`), false);
  assert.equal(await evaluate(`document.body.textContent.includes('Image not loaded') && document.body.textContent.includes('https://example.invalid/ref.png')`), true);
  await browser('find', 'role', 'button', 'click', '--name', 'Copy code');
  await wait(`Array.from(document.querySelectorAll('button')).some(e=>e.textContent==='Copied')`);
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
  await evaluate(`window.beforeZoom=Array.from(document.querySelectorAll('svg[role="img"]')).find(e=>e.getAttribute('aria-label')==='Sequence operations').querySelector('text');document.body.style.zoom='1.5';true`);
  await wait(`window.beforeZoom && !window.beforeZoom.isConnected`);
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
  await browser('press', '+'); const zoomed = await transform(); assert.ok(zoomed.includes('1.2'));
  await browser('press', '0'); assert.notEqual(await transform(), zoomed);
  await evaluate(`Array.from(document.querySelectorAll('header')).find(e=>e.textContent.includes('Sequence operations')).parentElement.querySelectorAll('button').forEach(e=>{if(e.textContent==='Open diagram separately')e.click()}); true`);
  await wait(`document.querySelector('dialog')?.open`);
  await browser('find', 'role', 'button', 'click', '--name', 'Close diagram');
  assert.equal(await evaluate(`document.activeElement.textContent==='Open diagram separately'`), true);
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
} finally {
  await browser('close').catch(() => {});
  if (running && running.child.exitCode === null) { running.child.kill('SIGINT'); await running.exited; }
}
