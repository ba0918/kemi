// ブラウザ検査のプロセスが終わるときの後始末。検査が途中で落ちても（例外・assert の失敗・待ちの期限切れ・
// Ctrl+C や SIGTERM）、起動した kemi とブラウザのセッションを残さない。node そのものが SIGKILL で止められた
// ときだけは、ここでは何もできない。

/** 後始末の段。kemi を止めてから、ブラウザを閉じ、最後にファイルを片付ける（止まる前の kemi が書く場所を消さない）。 */
const PHASES = /** @type {const} */ (['processes', 'browser', 'files']);

/** @type {Record<typeof PHASES[number], ((code: number) => void)[]>} */
const hooks = { processes: [], browser: [], files: [] };
let installed = false;

/**
 * プロセスが終わるときに呼ぶ処理を足す。'exit' の中なので同期で終わる処理だけを書く。
 * @param {typeof PHASES[number]} phase
 * @param {(code: number) => void} hook code はプロセスの終了コード
 */
export function onExit(phase, hook) {
  if (!installed) {
    installed = true;
    process.on('exit', (code) => {
      for (const name of PHASES) {
        for (const each of hooks[name]) {
          try {
            each(code);
          } catch (error) {
            console.error(error);
          }
        }
      }
    });
    // 合図で止められたときは、既定の動き（すぐに終わる）では 'exit' が来ないので、終了コードを付けて終わらせる。
    for (const [signal, number] of /** @type {const} */ ([['SIGINT', 2], ['SIGTERM', 15], ['SIGHUP', 1]])) {
      process.on(signal, () => process.exit(128 + number));
    }
  }
  hooks[phase].push(hook);
}
