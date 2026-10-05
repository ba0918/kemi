// @ts-check
// エージェントとの往復の表示（agent-channel.md）: 上部の状態と「Hand to agent」、チャット欄
// （発言の並びと書く欄）。`kemi wait` が一度も呼ばれていないレビューでは何も出さない。

import { actions } from "../actions.js";
import { button, dom, el, textEl } from "../dom.js";
import { state } from "../state.js";
import { agentControlsShown, agentStatusLabel, authorLabel } from "../model.js";

/**
 * 状態・渡す・チャット欄の入口。広い画面ではファイルツリーの下端に置き、上部バーの並びは
 * 変えない。狭い画面では上部バーのチャット欄の入口（状態の点つき）だけを出す。状態は
 * 隠している間も data 属性に持つ。
 */
export function renderAgent() {
  const agent = state.agent;
  const shown = agentControlsShown(agent);
  dom.agentStatus.dataset.status = agent.status;
  dom.agentStatus.textContent = agentStatusLabel(agent.status);
  dom.chatStatus.dataset.status = agent.status;
  dom.agentDock.hidden = !shown;
  dom.btnChat.hidden = !shown;
  document.body.toggleAttribute("data-agent", shown);
  dom.handCount.textContent = agent.unhanded > 0 ? ` ${agent.unhanded}` : "";
  dom.btnHand.disabled = state.submitted || agent.unhanded === 0;
  if (state.chatOpen) {
    renderChat();
  }
}

/** チャット欄。狭い画面では下から出すシートになる（CSS の狭い画面の規則）。 */
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
  if (state.messages.length === 0) {
    list.append(textEl("li", "cl-empty", "No messages yet"));
  }
  for (const message of state.messages) {
    const item = el("li", "chat-item");
    item.dataset.author = message.author;
    item.append(
      textEl("span", "reply-author", authorLabel(message.author)),
      textEl("p", "reply-body", message.body),
    );
    list.append(item);
  }
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
  const hand = button("btn");
  hand.textContent =
    state.agent.unhanded > 0 ? `Hand to agent (${state.agent.unhanded})` : "Hand to agent";
  hand.disabled = state.agent.unhanded === 0;
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
