//! # tatr-core
//!
//! Table Transformer (DETR) 表格检测的**纯算法核心**：输入约定、预处理、输出解码与后处理。
//!
//! 本 crate 不依赖任何推理运行时（无 `ort`）、不做 IO，因此可以在没有模型的情况下完整单测。
//! 推理运行时与模型解析见 `tatr-engine`。
//!
//! ## 输入约定（与 microsoft/table-transformer-detection 训练配置一致）
//!
//! ```text
//! 输入  pixel_values  f32 [1,3,H,W]   RGB, /255, 再按 ImageNet mean/std 归一化
//!       pixel_mask    f32 [1,H,W]     全 1（DETR 的 padding mask）
//! 输出  logits        f32 [1,Q,C]     Q=15 个 query, C=num_labels+1（末列是 no-object）
//!       pred_boxes    f32 [1,Q,4]     归一化 cxcywh
//! ```
//!
//! ## 解码语义（易错点，已由逐图对拍固化）
//!
//! 分数必须用**含 no-object 列的 softmax**，再在真实类别子集上取 max/argmax
//! （HF `DetrImageProcessor.post_process_object_detection` 口径）。
//! **不是**按类独立 sigmoid —— 那会给出虚高分数并错选 query。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod decode;
pub mod nms;
pub mod preprocess;
pub mod types;

pub use decode::{decode_detections, DecodeOutcome, RawOutputs};
pub use nms::nms;
pub use preprocess::{preprocess, target_size, PreprocessedInput, IMAGENET_MEAN, IMAGENET_STD};
pub use types::{BBox, Detection, DetectorConfig, Label, RasterImage, TatrError};

/// Table Transformer 检测模型的 query 数量（`num_queries=15`）。
pub const NUM_QUERIES: usize = 15;

/// 后端输出的类别数：`table` / `table rotated` / `no-object`。
pub const NUM_CLASS_COLUMNS: usize = 3;
