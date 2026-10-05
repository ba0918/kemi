// @ts-check
// 会話パネル（R-VIEW、agent-channel.md）: 畳んだ帯、見出しのエージェントの状態、一覧（発言と
// スレッドの項目、絞り込み）と、開いたスレッド（対象の行、本文・suggestion・返信と操作）。
// 書く欄（全体への発言と返信）は index.html に 1 つずつあり、ここでは作り直さない。届いた
// 通知で描き直しても、書きかけ・カーソル・変換中の文字が残る。

import { actions } from "../actions.js";
import { button, dom, el, textEl } from "../dom.js";
import { commitGroups, conversationShown, currentEntry, state, unitLabel } from "../state.js";
import {
  agentStatusLabel,
  authorLabel,
  commentLabel,
  conversationItems,
  describeComment,
  filterConversation,
  firstLine,
  followsNewest,
  handShown,
  threadChip,
  unreadCount,
} from "../model.js";

/** 開いたスレッドに見せる、対象の行の前後の行数。 */
const CONTEXT_LINES = 2;

/**
 * 会話パネルを今の状態に合わせる。畳んでいる間（狭い画面でシートを閉じている間）は中身を
 * 描かない。並びの位置は、`toEnd` のときだけ一番下へ送り、それ以外は動かさない。
 * @param {{ toEnd?: boolean }} [options]
 */
export function renderConversation(options = {}) {
  const shown = conversationShown();
  dom.conversation.dataset.open = String(shown);
  dom.conversation.style.setProperty("--cv-width", `${state.conversation.width}px`);
  dom.btnComments.setAttribute("aria-expanded", String(shown));
  renderAgentState();
  if (!shown) {
    return;
  }
  const comment = openThreadComment();
  dom.cvList.hidden = comment !== null;
  dom.cvThread.hidden = comment === null;
  dom.cvCompose.hidden = state.submitted;
  dom.cvReply.hidden = state.submitted;
  // 渡す操作は、見えている書く欄の操作の並びに置く（要素は 1 つ。移すだけで作り直さない）。
  const row = comment ? dom.cvReplyActions : dom.cvComposeActions;
  if (dom.btnHand.parentElement !== row) {
    row.prepend(dom.btnHand);
  }
  if (comment) {
    renderThread(comment, options);
  } else {
    renderList(options);
  }
}

/** 開いているスレッドのコメント。消えていれば null（一覧を出す）。 */
function openThreadComment() {
  const id = state.conversation.thread;
  if (id === null) {
    return null;
  }
  return state.allComments.find((comment) => comment.id === id) ?? null;
}

/**
 * エージェントの状態（見出しと畳んだ帯）と「Hand to agent」。「Hand to agent」は `kemi wait` が
 * 一度でも呼ばれたレビューでだけ出す（R-AGENT-STATE）。
 */
export function renderAgentState() {
  const agent = state.agent;
  const label = agentStatusLabel(agent.status);
  dom.agentStatus.dataset.status = agent.status;
  dom.agentStatus.textContent = label;
  dom.railStatus.dataset.status = agent.status;
  dom.railStatus.title = label;
  dom.btnHand.hidden = !handShown(agent);
  dom.handCount.textContent = agent.unhanded > 0 ? ` ${agent.unhanded}` : "";
  dom.btnHand.disabled = state.submitted || agent.unhanded === 0;
  const unread = unreadCount(state.allComments, state.messages, state.conversation.read);
  dom.cvUnread.hidden = unread === 0;
  dom.cvUnread.textContent = String(unread);
  dom.cvRail.setAttribute(
    "aria-label",
    unread > 0 ? `Open the conversation (${unread} new)` : "Open the conversation",
  );
}

/**
 * 並びを描き直す。一番下を見ていれば（と、`toEnd` のとき）新しいものが見えるところまで
 * ついていき、上のほうを見ていれば位置を変えずに、届いた数の印を並びの下端に出す
 * （R-AGENT-HAND）。届いたかどうかは、前に描いた並びの最後の通し番号と比べて決める。
 * @param {HTMLElement} list
 * @param {{ toEnd?: boolean }} options
 * @param {number[]} seqs 描く並びの各項目の通し番号
 * @param {() => void} fill
 */
function followScroll(list, options, seqs, fill) {
  const newest = Math.max(0, ...seqs);
  const shownNewest = Number(list.dataset.newest ?? newest);
  const following = Boolean(options.toEnd) || followsNewest(list);
  const top = list.scrollTop;
  fill();
  list.dataset.newest = String(newest);
  list.after(dom.cvNewer);
  if (following) {
    list.scrollTop = list.scrollHeight;
    dom.cvNewer.hidden = true;
    delete dom.cvNewer.dataset.count;
    return;
  }
  list.scrollTop = top;
  const arrived = seqs.filter((seq) => seq > shownNewest).length;
  if (arrived > 0) {
    const count = Number(dom.cvNewer.dataset.count ?? 0) + arrived;
    dom.cvNewer.dataset.count = String(count);
    dom.cvNewer.textContent = `${count} new ↓`;
    dom.cvNewer.hidden = false;
  }
}

