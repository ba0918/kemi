import test from 'node:test';
import assert from 'node:assert/strict';
import { freshness, resolveCodeReference, commentsForBlock, sectionCommentCounts, selectionAt, locateSelection, selectionTooLong, readingText, readingOffset, readingPoint } from './explanation-model.js';

test('reading and notification freshness use their own base and the screen Code token', () => {
  assert.equal(freshness('review:1', null), 'pending');
  assert.equal(freshness('review:1', 'review:1'), 'matching');
  assert.equal(freshness('review:2', 'review:1'), 'different');
});
const reference = {type:'code',unit:null,group_id:'all',path:'a',side:'new',start_line:null,end_line:null};
const review = {unit:null,diff_display_version:'r:1',groups:[{id:'all',files:[{id:'f',path:'a',status:'modify'}]}]};
test('Code references resolve only in the screen list of the same generation', () => {
  assert.equal(resolveCodeReference(reference, review, 'r:1').entry.file.id, 'f');
  assert.ok(resolveCodeReference(reference, review, 'r:2').reason);
  assert.ok(resolveCodeReference({...reference,path:'missing'}, review, 'r:1').reason);
  assert.ok(resolveCodeReference({...reference,side:'old'}, {...review,groups:[{id:'all',files:[{id:'f',path:'a',status:'add'}]}]}, 'r:1').reason);
});
test('line references never infer unknown totals or shorten a requested range', () => {
  const line = {...reference,start_line:2,end_line:3};
  assert.ok(resolveCodeReference(line, review, 'r:1').reason);
  assert.ok(resolveCodeReference(line, review, 'r:1', {new_total:2}).reason);
  assert.equal(resolveCodeReference(line, review, 'r:1', {new_total:3}).reason, null);
});

test('an explicitly absent side is unavailable even when the entry is labelled modified', () => {
  const withoutOld={...review,groups:[{id:'all',files:[{id:'f',path:'a',status:'modify',old_present:false,new_present:true}]}]};
  assert.ok(resolveCodeReference({...reference,side:'old'},withoutOld,'r:1').reason);
  assert.equal(resolveCodeReference(reference,withoutOld,'r:1').reason,null);
});
test('a block shows its original comments and unchanged comments from earlier revisions', () => {
  const block = {id:'b',part:{type:'text',body:'exact\r\n'},references:[]};
  const comment = {id:'c',target_type:'explanation',explanation:{revision:1,block_id:'b',quote:{section_title:'Old',block}}};
  assert.deepEqual(commentsForBlock([comment], 2, block), [comment]);
  assert.deepEqual(commentsForBlock([comment], 2, {...block,part:{type:'text',body:'exact\n'}}), []);
  assert.deepEqual(commentsForBlock([comment], 1, block), [comment]);
});

test('each section counts the comments shown beside its blocks in the reading revision', () => {
  const kept = {id:'kept',part:{type:'text',body:'same'},references:[]};
  const changed = {id:'changed',part:{type:'text',body:'before'},references:[]};
  const quiet = {id:'quiet',part:{type:'text',body:'none'},references:[]};
  const on = (/** @type {any} */ block, /** @type {number} */ revision, extra = {}) => ({id:`c-${block.id}-${revision}`,target_type:'explanation',explanation:{revision,block_id:block.id,quote:{section_title:'S',block}},...extra});
  const comments = [on(kept,1), on(kept,2,{resolved:true}), on(changed,1), {id:'file',target_type:'file',explanation:null}];
  const version = {revision:2,sections:[
    {id:'first',blocks:[kept]},
    {id:'second',blocks:[{...changed,part:{type:'text',body:'after'}}]},
    {id:'third',blocks:[quiet]},
  ]};
  const counts = sectionCommentCounts(comments, version);
  assert.equal(counts.get('first'), 2);
  assert.equal(counts.get('second'), 0, 'a comment whose block content changed is outdated here and not shown');
  assert.equal(counts.get('third'), 0);
});

test('a selection records which match of its text it is, counting overlapping matches', () => {
  const text = 'Read the intent, then the intent again.';
  const second = text.lastIndexOf('intent');
  assert.deepEqual(selectionAt(text, second, second + 'intent'.length), {text:'intent',occurrence:1});
  assert.deepEqual(selectionAt(text, text.indexOf('intent'), text.indexOf('intent') + 6), {text:'intent',occurrence:0});
  assert.deepEqual(selectionAt('aaaa', 1, 3), {text:'aa',occurrence:1});
  assert.deepEqual(selectionAt('aaaa', 2, 4), {text:'aa',occurrence:2});
});

test('a saved selection finds the same match again or reports none', () => {
  const text = 'Read the intent, then the intent again.';
  assert.deepEqual(locateSelection(text, {text:'intent',occurrence:1}), {start:text.lastIndexOf('intent'),end:text.lastIndexOf('intent') + 6});
  assert.deepEqual(locateSelection('aaaa', {text:'aa',occurrence:2}), {start:2,end:4});
  assert.equal(locateSelection(text, {text:'intent',occurrence:2}), null);
  assert.equal(locateSelection(text, {text:'absent',occurrence:0}), null);
});

test('only selections over 65,536 UTF-8 bytes are too long to quote', () => {
  assert.equal(selectionTooLong('あ'.repeat(21_845) + 'x'), false);
  assert.equal(selectionTooLong('あ'.repeat(21_846)), true);
});

/** 画面が後から入れる要素を印で区別する、DOM に相当する木。 */
const textNode = (/** @type {string} */ value) => ({nodeType:3,nodeValue:value,childNodes:[]});
const element = (/** @type {any[]} */ children, injected = false) => ({nodeType:1,nodeValue:null,childNodes:children,injected});
const skip = (/** @type {any} */ node) => node.injected;

test('the reading text leaves out cards and editors the screen inserted', () => {
  const card = element([textNode('word card')], true);
  const root = element([element([element([textNode('word one'), card]), element([textNode('word two')])])]);
  assert.equal(readingText(root, skip), 'word oneword two');
});

test('a second comment in a list item with a card keeps the same occurrence', () => {
  const second = textNode('word two');
  const card = element([textNode('word card')], true);
  const root = element([element([element([textNode('word one'), card]), element([second])])]);
  const text = readingText(root, skip);
  const start = readingOffset(root, second, 0, skip);
  const end = readingOffset(root, second, 4, skip);
  assert.deepEqual(selectionAt(text, start ?? -1, end ?? -1), {text:'word',occurrence:1});
});

test('element boundaries and positions outside the root convert to reading offsets', () => {
  const first = textNode('abc'), card = element([textNode('xyz')], true), last = textNode('def');
  const item = element([first, card, last]);
  const root = element([item]);
  assert.equal(readingOffset(root, item, 0, skip), 0);
  assert.equal(readingOffset(root, item, 2, skip), 3);
  assert.equal(readingOffset(root, item, 3, skip), 6);
  assert.equal(readingOffset(root, textNode('elsewhere'), 0, skip), null);
});

test('reading offsets map back to text positions without entering inserted elements', () => {
  const first = textNode('abc'), card = element([textNode('xyz')], true), last = textNode('def');
  const root = element([element([first, card, last])]);
  assert.deepEqual(readingPoint(root, 1, skip, 'start'), {node:first,offset:1});
  assert.deepEqual(readingPoint(root, 3, skip, 'start'), {node:last,offset:0});
  assert.deepEqual(readingPoint(root, 3, skip, 'end'), {node:first,offset:3});
  assert.deepEqual(readingPoint(root, 6, skip, 'end'), {node:last,offset:3});
  assert.equal(readingPoint(root, 7, skip, 'end'), null);
});
