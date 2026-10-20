import assert from "node:assert/strict";
import { test } from "node:test";

import {
  addMessage,
  agentStatusLabel,
  authorLabel,
  conversationItems,
  describeComment,
  filterConversation,
  firstLine,
  followsNewest,
  handControl,
  handLineState,
  otherThreadNews,
  replaceComment,
  threadChip,
  threadUnread,
  unreadCount,
  withOutdatedFrom,
} from "./conversation-model.js";

test("会話の旧plain本文は装飾せず最初の行を示す", () => {
  assert.equal(firstLine({body:"**plain**\nsecond",parts:null}),"**plain**");
  assert.equal(firstLine({body:"",parts:null}),"");
});

test("最後の書き込みが parts だけなら、最初の text パーツの本文の 1 行目を示す", () => {
  const code = { type: "code", body: "x = 1", language: "python", role: "example" };
  const reply = { author: "agent", body: null, parts: [code, { type: "text", body: "\n**Done**\nmore" }, { type: "text", body: "later" }] };

  assert.equal(firstLine(reply), "**Done**");
});

test("text パーツの無い parts は、最初のパーツの種類の呼び名を示す", () => {
  const code = { type: "code", body: "x = 1", language: "python", role: "example" };
  const sequence = { type: "sequence", title: "Calls", role: "example", participants: [], events: [] };
  const flow = { type: "flow", title: "Steps", role: "proposal", nodes: [], edges: [] };
  const grid = { type: "diagram", title: "Grid", role: "example", source: "a = A" };

  assert.equal(firstLine({ body: null, parts: [code, sequence] }), "Code");
  assert.deepEqual(
    [sequence, flow, grid].map((part) => firstLine({ body: null, parts: [part, code] })),
    ["Diagram", "Diagram", "Diagram"],
  );
});

test("chip_shows_the_first_line_of_the_body", () => {
  assert.equal(firstLine("一行目\n二行目"), "一行目");
  assert.equal(firstLine("\n\n本文"), "本文");
  assert.equal(firstLine(""), "");
});

test("comment_list_names_the_unit_and_the_commit", () => {
  const groups = new Map([["abc", "feat: 足す"]]);

  assert.deepEqual(
    describeComment({ group_id: "all", group_title: "x...y" }, { range: true, commitGroups: groups }),
    { unit: "file", subject: "", vanished: false },
  );
  assert.deepEqual(
    describeComment({ group_id: "abc", group_title: "feat: 足す" }, { range: true, commitGroups: groups }),
    { unit: "commit", subject: "feat: 足す", vanished: false },
  );
  assert.deepEqual(
    describeComment({ group_id: "g1", group_title: "" }, { range: false, commitGroups: null }),
    { unit: null, subject: "", vanished: false },
  );
});

test("comment_list_marks_comments_of_vanished_commits", () => {
  const groups = new Map([["new-sha", "fix: 後"]]);

  assert.deepEqual(
    describeComment({ group_id: "old-sha", group_title: "fix: 前" }, { range: true, commitGroups: groups }),
    { unit: "commit", subject: "fix: 前", vanished: true },
  );
  assert.equal(
    describeComment({ group_id: "old-sha", group_title: "fix: 前" }, { range: true, commitGroups: null }).vanished,
    false,
  );
});

// エージェントとの往復（agent-channel.md）。状態の呼び名、操作を出すか、届いた書き込みの取り込み。
test("エージェントの 5 つの状態を見分けられる呼び名で出す", () => {
  const labels = ["not-connected", "waiting", "working", "replied", "no-response"].map(agentStatusLabel);

  for (const label of labels) {
    assert.ok(label.trim() !== "");
  }
  assert.equal(new Set(labels).size, 5);
});

