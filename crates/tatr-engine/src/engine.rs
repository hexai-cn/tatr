//! 引擎门面：模型解析 → 会话 → 检测 API。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use tatr_core::{
    decode::decode_detections, nms::nms, preprocess::preprocess, Detection, DetectorConfig, RasterImage, TatrError,
};

use crate::model::ModelSource;
use crate::session::{InferenceSession, SessionConfig, TatrEngineError};

/// 引擎构造选项。
#[derive(Debug, Clone, Default)]
pub struct EngineOptions {
    /// 模型来源；默认从 GitHub Release 下载并校验 sha256。
    pub model: ModelSource,
    /// ONNX Runtime 会话配置。
    pub session: SessionConfig,
    /// 首次检测时是否应用 `TATR_NMS_IOU` 环境变量覆盖配置。
    pub apply_env_overrides: bool,
}

/// 单次检测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionResult {
    /// 图像宽（像素）。
    pub width: u32,
    /// 图像高（像素）。
    pub height: u32,
    /// 模型输入的缩放后尺寸 `(w, h)`。
    pub input_size: (u32, u32),
    /// 检测框（原图像素，已按分数降序）。
    pub detections: Vec<Detection>,
    /// 端到端耗时（毫秒，含预处理与解码）。
    pub elapsed_ms: f64,
}

impl DetectionResult {
    /// 仅保留正向表格之外的旋转表格数量（可观测性用）。
    #[must_use]
    pub fn rotated_count(&self) -> usize {
        self.detections
            .iter()
            .filter(|d| d.label == tatr_core::Label::TableRotated)
            .count()
    }
}

/// 表格检测引擎。
///
/// `ort::Session` 需要 `&mut` 执行，故内部用 `Mutex` 串行化；
/// CPU 场景下这是期望行为（避免线程超订），多实例扩展由上层负责。
pub struct TableDetectionEngine {
    session: Mutex<InferenceSession>,
    model_path: PathBuf,
    options: EngineOptions,
}

impl std::fmt::Debug for TableDetectionEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TableDetectionEngine")
            .field("model_path", &self.model_path)
            .field("threads", &self.session.lock().map(|s| s.threads()).unwrap_or(0))
            .finish()
    }
}

impl TableDetectionEngine {
    /// 解析模型并创建引擎（**启动期即失败**：模型缺失/损坏/契约不符都会在此报错）。
    pub fn new(options: EngineOptions) -> Result<Self, TatrEngineError> {
        let model_path = options.model.resolve()?;
        let session = InferenceSession::load(&model_path, &options.session)?;
        Ok(Self {
            session: Mutex::new(session),
            model_path,
            options,
        })
    }

    /// 当前使用的模型文件路径。
    #[must_use]
    pub fn model_path(&self) -> &std::path::Path {
        &self.model_path
    }

    /// 生效的 intra-op 线程数。
    #[must_use]
    pub fn threads(&self) -> usize {
        self.session.lock().map(|s| s.threads()).unwrap_or(0)
    }

    /// 检测一张图。
    pub fn detect(&self, image: &RasterImage, cfg: &DetectorConfig) -> Result<DetectionResult, TatrEngineError> {
        let cfg = self.effective_config(cfg);
        cfg.validate()?;

        let started = Instant::now();
        let input = preprocess(image, cfg.short_side, cfg.long_side)?;
        let input_size = (input.width, input.height);

        let raw = {
            let mut guard = self
                .session
                .lock()
                .map_err(|_| TatrEngineError::Inference("会话互斥锁中毒".into()))?;
            guard.run(&input)?
        };

        let outcome = decode_detections(
            &tatr_core::RawOutputs {
                logits: &raw.logits,
                boxes: &raw.boxes,
                num_queries: raw.num_queries,
                num_columns: raw.num_columns,
                image_width: image.width,
                image_height: image.height,
            },
            &cfg,
        )?;

        let mut detections = outcome.detections;
        nms(&mut detections, cfg.nms_iou);

        tracing::debug!(
            queries = outcome.stats.queries,
            low_score = outcome.stats.dropped_low_score,
            rotated = outcome.stats.dropped_rotated,
            degenerate = outcome.stats.dropped_degenerate_box,
            kept = detections.len(),
            "解码完成"
        );

        Ok(DetectionResult {
            width: image.width,
            height: image.height,
            input_size,
            detections,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// 便捷入口：从编码字节（PNG/JPEG/WebP/BMP/TIFF）解码后检测。
    pub fn detect_bytes(&self, bytes: &[u8], cfg: &DetectorConfig) -> Result<DetectionResult, TatrEngineError> {
        let img = decode_image_bytes(bytes)?;
        self.detect(&img, cfg)
    }

    fn effective_config(&self, cfg: &DetectorConfig) -> DetectorConfig {
        let mut out = *cfg;
        if self.options.apply_env_overrides {
            if let Ok(v) = std::env::var("TATR_NMS_IOU") {
                if let Ok(f) = v.trim().parse::<f32>() {
                    out.nms_iou = f;
                }
            }
        }
        out
    }
}

/// 把编码图像字节解码为 [`RasterImage`]（RGB8）。
pub fn decode_image_bytes(bytes: &[u8]) -> Result<RasterImage, TatrError> {
    let dynimg = image::load_from_memory(bytes).map_err(|e| TatrError::InvalidImage(format!("解码图像失败: {e}")))?;
    let rgb = dynimg.to_rgb8();
    let (w, h) = (rgb.width(), rgb.height());
    RasterImage::from_rgb(w, h, rgb.into_raw())
}

/// 从文件读取并解码为 [`RasterImage`]。
pub fn decode_image_file(path: &std::path::Path) -> Result<RasterImage, TatrError> {
    let bytes =
        std::fs::read(path).map_err(|e| TatrError::InvalidImage(format!("读取文件失败 {}: {e}", path.display())))?;
    decode_image_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_reports_missing_local_model_at_construction() {
        let opts = EngineOptions {
            model: ModelSource::LocalFile(PathBuf::from("/nonexistent/model.onnx")),
            ..Default::default()
        };
        let err = TableDetectionEngine::new(opts).unwrap_err();
        assert!(matches!(err, TatrEngineError::Core(TatrError::InvalidConfig(_))));
    }

    #[test]
    fn detect_bytes_rejects_garbage_input() {
        let err = decode_image_bytes(b"not an image").unwrap_err();
        assert!(matches!(err, TatrError::InvalidImage(_)));
    }

    #[test]
    fn session_config_env_override_precedence() {
        // 显式配置优先于环境变量
        let cfg = SessionConfig {
            intra_threads: Some(3),
            ..Default::default()
        };
        std::env::set_var("TATR_THREADS", "7");
        assert_eq!(cfg.resolve_threads(), 3);
        let auto = SessionConfig::default();
        assert_eq!(auto.resolve_threads(), 7);
        std::env::remove_var("TATR_THREADS");
    }
}
