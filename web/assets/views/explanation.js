// @ts-check
import { actions } from '../actions.js';
import { button, dom, el, textEl, iconButton, COMMENT_ICON } from '../dom.js';
import { state, unitLabel } from '../state.js';
import { commentsForBlock, freshness, sectionCommentCounts } from '../explanation-model.js';
import { partView } from './parts.js';

/** @param {string} label */
export function explanationCommentControl(label) {
  const control = iconButton(label,COMMENT_ICON);
  control.classList.add('explanation-comment-trigger');
  control.querySelector('svg')?.setAttribute('aria-hidden','true');
  return control;
}

export function renderExplanationStatus() {
  const {reading, notification, latest, returnTo} = state.explanation;
  for (const tab of dom.viewSwitch.querySelectorAll('button')) {
    const view = /** @type {HTMLButtonElement} */(tab).dataset.reviewView;
    tab.setAttribute('aria-pressed', String(state.view === view));
    if (view === 'explanation') {
      /** @type {HTMLButtonElement} */(tab).disabled = !latest;
      tab.setAttribute('title', latest ? 'Read the explanation' : 'No explanation yet');
    }
    if (view === 'page') /** @type {HTMLElement} */(tab).hidden = !state.review?.live;
  }
  const returning = Boolean(returnTo && state.view !== 'explanation');
  dom.explanationReturn.classList.toggle('has-return', returning);
  dom.explanationReturn.setAttribute('aria-label', returning ? 'Back to explanation' : 'Explanation');
  dom.explanationReturn.title = returning ? 'Back to your reading position' : latest ? 'Read the explanation' : 'No explanation yet';
  dom.explanationUpdate.hidden = !notification;
  if (notification) {
    const status = freshness(notification.base_display_version, state.codeToken);
    dom.explanationUpdate.textContent = `Open explanation revision ${notification.revision}${status === 'different' ? ` · Explanation for a different display version (${notification.base_display_version} / Code ${state.codeToken})` : status === 'pending' ? ' · Code display version not loaded' : ''}`;
  }
  dom.explanationStatus.hidden = state.view !== 'explanation' || !reading;
  const baseDetailsOpen=dom.explanationStatus.querySelector('details')?.open??false;
  dom.explanationStatus.replaceChildren();
  if (!reading) return;
  dom.explanationStatus.append(textEl('span','','Revision '+reading.revision));
  const status = freshness(reading.base_display_version, state.codeToken);
  if (status !== 'matching') dom.explanationStatus.append(textEl('p','explanation-mismatch',status === 'pending' ? 'Code display version not loaded; freshness is pending.' : `Explanation for a different display version · Explanation ${reading.base_display_version} / Code ${state.codeToken}`));
  const details = /** @type {HTMLDetailsElement} */(el('details'));
  details.open=baseDetailsOpen;
  details.append(textEl('summary','','About the explanation base'),textEl('p','',`Base ${reading.base_display_version}. This identifies a metadata import, not fixed file contents or proof that the explanation is correct. Files may be read again even with the same token.`));
  dom.explanationStatus.append(details);
}

export function renderExplanationComments() {
  const version = state.explanation.reading;
  if (!version) return;
  for (const section of version.sections) for (const block of section.blocks) {
    const host = dom.explanationDocument.querySelector(`[data-block-comments="${block.id}"]`);
    if (!host) continue;
    host.replaceChildren();
    for (const comment of commentsForBlock(state.allComments, version.revision, block)) {
      const chip = button('cchip');
      chip.textContent = `${comment.resolved ? 'Resolved · ' : ''}${comment.body}`;
      chip.addEventListener('click',()=>actions.openThread(comment.id));
      host.append(chip);
    }
  }
  renderExplanationCounts();
}

/**
 * @param {({type:'page',url:string}|{type:'code',path:string,side:string,unit:string|null,group_id:string,start_line:number|null,end_line:number|null})[]} references
 * @param {boolean} [navigate]
 */
