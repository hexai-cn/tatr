//! 贪心非极大值抑制。
//!
//! DETR 每个 query 独立出框且已做 query 间去重，因此**默认不需要 NMS**
//! （`DetectorConfig::nms_iou >= 1.0` 即关闭）。本模块用于上游做多尺度/多模型
//! 融合时合并重复框。

use crate::types::Detection;

/// 就地执行贪心 NMS（按分数降序，保留与已选框 IoU 均小于 `iou_threshold` 的框）。
///
/// `iou_threshold >= 1.0` 时直接返回（等价于关闭）。
pub fn nms(detections: &mut Vec<Detection>, iou_threshold: f32) {
    if iou_threshold >= 1.0 || detections.len() <= 1 {
        return;
    }
    detections.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<Detection> = Vec::with_capacity(detections.len());
    'outer: for cand in detections.drain(..) {
        for k in &kept {
            if cand.bbox.iou(&k.bbox) >= iou_threshold {
                continue 'outer;
            }
        }
        kept.push(cand);
    }
    *detections = kept;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BBox, Label};

    fn det(x: f32, y: f32, w: f32, h: f32, score: f32) -> Detection {
        Detection {
            bbox: BBox { x, y, w, h },
            score,
            label: Label::Table,
        }
    }

    #[test]
    fn removes_heavily_overlapping_duplicate_keeping_higher_score() {
        let mut ds = vec![det(0.0, 0.0, 10.0, 10.0, 0.9), det(1.0, 1.0, 10.0, 10.0, 0.8)];
        nms(&mut ds, 0.5);
        assert_eq!(ds.len(), 1);
        assert!((ds[0].score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn keeps_disjoint_boxes() {
        let mut ds = vec![det(0.0, 0.0, 10.0, 10.0, 0.9), det(50.0, 50.0, 10.0, 10.0, 0.8)];
        nms(&mut ds, 0.5);
        assert_eq!(ds.len(), 2);
    }

    #[test]
    fn threshold_at_or_above_one_is_a_noop() {
        let mut ds = vec![det(0.0, 0.0, 10.0, 10.0, 0.9), det(0.0, 0.0, 10.0, 10.0, 0.8)];
        nms(&mut ds, 1.0);
        assert_eq!(ds.len(), 2, "IoU 阈值 1.0 表示关闭 NMS");
    }
}
