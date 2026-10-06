// @ts-check
// 会話パネル（R-VIEW）の開閉と幅、一覧とスレッドの行き来、絞り込み、畳み、スレッドから
// その行へ移る処理と、書く欄（全体への発言と返信）の受け付け。広い画面では右の列、狭い画面
// では画面いっぱいのシート（R-NARROW）。

import * as api from "../api.js";
import { dom } from "../dom.js";
import {
  CONVERSATION_MAX_WIDTH,
  CONVERSATION_MIN_WIDTH,
  conversationShown,
  currentEntry,
  entryOfComment,
  fileCacheKey,
  state,
} from "../state.js";
import { saveConversationOpen, saveConversationWidth } from "../storage.js";
import { followsNewest, threadLastSeq } from "../model.js";
import { onResize, remeasureAndRender } from "./display.js";
import { jumpToEntry } from "./files.js";
import { refreshRendered } from "./rendered.js";
import { openCommentEditor, updateComments } from "./comments.js";
import { postMessage, replyTo } from "./agent.js";
import { switchUnit } from "./units.js";
import { renderConversation } from "../views/conversation.js";
import { showToast } from "../views/overlay.js";

/** @typedef {import("../state.js").Entry} Entry */

/**
 * 広い画面では列の幅が変わるので、差分の折返しと帯を測り直させる。
 */
function layoutChanged() {
  if (!state.narrow) {
    onResize();
  }
}

/**
 * ページを読み込んだ時点の最後の通し番号を覚える。それまでにあったものは新着にしない（R-VIEW）。
 */
export function markLoaded() {
  const seqs = [
    ...state.allComments.map(threadLastSeq),
    ...state.messages.map((message) => Number(message.seq) || 0),
  ];
  const loaded = Math.max(0, ...seqs);
  state.conversation.read.loaded = loaded;
  state.conversation.read.messages = loaded;
}

/** 会話パネルを見ている間に届いた発言は、帯の新着の数に入れない。 */
function markMessagesRead() {
  const read = state.conversation.read;
  read.messages = Math.max(read.messages, ...state.messages.map((message) => Number(message.seq) || 0));
}

/** 開いているスレッドのコメント。一覧を見ていれば undefined。 */
function openedThread() {
  return state.allComments.find((candidate) => candidate.id === state.conversation.thread);
}

/**
 * スレッドを見せたので、届いていた返信を読んだことにする（R-VIEW）。
 * @param {any} comment
 */
function markThreadRead(comment) {
  state.conversation.read.opened.set(comment.id, threadLastSeq(comment));
}

/**
 * スレッドを見せた後の描き直し。札の新着の印を消し、表示中でないファイルなら対象の行の前後を読む。
 * @param {any} comment
 */
function afterThreadShown(comment) {
  remeasureAndRender();
  refreshRendered();
  void loadThreadLines(comment);
}

export function openConversation() {
  markMessagesRead();
  // 前に開いていたスレッドのまま開けば、そのスレッドを見せることになる。
  const comment = openedThread();
  if (comment) {
    markThreadRead(comment);
  }
  if (state.narrow) {
    state.conversation.sheetOpen = true;
  } else {
    state.conversation.panelOpen = true;
    saveConversationOpen(true);
  }
  renderConversation({ toEnd: true });
  layoutChanged();
  if (comment) {
    afterThreadShown(comment);
  }
  void loadCommitGroups();
}

export function closeConversation() {
  if (state.narrow) {
    state.conversation.sheetOpen = false;
  } else {
    state.conversation.panelOpen = false;
    saveConversationOpen(false);
  }
  renderConversation();
  layoutChanged();
}

export function toggleConversation() {
  if (conversationShown()) {
    closeConversation();
  } else {
    openConversation();
  }
}

/** 狭い画面のシートを閉じる（幅をまたいだとき）。広い画面の開閉の好みは変えない。 */
export function closeSheet() {
  state.conversation.sheetOpen = false;
  renderConversation();
}

/**
 * スレッドを開く。会話パネルを畳んでいれば開き、パネル全体をそのスレッドにする。
 * @param {string} id
 */
export function openThread(id) {
  state.conversation.thread = id;
  if (!conversationShown()) {
    openConversation();
    return;
  }
  const comment = openedThread();
  if (comment) {
    markThreadRead(comment);
  }
  renderConversation({ toEnd: true });
  if (comment) {
    afterThreadShown(comment);
  }
}

/**
 * 開いたスレッドの対象の行の前後を見せるため、表示中でないファイルのスレッドなら、そのファイルの
 * 行を読んでキャッシュに入れる（R-VIEW）。読めなければ、覚えている対象の行（quote）のまま。
 * @param {any} comment
 */
