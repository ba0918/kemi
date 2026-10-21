// ブラウザ検査の待ち。条件が成り立たないまま止まり続けないよう、どの待ちにも期限を付ける。期限は順序を決める
// ためではなく、止まらないための上限。

/**
 * check が真を返すまで繰り返す。期限を過ぎたら、最後に返した値を添えて失敗にする。
 * @template T
 * @param {() => T | Promise<T>} check
 * @param {{ timeout?: number, interval?: number, what?: string }} [options]
 * @returns {Promise<T>}
 */
export async function until(check, { timeout = 15000, interval = 50, what = 'the condition' } = {}) {
  const deadline = performance.now() + timeout;
  for (;;) {
    const value = await check();
    if (value) {
      return value;
    }
    if (performance.now() >= deadline) {
      throw new Error(`${what} was not met within ${timeout} ms (last: ${JSON.stringify(value)})`);
    }
    await new Promise((done) => setTimeout(done, interval));
  }
}

/**
 * promise が期限までに決まらなければ失敗にする。
 * @template T
 * @param {Promise<T>} promise
 * @param {number} timeout
 * @param {() => string} describe 期限切れのときの文
 * @returns {Promise<T>}
 */
export async function withTimeout(promise, timeout, describe) {
  let timer;
  const expired = new Promise((_, fail) => {
    timer = setTimeout(() => fail(new Error(describe())), timeout);
  });
  try {
    return await Promise.race([promise, expired]);
  } finally {
    clearTimeout(timer);
  }
}
