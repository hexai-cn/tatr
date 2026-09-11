//! 公共类型：输入图像、检测结果、配置与错误。

use serde::{Deserialize, Serialize};

/// 轴对齐包围盒，坐标为**原图像素**，原点在左上角。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    /// 左上角 x。
    pub x: f32,
    /// 左上角 y。
    pub y: f32,
    /// 宽度（> 0）。
    pub w: f32,
    /// 高度（> 0）。
    pub h: f32,
}

impl BBox {
    /// 面积。
    #[must_use]
    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    /// 与另一个框的交并比。
    #[must_use]
    pub fn iou(&self, other: &Self) -> f32 {
        let ix = (self.x + self.w).min(other.x + other.w) - self.x.max(other.x);
        let iy = (self.y + self.h).min(other.y + other.h) - self.y.max(other.y);
        let inter = ix.max(0.0) * iy.max(0.0);
        let union = self.area() + other.area() - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    /// 裁剪到 `[0,w] × [0,h]` 页面内；退化（宽或高 ≤ 0）时返回 `None`。
    #[must_use]
    pub fn clamp_to(&self, page_w: f32, page_h: f32) -> Option<Self> {
        let x = self.x.max(0.0);
        let y = self.y.max(0.0);
        let w = (self.w.min(page_w - x)).max(0.0);
        let h = (self.h.min(page_h - y)).max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(Self { x, y, w, h })
    }
}

/// 检测类别。模型只区分正向表格与旋转表格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    /// 正向表格（类别 0）。
    Table,
    /// 旋转表格（类别 1）。
    TableRotated,
}

impl Label {
    /// 对应模型输出的列索引。
    #[must_use]
    pub fn column(self) -> usize {
        match self {
            Self::Table => 0,
            Self::TableRotated => 1,
        }
    }
}

/// 单个检测结果。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    /// 框（原图像素）。
    pub bbox: BBox,
    /// 置信度，`0..=1`（含 no-object 的 softmax 概率）。
    pub score: f32,
    /// 类别。
    pub label: Label,
}

/// 待检测图像：紧打包的 RGB8 像素。
#[derive(Debug, Clone)]
pub struct RasterImage {
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
    /// `width * height * 3` 字节、行优先、RGB 顺序。
    pub rgb: Vec<u8>,
}

impl RasterImage {
    /// 由 RGB8 缓冲构造，校验长度。
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Result<Self, TatrError> {
        let want = width as usize * height as usize * 3;
        if width == 0 || height == 0 {
            return Err(TatrError::InvalidImage("尺寸为 0".into()));
        }
        if rgb.len() != want {
            return Err(TatrError::InvalidImage(format!(
                "RGB 缓冲长度 {} 与 {}x{}x3={} 不符",
                rgb.len(),
                width,
                height,
                want
            )));
        }
        Ok(Self { width, height, rgb })
    }

    /// 页面宽（f32 便捷访问）。
    #[must_use]
    pub fn width_f(&self) -> f32 {
        self.width as f32
    }

    /// 页面高（f32 便捷访问）。
    #[must_use]
    pub fn height_f(&self) -> f32 {
        self.height as f32
    }
}

/// 检测配置。默认值即生产推荐值（对齐模型训练时的预处理配置）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DetectorConfig {
    /// 分数阈值（含 no-object 的 softmax 概率）。
    pub threshold: f32,
    /// 缩放后**短边**目标像素。
    pub short_side: u32,
    /// 缩放后**长边**上限像素。
    pub long_side: u32,
    /// 是否输出 `table rotated` 类。
    ///
    /// 实测：开启后开发集 F1 0.780→0.786 且**误检数不变**——该类是高频繁误判，
    /// 不是罕见角落，生产建议开启。
    pub include_rotated: bool,
    /// NMS IoU 阈值；`>= 1.0` 表示关闭（DETR 每 query 独立出框，默认无需 NMS）。
    pub nms_iou: f32,
    /// 小于此像素边长的框被丢弃（过滤退化框）。
    pub min_box_side: f32,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            short_side: 800,
            long_side: 800,
            include_rotated: true,
            nms_iou: 1.0,
            min_box_side: 1.0,
        }
    }
}

impl DetectorConfig {
    /// 校验配置合法性。
    pub fn validate(&self) -> Result<(), TatrError> {
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(TatrError::InvalidConfig("threshold 必须落在 0..=1".into()));
        }
        if self.short_side == 0 || self.long_side == 0 {
            return Err(TatrError::InvalidConfig("short_side/long_side 必须 > 0".into()));
        }
        if self.long_side < self.short_side {
            return Err(TatrError::InvalidConfig(
                "long_side 必须 >= short_side（否则短边缩放无意义）".into(),
            ));
        }
        Ok(())
    }
}

/// 本 crate 与上层统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum TatrError {
    /// 输入图像非法（尺寸为 0、缓冲长度不符等）。
    #[error("图像非法: {0}")]
    InvalidImage(String),
    /// 配置非法。
    #[error("配置非法: {0}")]
    InvalidConfig(String),
    /// 模型输出形状与约定不符。
    #[error("模型输出形状不符: {0}")]
    UnexpectedOutputShape(String),
}
