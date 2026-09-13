// @ts-check
// 上部の入口から開く、すべてのグループ単位のコメントの一覧。

import { actions } from "../actions.js";
import { button, dom, el, textEl } from "../dom.js";
import { commitGroups, state, unitLabel } from "../state.js";
import { commentLabel, countLabel, describeComment, firstLine } from "../model.js";

/** 上部の入口から開く、すべてのグループ単位のコメントの一覧（常設のパネルではない）。 */
export function renderCommentList() {
  dom.commentList.textContent = "";
  const head = el("div", "cl-head");
  head.append(textEl("b", "", countLabel(state.allComments.length, "comment")));
  const close = button("cl-close");
  close.textContent = "Close";
  close.addEventListener("click", () => actions.closeCommentList());
  head.append(close);
  dom.commentList.append(head);
  if (state.allComments.length === 0) {
    dom.commentList.append(textEl("p", "cl-empty", "No comments yet"));
    return;
  }
  const context = { range: state.units.length > 0, commitGroups: commitGroups() };
  const list = el("ul", "cl-items");
  for (const comment of state.allComments) {
    const info = describeComment(comment, context);
    const item = el("li", "cl-item");
    const target = button("cl-target");
    target.disabled = info.vanished;
    const meta = el("span", "cl-meta");
    if (info.vanished) {
      meta.append(textEl("span", "cl-unit vanished", "Vanished commit"));
    } else if (info.unit) {
      meta.append(textEl("span", "cl-unit", unitLabel(info.unit)));
    }
    meta.append(
      textEl("span", "cl-path", comment.path),
      textEl("span", "cl-where", commentLabel(comment)),
    );
    if (comment.outdated || info.vanished) {
      meta.append(textEl("span", "t-outdated-mark", "Outdated comment"));
    }
    target.append(meta);
    if (info.subject) {
      target.append(textEl("span", "cl-subject", info.subject));
    }
    target.append(textEl("span", "cl-first", firstLine(comment.body)));
    if (!info.vanished) {
      target.addEventListener("click", () => {
        actions.closeCommentList();
        actions.goToComment(comment, info.unit);
      });
    }
    item.append(target);
    // 消えたコミットのコメントは移り先が無いので、本文をここで見せる。
    if (info.vanished) {
      item.append(textEl("p", "cl-body", comment.body));
    }
    list.append(item);
  }
  dom.commentList.append(list);
}
