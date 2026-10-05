// @ts-check
// エージェントとの往復（agent-channel.md）: スレッドの返信と解決、チャット欄の発言、
// 「Hand to agent」、状態の表示。サーバからの通知（返信・発言・状態）は届いたらその場で
// 取り込み、見ている位置は動かさない（R-LIVE の例外）。

import * as api from "../api.js";
import { dom } from "../dom.js";
import { state } from "../state.js";
import { addMessage, replaceComment } from "../model.js";
import { setCommentOpen, updateComments } from "./comments.js";
import { renderAgent, renderChat } from "../views/chat.js";
import { showOverlay, showToast } from "../views/overlay.js";

/**
 * エージェントの状態と未渡しの件数（通知と、書いた後の応答から）。
 * @param {import("../model.js").AgentState} agent
 */
export function applyAgent(agent) {
  state.agent = agent;
  renderAgent();
}

/**
 * スレッドが変わった（返信が増えた）コメントを取り込む。
 * @param {any} comment
 */
export function applyThread(comment) {
  updateComments((comments) => replaceComment(comments, comment));
}

/**
 * 発言を取り込む。書いた画面には応答と通知の両方で届くが、二度は足さない。
 * @param {any} message
 */
export function applyMessage(message) {
  const before = state.messages;
  state.messages = addMessage(state.messages, message);
  renderAgent();
  if (state.messages !== before && message.author === "agent" && !state.chatOpen) {
    showToast("New message from the agent");
  }
}

/**
 * 通知が届かなかった（取りこぼしたか、つながっていなかった）。スレッド・発言・状態を取り直す
 * （コメントの増減は更新バッジに任せる）。
 */
export async function resyncAgent() {
  try {
    const review = await api.getReview(false, state.unit);
    const fresh = new Map((review.comments || []).map((/** @type {any} */ comment) => [comment.id, comment]));
    updateComments((comments) => comments.map((comment) => fresh.get(comment.id) || comment));
    state.messages = review.messages || [];
    if (review.agent) {
      state.agent = review.agent;
    }
    renderAgent();
  } catch {
    // 読み直せなければ、次の通知か再読み込みで揃う。
  }
}

/**
 * スレッドに返信を書く。書きかけは書けたときだけ消す。
 * @param {any} comment
 * @param {string} body
 */
export async function replyTo(comment, body) {
  try {
    const updated = await api.postComment({ op: "reply", id: comment.id, body });
    state.replyDrafts.delete(comment.id);
    applyThread(updated);
  } catch (error) {
    showOverlay("could not reply", String(error));
  }
}

/**
 * 解決と、その取り消し。解決したスレッドは畳む（開き直せる。R-AGENT-HAND）。
 * @param {any} comment
 * @param {boolean} resolved
 */
export async function setResolved(comment, resolved) {
  try {
    const updated = await api.postComment({ op: "resolve", id: comment.id, resolved });
    applyThread(updated);
    if (resolved) {
      setCommentOpen(comment.id, false);
    }
  } catch (error) {
    showOverlay("could not change the resolution", String(error));
  }
}

/**
 * レビュー全体への発言を書く。
 * @param {string} body
 */
export async function postMessage(body) {
  try {
    const message = await api.postMessage(body);
    state.chatDraft = "";
    applyMessage(message);
    if (state.chatOpen) {
      renderChat();
    }
  } catch (error) {
    showOverlay("could not post the message", String(error));
  }
}

/** 前に渡した後に書いたものを、まとめてエージェントに渡す。 */
export async function handToAgent() {
  if (state.submitted) {
    return;
  }
  try {
    const answer = await api.hand();
    showToast(answer.handed ? "Handed to the agent" : "Nothing new to hand to the agent");
  } catch (error) {
    showOverlay("could not hand to the agent", String(error));
  }
}

export function openChat() {
  state.chatOpen = true;
  dom.chat.hidden = false;
  dom.btnChat.setAttribute("aria-expanded", "true");
  dom.btnDockChat.setAttribute("aria-expanded", "true");
  renderChat();
  const field = dom.chat.querySelector("textarea");
  if (field && !state.narrow) {
    field.focus({ preventScroll: true });
  }
}

export function closeChat() {
  state.chatOpen = false;
  dom.chat.hidden = true;
  dom.btnChat.setAttribute("aria-expanded", "false");
  dom.btnDockChat.setAttribute("aria-expanded", "false");
}

export function toggleChat() {
  if (state.chatOpen) {
    closeChat();
  } else {
    openChat();
  }
}