export function explanationReferences(references, navigate = true) {
  const host = el('div','explanation-references');
  for (const reference of references) {
    let label = reference.type === 'page' ? `Page ${reference.url}` : `Code ${reference.path}${reference.start_line === null ? '' : `:${reference.start_line}–${reference.end_line}`} (${reference.side})`;
    if (reference.type === 'code') {
      if (reference.unit) label += ` · ${unitLabel(reference.unit)} / ${reference.group_id}`;
      else if (!navigate) label += ` · ${reference.group_id}`;
    }
    if (navigate) {
      const link = button('btn explanation-reference');
      link.textContent = label;
      link.addEventListener('click',()=>actions.followExplanationReference(reference));
      host.append(link);
    } else {
      host.append(textEl('span','explanation-reference',label));
    }
  }
  return host;
}

/** 描画のたびに進める。前の描画の Markdown が後から終わっても、今の文書に手を出させない。 */
let renderToken = 0;
let refreshQueued = false;

/** 目次の項目のうち、読み位置の上にある最後のものを示す。 */
function markCurrentContents() {
  const top = dom.explanationDocument.getBoundingClientRect().top + 80;
  const items = Array.from(dom.explanationToc.querySelectorAll('[data-toc-target]'));
  let current = items[0];
  for (const item of items) {
    const target = document.getElementById(/** @type {HTMLElement} */(item).dataset.tocTarget ?? '');
    if (target && target.getBoundingClientRect().top <= top) current = item;
  }
  for (const item of items) {
    if (item === current) item.setAttribute('aria-current','location');
    else item.removeAttribute('aria-current');
  }
}

/** @param {string} label @param {string} targetId */
function contentsItem(label, targetId) {
  const item = button('explanation-toc-item');
  item.dataset.tocTarget = targetId;
  item.setAttribute('aria-label',`Go to ${label}`);
  item.addEventListener('click',()=>{
    document.getElementById(targetId)?.scrollIntoView({block:'start'});
    actions.closeDrawer();
  });
  return item;
}

/** text パーツの Markdown 見出しを、描画の済んだものから目次の節の下に並べる。 */
function renderContentsHeadings() {
  const version = state.explanation.reading;
  if (!version) return;
  for (const old of dom.explanationToc.querySelectorAll('.explanation-toc-sub')) old.remove();
  for (const section of version.sections) {
    let after = dom.explanationToc.querySelector(`[data-toc-section="${section.id}"]`);
    if (!after) continue;
    for (const block of section.blocks) {
      const article = dom.explanationDocument.querySelector(`[data-explanation-block="${block.id}"]`);
      const headings = article?.querySelectorAll('.part-markdown :is(h1,h2,h3,h4,h5,h6)') ?? [];
      for (const [index, heading] of Array.from(headings).entries()) {
        // 目次から移るための DOM の id。コメントの対象 ID ではない。
        if (!heading.id) heading.id = `explanation-heading-${block.id}-${index}`;
        const li = el('li','explanation-toc-sub');
        const item = contentsItem(heading.textContent ?? '', heading.id);
        item.append(textEl('span','',heading.textContent ?? ''));
        li.append(item);
        after.after(li);
        after = li;
      }
    }
  }
  markCurrentContents();
}

function scheduleReadingRefresh() {
  if (refreshQueued) return;
  refreshQueued = true;
  requestAnimationFrame(()=>{ refreshQueued = false; renderContentsHeadings(); });
}

export function renderExplanationCounts() {
  const version = state.explanation.reading;
  if (!version) return;
  const counts = sectionCommentCounts(state.allComments, version);
  for (const [sectionId, count] of counts) {
    const badge = dom.explanationToc.querySelector(`[data-toc-count="${sectionId}"]`);
    if (!badge) continue;
    /** @type {HTMLElement} */(badge).hidden = count === 0;
    badge.textContent = String(count);
    badge.setAttribute('title',`${count} ${count === 1 ? 'comment' : 'comments'}`);
  }
}

