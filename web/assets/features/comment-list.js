// @ts-check
// コメントの一覧の開閉と、一覧から該当の行へ移る処理。

import * as api from "../api.js";
import { dom } from "../dom.js";
import { state } from "../state.js";
import { jumpToEntry } from "./files.js";
import { switchUnit } from "./units.js";
import { renderCommentList } from "../views/comment-list.js";
import { showToast } from "../views/overlay.js";

/** @typedef {import("../state.js").Entry} Entry */

function openCommentList() {
  renderCommentList();
  dom.commentList.hidden = false;
  dom.btnComments.setAttribute("aria-expanded", "true");
  void loadCommitGroups();
}

export function closeCommentList() {
  dom.commentList.hidden = true;
  dom.btnComments.setAttribute("aria-expanded", "false");
}

export function toggleCommentList() {
  if (dom.commentList.hidden) {
    openCommentList();
  } else {
    closeCommentList();
  }
}

/**
 * 一覧の外を押したら閉じる（入口のボタンの上は外と見なさない）。
 * @param {MouseEvent} event
 */
export function closeCommentListOnOutsideClick(event) {
  const target = /** @type {Node} */ (event.target);
  if (
    !dom.commentList.hidden &&
    !dom.commentList.contains(target) &&
    !dom.btnComments.contains(target)
  ) {
    closeCommentList();
  }
}

/**
 * コメント一覧で消えたコミットを見分けられるよう、作ってあるコミットごとの単位を
 * まだ読んでいなければ読む（再取得の後は表示中の単位しか控えていない）。
 */
async function loadCommitGroups() {
  const status = state.units.find((candidate) => candidate.unit === "commit");
  if (!status || status.state !== "ready" || state.reviews.has("commit")) {
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
  if (reviews === state.reviews && !dom.commentList.hidden) {
    renderCommentList();
  }
}

/**
 * コメント一覧から、そのコメントの単位へ切り替えて、その行へ移る。
 * @param {any} comment
 * @param {string | null} unit
 */
export async function goToComment(comment, unit) {
  const line =
    comment.start_line === null || comment.start_line === undefined
      ? null
      : Number(comment.start_line);
  state.commentOpen.set(comment.id, true);
  /** @param {Entry[]} entries */
  const find = (entries) =>
    entries.findIndex(
      (entry) => entry.group.id === comment.group_id && entry.file.path === comment.path,
    );
  if (unit && unit !== state.unit) {
    // 移り先が無ければ、一覧を開き直して（消えたコミットならそう示して）本文を見せる。
    const missing = () => {
      openCommentList();
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
