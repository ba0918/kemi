// `--live` の検査に使う、試験用の開発サーバ（docs/spec/kemi.md の R-VERIFY）。Node の標準だけで、
// ビルドの要らない静的ファイルと、HMR に当たる知らせ（ファイルを書き換えると WebSocket で
// ページに知らせ、CSS を差し替える）を返す。
//
//   node scripts/live-dev-server.mjs [--port <n>] [--dir <dir>]
//
// 起動すると stdout に `dev: http://127.0.0.1:<port>/` の 1 行を出す。`--dir` を省くと一時
// ディレクトリにページを書き出す。ほかのスクリプトからは `startDevServer` を import して使う。
//
// ページ:
//   /             style.css を読み、HMR の知らせで CSS を差し替える
//   /framed.html  `X-Frame-Options: DENY` と `frame-ancestors 'none'` を返す
//   /rich.html    shadow DOM・canvas・SVG・入力欄・onclick（押すと背景が黄色になる、左上 (0, 200) の
//                 300×100 のボタン）を持つ
//   /siblings.html  兄弟の並び。`?extra=1` で途中に 1 つ足す
//   /resources.html  CSS・@import・style 属性の url()・<picture> の <source>・video の poster・SVG の
//                 <image>・<input type=image> で、1 つずつ色の違う 80×80 の箱を描く。別のオリジンの
//                 CSS は同じサーバを localhost で指す（中継から見て別のオリジン）。箱の位置と色は
//                 RESOURCE_BOXES。video はスクリプトの止まった枠では操作部が必ず重なるので、動いている
//                 ページでも controls で重ねておく
//   /referrer.html  `?image=<url>` の画像を `referrerpolicy="unsafe-url"` 付きで出す（スナップショットが
//                 外部の画像へ referrer を送るかの確かめ）
//   /tall.html    高さ 3000px の色の帯（スクロールをそろえる確かめに使う。#band-<n> で移れる）
//   /other.html   別のページ（ページの移動に使う）
//   /changing.html  同じ URL のまま中身が変わるページ（差分の確かめに使う）。兄弟の並び（#items の li）と、
//                 その下のボタン（#buy）と文（.note）と、高い余白を持つ。changing.css を書き換えると HMR の知らせで CSS を
//                 差し替え、changing.html を書き換えると読み込み直さずに body を差し替える。中身は CHANGING_*
//   /many.html    要素の多いページ（差分の計算の時間を測るのに使う）。`?cards=<n>`（既定 1250）枚のカードを並べ、
//                 1 枚は 4 要素（カード・見出し・文・ボタン）。many.css を書き換えると HMR の知らせで差し替える
//   /__cookies    受け取った Cookie ヘッダを JSON で返す（中継が cookie を外すかの確かめ）
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { watch } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const HMR_CLIENT = `
const socket = new WebSocket((location.protocol === 'https:' ? 'wss://' : 'ws://') + location.host + '/__hmr');
socket.addEventListener('message', (event) => {
  const change = JSON.parse(event.data);
  if (change.type !== 'css') return;
  for (const link of document.querySelectorAll('link[rel="stylesheet"]')) {
    const href = new URL(link.getAttribute('href'), location.href);
    if (href.pathname === change.path) {
      href.searchParams.set('t', String(change.version));
      link.href = href.pathname + href.search;
    }
  }
  document.documentElement.dataset.hmr = String(change.version);
});
// HTML の書き換えは、読み込み直さずに body を差し替える（フレームワークの HMR が DOM を直すのに当たる）。
socket.addEventListener('message', async (event) => {
  const change = JSON.parse(event.data);
  if (change.type !== 'html' || change.path !== location.pathname) return;
  const html = await (await fetch(location.href, { cache: 'no-store' })).text();
  document.body.innerHTML = new DOMParser().parseFromString(html, 'text/html').body.innerHTML;
  document.documentElement.dataset.hmrHtml = String(change.version);
});
socket.addEventListener('open', () => { document.documentElement.dataset.hmrConnected = 'true'; });
`;

