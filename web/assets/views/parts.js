// @ts-check
import { actions } from '../actions.js';
import { COPY_ICON, el, iconButton, textEl } from '../dom.js';
import { diagramViewport } from './diagrams/viewport.js';
import { gridView } from './diagrams/grid.js';

/** @param {string | {body: string|null, parts?: import('../diagrams/types.js').Part[]|null}} content */
export function contentView(content) {
  if (typeof content === 'string' || !content.parts) {
    return partView({ type: 'text', body: typeof content === 'string' ? content : content.body ?? '' });
  }
  const host = el('div', 'content-parts');
  for (const part of content.parts) {
    host.append(partView(part));
  }
  return host;
}

/**
 * @typedef {{onMarkdown?: (host: HTMLElement) => void, onDiagramComment?: (button: HTMLButtonElement) => void}} PartHooks
 * onMarkdown は Markdown の描画が終わった（失敗の表示を含む）後に呼ぶ。描画はサーバで行うので、
 * 見出しや選択の印はこの後でないと付けられない。onDiagramComment を渡した図にだけコメントのボタンを置く。
 */

/** @param {import('../diagrams/types.js').Part} value @param {PartHooks} [hooks] */
export function partView(value, hooks = {}) {
  const host = el('section', 'content-part');
  if (value.type === 'text') {
    const body = value.body;
    host.append(textEl('pre', 'part-original', body));
    void actions.renderPartMarkdown(body, host).then(() => hooks.onMarkdown?.(host));
  } else if (value.type === 'code') {
    const body = value.body;
    const title = value.title ?? '';
    const language = value.language;
    const role = value.role;
    host.append(textEl('header', 'part-metadata', `${title} ${language === 'pseudocode' ? 'Pseudocode' : language} · ${role}`));
    const frame = el('div', 'part-code-frame');
    const pre = el('pre', 'part-code');
    pre.append(textEl('code', '', body));
    frame.append(pre, copyCodeButton(body));
    host.append(frame);
  } else if (value.type === 'sequence' || value.type === 'flow') {
    host.append(diagramViewport(value, false, { comment: hooks.onDiagramComment }));
  } else if (value.type === 'diagram') {
    host.append(gridView(value, { comment: hooks.onDiagramComment }));
  } else {
    host.append(textEl('pre', 'part-original', JSON.stringify(value, null, 2)));
  }
  return host;
}

/** @param {string} body */
function copyCodeButton(body) {
  const copy = iconButton('Copy code', COPY_ICON);
  copy.classList.add('part-copy');
  // コピーのラベルは、Explanation の選択・引用する本文には含めない。
  copy.setAttribute('data-explanation-injected', '');
  copy.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(body); labelCopy(copy, 'Copied', true); }
    catch (error) { labelCopy(copy, `Could not copy: ${error}`, false); }
  });
  return copy;
}

/** @param {HTMLButtonElement} copy @param {string} label @param {boolean} copied */
function labelCopy(copy, label, copied) {
  copy.title = label;
  copy.setAttribute('aria-label', label);
  copy.classList.toggle('copied', copied);
  window.setTimeout(() => {
    copy.title = 'Copy code';
    copy.setAttribute('aria-label', 'Copy code');
    copy.classList.remove('copied');
  }, 1_500);
}

/** @param {HTMLElement} host @param {string} html */
export function showMarkdown(host, html) {
  const rendered = el('div', 'part-markdown');
  rendered.innerHTML = html;
  for (const code of rendered.querySelectorAll('pre > code')) {
    const pre = code.parentElement;
    if (!pre) continue;
    const frame = el('div', 'part-code-frame');
    pre.classList.add('part-code');
    pre.replaceWith(frame);
    frame.append(pre, copyCodeButton(code.textContent ?? ''));
  }
  host.replaceChildren(rendered);
}

/** @param {HTMLElement} host @param {string} body @param {unknown} reason */
export function markdownFailure(host, body, reason) {
  const status = textEl('p', 'part-failure', `Markdown unavailable: ${reason}`);
  status.setAttribute('role', 'status');
  host.replaceChildren(status, textEl('pre', 'part-original', body));
}
