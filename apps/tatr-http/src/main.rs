//! # tatr-http — 表格检测 HTTP 服务
//!
//! 面向 CPU 部署的单进程服务；吞吐扩展依靠多实例（见 `docs/runbooks/`）。
//!
//! ## 端点
//!
//! | 方法 | 路径 | 说明 |
//! |---|---|---|
//! | GET | `/healthz` | 存活探针 |
//! | GET | `/readyz` | 就绪探针（引擎已加载） |
//! | GET | `/v1/model` | 模型与配置信息 |
//! | POST | `/v1/detect` | 请求体为图像字节 |
//! | POST | `/v1/detect/multipart` | `multipart/form-data`，字段名 `file` |
//!
//! 查询参数（两者通用）：`threshold`、`short_side`、`long_side`、`drop_rotated`、`nms_iou`。
//!
//! ## 环境变量
//!
//! - `TATR_MODEL`：本地模型路径（省略则用缓存/远程下载）
//! - `TATR_BIND`：监听地址，默认 `0.0.0.0:8080`
//! - `TATR_THREADS`：ONNX intra-op 线程数（默认物理核数）
//! - `TATR_MAX_CONCURRENCY`：最大并发请求数，默认 4
//! - `RUST_LOG`：日志过滤，默认 `tatr_http=info,tower_http=info`

mod api;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use api::{router, AppState};
use tatr_core::DetectorConfig;
use tatr_engine::{EngineOptions, ModelSource, TableDetectionEngine};
use tokio::sync::Semaphore;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let bind = std::env::var("TATR_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let max_concurrency = std::env::var("TATR_MAX_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(4);

    let model = match std::env::var("TATR_MODEL") {
        Ok(p) if !p.trim().is_empty() => ModelSource::LocalFile(PathBuf::from(p)),
        _ => ModelSource::default(),
    };
    let options = EngineOptions {
        model,
        ..Default::default()
    };
    let engine = TableDetectionEngine::new(options).context("初始化检测引擎失败")?;

    let cfg = DetectorConfig::default();
    cfg.validate().context("默认配置非法")?;

    tracing::info!(
        model = %engine.model_path().display(),
        threads = engine.threads(),
        max_concurrency,
        bind = %bind,
        "tatr-http 启动"
    );

    let state = Arc::new(AppState {
        engine,
        default_config: cfg,
        gate: Semaphore::new(max_concurrency),
    });
    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("绑定 {bind} 失败"))?;
    tracing::info!("监听 {bind}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP 服务异常退出")?;
    tracing::info!("已优雅退出");
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| "tatr_http=info,tower_http=info,tatr_engine=info".into());
    fmt().with_env_filter(filter).with_target(false).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("安装 Ctrl-C 处理器失败");
    };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("安装 SIGTERM 处理器失败")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("收到 Ctrl-C，开始优雅退出"),
        _ = term => tracing::info!("收到 SIGTERM，开始优雅退出"),
        _ = tokio::time::sleep(Duration::from_secs(u64::MAX)) => {}
    }
}

/// 供测试复用的服务装配（不绑定端口）。
pub fn build_app(engine: TableDetectionEngine, cfg: DetectorConfig, max_concurrency: usize) -> axum::Router {
    router(Arc::new(AppState {
        engine,
        default_config: cfg,
        gate: Semaphore::new(max_concurrency),
    }))
}
