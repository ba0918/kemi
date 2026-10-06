// @ts-check
// `--live` の差分（live.md の R-PAGE-DIFF）の純粋な処理。比べる相手のスナップショットと動いている
// ページの要素の記述を要素ごとに対応させ、変化の一覧を作る。要素も通信も触らない。
// `--live` のレビューでだけ読み込む（R-VERIFY: ほかのレビューはページ用のファイルを読まない）。

/**
 * ページが書き出す要素の記述（live.md の DL3）。要素は文書の順で、親は自分より前にある。
 * 開いている shadow root の中の要素は、その持ち主の子として並ぶ。
 * @typedef {{
 *   parent: number,
 *   tag: string,
 *   id: string,
 *   cls: string,
 *   text: string,
 *   box: number[],
 *   style: number,
 * }} DescribedElement
 * @typedef {{
 *   width: number,
 *   height: number,
 *   styles: Record<string, string>[],
 *   elements: DescribedElement[],
 * }} Description
 */

/**
 * ページから届く詰めた形。要素は `[親, タグ, id, class, 文字, 左, 上, 幅, 高さ, スタイルの番号]` の並び。
 * 要素ごとに項目名を持たないので、要素の多いページでも小さく送れる。
 * @param {unknown} value
 * @returns {Description | null} 形が違えば null（比べない）
 */
export function unpackDescription(value) {
  const packed = /** @type {{ width: unknown, height: unknown, styles: unknown, elements: unknown }} */ (value);
  if (
    value === null ||
    typeof value !== "object" ||
    typeof packed.width !== "number" ||
    typeof packed.height !== "number" ||
    !Array.isArray(packed.styles) ||
    !Array.isArray(packed.elements)
  ) {
    return null;
  }
  /** @type {DescribedElement[]} */
  const elements = [];
  for (const row of packed.elements) {
    if (!Array.isArray(row) || row.length !== 10) {
      return null;
    }
    const [parent, tag, id, cls, text, left, top, width, height, style] = row;
    elements.push({
      parent: Number(parent),
      tag: String(tag),
      id: String(id),
      cls: String(cls),
      text: String(text),
      box: [Number(left), Number(top), Number(width), Number(height)],
      style: Number(style),
    });
  }
  return { width: packed.width, height: packed.height, styles: packed.styles, elements };
}

/**
 * 変化 1 つ。`before` と `now` はそれぞれの記述の中の要素の番号で、無い側は null。
 * 見た目の変化は `property` の値の前後を、文字の変化は文字の前後を `was` と `is` に持つ。
 * 増えた・消えたは、その要素の中の文字の始まりを持つ。
 * @typedef {{
 *   kind: "visual" | "text" | "added" | "removed" | "shifted",
 *   before: number | null,
 *   now: number | null,
 *   label: string,
 *   property: string,
 *   was: string,
 *   is: string,
 * }} Change
 */

/**
 * 印 1 つ。`index` は記述の中の要素の番号。主な変化・増えた・ずれただけは今のページの側に、
 * 消えたは比べる相手の側に付ける。
 * @typedef {{ index: number, kind: "main" | "added" | "shifted" | "removed" }} Mark
 */

/** 位置と大きさの差のうち、これ以下は同じとみなす（小数の丸めの差を拾わない）。 */
const BOX_TOLERANCE = 0.5;

/** 増えた・消えたに添える文字の長さ。 */
const EXCERPT_LENGTH = 60;

/** 兄弟の並びを最長共通部分列で比べる上限（両側の数の積）。超えたら前から順に組む。 */
const LCS_LIMIT = 4_000_000;

/**
 * 2 つの記述（前と今）を比べ、変化の一覧を返す。主な変化（見た目・文字・増えた・消えた）が先で、
 * ずれただけが後。
 * @param {Description} before
 * @param {Description} now
 * @returns {Change[]}
 */
