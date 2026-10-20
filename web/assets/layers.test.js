// 層の規則（PROJECT.md「web/assets/ の層」）と index.html の modulepreload の一覧を、import 文から確かめる。
// あわせて index.html の stylesheet の順を確かめる。
// 辺にするのは行頭（字下げを含む）の静的な import / export … from だけ。JSDoc の import("…") は型だけの
// 参照で読み込みを生まないので数えない。動的な import(…) は app.js が --live のモジュールを読む所だけに
// 限ることを、別に確かめる。
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, posix } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const assets = dirname(fileURLToPath(import.meta.url));
const web = dirname(assets);
const root = dirname(web);

/**
 * web/assets の下の、テストを除く .js（web/assets からの相対パス）。
 * @param {string} dir
 * @returns {string[]}
 */
function modules(dir = "") {
  return readdirSync(join(assets, dir), { withFileTypes: true }).flatMap((item) => {
    const path = dir === "" ? item.name : `${dir}/${item.name}`;
    if (item.isDirectory()) {
      return modules(path);
    }
    return item.name.endsWith(".js") && !item.name.endsWith(".test.js") ? [path] : [];
  });
}

/**
 * そのモジュールが静的に import するモジュール（web/assets からの相対パス）。
 * @param {string} path
 * @returns {string[]}
 */