/**
 * 一覧: 発言とスレッドを最後に書き込みがあった順に並べ、絞り込みを効かせる。
 * @param {{ toEnd?: boolean }} options
 */
function renderList(options) {
  const filter = state.conversation.filter;
  for (const choice of dom.cvFilter.querySelectorAll("button")) {
    choice.setAttribute("aria-pressed", String(choice.dataset.filter === filter));
  }
  const entry = currentEntry();
  const file = entry ? { group_id: entry.group.id, path: entry.file.path } : null;
  const items = filterConversation(
    conversationItems(state.allComments, state.messages),
    filter,
    file,
  );
  const context = { range: state.units.length > 0, commitGroups: commitGroups() };
  followScroll(dom.cvItems, options, items.map((item) => item.seq), () => {
    dom.cvItems.textContent = "";
    if (items.length === 0) {
      dom.cvItems.append(textEl("li", "cv-empty", "Nothing here yet"));
      return;
    }
    for (const item of items) {
      const row = el("li");
      row.append(
        item.kind === "thread" ? threadCard(item.comment, context) : messageRow(item.message),
      );
      dom.cvItems.append(row);
    }
  });
}

/**
 * 発言 1 つ。書いた人と本文。
 * @param {any} message
 * @returns {HTMLElement}
 */
function messageRow(message) {
  return post(message.author, message.body, "cv-msg", message.id);
}

/**
 * 書き込み 1 つ（発言・コメントの本文・返信）。書いた人で見分ける。
 * @param {string} author
 * @param {string} body
 * @param {string} className
 * @param {string} id
 * @returns {HTMLElement}
 */
function post(author, body, className, id) {
  const item = el("div", `cv-post ${className}`);
  item.dataset.author = author;
  item.dataset.id = id;
  item.append(textEl("span", "cv-who", authorLabel(author)), textEl("p", "cv-text", body));
  return item;
}

/**
 * スレッドの場所: グループ単位（消えたコミット）、パス、行範囲。
 * @param {any} comment
 * @param {{ unit: string | null, vanished: boolean }} info
 * @returns {HTMLElement}
 */
function place(comment, info) {
  const where = el("span", "cv-place");
  if (info.vanished) {
    where.append(textEl("span", "cv-unit vanished", "Vanished commit"));
  } else if (info.unit) {
    where.append(textEl("span", "cv-unit", unitLabel(info.unit)));
  }
  where.append(
    textEl("span", "cv-path", comment.path),
    textEl("span", "cv-where", commentLabel(comment)),
  );
  return where;
}

/**
 * 一覧のスレッドの項目。場所、件名（コミットごと）、最後の書き込みを示し、押すとパネル全体が
 * そのスレッドになる。畳んだスレッドは場所の 1 行だけにする（R-AGENT-HAND）。
 * @param {any} comment
 * @param {{ range: boolean, commitGroups: Map<string, string> | null }} context
 * @returns {HTMLElement}
 */
function threadCard(comment, context) {
  const info = describeComment(comment, context);
  const chip = threadChip(comment, state.conversation.read, state.conversation.folded);
  const card = button(`cv-card${chip.folded ? " folded" : ""}`);
  card.dataset.id = comment.id;
  const head = el("span", "cv-card-head");
  head.append(place(comment, info));
  if (comment.outdated && !info.vanished) {
    head.append(textEl("span", "t-outdated-mark", "Outdated comment"));
  }
  if (chip.resolved) {
    head.append(textEl("span", "t-resolved-mark", "Resolved"));
  }
  if (chip.unread) {
    head.append(textEl("span", "unread-mark", "New"));
  }
  if (!chip.folded && chip.replies > 0) {
    head.append(textEl("span", "reply-count", chip.replies === 1 ? "1 reply" : `${chip.replies} replies`));
  }
  card.append(head);
  if (!chip.folded) {
    if (info.subject) {
      card.append(textEl("span", "cv-subject", info.subject));
    }
    const replies = comment.replies || [];
    const last = replies.length > 0 ? replies[replies.length - 1] : { author: "reviewer", body: comment.body };
    const line = el("span", "cv-last");
    line.dataset.author = last.author;
    line.append(textEl("b", "cv-who", authorLabel(last.author)), textEl("span", "cv-first", firstLine(last.body)));
    card.append(line);
  }
  card.addEventListener("click", () => actions.openThread(comment.id));
  return card;
}

/**
 * 開いたスレッド。見出し（一覧へ戻る、場所、操作）、対象の行の前後、本文・suggestion・返信。
 * @param {any} comment
 * @param {{ toEnd?: boolean }} options
 */