export function diffDescriptions(before, now) {
  // 署名の表とスタイルの表は両側で共有する（同じ中身なら同じ番号）。
  /** @type {Map<string, number>} */
  const signatureTable = new Map();
  /** @type {Map<string, number>} */
  const styleTable = new Map();
  const left = prepare(before, signatureTable, styleTable);
  const right = prepare(now, signatureTable, styleTable);
  const pairs = pairElements(left, right);

  /** @type {Change[]} */
  const changes = [];
  const styleDiffs = new Map();
  right.elements.forEach((element, index) => {
    const partner = pairs.nowToBefore[index];
    if (partner === -1) {
      if (element.parent === -1 || pairs.nowToBefore[element.parent] !== -1) {
        changes.push(change("added", null, index, right, { is: excerpt(right, index) }));
      }
      return;
    }
    const was = left.elements[partner];
    const key = `${was.style} ${element.style}`;
    if (!styleDiffs.has(key)) {
      styleDiffs.set(key, diffStyles(before.styles[was.style] ?? {}, now.styles[element.style] ?? {}));
    }
    /** @type {{ property: string, was: string, is: string }[]} */
    const visual = styleDiffs.get(key);
    for (const difference of visual) {
      changes.push(change("visual", partner, index, right, difference));
    }
    const textChanged = left.texts[partner] !== right.texts[index];
    if (textChanged) {
      changes.push(change("text", partner, index, right, { was: left.texts[partner], is: right.texts[index] }));
    }
    if (visual.length === 0 && !textChanged && moved(was.box, element.box)) {
      changes.push(change("shifted", partner, index, right, {}));
    }
  });
  left.elements.forEach((element, index) => {
    if (pairs.beforeToNow[index] === -1 && (element.parent === -1 || pairs.beforeToNow[element.parent] !== -1)) {
      changes.push(change("removed", index, null, left, { was: excerpt(left, index) }));
    }
  });
  // 今のページにある主な変化を文書の順に、その後に消えたもの、最後にずれただけ。
  const rank = (/** @type {Change} */ item) => (item.kind === "shifted" ? 2 : item.kind === "removed" ? 1 : 0);
  return changes
    .map((item, order) => ({ item, order }))
    .sort((x, y) => rank(x.item) - rank(y.item) || x.order - y.order)
    .map(({ item }) => item);
}

/**
 * 比べる前の下ごしらえ: 子の並び、空白を詰めた文字、id を鍵にできる要素、中身の署名（見た目を
 * 含むものと含まないもの）。
 * @param {Description} description
 * @param {Map<string, number>} signatureTable
 * @param {Map<string, number>} styleTable
 */
function prepare(description, signatureTable, styleTable) {
  const elements = description.elements;
  /** @type {number[][]} */
  const children = elements.map(() => []);
  elements.forEach((element, index) => {
    if (element.parent >= 0 && element.parent < index) {
      children[element.parent].push(index);
    }
  });
  // 空白だけの差は変化に数えない（改行や字下げの違いで文字の変化が並ばないように）。
  const texts = elements.map((element) => element.text.replace(/\s+/g, " ").trim());
  /** @type {Map<string, number>} */
  const idCounts = new Map();
  for (const element of elements) {
    if (element.id !== "") {
      idCounts.set(element.id, (idCounts.get(element.id) ?? 0) + 1);
    }
  }
  // 同じ id が 2 つ以上あるときは鍵にしない（どれと組むか決められない）。
  const keyed = elements.map((element) => element.id !== "" && idCounts.get(element.id) === 1);
  // スタイルの番号は記述ごとに違うので、中身で両側に共通の番号に直す。
  const styleNumbers = description.styles.map((style) => {
    const key = JSON.stringify(style);
    let number = styleTable.get(key);
    if (number === undefined) {
      number = styleTable.size;
      styleTable.set(key, number);
    }
    return number;
  });
  const looks = elements.map((element) => String(styleNumbers[element.style] ?? -1));
  const signatures = sign(elements, children, texts, looks, signatureTable);
  const plainSignatures = sign(elements, children, texts, null, signatureTable);
  // 同じタグの兄弟の中の何番目かと、その数（手がかりの :nth-of-type）。変化ごとに兄弟を数え直すと、
  // 兄弟が多いページで変化が多いとき、数の 2 乗の時間がかかる。
  const typeOrder = new Array(elements.length).fill(1);
  const typeCount = new Array(elements.length).fill(1);
  for (const siblings of children) {
    /** @type {Map<string, number[]>} */
    const byTag = new Map();
    for (const child of siblings) {
      const tag = elements[child].tag;
      const same = byTag.get(tag) ?? [];
      same.push(child);
      byTag.set(tag, same);
    }
    for (const same of byTag.values()) {
      same.forEach((child, order) => {
        typeOrder[child] = order + 1;
        typeCount[child] = same.length;
      });
    }
  }
  return { elements, children, texts, keyed, signatures, plainSignatures, typeOrder, typeCount };
}

