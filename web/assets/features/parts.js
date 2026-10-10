// @ts-check
import { renderPart } from '../api.js';
import { markdownFailure, showMarkdown } from '../views/parts.js';

/** @param {string} body @param {HTMLElement} host */
export async function renderPartMarkdown(body, host) {
  try {
    const result = await renderPart({ type: 'text', body });
    showMarkdown(host, result.html);
  } catch (error) {
    markdownFailure(host, body, error);
  }
}
