// @ts-check
import { actions } from '../../actions.js';
import { dom, textEl } from '../../dom.js';
import { state, explanationInteractable } from '../../state.js';
import { readingOffset, readingText, selectionAt, selectionTooLong } from '../../explanation-model.js';
import { isInjected, readingRoot, selectionCommentButton } from '../../views/explanation.js';

/**
 * 本文の選択から、コメントのボタンか、コメントできない理由を出す。
 * @returns {()=>void} 選択とボタンを消す関数
 */
export function setupExplanationSelection() {
  const popup = selectionCommentButton('Comment');
  popup.setAttribute('popover','manual');
  const hint = textEl('div','explanation-selection-hint','');
  hint.setAttribute('role','status');
  hint.setAttribute('popover','manual');
  document.body.append(popup,hint);
  /** @type {{blockId:string,revision:number,selection:{text:string,occurrence:number},rect:DOMRect}|null} */
  let target = null;
  let dragging = false, queued = false;
  /** @param {HTMLElement} element */
  const hide = element => { if (element.matches(':popover-open')) element.hidePopover(); };
  const close = () => { target = null; hide(popup); hide(hint); };
  const dismiss = () => { close(); window.getSelection()?.removeAllRanges(); };
  /** @param {Node|null} node */
  const blockOf = node => {
    const element = node instanceof Element ? node : node?.parentElement;
    const article = /** @type {HTMLElement|null|undefined} */(element?.closest('[data-explanation-block]'));
    return article && dom.explanationDocument.contains(article) ? article : null;
  };
  /** @param {Range} range */
  const visibleBounds = range => {
    const area = dom.explanationDocument.getBoundingClientRect(), viewport = window.visualViewport;
    const left = Math.max(area.left,viewport?.offsetLeft ?? 0), top = Math.max(area.top,viewport?.offsetTop ?? 0);
    const right = Math.min(area.right,(viewport?.offsetLeft ?? 0)+(viewport?.width ?? window.innerWidth));
    const hand = dom.handFloatBar.getBoundingClientRect();
    const bottom = Math.min(area.bottom,(viewport?.offsetTop ?? 0)+(viewport?.height ?? window.innerHeight),state.narrow && hand.height ? hand.top : Infinity);
    const rect = Array.from(range.getClientRects()).filter(r=>r.width>0 && r.right>left && r.left<right && r.bottom>top && r.top<bottom).at(-1);
    return rect ? {rect,left,top,right,bottom} : null;
  };
  /** @param {HTMLElement} element @param {{rect:DOMRect,left:number,top:number,right:number,bottom:number}} place */
  const showAt = (element, place) => {
    if (!element.matches(':popover-open')) element.showPopover();
    element.style.left = '0px';element.style.top = '0px';
    const box = element.getBoundingClientRect(), scale = box.width / element.offsetWidth;
    if (place.right-place.left < box.width+8 || place.bottom-place.top < box.height+8) {hide(element);return false;}
    const x = Math.max(place.left+4,Math.min(place.right-box.width-4,place.rect.right-box.width/2));
    const y = Math.max(place.top+4,Math.min(place.bottom-box.height-4,place.rect.bottom+6));
    element.style.left = `${(x-box.left)/scale}px`;element.style.top = `${(y-box.top)/scale}px`;
    return true;
  };
  /** @param {Range} range @param {string} reason */
  const explain = (range, reason) => {
    target = null; hide(popup);
    const place = visibleBounds(range);
    if (!place) {hide(hint);return;}
    hint.textContent = reason;
    showAt(hint, place);
  };
  const refresh = () => {
    queued = false;
    const selection = window.getSelection();
    if (dragging || !explanationInteractable() || !state.explanation.reading || selection?.rangeCount !== 1 || selection.isCollapsed || document.activeElement?.matches('textarea,input,[contenteditable="true"]')) {close();return;}
    const range = selection.getRangeAt(0), start = blockOf(range.startContainer), end = blockOf(range.endContainer);
    if (!start && !end) {close();return;}
    if (start !== end || !start) {explain(range,'Comments cannot span blocks. Select text within one block.');return;}
    const root = readingRoot(start);
    if (!root) {close();return;}
    // 端が根の外（見出し・参照ボタン・トリプルクリックで次の要素の先頭に置かれた端など）なら根の境界に詰める。
    const clipped = range.cloneRange(), whole = document.createRange();
    whole.selectNodeContents(root);
    if (clipped.compareBoundaryPoints(Range.START_TO_START,whole) < 0) clipped.setStart(root,0);
    if (clipped.compareBoundaryPoints(Range.END_TO_END,whole) > 0) clipped.setEnd(root,root.childNodes.length);
    if (clipped.collapsed || clipped.compareBoundaryPoints(Range.START_TO_END,whole) < 0 || clipped.compareBoundaryPoints(Range.END_TO_START,whole) > 0) {close();return;}
    const from = readingOffset(root,clipped.startContainer,clipped.startOffset,isInjected);
    const to = readingOffset(root,clipped.endContainer,clipped.endOffset,isInjected);
    if (from === null || to === null || to <= from) {close();return;}
    const text = readingText(root,isInjected);
    const chosen = selectionAt(text,from,to);
    if (!chosen.text.trim()) {close();return;}
    if (selectionTooLong(chosen.text)) {explain(range,'This selection is too long to quote (over 65,536 bytes).');return;}
    const place = visibleBounds(clipped);
    if (!place) {close();return;}
    hide(hint);
    target = {blockId:start.dataset.explanationBlock ?? '',revision:state.explanation.reading.revision,selection:chosen,rect:place.rect};
    if (!showAt(popup, place)) target = null;
  };
  const schedule = () => {if (!queued) {queued = true;requestAnimationFrame(refresh);}};
  const open = () => {
    const chosen = target;
    if (!chosen || chosen.revision !== state.explanation.reading?.revision || !explanationInteractable()) {dismiss();return;}
    dismiss();
    actions.openExplanationEditor({blockId:chosen.blockId,selection:chosen.selection,anchor:chosen.rect,quote:chosen.selection.text});
  };
  popup.addEventListener('pointerdown',event=>event.preventDefault());
  popup.addEventListener('click',open);
  document.addEventListener('selectionchange',schedule);
  document.addEventListener('pointerdown',event=>{
    if (popup.contains(/** @type {Node} */(event.target))) return;
    dragging = Boolean(blockOf(/** @type {Node} */(event.target)));
    if (dragging) close();else dismiss();
  });
  document.addEventListener('pointerup',()=>{dragging = false;schedule();});
  document.addEventListener('pointercancel',()=>{dragging = false;close();});
  document.addEventListener('keyup',schedule);
  document.addEventListener('keydown',event=>{
    if (event.key==='Escape' && (target || hint.matches(':popover-open'))) {event.preventDefault();event.stopPropagation();dismiss();return;}
    // コピーの Ctrl+C / Cmd+C、IME の変換中、入力欄の中の c は受けない。
    if (event.key!=='c' || !target || event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return;
    const element = /** @type {HTMLElement|null} */(event.target instanceof HTMLElement ? event.target : null);
    if (element?.closest('input,textarea,[contenteditable="true"]')) return;
    event.preventDefault();event.stopPropagation();open();
  },true);
  dom.explanationDocument.addEventListener('scroll',schedule);
  new ResizeObserver(schedule).observe(dom.explanationDocument);
  const geometry = new MutationObserver(records=>{
    if (records.some(record=>!(record.target instanceof SVGElement))) schedule();
  });
  geometry.observe(dom.explanationDocument,{attributes:true,subtree:true});
  geometry.observe(dom.conversation,{attributes:true,attributeFilter:['data-open']});
  for (let node = dom.explanationDocument.parentElement; node; node = node.parentElement) geometry.observe(node,{attributes:true});
  geometry.observe(document.head,{childList:true,subtree:true,attributes:true,characterData:true});
  document.fonts.addEventListener('loadingdone',schedule);
  window.addEventListener('resize',schedule);
  window.visualViewport?.addEventListener('resize',schedule);
  window.visualViewport?.addEventListener('scroll',schedule);
  return dismiss;
}