const PAGES = {
  'index.html': `<!doctype html>
<html><head><meta charset="utf-8"><title>Dev index</title>
<link rel="stylesheet" href="/style.css">
<script type="module" src="/__hmr.js"></script>
</head><body>
<h1 id="title">Dev server</h1>
<p class="box">Edit style.css to see the page change.</p>
<a id="to-other" href="/other.html">Other page</a>
</body></html>
`,
  'style.css': `.box { background: rgb(255, 0, 0); width: 200px; height: 80px; }\n`,
  'framed.html': `<!doctype html>
<html><head><meta charset="utf-8"><title>Framed</title></head>
<body><p id="framed">This page refuses to be framed.</p></body></html>
`,
  'rich.html': `<!doctype html>
<html><head><meta charset="utf-8"><title>Rich</title>
<style>
  body { margin: 0; font: 16px/1.4 sans-serif; }
  .row { display: flex; gap: 12px; padding: 12px; }
  #button { position: absolute; left: 0; top: 200px; width: 300px; height: 100px; }
</style>
</head><body>
<div class="row">
  <div id="host"></div>
  <canvas id="canvas" width="120" height="60"></canvas>
  <svg width="60" height="60"><circle cx="30" cy="30" r="25" fill="rgb(0, 0, 255)"></circle></svg>
</div>
<div class="row">
  <input id="field" type="text">
  <input id="check" type="checkbox">
</div>
<button id="button" onclick="document.body.style.background = 'rgb(255, 255, 0)'">Press</button>
<script>
  const host = document.getElementById('host');
  const root = host.attachShadow({ mode: 'open' });
  root.innerHTML = '<style>p { color: rgb(200, 0, 200); margin: 0; }</style><p id="inside">Inside the shadow root</p>';
  const context = document.getElementById('canvas').getContext('2d');
  context.fillStyle = 'rgb(255, 160, 0)';
  context.fillRect(10, 10, 100, 40);
  document.getElementById('field').value = 'typed value';
  document.getElementById('check').checked = true;
</script>
</body></html>
`,
  'tall.html': `<!doctype html>
<html><head><meta charset="utf-8"><title>Tall</title>
<style>body { margin: 0; } .band { height: 300px; font: 24px sans-serif; padding: 12px; box-sizing: border-box; }</style>
</head><body>
${Array.from({ length: 10 }, (_, index) => `<div class="band" id="band-${index + 1}" style="background: hsl(${index * 36}, 70%, 70%)">Band ${index + 1}</div>`).join('\n')}
</body></html>
`,
  'other.html': `<!doctype html>
<html><head><meta charset="utf-8"><title>Other</title></head>
<body><h1 id="other">Other page</h1><a id="to-index" href="/">Back</a></body></html>
`,
};

/** /changing.html の兄弟の並び。 */
export const CHANGING_ITEMS = ['one', 'two', 'three', 'four'];

/** /changing.html のボタンの背景色。 */
export const CHANGING_BUTTON = 'rgb(49, 89, 214)';

/**
 * /changing.html の要素の高さ（左上は 0, 0 から縦に並ぶ）。兄弟 1 つ、ボタン（幅も）、文、末尾の余白。
 * 末尾の余白は、スナップショットの枠をスクロールできる高さにするため。
 */
export const CHANGING_GEOMETRY = { item: 30, button: { width: 200, height: 60 }, note: 40 };
const CHANGING_TAIL = 2000;

/**
 * /changing.html の中身。`items` の順に兄弟を並べる。
 * @param {string[]} [items]
 */
export function changingPage(items = CHANGING_ITEMS) {
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Changing</title>
<link rel="stylesheet" href="/changing.css">
<script type="module" src="/__hmr.js"></script>
</head><body>
<ul id="items">${items.map((item) => `<li class="item">${item}</li>`).join('')}</ul>
<button id="buy" class="buy">Buy</button>
<p class="note">Below the button</p>
<div class="tail"></div>
</body></html>
`;
}

/**
 * /changing.html の CSS。`button` はボタンの背景色。
 * @param {string} [button]
 */
export function changingCss(button = CHANGING_BUTTON) {
  return `body { margin: 0; font: 16px sans-serif; }
ul { margin: 0; padding: 0; list-style: none; }
.item { height: ${CHANGING_GEOMETRY.item}px; }
.buy { display: block; width: ${CHANGING_GEOMETRY.button.width}px; height: ${CHANGING_GEOMETRY.button.height}px; border: 0; color: rgb(255, 255, 255); background: ${button}; }
.note { margin: 0; height: ${CHANGING_GEOMETRY.note}px; }
.tail { height: ${CHANGING_TAIL}px; }
`;
}

PAGES['changing.html'] = changingPage();
PAGES['changing.css'] = changingCss();
PAGES['many.css'] = '.cards { display: flex; flex-wrap: wrap; gap: 8px; margin: 8px; } .card { width: 160px; border: 1px solid rgb(220, 220, 220); border-radius: 8px; padding: 8px; } .card button { background: rgb(49, 89, 214); color: rgb(255, 255, 255); }\n';

/** @param {number} cards */
function manyPage(cards) {
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Many</title>
<link rel="stylesheet" href="/many.css">
<script type="module" src="/__hmr.js"></script>
</head><body><div class="cards">
${Array.from({ length: cards }, (_, index) => `<div class="card"><h3>Item ${index + 1}</h3><p>Description of item ${index + 1}</p><button>Buy</button></div>`).join('\n')}
</div></body></html>
`;
}

