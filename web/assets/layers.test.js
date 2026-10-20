// 層の規則（PROJECT.md「web/assets/ の層」）と index.html の modulepreload の一覧を、import 文から確かめる。
// 読むのは行頭の import / export … from だけ。JSDoc の import("…") は型だけの参照で読み込みを生まず、
// 字下げした動的 import（--live のモジュール）は modulepreload に載せない約束なので、どちらも数えない。
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
  const statement = /^(?:import|export)\s+(?:[\w*{}\s,$]+?\s+from\s+)?["']([^"']+)["']/gm;
  return [...source.matchAll(statement)].map((match) => posix.normalize(posix.join(posix.dirname(path), match[1])));
}

const graph = new Map(modules().map((path) => [path, importsOf(path)]));

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
 * モジュールの層。features/ の下のディレクトリは同じ名前の feature の一部。
 * @param {string} path
 * @returns {{ layer: "app" } | { layer: "feature", name: string } | { layer: "view" } | { layer: "leaf" }}
 */
function layerOf(path) {
  if (path === "app.js") {
    return { layer: "app" };
  }
  if (path.startsWith("features/")) {
    return { layer: "feature", name: path.split("/")[1].replace(/\.js$/, "") };
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

test("import する先はどれも web/assets の中にある", () => {
  const missing = [...graph].flatMap(([from, imports]) =>
    imports.filter((to) => !graph.has(to)).map((to) => `${from} → ${to}`),
  );
  assert.deepEqual(missing, []);
});

test("features は自分より前の feature だけを直接 import する", () => {
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
      return source.name === target.name || order.indexOf(target.name) < order.indexOf(source.name);
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
  assert.deepEqual(
    preloaded.filter((path) => path.includes("live")),
    [],
    "--live のモジュールは --live のレビューでだけ動的に読む",
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
