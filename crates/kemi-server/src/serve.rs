//! 起動の段取り（R-SERVE）。起動時のレビューとセッションの状態を決め、状態を組み立て、
//! レビュー画面・エージェント用の API・`--live` の中継を動かして、止まり方を結果にする。

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};

use kemi_core::domain::agent::AgentLink;
use kemi_core::domain::display_version::{DisplayVersions, Start, known_generation};
use kemi_core::domain::explanation::ExplanationDocument;
use kemi_core::source::ReviewSource;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch as shutdown_watch};

use crate::api::{self, AllowedHosts};
use crate::session::Session;
use crate::{
    AgentChannel, AppState, PersistGuarded, ServeOutcome, ServeParams, ServerError, Stop, Stopping,
    SubmitState, explanation, freeze, live, units, watch,
};

type ServerFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send>>;

/// 停止の合図（R-SUBMIT）を待つ future。
fn stopping(state: &AppState) -> impl std::future::Future<Output = ()> + use<> {
    let mut receiver = state.stopping.shutdown.subscribe();
    async move {
        loop {
            if *receiver.borrow() {
                break;
            }
            if receiver.changed().await.is_err() {
                break;
            }
        }
    }
}

/// 起動時に取得したレビュー。
struct BootstrapReview {
    initial_source: Arc<dyn ReviewSource>,
    review: kemi_core::domain::review::ReviewMeta,
    units: Vec<kemi_core::domain::review::GroupBy>,
    resumed: bool,
    code_error: Option<String>,
}

/// 復元を反映した、起動時のセッションと単位の状態。
struct InitialSessionState {
    initial_source: Arc<dyn ReviewSource>,
    initial_state: kemi_core::session::SessionState,
    display_id: String,
    review_state: units::ReviewState,
    initial_review: Option<freeze::FreezeInput>,
}

pub async fn serve(
    listener: TcpListener,
    mut params: ServeParams,
) -> Result<ServeOutcome, ServerError> {
    let address = listener.local_addr().map_err(ServerError::Io)?;
    let bind = match address.ip() {
        IpAddr::V4(bind) => bind,
        IpAddr::V6(_) => {
            return Err(ServerError::Stopped(format!(
                "{address} is not an IPv4 listener"
            )));
        }
    };
    let allowed = AllowedHosts::new(bind, params.share_address, address.port());

    let initial_explanation = params.initial_explanation.take();
    let bootstrap = bootstrap_review(&params)?;
    let initial = initial_session_state(&params, bootstrap, initial_explanation)?;
    run_servers(listener, params, address, allowed, initial).await
}

/// ソースを分けて起動時のレビューを取得する。復元した `--live` では取得の誤りを許す。
fn bootstrap_review(params: &ServeParams) -> Result<BootstrapReview, ServerError> {
    let resumed = params
        .session
        .as_ref()
        .is_some_and(|sink| sink.is_resumed());
    let tolerate = resumed && params.live.is_some();
    let (initial_source, read) = match params.source.fork() {
        Ok(forked) => {
            let source = forked.unwrap_or_else(|| params.source.clone());
            let read = source.review();
            (source, read)
        }
        Err(error) => (params.source.clone(), Err(error)),
    };
    let (review, code_error) = match read {
        Ok(review) => (review, None),
        Err(error) if tolerate => (code_unavailable_review(params), Some(error.to_string())),
        Err(error) => return Err(ServerError::Source(error)),
    };
    // セッションの情報に、起動時の単位の題と全ファイル数を残す（R-SESSION）。
    if let Some(sink) = &params.session
        && code_error.is_none()
    {
        let total_files = review.groups.iter().map(|group| group.files.len()).sum();
        sink.describe_review(&review.title, total_files);
    }
    let units = initial_source.units();
    Ok(BootstrapReview {
        initial_source,
        review,
        units,
        resumed,
        code_error,
    })
}

