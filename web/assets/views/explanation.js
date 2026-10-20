// @ts-check
import { actions } from '../actions.js';
import { button, dom, el, textEl, svgIcon, COMMENT_ICON } from '../dom.js';
import { state } from '../state.js';
import { replyCountLabel, unitLabel } from '../model.js';
import { commentsForBlock, freshness, sectionCommentCounts, locateSelection, readingText, readingPoint, explanationElementId, commentCountDescription } from '../explanation-model.js';
import { partView } from './parts.js';

/** 画面が本文の中へ後から入れる要素（札）の印。選択の位置を数えるときに中へ入らない。 */
const INJECTED = 'data-explanation-injected';
const HIGHLIGHT = 'explanation-comment';

/** @param {any} node */
export function isInjected(node) {
  return node instanceof Element && node.hasAttribute(INJECTED);
}

/**
 * 選択と印の基準にする、利用者が読む本文だけの要素。見出し・Copy・参照ボタンは含めない。
 * Markdown の描画が済むまでは null（描画前の原文で数えると描画後とずれるため）。
 * @param {Element} article
 * @returns {HTMLElement|null}
 */
export function readingRoot(article) {
  const type = /** @type {HTMLElement} */(article).dataset.partType;
  if (type === 'code') return article.querySelector('.part-code > code');
  if (type !== 'text') return null;
  return article.querySelector('.part-markdown') ?? (article.querySelector('.part-failure') ? article.querySelector('.part-original') : null);
}

/** @param {string} label */
function commentIcon(label = '') {
  const icon = svgIcon(COMMENT_ICON);
  icon.setAttribute('aria-hidden','true');
  if (label) icon.classList.add(label);
  return icon;
}

/** @param {string} text */
export function selectionCommentButton(text) {
  const control = button('explanation-selection-comment');
  control.setAttribute('aria-label','Comment on selection');
  control.append(commentIcon(),textEl('span','',text));
  return control;
}

/**
 * 選んだ所（図ではボタン）の下に浮かべる入力欄。
 * @param {{sectionTitle:string, scope:string, quote:string, draft:string}} target
 */
