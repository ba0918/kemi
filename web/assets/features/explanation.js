// @ts-check
import * as api from '../api.js';
import { actions } from '../actions.js';
import { dom } from '../dom.js';
import { state, fileCacheKey } from '../state.js';
import { resolveCodeReference } from '../explanation-model.js';
import { applyReview, jumpToEntry, selectIndex } from './files.js';
import { updateComments } from './comments.js';
import { renderHeader } from '../views/header.js';
import { renderTree } from '../views/tree.js';
import { renderExplanationDocument, renderExplanationComments, renderExplanationStatus } from '../views/explanation.js';
import { showOverlay } from '../views/overlay.js';

/** @type {(view:'code'|'page', page?:string)=>void} */
let liveView = ()=>{};
let contentsWidth = 280;

function setupContentsResize() {
  const splitter = document.getElementById('explanation-splitter');
  const layout = dom.explanationToc.parentElement;
  if (!splitter || !layout) return;
  const maximum = () => Math.max(200, Math.min(480, layout.clientWidth * .4));
  /** @param {number} width */
  const setWidth = (width) => {
    contentsWidth = Math.max(200, Math.min(maximum(), width));
    layout.style.setProperty('--explanation-toc-width', `${contentsWidth}px`);
    splitter.setAttribute('aria-valuemin', '200');
    splitter.setAttribute('aria-valuemax', String(Math.floor(maximum())));
    splitter.setAttribute('aria-valuenow', String(Math.round(contentsWidth)));
  };
  splitter.addEventListener('keydown', event => {
    if (state.narrow || state.view !== 'explanation') return;
    const width = event.key === 'Home' ? 200 : event.key === 'End' ? maximum() : event.key === 'ArrowLeft' ? contentsWidth - 20 : event.key === 'ArrowRight' ? contentsWidth + 20 : null;
    if (width !== null) { event.preventDefault(); setWidth(width); }
  });
  splitter.addEventListener('pointerdown', event => {
    if (state.narrow || state.view !== 'explanation' || event.button !== 0) return;
    event.preventDefault(); splitter.focus();
    const startX = event.clientX, startWidth = contentsWidth;
    splitter.setPointerCapture(event.pointerId);
    /** @param {PointerEvent} move */
    const move = (move) => setWidth(startWidth + move.clientX - startX);
    const finish = () => {
      splitter.removeEventListener('pointermove', move);
      splitter.removeEventListener('pointerup', finish);
      splitter.removeEventListener('pointercancel', finish);
    };
    splitter.addEventListener('pointermove', move);
    splitter.addEventListener('pointerup', finish);
    splitter.addEventListener('pointercancel', finish);
  });
  new ResizeObserver(() => { if (!state.narrow) setWidth(contentsWidth); }).observe(layout);
  setWidth(contentsWidth);
}
/** @param {(view:'code'|'page', page?:string)=>void} handler */
export function bindLiveView(handler) {liveView=handler;}

/** @param {'explanation'|'code'|'page'} view */
export async function selectView(view) {
  if (state.view==='explanation' && view!=='explanation') state.explanation.scrollTop=dom.explanationDocument.scrollTop;
  if (view === 'explanation' && !state.explanation.reading) {
    await openLatestExplanation();return;
  }
  if (view === 'page' && !state.review?.live) return;
  if (view !== 'explanation' && state.review?.live) liveView(view);
  state.view=view;
  showView();
  if (view==='code' && !state.current && state.visible.length) await selectIndex(0,{scrollTop:true});
}

/** @param {'code'|'page'} view */
export function syncReviewView(view) {state.view=view;showView();}

function showView() {
  document.body.dataset.reviewView=state.view;
  dom.explanationDocument.hidden=state.view!=='explanation';
  dom.explanationToc.hidden=state.view!=='explanation';
  const splitter = document.getElementById('explanation-splitter');
  if (splitter) splitter.hidden = state.view !== 'explanation';
  if (state.drawerOpen) dom.explanationToc.dataset.drawer='open';
  else delete dom.explanationToc.dataset.drawer;
  renderHeader();renderExplanationStatus();
  if (state.view==='explanation') dom.explanationDocument.scrollTop=state.explanation.scrollTop;
  if (state.view==='code') {renderTree();window.dispatchEvent(new Event('resize'));}
}