export function renderExplanationDocument() {
  const version = state.explanation.reading;
  const token = ++renderToken;
  dom.explanationDocument.replaceChildren();
  dom.explanationToc.replaceChildren();
  if (!version) return;
  dom.explanationToc.append(textEl('div','explanation-toc-heading','CONTENTS'), textEl('p','explanation-toc-meta',`${version.sections.length} sections · Revision ${version.revision}`));
  const list = el('ol','explanation-toc-list');
  dom.explanationToc.append(list);
  dom.explanationDocument.onscroll = markCurrentContents;
  for (const [sectionIndex, section] of version.sections.entries()) {
    const sectionElement = el('section','explanation-section');
    sectionElement.id = `explanation-section-${section.id}`;
    const number = String(sectionIndex + 1).padStart(2,'0');
    const head = el('header','explanation-section-head');
    const title = textEl('h2','',section.title);
    title.id = `explanation-section-title-${section.id}`;
    sectionElement.setAttribute('aria-labelledby',title.id);
    head.append(textEl('span','explanation-section-number',number),title);
    sectionElement.append(head);
    const li = el('li','explanation-toc-section');
    li.dataset.tocSection = section.id;
    const toc = contentsItem(section.title, sectionElement.id);
    const count = textEl('span','explanation-toc-count','');
    count.id = `explanation-toc-count-${section.id}`;
    count.dataset.tocCount = section.id;
    count.hidden = true;
    toc.setAttribute('aria-describedby',count.id);
    toc.append(textEl('span','explanation-section-number',number),textEl('span','explanation-toc-title',section.title),count);
    li.append(toc);
    list.append(li);
    for (const [index, block] of section.blocks.entries()) {
      const article = el('article','explanation-block');
      article.id = `explanation-block-${block.id}`;
      article.dataset.explanationBlock = block.id;
      article.dataset.partType = block.part.type;
      article.setAttribute('aria-label',`${section.title} · Block ${index + 1}`);
      article.append(partView(block.part,{onMarkdown:()=>{ if (token === renderToken) scheduleReadingRefresh(); }}));
      article.append(explanationReferences(block.references));
      const comments = el('div','explanation-comments'); comments.dataset.blockComments = block.id; article.append(comments);
      const trigger = explanationCommentControl(`Comment on ${section.title} · Block ${index + 1}`);
      trigger.dataset.commentTrigger = '';
      trigger.setAttribute('aria-expanded','false');
      const form = /** @type {HTMLFormElement} */(el('form','explanation-comment-editor'));
      form.hidden = true;form.id = `explanation-form-${block.id}`;
      trigger.setAttribute('aria-controls',form.id);
      trigger.addEventListener('click',()=>actions.openExplanationEditor(block.id));
      article.append(trigger);
      const target = /** @type {HTMLLabelElement} */(textEl('label','explanation-comment-target',`${section.title} · Block ${index + 1}`));
      const input = /** @type {HTMLTextAreaElement} */(el('textarea'));
      input.id = `explanation-input-${block.id}`;target.htmlFor = input.id;
      input.value = state.explanation.drafts.get(`${version.revision}:${block.id}`) ?? '';
      input.addEventListener('input',()=>actions.keepExplanationDraft(version.revision,block.id,input.value));
      input.placeholder = 'Ask a question or suggest a change…';
      const save = button('btn primary'); save.textContent='Add comment';
      const cancel = button('btn');cancel.textContent = 'Cancel';cancel.setAttribute('aria-label','Cancel comment');
      const close = () => {form.hidden = true;trigger.setAttribute('aria-expanded','false');trigger.focus({preventScroll:true});};
      cancel.addEventListener('click',close);
      form.addEventListener('keydown',event=>{if(event.key==='Escape'){event.preventDefault();event.stopPropagation();close();}});
      save.type='submit'; form.append(target,textEl('p','explanation-comment-scope','Comment on the entire block'),input,save,cancel);article.append(form);
      form.addEventListener('submit', async event=>{
        event.preventDefault();save.disabled=true;
        const submittedBody = input.value;
        if (await actions.commentOnExplanation(block,submittedBody,version.revision) && input.value === submittedBody) {input.value='';close();}
        save.disabled=false;
      });
      sectionElement.append(article);
    }
    dom.explanationDocument.append(sectionElement);
  }
  requestAnimationFrame(markCurrentContents);
  renderExplanationComments();
}