/// コードを読めない復元した `--live` のレビュー。ファイルは無く、題はセッションに残した題。
fn code_unavailable_review(params: &ServeParams) -> kemi_core::domain::review::ReviewMeta {
    kemi_core::domain::review::ReviewMeta {
        title: params
            .session
            .as_ref()
            .and_then(|sink| sink.initial_title())
            .unwrap_or_else(|| "Live review".into()),
        subtitle: String::new(),
        meta: serde_json::Value::Null,
        groups: vec![],
        approval: vec![],
    }
}

/// 表示版・初期の解説・世代・凍結の入力を決める。
fn initial_session_state(
    params: &ServeParams,
    bootstrap: BootstrapReview,
    initial_explanation: Option<ExplanationDocument>,
) -> Result<InitialSessionState, ServerError> {
    let BootstrapReview {
        initial_source,
        review,
        units,
        resumed,
        code_error,
    } = bootstrap;
    // 復元では、セッションの状態から見た・折りたたみ・コメント・解決を読み戻す（R-SESSION）。
    let mut initial_state = params
        .session
        .as_ref()
        .map(|sink| sink.initial_state())
        .unwrap_or_default();
    let display_id = params
        .agent
        .as_ref()
        .map(|agent| agent.review.clone())
        .or_else(|| params.session.as_ref().and_then(|sink| sink.review_id()))
        .or_else(|| {
            initial_state
                .display
                .as_ref()
                .and_then(|display| display.current.split_once(':').map(|(id, _)| id.to_owned()))
        })
        .unwrap_or_else(kemi_core::session::new_ulid);
    let start = if resumed {
        Start::Resumed {
            rereads: params.live.is_some() && code_error.is_none(),
        }
    } else {
        Start::Fresh
    };
    let display = DisplayVersions::at_start(initial_state.display.take(), &display_id, start)
        .map_err(|e| ServerError::Stopped(e.to_string()))?;
    initial_state.display = Some(display.clone());
    if let Some(document) = initial_explanation {
        if params.agent.is_none() {
            return Err(ServerError::Stopped(
                "initial explanation requires an agent endpoint".into(),
            ));
        }
        let Some(sink) = &params.session else {
            return Err(ServerError::Stopped(
                "initial explanation requires a persistent session".into(),
            ));
        };
        let pages: Vec<_> = params
            .live
            .as_ref()
            .map_or_else(Vec::new, |live| vec![live.target.start()]);
        initial_state.explanation = Some(explanation::initial_history(
            document,
            initial_source.as_ref(),
            &review,
            &units,
            &display,
            &display_id,
            &pages,
        )?);
        kemi_core::session::validate_additional(&initial_state).map_err(ServerError::Session)?;
        sink.save_explanation_state(initial_state.clone())
            .map_err(ServerError::Session)?;
    }
    let generation = known_generation(&display.current, &display_id, display.issued)
        .map_err(|e| ServerError::Stopped(e.to_string()))?;
    let mut review_state = units::ReviewState::new(&units, review);
    review_state.code_error = code_error;
    if let Some(other) = &mut review_state.other {
        other.generation = generation;
    }
    let initial_review = params
        .session
        .as_ref()
        .filter(|sink| sink.needs_copy())
        .map(|_| freeze::FreezeInput {
            source: initial_source.clone(),
            startup_unit: review_state.startup_unit,
            startup: review_state.startup.clone(),
            other: review_state.other.clone(),
            generation,
            token: display.current,
        });
    Ok(InitialSessionState {
        initial_source,
        initial_state,
        display_id,
        review_state,
        initial_review,
    })
}

