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
      post({ type: 'described', id: message.id, path: location.pathname + location.search, description: describePage() });
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

  /**
   * @param {CSSStyleDeclaration} computed
   * @param {string} property
   */
  function styleValue(computed, property) {
    const value = computed.getPropertyValue(property);
    const parts = STYLE_PARTS[property];
    return value !== '' || !parts ? value : parts.map((part) => computed.getPropertyValue(part)).join(' ');
  }

  /** @param {number} value */
  const round = (value) => Math.round(value * 100) / 100;

  /**
   * 今のページの要素の記述。位置と大きさはスクロールに依らない文書の座標。開いている shadow root の
   * 中の要素は持ち主の子として並べ、閉じた shadow root は読めないので持ち主までにする。
   */
  function describePage() {
    /** @type {{ parent: number, tag: string, id: string, cls: string, text: string, box: number[], style: number }[]} */
    const elements = [];
    /** @type {Record<string, string>[]} */
    const styles = [];
    /** @type {Map<string, number>} */
    const styleIndex = new Map();
    const left = scrollX;
    const top = scrollY;
    /**
     * @param {Element} element
     * @param {number} parent
     */
    const visit = (element, parent) => {
      if (UNDESCRIBED.has(element.localName)) return;
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
      const index = elements.length;
      elements.push({
        parent,
        tag: element.localName,
        id: element.id,
        cls: element.getAttribute('class') ?? '',
        text,
        box: [round(rect.left + left), round(rect.top + top), round(rect.width), round(rect.height)],
        style: number,
      });
      if (element.shadowRoot) {
        watchRoot(element.shadowRoot);
        for (const child of element.shadowRoot.children) visit(child, index);
      }
      for (const child of element.children) visit(child, index);
    };
    visit(document.documentElement, -1);
    return { width: innerWidth, height: document.documentElement.scrollHeight, styles, elements };
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
  const mutations = new MutationObserver(changed);
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
   * @returns {Promise<Element | null>}
   */
  async function snapshotElement(original, copy) {
    const owner = /** @type {Document} */ (copy.ownerDocument);
    const tag = original.localName;
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
    if (tag === 'source' && original.parentElement?.localName === 'picture') {
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
      await snapshotChildren(original, copy);
    }
    if (original.shadowRoot) {
      const template = owner.createElement('template');
      template.setAttribute('shadowrootmode', 'open');
      for (const child of [...original.shadowRoot.childNodes]) {
        const copied = await snapshotNode(child, owner);
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
   * 1 つの節を写す。要素は状態ごと、それ以外はそのまま。
   * @param {Node} original
   * @param {Document} owner
   * @returns {Promise<Node | null>}
   */
  async function snapshotNode(original, owner) {
    const copy = owner.importNode(original, false);
    if (original instanceof Element) {
      return snapshotElement(original, /** @type {Element} */ (copy));
    }
    return copy;
  }

  /**
   * 子を 1 つずつ写す（浅く写して、子は自分で足す）。
   * @param {Element} original
   * @param {Element} copy
   */
  async function snapshotChildren(original, copy) {
    const owner = /** @type {Document} */ (copy.ownerDocument);
    const children = original instanceof HTMLTemplateElement ? original.content.childNodes : original.childNodes;
    const target = copy instanceof HTMLTemplateElement ? copy.content : copy;
    for (const child of [...children]) {
      const copied = await snapshotNode(child, owner);
      if (copied) target.append(copied);
    }
  }

  async function captureSnapshot() {
    // 写す途中で読み込みを待つ間にページが変わることがあるので、記述は写し始める前の同じ DOM から作る。
    const description = describePage();
    inlined = new Map();
    const owner = document.implementation.createHTMLDocument('');
    const root = /** @type {Element} */ (await snapshotNode(document.documentElement, owner));
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
    return { html: doctype + root.outerHTML, description };
  }
})();