function importsOf(path) {
  const source = readFileSync(join(assets, path), "utf8");
  const statement = /^\s*(?:import|export)\s*(?:[^;"']*?\bfrom\s*)?["']([^"']+)["']/gm;
  const found = [...source.matchAll(statement)];
  // 読めない書き方（波括弧の中の文字列やクォートを含むコメントなど）の import は、黙って辺から外れると
  // 違反が通る。行頭の import と export … from の数と合わなければ落とす。
  const starts =
    [...source.matchAll(/^\s*import\s*[\w*{"']/gm)].length +
    [...source.matchAll(/^\s*export\s*(?:\*|\{)[^;]*?\bfrom\b/gm)].length;
  assert.equal(found.length, starts, `${path}: 読めない import / export … from がある`);
  return found.map((match) => posix.normalize(posix.join(posix.dirname(path), match[1])));
}

/**
 * コメントを除いたソース（JSDoc の import("…") を動的な import と取り違えないため）。
 * @param {string} source
 * @returns {string}
 */
function withoutComments(source) {
  return source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

const graph = new Map(modules().map((path) => [path, importsOf(path)]));

/**
 * `--live` のページの見方のモジュール（PROJECT.md の例外。live.css は JS ではないのでここに無い）。
 * features/live/ の下と views/live*.js に分けても含める。名前に live を含むだけのモジュールは含めない。
 * @param {string} path
 * @returns {boolean}
 */
function isLiveModule(path) {
  return (
    path === "features/live.js" ||
    path.startsWith("features/live/") ||
    /^views\/live[^/]*\.js$/.test(path) ||
    path === "live-model.js" ||
    path === "live-diff.js"
  );
}

/**
 * features の順番。PROJECT.md の `a → b → …` の 1 か所を正典にする。
 * @returns {string[]}
 */
function featureOrder() {
  const project = readFileSync(join(root, "PROJECT.md"), "utf8");
  const chains = [...project.matchAll(/`([\w-]+(?: → [\w-]+)+)`/g)];
  assert.equal(chains.length, 1, "PROJECT.md に features の順番（`a → b → …`）がちょうど 1 つあること");
  return chains[0][1].split(" → ");
}

/**
 * モジュールの層。features/ の下のディレクトリは同じ名前の feature の一部（child）。
 * @param {string} path
 * @returns {{ layer: "app" } | { layer: "feature", name: string, child: boolean } | { layer: "view" } | { layer: "leaf" }}
 */
function layerOf(path) {
  if (path === "app.js") {
    return { layer: "app" };
  }
  if (path.startsWith("features/")) {
    const segments = path.split("/");
    return { layer: "feature", name: segments[1].replace(/\.js$/, ""), child: segments.length > 2 };
  }
  if (path.startsWith("views/")) {
    return { layer: "view" };
  }
  return { layer: "leaf" };
}

/**
 * import の向きの違反（`from → to`）。
 * @param {(from: string, to: string) => boolean} allowed
 * @param {(path: string) => boolean} applies
 * @returns {string[]}
 */
function violations(applies, allowed) {
  return [...graph]
    .filter(([from]) => applies(from))
    .flatMap(([from, imports]) => imports.filter((to) => !allowed(from, to)).map((to) => `${from} → ${to}`));
}

test("動的な import は app.js が --live のモジュールを読む所だけ", () => {
  const found = modules().flatMap((path) => {
    const code = withoutComments(readFileSync(join(assets, path), "utf8"));
    return [...code.matchAll(/\bimport\s*\(\s*(?:["']([^"']+)["'])?/g)].map((match) => ({
      from: path,
      to: match[1] === undefined ? null : posix.normalize(posix.join(posix.dirname(path), match[1])),
    }));
  });
  assert.ok(found.length > 0, "app.js が --live のモジュールを動的に読む所が見つかること");
  assert.deepEqual(
    found.filter(({ from, to }) => from !== "app.js" || to === null || !isLiveModule(to)).map(({ from, to }) => `${from} → ${to}`),
    [],
  );
});

test("import する先はどれも web/assets の中にある", () => {
  const missing = [...graph].flatMap(([from, imports]) =>
    imports.filter((to) => !graph.has(to)).map((to) => `${from} → ${to}`),
  );
  assert.deepEqual(missing, []);
});

test("features は自分より前の feature だけを直接 import し、features/<name>/ の下は親を import しない", () => {
  const order = featureOrder();
  const unknown = [...graph.keys()]
    .map(layerOf)
    .flatMap((layer) => (layer.layer === "feature" && !order.includes(layer.name) ? [layer.name] : []));
  assert.deepEqual([...new Set(unknown)], [], "features の順番に載っていない feature");
  const found = violations(
    (from) => layerOf(from).layer === "feature",
    (from, to) => {
      const source = layerOf(from);
      const target = layerOf(to);
      if (target.layer === "view" || target.layer === "leaf") {
        return true;
      }
      if (source.layer !== "feature" || target.layer !== "feature") {
        return false;
      }
      if (source.name === target.name) {
        // 親（features/<name>.js）から下のファイルへの向きだけ。下から親を呼ぶときは actions を通す。
        return !(source.child && !target.child);
      }
      return order.indexOf(target.name) < order.indexOf(source.name);
    },
  );
  assert.deepEqual(found, []);
});

test("views は api.js・features・app.js を import しない", () => {
  const found = violations(
    (from) => layerOf(from).layer === "view",
    (_from, to) => {
      const target = layerOf(to);
      return target.layer === "view" || (target.layer === "leaf" && to !== "api.js");
    },
  );
  assert.deepEqual(found, []);
});

test("leaf は許可表のほかを import しない", () => {
  /**
   * state.js は storage.js と model.js だけ。diagrams/ は shoryo から取り込んだ図の描画の束で、中では互いに
   * import する。ほかの leaf は何も import しない。
   * @param {string} from
   * @param {string} to
   */
  const allowed = (from, to) => {
    if (from === "state.js") {
      return to === "storage.js" || to === "model.js";
    }
    if (from.startsWith("diagrams/")) {
      return to.startsWith("diagrams/");
    }
    return false;
  };
  const found = violations((from) => layerOf(from).layer === "leaf", allowed);
  assert.deepEqual(found, []);
});

test("modulepreload は app.js から静的にたどれるモジュールとちょうど同じで、--live のモジュールを含まない", () => {
  const html = readFileSync(join(web, "index.html"), "utf8");
  const preloaded = [...html.matchAll(/<link\s+rel="modulepreload"\s+href="assets\/([^"]+)"\s*\/?>/g)].map(
    (match) => match[1],
  );
  /** @type {Set<string>} */
  const reachable = new Set();
  const pending = ["app.js"];
  while (pending.length > 0) {
    const path = /** @type {string} */ (pending.pop());
    if (!reachable.has(path)) {
      reachable.add(path);
      pending.push(...(graph.get(path) ?? []));
    }
  }
  // 先に確かめる。--live のモジュールを静的に import すると、下の「modulepreload に無い」より先に、
  // 原因どおりの文言で落ちる。
  assert.deepEqual(
    [...reachable].filter(isLiveModule).sort(),
    [],
    "--live のモジュールは --live のレビューでだけ動的に読む（静的に import しない）",
  );
  assert.deepEqual(
    preloaded.filter(isLiveModule),
    [],
    "--live のモジュールは modulepreload に載せない",
  );
  assert.deepEqual(
    [...reachable].filter((path) => !preloaded.includes(path)).sort(),
    [],
    "modulepreload に無いモジュール",
  );
  assert.deepEqual(
    preloaded.filter((path) => !reachable.has(path)).sort(),
    [],
    "app.js から静的にたどれないのに modulepreload にあるモジュール",
  );
  assert.equal(new Set(preloaded).size, preloaded.length, "modulepreload の重複");
});

test("index.html は部品の stylesheet の後に style.css を読む", () => {
  // parts.css の `.part-copy.iconbtn` と style.css の `.iconbtn:hover` のように、部品の規則と style.css の
  // 規則は詳細度が同じで同じ要素に当たることがあり、後に読んだ方が勝つ。今の見た目（コードの写しのボタンに
  // 乗せると .iconbtn:hover の色になる）は、この順で決まっている。
  const html = readFileSync(join(web, "index.html"), "utf8");
  const sheets = [...html.matchAll(/<link\s+rel="stylesheet"\s+href="assets\/([^"]+)"\s*\/?>/g)].map((match) => match[1]);
  assert.deepEqual(sheets, ["parts.css", "explanation.css", "style.css"]);
});