/** @param {any} review */
export async function startExplanation(review) {
  setupContentsResize();
  const history=review.explanation;
  const current=history?.versions.find((/** @type {any} */ version)=>version.revision===history.current_revision);
  state.explanation.latest=current??null;
  if (current) await openLatestExplanation();
  else showView();
}

export async function openLatestExplanation() {
  const version=state.explanation.latest;
  if (!version) return;
  try {
    await api.holdExplanation(state.explanation.screen,version.revision);
    const previous=state.explanation.reading;
    state.explanation.reading=version;
    state.explanation.notification=null;
    state.explanation.returnTo=false;
    renderExplanationDocument();
    state.explanation.scrollTop=0;
    state.view='explanation';showView();
    if (previous && previous.revision!==version.revision) await api.releaseExplanation(state.explanation.screen,previous.revision);
  } catch(error) {showOverlay('could not open the explanation',String(error));}
}

/** @param {any} notice */
export async function receiveExplanation(notice) {
  try {
    const version=await api.getExplanation(notice?.revision??null);
    if (!state.explanation.latest || version.revision>state.explanation.latest.revision) {
      state.explanation.latest=version;
      if (version.revision!==state.explanation.reading?.revision) state.explanation.notification=version;
    }
    renderExplanationStatus();
    // 会話の比較値だけを取り直す。Codeの一覧を取り込む操作ではない。
    actions.explanationChanged();
  } catch { /* 次の通知か再接続時に再取得する。 */ }
}

export function explanationChanged() {renderExplanationComments();}
export function explanationCodeChanged() {renderExplanationStatus();}

/** @param {any} block @param {string} body */
export async function commentOnExplanation(block,body) {
  if (!body.trim() || !state.explanation.reading) return false;
  try {
    const comment=await api.postComment({op:'add_explanation',explanation_revision:state.explanation.reading.revision,block_id:block.id,body});
    updateComments(comments=>[...comments,comment]);
    renderExplanationComments();return true;
  } catch(error) {showOverlay('could not add the comment',String(error));return false;}
}

/** @param {any} reference */
export async function followExplanationReference(reference) {
  const unavailable=(/** @type {string} */ reason)=>showOverlay('Reference unavailable for this display version',reason);
  const version=state.explanation.reading;
  if (!version) return;
  state.explanation.scrollTop=dom.explanationDocument.scrollTop;
  try {
    await api.holdExplanation(state.explanation.screen,version.revision);
    if (reference.type==='page') {
      if (!state.review?.live) {unavailable('Page is available only in a live review.');return;}
      state.explanation.returnTo=true;
      liveView('page',reference.url);state.view='page';showView();return;
    }
    const review=reference.unit===state.unit ? state.review : state.reviews.get(reference.unit);
    // 最新一覧の取得は、参照移動だけで読者が選んだCodeの世代を変えてしまう。
    const whole=resolveCodeReference({...reference,start_line:null,end_line:null},review,state.codeToken);
    if (whole.reason) {unavailable(whole.reason);return;}
    const entry=whole.entry;
    const resolved=resolveCodeReference(reference,review,state.codeToken,state.cache.get(fileCacheKey(entry)));
    if (resolved.reason) {unavailable(resolved.reason);return;}
    state.explanation.returnTo=true;
    if (review!==state.review) applyReview(review,false);
    await selectView('code');
    await jumpToEntry(entry);
    if (!state.cache.has(fileCacheKey(entry))) {unavailable('The target content could not be loaded.');return;}
    if (reference.start_line!==null) {
      const after=resolveCodeReference(reference,review,state.codeToken,state.cache.get(fileCacheKey(entry)));
      if (after.reason) {unavailable(after.reason);return;}
      await jumpToEntry(entry,{side:reference.side,line:reference.start_line});
    }
  } catch(error) {unavailable(String(error));}
}

/** @param {any} comment */
export async function showExplanationComment(comment) {
  try {
    const revision=comment.explanation.revision;
    await api.holdExplanation(state.explanation.screen,revision);
    const version=await api.getExplanation(revision);
    const previous=state.explanation.reading;
    state.explanation.reading=version;
    renderExplanationDocument();state.view='explanation';showView();
    document.getElementById(`explanation-block-${comment.explanation.block_id}`)?.scrollIntoView();
    if (previous && previous.revision!==revision) await api.releaseExplanation(state.explanation.screen,previous.revision);
  } catch(error) {showOverlay('could not open the original explanation',String(error));}
}
