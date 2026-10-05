// @ts-check
// エージェントとの往復の表示（agent-channel.md）: 上部の状態と「Hand to agent」、チャット欄
// （発言の並びと書く欄）。「Hand to agent」だけは `kemi wait` が一度でも呼ばれたレビューでだけ出す。

import { actions } from "../actions.js";
import { button, dom, el, textEl } from "../dom.js";
import { state } from "../state.js";
import { agentStatusLabel, authorLabel, handShown } from "../model.js";

/**
 * 状態・渡す・チャット欄の入口。広い画面ではファイルツリーの下端に置き、上部バーの並びは
 * 変えない。狭い画面では上部バーのチャット欄の入口（状態の点つき）だけを出す。状態は
 * 隠している間も data 属性に持つ。
 */
export function renderAgent() {
  const agent = state.agent;
  dom.agentStatus.dataset.status = agent.status;
  dom.agentStatus.textContent = agentStatusLabel(agent.status);
  dom.chatStatus.dataset.status = agent.status;
  dom.btnHand.hidden = !handShown(agent);
  dom.handCount.textContent = agent.unhanded > 0 ? ` ${agent.unhanded}` : "";
  dom.btnHand.disabled = state.submitted || agent.unhanded === 0;
  if (state.chatOpen) {
    refreshChat();
  }
}

/**
 * チャット欄を作り直す（開いたときと、自分で発言したとき）。並びは末尾を見せる。
 * 狭い画面では下から出すシートになる（CSS の狭い画面の規則）。
 */
export function renderChat() {
  const focused = document.activeElement === dom.chat.querySelector("textarea");
  dom.chat.textContent = "";
  const head = el("div", "chat-head");
  head.append(textEl("b", "", "Chat"));
  const status = textEl("span", "agent-status", agentStatusLabel(state.agent.status));
  status.dataset.status = state.agent.status;
  head.append(status);
  const close = button("cl-close");
  close.textContent = "Close";
  close.addEventListener("click", () => actions.closeChat());
  head.append(close);
  dom.chat.append(head);

  const list = el("ol", "chat-items");
  fillMessages(list);
  dom.chat.append(list);
  if (state.submitted) {
    return;
  }

  const form = /** @type {HTMLFormElement} */ (el("form", "chat-form"));
  const body = document.createElement("textarea");
  body.rows = 2;
  body.placeholder = "Write to the agent about the whole review (Cmd/Ctrl+Enter to post)";
  body.value = state.chatDraft;
  body.addEventListener("input", () => {
    state.chatDraft = body.value;
  });
  body.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      form.requestSubmit();
    }
  });
  const row = el("div", "chat-actions");
  const hand = button("btn chat-hand");
  labelHand(hand);
  hand.addEventListener("click", () => actions.handToAgent());
  const post = /** @type {HTMLButtonElement} */ (el("button", "btn primary"));
  post.type = "submit";
  post.textContent = "Post";
  row.append(hand, post);
  form.append(body, row);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (body.value.trim() !== "") {
      actions.postMessage(body.value);
    }
  });
  dom.chat.append(form);
  list.scrollTop = list.scrollHeight;
  if (focused) {
    body.focus({ preventScroll: true });
  }
}

/**
 * 届いた通知（エージェントの状態・発言・返信）でチャット欄をその場で直す。並びの
 * スクロール位置と、書く欄（書きかけ・カーソル・変換中の文字）はそのまま残す（R-LIVE）。
 */
function refreshChat() {
  const list = /** @type {HTMLElement | null} */ (dom.chat.querySelector(".chat-items"));
  if (!list) {
    renderChat();
    return;
  }
  const status = /** @type {HTMLElement | null} */ (dom.chat.querySelector(".chat-head .agent-status"));
  if (status) {
    status.dataset.status = state.agent.status;
    status.textContent = agentStatusLabel(state.agent.status);
  }
  const top = list.scrollTop;
  fillMessages(list);
  list.scrollTop = top;
  const form = dom.chat.querySelector(".chat-form");
  if (form && state.submitted) {
    form.remove();
    return;
  }
  const hand = /** @type {HTMLButtonElement | null} */ (dom.chat.querySelector(".chat-hand"));
  if (hand) {
    labelHand(hand);
  }
}

/**
 * 発言の並びを state.messages に合わせる。発言は後ろに増えるだけなので、並んでいるものが
 * 先頭から一致していれば足りない分だけを足し、そうでなければ並べ直す。
 * @param {HTMLElement} list
 */
function fillMessages(list) {
  const shown = Array.from(
    list.querySelectorAll(".chat-item"),
    (item) => /** @type {HTMLElement} */ (item).dataset.id,
  );
  const appendOnly =
    shown.length <= state.messages.length &&
    shown.every((id, index) => id === state.messages[index].id);
  if (!appendOnly) {
    list.textContent = "";
  }
  list.querySelector(".cl-empty")?.remove();
  if (state.messages.length === 0) {
    list.append(textEl("li", "cl-empty", "No messages yet"));
    return;
  }
  for (const message of state.messages.slice(appendOnly ? shown.length : 0)) {
    const item = el("li", "chat-item");
    item.dataset.id = message.id;
    item.dataset.author = message.author;
    item.append(
      textEl("span", "reply-author", authorLabel(message.author)),
      textEl("p", "reply-body", message.body),
    );
    list.append(item);
  }
}

/**
 * 「Hand to agent」の文言（未渡しの件数つき）と、出すか・押せるか。最初の `kemi wait` で
 * 出すとき、書く欄を作り直さずに済むよう、ボタンは常に作っておき隠すだけにする。
 * @param {HTMLButtonElement} hand
 */
function labelHand(hand) {
  hand.hidden = !handShown(state.agent);
  hand.textContent =
    state.agent.unhanded > 0 ? `Hand to agent (${state.agent.unhanded})` : "Hand to agent";
  hand.disabled = state.agent.unhanded === 0;
}