test("「Hand to agent」は kemi wait の前は押せず、写す kemi wait <id> と案内を添える", () => {
  assert.deepEqual(handControl({ called: false, status: "not-connected", unhanded: 2, review: "01K5ABC" }, false), {
    disabled: true,
    note: "connect",
    command: "kemi wait 01K5ABC",
  });
});

test("エージェントがつながれないレビューでは「Hand to agent」は押せず、写すコマンドは無い", () => {
  assert.deepEqual(handControl({ called: false, status: "not-connected", unhanded: 2, review: null }, false), {
    disabled: true,
    note: "unavailable",
    command: null,
  });
});

test("レビューの id をサーバから受け取る前は、つながれないとは言わず押せないだけにする", () => {
  assert.deepEqual(handControl({ called: false, status: "not-connected", unhanded: 0 }, false), {
    disabled: true,
    note: null,
    command: null,
  });
});

test("kemi wait が呼ばれたら、渡すものがあるときだけ「Hand to agent」を押せる", () => {
  assert.deepEqual(handControl({ called: true, status: "waiting", unhanded: 2, review: "r" }, false), {
    disabled: false,
    note: null,
    command: null,
  });
  assert.deepEqual(handControl({ called: true, status: "replied", unhanded: 0, review: "r" }, false), {
    disabled: true,
    note: "nothing",
    command: null,
  });
});

test("submit した後は「Hand to agent」を押せない", () => {
  assert.equal(handControl({ called: true, status: "waiting", unhanded: 2, review: "r" }, true).disabled, true);
});

test("渡した 1 回分の行は、スレッドごとと、発言だけの並びの末尾のものを見分ける", () => {
  /** @type {import("./conversation-model.js").HandLine[]} */
  const lines = [
    { thread: "c1", state: "working" },
    { thread: null, state: "pending" },
  ];

  assert.equal(handLineState(lines, "c1"), "working");
  assert.equal(handLineState(lines, "c2"), null);
  assert.equal(handLineState(lines, null), "pending");
  assert.equal(handLineState(undefined, "c1"), null);
});

test("書いた人が人間かエージェントかを見分けられる呼び名で出す", () => {
  const reviewer = authorLabel("reviewer");
  const agent = authorLabel("agent");

  assert.ok(reviewer.trim() !== "" && agent.trim() !== "");
  assert.notEqual(reviewer, agent);
});

test("届いたコメントは同じ id のものだけを差し替える", () => {
  const comments = [
    { id: "c1", body: "one", replies: [] },
    { id: "c2", body: "two", replies: [] },
  ];
  const updated = { id: "c2", body: "two", replies: [{ id: "r1", author: "agent", body: "done" }] };

  const next = replaceComment(comments, updated);

  assert.deepEqual(next, [comments[0], updated]);
  assert.deepEqual(comments[1].replies, [], "the original list is not changed");
});

test("知らないコメントが届いても一覧は変わらない", () => {
  const comments = [{ id: "c1", body: "one", replies: [] }];

  assert.deepEqual(replaceComment(comments, { id: "c9", body: "x", replies: [] }), comments);
});

test("読んだファイルで古いと決め直したコメントは、手元の並びでも古いコメントになり、返信は手元のまま", () => {
  const comments = [
    { id: "c1", body: "one", outdated: false, replies: [{ id: "r1", author: "agent", body: "done" }] },
    { id: "c2", body: "two", outdated: false, replies: [] },
  ];
  const fetched = [{ id: "c1", body: "one", outdated: true, replies: [] }];

  const next = withOutdatedFrom(comments, fetched);

  assert.deepEqual(next[0], { ...comments[0], outdated: true });
  assert.equal(next[1], comments[1]);
  assert.equal(comments[0].outdated, false, "the original list is not changed");
});

test("読んだファイルのコメントの outdated が手元と同じなら、並びを作り直さない", () => {
  const comments = [{ id: "c1", body: "one", outdated: true, replies: [] }];

  assert.equal(withOutdatedFrom(comments, [{ id: "c1", outdated: true }, { id: "c9", outdated: false }]), comments);
});