export function explanationEditor(target) {
  const form = /** @type {HTMLFormElement} */(el('form','explanation-comment-editor'));
  form.setAttribute('aria-label','New explanation comment');
  const head = el('div','explanation-comment-target');
  head.append(textEl('b','','Comment'),textEl('span','',`${target.sectionTitle} · ${target.scope}`));
  const quote = textEl('blockquote','explanation-comment-quote',target.quote);
  const input = /** @type {HTMLTextAreaElement} */(el('textarea'));
  input.setAttribute('aria-label',`Comment on ${target.sectionTitle}`);
  input.placeholder = 'Ask a question or suggest a change…';
  input.value = target.draft;
  const actions = el('div','explanation-comment-actions');
  const cancel = button('btn');cancel.textContent = 'Cancel';cancel.setAttribute('aria-label','Cancel comment');
  const save = button('btn primary');save.textContent = 'Add comment';save.type = 'submit';
  actions.append(cancel,save);
  form.append(head,quote,input,actions);
  return {form,input,save,cancel};
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

/** 札を置く、選択の終わりを含む段落・リスト項目・引用・表・コードのうち最も内側の要素。 */
const CARD_ANCHOR = 'p, li, blockquote, table, pre';

/**
 * @param {Element} article @param {Element|null} anchor 段落などの要素。null ならブロック全体のコメント。
 * @returns {HTMLElement}
 */
function cardsHost(article, anchor) {
  if (!anchor) {
    article.classList.add('has-comment');
    let host = /** @type {HTMLElement|null} */(article.querySelector(':scope > [data-explanation-cards]'));
    if (!host) { host = el('div','explanation-cards'); host.dataset.explanationCards = ''; host.setAttribute(INJECTED,''); article.append(host); }
    return host;
  }
  // リスト項目では項目の中の末尾に入れる（ul の直下に div を置かない）。
  const existing = anchor.matches('li') ? anchor.querySelector(':scope > [data-explanation-cards]') : anchor.nextElementSibling?.matches('[data-explanation-cards]') ? anchor.nextElementSibling : null;
  if (existing) return /** @type {HTMLElement} */(existing);
  const host = el('div','explanation-cards'); host.dataset.explanationCards = ''; host.setAttribute(INJECTED,'');
  if (anchor.matches('li')) anchor.append(host); else anchor.after(host);
  return host;
}

/** ブロックの中に画面が入れた札の入れ物。 @param {string} blockId @returns {HTMLElement[]} */
export function commentCardHosts(blockId) {
  const article = dom.explanationDocument.querySelector(`[data-explanation-block="${blockId}"]`);
  return article ? Array.from(article.querySelectorAll('[data-explanation-cards]')) : [];
}

/** @param {any} comment */
function commentCard(comment) {
  const card = button('explanation-card');
  card.append(commentIcon(),textEl('span','explanation-card-body',`${comment.resolved ? 'Resolved · ' : ''}${String(comment.body).split('\n')[0]}`));
  card.append(textEl('span','explanation-card-meta',replyCountLabel((comment.replies ?? []).length)));
  card.addEventListener('click',()=>actions.openThread(comment.id));
  return card;
}

/**
 * 保存した選択を、今の描画の範囲に戻す。戻せなければ null。
 * @param {Element} article @param {{text:string,occurrence:number}|null} selection
 */
function selectionRange(article, selection) {
  const root = selection ? readingRoot(article) : null;
  if (!root || !selection) return null;
  const at = locateSelection(readingText(root,isInjected), selection);
  const start = at && readingPoint(root,at.start,isInjected,'start');
  const end = at && readingPoint(root,at.end,isInjected,'end');
  if (!start || !end) return null;
  const range = document.createRange();
  range.setStart(/** @type {Node} */(/** @type {unknown} */(start.node)),start.offset);
  range.setEnd(/** @type {Node} */(/** @type {unknown} */(end.node)),end.offset);
  return range;
}

export function renderExplanationComments() {
  const version = state.explanation.reading;
  for (const injected of dom.explanationDocument.querySelectorAll(`[${INJECTED}]`)) injected.remove();
  for (const marked of dom.explanationDocument.querySelectorAll('.has-comment')) marked.classList.remove('has-comment');
  /** @type {Range[]} */ const ranges = [];
  if (version) for (const section of version.sections) for (const block of section.blocks) {
    const article = dom.explanationDocument.querySelector(`[data-explanation-block="${block.id}"]`);
    if (!article) continue;
    for (const comment of commentsForBlock(state.allComments, version.revision, block)) {
      const range = selectionRange(article, comment.explanation.selection ?? null);
      let anchor = null;
      if (range) {
        ranges.push(range);
        const end = range.endContainer instanceof Element ? range.endContainer : range.endContainer.parentElement;
        const found = end?.closest(CARD_ANCHOR);
        anchor = found && article.contains(found) ? found : null;
      }
      cardsHost(article, anchor).append(commentCard(comment));
    }
  }
  if (typeof Highlight === 'function' && CSS.highlights) {
    if (ranges.length) CSS.highlights.set(HIGHLIGHT,new Highlight(...ranges));
    else CSS.highlights.delete(HIGHLIGHT);
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
        if (!heading.id) heading.id = explanationElementId('heading',block.id,index);
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
  requestAnimationFrame(()=>{ refreshQueued = false; renderContentsHeadings(); renderExplanationComments(); });
}

function renderExplanationCounts() {
  const version = state.explanation.reading;
  if (!version) return;
  const counts = sectionCommentCounts(state.allComments, version);
  for (const [sectionId, count] of counts) {
    const badge = dom.explanationToc.querySelector(`[data-toc-count="${sectionId}"]`);
    const description = document.getElementById(explanationElementId('toc-count',sectionId));
    if (!badge || !description) continue;
    /** @type {HTMLElement} */(badge).hidden = count === 0;
    badge.textContent = String(count);
    badge.setAttribute('title',commentCountDescription(count));
    description.textContent = commentCountDescription(count);
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
    sectionElement.id = explanationElementId('section',section.id);
    const number = String(sectionIndex + 1).padStart(2,'0');
    const head = el('header','explanation-section-head');
    const title = textEl('h2','',section.title);
    title.id = explanationElementId('section-title',section.id);
    sectionElement.setAttribute('aria-labelledby',title.id);
    head.append(textEl('span','explanation-section-number',number),title);
    sectionElement.append(head);
    const li = el('li','explanation-toc-section');
    li.dataset.tocSection = section.id;
    const toc = contentsItem(section.title, sectionElement.id);
    // aria-describedby は hidden の要素でも中身を読む。見える数字の要素を指すと 0 件でも裸の数字が読まれるので、語の付いた説明を別の要素に置き、0 件では空にする。
    const count = textEl('span','explanation-toc-count','');
    count.dataset.tocCount = section.id;
    count.hidden = true;
    const description = textEl('span','','');
    description.id = explanationElementId('toc-count',section.id);
    description.hidden = true;
    toc.setAttribute('aria-describedby',description.id);
    toc.append(textEl('span','explanation-section-number',number),textEl('span','explanation-toc-title',section.title),count);
    li.append(toc,description);
    list.append(li);
    for (const [index, block] of section.blocks.entries()) {
      const article = el('article','explanation-block');
      article.id = explanationElementId('block',block.id);
      article.dataset.explanationBlock = block.id;
      article.dataset.partType = block.part.type;
      article.setAttribute('aria-label',`${section.title} · Block ${index + 1}`);
      article.append(partView(block.part,{
        onMarkdown:()=>{ if (token === renderToken) scheduleReadingRefresh(); },
        onDiagramComment:control=>actions.openExplanationEditor({blockId:block.id,selection:null,anchor:()=>control.getBoundingClientRect(),quote:'title' in block.part ? String(block.part.title) : '',returnFocus:control}),
      }));
      article.append(explanationReferences(block.references));
      sectionElement.append(article);
    }
    dom.explanationDocument.append(sectionElement);
  }
  requestAnimationFrame(markCurrentContents);
  renderExplanationComments();
}
