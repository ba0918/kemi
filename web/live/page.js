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
    if (message.type === 'place') {
      post({ type: 'placed', id: message.id, ...resolvePlace(String(message.kind), Array.isArray(message.points) ? message.points : []) });
      return;
    }
    if (message.type === 'places') {
      drawPlaceSets(Array.isArray(message.sets) ? message.sets : []);
      return;
    }
    if (message.type === 'scroll-by') {
      scrollBy(finite(message.x), finite(message.y));
      return;
    }
    if (message.type === 'image') {
      try {
        const image = await placesImage(
          message.rect ?? null,
          Array.isArray(message.places) ? message.places : [],
          { page: String(message.page), width: finite(message.width), height: finite(message.height) },
        );
        post({ type: 'imaged', id: message.id, ...image });
      } catch (error) {
        post({ type: 'imaged', id: message.id, error: String(error) });
      }
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
  /** 印の層（変わったところの枠）。 */
  /** @type {HTMLElement | null} */
  let marksLayer = null;
  /** コメントの場所の層（描き込みと番号）。文書の座標で描き、ページのスクロールの分だけ戻す。 */
  /** @type {HTMLElement | null} */
  let placesLayer = null;

  /**
   * 印と場所を描く層を作り、文書に置く。ページの見た目を変えないよう、画面の左上に大きさ 0 で固定し、閉じた
   * shadow root の中に描く。
   * @returns {{ marks: HTMLElement, places: HTMLElement }}
   */
  function layers() {
    if (!marksHost || !marksLayer || !placesLayer) {
      marksHost = document.createElement('div');
      setStyles(marksHost, {
        position: 'fixed', left: '0', top: '0', width: '0', height: '0', margin: '0', padding: '0', border: '0',
        overflow: 'visible', 'pointer-events': 'none', 'z-index': '2147483647',
      });
      const root = marksHost.attachShadow({ mode: 'closed' });
      marksLayer = document.createElement('div');
      placesLayer = document.createElement('div');
      for (const layer of [marksLayer, placesLayer]) {
        setStyles(layer, { position: 'absolute', left: '0', top: '0', width: '0', height: '0', overflow: 'visible', 'pointer-events': 'none' });
      }
      root.append(marksLayer, placesLayer);
    }
    if (!marksHost.isConnected) document.documentElement.append(marksHost);
    return { marks: marksLayer, places: placesLayer };
  }

  /** どちらの層も空なら、層を文書から外す。 */
  function releaseLayers() {
    if (marksLayer?.childElementCount === 0 && placesLayer?.childElementCount === 0) marksHost?.remove();
  }
  /** 描いている印と、その要素と枠の線と、要素を切って見せる祖先（中身をはみ出させない箱）。 */
  /** @type {{ box: HTMLElement, element: Element, border: string, clippers: Element[] }[]} */
  let shownMarks = [];

  /**
   * 最後に渡した記述の要素に印を描く。前の印は消す。同じ要素に同じ印なら、印の要素は作り直さずに置き直す
   * （動き続けるページで比べ直すたびに、同じ印を作り直さないように）。
   * @param {unknown[]} marks
   */
  function drawMarks(marks) {
    const previous = shownMarks;
    shownMarks = [];
    if (marks.length === 0) {
      marksLayer?.replaceChildren();
      releaseLayers();
      return;
    }
    const { marks: layer } = layers();
    /** @type {Map<Element, boolean>} */
    const clips = new Map();
    /** @type {{ element: Element, border: string }[]} */
    const wanted = [];
    for (const mark of marks) {
      const { index, kind } = /** @type {{ index: unknown, kind: unknown }} */ (mark ?? {});
      const element = typeof index === 'number' ? described[index] : undefined;
      const border = Object.hasOwn(MARK_BORDERS, String(kind)) ? MARK_BORDERS[String(kind)] : undefined;
      if (!element?.isConnected || border === undefined) continue;
      wanted.push({ element, border });
    }
    const unchanged =
      wanted.length === previous.length &&
      wanted.every(({ element, border }, at) => element === previous[at].element && border === previous[at].border);
    shownMarks = wanted.map(({ element, border }, at) => {
      const box = unchanged ? previous[at].box : document.createElement('div');
      if (!unchanged) setStyles(box, { position: 'absolute', 'box-sizing': 'border-box', 'pointer-events': 'none', border });
      return { box, element, border, clippers: clippersOf(element, clips) };
    });
    if (!unchanged) layer.replaceChildren(...shownMarks.map((mark) => mark.box));
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
    placePlaces();
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
    if (placeQueued || (shownMarks.length === 0 && (placesLayer?.childElementCount ?? 0) === 0)) return;
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

  // ---- コメントの場所（R-PAGE-COMMENT） ----
  // 押す・描く操作はレビュー画面が枠の上に重ねた層で受け（ページのスクリプトに横取りされないように）、画面の座標を
  // 送ってくる。ここではそれを文書の座標にし、指している要素を決める。場所の描き込みと番号も印と同じ層に描くので、
  // 記述・スナップショット・見張りには入らない。

  /** ペンで囲んだ範囲と重なる要素として渡す数の上限。 */
  const ENCLOSED_LIMIT = 5;
  /** 要素の文字として渡す長さの上限。 */
  const TEXT_LIMIT = 200;
  /**
   * ペンで囲んだ範囲と重なる要素に入れない要素（ページ全体を覆い、囲んだ要素を押し出す）。押した・指した位置の要素や、
   * 範囲を含む一番内側の要素としては選ぶ（どの要素にも掛からない地では、これが場所の要素になる）。
   */
  const PAGE_ROOTS = new Set(['html', 'body']);

  /**
   * 画面の座標のその点にある要素。開いている shadow root の中まで下りる。
   * @param {number} x
   * @param {number} y
   * @returns {Element | null}
   */
  function elementAt(x, y) {
    let element = document.elementFromPoint(x, y);
    while (element?.shadowRoot) {
      const inner = element.shadowRoot.elementFromPoint(x, y);
      if (!inner || inner === element) break;
      element = inner;
    }
    return element;
  }

  /**
   * 要素をその root（文書か shadow root）の中で 1 つに決めるセレクタ。id が root の中で 1 つならそれを根にし、
   * 無ければ兄弟の中の同じ名前の何番目かでたどる。shadow root の中の要素は、持ち主のセレクタと ` >>> ` でつなぐ
   * （文書の querySelector は shadow root の中に届かないため）。
   * @param {Element} element
   * @returns {string}
   */
  function selectorOf(element) {
    const root = /** @type {Document | ShadowRoot} */ (element.getRootNode());
    /** @type {string[]} */
    const parts = [];
    /** @type {Element | null} */
    let current = element;
    while (current) {
      if (current.id && root.querySelectorAll(`#${CSS.escape(current.id)}`).length === 1) {
        parts.unshift(`#${CSS.escape(current.id)}`);
        break;
      }
      if (current === document.documentElement) {
        parts.unshift('html');
        break;
      }
      /** @type {Element | null} */
      const parent = current.parentElement;
      const siblings = parent ? [...parent.children] : [...root.children];
      const name = current.localName;
      const same = siblings.filter((sibling) => sibling.localName === name);
      parts.unshift(same.length > 1 ? `${CSS.escape(name)}:nth-of-type(${same.indexOf(current) + 1})` : CSS.escape(name));
      current = parent;
    }
    const selector = parts.join(' > ');
    return root instanceof ShadowRoot ? `${selectorOf(root.host)} >>> ${selector}` : selector;
  }

  /**
   * エージェントに渡す要素の情報。位置と大きさは文書の座標（ページのスクロールに依存しない）。
   * @param {Element} element
   */
  function placeElement(element) {
    const rect = element.getBoundingClientRect();
    const text = (element instanceof HTMLElement ? element.innerText : element.textContent) ?? '';
    return {
      selector: selectorOf(element),
      text: text.replace(/\s+/g, ' ').trim().slice(0, TEXT_LIMIT),
      rect: { x: round(rect.left + scrollX), y: round(rect.top + scrollY), w: round(rect.width), h: round(rect.height) },
    };
  }

  /**
   * 文書の要素を、開いている shadow root の中も含めてたどる。kemi の層は除く。
   * @param {(element: Element) => void} visit
   */
  function eachPageElement(visit) {
    /** @param {Element} element */
    const walk = (element) => {
      if (element === marksHost || UNDESCRIBED.has(element.localName)) return;
      visit(element);
      if (element.shadowRoot) for (const child of element.shadowRoot.children) walk(child);
      for (const child of element.children) walk(child);
    };
    walk(document.documentElement);
  }

  /**
   * ペンで囲んだ範囲（線の外接矩形）と重なる要素を、重なる面積の大きい順に上限まで。画面の座標で比べる。範囲を丸ごと
   * 含む要素（ページを包む入れ物など）と、文書の根（html と body）は、重なる面積が範囲いっぱいになって囲んだ要素を
   * 押し出すので除く。除くと何も残らない（1 つの要素の内側だけか、地だけを囲んだ）ときは、範囲を含む一番内側の要素
   * 1 つにする。
   * @param {{ x: number, y: number }[]} points 画面の座標
   */
  function enclosedElements(points) {
    const left = Math.min(...points.map((point) => point.x));
    const right = Math.max(...points.map((point) => point.x));
    const top = Math.min(...points.map((point) => point.y));
    const bottom = Math.max(...points.map((point) => point.y));
    /** @type {{ element: Element, area: number }[]} */
    const found = [];
    /** @type {{ element: Element | null, area: number }} */
    const innermost = { element: null, area: Infinity };
    eachPageElement((element) => {
      const rect = element.getBoundingClientRect();
      if (rect.left <= left && rect.top <= top && rect.right >= right && rect.bottom >= bottom) {
        // 文書の順にたどるので、面積が同じなら後に来る（内側の）要素を選ぶ。
        const area = rect.width * rect.height;
        if (area <= innermost.area) Object.assign(innermost, { element, area });
        return;
      }
      if (PAGE_ROOTS.has(element.localName)) return;
      const width = Math.min(right, rect.right) - Math.max(left, rect.left);
      const height = Math.min(bottom, rect.bottom) - Math.max(top, rect.top);
      if (width > 0 && height > 0) found.push({ element, area: width * height });
    });
    // 文書の根の箱より外（中身の短いページの下の地）でも、地は文書の根のものとして描かれる。
    if (found.length === 0) return [placeElement(innermost.element ?? document.documentElement)];
    found.sort((a, b) => b.area - a.area);
    return found.slice(0, ENCLOSED_LIMIT).map(({ element }) => placeElement(element));
  }

  /**
   * 置いた場所を決める。点は画面の座標で届き、文書の座標にして返す。要素は 1 つ、矢印は先端の要素、ペンは
   * 囲んだ範囲と重なる要素。要素の場所の点は空にする（R-SUBMIT の `points`）。押した点は `at` に入れて返す。
   * 文書の根（地）を選んだときは、その箱ではなくこの点が画像の範囲と描き込みの位置になる。レビュー画面は
   * これを画像の頼みにだけ載せ、保存する場所には入れない。
   * @param {string} kind
   * @param {unknown[]} requested
   */
  function resolvePlace(kind, requested) {
    const points = requested.map((point) => {
      const { x, y } = /** @type {{ x: unknown, y: unknown }} */ (point ?? {});
      return { x: finite(x), y: finite(y) };
    });
    if (points.length === 0) return { error: 'no point' };
    const toDocument = (/** @type {{ x: number, y: number }} */ point) => ({ x: round(point.x + scrollX), y: round(point.y + scrollY) });
    if (kind === 'pen') {
      return { kind, points: points.map(toDocument), elements: enclosedElements(points) };
    }
    const at = kind === 'arrow' ? points[points.length - 1] : points[0];
    const element = elementAt(at.x, at.y);
    const elements = element ? [placeElement(element)] : [];
    if (kind === 'arrow') return { kind, points: points.map(toDocument), elements };
    if (elements.length === 0) return { error: 'no element there' };
    return { kind: 'element', points: [], elements, at: toDocument(at) };
  }

  /** 場所の描き込みの見た目。書いている途中と、目立たせる保存したものは濃く、ほかの保存したものは控えめに。 */
  /** @type {Record<string, { width: number, opacity: string, numbers: boolean }>} */
  const PLACE_LOOKS = {
    draft: { width: 2, opacity: '1', numbers: true },
    focus: { width: 2, opacity: '1', numbers: true },
    saved: { width: 1, opacity: '0.55', numbers: true },
  };
  const SVG = 'http://www.w3.org/2000/svg';

  /**
   * 場所の組を描く。前に描いたものは消す。座標は文書の座標で、層ごとページのスクロールの分だけ戻す。
   * @param {unknown[]} sets 各組は `{ places, look }`
   */
  function drawPlaceSets(sets) {
    const drawn = [];
    for (const set of sets) {
      const { places, look } = /** @type {{ places: unknown, look: unknown }} */ (set ?? {});
      const style = PLACE_LOOKS[String(look)] ?? PLACE_LOOKS.saved;
      drawn.push(...placeShapes(readPlaces(Array.isArray(places) ? places : []), style));
    }
    if (drawn.length === 0) {
      placesLayer?.replaceChildren();
      releaseLayers();
      return;
    }
    layers().places.replaceChildren(...drawn);
    placePlaces();
  }

  /**
   * 1 組の場所の描き込みの要素。
   * @param {ImagePlace[]} places
   * @param {{ width: number, opacity: string, numbers: boolean }} style
   * @returns {Element[]}
   */
  function placeShapes(places, style) {
    /** @type {Element[]} */
    const drawn = [];
    const svg = document.createElementNS(SVG, 'svg');
    svg.setAttribute('width', '1');
    svg.setAttribute('height', '1');
    svg.setAttribute('overflow', 'visible');
    svg.setAttribute('opacity', style.opacity);
    setStyles(/** @type {any} */ (svg), { position: 'absolute', left: '0', top: '0', overflow: 'visible' });
    for (const place of places) {
      if (place.kind === 'element') {
        // 文書の根の箱は文書全体を囲むので描かない。地を選んだ場所は番号だけで示す。
        for (const { rect } of place.elements.filter(({ root }) => !root)) {
          const box = document.createElementNS(SVG, 'rect');
          for (const [name, value] of Object.entries({ x: rect.x - 2, y: rect.y - 2, width: rect.w + 4, height: rect.h + 4 })) box.setAttribute(name, String(value));
          box.setAttribute('fill', 'none');
          box.setAttribute('stroke', PLACE_COLOR);
          box.setAttribute('stroke-width', String(style.width));
          svg.append(box);
        }
      } else if (place.points.length > 0) {
        const line = document.createElementNS(SVG, 'polyline');
        line.setAttribute('points', place.points.map((point) => `${point.x},${point.y}`).join(' '));
        line.setAttribute('fill', 'none');
        line.setAttribute('stroke', PLACE_COLOR);
        line.setAttribute('stroke-width', String(style.width));
        line.setAttribute('stroke-linejoin', 'round');
        line.setAttribute('stroke-linecap', 'round');
        svg.append(line);
        const from = place.points.at(-2);
        const to = place.points.at(-1);
        if (place.kind === 'arrow' && from && to) {
          const angle = Math.atan2(to.y - from.y, to.x - from.x);
          const head = document.createElementNS(SVG, 'polygon');
          const corner = (/** @type {number} */ turn) => `${to.x - 12 * Math.cos(angle + turn)},${to.y - 12 * Math.sin(angle + turn)}`;
          head.setAttribute('points', `${to.x},${to.y} ${corner(-0.45)} ${corner(0.45)}`);
          head.setAttribute('fill', PLACE_COLOR);
          svg.append(head);
        }
      }
    }
    drawn.push(svg);
    if (style.numbers) {
      for (const place of places) {
        const anchor = placeAnchor(place);
        if (!anchor) continue;
        const badge = document.createElement('div');
        badge.textContent = String(place.n);
        setStyles(badge, {
          position: 'absolute', left: `${Math.max(0, anchor.x - 10)}px`, top: `${Math.max(0, anchor.y - 10)}px`, width: '20px', height: '20px',
          'border-radius': '50%', background: PLACE_COLOR, color: 'rgb(255, 255, 255)', font: 'bold 11px/20px sans-serif',
          'text-align': 'center', opacity: style.opacity, 'box-sizing': 'border-box', margin: '0', padding: '0',
        });
        drawn.push(badge);
      }
    }
    return drawn;
  }

  /** 場所の層をページのスクロールに合わせて置き直す（描き込みは文書の座標で描いてある）。 */
  function placePlaces() {
    if (!placesLayer || placesLayer.childElementCount === 0) return;
    setStyles(placesLayer, { transform: `translate(${-scrollX}px, ${-scrollY}px)` });
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
    // 表示幅を切り替えると、文書の大きさが変わらなくても（<html> の min-width より狭い幅どうしなど）記述の
    // 表示幅が変わる。レビュー画面は今の表示幅の記述を待っている。
    window.addEventListener('resize', changed);
    watchCssom();
  }

  /**
   * CSSOM だけで見た目を変える操作（構築したスタイルシートの replaceSync、既存のシートへの insertRule、規則の
   * スタイルの書き換えなど）は DOM を変えず、ほかの見張りに掛からない。その操作を包んで、変わったと知らせる。
   * 要素の style 属性のスタイル（parentRule が無い）は DOM の変化として見張っているので、ここでは知らせない
   * （印の層のスタイルを入れるたびに知らせて、付け直しが繰り返さないように）。
   * 規則のスタイルに `rule.style.color = …` と代入するもの、adoptedStyleSheets の配列に push するものは包めない。
   */
  function watchCssom() {
    /**
     * @param {any} owner
     * @param {string} name
     * @param {(target: any) => boolean} [applies]
     */
    const wrapMethod = (owner, name, applies) => {
      const original = owner?.[name];
      if (typeof original !== 'function') return;
      owner[name] = function (/** @type {unknown[]} */ ...args) {
        const result = original.apply(this, args);
        if (!applies || applies(this)) {
          changed();
          // replace は中身を読み終えてから効く。
          if (result instanceof Promise) result.then(changed, () => {});
        }
        return result;
      };
    };
    /**
     * @param {any} owner
     * @param {string} name
     * @param {(target: any) => boolean} [applies]
     */
    const wrapSetter = (owner, name, applies) => {
      const descriptor = owner && Object.getOwnPropertyDescriptor(owner, name);
      const set = descriptor?.set;
      if (!descriptor || !set) return;
      Object.defineProperty(owner, name, {
        ...descriptor,
        set(/** @type {unknown} */ value) {
          set.call(this, value);
          if (!applies || applies(this)) changed();
        },
      });
    };
    /** @param {CSSStyleDeclaration} declaration */
    const inRule = (declaration) => declaration.parentRule !== null;
    for (const name of ['insertRule', 'deleteRule', 'addRule', 'removeRule', 'replace', 'replaceSync']) {
      wrapMethod(globalThis.CSSStyleSheet?.prototype, name);
    }
    for (const name of ['insertRule', 'deleteRule']) wrapMethod(globalThis.CSSGroupingRule?.prototype, name);
    wrapSetter(globalThis.StyleSheet?.prototype, 'disabled');
    wrapSetter(Document.prototype, 'adoptedStyleSheets');
    wrapSetter(ShadowRoot.prototype, 'adoptedStyleSheets');
    for (const name of ['setProperty', 'removeProperty']) wrapMethod(CSSStyleDeclaration.prototype, name, inRule);
    wrapSetter(CSSStyleDeclaration.prototype, 'cssText', inRule);
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
    return blobBase64(await new Response(zipped).blob());
  }

  /**
   * @param {Blob} blob
   * @returns {Promise<string>}
   */
  async function blobBase64(blob) {
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
    const { owner, root, head } = await copyDocument(indices);
    if (head) {
      // 埋め込めなかった相対の参照は、中継のオリジンに解く（読めなくても形は崩れない）。
      const base = owner.createElement('base');
      base.setAttribute('href', location.href);
      head.prepend(base);
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

  /**
   * 今の文書の写し（スクリプトを除き、CSS と画像を data: に埋め込んだもの）。スナップショットとコメントの画像が使う。
   * @param {Map<Element, number>} indices 記述の要素の番号（写しの要素に目印として付ける）
   */
  async function copyDocument(indices) {
    inlined = new Map();
    const owner = document.implementation.createHTMLDocument('');
    const root = /** @type {Element} */ (await snapshotNode(document.documentElement, owner, indices));
    const head = root.querySelector('head');
    if (head) {
      for (const sheet of document.adoptedStyleSheets) {
        head.append(styleElement(owner, (await sheetCss(sheet, document.baseURI)) ?? ''));
      }
    }
    return { owner, root, head };
  }

  // ---- コメントの画像（R-PAGE-COMMENT） ----
  // 場所の周りに描き込みと番号を重ねた PNG。今の文書の写しを SVG の foreignObject に入れて画像として読み、
  // canvas に描いて作る。ブラウザを外から操作しない（live.md の RL7）。SVG の画像の中では外の資源を
  // 読まないので、写しの CSS と画像を data: に埋め込んであることを使う。画像の中は XML として読まれ、
  // 宣言的な shadow root が働かないので、shadow root の中身は持ち主の子として並べ直す（スコープの効かない分の
  // 見た目の違いは、仕様が認める細部の違い）。

  /** 場所の外接矩形の周りに足す余白（CSS ピクセル）。 */
  const IMAGE_MARGIN = 48;
  /** 画像の長い辺の上限（ピクセル）。超える範囲は縮めて描く。 */
  const IMAGE_MAX_SIDE = 2000;
  /**
   * PNG の大きさの上限。base64 で 4/3 倍になっても、要求の本文の上限（8 MB）に収まる大きさ。コメントの本文と場所を
   * 足して収まらないときは、レビュー画面が画像なしで保存する。
   */
  const IMAGE_MAX_BYTES = 5 * 1024 * 1024;
  /** 描き込みと番号の色（画面モックの紫）。 */
  const PLACE_COLOR = 'rgb(123, 79, 208)';

  /**
   * @typedef {{ x: number, y: number }} Point
   * @typedef {{ x: number, y: number, w: number, h: number }} Rect
   * @typedef {{ n: number, kind: string, points: Point[], elements: { rect: Rect, root: boolean }[], at: Point | null }} ImagePlace
   */

  /** @param {unknown} value */
  const finite = (value) => (typeof value === 'number' && Number.isFinite(value) ? value : 0);

  /**
   * 頼まれた場所を、描ける形に読む（数でない値は 0 に）。
   * @param {unknown[]} places
   * @returns {ImagePlace[]}
   */
  function readPlaces(places) {
    return places.map((place) => {
      const { n, kind, points, elements, at } = /** @type {Record<string, unknown>} */ (place ?? {});
      const point = /** @type {Record<string, unknown> | null} */ (at && typeof at === 'object' ? at : null);
      return {
        n: finite(n),
        kind: String(kind),
        points: (Array.isArray(points) ? points : []).map((point) => ({ x: finite(point?.x), y: finite(point?.y) })),
        elements: (Array.isArray(elements) ? elements : []).map((element) => {
          const rect = element?.rect ?? {};
          return { rect: { x: finite(rect.x), y: finite(rect.y), w: finite(rect.w), h: finite(rect.h) }, root: isPageRoot(element?.selector) };
        }),
        at: point ? { x: finite(point.x), y: finite(point.y) } : null,
      };
    });
  }

  /**
   * セレクタが文書の根（html か body）を指すか。shadow root の中のセレクタ（` >>> ` を含む）は読めずに false。
   * @param {unknown} selector
   */
  function isPageRoot(selector) {
    if (typeof selector !== 'string') return false;
    try {
      const element = document.querySelector(selector);
      return element !== null && (element === document.documentElement || element === document.body);
    } catch {
      return false;
    }
  }

  /**
   * 文書の根だけを指す要素の場所（地を選んだ）の押した点。ほかの場所と、押した点を持たない場所（保存したもの）は null。
   * @param {ImagePlace} place
   */
  function onlyRootAt(place) {
    return place.kind === 'element' && place.elements.every(({ root }) => root) ? place.at : null;
  }

  /**
   * 番号を置く点。要素の場所は文書の根でない最初の要素の左上か、地を選んだなら押した点。矢印とペンは最初の点。
   * @param {ImagePlace} place
   * @returns {Point | undefined}
   */
  function placeAnchor(place) {
    if (place.kind !== 'element') return place.points[0];
    return place.elements.find(({ root }) => !root)?.rect ?? place.at ?? place.elements[0]?.rect;
  }

  /**
   * 画像にする文書の範囲。頼まれた矩形か、無ければ場所の外接矩形に余白を足したもの。文書の外は切る。
   * @param {unknown} asked
   * @param {ImagePlace[]} places
   * @param {number} width 文書の幅
   * @param {number} height 文書の高さ
   * @returns {Rect}
   */
  function imageArea(asked, places, width, height) {
    /** @type {Rect} */
    let area;
    if (asked && typeof asked === 'object') {
      const rect = /** @type {Record<string, unknown>} */ (asked);
      area = { x: finite(rect.x), y: finite(rect.y), w: finite(rect.w), h: finite(rect.h) };
    } else {
      const xs = [];
      const ys = [];
      for (const place of places) {
        for (const point of place.points) {
          xs.push(point.x);
          ys.push(point.y);
        }
        // 文書の根の箱は文書全体なので範囲に入れない。地を選んだ要素の場所は押した点を入れる。
        for (const { rect } of place.elements.filter(({ root }) => !root)) {
          xs.push(rect.x, rect.x + rect.w);
          ys.push(rect.y, rect.y + rect.h);
        }
        const at = onlyRootAt(place);
        if (at) {
          xs.push(at.x);
          ys.push(at.y);
        }
      }
      if (xs.length === 0) throw new Error('no place to draw around');
      const left = Math.min(...xs) - IMAGE_MARGIN;
      const top = Math.min(...ys) - IMAGE_MARGIN;
      area = { x: left, y: top, w: Math.max(...xs) + IMAGE_MARGIN - left, h: Math.max(...ys) + IMAGE_MARGIN - top };
    }
    const left = Math.max(0, Math.floor(area.x));
    const top = Math.max(0, Math.floor(area.y));
    const right = Math.min(width, Math.ceil(area.x + area.w));
    const bottom = Math.min(height, Math.ceil(area.y + area.h));
    if (right <= left || bottom <= top) throw new Error('the area is outside the page');
    return { x: left, y: top, w: right - left, h: bottom - top };
  }

  /**
   * 写しの shadow root（宣言的な shadow root の <template>）を、持ち主の子として並べ直す。slot には割り当てられる
   * 子を入れ、割り当てのない slot は中の既定の中身にする。どの slot にも入らない子は描かれないので外す。
   * @param {Element} root
   */
  function flattenShadowRoots(root) {
    for (let template = root.querySelector('template[shadowrootmode]'); template; template = root.querySelector('template[shadowrootmode]')) {
      const host = template.parentElement;
      const shadow = /** @type {HTMLTemplateElement} */ (template).content;
      template.remove();
      if (!host) continue;
      const light = [...host.childNodes];
      for (const slot of [...shadow.querySelectorAll('slot')]) {
        const name = slot.getAttribute('name') ?? '';
        const assigned = light.filter((node) => node.parentNode === host && (node instanceof Element ? node.getAttribute('slot') ?? '' : '') === name);
        slot.replaceWith(...(assigned.length > 0 ? assigned : [...slot.childNodes]));
      }
      host.replaceChildren(shadow);
    }
  }

  /**
   * 文書の写しを、範囲を縦に画面の高さずつ切った SVG の画像として 1 枚ずつ読み、context に描く。SVG の画像の中の
   * メディアクエリと vw・vh は SVG の大きさで決まるので、画像は動いているページの画面と同じ大きさにし、写しを
   * ずらして範囲を画面に入れる（範囲の大きさにすると、幅で変わる CSS や vh の要素が別の見た目で並ぶ）。
   * 写しは 1 回だけ符号化して使い回し、読んだ画像は描いたら手放す（長い範囲でも持つのは 1 枚だけ）。
   * @param {CanvasRenderingContext2D} context 範囲の左上を原点にし、縮尺を掛けてある
   * @param {Rect} area
   * @param {number} width 文書の幅
   * @param {number} height 文書の高さ
   * @param {{ w: number, h: number }} screen 動いているページの画面の大きさ
   */
  async function drawPageCopy(context, area, width, height, screen) {
    const { root } = await copyDocument(new Map());
    flattenShadowRoots(root);
    const body = encodeURIComponent(new XMLSerializer().serializeToString(root));
    const end = encodeURIComponent('</foreignObject></svg>');
    for (let top = area.y; top < area.y + area.h; top += screen.h) {
      const start = `<svg xmlns="http://www.w3.org/2000/svg" width="${screen.w}" height="${screen.h}">`
        + `<foreignObject x="0" y="${-top}" width="${width}" height="${height}">`;
      const image = new Image();
      image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(start)}${body}${end}`;
      try {
        await image.decode();
      } catch {
        throw new Error('the copy of the page could not be drawn as an image (the page may forbid data: images)');
      }
      const tileHeight = Math.min(screen.h, area.y + area.h - top);
      context.drawImage(image, area.x, 0, area.w, tileHeight, 0, top - area.y, area.w, tileHeight);
    }
  }

  /**
   * 場所の描き込みと番号を描く。座標は文書の座標で、呼ぶ側が範囲の左上へずらしておく。
   * @param {CanvasRenderingContext2D} context
   * @param {ImagePlace[]} places
   */
  function drawPlaces(context, places) {
    context.strokeStyle = PLACE_COLOR;
    context.fillStyle = PLACE_COLOR;
    context.lineWidth = 2;
    context.lineJoin = 'round';
    context.lineCap = 'round';
    for (const place of places) {
      if (place.kind === 'element') {
        // 文書の根の箱は文書全体を囲むので描かない。地を選んだ場所は番号だけで示す。
        for (const { rect } of place.elements.filter(({ root }) => !root)) context.strokeRect(rect.x - 2, rect.y - 2, rect.w + 4, rect.h + 4);
      } else if (place.points.length > 0) {
        context.beginPath();
        context.moveTo(place.points[0].x, place.points[0].y);
        for (const point of place.points.slice(1)) context.lineTo(point.x, point.y);
        context.stroke();
        if (place.kind === 'arrow' && place.points.length > 1) drawArrowHead(context, place.points.at(-2), place.points.at(-1));
      }
    }
    for (const place of places) {
      const anchor = placeAnchor(place);
      if (!anchor) continue;
      // 文書の左上の端の場所でも、番号が切れないように内側へ寄せる。
      drawNumber(context, place.n, Math.max(10, anchor.x), Math.max(10, anchor.y));
    }
  }

  /**
   * @param {CanvasRenderingContext2D} context
   * @param {Point | undefined} from
   * @param {Point | undefined} to
   */
  function drawArrowHead(context, from, to) {
    if (!from || !to) return;
    const angle = Math.atan2(to.y - from.y, to.x - from.x);
    context.beginPath();
    context.moveTo(to.x, to.y);
    context.lineTo(to.x - 12 * Math.cos(angle - 0.45), to.y - 12 * Math.sin(angle - 0.45));
    context.lineTo(to.x - 12 * Math.cos(angle + 0.45), to.y - 12 * Math.sin(angle + 0.45));
    context.closePath();
    context.fill();
  }

  /**
   * 番号の丸。場所の起点の左上に重ねる。
   * @param {CanvasRenderingContext2D} context
   * @param {number} n
   * @param {number} x
   * @param {number} y
   */
  function drawNumber(context, n, x, y) {
    context.save();
    context.beginPath();
    context.arc(x, y, 10, 0, Math.PI * 2);
    context.fillStyle = PLACE_COLOR;
    context.fill();
    context.fillStyle = 'rgb(255, 255, 255)';
    context.font = 'bold 11px sans-serif';
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText(String(n), x, y + 0.5);
    context.restore();
  }

  /**
   * @param {HTMLCanvasElement} canvas
   * @returns {Promise<Blob>}
   */
  function canvasPng(canvas) {
    return new Promise((resolve, reject) => {
      try {
        canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('the image could not be encoded'))), 'image/png');
      } catch {
        // 写しを描いた canvas が汚染されて読み出せないブラウザ。
        reject(new Error('the drawn copy of the page cannot be read back'));
      }
    });
  }

  /**
   * @param {Rect} area
   * @param {number} scale
   */
  function sizedCanvas(area, scale) {
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(area.w * scale));
    canvas.height = Math.max(1, Math.round(area.h * scale));
    return canvas;
  }

  /**
   * 頼まれた画像のページと表示幅に、いまの文書が並んでいなければ断る。画像を作る途中でページが移ったり幅が変わったり
   * すると、場所の座標と写しの並びが合わない画像になる。幅は 1px の違いまで同じとみなす。端数の拡大率（ブラウザの
   * 拡大縮小など）では、枠の幅を装置の画素に丸めた分だけ innerWidth が頼んだ幅から 1 ずれる。
   * @param {{ page: string, width: number }} view
   */
  function checkView(view) {
    if (location.pathname + location.search !== view.page) throw new Error('the page is not the page of the places');
    if (Math.abs(innerWidth - view.width) > 1) throw new Error('the page is not laid out at the width of the places');
  }

  /**
   * 場所の周りの画像を作る。PNG が上限を超えるときは縮めて描き直し、それでも超えれば諦める。画面の大きさは頼まれた値
   * だけを使い、文書の大きさは始めに 1 回だけ読む（途中で読むと、その間に変わった並びで描く）。
   * @param {unknown} asked 画像にする文書の矩形（無ければ場所から決める）
   * @param {unknown[]} requested 重ねる場所（番号・種類・点・要素の矩形）
   * @param {{ page: string, width: number, height: number }} view 場所のページ・表示幅・並べた画面の高さ
   * @returns {Promise<{ png: string, width: number, height: number }>} png は base64
   */
  async function placesImage(asked, requested, view) {
    checkView(view);
    const places = readPlaces(requested);
    const screen = { w: Math.max(1, view.width), h: Math.max(1, view.height) };
    const width = document.documentElement.clientWidth;
    const height = Math.max(document.documentElement.scrollHeight, screen.h);
    const area = imageArea(asked, places, width, height);
    // 写しは最初の縮尺で 1 回だけ描き、上限を超えて縮めるときはそれを縮めて使う。場所は縮尺ごとに描き直す。
    let scale = Math.min(1, IMAGE_MAX_SIDE / Math.max(area.w, area.h));
    const page = sizedCanvas(area, scale);
    const pageContext = /** @type {CanvasRenderingContext2D} */ (page.getContext('2d'));
    pageContext.fillStyle = 'rgb(255, 255, 255)';
    pageContext.fillRect(0, 0, page.width, page.height);
    pageContext.scale(scale, scale);
    await drawPageCopy(pageContext, area, width, height, screen);
    checkView(view);
    for (let attempt = 0; attempt < 4; attempt += 1, scale *= 0.6) {
      const canvas = sizedCanvas(area, scale);
      const context = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'));
      context.drawImage(page, 0, 0, canvas.width, canvas.height);
      context.scale(scale, scale);
      context.translate(-area.x, -area.y);
      drawPlaces(context, places);
      const blob = await canvasPng(canvas);
      if (blob.size <= IMAGE_MAX_BYTES) return { png: await blobBase64(blob), width: canvas.width, height: canvas.height };
    }
    throw new Error('the image is too large to send');
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