function renderThread(comment, options) {
  const context = { range: state.units.length > 0, commitGroups: commitGroups() };
  const info = describeComment(comment, context);
  const chip = threadChip(comment, state.conversation.read, state.conversation.folded);
  dom.cvThreadHead.textContent = "";
  const top = el("div", "cv-thread-top");
  const back = button("cv-back");
  back.textContent = "‹ Conversation";
  back.addEventListener("click", () => actions.closeThread());
  top.append(back, place(comment, info));
  dom.cvThreadHead.append(top);

  const acts = el("div", "cv-acts");
  if (!info.vanished) {
    const go = button("cv-go");
    go.textContent = "Go to line";
    go.addEventListener("click", () => actions.goToComment(comment, info.unit));
    acts.append(go);
  }
  if (!state.submitted) {
    // 解決は人間だけが付ける（R-AGENT-HAND）。
    const resolve = button("cv-resolve");
    resolve.textContent = comment.resolved ? "Reopen" : "Resolve";
    resolve.addEventListener("click", () => actions.setResolved(comment, !comment.resolved));
    acts.append(resolve);
  }
  const fold = button("cv-fold");
  fold.textContent = chip.folded ? "Unfold" : "Fold";
  fold.addEventListener("click", () => actions.setThreadFolded(comment.id, !chip.folded));
  acts.append(fold);
  if (!state.submitted && !info.vanished) {
    const edit = button("cv-edit");
    edit.textContent = "Edit";
    edit.addEventListener("click", () => actions.editFromThread(comment, info.unit));
    acts.append(edit);
  }
  if (!state.submitted) {
    const remove = button("cv-delete");
    remove.textContent = "Delete";
    remove.addEventListener("click", () => actions.confirmDeleteComment(comment));
    acts.append(remove);
  }
  dom.cvThreadHead.append(acts);

  const posts = [comment, ...(comment.replies || [])].map((item) => Number(item.seq) || 0);
  followScroll(dom.cvThreadBody, options, posts, () => {
    dom.cvThreadBody.textContent = "";
    const lines = targetLines(comment);
    if (lines.length > 0) {
      const box = el("div", "cv-context");
      for (const line of lines) {
        const row = textEl("div", `cv-line ${line.tone}${line.hit ? " hit" : ""}`, line.text);
        box.append(row);
      }
      dom.cvThreadBody.append(box);
    }
    const marks = el("div", "cv-marks");
    if (info.subject) {
      marks.append(textEl("span", "cv-subject", info.subject));
    }
    if (comment.outdated || info.vanished) {
      marks.append(textEl("span", "t-outdated-mark", "Outdated comment"));
    }
    if (comment.resolved) {
      marks.append(textEl("span", "t-resolved-mark", "Resolved"));
    }
    if (marks.childElementCount > 0) {
      dom.cvThreadBody.append(marks);
    }
    const opening = post("reviewer", comment.body, "cv-comment", comment.id);
    if (comment.suggestion) {
      const box = el("div", "t-suggestion");
      box.append(textEl("div", "sug-head", "Suggested change"));
      const pre = el("pre");
      pre.textContent =
        comment.suggestion.replacement === "" ? "(line deletion)" : comment.suggestion.replacement;
      box.append(pre);
      opening.append(
        box,
        textEl(
          "div",
          "t-note",
          "This suggestion is sent to the agent as JSON together with the comment (the agent applies it).",
        ),
      );
    }
    if (comment.outdated) {
      opening.append(
        textEl(
          "div",
          "t-outdated",
          "Outdated comment — the file changed after this comment (line numbers are as of creation)",
        ),
      );
    }
    dom.cvThreadBody.append(opening);
    for (const reply of comment.replies || []) {
      dom.cvThreadBody.append(post(reply.author, reply.body, "cv-reply-post", reply.id));
    }
  });

  // 返信の欄は、開いたスレッドが変わったときだけ、そのスレッドの書きかけに入れ替える。
  if (dom.cvReply.dataset.thread !== comment.id) {
    dom.cvReply.dataset.thread = comment.id;
    dom.cvReplyText.value = state.replyDrafts.get(comment.id) ?? "";
  }
}

/**
 * 開いたスレッドの上に見せる対象の行。表示中のファイルのコメントなら、読んである行から
 * 対象の行の前後を添える。そうでなければ、コメントが覚えている対象の行（quote）だけ。
 * @param {any} comment
 * @returns {{ text: string, tone: string, hit: boolean }[]}
 */
function targetLines(comment) {
  if (comment.start_line === null || comment.start_line === undefined) {
    return [];
  }
  const start = Number(comment.start_line);
  const end = Number(comment.end_line ?? comment.start_line);
  const entry = currentEntry();
  const showing =
    entry !== null &&
    entry.group.id === comment.group_id &&
    entry.file.path === comment.path &&
    !comment.outdated;
  if (showing) {
    /** @type {{ text: string, tone: string, hit: boolean }[]} */
    const lines = [];
    for (const row of state.rows) {
      const line = comment.side === "new" ? row.new : row.old;
      if (!line) {
        continue;
      }
      const number = Number(line.number);
      if (number < start - CONTEXT_LINES || number > end + CONTEXT_LINES) {
        continue;
      }
      const changed = row.kind !== "equal";
      const tone = !changed ? "" : comment.side === "new" ? "add" : "del";
      lines.push({ text: line.text, tone, hit: number >= start && number <= end });
    }
    if (lines.length > 0) {
      return lines;
    }
  }
  return (comment.quote || []).map((/** @type {string} */ text) => ({ text, tone: "", hit: true }));
}
