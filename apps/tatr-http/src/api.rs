//! HTTP API 路由与状态。

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tatr_core::DetectorConfig;
use tatr_engine::{TableDetectionEngine, TatrEngineError};
use tokio::sync::Semaphore;
use tower_http::trace::TraceLayer;

/// 单请求体上限（PDF 渲染页远小于此值）。
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// 共享状态。
pub struct AppState {
    /// 检测引擎。
    pub engine: TableDetectionEngine,
    /// 服务默认配置（请求可覆盖部分字段）。
    pub default_config: DetectorConfig,
    /// 并发闸门：ONNX 会话内部串行，这里限制排队长度避免内存无界增长。
    pub gate: Semaphore,
}

/// 健康检查响应。
#[derive(Serialize)]
struct Health {
    status: &'static str,
    model: String,
    threads: usize,
}

/// 模型信息响应。
#[derive(Serialize)]
struct ModelInfo {
    path: String,
    threads: usize,
    default_config: DetectorConfig,
    num_queries: usize,
    num_columns: usize,
}

/// 检测请求的可覆盖参数（查询串）。
#[derive(Debug, Deserialize)]
struct DetectParams {
    threshold: Option<f32>,
    short_side: Option<u32>,
    long_side: Option<u32>,
    drop_rotated: Option<bool>,
    nms_iou: Option<f32>,
}

impl DetectParams {
    fn apply(&self, base: &DetectorConfig) -> Result<DetectorConfig, ApiError> {
        let mut c = *base;
        if let Some(v) = self.threshold {
            c.threshold = v;
        }
        if let Some(v) = self.short_side {
            c.short_side = v;
        }
        if let Some(v) = self.long_side {
            c.long_side = v;
        }
        if let Some(v) = self.drop_rotated {
            c.include_rotated = !v;
        }
        if let Some(v) = self.nms_iou {
            c.nms_iou = v;
        }
        c.validate().map_err(ApiError::from)?;
        Ok(c)
    }
}

/// 错误响应体。
#[derive(Serialize)]
struct ErrorBody {
    error: String,
    detail: String,
}

/// API 错误。
pub struct ApiError {
    status: StatusCode,
    message: String,
    detail: String,
}

impl ApiError {
    fn bad_request(msg: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
            detail: detail.into(),
        }
    }

    fn unavailable(msg: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: msg.into(),
            detail: detail.into(),
        }
    }
}

impl From<tatr_core::TatrError> for ApiError {
    fn from(e: tatr_core::TatrError) -> Self {
        match e {
            tatr_core::TatrError::InvalidImage(d) => Self::bad_request("invalid_image", d),
            other => Self::bad_request("invalid_input", other.to_string()),
        }
    }
}

impl From<TatrEngineError> for ApiError {
    fn from(e: TatrEngineError) -> Self {
        match e {
            TatrEngineError::Core(c) => Self::from(c),
            other => Self::unavailable("inference_failed", other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: self.message,
            detail: self.detail,
        };
        (self.status, Json(body)).into_response()
    }
}

/// 构建路由。
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/model", get(model_info))
        .route("/v1/detect", post(detect_raw))
        .route("/v1/detect/multipart", post(detect_multipart))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz(State(st): State<Arc<AppState>>) -> Json<Health> {
    Json(Health {
        status: "ok",
        model: st.engine.model_path().display().to_string(),
        threads: st.engine.threads(),
    })
}

async fn readyz(State(st): State<Arc<AppState>>) -> Json<Health> {
    // 引擎在构造期已完成模型解析与会话建立，能构造出 State 即为就绪。
    Json(Health {
        status: "ready",
        model: st.engine.model_path().display().to_string(),
        threads: st.engine.threads(),
    })
}

async fn model_info(State(st): State<Arc<AppState>>) -> Json<ModelInfo> {
    Json(ModelInfo {
        path: st.engine.model_path().display().to_string(),
        threads: st.engine.threads(),
        default_config: st.default_config,
        num_queries: tatr_core::NUM_QUERIES,
        num_columns: tatr_core::NUM_CLASS_COLUMNS,
    })
}

/// `POST /v1/detect`：请求体直接是图像字节（PNG/JPEG/...）。
async fn detect_raw(
    State(st): State<Arc<AppState>>,
    Query(params): Query<DetectParams>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    if body.is_empty() {
        return Err(ApiError::bad_request("empty_body", "请求体为空"));
    }
    let cfg = params.apply(&st.default_config)?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let started = Instant::now();
    let result = run_detection(&st, &body, &cfg).await?;
    Ok(json_response(result, started, content_type))
}

/// `POST /v1/detect/multipart`：`multipart/form-data`，字段名 `file`。
async fn detect_multipart(
    State(st): State<Arc<AppState>>,
    Query(params): Query<DetectParams>,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let cfg = params.apply(&st.default_config)?;
    let mut payload: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request("bad_multipart", e.to_string()))?
    {
        if field.name().unwrap_or("") != "file" {
            continue;
        }
        let name = field.file_name().unwrap_or("upload").to_string();
        let data = field
            .bytes()
            .await
            .map_err(|e| ApiError::bad_request("bad_multipart", e.to_string()))?;
        tracing::debug!(file = %name, bytes = data.len(), "收到上传");
        payload = Some(data.to_vec());
        break;
    }
    let bytes = payload.ok_or_else(|| ApiError::bad_request("missing_file", "缺少 multipart 字段 `file`"))?;
    let started = Instant::now();
    let result = run_detection(&st, &bytes, &cfg).await?;
    Ok(json_response(result, started, "multipart/form-data".into()))
}

async fn run_detection(st: &AppState, bytes: &[u8], cfg: &DetectorConfig) -> Result<serde_json::Value, ApiError> {
    let _permit = st
        .gate
        .acquire()
        .await
        .map_err(|_| ApiError::unavailable("overloaded", "服务正在关闭"))?;
    let img = tatr_engine::decode_image_bytes(bytes)?;
    let result = st.engine.detect(&img, cfg)?;
    Ok(serde_json::json!({
        "width": result.width,
        "height": result.height,
        "input_size": [result.input_size.0, result.input_size.1],
        "elapsed_ms": (result.elapsed_ms * 100.0).round() / 100.0,
        "detections": result.detections,
    }))
}

fn json_response(value: serde_json::Value, started: Instant, source: String) -> Response {
    let envelope = serde_json::json!({
        "source_content_type": source,
        "total_ms": (started.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        "result": value,
    });
    (StatusCode::OK, Json(envelope)).into_response()
}