/**
 * 要素ごとに、タグ・id・class・文字・子の署名（`looks` を渡せば見た目も）から決まる番号を付ける。
 * 中身がそっくり同じ部分木は同じ番号になる。子は自分より後ろにあるので、後ろから決める。
 * @param {DescribedElement[]} elements
 * @param {number[][]} children
 * @param {string[]} texts
 * @param {string[] | null} looks 要素ごとの見た目の番号
 * @param {Map<string, number>} signatureTable
 * @returns {number[]}
 */
function sign(elements, children, texts, looks, signatureTable) {
  /** @type {number[]} */
  const signatures = new Array(elements.length).fill(0);
  for (let index = elements.length - 1; index >= 0; index--) {
    const element = elements[index];
    const key = [
      looks === null ? "" : looks[index],
      element.tag,
      element.id,
      element.cls,
      texts[index],
      children[index].map((child) => signatures[child]).join(","),
    ].join("\u0000");
    let number = signatureTable.get(key);
    if (number === undefined) {
      number = signatureTable.size;
      signatureTable.set(key, number);
    }
    signatures[index] = number;
  }
  return signatures;
}

/**
 * 要素を対応させる（live.md の DL2）。
 * - id を鍵にできる要素は、同じ id の要素とだけ組む（親が違ってもよい）。
 * - それ以外は、組んだ親の子の並びの中で、見た目まで中身がそっくり同じ要素を最長共通部分列で組み、
 *   その間に残った要素を、見た目を除いた中身が同じものどうし、さらに残りを同じタグどうし、
 *   並びの順に組む。兄弟の途中に 1 つ入っても、後ろの兄弟は中身が同じなので組まれ、入ったものだけが
 *   残る。見た目だけが違う兄弟（色違いのボタンなど）も見た目で見分ける。親から受け継ぐ色が全体で
 *   変わったときは、見た目を除いた中身で組む。
 * @param {ReturnType<typeof prepare>} left
 * @param {ReturnType<typeof prepare>} right
 */
function pairElements(left, right) {
  const beforeToNow = new Array(left.elements.length).fill(-1);
  const nowToBefore = new Array(right.elements.length).fill(-1);
  /** @type {[number, number][]} */
  const queue = [];
  const join = (/** @type {number} */ a, /** @type {number} */ b) => {
    beforeToNow[a] = b;
    nowToBefore[b] = a;
    queue.push([a, b]);
  };
  if (left.elements.length > 0 && right.elements.length > 0) {
    join(0, 0);
  }
  /** @type {Map<string, number>} */
  const keyedNow = new Map();
  right.elements.forEach((element, index) => {
    if (right.keyed[index] && index !== 0) {
      keyedNow.set(element.id, index);
    }
  });
  left.elements.forEach((element, index) => {
    const partner = left.keyed[index] && index !== 0 ? keyedNow.get(element.id) : undefined;
    if (partner !== undefined && left.elements[index].tag === right.elements[partner].tag) {
      join(index, partner);
    }
  });
  // 似かたは、両側の要素から比べる鍵を引く関数の対で表す（鍵が同じなら似ている）。
  /** @type {Likeness[]} */
  const likenesses = [
    [(x) => left.signatures[x], (y) => right.signatures[y]],
    [(x) => left.plainSignatures[x], (y) => right.plainSignatures[y]],
    [(x) => left.elements[x].tag, (y) => right.elements[y].tag],
  ];
  for (let next = 0; next < queue.length; next++) {
    const [a, b] = queue[next];
    const sideA = left.children[a].filter((child) => !left.keyed[child] && beforeToNow[child] === -1);
    const sideB = right.children[b].filter((child) => !right.keyed[child] && nowToBefore[child] === -1);
    pairInOrder(sideA, sideB, likenesses, join);
  }
  return { beforeToNow, nowToBefore };
}

/**
 * 似かた 1 つ。前と今の要素の番号から、比べる鍵を引く関数の対。
 * @typedef {[(x: number) => number | string, (y: number) => number | string]} Likeness
 */

