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
  dom.explanationReturn.hidden = !returnTo || state.view === 'explanation';
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
  for (const section of version.sections) {
    const details = el('details','explanation-section');
    /** @type {HTMLDetailsElement} */(details).open = true;
    details.id = `explanation-section-${section.id}`;
    details.append(textEl('summary','',section.title));
    const toc = button('explanation-toc-item');
    toc.textContent = section.title;
    toc.setAttribute('aria-label',`Go to ${section.title}`);
    toc.addEventListener('click',()=>{
      /** @type {HTMLDetailsElement} */(details).open = true;
      details.scrollIntoView({block:'start'});
      actions.closeDrawer();
    });
    dom.explanationToc.append(toc);
    for (const block of section.blocks) {
      const article = el('article','explanation-block');
      article.id = `explanation-block-${block.id}`;
      article.append(partView(block.part));
      for (const reference of block.references) {
        const link = button('btn explanation-reference');
        link.textContent = reference.type === 'page' ? `Page ${reference.url}` : `Code ${reference.path}${reference.start_line === null ? '' : `:${reference.start_line}–${reference.end_line}`} (${reference.side})`;
        if (reference.type==='code' && reference.unit) link.textContent+=` · ${unitLabel(reference.unit)} / ${reference.group_id}`;
        link.addEventListener('click',()=>actions.followExplanationReference(reference));
        article.append(link);
      }
      const comments = el('div'); comments.dataset.blockComments = block.id; article.append(comments);
      const editor = /** @type {HTMLDetailsElement} */(el('details'));
      editor.append(textEl('summary','','Comment on this block'));
      const form = /** @type {HTMLFormElement} */(el('form'));
      const input = /** @type {HTMLTextAreaElement} */(el('textarea'));
      input.setAttribute('aria-label',`Comment on ${block.id}`);
      const save = button('btn'); save.textContent='Add comment';
      save.type='submit'; form.append(input,save);editor.append(form);article.append(editor);
      form.addEventListener('submit', async event=>{
        event.preventDefault();save.disabled=true;
        if (await actions.commentOnExplanation(block,input.value)) {input.value='';editor.open=false;}
        save.disabled=false;
      });
      details.append(article);
    }
    dom.explanationDocument.append(details);
  }
  renderExplanationComments();
}
