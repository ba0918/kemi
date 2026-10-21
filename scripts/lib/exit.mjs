// ブラウザ検査のプロセスが終わるときの後始末。検査が途中で落ちても（例外・assert の失敗・待ちの期限切れ・
// Ctrl+C や SIGTERM）、起動した kemi とブラウザのセッションを残さない。node そのものが SIGKILL で止められた
// ときだけは、ここでは何もできない。

/** 後始末の段。kemi を止めてから、ブラウザを閉じ、最後にファイルを片付ける（止まる前の kemi が書く場所を消さない）。 */
const PHASES = /** @type {const} */ (['processes', 'browser', 'files']);

/** @type {Record<typeof PHASES[number], ((code: number) => void)[]>} */
const hooks = { processes: [], browser: [], files: [] };
/** @type {(() => Promise<void>)[]} */
const signalHooks = [];
/**
 * 合図で止められたとき、待つ処理の終わりを待つ上限（ms）。過ぎたら待つのをやめて終わる。待つ処理のどれかが終わらないと、
 * 重ねて来た合図も受けないまま止まらなくなるため。kemi に止める合図を送って待つ処理の期限（kemi.mjs の 3 秒）より長くする。
 */
const SIGNAL_HOOKS_TIMEOUT = 5000;
let installed = false;
let stopping = false;

/**
 * プロセスが終わるときに呼ぶ処理を足す。'exit' の中なので同期で終わる処理だけを書く。
 * @param {typeof PHASES[number]} phase
 * @param {(code: number) => void} hook code はプロセスの終了コード
 */
export function onExit(phase, hook) {
  install();
  hooks[phase].push(hook);
}

/**
 * 合図で止められたとき、終わる前に待つ処理を足す。'exit' の中ではできない、待つ後始末（kemi に止める合図を
 * 送って終わるのを待つなど）を書く。待つのは全部で SIGNAL_HOOKS_TIMEOUT までで、それまでに終わらない処理は
 * 待たずに終わる（その後の 'exit' の段の後始末は走る）。
 * @param {() => Promise<void>} hook
 */
export function onSignal(hook) {
  install();
  signalHooks.push(hook);
}

function install() {
  if (installed) {
    return;
  }
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
  // 合図で止められたときは、既定の動き（すぐに終わる）では 'exit' が来ないので、待つ処理を終えてから終了コードを
  // 付けて終わらせる。
  for (const [signal, number] of /** @type {const} */ ([['SIGINT', 2], ['SIGTERM', 15], ['SIGHUP', 1]])) {
    process.on(signal, () => {
      // Ctrl+C の連打で、始めた後始末を二重に走らせない。2 回目ですぐに終わらせないのは、止まる途中の kemi を
      // 殺すことになるため（待つのは SIGNAL_HOOKS_TIMEOUT までなので、無視しても止まらなくなることはない）。
      if (stopping) {
        return;
      }
      stopping = true;
      // 端末の合図は同じプロセスグループの agent-browser にも届き、検査の本筋がその失敗で落ちる。そこで終わると、
      // 止まる途中の kemi を 'exit' の段で殺すので、待つ間の例外は出すだけにする。
      process.on('uncaughtException', (error) => console.error(error));
      const finished = Promise.allSettled(signalHooks.map(async (hook) => hook()));
      const expired = new Promise((done) => setTimeout(() => done('expired'), SIGNAL_HOOKS_TIMEOUT));
      Promise.race([finished, expired]).then((result) => {
        if (result === 'expired') {
          console.error(`the cleanup after ${signal} did not finish within ${SIGNAL_HOOKS_TIMEOUT} ms`);
        }
        process.exit(128 + number);
      });
    });
  }
}