/**
 * 2 つの兄弟の並びを、最初の似かたの最長共通部分列で組み、その間に残った並びを次の似かたで組む。
 * @param {number[]} sideA
 * @param {number[]} sideB
 * @param {Likeness[]} likenesses 強いものから順に
 * @param {(a: number, b: number) => void} join
 */
function pairInOrder(sideA, sideB, likenesses, join) {
  if (likenesses.length === 0 || sideA.length === 0 || sideB.length === 0) {
    return;
  }
  const [like, ...weaker] = likenesses;
  let fromA = 0;
  let fromB = 0;
  for (const [x, y] of [...commonSubsequence(sideA, sideB, like), [sideA.length, sideB.length]]) {
    pairInOrder(sideA.slice(fromA, x), sideB.slice(fromB, y), weaker, join);
    if (x < sideA.length) {
      join(sideA[x], sideB[y]);
    }
    fromA = x + 1;
    fromB = y + 1;
  }
}

/**
 * 2 つの並びの最長共通部分列を、組になった位置の対で返す。前と後ろの一致を先に除き、
 * 中だけを表で解く。表が大きすぎるときは、前から順に貪欲に組む。
 * @param {number[]} a
 * @param {number[]} b
 * @param {Likeness} likeness
 * @returns {[number, number][]}
 */
function commonSubsequence(a, b, [keyOfA, keyOfB]) {
  const equal = (/** @type {number} */ x, /** @type {number} */ y) => keyOfA(x) === keyOfB(y);
  /** @type {[number, number][]} */
  const head = [];
  let start = 0;
  while (start < a.length && start < b.length && equal(a[start], b[start])) {
    head.push([start, start]);
    start++;
  }
  /** @type {[number, number][]} */
  const tail = [];
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && equal(a[endA - 1], b[endB - 1])) {
    endA--;
    endB--;
    tail.unshift([endA, endB]);
  }
  const n = endA - start;
  const m = endB - start;
  /** @type {[number, number][]} */
  const middle = [];
  if (n > 0 && m > 0 && n * m <= LCS_LIMIT) {
    const width = m + 1;
    const table = new Uint32Array((n + 1) * width);
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        table[i * width + j] = equal(a[start + i], b[start + j])
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (equal(a[start + i], b[start + j])) {
        middle.push([start + i, start + j]);
        i++;
        j++;
      } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
        i++;
      } else {
        j++;
      }
    }
  } else if (n > 0 && m > 0) {
    // 前から順に、まだ使っていない位置のうち最初に鍵が合うものと組む。鍵ごとに b の位置を並べておき、
    // 使った位置より前を捨てながら引く（全体を見渡すと、何も合わないときに数の 2 乗の時間がかかる）。
    /** @type {Map<number | string, number[]>} */
    const positions = new Map();
    for (let k = 0; k < m; k++) {
      const key = keyOfB(b[start + k]);
      const list = positions.get(key);
      if (list === undefined) {
        positions.set(key, [k]);
      } else {
        list.push(k);
      }
    }
    /** @type {Map<number | string, number>} 鍵ごとに、次に見る positions の中の場所 */
    const cursors = new Map();
    let j = 0;
    for (let i = 0; i < n && j < m; i++) {
      const key = keyOfA(a[start + i]);
      const list = positions.get(key);
      if (list === undefined) {
        continue;
      }
      let cursor = cursors.get(key) ?? 0;
      while (cursor < list.length && list[cursor] < j) {
        cursor++;
      }
      cursors.set(key, cursor);
      if (cursor < list.length) {
        const k = list[cursor];
        middle.push([start + i, start + k]);
        j = k + 1;
      }
    }
  }
  return [...head, ...middle, ...tail];
}

/**
 * 見た目のスタイルの違い。値はページの計算済みの値のまま。
 * @param {Record<string, string>} was
 * @param {Record<string, string>} is
 * @returns {{ property: string, was: string, is: string }[]}
 */
function diffStyles(was, is) {
  const properties = [...new Set([...Object.keys(is), ...Object.keys(was)])];
  return properties
    .filter((property) => (was[property] ?? "") !== (is[property] ?? ""))
    .map((property) => ({ property, was: was[property] ?? "", is: is[property] ?? "" }));
}

/**
 * @param {number[]} was
 * @param {number[]} is
 */
function moved(was, is) {
  return [0, 1, 2, 3].some((at) => Math.abs((was[at] ?? 0) - (is[at] ?? 0)) > BOX_TOLERANCE);
}

