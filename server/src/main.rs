mod api;
mod auth;
mod config;
mod db;
mod error;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::db::Db;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tomatossh_sync=info,tower_http=warn".into()),
        )
        .init();

    let cfg = Config::from_env();
    let db = Db::open(&cfg.db_path).expect("无法打开数据库");

    tracing::info!(
        bind = %cfg.bind,
        db = %cfg.db_path,
        allow_register = cfg.allow_register,
        "TomatoSSH 同步服务启动中"
    );

    let state = api::AppState {
        db: Arc::new(db),
        cfg: Arc::new(cfg.clone()),
    };

    let app = Router::new()
        .route("/healthz", get(api::health))
        .route("/api/v1/auth/register", post(api::register))
        .route("/api/v1/auth/login", post(api::login))
        .route(
            "/api/v1/sync/vault",
            get(api::get_vault).put(api::put_vault),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .expect("无法监听端口");
    tracing::info!("监听 {}", cfg.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("收到退出信号，正在关闭");
        })
        .await
        .expect("服务异常退出");
}
