// @ts-check
import { actions } from '../actions.js';
import { button, el, textEl } from '../dom.js';
import { diagramViewport } from './diagrams/viewport.js';
import { gridView } from './diagrams/grid.js';

/** @param {string | {body: string|null, parts?: import('../diagrams/types.js').Part[]|null}} content */
export function contentView(content) {
  if (typeof content === 'string' || !content.parts) {
    return textEl('p', 'cv-text', typeof content === 'string' ? content : content.body ?? '');
  }
  const host = el('div', 'content-parts');
  for (const part of content.parts) {
    host.append(partView(part));
  }
  return host;
}

/**
 * @typedef {{onMarkdown?: (host: HTMLElement) => void}} PartHooks
 * onMarkdown は Markdown の描画が終わった（失敗の表示を含む）後に呼ぶ。描画はサーバで行うので、
 * 見出しや選択の印はこの後でないと付けられない。
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
    const pre = el('pre', 'part-code');
    pre.append(textEl('code', '', body));
    const copy = button('part-copy'); copy.textContent = 'Copy code';
    copy.addEventListener('click', async () => {
      try { await navigator.clipboard.writeText(body); copy.textContent = 'Copied'; }
      catch (error) { copy.textContent = `Could not copy: ${error}`; }
    });
    host.append(pre, copy);
  } else if (value.type === 'sequence' || value.type === 'flow') {
    host.append(diagramViewport(value));
  } else if (value.type === 'diagram') {
    host.append(gridView(value));
  } else {
    host.append(textEl('pre', 'part-original', JSON.stringify(value, null, 2)));
  }
  return host;
}

/** @param {HTMLElement} host @param {string} html */
export function showMarkdown(host, html) {
  const rendered = el('div', 'part-markdown');
  rendered.innerHTML = html;
  host.replaceChildren(rendered);
}

/** @param {HTMLElement} host @param {string} body @param {unknown} reason */
export function markdownFailure(host, body, reason) {
  const status = textEl('p', 'part-failure', `Markdown unavailable: ${reason}`);
  status.setAttribute('role', 'status');
  host.replaceChildren(status, textEl('pre', 'part-original', body));
}
