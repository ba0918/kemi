// @ts-check
import { renderPart } from '../api.js';
import { markdownFailure, showMarkdown } from '../views/parts.js';
import { state } from '../state.js';

/** @type {WeakMap<HTMLElement, string>} */
const markdownBodies = new WeakMap();

/** @param {string} body */
async function themedMarkdown(body) {
  for (;;) {
    const dark = state.dark;
    const result = await renderPart({ type: 'text', body }, dark);
    // 応答待ちの間に明暗が変わった場合、古い配色を画面へ戻さない。
    if (dark === state.dark) return result;
  }
}

/** @param {string} body @param {HTMLElement} host */
export async function renderPartMarkdown(body, host) {
  markdownBodies.set(host, body);
  try {
    const result = await themedMarkdown(body);
    showMarkdown(host, result.html);
  } catch (error) {
    markdownFailure(host, body, error);
  }
}

export function refreshPartHighlights() {
  for (const rendered of document.querySelectorAll('.part-markdown')) {
    const host = rendered.parentElement;
    const body = host ? markdownBodies.get(host) : undefined;
    if (body === undefined || !rendered.querySelector('pre > code')) continue;
    void themedMarkdown(body).then(result => {
      if (!rendered.isConnected) return;
      const template = document.createElement('template');
      template.innerHTML = result.html;
      const codes = template.content.querySelectorAll('pre > code');
      // 本文全体を置き換えると、Explanation のコメント札・入力欄・見出しの印が消える。
      rendered.querySelectorAll('pre > code').forEach((code, index) => {
        if (codes[index]) code.innerHTML = codes[index].innerHTML;
      });
    }).catch(() => { /* 色の再取得に失敗しても、表示中のコードとコピー操作は残す。 */ });
  }
}