/** /resources.html の箱。名前・左上の位置・色。色の画像は `<名前>.svg` で配る。 */
export const RESOURCE_BOXES = [
  { name: 'imported', left: 10, top: 10, color: [0, 120, 0] },
  { name: 'cross-imported', left: 100, top: 10, color: [0, 0, 160] },
  { name: 'cross', left: 190, top: 10, color: [160, 0, 160] },
  { name: 'style', left: 280, top: 10, color: [0, 160, 160] },
  { name: 'picture', left: 10, top: 100, color: [200, 100, 0] },
  { name: 'poster', left: 100, top: 100, color: [100, 50, 0] },
  { name: 'svg-image', left: 190, top: 100, color: [50, 50, 200] },
  { name: 'input-image', left: 280, top: 100, color: [200, 0, 80] },
];

const colorSvg = ([r, g, b]) => `<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><rect width="80" height="80" fill="rgb(${r}, ${g}, ${b})"/></svg>\n`;
const box = (name) => {
  const { left, top } = RESOURCE_BOXES.find((entry) => entry.name === name) ?? { left: 0, top: 0 };
  return `left: ${left}px; top: ${top}px`;
};

for (const { name, color } of RESOURCE_BOXES) PAGES[`${name}.svg`] = colorSvg(color);
PAGES['never.svg'] = colorSvg([255, 0, 0]);
PAGES['imported.css'] = '.imported { background: url(imported.svg); }\n';
PAGES['cross-imported.css'] = '.cross-imported { background: url(cross-imported.svg); }\n';
PAGES['cross.css'] = '.cross { background: url(cross.svg); }\n';

/** @param {string} cross 別のオリジンとして指す、このサーバの URL */
function resourcesPage(cross) {
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Resources</title>
<style>
  @import url("/imported.css");
  @import url("${cross}cross-imported.css");
  body { margin: 0; }
  .box { position: absolute; display: block; width: 80px; height: 80px; margin: 0; padding: 0; border: 0; }
</style>
<link rel="stylesheet" href="${cross}cross.css">
</head><body>
<div class="box imported" style="${box('imported')}"></div>
<div class="box cross-imported" style="${box('cross-imported')}"></div>
<div class="box cross" style="${box('cross')}"></div>
<div class="box" style="${box('style')}; background: url('/style.svg')"></div>
<picture><source srcset="/picture.svg"><img class="box" src="/never.svg" alt="" style="${box('picture')}"></picture>
<video class="box" poster="/poster.svg" controls style="${box('poster')}"></video>
<svg class="box" style="${box('svg-image')}" viewBox="0 0 80 80"><image href="/svg-image.svg" width="80" height="80" preserveAspectRatio="none"/></svg>
<input class="box" type="image" src="/input-image.svg" alt="" style="${box('input-image')}">
</body></html>
`;
}

function siblingsPage(extra) {
  const items = ['one', 'two', ...(extra ? ['inserted'] : []), 'three', 'four'];
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Siblings</title></head>
<body><ul id="list">${items.map((item) => `<li class="item">${item}</li>`).join('')}</ul></body></html>
`;
}

