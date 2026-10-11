import test from 'node:test';
import assert from 'node:assert/strict';
import { freshness, resolveCodeReference, commentsForBlock, sectionCommentCounts } from './explanation-model.js';

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
