// @ts-check
// 会話パネルとエージェントとの往復（R-VIEW、R-AGENT-HAND、R-AGENT-STATE）の純ロジック。DOM には触れない。

/**
 * 渡した 1 回分の行（R-AGENT-HAND）。`thread` が null なら発言だけの 1 回分の、並びの末尾の行。
 * @typedef {{ thread: string | null, state: "pending" | "working" }} HandLine
 * `review` は写す `kemi wait <id>` の id。エージェント用の API が無いレビューでは null。
 * @typedef {{ called: boolean, status: string, unhanded: number, lines?: HandLine[], review?: string | null }} AgentState
 */

/**
 * 状態の呼び名。人が次に何をすればよいかが分かる言い方にする（R-AGENT-STATE）。
 * @type {Record<string, string>}
 */
const AGENT_STATUS_LABELS = {
  "not-connected": "Not connected",
  waiting: "Ready — you can hand",
  working: "Working…",
  replied: "Replied",
  "no-response": "Not responding",
};

/**
 * エージェントの状態の画面での呼び名（R-AGENT-STATE）。知らない状態はサーバの名前のまま。
 * @param {string} status
 * @returns {string}
 */
export function agentStatusLabel(status) {
  return AGENT_STATUS_LABELS[status] || status;
}

/**
 * そのスレッド（`null` なら発言だけの 1 回分の、並びの末尾）の渡した 1 回分の行の状態。行が無ければ null。
 * @param {HandLine[] | undefined} lines
 * @param {string | null} thread
 * @returns {"pending" | "working" | null}
 */
export function handLineState(lines, thread) {
  return (lines ?? []).find((line) => line.thread === thread)?.state ?? null;
}

/**
 * 「Hand to agent」の押せる・押せないと、そばに出す案内（R-AGENT-STATE）。ボタンは状態によらず出す。
 * `kemi wait` の前は押せず、渡せるようになる `kemi wait <id>` を写せるようにする。エージェント用の API が無い
 * レビュー（`review` が null）は写すものが無いので、つながれないことを言う。`review` が無い（サーバの答えをまだ
 * 受け取っていない）間は、つながれるか分からないので案内を出さない。呼ばれた後は、渡すもの（前に渡した後の変化）が
 * あるときだけ押せる。
 * @param {AgentState} agent
 * @param {boolean} submitted
 * @returns {{ disabled: boolean, note: "connect" | "unavailable" | "nothing" | null, command: string | null }}
 */
export function handControl(agent, submitted) {
  if (!agent.called) {
    if (agent.review === undefined) {
      return { disabled: true, note: null, command: null };
    }
    const review = agent.review;
    return review === null
      ? { disabled: true, note: "unavailable", command: null }
      : { disabled: true, note: "connect", command: `kemi wait ${review}` };
  }
  if (agent.unhanded === 0) {
    return { disabled: true, note: "nothing", command: null };
  }
  return { disabled: submitted, note: null, command: null };
}

/**
 * 返信と発言を書いた人の呼び名。
 * @param {string} author
 * @returns {string}
 */
export function authorLabel(author) {
  return author === "agent" ? "Agent" : "You";
}

/**
 * 同じ id のコメントだけを差し替えた新しい並び。知らない id なら同じ中身の並び。
 * @param {any[]} comments
 * @param {any} updated
 * @returns {any[]}
 */
export function replaceComment(comments, updated) {
  return comments.map((comment) => (comment.id === updated.id ? updated : comment));
}

/**
 * 発言を足した新しい並び。書いた画面には応答と通知の両方で届くので、同じ id は足さない。
 * @param {any[]} messages
 * @param {any} message
 * @returns {any[]}
 */
export function addMessage(messages, message) {
  if (messages.some((known) => known.id === message.id)) {
    return messages;
  }
  return [...messages, message];
}

// ---- 会話パネル（R-VIEW、R-AGENT-HAND） ----

