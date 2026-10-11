// @ts-check
import { actions } from '../../actions.js';
import { dom } from '../../dom.js';
import { state } from '../../state.js';
import { selectedExplanationBlock } from '../../explanation-model.js';
import { explanationCommentControl } from '../../views/explanation.js';

export function setupExplanationSelection() {
  const popup = explanationCommentControl('Comment on selected block');
  popup.classList.add('explanation-selection-comment');
  popup.setAttribute('popover','manual');
  document.body.append(popup);
  /** @type {{article:HTMLElement,revision:number}|null} */
  let target = null;
  let dragging = false, queued = false;
  const close = () => { target = null; if (popup.matches(':popover-open')) popup.hidePopover(); };
  const dismiss = () => { close(); window.getSelection()?.removeAllRanges(); };
  /** @param {Node|null} node */
  const endpoint = node => {
    const element = node instanceof Element ? node : node?.parentElement;
    if (!element?.closest('.part-markdown') || element.closest('button,textarea,input,svg')) return null;
    const article = /** @type {HTMLElement|null} */(element.closest('[data-explanation-block]'));
    return article && dom.explanationDocument.contains(article) ? {article,id:article.dataset.explanationBlock ?? '',type:article.dataset.partType ?? ''} : null;
  };
  const refresh = () => {
    queued = false;
    const selection = window.getSelection();
    if (dragging || state.view !== 'explanation' || !state.explanation.reading || selection?.rangeCount !== 1 || document.activeElement?.matches('textarea,input,[contenteditable="true"]')) {close();return;}
    const range = selection.getRangeAt(0), start = endpoint(range.startContainer), end = endpoint(range.endContainer);
    const id = selectedExplanationBlock(start,end,!selection.isCollapsed && Boolean(selection.toString().trim()));
    if (!id || !start || !end || start.article !== end.article) {close();return;}
    // 端点だけが本文にあっても、途中に入力や操作がある選択は本文の選択と扱わない。
    if (Array.from(start.article.querySelectorAll('form,button')).some(node=>range.intersectsNode(node))) {close();return;}
    const area = dom.explanationDocument.getBoundingClientRect(), viewport = window.visualViewport;
    const left = Math.max(area.left,viewport?.offsetLeft ?? 0), top = Math.max(area.top,viewport?.offsetTop ?? 0);
    const right = Math.min(area.right,(viewport?.offsetLeft ?? 0)+(viewport?.width ?? window.innerWidth));
    const hand = dom.handFloatBar.getBoundingClientRect();
    const bottom = Math.min(area.bottom,(viewport?.offsetTop ?? 0)+(viewport?.height ?? window.innerHeight),state.narrow && hand.height ? hand.top : Infinity);
    const rect = Array.from(range.getClientRects()).filter(r=>r.width>0 && r.right>left && r.left<right && r.bottom>top && r.top<bottom).at(-1);
    if (!rect) {close();return;}
    target = {article:start.article,revision:state.explanation.reading.revision};
    if (!popup.matches(':popover-open')) popup.showPopover();
    popup.style.left = '0px';popup.style.top = '0px';
    const box = popup.getBoundingClientRect(), scale = box.width / popup.offsetWidth;
    if (right-left < box.width+8 || bottom-top < box.height+8) {close();return;}
    const x = Math.max(left+4,Math.min(right-box.width-4,rect.right-box.width/2));
    const y = Math.max(top+4,Math.min(bottom-box.height-4,rect.bottom+6));
    popup.style.left = `${(x-box.left)/scale}px`;popup.style.top = `${(y-box.top)/scale}px`;
  };
  const schedule = () => {if (!queued) {queued = true;requestAnimationFrame(refresh);}};
  popup.addEventListener('pointerdown',event=>event.preventDefault());
  popup.addEventListener('click',()=>{
    const chosen = target;
    if (!chosen?.article.isConnected || chosen.revision !== state.explanation.reading?.revision || state.view !== 'explanation') {dismiss();return;}
    dismiss();actions.openExplanationEditor(chosen.article.dataset.explanationBlock ?? '');
  });
  document.addEventListener('selectionchange',schedule);
  document.addEventListener('pointerdown',event=>{
    if (popup.contains(/** @type {Node} */(event.target))) return;
    dragging = Boolean(endpoint(/** @type {Node} */(event.target)));
    if (dragging) close();else dismiss();
  });
  document.addEventListener('pointerup',()=>{dragging = false;schedule();});
  document.addEventListener('pointercancel',()=>{dragging = false;close();});
  document.addEventListener('keyup',schedule);
  document.addEventListener('keydown',event=>{if(event.key==='Escape' && target){event.preventDefault();event.stopPropagation();dismiss();}},true);
  dom.explanationDocument.addEventListener('scroll',schedule);
  dom.explanationDocument.addEventListener('toggle',schedule,true);
  window.addEventListener('resize',schedule);
  window.visualViewport?.addEventListener('resize',schedule);
  window.visualViewport?.addEventListener('scroll',schedule);
  return dismiss;
}