async function loadThreadLines(comment) {
  const entry = comment.outdated ? null : entryOfComment(comment);
  if (!entry || comment.start_line === null || comment.start_line === undefined) {
    return;
  }
  const key = fileCacheKey(entry);
  if (state.cache.has(key)) {
    return;
  }
  const reviews = state.reviews;
  /** @type {any} */
  let data;
  try {
    data = await api.getFile(entry.file.id, null, {
      dark: state.dark,
      highlight: state.highlightOverrides.get(entry.file.id),
    });
  } catch {
    return;
  }
  // 読み直し（テーマや更新）で消えたキャッシュへ、古い行を戻さない。表示中のファイルとして
  // 先に読み終えていれば、そちらを残す。
  if (reviews !== state.reviews || fileCacheKey(entry) !== key || state.cache.has(key)) {
    return;
  }
  const rows = data.rows || [];
  state.cache.set(key, { ...data, rows, collapsedRows: rows });
  // 後で返信や解決があっても、そのファイルを選んだときに読み直した当時のコメントが出ないよう、
  // 差し替えの届く控えへ入れておく。
  if (!state.commentStore.has(entry.file.id)) {
    state.commentStore.set(entry.file.id, data.comments || []);
  }
  if (state.conversation.thread === comment.id) {
    renderConversation();
  }
}

/**
 * 会話パネルで開いているスレッドの対象の行の前後を読み直す。再取得で行のキャッシュが消えた後に
 * 呼ぶ。
 */
export function reloadThreadLines() {
  const comment = openedThread();
  if (conversationShown() && comment) {
    void loadThreadLines(comment);
  }
}

/** 届いたことを示す印を押した。見えている並びを一番下へ送る。 */
export function showNewest() {
  const list = dom.cvThread.hidden ? dom.cvItems : dom.cvThreadBody;
  list.scrollTop = list.scrollHeight;
  hideNewer();
}

function hideNewer() {
  dom.cvNewer.hidden = true;
  delete dom.cvNewer.dataset.count;
}

/**
 * 並びを一番下まで読んだら、届いたことを示す印を消す。
 * @param {Event} event
 */
export function onConversationScroll(event) {
  if (!dom.cvNewer.hidden && followsNewest(/** @type {HTMLElement} */ (event.target))) {
    hideNewer();
  }
}

/** 開いたスレッドから一覧へ戻る。 */
export function closeThread() {
  state.conversation.thread = null;
  state.conversation.editing = null;
  renderConversation({ toEnd: true });
}

/**
 * ページへのコメントの本文の編集を、開いたスレッドの中で始める（null でやめる）。場所は変えない
 * （live.md の R-PAGE-COMMENT。コードへのコメントは差分の中の入力欄で編集する）。
 * @param {string | null} id
 */
export function editPageComment(id) {
  state.conversation.editing = id;
  renderConversation();
}

/**
 * ページへのコメントの本文を保存する。
 * @param {any} comment
 * @param {string} body
 */
export async function savePageCommentBody(comment, body) {
  try {
    const updated = await api.postComment({ op: "edit", id: comment.id, body });
    state.conversation.editing = null;
    updateComments((comments) => comments.map((item) => (item.id === updated.id ? updated : item)));
  } catch (error) {
    showToast(`could not edit the comment: ${error instanceof Error ? error.message : String(error)}`);
  }
}

/**
 * @param {string} filter
 */
export function setConversationFilter(filter) {
  if (filter !== "all" && filter !== "unresolved" && filter !== "file") {
    return;
  }
  state.conversation.filter = filter;
  renderConversation({ toEnd: true });
}

/**
 * スレッドを畳む・開く（R-AGENT-HAND）。差分の中の札の形も変わる。
 * @param {string} id
 * @param {boolean} folded
 */
export function setThreadFolded(id, folded) {
  state.conversation.folded.set(id, folded);
  renderConversation();
  // 札の形が変わり、行の高さも変わるので測り直させる。
  remeasureAndRender();
  refreshRendered();
}

/**
 * 会話パネルで消えたコミットを見分けられるよう、会話パネルが見えていれば、作ってある
 * コミットごとの単位をまだ読んでいなければ読む（起動の直後と再取得の後は、表示中の単位しか
 * 控えていない）。パネルを開いたとき、開いたまま起動したとき、単位ができたという通知
 * （再取得で作り直した後にも届く）を受けたときに呼ぶ。
 */