/**
 * どこまで読んだか。ページを開いている間だけ持ち、保存しない（R-VIEW）。
 * `loaded` はページを読み込んだ時点の最後の通し番号で、それまでにあったものは新着にしない。
 * `opened` はスレッドごとの、最後に開いた時点のそのスレッドの最後の通し番号。
 * `messages` は会話パネルを最後に見ていた時点の、最後の発言の通し番号。
 * @typedef {{ loaded: number, opened: Map<string, number>, messages: number }} ReadMarks
 */

/**
 * @typedef {{ kind: "thread", comment: any, seq: number }
 *   | { kind: "message", message: any, seq: number }} ConversationItem
 */

/** @typedef {"all" | "unresolved" | "file"} ConversationFilter */

/**
 * スレッドの最後の書き込み（コメントか、最後の返信）の通し番号。
 * @param {any} comment
 * @returns {number}
 */
export function threadLastSeq(comment) {
  return Math.max(
    Number(comment.seq) || 0,
    ...(comment.replies || []).map((/** @type {any} */ reply) => Number(reply.seq) || 0),
  );
}

/**
 * 会話パネルの並び。発言とすべてのスレッドを、最後に書き込みがあった順に 1 本にする。
 * 返信が書かれたスレッドは一番後ろへ移る。
 * @param {any[]} comments
 * @param {any[]} messages
 * @returns {ConversationItem[]}
 */
export function conversationItems(comments, messages) {
  /** @type {ConversationItem[]} */
  const items = [
    ...comments.map((comment) => ({
      kind: /** @type {const} */ ("thread"),
      comment,
      seq: threadLastSeq(comment),
    })),
    ...messages.map((message) => ({
      kind: /** @type {const} */ ("message"),
      message,
      seq: Number(message.seq) || 0,
    })),
  ];
  return items.sort((a, b) => a.seq - b.seq);
}

/**
 * 会話パネルの絞り込み。「すべて」は発言とスレッドのすべて。「未解決」と「表示中のファイル」は
 * スレッドを絞るもので、どのスレッドにも属さない発言は出さない。表示中のファイルは
 * グループとパスで見分ける（コメントの JSON はファイル id を持たない）。
 * @param {ConversationItem[]} items
 * @param {ConversationFilter} filter
 * @param {{ group_id: string, path: string } | null} file
 * @returns {ConversationItem[]}
 */
export function filterConversation(items, filter, file) {
  if (filter === "all") {
    return items;
  }
  return items.filter((item) => {
    if (item.kind !== "thread") {
      return false;
    }
    if (filter === "unresolved") {
      return !item.comment.resolved;
    }
    return (
      file !== null && item.comment.group_id === file.group_id && item.comment.path === file.path
    );
  });
}

/**
 * 会話パネルのスレッドの項目の、グループ単位と件名。コミット範囲で、付けたコミットが履歴の
 * 書き換えで消えていれば vanished（「消えたコミット」）。コミットごとの単位をまだ
 * 読んでいない（commitGroups が null）ときは消えたと決めない。
 * @param {any} comment
 * @param {{ range: boolean, commitGroups: Map<string, string> | null }} context
 * @returns {{ unit: string | null, subject: string, vanished: boolean }}
 */
export function describeComment(comment, context) {
  if (!context.range || comment.explanation || comment.target_type === "explanation") {
    return { unit: null, subject: "", vanished: false };
  }
  if (comment.group_id === "all") {
    return { unit: "file", subject: "", vanished: false };
  }
  const vanished = context.commitGroups !== null && !context.commitGroups.has(comment.group_id);
  return { unit: "commit", subject: comment.group_title || "", vanished };
}

/**
 * スレッドに新着があるか。最後に開いた後（開いていなければ読み込んだ後）に届いた
 * エージェントの返信があれば新着。
 * @param {any} comment
 * @param {ReadMarks} marks
 * @returns {boolean}
 */
export function threadUnread(comment, marks) {
  const read = Math.max(marks.loaded, marks.opened.get(comment.id) ?? 0);
  return (comment.replies || []).some(
    (/** @type {any} */ reply) => reply.author === "agent" && Number(reply.seq) > read,
  );
}

