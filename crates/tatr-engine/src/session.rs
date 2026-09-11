//! ONNX Runtime 会话：张量契约校验 + 线程配置 + 单次推理。

use std::path::Path;

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Value;

use tatr_core::{PreprocessedInput, TatrError};

/// 引擎错误（对外统一）。
#[derive(Debug, thiserror::Error)]
pub enum TatrEngineError {
    /// 核心层错误（图像/配置/输出形状）。
    #[error(transparent)]
    Core(#[from] TatrError),
    /// ONNX Runtime 会话创建失败。
    #[error("创建 ONNX Runtime 会话失败: {0}")]
    Session(String),
    /// 推理执行失败。
    #[error("推理失败: {0}")]
    Inference(String),
    /// 模型 IO 输入契约与预期不符。
    #[error("模型契约不符: {0}")]
    Contract(String),
}

/// 会话与运行时配置。
#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    /// intra-op 线程数。`None` 表示用物理核数；环境变量 `TATR_THREADS` 可覆盖。
    pub intra_threads: Option<usize>,
    /// 图优化级别。
    pub optimization_level: GraphOptimizationLevel,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            intra_threads: None,
            optimization_level: GraphOptimizationLevel::Level3,
        }
    }
}

impl SessionConfig {
    /// 解析生效的线程数：显式配置 > `TATR_THREADS` > 物理核数。
    #[must_use]
    pub fn resolve_threads(&self) -> usize {
        if let Some(n) = self.intra_threads.filter(|n| *n > 0) {
            return n;
        }
        if let Ok(v) = std::env::var("TATR_THREADS") {
            if let Ok(n) = v.trim().parse::<usize>() {
                if n > 0 {
                    return n;
                }
            }
        }
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    }
}

/// 一次推理的原始输出（已拷贝为连续缓冲）。
pub(crate) struct RawInference {
    pub logits: Vec<f32>,
    pub boxes: Vec<f32>,
    pub num_queries: usize,
    pub num_columns: usize,
}

/// 模型 IO 名称（与导出脚本固定）。
const IN_PIXELS: &str = "pixel_values";
const IN_MASK: &str = "pixel_mask";
const OUT_LOGITS: &str = "logits";
const OUT_BOXES: &str = "pred_boxes";

/// 已加载的 ONNX 会话。
pub struct InferenceSession {
    session: Session,
    threads: usize,
}

impl std::fmt::Debug for InferenceSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceSession")
            .field("threads", &self.threads)
            .finish()
    }
}

impl InferenceSession {
    /// 从模型文件创建会话，并**立即校验 IO 契约**（错误的模型应尽早在启动期失败）。
    pub fn load(model_path: &Path, cfg: &SessionConfig) -> Result<Self, TatrEngineError> {
        let threads = cfg.resolve_threads();
        let session = Session::builder()
            .map_err(|e| TatrEngineError::Session(e.to_string()))?
            .with_optimization_level(cfg.optimization_level)
            .map_err(|e| TatrEngineError::Session(e.to_string()))?
            .with_intra_threads(threads)
            .map_err(|e| TatrEngineError::Session(e.to_string()))?
            .commit_from_file(model_path)
            .map_err(|e| TatrEngineError::Session(format!("加载 {} 失败: {e}", model_path.display())))?;

        let inputs: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
        let outputs: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();
        for want in [IN_PIXELS, IN_MASK] {
            if !inputs.iter().any(|n| n == want) {
                return Err(TatrEngineError::Contract(format!(
                    "模型缺少输入 `{want}`（实际: {inputs:?}）"
                )));
            }
        }
        for want in [OUT_LOGITS, OUT_BOXES] {
            if !outputs.iter().any(|n| n == want) {
                return Err(TatrEngineError::Contract(format!(
                    "模型缺少输出 `{want}`（实际: {outputs:?}）"
                )));
            }
        }
        tracing::info!(threads, path = %model_path.display(), "ONNX 会话就绪");
        Ok(Self { session, threads })
    }

    /// 生效的 intra-op 线程数。
    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// 执行一次前向，返回原始 logits / boxes。
    pub(crate) fn run(&mut self, input: &PreprocessedInput) -> Result<RawInference, TatrEngineError> {
        let shape = input.pixel_shape();
        let pixels = ndarray::Array4::from_shape_vec((shape[0], shape[1], shape[2], shape[3]), input.pixels.clone())
            .map_err(|e| TatrEngineError::Contract(format!("像素张量形状不合法: {e}")))?;
        let mshape = input.mask_shape();
        let mask = ndarray::Array3::from_shape_vec((mshape[0], mshape[1], mshape[2]), input.mask.clone())
            .map_err(|e| TatrEngineError::Contract(format!("mask 张量形状不合法: {e}")))?;

        let px = Value::from_array(pixels).map_err(|e| TatrEngineError::Contract(e.to_string()))?;
        let mk = Value::from_array(mask).map_err(|e| TatrEngineError::Contract(e.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![IN_PIXELS => px, IN_MASK => mk])
            .map_err(|e| TatrEngineError::Inference(e.to_string()))?;

        let (logits_shape, logits) = outputs[OUT_LOGITS]
            .try_extract_tensor::<f32>()
            .map_err(|e| TatrEngineError::Contract(format!("logits 张量类型不符: {e}")))?;
        let (boxes_shape, boxes) = outputs[OUT_BOXES]
            .try_extract_tensor::<f32>()
            .map_err(|e| TatrEngineError::Contract(format!("pred_boxes 张量类型不符: {e}")))?;

        if logits_shape.len() != 3 || logits_shape[0] != 1 {
            return Err(TatrEngineError::Contract(format!(
                "logits 期望 [1,Q,C]，实际 {:?}",
                logits_shape
            )));
        }
        if boxes_shape.len() != 3 || boxes_shape[0] != 1 || boxes_shape[2] != 4 {
            return Err(TatrEngineError::Contract(format!(
                "pred_boxes 期望 [1,Q,4]，实际 {:?}",
                boxes_shape
            )));
        }
        let num_queries = logits_shape[1] as usize;
        let num_columns = logits_shape[2] as usize;
        if boxes_shape[1] as usize != num_queries {
            return Err(TatrEngineError::Contract(format!(
                "query 数不一致: logits {:?} vs boxes {:?}",
                logits_shape, boxes_shape
            )));
        }

        Ok(RawInference {
            logits: logits.to_vec(),
            boxes: boxes.to_vec(),
            num_queries,
            num_columns,
        })
    }
}
