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
