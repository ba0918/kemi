// 説明本文の文字を選んでその場でコメントし、選んだ文字が引用として result まで届くことを実バイナリで確かめる。
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {mkdtemp,writeFile,readdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {promisify} from 'node:util';

import {attachToPage,browserSession,isVisible as visible} from './lib/browser.mjs';
import {getJson,isolatedEnvironment,launchKemi,postJson,stopKemi} from './lib/kemi.mjs';

const run=promisify(execFile), binary=resolve(process.argv[2]);
const root=await mkdtemp(join(tmpdir(),'kemi-selection-')), session=`kemi-selection-${process.pid}`;
const env=isolatedEnvironment(root);
const {browser,evaluate,waitFor:wait,close}=browserSession(session);
const frames=()=>evaluate(`new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
const popup=`Array.from(document.querySelectorAll('button')).find(e=>e.getAttribute('aria-label')==='Comment on selection')`;
const hint=`Array.from(document.querySelectorAll('[role="status"]')).find(e=>e.textContent.includes('cannot span blocks'))`;
const editor=`document.querySelector('form[aria-label="New explanation comment"]')`;
const block=id=>`document.querySelector('[data-explanation-block="${id}"]')`;
/** 文字のノードの [start, end) を選ぶ。node は式の文字列。 */
async function selectText(node,start,end,endNode=node,endOffset=end) {
  await evaluate(`(()=>{const a=${node},b=${endNode};a.parentElement.scrollIntoView({block:'center'});const r=document.createRange();r.setStart(a,${start});r.setEnd(b,${endOffset});const s=getSelection();s.removeAllRanges();s.addRange(r)})()`);
  await frames();
}
const highlighted=()=>evaluate(`Array.from(CSS.highlights.get('explanation-comment')??[]).map(r=>({text:r.toString(),node:r.startContainer.nodeValue,offset:r.startOffset}))`);
const ref={type:'code',unit:null,group_id:'worktree',path:'a.txt',side:'new',start_line:null,end_line:null};
const first={id:'kept-text',part:{type:'text',body:'Selection starts here with **emphasis**.\n\nRead the intent, then the intent again.\n\n- List item one\n- List item two'},references:[ref]};
const second={id:'other-text',part:{type:'text',body:'A separate block must never be guessed as the selection target.'},references:[]};
const code={id:'literal-code',part:{type:'code',title:'Read-only example',language:'pseudocode',role:'example',body:'do_not_execute()\nsecond_line()\n'},references:[]};
const diagram={id:'diagram',part:{type:'sequence',title:'Example exchange',role:'example',participants:[{id:'a',label:'Reader'},{id:'b',label:'Server'}],events:[{type:'message',from:'a',to:'b',label:'Request',kind:'call'}]},references:[]};
const grid={id:'grid',part:{type:'diagram',title:'Grid map',role:'example',source:'a = Start\nb = Finish\n| a | b |\na -> b'},references:[]};
const tail={id:'tail',part:{type:'text',body:`${'Filler sentence for scrolling. '.repeat(60)}\n\nTail paragraph to comment on.\n\n${'More filler after the tail. '.repeat(60)}`},references:[]};
const sections=[{id:'one-section',title:'Selection review',blocks:[first,second,code,diagram,grid]},{id:'later',title:'Later section',blocks:[tail]}];
const firstParagraph=`${block('kept-text')}.querySelector('.part-markdown p')`;
const intentParagraph=`${block('kept-text')}.querySelectorAll('.part-markdown p')[1]`;
let kemi, page;
try {
  await writeFile(join(root,'a.txt'),'old\n');await run('git',['init','-q',root]);await run('git',['-C',root,'add','a.txt']);await run('git',['-C',root,'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture']);await writeFile(join(root,'a.txt'),'new\n');
  await writeFile(join(root,'explanation.json'),JSON.stringify({sections}));
  kemi=await launchKemi(binary,{cwd:root,env,args:['--worktree','--explanation',join(root,'explanation.json')]});
  const url=kemi.url;
  const review=()=>getJson(url,'api/review');
  await browser('set','viewport','1280','900');await browser('open',url);
  await wait(`!!document.querySelector('.part-markdown strong') && !!${block('literal-code')}`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('button')).some(e=>/^Comment on .* · Block \\d+$/.test(e.getAttribute('aria-label')??''))`),false,'no block has a persistent comment control');

  // 段落の途中の数文字を、通常のポインタ操作で選ぶ。
  const chosen='starts here';
  const points=await evaluate(`(()=>{const n=${firstParagraph}.firstChild,i=n.nodeValue.indexOf(${JSON.stringify(chosen)}),r=document.createRange();r.setStart(n,i);r.setEnd(n,i+${chosen.length});const b=r.getBoundingClientRect();return {x:Math.round(b.left+1),y:Math.round(b.top+b.height/2),end:Math.round(b.right-1),bottom:b.bottom}})()`);
  await browser('mouse','move',String(points.x),String(points.y));await browser('mouse','down');await browser('mouse','move',String(points.end),String(points.y));await browser('mouse','up');
  await frames();
  assert.equal(await evaluate(`getSelection().toString()`),chosen,'the pointer selected the intended characters');
  assert.ok(await evaluate(visible(popup)),'selecting text offers a comment button');
  const near=await evaluate(`(()=>{const b=${popup}.getBoundingClientRect();return b.top-${points.bottom}})()`);
  assert.ok(near>=0 && near<40,`the button sits right below the selection (${near})`);
  await browser('find','role','button','click','--name','Comment on selection','--exact');
  await wait(`document.activeElement.tagName==='TEXTAREA'`);
  const placed=await evaluate(`(()=>{const f=${editor}.getBoundingClientRect();return {gap:f.top-${points.bottom},quote:${editor}.querySelector('blockquote').textContent,head:${editor}.textContent}})()`);
  assert.ok(placed.gap>=0 && placed.gap<24,`the editor opens right below the selected line (${placed.gap})`);
  assert.equal(placed.quote,chosen,'the editor quotes the selected characters');
  assert.ok(placed.head.includes('Selection review'),'the editor names the section heading');
  await browser('find','label','Comment on Selection review','fill','Why start here?');
  // 一件の保存を通信境界で待たせ、送信後に書いた本文を保存結果と区別する。
  await evaluate(`(()=>{const original=window.fetch;window.fetch=async(input,init)=>{let value;try{value=JSON.parse(init?.body)}catch{};if(value?.op==='add_explanation' && value.body==='Why start here?'){window.fetch=original;await new Promise(resolve=>window.releaseCommentSave=resolve);}return original(input,init);}})()`);
  await browser('find','role','button','click','--name','Add comment','--exact');
  await wait(`typeof window.releaseCommentSave==='function'`);
  await browser('find','label','Comment on Selection review','fill','New unsent draft written while saving');
  await evaluate(`window.releaseCommentSave()`);
  await wait(`document.body.textContent.includes('Why start here?')`);
  const saved=(await review()).comments[0];
  assert.deepEqual(saved.explanation.selection,{text:chosen,occurrence:0});
  assert.deepEqual(saved.explanation.quote,{section_title:'Selection review',block:first},'the whole block is still the saved quote');
  assert.equal(saved.body,'Why start here?');
  assert.ok((await highlighted()).some(r=>r.text===chosen),'the selected characters are marked');
  assert.equal(await evaluate(`(()=>{const c=Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Why start here?'));return c.parentElement.previousElementSibling===${firstParagraph}})()`),true,'the card sits directly below the paragraph');
  assert.equal(await evaluate(`${editor}?.querySelector('textarea').value`),'New unsent draft written while saving','a successful save keeps newer unsent text open');
  await browser('press','Escape');
  assert.equal(await evaluate(`Boolean(${editor})`),false,'Escape closes the editor');
  await selectText(`${firstParagraph}.firstChild`,0,9);
  await browser('press','c');
  await wait(`document.activeElement.tagName==='TEXTAREA'`);
  assert.equal(await evaluate(`document.activeElement.value`),'New unsent draft written while saving','the block draft survives closing and reopening');
  await browser('press','Escape');
  console.log('PASS: pointer selection, button and editor right below the line, quote of the selected text, delayed save keeps the draft, mark and card below the paragraph');

  // 同じ語が二回ある段落で二回目を選ぶ。
  const intentText=await evaluate(`${intentParagraph}.firstChild.nodeValue`);
  const secondIntent=intentText.lastIndexOf('intent');
  await selectText(`${intentParagraph}.firstChild`,secondIntent,secondIntent+6);
  await browser('press','Control+c');await frames();
  assert.equal(await evaluate(`Boolean(${editor})`),false,'Ctrl+C copies instead of opening the editor');
  await browser('press','c');
  await wait(`document.activeElement.tagName==='TEXTAREA'`);
  await browser('find','label','Comment on Selection review','fill','The second intent');
  await browser('find','role','button','click','--name','Add comment','--exact');
  await wait(`document.body.textContent.includes('The second intent')`);
  assert.deepEqual((await review()).comments[1].explanation.selection,{text:'intent',occurrence:1});
  await browser('open',url);
  await wait(`!!document.querySelector('.part-markdown strong') && document.body.textContent.includes('The second intent')`);
  await wait(`(CSS.highlights.get('explanation-comment')?.size??0)>=2`);
  const marks=await highlighted();
  assert.ok(marks.some(r=>r.text==='intent' && r.node===intentText && r.offset===secondIntent),'after reloading, the second occurrence is marked');
  assert.ok(!marks.some(r=>r.text==='intent' && r.offset===intentText.indexOf('intent')),'the first occurrence is not marked');
  console.log('PASS: Ctrl+C does not open, c opens; the second of two identical words saves occurrence 1 and is marked again after reload');

  // 入力欄は本文と一緒にスクロールする。
  const tailParagraph=`${block('tail')}.querySelectorAll('.part-markdown p')[1]`;
  await selectText(`${tailParagraph}.firstChild`,0,4);
  await browser('press','c');await wait(`Boolean(${editor})`);
  const offset=()=>evaluate(`${editor}.getBoundingClientRect().top-${tailParagraph}.getBoundingClientRect().top`);
  const before=await offset();
  await evaluate(`document.querySelector('[aria-label="Explanation"][role="region"]').scrollTop+=60`);await frames();
  assert.ok(Math.abs(await offset()-before)<1,'the editor scrolls together with the text');
  // 幅が変わって本文が折り返し直しても、入力欄は選んだ行のすぐ下に付いていく。
  await browser('set','viewport','960','900');await frames();
  const followed=await evaluate(`(()=>{const n=${tailParagraph}.firstChild,r=document.createRange();r.setStart(n,0);r.setEnd(n,4);return ${editor}.getBoundingClientRect().top-r.getBoundingClientRect().bottom})()`);
  assert.ok(followed>=0 && followed<24,`the editor stays right below the selected line after the text reflows (${followed})`);
  await browser('set','viewport','1280','900');await frames();
  await browser('press','Escape');
  // code パーツでもボタンが出る。ブロックをまたぐ選択では理由が出る。
  await selectText(`${block('literal-code')}.querySelector('code').firstChild`,0,6);
  assert.ok(await evaluate(visible(popup)),'code text offers a comment button');
  await selectText(`${firstParagraph}.firstChild`,2,4,`${block('other-text')}.querySelector('.part-markdown p').firstChild`,5);
  assert.equal(await evaluate(visible(popup)),false,'a cross-block selection has no comment button');
  assert.ok(await evaluate(visible(hint)),'a cross-block selection explains why it cannot be commented');
  await selectText(`${firstParagraph}.firstChild`,0,4);await browser('press','Escape');
  assert.equal(await evaluate(visible(popup)),false,'Escape dismisses the button');
  console.log('PASS: editor scrolls with the text; code selection offers the button; cross-block selection shows the reason');

  // 図の操作はポインタとフォーカスで出入りし、ホバーできない端末では常に出る。
  page=await attachToPage(browser);
  const {cdp}=page;
  // Chrome の Emulation.setEmulatedMedia は hover を変えられない（調べた結果）。この環境のブラウザは hover: none
  // なので、ホバーできる端末の見え方は (hover: none) の規則を一時的に外して確かめる。
  assert.equal(await evaluate(`matchMedia('(hover: none)').matches`),true,'this browser reports a device without hover');
  const hoverRule=`(()=>{for(const sheet of document.styleSheets){let rules;try{rules=Array.from(sheet.cssRules)}catch{continue}const index=rules.findIndex(r=>r instanceof CSSMediaRule && r.conditionText.replace(/\\s/g,'')==='(hover:none)' && r.cssText.includes('part-diagram-controls'));if(index>=0)return {sheet,index,text:rules[index].cssText};}return null})()`;
  const hover=async value=>{
    if (value==='hover') assert.equal(await evaluate(`(()=>{const rule=${hoverRule};if(!rule)return false;window.hoverNoneRule=rule;rule.sheet.deleteRule(rule.index);return true})()`),true,'the hover: none rule for diagram controls exists');
    else await evaluate(`(()=>{const rule=window.hoverNoneRule;if(rule){rule.sheet.insertRule(rule.text,rule.index);window.hoverNoneRule=null}return true})()`);
  };
  const tools=`document.querySelector('[aria-label="Example exchange view tools"]')`;
  const opacity=async()=>{await evaluate(`new Promise(resolve=>setTimeout(resolve,200))`);return evaluate(`Number(getComputedStyle(${tools}).opacity)`);};
  await evaluate(`${block('diagram')}.scrollIntoView({block:'center'});true`);
  await wait(`Boolean(${tools}) && !document.querySelector('[aria-label="Example exchange view tools"] [aria-label="Zoom in"]').closest('[hidden]')`);
  await hover('hover');await browser('mouse','move','5','5');await frames();
  assert.equal(await opacity(),0,'diagram controls are hidden while the pointer is elsewhere');
  const surfaceBox=await evaluate(`(()=>{const b=document.querySelector('[aria-label="Example exchange diagram controls"]').getBoundingClientRect();return {x:Math.round(b.x+b.width/2),y:Math.round(b.y+b.height/2)}})()`);
  await browser('mouse','move',String(surfaceBox.x),String(surfaceBox.y));await frames();
  assert.equal(await opacity(),1,'pointing at the diagram shows its controls');
  await browser('mouse','move','5','5');await frames();
  await evaluate(`document.querySelector('[aria-label="Example exchange diagram controls"]').focus();true`);await frames();
  assert.equal(await opacity(),1,'focus inside the diagram shows its controls');
  const transform=()=>evaluate(`document.querySelector('svg[aria-label="Example exchange"]').style.transform`);
  const panned=await transform();await browser('press','ArrowRight');
  assert.notEqual(await transform(),panned,'arrow keys pan the inline diagram');
  // 図の上で押してドラッグしても、文字は選ばれず図が動く。1 回目は文字の上から、2 回目は図が動いたあとの同じ点（文字の手前の空き）から押す。
  const label=await evaluate(`(()=>{const t=Array.from(document.querySelectorAll('svg[aria-label="Example exchange"] text')).find(e=>e.textContent==='Request').getBoundingClientRect();return {x:Math.round(t.x+t.width/2),y:Math.round(t.y+t.height/2)}})()`);
  for (const round of [1,2]) {
    const before=await transform();
    await browser('mouse','move',String(label.x),String(label.y));await browser('mouse','down');
    for (const step of [4,20,40,60])await browser('mouse','move',String(label.x+step),String(label.y+step/2));
    await browser('mouse','up');await frames();
    assert.equal(await evaluate(`getSelection().toString()`),'',`dragging on the diagram selects nothing (round ${round})`);
    assert.notEqual(await transform(),before,`dragging on the diagram pans it (round ${round})`);
  }
  await browser('mouse','move','5','5');
  await evaluate(`document.activeElement.blur();true`);await frames();
  assert.equal(await opacity(),0);
  await hover('none');await frames();
  assert.equal(await opacity(),1,'controls are always shown without hover');
  assert.equal(await evaluate(`Boolean(${tools}.querySelector('[aria-label="Move right"]') || ${tools}.querySelector('[aria-label="Reset diagram"]'))`),false,'inline diagrams have no pan or reset buttons');
  await evaluate(`window.diagramSource=${tools}.querySelector('[aria-label="Open diagram separately"]');window.diagramSource.click();true`);
  await wait(`document.querySelector('dialog')?.open`);
  assert.equal(await evaluate(`Boolean(document.querySelector('dialog [aria-label="Move right"]') && document.querySelector('dialog [aria-label="Reset diagram"]'))`),true,'the separate view keeps pan and reset buttons');
  await browser('find','role','button','click','--name','Close diagram');
  assert.equal(await evaluate(`!document.querySelector('dialog') && document.activeElement===window.diagramSource`),true,'closing the separate view returns to the diagram');
  // 図のコメントのボタンはキーボードとタッチで押せ、引用が図の見出しになる。
  await evaluate(`document.querySelector('[aria-label="Comment on Example exchange"]').focus();true`);
  await browser('press','Enter');
  await wait(`document.activeElement.tagName==='TEXTAREA'`);
  assert.equal(await evaluate(`${editor}.querySelector('blockquote').textContent`),'Example exchange');
  const below=await evaluate(`${editor}.getBoundingClientRect().top-document.querySelector('[aria-label="Comment on Example exchange"]').getBoundingClientRect().bottom`);
  assert.ok(below>=0 && below<24,`the diagram editor opens right below its button (${below})`);
  await browser('press','Escape');
  assert.equal(await evaluate(`document.activeElement.getAttribute('aria-label')`),'Comment on Example exchange','Escape returns focus to the diagram button');
  await evaluate(`${block('grid')}.scrollIntoView({block:'center'});true`);await frames();
  const gridButton=await evaluate(`(()=>{const b=document.querySelector('[aria-label="Comment on Grid map"]').getBoundingClientRect();return {x:b.x+b.width/2,y:b.y+b.height/2}})()`);
  await cdp('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:gridButton.x,y:gridButton.y}]});
  await cdp('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await wait(`Boolean(${editor})`);
  assert.equal(await evaluate(`${editor}.querySelector('blockquote').textContent`),'Grid map','touch opens the diagram editor with the diagram title');
  await browser('find','label','Comment on Selection review','fill','Grid question');
  await browser('find','role','button','click','--name','Add comment','--exact');
  await wait(`document.body.textContent.includes('Grid question')`);
  assert.equal((await review()).comments[2].explanation.selection,null);
  assert.equal(await evaluate(`(()=>{const c=Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('Grid question'));return c.parentElement.parentElement===${block('grid')}})()`),true,'a diagram comment shows a card below the block');
  console.log('PASS: diagram controls follow hover/focus and stay without hover; arrow-key pan; separate view keeps pan buttons and returns; diagram comment by keyboard and touch quotes the title');

  // 本文に無い text の selection は、ブロック全体のコメントとして出す。
  await postJson(url,'api/comment',{op:'add_explanation',explanation_revision:1,block_id:'other-text',body:'No match here',selection:{text:'absent words',occurrence:0}});
  await browser('open',url);
  await wait(`document.body.textContent.includes('No match here') && !!document.querySelector('.part-markdown strong')`);
  assert.equal(await evaluate(`(()=>{const c=Array.from(document.querySelectorAll('button')).find(e=>e.textContent.includes('No match here'));return c.parentElement.parentElement===${block('other-text')}})()`),true,'an unmatched selection falls back to a card below the block');
  assert.deepEqual((await review()).comments[3].explanation.selection,{text:'absent words',occurrence:0},'the unmatched selection is kept unchanged');
  // 札を押すと会話パネルでスレッドが開き、選んだ文字を引用として出す。
  await evaluate(`Array.from(document.querySelectorAll('[data-explanation-block] button')).find(e=>e.textContent.includes('Why start here?')).click();true`);
  await wait(`Array.from(document.querySelectorAll('blockquote')).some(e=>e.getAttribute('aria-label')==='Selected text' && e.textContent===${JSON.stringify(chosen)} && e.checkVisibility())`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('.cv-context')).some(e=>e.checkVisibility() && e.textContent.includes('List item two'))`),true,'the whole original block is still readable in the thread');
  await browser('find','role','button','click','--name','Close','--exact');
  console.log('PASS: unmatched selections show at block level unchanged; a card opens its thread, which quotes the selected text and keeps the whole block');

  // 旧版を読んでいる間の選択は旧版のブロックを対象にする。
  const id=(await readdir(join(root,'kemi','sessions'))).find(n=>n.endsWith('.endpoint')).slice(0,-9);
  const context=JSON.parse((await run(binary,['explain',id,'--context'],{cwd:root,env})).stdout);
  const updated=[{...sections[0],blocks:[{...first,part:{type:'text',body:'New revision text'}},second,code,diagram,grid]},sections[1]];
  await writeFile(join(root,'update.json'),JSON.stringify({expected_revision:1,base_display_version:context.diff_display_version,sections:updated}));
  await run(binary,['explain',id,'--file',join(root,'update.json')],{cwd:root,env});
  await wait(`document.body.textContent.includes('Open explanation revision 2')`);
  await selectText(`${firstParagraph}.firstChild`,0,9);await browser('press','c');
  await browser('find','label','Comment on Selection review','fill','Old reading still targets old block');await browser('find','role','button','click','--name','Add comment','--exact');
  await wait(`document.body.textContent.includes('Old reading still targets old block')`);
  const old=(await review()).comments[4];assert.equal(old.explanation.revision,1);assert.deepEqual(old.explanation.quote.block,first);assert.equal(old.outdated,true);
  await browser('find','role','button','click','--name','Open explanation revision 2');
  await wait(`document.body.textContent.includes('New revision text')`);

  // 390px でも同じ操作ができ、入力欄が表示領域と手渡しバーの外にはみ出さない。
  await browser('set','viewport','390','900');
  await wait(`!!${block('kept-text')}?.querySelector('.part-markdown p')`);
  await selectText(`${block('kept-text')}.querySelector('.part-markdown p').firstChild`,4,12);
  await wait(visible(popup));
  assert.ok(await evaluate(`(()=>{const b=${popup}.getBoundingClientRect(),v=visualViewport;return b.left>=v.offsetLeft && b.top>=v.offsetTop && b.right<=v.offsetLeft+v.width && b.bottom<=v.offsetTop+v.height})()`),'the button stays in the viewport at 390px');
  await browser('press','c');
  await wait(`document.activeElement.tagName==='TEXTAREA'`);
  await frames();
  const narrow=await evaluate(`(()=>{const f=${editor}.getBoundingClientRect(),d=document.querySelector('[aria-label="Explanation"][role="region"]'),r=d.getBoundingClientRect(),hand=document.querySelector('#hand-float-bar').getBoundingClientRect(),v=visualViewport;return {bottom:f.bottom,limit:Math.min(r.bottom,hand.height?hand.top:Infinity,v.offsetTop+v.height),inside:f.left>=v.offsetLeft && f.right<=v.offsetLeft+v.width}})()`);
  assert.ok(narrow.inside,'the editor stays inside the viewport at 390px');
  assert.ok(narrow.bottom<=narrow.limit+1,`the editor is not hidden behind the hand bar (${narrow.bottom} / ${narrow.limit})`);
  await browser('find','label','Comment on Selection review','fill','Narrow question');await browser('find','role','button','click','--name','Add comment','--exact');
  await wait(`document.body.textContent.includes('Narrow question')`);
  const narrowComment=(await review()).comments[5];
  assert.equal(narrowComment.explanation.revision,2);
  assert.deepEqual(narrowComment.explanation.selection,{text:'revision',occurrence:0});
  console.log('PASS: old revision target; 390px selection button, c, editor inside the viewport and above the hand bar, and a saved selection');
} finally {
  page?.close();
  await close();
  if(kemi) await stopKemi(kemi);
}