function referrerPage(image) {
  const src = image.replaceAll('&', '&amp;').replaceAll('"', '&quot;');
  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Referrer</title></head>
<body><img src="${src}" referrerpolicy="unsafe-url" width="10" height="10" alt=""></body></html>
`;
}

const TYPES = { '.html': 'text/html; charset=utf-8', '.css': 'text/css', '.js': 'text/javascript', '.svg': 'image/svg+xml' };

/** WebSocket の受け答え（RFC 6455 の最小）。テキストの送信と、閉じる合図だけを扱う。 */
function acceptWebSocket(request, socket) {
  const key = request.headers['sec-websocket-key'];
  const accept = createHash('sha1').update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest('base64');
  socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
  socket.on('data', (data) => {
    // 送られてくるのは閉じる合図だけを想定する。opcode 8 なら閉じて返す。
    if ((data[0] & 0x0f) === 8) socket.end(Buffer.from([0x88, 0]));
  });
  socket.on('error', () => {});
  return {
    send(text) {
      const payload = Buffer.from(text);
      const header = payload.length < 126
        ? Buffer.from([0x81, payload.length])
        : Buffer.from([0x81, 126, payload.length >> 8, payload.length & 0xff]);
      socket.write(Buffer.concat([header, payload]));
    },
    socket,
  };
}

/**
 * 試験用の開発サーバを立てる。
 * @param {{ port?: number, dir?: string }} [options]
 * @returns {Promise<{ url: string, port: number, dir: string, close: () => Promise<void> }>}
 */
export async function startDevServer({ port = 0, dir } = {}) {
  const root = dir ?? await mkdtemp(join(tmpdir(), 'kemi-dev-'));
  for (const [name, content] of Object.entries(PAGES)) {
    await writeFile(join(root, name), content);
  }
  /** @type {Set<{ send: (text: string) => void, socket: import('node:net').Socket }>} */
  const clients = new Set();
  let version = 0;
  let actual = port;
  const server = createServer(async (request, response) => {
    const url = new URL(request.url ?? '/', 'http://localhost');
    if (url.pathname === '/__hmr.js') {
      response.writeHead(200, { 'Content-Type': 'text/javascript' });
      response.end(HMR_CLIENT);
      return;
    }
    if (url.pathname === '/__cookies') {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ cookie: request.headers.cookie ?? null }));
      return;
    }
    if (url.pathname === '/siblings.html') {
      response.writeHead(200, { 'Content-Type': TYPES['.html'] });
      response.end(siblingsPage(url.searchParams.get('extra') === '1'));
      return;
    }
    if (url.pathname === '/many.html') {
      response.writeHead(200, { 'Content-Type': TYPES['.html'] });
      response.end(manyPage(Math.min(100_000, Math.max(1, Number(url.searchParams.get('cards')) || 1250))));
      return;
    }
    if (url.pathname === '/referrer.html') {
      response.writeHead(200, { 'Content-Type': TYPES['.html'] });
      response.end(referrerPage(url.searchParams.get('image') ?? ''));
      return;
    }
    if (url.pathname === '/resources.html') {
      response.writeHead(200, { 'Content-Type': TYPES['.html'] });
      response.end(resourcesPage(`http://localhost:${actual}/`));
      return;
    }
    const name = url.pathname === '/' ? 'index.html' : normalize(url.pathname.slice(1));
    if (name.startsWith('..') || !Object.hasOwn(PAGES, name)) {
      response.writeHead(404, { 'Content-Type': 'text/plain' });
      response.end('not found');
      return;
    }
    const headers = { 'Content-Type': TYPES[extname(name)] ?? 'application/octet-stream', 'Cache-Control': 'no-store' };
    if (name === 'framed.html') {
      headers['X-Frame-Options'] = 'DENY';
      headers['Content-Security-Policy'] = "frame-ancestors 'none'; script-src 'self'";
    }
    response.writeHead(200, headers);
    response.end(await readFile(join(root, name)));
  });
  server.on('upgrade', (request, socket) => {
    if (new URL(request.url ?? '/', 'http://localhost').pathname !== '/__hmr') {
      socket.destroy();
      return;
    }
    const client = acceptWebSocket(request, socket);
    clients.add(client);
    socket.on('close', () => clients.delete(client));
  });
  const watcher = watch(root, (_, file) => {
    const type = file?.endsWith('.css') ? 'css' : file?.endsWith('.html') ? 'html' : null;
    if (!type) return;
    version += 1;
    for (const client of clients) client.send(JSON.stringify({ type, path: `/${file}`, version }));
  });
  await new Promise((resolve) => server.listen(port, '127.0.0.1', () => resolve(undefined)));
  const address = server.address();
  actual = typeof address === 'object' && address ? address.port : port;
  return {
    url: `http://127.0.0.1:${actual}/`,
    port: actual,
    dir: root,
    close: async () => {
      watcher.close();
      for (const client of clients) client.socket.destroy();
      server.closeAllConnections();
      await new Promise((resolve) => server.close(() => resolve(undefined)));
    },
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const option = (name) => {
    const index = process.argv.indexOf(name);
    return index >= 0 ? process.argv[index + 1] : undefined;
  };
  const dev = await startDevServer({ port: Number(option('--port') ?? 0), dir: option('--dir') });
  console.log(`dev: ${dev.url}`);
  const stop = () => dev.close().then(() => process.exit(0));
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}