test("発言は id が同じものを二度足さない", () => {
  const first = { id: "m1", author: "reviewer", body: "hello" };
  const once = addMessage([], first);

  assert.deepEqual(once, [first]);
  // 書いた画面には、応答と SSE の通知の両方で同じ発言が届く。
  assert.deepEqual(addMessage(once, { ...first }), [first]);
  assert.deepEqual(addMessage(once, { id: "m2", author: "agent", body: "hi" }).length, 2);
});

// 会話パネル（R-VIEW、R-AGENT-HAND）。並び・絞り込み・札の中身・新着・帯の数・追従。

/**
 * @param {string} id
 * @param {number} seq
 * @param {{ author?: string, seq: number }[]} replies
 * @param {Record<string, any>} [extra]
 */
function thread(id, seq, replies = [], extra = {}) {
  return {
    id,
    seq,
    group_id: "g1",
    path: "src/a.rs",
    body: "本文",
    resolved: false,
    replies: replies.map((reply, index) => ({
      id: `${id}-r${index}`,
      author: reply.author ?? "reviewer",
      body: "返信",
      seq: reply.seq,
    })),
    ...extra,
  };
}

/**
 * @param {string} id
 * @param {number} seq
 * @param {string} [author]
 */
function message(id, seq, author = "reviewer") {
  return { id, seq, author, body: "発言" };
}

/** @param {any[]} items */
function itemIds(items) {
  return items.map((item) => (item.kind === "thread" ? item.comment.id : item.message.id));
}

/** @param {Partial<import("./conversation-model.js").ReadMarks>} [marks] */
function readMarks(marks = {}) {
  return { loaded: 0, opened: new Map(), messages: 0, ...marks };
}

test("会話パネルは発言とスレッドを最後の書き込みの順に 1 本に並べる", () => {
  const comments = [thread("c1", 1), thread("c2", 3)];
  const messages = [message("m1", 2), message("m2", 4)];

  assert.deepEqual(itemIds(conversationItems(comments, messages)), ["c1", "m1", "c2", "m2"]);
});

test("返信が足されたスレッドは並びの一番後ろへ移る", () => {
  const comments = [thread("c1", 1, [{ seq: 5 }]), thread("c2", 3)];
  const messages = [message("m1", 2), message("m2", 4)];

  assert.deepEqual(itemIds(conversationItems(comments, messages)), ["m1", "c2", "m2", "c1"]);
});

test("すべての絞り込みでは発言とスレッドをすべて出す", () => {
  const items = conversationItems([thread("c1", 1, [], { resolved: true })], [message("m1", 2)]);

  assert.deepEqual(itemIds(filterConversation(items, "all", null)), ["c1", "m1"]);
});

test("未解決の絞り込みでは解決していないスレッドだけを出す", () => {
  const items = conversationItems(
    [thread("c1", 1, [], { resolved: true }), thread("c2", 2)],
    [message("m1", 3)],
  );

  assert.deepEqual(itemIds(filterConversation(items, "unresolved", null)), ["c2"]);
});

test("表示中のファイルの絞り込みではそのグループとパスのスレッドだけを出す", () => {
  const items = conversationItems(
    [
      thread("c1", 1),
      thread("c2", 2, [], { path: "src/b.rs" }),
      thread("c3", 3, [], { group_id: "g2" }),
    ],
    [message("m1", 4)],
  );

  assert.deepEqual(
    itemIds(filterConversation(items, "file", { group_id: "g1", path: "src/a.rs" })),
    ["c1"],
  );
  assert.deepEqual(itemIds(filterConversation(items, "file", null)), []);
});

test("札は本文の 1 行目と返信の数を示す", () => {
  const comment = thread("c1", 1, [{ seq: 2 }, { seq: 3 }], { body: "\n一行目\n二行目" });

  const chip = threadChip(comment, readMarks(), new Map());

  assert.equal(chip.first, "一行目");
  assert.equal(chip.replies, 2);
});