/// 状態を組み立て、監視とサーバを動かし、止まり方を結果にする。
async fn run_servers(
    listener: TcpListener,
    params: ServeParams,
    address: SocketAddr,
    allowed: AllowedHosts,
    initial: InitialSessionState,
) -> Result<ServeOutcome, ServerError> {
    let InitialSessionState {
        initial_source,
        initial_state,
        display_id,
        review_state,
        initial_review,
    } = initial;
    let sources = Arc::new(kemi_core::source::SourceSlot::new(
        params.source,
        initial_source,
    ));
    // エージェントの返信は短い間に続けて届くことがあるので、取りこぼしにくい長さにする。
    let (events, _) = broadcast::channel(256);
    let (shutdown, _) = shutdown_watch::channel(false);
    let live = match params.live {
        Some(live) => {
            let (listener, info, target) = live::prepare(live).map_err(ServerError::Io)?;
            Some((listener, Arc::new(info), target))
        }
        None => None,
    };

    let agent = AgentLink::new((params.clock)(), &initial_state.channel.events);
    let state = Arc::new(AppState {
        sources,
        initial_review: Mutex::new(initial_review),
        display_id,
        display_ready: std::sync::atomic::AtomicBool::new(false),
        highlighter: std::sync::OnceLock::new(),
        preparation: Arc::new(tokio::sync::Semaphore::new(
            std::thread::available_parallelism().map_or(1, |count| count.get().min(4)),
        )),
        assets: params.assets,
        results: params.results,
        session_sink: params.session,
        notices: params.notices,
        token: params.token,
        allowed,
        review: RwLock::new(review_state),
        refresh: tokio::sync::Mutex::new(()),
        session: Mutex::new(Session::from_state(initial_state)),
        persist: Mutex::new(PersistGuarded {
            registered_pages: std::collections::BTreeSet::new(),
            explanation_holds: std::collections::BTreeMap::new(),
        }),
        events,
        agent: AgentChannel {
            link: Mutex::new(agent),
            token: params.agent.as_ref().map(|agent| agent.token.clone()),
            wake: tokio::sync::Notify::new(),
        },
        stopping: Stopping {
            shutdown,
            stop: Mutex::new(None),
            submit_state: Mutex::new(SubmitState::Open),
        },
        freeze_started: std::sync::atomic::AtomicBool::new(false),
        live: live.as_ref().map(|(_, info, _)| info.clone()),
        clock: params.clock,
    });

    if let Some(agent) = &params.agent {
        let _ = agent.control.state.set(Arc::downgrade(&state));
        agent.control.ready.notify_waiters();
    }
    let _watch = watch::start(
        state.sources.watch_paths(),
        state.sources.work_tree(),
        state.events.clone(),
        state.notices.clone(),
    );

    let app = api::router(state.clone());
    let page = axum::serve(listener, app).with_graceful_shutdown(stopping(&state));
    let mut servers: Vec<ServerFuture> = vec![Box::pin(page.into_future())];
    if let Some(agent) = params.agent {
        api::start_status_ticker(&state);
        let agent_app =
            api::agent_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>();
        // graceful shutdown は処理中の接続が閉じるまで待つ。待っている kemi wait は
        // submit の結果を返し終えてから閉じる（R-AGENT-CLI）。
        let agent = axum::serve(agent.listener, agent_app).with_graceful_shutdown(stopping(&state));
        servers.push(Box::pin(agent.into_future()));
    }
    if let Some((live_listener, info, target)) = live {
        let live_state = Arc::new(live::LiveState::new(
            info,
            target,
            state.assets.clone(),
            address.port(),
            state.notices.clone(),
        ));
        let relay = axum::serve(live_listener, live::router(live_state));
        let stop = stopping(&state);
        // 中継は開発サーバの長く続く応答（SSE など）を抱えうるので、graceful shutdown で
        // 接続が閉じるのを待たない。止まる合図で受け付けをやめるだけにする。
        servers.push(Box::pin(async move {
            tokio::select! {
                result = relay.into_future() => result,
                () = stop => Ok(()),
            }
        }));
    }
    futures_util::future::try_join_all(servers)
        .await
        .map_err(ServerError::Io)?;

    let stop = state.stopping.stop.lock().expect("stop poisoned").take();
    match stop {
        Some(Stop::Submitted(document)) => Ok(ServeOutcome::Submitted(document)),
        Some(Stop::Failed(message)) => Err(ServerError::Stopped(message)),
        Some(Stop::Suspended) => Err(ServerError::Stopped("the review was suspended".to_string())),
        None => Err(ServerError::Stopped(
            "server stopped without a submit".to_string(),
        )),
    }
}
