// @ts-check
// 下の層（要素を作るところ）から上の層（状態を変えてサーバを呼ぶところ）を呼ぶための
// 入れ物。中身は起動時に bindActions で 1 回だけ入れる。
//
// why not: 実装が 1 つしかない注入点は普通は作らない。それでもここに置くのは、代わりが
// 「モジュールが互いに import し合うのを認める」か「描画の関数すべてに処理の束を引数で
// 渡し回す」しか無いため。前者はどのモジュールも単独では読めなくなり、分ける意味が消える。
// 後者でも束を組む場所は結局 1 か所に要る。結線を 1 か所（app.js）に集めるほうを採った。

/**
 * グループ単位を切り替えたうえで、指定の行へ移る指定。
 * @typedef {{
 *   find: (entries: import("./state.js").Entry[]) => number,
 *   side: string,
 *   line: number | null,
 *   missing: () => void,
 * }} UnitJump
 */

/**
 * @typedef {{
 *   addComment: (payload: any) => void,
 *   closeEditor: () => void,
 *   confirmDeleteComment: (comment: any) => void,
 *   editComment: (payload: any) => void,
 *   openCommentEditor: (comment: any) => void,
 *   selectIndex: (index: number, options?: { scrollTop?: boolean }) => void,
 *   setCommentOpen: (id: string, open: boolean) => void,
 *   switchUnit: (unit: string, jump: UnitJump | null) => void,
 * }} Actions
 */

/**
 * 束ねた処理。views と features はこれを呼ぶだけで、実体を import しない。
 * @type {Actions}
 */
export const actions = /** @type {Actions} */ ({});

/**
 * 起動時に 1 回だけ、処理の実体を入れる。
 * @param {Actions} bound
 */
export function bindActions(bound) {
  Object.assign(actions, bound);
}