test("解決したスレッドの札は畳み、解決済みの印を付ける。開き直せる", () => {
  const resolved = thread("c1", 1, [], { resolved: true });

  const chip = threadChip(resolved, readMarks(), new Map());

  assert.equal(chip.folded, true);
  assert.equal(chip.resolved, true);
  assert.equal(threadChip(resolved, readMarks(), new Map([["c1", false]])).folded, false);
  assert.equal(threadChip(thread("c2", 2), readMarks(), new Map([["c2", true]])).folded, true);
  assert.equal(threadChip(thread("c3", 3), readMarks(), new Map()).folded, false);
});

test("開いているスレッドのほかで、エージェントの返信が最後に届いた新着のスレッドを知らせる", () => {
  const comments = [
    thread("c1", 1, [{ author: "agent", seq: 9 }]),
    thread("c2", 2, [{ author: "agent", seq: 7 }]),
    thread("c3", 3, [{ author: "agent", seq: 8 }]),
  ];

  assert.equal(otherThreadNews(comments, readMarks({ loaded: 2 }), "c1")?.id, "c3");
  assert.equal(otherThreadNews(comments, readMarks({ loaded: 2, opened: new Map([["c3", 8]]) }), "c1")?.id, "c2");
  assert.equal(otherThreadNews(comments, readMarks({ loaded: 9 }), "c1"), null);
  assert.equal(otherThreadNews(comments, readMarks({ loaded: 2 }), null), null);
});

test("スレッドを最後に開いた後に届いたエージェントの返信だけを新着にする", () => {
  const comment = thread("c1", 1, [{ author: "agent", seq: 4 }, { seq: 6 }]);

  assert.equal(threadUnread(comment, readMarks({ loaded: 2 })), true);
  assert.equal(threadUnread(comment, readMarks({ loaded: 2, opened: new Map([["c1", 5]]) })), false);
  // 人間の返信は新着にしない。
  assert.equal(
    threadUnread(thread("c2", 1, [{ seq: 6 }]), readMarks({ loaded: 2 })),
    false,
  );
  assert.equal(threadChip(comment, readMarks({ loaded: 2 }), new Map()).unread, true);
});

test("ページを読み込んだ時点であった返信は新着にしない", () => {
  const comment = thread("c1", 1, [{ author: "agent", seq: 4 }]);

  assert.equal(threadUnread(comment, readMarks({ loaded: 4 })), false);
});

test("帯の新着の数は新着のスレッドの数と畳んでいる間に届いたエージェントの発言の数の合計", () => {
  const comments = [
    thread("c1", 1, [{ author: "agent", seq: 4 }, { author: "agent", seq: 5 }]),
    thread("c2", 2, [{ author: "agent", seq: 6 }]),
    thread("c3", 3),
  ];
  const messages = [
    message("m1", 7, "agent"),
    message("m2", 8, "agent"),
    message("m3", 9, "reviewer"),
    message("m4", 10, "agent"),
  ];

  const marks = readMarks({ loaded: 3, opened: new Map([["c2", 6]]), messages: 7 });

  assert.equal(unreadCount(comments, messages, marks), 1 + 2);
});

test("並びの一番下を見ているときだけ新しいものについていく", () => {
  assert.equal(followsNewest({ scrollTop: 600, clientHeight: 400, scrollHeight: 1000 }), true);
  assert.equal(followsNewest({ scrollTop: 590, clientHeight: 400, scrollHeight: 1000 }), true);
  assert.equal(followsNewest({ scrollTop: 300, clientHeight: 400, scrollHeight: 1000 }), false);
  // 並びが短く、スクロールしないとき。
  assert.equal(followsNewest({ scrollTop: 0, clientHeight: 400, scrollHeight: 200 }), true);
});
