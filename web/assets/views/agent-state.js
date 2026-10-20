// @ts-check
// エージェントの状態（会話パネルの見出しと畳んだ帯）と「Hand to agent」の 3 つ（書く欄の並び・
// 畳んだ帯・狭い画面の浮かぶ操作）、新着の数（R-AGENT-STATE、R-AGENT-HAND）。要素は
// index.html に 1 つずつあり、ここでは中身と押せるかどうかだけを合わせる。

import { button, copyText, dom, textEl } from "../dom.js";
import { state } from "../state.js";
import { agentStatusLabel, handControl, unreadCount } from "../conversation-model.js";

/**
 * エージェントの状態（見出しと畳んだ帯）と「Hand to agent」（書く欄の並びと畳んだ帯）。
 * 「Hand to agent」は状態によらず出し、押せないときはそのわけをそばに出す（R-AGENT-STATE）。
 */
export function renderAgentState() {
  const agent = state.agent;
  const label = agentStatusLabel(agent.status);
  dom.agentStatus.dataset.kemiAgentState = agent.status;
  dom.agentStatus.textContent = label;
  dom.railStatus.dataset.kemiAgentState = agent.status;
  dom.railStatus.title = label;
  const count = agent.unhanded > 0 ? ` ${agent.unhanded}` : "";
  const control = handControl(agent, state.submitted);
  for (const hand of [dom.btnHand, dom.railHand, dom.handFloat]) {
    hand.disabled = control.disabled;
    hand.dataset.handNote = control.note ?? "";
  }
  for (const note of [dom.handNote, dom.railHandNote, dom.handFloatNote]) {
    renderHandNote(note, control);
  }
  dom.handCount.textContent = count;
  dom.handFloatCount.textContent = count;
  dom.railHandCount.textContent = count;
  dom.railHand.setAttribute("aria-label", `Hand to agent${count ? ` (${agent.unhanded})` : ""}`);
  const unread = unreadCount(state.allComments, state.messages, state.conversation.read);
  dom.cvUnread.hidden = unread === 0;
  dom.cvUnread.textContent = String(unread);
  // 会話パネルを開いている間も、見出しに新着の数を出す（数え方は帯と同じ。R-AGENT-HAND）。
  dom.cvHeadUnread.hidden = unread === 0;
  dom.cvHeadUnread.textContent = `${unread} new`;
  dom.cvRail.setAttribute(
    "aria-label",
    unread > 0 ? `Open the conversation (${unread} new)` : "Open the conversation",
  );
}

/** 「Hand to agent」を押せないわけ（画面モックの状態 1）。 */
const HAND_NOTES = {
  connect: "You can hand comments once the agent runs",
  unavailable: "No agent can connect to this review. Submit to finish it.",
  nothing: "Nothing new to hand",
};

/**
 * 「Hand to agent」のそばの案内。`kemi wait` の前は、渡せるようになるコマンドとそれを写す操作も出す。
 * 中身が同じなら作り直さない（状態の知らせのたびに描き直され、写した印が消えるため）。
 * @param {HTMLElement} note
 * @param {ReturnType<typeof handControl>} control
 */
function renderHandNote(note, control) {
  const key = `${control.note ?? ""} ${control.command ?? ""}`;
  if (note.dataset.key === key) {
    return;
  }
  note.dataset.key = key;
  note.textContent = "";
  note.hidden = control.note === null;
  if (control.note === null) {
    return;
  }
  note.append(textEl("span", "hand-note-text", HAND_NOTES[control.note]));
  const command = control.command;
  if (command === null) {
    return;
  }
  const code = textEl("code", "hand-command", command);
  const copy = button("hand-copy");
  copy.textContent = "Copy";
  copy.title = `Copy ${command}`;
  copy.addEventListener("click", async () => {
    const failure = await copyText(command);
    copy.textContent = failure === null ? "Copied" : `Could not copy: ${failure}`;
  });
  note.append(" ", code, copy);
}
