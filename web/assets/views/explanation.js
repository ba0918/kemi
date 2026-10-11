// @ts-check
import { actions } from '../actions.js';
import { button, dom, el, textEl } from '../dom.js';
import { state, unitLabel } from '../state.js';
import { commentsForBlock, freshness } from '../explanation-model.js';
import { partView } from './parts.js';

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
  dom.explanationReturn.setAttribute('aria-label', returning ? 'Back to explanation' : 'EXPLANATION');
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
      article.setAttribute('aria-label',`${section.title} · Block ${index + 1}`);
      article.append(partView(block.part));
      const references = el('div','explanation-references');
      for (const reference of block.references) {
        const link = button('btn explanation-reference');
        link.textContent = reference.type === 'page' ? `Page ${reference.url}` : `Code ${reference.path}${reference.start_line === null ? '' : `:${reference.start_line}–${reference.end_line}`} (${reference.side})`;
        if (reference.type==='code' && reference.unit) link.textContent+=` · ${unitLabel(reference.unit)} / ${reference.group_id}`;
        link.addEventListener('click',()=>actions.followExplanationReference(reference));
        references.append(link);
      }
      article.append(references);
      const comments = el('div','explanation-comments'); comments.dataset.blockComments = block.id; article.append(comments);
      const editor = /** @type {HTMLDetailsElement} */(el('details','explanation-comment-editor'));
      editor.append(textEl('summary','','Comment on this block'));
      const form = /** @type {HTMLFormElement} */(el('form'));
      const target = /** @type {HTMLLabelElement} */(textEl('label','explanation-comment-target',`${section.title} · Block ${index + 1}`));
      const input = /** @type {HTMLTextAreaElement} */(el('textarea'));
      input.id = `explanation-input-${block.id}`;target.htmlFor = input.id;
      input.placeholder = 'Ask a question or suggest a change…';
      const save = button('btn primary'); save.textContent='Add comment';
      save.type='submit'; form.append(target,input,save);editor.append(form);article.append(editor);
      editor.addEventListener('toggle',()=>{if(editor.open) input.focus();});
      form.addEventListener('submit', async event=>{
        event.preventDefault();save.disabled=true;
        if (await actions.commentOnExplanation(block,input.value)) {input.value='';editor.open=false;}
        save.disabled=false;
      });
      details.append(article);
    }
    dom.explanationDocument.append(details);
  }
  requestAnimationFrame(markCurrent);
  renderExplanationComments();
}
