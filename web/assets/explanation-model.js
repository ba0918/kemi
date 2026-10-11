// @ts-check

/** @param {string} base @param {string|null} code */
export function freshness(base, code) {
  return code === null ? 'pending' : base === code ? 'matching' : 'different';
}

/** @param {any} reference @param {any} review @param {string|null} token @param {any} [data] */
export function resolveCodeReference(reference, review, token, data) {
  const unavailable = (/** @type {string} */ reason) => ({entry:null,reason});
  if (!review || !token || review.diff_display_version !== token) return unavailable('This unit has not been loaded for this display version.');
  if ((review.unit ?? null) !== reference.unit) return unavailable('This group unit has not been loaded.');
  const entries = review.groups.flatMap((/** @type {any} */ group) => group.files
    .filter((/** @type {any} */ file) => group.id === reference.group_id && file.path === reference.path)
    .map((/** @type {any} */ file) => ({group,file})));
  if (entries.length !== 1) return unavailable(entries.length ? 'The target is ambiguous.' : 'The target is absent from this list.');
  const entry = entries[0];
  const present=entry.file[`${reference.side}_present`] ?? (reference.side==='old' ? entry.file.status!=='add' : entry.file.status!=='delete');
  if (!present) return unavailable('The requested side is absent.');
  if (reference.start_line !== null) {
    const total = data?.[`${reference.side}_total`] ?? entry.file[`${reference.side}_lines`];
    if (!Number.isInteger(total) || data?.binary || data?.content_skipped) return unavailable('The line count is unknown or this content cannot be loaded.');
    if (reference.end_line > total) return unavailable('The requested lines are no longer present.');
  }
  return {entry,reason:null};
}

/** @param {unknown} value @returns {string} */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(/** @type {Record<string,unknown>} */(value)[key])}`).join(',')}}`;
  return JSON.stringify(value);
}

/** @param {any[]} comments @param {number} revision @param {any} block */
export function commentsForBlock(comments, revision, block) {
  return comments.filter(comment => comment.target_type === 'explanation' && comment.explanation.block_id === block.id &&
    (comment.explanation.revision === revision || canonical(comment.explanation.quote.block) === canonical(block)));
}

/**
 * 目次の節ごとのコメント数。読んでいる版でブロックのそばに出すコメントと同じ集合を数える。
 * @param {any[]} comments @param {{revision:number,sections:{id:string,blocks:any[]}[]}} version
 * @returns {Map<string,number>}
 */
export function sectionCommentCounts(comments, version) {
  return new Map(version.sections.map(section => [section.id,
    section.blocks.reduce((sum, block) => sum + commentsForBlock(comments, version.revision, block).length, 0)]));
}

/** サーバと同じ上限。選んだ文字を UTF-8 で数える。 */
const MAX_SELECTION_BYTES = 65_536;

/** @param {string} text */
export function selectionTooLong(text) {
  return new TextEncoder().encode(text).length > MAX_SELECTION_BYTES;
}

/**
 * 表示上の文字列の [start, end) を、何番目の一致かで表す。一致は開始位置の早い順に数え、重なる一致も数える。
 * @param {string} text @param {number} start @param {number} end
 * @returns {{text:string,occurrence:number}}
 */
export function selectionAt(text, start, end) {
  const needle = text.slice(start, end);
  let occurrence = 0;
  for (let at = text.indexOf(needle); at !== -1 && at < start; at = text.indexOf(needle, at + 1)) occurrence++;
  return {text:needle, occurrence};
}

/**
 * 保存した選択を、表示上の文字列の occurrence 番目の一致として戻す。無ければ null。
 * @param {string} text @param {{text:string,occurrence:number}} selection
 * @returns {{start:number,end:number}|null}
 */
export function locateSelection(text, selection) {
  if (!selection.text) return null;
  let at = -1;
  for (let seen = 0; seen <= selection.occurrence; seen++) {
    at = text.indexOf(selection.text, at + 1);
    if (at === -1) return null;
  }
  return {start:at, end:at + selection.text.length};
}

/**
 * DOM の Node に相当する最小の形。node のテストで同じ関数を使うため、document に依存させない。
 * @typedef {{nodeType:number,nodeValue:string|null,childNodes:ArrayLike<ReadingNode>}} ReadingNode
 */

/**
 * 根の中の文字を文書順に辿る。skip が真を返す要素（画面が後から入れた札・入力欄）の中には入らない。
 * visit が true を返したら止める。
 * @param {ReadingNode} root @param {(node:ReadingNode)=>boolean} skip
 * @param {(node:ReadingNode, before:number)=>boolean|void} visit 文字のノードと、その前までの文字数
 */
function walkReading(root, skip, visit) {
  let count = 0;
  /** @param {ReadingNode} node @returns {boolean} */
  const step = node => {
    if (node.nodeType === 3) {
      if (visit(node, count)) return true;
      count += (node.nodeValue ?? '').length;
      return false;
    }
    if (node !== root && skip(node)) return false;
    for (const child of Array.from(node.childNodes)) if (step(child)) return true;
    return false;
  };
  step(root);
  return count;
}

/** @param {ReadingNode} root @param {(node:ReadingNode)=>boolean} skip */
export function readingText(root, skip) {
  let text = '';
  walkReading(root, skip, node => { text += node.nodeValue ?? ''; });
  return text;
}

/**
 * Range の端（container, offset）を表示上の文字列の位置にする。根の中に無い・飛ばす要素の中なら null。
 * @param {ReadingNode} root @param {ReadingNode} container @param {number} offset @param {(node:ReadingNode)=>boolean} skip
 * @returns {number|null}
 */
export function readingOffset(root, container, offset, skip) {
  let count = 0;
  /** @type {number|null} */ let found = null;
  /** @param {ReadingNode} node @returns {boolean} 見つけたら true */
  const step = node => {
    if (node.nodeType === 3 && node !== container) { count += (node.nodeValue ?? '').length; return false; }
    if (node !== root && skip(node)) return false;
    if (node === container) {
      if (node.nodeType === 3) { found = count + offset; return true; }
      // 要素の端は offset 番目の子の前。
      const children = Array.from(node.childNodes);
      for (let index = 0; index < children.length && index < offset; index++) step(children[index]);
      found = count;
      return true;
    }
    for (const child of Array.from(node.childNodes)) if (step(child)) return true;
    return false;
  };
  step(root);
  return found;
}

/**
 * 表示上の文字列の位置を、文字のノードと位置に戻す。境目では start は次のノードの先頭、end は前のノードの末尾を選ぶ。
 * @param {ReadingNode} root @param {number} index @param {(node:ReadingNode)=>boolean} skip @param {'start'|'end'} bias
 * @returns {{node:ReadingNode,offset:number}|null}
 */
export function readingPoint(root, index, skip, bias) {
  /** @type {{node:ReadingNode,offset:number}|null} */ let found = null;
  walkReading(root, skip, (node, before) => {
    const length = (node.nodeValue ?? '').length;
    if (bias === 'start' ? index < before + length : index <= before + length) {
      if (index >= before) found = {node, offset:index - before};
      return true;
    }
  });
  if (!found && bias === 'start') {
    // 末尾の位置は最後の文字のノードの末尾。
    /** @type {{node:ReadingNode,offset:number}|null} */ let last = null;
    const total = walkReading(root, skip, (node, before) => { last = {node, offset:index - before}; });
    if (index === total && last) found = last;
  }
  return found;
}
