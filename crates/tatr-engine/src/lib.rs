//! # tatr-engine
//!
//! 表格检测引擎：模型获取（本地 / 缓存 / URL+sha256）、ONNX Runtime 会话管理与检测 API。
//!
//! 面向两类消费者：
//!
//! 1. **上层集成（进程内）**——直接持有 [`TableDetectionEngine`]：
//!
//! ```no_run
//! use tatr_engine::{EngineOptions, TableDetectionEngine};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = TableDetectionEngine::new(EngineOptions::default())?;
//! let img = tatr_core::RasterImage::from_rgb(2, 2, vec![255u8; 12])?;
//! let result = engine.detect(&img, &tatr_core::DetectorConfig::default())?;
//! println!("{} tables", result.detections.len());
//! # Ok(()) }
//! ```
//!
//! 2. **独立 HTTP 服务**——见 `apps/tatr-http`。
//!
//! ## CPU 部署
//!
//! 引擎默认按物理核数设置 ONNX Runtime intra-op 线程（可用 `TATR_THREADS` 覆盖）。
//! 实测 Apple M5 Pro：6 线程即饱和（153ms/页 @800px），单进程吞吐约 6 页/秒；
//! 更高吞吐请多进程/多实例水平扩展。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod engine;
mod model;
mod session;

pub use engine::{decode_image_bytes, decode_image_file, DetectionResult, EngineOptions, TableDetectionEngine};
pub use model::{file_sha256 as model_sha256, ModelSource, DEFAULT_MODEL_SHA256, DEFAULT_MODEL_URL};
pub use session::{SessionConfig, TatrEngineError};

/// 便捷重导出：核心类型。
pub use tatr_core as core;
