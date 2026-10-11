// @ts-check
import { actions } from '../actions.js';
import { button, dom, el, textEl, iconButton, COMMENT_ICON } from '../dom.js';
import { state, unitLabel } from '../state.js';
import { commentsForBlock, freshness } from '../explanation-model.js';
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

export function renderExplanationDocument() {
  const version = state.explanation.reading;
  dom.explanationDocument.replaceChildren();
  dom.explanationToc.replaceChildren();
  if (!version) return;
  dom.explanationToc.append(textEl('div','explanation-toc-heading','CONTENTS'), textEl('p','explanation-toc-meta',`${version.sections.length} sections · Revision ${version.revision}`));
  /** @type {{details:HTMLElement,toc:HTMLButtonElement}[]} */
  const sections = [];
  const markCurrent = () => {
    const top = dom.explanationDocument.getBoundingClientRect().top;
    let current = sections[0];
    for (const item of sections) if (item.details.getBoundingClientRect().top <= top + 80) current = item;
    for (const item of sections) {
      if (item === current) item.toc.setAttribute('aria-current','location');
      else item.toc.removeAttribute('aria-current');
    }
  };
  dom.explanationDocument.onscroll = markCurrent;
  for (const section of version.sections) {
    const details = el('details','explanation-section');
    /** @type {HTMLDetailsElement} */(details).open = true;
    details.id = `explanation-section-${section.id}`;
    const number = String(sections.length + 1).padStart(2,'0');
    const summary = textEl('summary','',section.title);
    summary.dataset.number = number;
    details.append(summary);
    const toc = button('explanation-toc-item');
    toc.append(textEl('span','explanation-section-number',number),textEl('span','',section.title));
    toc.setAttribute('aria-label',`Go to ${section.title}`);
    toc.addEventListener('click',()=>{
      /** @type {HTMLDetailsElement} */(details).open = true;
      details.scrollIntoView({block:'start'});
      actions.closeDrawer();
    });
    dom.explanationToc.append(toc);
    sections.push({details,toc});
    for (const [index, block] of section.blocks.entries()) {
      const article = el('article','explanation-block');
      article.id = `explanation-block-${block.id}`;
      article.dataset.explanationBlock = block.id;
      article.dataset.partType = block.part.type;
      article.setAttribute('aria-label',`${section.title} · Block ${index + 1}`);
      article.append(partView(block.part));
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
      details.append(article);
    }
    dom.explanationDocument.append(details);
  }
  requestAnimationFrame(markCurrent);
  renderExplanationComments();
}