/**
 * @param {Change["kind"]} kind
 * @param {number | null} before
 * @param {number | null} now
 * @param {ReturnType<typeof prepare>} side 手がかりを読む側
 * @param {{ property?: string, was?: string, is?: string }} values
 * @returns {Change}
 */
function change(kind, before, now, side, values) {
  const index = /** @type {number} */ (now ?? before);
  return {
    kind,
    before,
    now,
    label: label(side, index),
    property: values.property ?? "",
    was: values.was ?? "",
    is: values.is ?? "",
  };
}

/**
 * 一覧に出す要素の手がかり。id を持つ祖先か 3 段までの、CSS のセレクタに似た並び。
 * @param {ReturnType<typeof prepare>} side
 * @param {number} index
 */
function label(side, index) {
  /** @type {string[]} */
  const parts = [];
  let current = index;
  while (current !== -1 && parts.length < 3) {
    const element = side.elements[current];
    if (element.tag === "html" || element.tag === "body") {
      break;
    }
    parts.unshift(segment(side, current));
    if (element.id !== "") {
      break;
    }
    current = element.parent;
  }
  return parts.length > 0 ? parts.join(" > ") : side.elements[index].tag;
}

/**
 * @param {ReturnType<typeof prepare>} side
 * @param {number} index
 */
function segment(side, index) {
  const element = side.elements[index];
  if (element.id !== "") {
    return `${element.tag}#${element.id}`;
  }
  const first = element.cls.trim().split(/\s+/)[0];
  let text = first ? `${element.tag}.${first}` : element.tag;
  if (side.typeCount[index] > 1) {
    text += `:nth-of-type(${side.typeOrder[index]})`;
  }
  return text;
}

/**
 * 要素とその中の文字の始まり。
 * @param {ReturnType<typeof prepare>} side
 * @param {number} index
 */
function excerpt(side, index) {
  let text = "";
  /** @type {number[]} */
  const stack = [index];
  while (stack.length > 0 && text.length < EXCERPT_LENGTH) {
    const current = /** @type {number} */ (stack.pop());
    if (side.texts[current] !== "") {
      text += (text === "" ? "" : " ") + side.texts[current];
    }
    stack.push(...[...side.children[current]].reverse());
  }
  return text.length > EXCERPT_LENGTH ? `${text.slice(0, EXCERPT_LENGTH - 1)}…` : text;
}

/** 1 つの要素に変化が重なるときに残す印の順（前が強い）。 */
const MARK_ORDER = ["main", "added", "removed", "shifted"];

/**
 * 変化の一覧から、要素ごとに 1 つの印を作る。1 つの要素に見た目と文字の両方の変化があっても
 * 印は 1 つ。
 * @param {Change[]} changes
 * @returns {{ now: Mark[], before: Mark[] }}
 */
export function marksOf(changes) {
  /** @type {Map<number, Mark["kind"]>} */
  const now = new Map();
  /** @type {Map<number, Mark["kind"]>} */
  const before = new Map();
  for (const change of changes) {
    const kind = change.kind === "visual" || change.kind === "text" ? "main" : change.kind;
    const side = kind === "removed" ? before : now;
    const index = kind === "removed" ? change.before : change.now;
    if (index === null) {
      continue;
    }
    const held = side.get(index);
    if (held === undefined || MARK_ORDER.indexOf(kind) < MARK_ORDER.indexOf(held)) {
      side.set(index, kind);
    }
  }
  const list = (/** @type {Map<number, Mark["kind"]>} */ marks) => [...marks].map(([index, kind]) => ({ index, kind }));
  return { now: list(now), before: list(before) };
}

/**
 * 2 つの変化の一覧が同じか（どちらも無しも同じ）。比べ直した結果が前と同じなら、一覧も印も作り直さないために使う。
 * @param {Change[] | null} left
 * @param {Change[] | null} right
 */
export function sameChanges(left, right) {
  if (left === null || right === null) {
    return left === right;
  }
  return (
    left.length === right.length &&
    left.every((change, index) => {
      const other = right[index];
      return (
        change.kind === other.kind &&
        change.before === other.before &&
        change.now === other.now &&
        change.label === other.label &&
        change.property === other.property &&
        change.was === other.was &&
        change.is === other.is
      );
    })
  );
}
