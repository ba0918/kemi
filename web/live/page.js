// 中継したページに差し込むスクリプト（live.md の R-PAGE-PROXY）。開発中のページの中で動き、
// レビュー画面とは postMessage だけで話す。kemi の API には触れない（トークンを持たない）。
// モジュールにしないのは、ページの CSP の script-src に足すのがこのファイル 1 つだけで
// 済むようにするため。
(() => {
  const script = /** @type {HTMLScriptElement | null} */ (document.currentScript);
  // 手元の HTML ファイルは、配った範囲のファイルが保存されたら読み込み直す（R-LIVE の例外）。
  if (script?.dataset.kemiWatch === 'true') {
    const events = new EventSource('/__kemi/events');
    events.addEventListener('reload', () => location.reload());
  }

  const review = script?.dataset.kemiReview;
  if (!review || window.parent === window) return;

  /** @param {Record<string, unknown>} message */
  const post = (message) => window.parent.postMessage({ kemi: 'live', ...message }, review);

  window.addEventListener('message', async (event) => {
    if (event.source !== window.parent || event.origin !== review) return;
    const message = event.data;
    if (!message || message.kemi !== 'live') return;
    if (message.type === 'describe') {
      watchChanges();
      const { description, elements } = describePage();
      described = elements;
      post({ type: 'described', id: message.id, path: location.pathname + location.search, description });
      return;
    }
    if (message.type === 'marks') {
      drawMarks(Array.isArray(message.marks) ? message.marks : []);
      return;
    }
    if (message.type !== 'capture') return;
    try {
      const { html, description } = await captureSnapshot();
      post({ type: 'captured', id: message.id, path: location.pathname + location.search, html, description });
    } catch (error) {
      post({ type: 'captured', id: message.id, error: String(error) });
    }
  });

  // 重ねて透かすときに比べる相手のスクロールをそろえるため、スクロールの位置と中身の高さを
  // 知らせる（live-compare.md の DC2）。
  let scrollQueued = false;
  const reportScroll = () => {
    if (scrollQueued) return;
    scrollQueued = true;
    requestAnimationFrame(() => {
      scrollQueued = false;
      post({ type: 'scroll', x: scrollX, y: scrollY, height: document.documentElement.scrollHeight });
    });
  };
  window.addEventListener('scroll', reportScroll, { passive: true });
  window.addEventListener('resize', reportScroll);

  // 読み込み終えてから知らせる。知らせを受けたレビュー画面は、すぐ開始時の写しを頼むことがある。
  const announce = () => post({
    type: 'page',
    path: location.pathname + location.search,
    reachable: script?.dataset.kemiReachable !== 'false',
    rewrote: (script?.dataset.kemiRewrote ?? '').split(' ').filter(Boolean),
  });
  const ready = () => {
    announce();
    reportScroll();
  };
  if (document.readyState === 'complete') ready();
  else window.addEventListener('load', ready, { once: true });

  // 読み込み直さずに URL を変えるページ（history.pushState など）でも、移ったことを知らせる。
  // 知らせないと、レビュー画面は前のページのまま比べる相手を選ぶ。
  // `#` から後ろだけの変化や、同じ URL への replaceState は知らせない（比べる相手は変わらない）。
  let announced = location.pathname + location.search;
  const announceIfMoved = () => {
    const now = location.pathname + location.search;
    if (now === announced) return;
    announced = now;
    announce();
  };
  const pushState = history.pushState;
  const replaceState = history.replaceState;
  history.pushState = function (...args) {
    pushState.apply(this, args);
    announceIfMoved();
  };
  history.replaceState = function (...args) {
    replaceState.apply(this, args);
    announceIfMoved();
  };
  window.addEventListener('popstate', announceIfMoved);

  // ---- 要素の記述（R-PAGE-DIFF、形は DL3） ----
  // レビュー画面はこのページの DOM もスナップショットの枠の DOM も読めないので、要素ごとに比べる
  // ための記述をここで作って渡す。比べるのはレビュー画面（live-diff.js）。

  /** 記述に入れる見た目のスタイル。 */
  const DESCRIBED_STYLES = [
    'color', 'background-color', 'background-image', 'font-size', 'font-weight', 'font-style', 'font-family',
    'text-decoration-line', 'border-radius', 'border-color', 'border-width', 'border-style', 'box-shadow',
    'opacity', 'visibility', 'display',
  ];

  /** 角や辺ごとの値に分かれるスタイル。まとめた値を返さないブラウザでは、分けた値を並べる。 */
  /** @type {Record<string, string[]>} */
  const STYLE_PARTS = {
    'border-radius': ['border-top-left-radius', 'border-top-right-radius', 'border-bottom-right-radius', 'border-bottom-left-radius'],
    'border-color': ['border-top-color', 'border-right-color', 'border-bottom-color', 'border-left-color'],
    'border-width': ['border-top-width', 'border-right-width', 'border-bottom-width', 'border-left-width'],
    'border-style': ['border-top-style', 'border-right-style', 'border-bottom-style', 'border-left-style'],
  };

  /** 描かれない要素。記述に入れない。 */
  const UNDESCRIBED = new Set(['head', 'script', 'style', 'link', 'meta', 'noscript', 'template', 'title', 'base']);

  /** 最後に記述を渡した要素。印の番号はこの並びを指す。 */
  /** @type {Element[]} */
  let described = [];

  /**
   * @param {CSSStyleDeclaration} computed
   * @param {string} property
   */
  function styleValue(computed, property) {
    const value = computed.getPropertyValue(property);
    const parts = STYLE_PARTS[property];
    return value !== '' || !parts ? value : parts.map((part) => computed.getPropertyValue(part)).join(' ');
  }

  /**
   * <picture> の <source>。描かれず、スナップショットの写しにも入れないので、記述にも入れない。
   * @param {Element} element
   */
  const isPictureSource = (element) => element.localName === 'source' && element.parentElement?.localName === 'picture';

  /** @param {number} value */
  const round = (value) => Math.round(value * 100) / 100;

  /**
   * 今のページの要素の記述。位置と大きさは、ページも中の箱もどこもスクロールしていないとしたときの文書の
   * 座標（スクロールしただけで変化に入らないように）。画面に固定した要素とその中は画面の座標。上に張り付く
   * 要素は、張り付いたぶんがスクロールで変わり測り分けられないので、自分を親の左上に置き、中の要素は
   * そこからの位置にする（張り付く要素自身の親の中での動きは変化に出ない）。
   * 開いている shadow root の中の要素は持ち主の子として並べ、閉じた shadow root は読めないので持ち主までにする。
   */
  function describePage() {
    // 要素ごとに項目名を持たない詰めた形（live-diff.js の unpackDescription が読む）。
    /** @type {(string | number)[][]} */
    const elements = [];
    /** @type {Element[]} */
    const originals = [];
    /** @type {Record<string, string>[]} */
    const styles = [];
    /** @type {Map<string, number>} */
    const styleIndex = new Map();
    const scroller = document.scrollingElement ?? document.documentElement;
    /** 要素ごとの、その中に置かれた要素の画面の座標に足す量（スクロールのぶん）。 */
    /** @type {Map<Element, number[]>} */
    const insideShift = new Map();
    /**
     * @param {Element} element
     * @param {number} parent
     * @param {number[]} shift 親の中に置かれた要素の、画面の座標に足す量
     */
    const visit = (element, parent, shift) => {
      if (UNDESCRIBED.has(element.localName) || element === marksHost || isPictureSource(element)) return;
      const computed = getComputedStyle(element);
      /** @type {Record<string, string>} */
      const style = {};
      for (const property of DESCRIBED_STYLES) style[property] = styleValue(computed, property);
      const key = JSON.stringify(style);
      let number = styleIndex.get(key);
      if (number === undefined) {
        number = styles.length;
        styleIndex.set(key, number);
        styles.push(style);
      }
      let text = '';
      for (const node of element.childNodes) {
        if (node.nodeType === Node.TEXT_NODE) text += /** @type {Text} */ (node).data;
      }
      const rect = element.getBoundingClientRect();
      const position = computed.position;
      // 固定した要素は画面に、絶対配置の要素はその基準の要素の中に置かれる（間の箱のスクロールで動かない）。
      const containing = position === 'absolute' && element instanceof HTMLElement ? element.offsetParent : null;
      let own = position === 'fixed' ? [0, 0] : (containing && insideShift.get(containing)) || shift;
      let left = rect.left + own[0];
      let top = rect.top + own[1];
      if (position === 'sticky' && parent !== -1) {
        left = Number(elements[parent][5]);
        top = Number(elements[parent][6]);
        own = [left - rect.left, top - rect.top];
      }
      const inside = element === scroller ? own : [own[0] + element.scrollLeft, own[1] + element.scrollTop];
      insideShift.set(element, inside);
      const index = elements.length;
      elements.push([
        parent,
        element.localName,
        element.id,
        element.getAttribute('class') ?? '',
        text,
        round(left),
        round(top),
        round(rect.width),
        round(rect.height),
        number,
      ]);
      originals.push(element);
      if (element.shadowRoot) {
        watchRoot(element.shadowRoot);
        for (const child of element.shadowRoot.children) visit(child, index, inside);
      }
      for (const child of element.children) visit(child, index, inside);
    };
    visit(document.documentElement, -1, [scrollX, scrollY]);
    return { description: { width: innerWidth, height: document.documentElement.scrollHeight, styles, elements }, elements: originals };
  }

  // ---- 印（R-PAGE-VIEW の変わったところに必ず印） ----
  // 変わった要素に枠を重ねる層。ページの見た目を変えないよう、画面の左上に大きさ 0 で固定し、閉じた
  // shadow root の中に描く。ページや中の箱がスクロールしたら置き直す（固定した要素や箱の中の要素から
  // 印が離れないように）。スクロールで箱の見えている範囲の外に出た要素の印は、その範囲で切る。この層は記述にもスナップショットにも入れず、見張りも反応させない
  // （入れると、印そのものが変化として出て、付け直しが繰り返す）。
  // 見た目は <style> や style 属性の文字列ではなく、スクリプトから要素のスタイルの値を 1 つずつ入れる。
  // ページの CSP の style-src がインラインのスタイルを止めていても効くようにするため。

  /** 印の枠の色と線。主な変化は赤、増えたは緑、ずれただけは控えめに薄い紫の破線。 */
  /** @type {Record<string, string>} */
  const MARK_BORDERS = {
    main: '2px solid rgb(214, 69, 69)',
    added: '2px solid rgb(26, 154, 74)',
    shifted: '1px dashed rgb(185, 166, 217)',
  };

  /**
   * @param {HTMLElement} element
   * @param {Record<string, string>} styles
   */
  function setStyles(element, styles) {
    for (const [property, value] of Object.entries(styles)) element.style.setProperty(property, value);
  }

  /** @type {HTMLElement | null} */
  let marksHost = null;
  /** @type {ShadowRoot | null} */
  let marksRoot = null;
  /** 描いている印と、その要素と、要素を切って見せる祖先（中身をはみ出させない箱）。 */
  /** @type {{ box: HTMLElement, element: Element, clippers: Element[] }[]} */
  let shownMarks = [];

  /**
   * 最後に渡した記述の要素に印を描く。前の印は消す。
   * @param {unknown[]} marks
   */
  function drawMarks(marks) {
    shownMarks = [];
    if (marks.length === 0) {
      marksHost?.remove();
      return;
    }
    if (!marksHost || !marksRoot) {
      marksHost = document.createElement('div');
      setStyles(marksHost, {
        position: 'fixed', left: '0', top: '0', width: '0', height: '0', margin: '0', padding: '0', border: '0',
        overflow: 'visible', 'pointer-events': 'none', 'z-index': '2147483647',
      });
      marksRoot = marksHost.attachShadow({ mode: 'closed' });
    }
    if (!marksHost.isConnected) document.documentElement.append(marksHost);
    /** @type {Map<Element, boolean>} */
    const clips = new Map();
    for (const mark of marks) {
      const { index, kind } = /** @type {{ index: unknown, kind: unknown }} */ (mark ?? {});
      const element = typeof index === 'number' ? described[index] : undefined;
      const border = Object.hasOwn(MARK_BORDERS, String(kind)) ? MARK_BORDERS[String(kind)] : undefined;
      if (!element?.isConnected || border === undefined) continue;
      const box = document.createElement('div');
      setStyles(box, { position: 'absolute', 'box-sizing': 'border-box', 'pointer-events': 'none', border });
      shownMarks.push({ box, element, clippers: clippersOf(element, clips) });
    }
    marksRoot.replaceChildren(...shownMarks.map((mark) => mark.box));
    placeMarks();
  }

  /**
   * 要素を切って見せる祖先。中身をはみ出させない箱で、画面に固定した要素より外のものは除く（固定した
   * 要素は外の箱のスクロールで動かず、切られもしない）。ページ全体のスクロールは画面が切るので除く。
   * @param {Element} element
   * @param {Map<Element, boolean>} clips 祖先ごとに、中身をはみ出させないか（1 回の描き直しの中で覚える）
   */
  function clippersOf(element, clips) {
    /** @type {Element[]} */
    const clippers = [];
    /** @type {Element | null} */
    let current = element;
    while (current && current !== document.body && current !== document.documentElement) {
      if (current !== element) {
        let clipping = clips.get(current);
        if (clipping === undefined) {
          const computed = getComputedStyle(current);
          clipping = computed.overflowX !== 'visible' || computed.overflowY !== 'visible';
          clips.set(current, clipping);
        }
        if (clipping) clippers.push(current);
      }
      if (getComputedStyle(current).position === 'fixed') break;
      /** @type {Node | null} */
      const root = current.parentNode;
      current = root instanceof ShadowRoot ? root.host : current.parentElement;
    }
    return clippers;
  }

  /** 印を今の要素の位置に置き直す。先にすべて測ってから書き込む（測るたびに配置を計算し直させない）。 */
  function placeMarks() {
    if (!marksHost || shownMarks.length === 0) return;
    const origin = marksHost.getBoundingClientRect();
    const placed = shownMarks.map(({ element, clippers }) => {
      const rect = element.getBoundingClientRect();
      let visible = { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
      for (const clipper of clippers) {
        const bound = clipper.getBoundingClientRect();
        visible = {
          left: Math.max(visible.left, bound.left),
          top: Math.max(visible.top, bound.top),
          right: Math.min(visible.right, bound.right),
          bottom: Math.min(visible.bottom, bound.bottom),
        };
      }
      return { rect, visible };
    });
    shownMarks.forEach(({ box }, at) => {
      const { rect, visible } = placed[at];
      const hidden = visible.right <= visible.left || visible.bottom <= visible.top;
      setStyles(box, {
        display: hidden ? 'none' : 'block',
        left: `${rect.left - origin.left}px`,
        top: `${rect.top - origin.top}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
        'clip-path': `inset(${visible.top - rect.top}px ${rect.right - visible.right}px ${rect.bottom - visible.bottom}px ${visible.left - rect.left}px)`,
      });
    });
  }

  let placeQueued = false;
  const placeMarksSoon = () => {
    if (placeQueued || shownMarks.length === 0) return;
    placeQueued = true;
    requestAnimationFrame(() => {
      placeQueued = false;
      placeMarks();
    });
  };
  // スクロールの知らせは浮き上がらないので、捕まえる側で受ける。shadow root の中のものは watchRoot で受ける。
  document.addEventListener('scroll', placeMarksSoon, { capture: true, passive: true });
  window.addEventListener('resize', placeMarksSoon);

  /**
   * 印の層を足し外ししただけの変化か。
   * @param {MutationRecord} record
   */
  function marksOnly(record) {
    if (record.target === marksHost) return true;
    const nodes = [...record.addedNodes, ...record.removedNodes];
    return record.type === 'childList' && nodes.length > 0 && nodes.every((node) => node === marksHost);
  }

  // ---- ページの変化の見張り ----
  // レビュー画面が記述を一度頼んでから見張る。変わったら間を置いてまとめて「変わった」とだけ知らせ、
  // 記述を作り直すかはレビュー画面が決める。動き続けるページで知らせが続かないよう、知らせの間を空ける。

  /** 最後の変化から知らせるまで待つ時間（ミリ秒）。 */
  const CHANGE_QUIET = 150;
  /** 知らせと知らせの間の最小の時間（ミリ秒）。 */
  const CHANGE_INTERVAL = 500;
  const OBSERVED = { subtree: true, childList: true, attributes: true, characterData: true };
  let watching = false;
  /** @type {ReturnType<typeof setTimeout> | null} */
  let changeTimer = null;
  let lastNotice = -Infinity;
  const changed = () => {
    if (changeTimer !== null) return;
    const wait = Math.max(CHANGE_QUIET, lastNotice + CHANGE_INTERVAL - performance.now());
    changeTimer = setTimeout(() => {
      changeTimer = null;
      lastNotice = performance.now();
      post({ type: 'changed' });
    }, wait);
  };
  const mutations = new MutationObserver((records) => {
    if (!records.every(marksOnly)) changed();
  });
  const resizes = new ResizeObserver(changed);
  /** @type {WeakSet<ShadowRoot>} */
  const watchedRoots = new WeakSet();

  function watchChanges() {
    if (watching) return;
    watching = true;
    mutations.observe(document, OBSERVED);
    resizes.observe(document.documentElement);
    // 差し替えた CSS や画像は、要素が変わった後に読み込まれて見た目が変わる。
    document.addEventListener('load', changed, true);
    document.fonts?.addEventListener('loadingdone', changed);
  }

  /** @param {ShadowRoot} root */
  function watchRoot(root) {
    if (!watching || watchedRoots.has(root)) return;
    watchedRoots.add(root);
    mutations.observe(root, OBSERVED);
    root.addEventListener('scroll', placeMarksSoon, { capture: true, passive: true });
  }

  /**
   * スナップショットと一緒に預ける記述を、gzip で縮めて base64 の文字列にする。要素の多いページでは
   * 記述が HTML より大きくなり、縮めないと要求の本文の上限を超えて取れなくなる。
   * @param {unknown} description
   * @returns {Promise<string>}
   */
  async function packForUpload(description) {
    const zipped = new Blob([JSON.stringify(description)]).stream().pipeThrough(new CompressionStream('gzip'));
    const blob = await new Response(zipped).blob();
    const url = await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(blob);
    });
    return url.slice(url.indexOf(',') + 1);
  }

  // ---- スナップショット（R-PAGE-SNAPSHOT、形は DL3） ----
  // 今の DOM を写し、スクリプトを除いた 1 つの HTML にする。shadow DOM は宣言的な
  // shadow root に、canvas は画像に、入力欄の値は属性に移す。読み込んだ CSS と画像は
  // data: の URL に埋め込む。スナップショットを出す枠はレビュー画面の中の別のオリジンで、
  // 中継の cookie を持たず、中継のポートから何も読めないため。

  /** 1 回の写しの中で読んだ URL。写すたびに作り直し、前の写しの後で変わった画像や CSS を読み直す。 */
  /** @type {Map<string, Promise<string>>} */
  let inlined = new Map();

  /**
   * URL の中身を data: の URL にする。読めなければ元の URL のまま。
   * @param {string} url
   * @returns {Promise<string>}
   */
  function dataUrl(url) {
    if (url.startsWith('data:')) return Promise.resolve(url);
    let pending = inlined.get(url);
    if (!pending) {
      pending = fetch(url, { credentials: 'same-origin' })
        .then((response) => (response.ok ? response.blob() : Promise.reject(new Error(String(response.status)))))
        .then((blob) => new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(String(reader.result));
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(blob);
        }))
        .then((value) => String(value))
        .catch(() => url);
      inlined.set(url, pending);
    }
    return pending;
  }

  /**
   * CSS の url() を、基準の URL で解いて data: の URL に埋め込む。
   * @param {string} css
   * @param {string} base
   * @returns {Promise<string>}
   */
  async function inlineCss(css, base) {
    const pattern = /url\(\s*(['"]?)([^'")]+)\1\s*\)/g;
    const found = [...css.matchAll(pattern)];
    const replacements = await Promise.all(found.map(async (match) => {
      const target = match[2].trim();
      if (target.startsWith('data:') || target.startsWith('#')) return match[0];
      const absolute = new URL(target, base).href;
      return `url("${await dataUrl(absolute)}")`;
    }));
    let index = 0;
    return css.replace(pattern, () => replacements[index++]);
  }

  /**
   * スタイルシートの規則を、url() を埋め込んだ CSS の文字列にする。@import は読める
   * ものを展開し、読めないものは元の URL を指したまま先頭に残す（@import は先頭にしか
   * 置けない）。シート自身の規則が読めない（別のオリジンの）ときは null。
   * @param {CSSStyleSheet} sheet
   * @param {string} base
   * @returns {Promise<string | null>}
   */
  async function sheetCss(sheet, base) {
    const parts = await sheetParts(sheet, base);
    return parts && [...parts.imports, parts.body].join('\n');
  }

  /**
   * sheetCss の中身。展開できなかった CSS の import の行と、それ以外の規則を分けて返す。
   * 展開したシートの中に残った import の行も、外側の先頭へ寄せるため。
   * @param {CSSStyleSheet} sheet
   * @param {string} base
   * @returns {Promise<{ imports: string[], body: string } | null>}
   */
  async function sheetParts(sheet, base) {
    /** @type {CSSRule[]} */
    let rules;
    try {
      rules = [...sheet.cssRules];
    } catch {
      return null;
    }
    // 規則ごとの url() の取り寄せは中継との往復なので、順に待たず一度に始め、元の順に並べる。
    const parts = await Promise.all(rules.map((rule) => ruleParts(rule, base)));
    return {
      imports: parts.flatMap((part) => part.imports),
      body: parts.map((part) => part.body).filter((text) => text !== null).join('\n'),
    };
  }

  /**
   * sheetParts の規則 1 つぶん。import の行と、本文に置く CSS（無ければ null）。
   * @param {CSSRule} rule
   * @param {string} base
   * @returns {Promise<{ imports: string[], body: string | null }>}
   */
  async function ruleParts(rule, base) {
    if (!(rule instanceof CSSImportRule)) {
      return { imports: [], body: await inlineCss(rule.cssText, base) };
    }
    const href = new URL(rule.href, base).href;
    const media = rule.media.mediaText;
    const imported = rule.styleSheet ? await sheetParts(rule.styleSheet, rule.styleSheet.href ?? href) : null;
    if (imported === null) {
      return { imports: [`@import url("${href}")${media ? ` ${media}` : ''};`], body: null };
    }
    return {
      imports: imported.imports,
      body: media ? `@media ${media} {\n${imported.body}\n}` : imported.body,
    };
  }

  /**
   * @param {Document} owner
   * @param {string} css
   */
  function styleElement(owner, css) {
    const style = owner.createElement('style');
    style.textContent = css;
    return style;
  }

  /**
   * 元の要素の今の状態を、写しの要素に移す。写しは置き換わることがあるので、最後に
   * 写しとして使う要素を返す（取り除くときは null）。
   * @param {Element} original
   * @param {Element} copy
   * @param {Map<Element, number>} indices 記述の要素の番号（写しの要素に目印として付ける）
   * @returns {Promise<Element | null>}
   */
  async function snapshotElement(original, copy, indices) {
    const owner = /** @type {Document} */ (copy.ownerDocument);
    const tag = original.localName;
    if (original === marksHost) return null;
    if (tag === 'script' || tag === 'noscript' || (tag === 'meta' && /refresh/i.test(original.getAttribute('http-equiv') ?? ''))) {
      return null;
    }
    // ページの referrer の指定を写すと、スナップショットの中の外部の画像へトークンの URL を
    // 送ることがある（レビュー画面の no-referrer より優先されるため）。
    if (tag === 'meta' && (original.getAttribute('name') ?? '').toLowerCase() === 'referrer') {
      return null;
    }
    if (tag === 'link') {
      const rel = (original.getAttribute('rel') ?? '').toLowerCase().split(/\s+/);
      const sheet = /** @type {HTMLLinkElement} */ (original).sheet;
      if (rel.includes('stylesheet') && sheet) {
        const css = await sheetCss(sheet, sheet.href ?? location.href);
        if (css !== null) return styleElement(owner, css);
        // 規則を読めない別のオリジンの CSS は、元の URL を指したまま残す。中継を通らないので、
        // スナップショットの枠からも読める。
        if (sheet.href) copy.setAttribute('href', sheet.href);
      }
      if (rel.some((value) => ['preload', 'modulepreload', 'prefetch'].includes(value))) return null;
    }
    if (tag === 'style') {
      // 中身の文字列ではなく今の規則から写す。insertRule などで足した規則は文字列に出ない。
      const sheet = /** @type {HTMLStyleElement} */ (original).sheet;
      const css = sheet ? await sheetCss(sheet, document.baseURI) : null;
      copy.textContent = css ?? await inlineCss(original.textContent ?? '', document.baseURI);
      return copy;
    }
    // <picture> の <source> は外す。選ばれた画像は <img> の currentSrc として埋め込む。
    if (isPictureSource(original)) {
      return null;
    }
    if (tag === 'canvas') {
      const canvas = /** @type {HTMLCanvasElement} */ (original);
      const image = owner.createElement('img');
      for (const attribute of [...original.attributes]) image.setAttribute(attribute.name, attribute.value);
      image.setAttribute('width', String(canvas.width));
      image.setAttribute('height', String(canvas.height));
      try {
        image.setAttribute('src', canvas.toDataURL());
      } catch {
        // 別のオリジンの画像を描いた canvas は読み出せない。大きさだけを保つ。
      }
      return image;
    }
    if (tag === 'img') {
      const image = /** @type {HTMLImageElement} */ (original);
      const source = image.currentSrc || image.src;
      if (source) copy.setAttribute('src', await dataUrl(source));
      copy.removeAttribute('srcset');
      copy.removeAttribute('sizes');
      copy.removeAttribute('loading');
    }
    if (tag === 'video') {
      const poster = /** @type {HTMLVideoElement} */ (original).poster;
      if (poster) copy.setAttribute('poster', await dataUrl(poster));
    }
    if (tag === 'image' && original.namespaceURI === 'http://www.w3.org/2000/svg') {
      for (const namespace of [null, 'http://www.w3.org/1999/xlink']) {
        const value = original.getAttributeNS(namespace, 'href');
        if (value) copy.setAttributeNS(namespace, 'href', await dataUrl(new URL(value, document.baseURI).href));
      }
    }
    if (tag === 'input') {
      const input = /** @type {HTMLInputElement} */ (original);
      if (input.type === 'image' && input.src) {
        copy.setAttribute('src', await dataUrl(input.src));
      }
      if (input.type === 'checkbox' || input.type === 'radio') {
        if (input.checked) copy.setAttribute('checked', '');
        else copy.removeAttribute('checked');
      } else if (input.type !== 'file' && input.type !== 'password') {
        copy.setAttribute('value', input.value);
      }
    }
    if (tag === 'textarea') {
      copy.textContent = /** @type {HTMLTextAreaElement} */ (original).value;
    }
    if (tag === 'option') {
      if (/** @type {HTMLOptionElement} */ (original).selected) copy.setAttribute('selected', '');
      else copy.removeAttribute('selected');
    }
    // referrerpolicy は要素ごとに文書の no-referrer より優先されるので、写しには残さない。
    // 残すと、埋め込めなかった外部の画像などへトークンの URL を送ることがある。
    for (const attribute of [...copy.attributes]) {
      if (/^on/i.test(attribute.name) || /^\s*javascript:/i.test(attribute.value) || attribute.name.toLowerCase() === 'referrerpolicy') {
        copy.removeAttribute(attribute.name);
      } else if (attribute.name.toLowerCase() === 'style' && /url\(/i.test(attribute.value)) {
        copy.setAttribute(attribute.name, await inlineCss(attribute.value, document.baseURI));
      }
    }
    if (tag !== 'textarea') {
      await snapshotChildren(original, copy, indices);
    }
    if (original.shadowRoot) {
      const template = owner.createElement('template');
      template.setAttribute('shadowrootmode', 'open');
      for (const child of [...original.shadowRoot.childNodes]) {
        const copied = await snapshotNode(child, owner, indices);
        if (copied) template.content.append(copied);
      }
      for (const sheet of original.shadowRoot.adoptedStyleSheets) {
        template.content.append(styleElement(owner, (await sheetCss(sheet, document.baseURI)) ?? ''));
      }
      copy.prepend(template);
    }
    return copy;
  }

  /**
   * 1 つの節を写す。要素は状態ごと、それ以外はそのまま。記述した要素の写しには、その番号の目印を付ける。
   * @param {Node} original
   * @param {Document} owner
   * @param {Map<Element, number>} indices 記述の要素の番号
   * @returns {Promise<Node | null>}
   */
  async function snapshotNode(original, owner, indices) {
    const copy = owner.importNode(original, false);
    if (!(original instanceof Element)) {
      return copy;
    }
    const copied = await snapshotElement(original, /** @type {Element} */ (copy), indices);
    if (copied) {
      // ページが同じ名前の属性を持っていても、目印と読み違えないよう外す。
      copied.removeAttribute(ELEMENT_STAMP);
      const index = indices.get(original);
      if (index !== undefined) copied.setAttribute(ELEMENT_STAMP, String(index));
    }
    return copied;
  }

  /**
   * 子を 1 つずつ写す（浅く写して、子は自分で足す）。
   * @param {Element} original
   * @param {Element} copy
   * @param {Map<Element, number>} indices 記述の要素の番号
   */
  async function snapshotChildren(original, copy, indices) {
    const owner = /** @type {Document} */ (copy.ownerDocument);
    const children = original instanceof HTMLTemplateElement ? original.content.childNodes : original.childNodes;
    const target = copy instanceof HTMLTemplateElement ? copy.content : copy;
    for (const child of [...children]) {
      const copied = await snapshotNode(child, owner, indices);
      if (copied) target.append(copied);
    }
  }

  async function captureSnapshot() {
    // 写す途中で読み込みを待つ間にページが変わることがあるので、記述は写し始める前の DOM から作り、
    // 写しの要素とは並びではなく元の要素で結ぶ（写す途中で足された要素は記述に無く、目印も付かない）。
    const page = describePage();
    const description = packForUpload(page.description);
    const indices = new Map(page.elements.map((element, index) => [element, index]));
    inlined = new Map();
    const owner = document.implementation.createHTMLDocument('');
    const root = /** @type {Element} */ (await snapshotNode(document.documentElement, owner, indices));
    const head = root.querySelector('head');
    if (head) {
      // 埋め込めなかった相対の参照は、中継のオリジンに解く（読めなくても形は崩れない）。
      const base = owner.createElement('base');
      base.setAttribute('href', location.href);
      head.prepend(base);
      for (const sheet of document.adoptedStyleSheets) {
        head.append(styleElement(owner, (await sheetCss(sheet, document.baseURI)) ?? ''));
      }
    }
    const doctype = document.doctype ? `<!doctype ${document.doctype.name}>` : '';
    const map = elementMap(doctype + root.outerHTML);
    eachElement(root, (element) => element.removeAttribute(ELEMENT_STAMP));
    if (head && map !== null) {
      const meta = owner.createElement('meta');
      meta.setAttribute('name', ELEMENT_MAP);
      meta.setAttribute('content', map);
      head.prepend(meta);
    }
    return { html: doctype + root.outerHTML, description: await description };
  }

  // ---- 写しの要素と記述の番号の対応 ----
  // レビュー画面は消えた要素の印を付けるため、スナップショットの HTML を読み直して記述の要素を探す
  // （views/live.js の markRemovedInSnapshot）。読み直すと要素の並びが DOM と変わることがある（スクリプトが
  // 組んだ表に tbody が足される、p の中の div が外に出るなど）ので、ここで同じように読み直し、記述の番号ごとに
  // 読み直した文書での要素の位置を求めて HTML に添える。要素ごとの目印は HTML を大きくし 2 MB の上限に
  // 掛かるので、写す間だけ付けて外し、続いて並ぶ番号をまとめた対応だけを 1 つの <meta> に入れる。

  /** 写す間だけ、記述した要素の写しに付ける目印（記述の番号）。 */
  const ELEMENT_STAMP = 'data-kemi-element';
  /** 対応を入れる <meta> の name。レビュー画面（views/live.js）が読んで外す。 */
  const ELEMENT_MAP = 'kemi-elements';

  /**
   * 要素とその中の要素を、文書の順に（<template> は中身を）たどる。位置の数え方は views/live.js と同じ。
   * @param {Element} element
   * @param {(element: Element) => void} visit
   */
  function eachElement(element, visit) {
    visit(element);
    const children = element instanceof HTMLTemplateElement ? element.content.children : element.children;
    for (const child of [...children]) eachElement(child, visit);
  }

  /**
   * 目印を付けた HTML を読み直し、記述の番号と読み直した文書での要素の位置の対応を返す。続いて並ぶ
   * ものは「位置,番号,個数」にまとめ、空白で区切る。読み直せないページでは null を返し、スナップショットは
   * 対応を持たずに撮る（消えた要素の印は付かない）。
   * @param {string} html
   * @returns {string | null}
   */
  function elementMap(html) {
    /** @type {Document} */
    let parsed;
    try {
      parsed = new DOMParser().parseFromString(html, 'text/html');
    } catch {
      // ページの CSP の require-trusted-types-for は文字列を読む DOMParser を止める。中継はその CSP を
      // 緩めない（R-PAGE-PROXY）ので、対応を諦めてスナップショットのほうを撮る。
      return null;
    }
    /** @type {number[][]} */
    const runs = [];
    let position = 0;
    eachElement(parsed.documentElement, (element) => {
      const stamp = element.getAttribute(ELEMENT_STAMP);
      if (stamp !== null) {
        const index = Number(stamp);
        const last = runs.at(-1);
        if (last && last[0] + last[2] === position && last[1] + last[2] === index) last[2] += 1;
        else runs.push([position, index, 1]);
      }
      position += 1;
    });
    return runs.map((run) => run.join(',')).join(' ');
  }
})();
