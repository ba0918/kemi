//! 大きなファイルの計算を、同時実行数を絞って async ワーカーの外で行う。

use std::sync::Arc;
use tokio::sync::Semaphore;

pub(crate) async fn acquire(limit: &Arc<Semaphore>) -> tokio::sync::OwnedSemaphorePermit {
    limit
        .clone()
        .acquire_owned()
        .await
        .expect("preparation semaphore is never closed")
}

pub(crate) async fn run<T: Send + 'static>(
    permit: tokio::sync::OwnedSemaphorePermit,
    compute: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    tokio::task::spawn_blocking(move || {
        // 要求の future が捨てられても、既に始まった計算が終わるまでは枠を返さない。
        let _permit = permit;
        compute()
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn cancelling_a_request_does_not_release_capacity_while_its_computation_runs() {
        let limit = Arc::new(tokio::sync::Semaphore::new(1));
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let first_limit = limit.clone();
        let task = tokio::spawn(async move {
            let permit = super::acquire(&first_limit).await;
            super::run(permit, move || {
                entered.send(()).expect("test waits for preparation");
                released.recv_timeout(Duration::from_secs(5))
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), started)
            .await
            .expect("preparation starts")
            .expect("start signal");
        task.abort();
        assert!(task.await.expect_err("request cancelled").is_cancelled());
        let mut next = Box::pin(super::acquire(&limit));
        assert!(matches!(
            futures_util::poll!(next.as_mut()),
            std::task::Poll::Pending
        ));
        release.send(()).expect("computation is still waiting");
        let _permit = tokio::time::timeout(Duration::from_secs(5), next)
            .await
            .expect("capacity returns after computation");
    }

    #[tokio::test]
    async fn blocked_preparation_leaves_the_async_runtime_free_to_release_it() {
        let limit = Arc::new(tokio::sync::Semaphore::new(1));
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            let permit = super::acquire(&limit).await;
            super::run(permit, move || {
                entered.send(()).expect("test is waiting for preparation");
                released.recv_timeout(Duration::from_secs(5))
            })
            .await
        });
        tokio::time::timeout(Duration::from_secs(5), started)
            .await
            .expect("preparation starts")
            .expect("start signal");
        release.send(()).expect("preparation is still waiting");
        assert!(
            task.await
                .expect("async task")
                .expect("blocking task")
                .is_ok()
        );
    }
}
