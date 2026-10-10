import test from 'node:test';
import assert from 'node:assert/strict';
import { freshness, resolveCodeReference, commentsForBlock } from './explanation-model.js';

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
test('a block shows its original comments and unchanged comments from earlier revisions', () => {
  const block = {id:'b',part:{type:'text',body:'exact\r\n'},references:[]};
  const comment = {id:'c',target_type:'explanation',explanation:{revision:1,block_id:'b',quote:{section_title:'Old',block}}};
  assert.deepEqual(commentsForBlock([comment], 2, block), [comment]);
  assert.deepEqual(commentsForBlock([comment], 2, {...block,part:{type:'text',body:'exact\n'}}), []);
  assert.deepEqual(commentsForBlock([comment], 1, block), [comment]);
});