export async function loadCommitGroups() {
  const status = state.units.find((candidate) => candidate.unit === "commit");
  if (!conversationShown() || !status || status.state !== "ready" || state.reviews.has("commit")) {
    return;
  }
  const reviews = state.reviews;
  try {
    const review = await api.getReview(false, "commit");
    if (!reviews.has("commit")) {
      reviews.set("commit", review);
    }
  } catch {
    // 読めなければ消えたかどうかを決めないまま。押したときに移り先を確かめる。
    return;
  }
  if (reviews === state.reviews) {
    renderConversation();
    // 開いているコミットごとのスレッドのファイルは、ここで初めて分かることがある。
    const comment = openedThread();
    if (comment) {
      void loadThreadLines(comment);
    }
  }
}

/**
 * スレッドから、そのコメントの単位へ切り替えて、その行へ移る。狭い画面ではシートを閉じて
 * その行を見せる（R-NARROW）。
 * @param {any} comment
 * @param {string | null} unit
 */
export async function goToComment(comment, unit) {
  const line =
    comment.start_line === null || comment.start_line === undefined
      ? null
      : Number(comment.start_line);
  if (state.narrow) {
    closeSheet();
  }
  /** @param {Entry[]} entries */
  const find = (entries) =>
    entries.findIndex(
      (entry) => entry.group.id === comment.group_id && entry.file.path === comment.path,
    );
  if (unit && unit !== state.unit) {
    // 移り先が無ければ、会話パネルにスレッドを開いたまま（消えたコミットならそう示して）知らせる。
    const missing = () => {
      openThread(comment.id);
      showToast("could not find the target file");
    };
    await switchUnit(unit, { find, side: comment.side, line, missing });
    return;
  }
  const index = find(state.entries);
  if (index < 0) {
    return;
  }
  const entry = state.entries[index];
  await jumpToEntry(entry, { side: comment.side, line });
}

/**
 * 開いたスレッドの「編集」。編集の入力欄は差分の中のそのコメントの位置に開くので、
 * その行へ移ってから開く。
 * @param {any} comment
 * @param {string | null} unit
 */
export async function editFromThread(comment, unit) {
  await goToComment(comment, unit);
  const entry = currentEntry();
  if (entry && entry.group.id === comment.group_id && entry.file.path === comment.path) {
    openCommentEditor(comment);
  }
}

/**
 * 縁を掴んで会話パネルの幅を変える。幅は離したときに覚える（R-SERVE）。
 * @param {PointerEvent} event
 */
export function startResize(event) {
  if (state.narrow || !state.conversation.panelOpen || event.button !== 0) {
    return;
  }
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = state.conversation.width;
  dom.cvResizer.setPointerCapture(event.pointerId);
  /** @param {PointerEvent} move */
  const onMove = (move) => {
    const width = Math.min(
      CONVERSATION_MAX_WIDTH,
      Math.max(CONVERSATION_MIN_WIDTH, startWidth + (startX - move.clientX)),
    );
    state.conversation.width = width;
    dom.conversation.style.setProperty("--cv-width", `${width}px`);
    layoutChanged();
  };
  const onUp = () => {
    dom.cvResizer.removeEventListener("pointermove", onMove);
    dom.cvResizer.removeEventListener("pointerup", onUp);
    dom.cvResizer.removeEventListener("pointercancel", onUp);
    saveConversationWidth(state.conversation.width);
  };
  dom.cvResizer.addEventListener("pointermove", onMove);
  dom.cvResizer.addEventListener("pointerup", onUp);
  dom.cvResizer.addEventListener("pointercancel", onUp);
}

/**
 * 書く欄で Cmd/Ctrl+Enter を押すと送る。
 * @param {KeyboardEvent} event
 */
export function submitOnModEnter(event) {
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
    event.preventDefault();
    const form = /** @type {HTMLTextAreaElement} */ (event.target).form;
    form?.requestSubmit();
  }
}

/**
 * 全体への発言を書く。
 * @param {SubmitEvent} event
 */
export function submitMessage(event) {
  event.preventDefault();
  const body = dom.cvMessage.value;
  if (body.trim() !== "") {
    void postMessage(body);
  }
}

/**
 * 開いたスレッドへ返信を書く。
 * @param {SubmitEvent} event
 */
export function submitReply(event) {
  event.preventDefault();
  const comment = openedThread();
  const body = dom.cvReplyText.value;
  if (comment && body.trim() !== "") {
    void replyTo(comment, body);
  }
}

/** 返信の書きかけは、描き直しやスレッドの行き来で消えないようページの状態に置く。 */
export function keepReplyDraft() {
  const id = state.conversation.thread;
  if (id !== null) {
    state.replyDrafts.set(id, dom.cvReplyText.value);
  }
}

/**
 * 絞り込みのボタン。
 * @param {MouseEvent} event
 */
export function chooseFilter(event) {
  const choice = /** @type {HTMLElement} */ (event.target).closest("button");
  if (choice && choice.dataset.filter) {
    setConversationFilter(choice.dataset.filter);
  }
}
