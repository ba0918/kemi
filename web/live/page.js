// 中継したページに差し込むスクリプト（live.md の R-PAGE-PROXY）。開発中のページの中で動き、
// レビュー画面とは postMessage だけで話す。kemi の API には触れない（トークンを持たない）。
// モジュールにしないのは、ページの CSP の script-src に足すのがこのファイル 1 つだけで
// 済むようにするため。
(() => {
  const script = /** @type {HTMLScriptElement | null} */ (document.currentScript);
  const review = script?.dataset.kemiReview;
  if (!review || window.parent === window) return;

  /** @param {Record<string, unknown>} message */
  const post = (message) => window.parent.postMessage({ kemi: 'live', ...message }, review);

  post({
    type: 'page',
    path: location.pathname + location.search,
    reachable: script?.dataset.kemiReachable !== 'false',
    rewrote: (script?.dataset.kemiRewrote ?? '').split(' ').filter(Boolean),
  });
})();