/**
 * スレッドを畳んでいるか。解決したスレッドは既定で畳み、畳むと開くの操作はその既定を
 * 上書きする（R-AGENT-HAND）。
 * @param {any} comment
 * @param {Map<string, boolean>} folded
 * @returns {boolean}
 */
function threadFolded(comment, folded) {
  return folded.get(comment.id) ?? Boolean(comment.resolved);
}

/**
 * コメントの札に出す、本文の最初の空でない行。
 * @param {string | {body: string|null, parts?: unknown[]|null}} body
 * @returns {string}
 */
export function firstLine(body) {
  return conversationText(body).split("\n").find((line) => line.trim() !== "") ?? "";
}

/**
 * @param {string | {body: string|null, parts?: unknown[]|null}} content
 * @returns {string}
 */
function conversationText(content) {
  if (typeof content === "string") return content;
  if (content.body !== null) return content.body;
  return content.parts ? JSON.stringify(content.parts, null, 2) : "";
}

/**
 * 差分の中の札と会話パネルの項目が示すもの（R-VIEW）。
 * @param {any} comment
 * @param {ReadMarks} marks
 * @param {Map<string, boolean>} folded
 * @returns {{ first: string, replies: number, unread: boolean, folded: boolean, resolved: boolean }}
 */
export function threadChip(comment, marks, folded) {
  return {
    first: firstLine(comment.body),
    replies: (comment.replies || []).length,
    unread: threadUnread(comment, marks),
    folded: threadFolded(comment, folded),
    resolved: Boolean(comment.resolved),
  };
}

/**
 * 畳んだ会話パネルの帯に出す新着の数。新着の印が付いたスレッドの数と、パネルを畳んでいる
 * 間に届いたエージェントの発言の数の合計（R-VIEW）。
 * @param {any[]} comments
 * @param {any[]} messages
 * @param {ReadMarks} marks
 * @returns {number}
 */
export function unreadCount(comments, messages, marks) {
  const read = Math.max(marks.loaded, marks.messages);
  const threads = comments.filter((comment) => threadUnread(comment, marks)).length;
  const arrived = messages.filter(
    (message) => message.author === "agent" && Number(message.seq) > read,
  ).length;
  return threads + arrived;
}

/**
 * スレッドを開いている間に別のスレッドへ届いた返信の知らせ（R-AGENT-HAND）。開いているスレッドのほかで新着の
 * あるスレッドのうち、エージェントの返信が最後に届いたもの。スレッドを開いていなければ（一覧では札の新着の印が
 * 示すので）null。
 * @param {any[]} comments
 * @param {ReadMarks} marks
 * @param {string | null} open 開いているスレッドの id
 * @returns {any | null}
 */
export function otherThreadNews(comments, marks, open) {
  if (open === null) {
    return null;
  }
  const lastAgentReply = (/** @type {any} */ comment) =>
    Math.max(0, ...(comment.replies || []).filter((/** @type {any} */ reply) => reply.author === "agent").map((/** @type {any} */ reply) => Number(reply.seq) || 0));
  return (
    comments
      .filter((comment) => comment.id !== open && threadUnread(comment, marks))
      .reduce((latest, comment) => (latest === null || lastAgentReply(comment) > lastAgentReply(latest) ? comment : latest), null)
  );
}

/** 並びの一番下とみなす余白。行の高さの半分ほどで、端数のずれを一番下として扱う。 */
const FOLLOW_MARGIN = 12;

/**
 * 並びの一番下を見ているか。見ていれば、新しいものが届いたときにそこまでついていく
 * （R-AGENT-HAND）。
 * @param {{ scrollTop: number, clientHeight: number, scrollHeight: number }} view
 * @returns {boolean}
 */
export function followsNewest(view) {
  return view.scrollHeight - view.scrollTop - view.clientHeight <= FOLLOW_MARGIN;
}
