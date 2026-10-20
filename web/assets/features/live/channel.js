// @ts-check
// `--live` の中継したページとの往復（live.md の R-PAGE-PROXY）。頼みごとを postMessage で送り、同じ番号の返事を待つ。
// 返事は入口の receive が、送り手が中継のオリジンの表示中の枠であることを確かめてから deliverAnswer に渡す。

import { live } from "./state.js";

/** ページが頼みごとに返す知らせの種類。 */
const ANSWERS = ["captured", "described", "placed", "imaged", "saved-found", "glowed"];

/** 頼みごとをして返事を待っているもの（番号 → 返事を受け取る関数）。 */
/** @type {Map<number, (message: any) => void>} */
const pendingAnswers = new Map();

let nextAsk = 1;

/**
 * 中継したページに頼みごとをして、返事を待つ。時間内に返らなければ error を持つ返事にする。
 * @param {Window} frame
 * @param {"capture" | "describe" | "place" | "image" | "saved-at" | "glow-element" | "glow-places"} type
 * @param {number} timeout
 * @param {Record<string, unknown>} [details] 頼みごとの中身
 * @returns {Promise<any>}
 */
export async function ask(frame, type, timeout, details = {}) {
  const id = nextAsk++;
  const answer = await new Promise((resolve) => {
    const timer = setTimeout(() => resolve({ error: "the page did not answer" }), timeout);
    pendingAnswers.set(id, (message) => {
      clearTimeout(timer);
      resolve(message);
    });
    frame.postMessage({ kemi: "live", ...details, type, id }, live.origin);
  });
  pendingAnswers.delete(id);
  return answer;
}

/**
 * ページからの知らせが頼みごとへの返事なら、待っているものに渡して true を返す。
 * @param {any} message
 * @returns {boolean}
 */
export function deliverAnswer(message) {
  if (!ANSWERS.includes(message.type)) {
    return false;
  }
  pendingAnswers.get(Number(message.id))?.(message);
  return true;
}
