//! DETR 输出解码：`logits`/`pred_boxes` → 像素坐标检测框。
//!
//! ## 必须遵守的语义
//!
//! 模型 `logits` 有 `num_labels + 1` 列，**最后一列是 no-object**。类别分数必须用
//! **包含 no-object 列在内**的 softmax，再在真实类别子集上取 max/argmax：
//!
//! ```text
//! p          = softmax(logits, dim = -1)      // 含 no-object 列
//! score,cls  = max(p[..., :-1], dim = -1)     // 只在真实类别里选
//! ```
//!
//! 这与 HF `DetrImageProcessor.post_process_object_detection` 一致。
//!
//! **反例（曾经的真实缺陷）**：按类独立 sigmoid 会得到虚高分数并错选 query
//! —— 例如表格 logit 4.1、no-object logit 4.9 时，sigmoid 给出 0.98（误检），
//! 而正确 softmax 只有约 0.31（正确抑制）。见本文件单元测试。

use crate::types::{BBox, Detection, DetectorConfig, Label, TatrError};

/// 后端原始输出（行优先连续缓冲）。
#[derive(Debug, Clone, Copy)]
pub struct RawOutputs<'a> {
    /// `[1, num_queries, num_columns]` 的类别 logit。
    pub logits: &'a [f32],
    /// `[1, num_queries, 4]` 的归一化 `cxcywh`。
    pub boxes: &'a [f32],
    /// query 数（模型固定 15）。
    pub num_queries: usize,
    /// 类别列数（`num_labels + 1`，本模型为 3）。
    pub num_columns: usize,
    /// 原图宽（用于把归一化框映射为像素）。
    pub image_width: u32,
    /// 原图高。
    pub image_height: u32,
}

/// 解码统计，用于可观测性与调参。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecodeStats {
    /// 参与解码的 query 总数。
    pub queries: usize,
    /// 因分数低于阈值被丢弃。
    pub dropped_low_score: usize,
    /// 因类别为旋转表格且配置关闭而被丢弃。
    pub dropped_rotated: usize,
    /// 因框退化（越界/过小）被丢弃。
    pub dropped_degenerate_box: usize,
    /// 通过全部过滤的检测数。
    pub kept: usize,
}

/// 解码结果。
#[derive(Debug, Clone)]
pub struct DecodeOutcome {
    /// 检测框（原图像素坐标）。
    pub detections: Vec<Detection>,
    /// 统计。
    pub stats: DecodeStats,
}

/// 把后端原始输出解码为检测框。
pub fn decode_detections(raw: &RawOutputs<'_>, cfg: &DetectorConfig) -> Result<DecodeOutcome, TatrError> {
    let q = raw.num_queries;
    let c = raw.num_columns;
    if c < 2 {
        return Err(TatrError::UnexpectedOutputShape(format!(
            "类别列数 {c} 至少为 2（真实类别 + no-object）"
        )));
    }
    if raw.logits.len() < q * c {
        return Err(TatrError::UnexpectedOutputShape(format!(
            "logits 长度 {} 小于 {}x{}",
            raw.logits.len(),
            q,
            c
        )));
    }
    if raw.boxes.len() < q * 4 {
        return Err(TatrError::UnexpectedOutputShape(format!(
            "boxes 长度 {} 小于 {}x4",
            raw.boxes.len(),
            q
        )));
    }

    let real_classes = c - 1; // 末列为 no-object
    let (pw, ph) = (raw.image_width as f32, raw.image_height as f32);
    let mut stats = DecodeStats {
        queries: q,
        ..Default::default()
    };
    let mut detections = Vec::new();

    for i in 0..q {
        let row = &raw.logits[i * c..(i + 1) * c];

        // softmax（含 no-object 列），数值稳定写法
        let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut sum = 0.0f32;
        for &v in row {
            sum += (v - max).exp();
        }
        if !(sum.is_finite() && sum > 0.0) {
            stats.dropped_low_score += 1;
            continue;
        }

        // 在真实类别子集上取 max/argmax
        let mut best_p = f32::NEG_INFINITY;
        let mut best_c = 0usize;
        for (k, &logit) in row.iter().take(real_classes).enumerate() {
            let p = (logit - max).exp() / sum;
            if p > best_p {
                best_p = p;
                best_c = k;
            }
        }

        if best_p < cfg.threshold {
            stats.dropped_low_score += 1;
            continue;
        }

        let label = match best_c {
            0 => Label::Table,
            1 => Label::TableRotated,
            other => {
                return Err(TatrError::UnexpectedOutputShape(format!(
                    "类别索引 {other} 超出已实现的 {{table, table_rotated}}"
                )))
            }
        };
        if label == Label::TableRotated && !cfg.include_rotated {
            stats.dropped_rotated += 1;
            continue;
        }

        // 归一化 cxcywh → 像素 xywh
        let (cx, cy, bw, bh) = (
            raw.boxes[i * 4],
            raw.boxes[i * 4 + 1],
            raw.boxes[i * 4 + 2],
            raw.boxes[i * 4 + 3],
        );
        let raw_box = BBox {
            x: (cx - bw / 2.0) * pw,
            y: (cy - bh / 2.0) * ph,
            w: bw * pw,
            h: bh * ph,
        };
        let Some(bbox) = raw_box.clamp_to(pw, ph) else {
            stats.dropped_degenerate_box += 1;
            continue;
        };
        if bbox.w < cfg.min_box_side || bbox.h < cfg.min_box_side {
            stats.dropped_degenerate_box += 1;
            continue;
        }

        detections.push(Detection {
            bbox,
            score: best_p,
            label,
        });
    }

    stats.kept = detections.len();
    Ok(DecodeOutcome { detections, stats })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NUM_QUERIES;

    /// 构造一个 query 的 logits：`[table, rotated, no_object]`。
    fn raw3(q: usize, rows: &[[f32; 3]], boxes: &[[f32; 4]]) -> (Vec<f32>, Vec<f32>) {
        let mut logits = vec![0f32; q * 3];
        for (i, r) in rows.iter().enumerate() {
            logits[i * 3..i * 3 + 3].copy_from_slice(r);
        }
        let mut b = vec![0f32; q * 4];
        for (i, r) in boxes.iter().enumerate() {
            b[i * 4..i * 4 + 4].copy_from_slice(r);
        }
        (logits, b)
    }

    fn cfg() -> DetectorConfig {
        DetectorConfig {
            threshold: 0.5,
            include_rotated: true,
            ..Default::default()
        }
    }

    #[test]
    fn decodes_table_box_to_pixel_coordinates() {
        // 唯一强表格 query：中心 (0.5,0.5)、尺寸 (0.5,0.25) → 像素 (50,75,100,50)
        let rows = [[6.0, -6.0, -6.0]];
        let boxes = [[0.5, 0.5, 0.5, 0.25]];
        let (logits, bx) = raw3(1, &rows, &boxes);
        let raw = RawOutputs {
            logits: &logits,
            boxes: &bx,
            num_queries: 1,
            num_columns: 3,
            image_width: 200,
            image_height: 200,
        };
        let out = decode_detections(&raw, &cfg()).unwrap();
        assert_eq!(out.detections.len(), 1);
        let d = out.detections[0];
        assert_eq!(d.label, Label::Table);
        assert!((d.bbox.x - 50.0).abs() < 1e-3, "x={}", d.bbox.x);
        assert!((d.bbox.y - 75.0).abs() < 1e-3, "y={}", d.bbox.y);
        assert!((d.bbox.w - 100.0).abs() < 1e-3, "w={}", d.bbox.w);
        assert!((d.bbox.h - 50.0).abs() < 1e-3, "h={}", d.bbox.h);
        assert!(d.score > 0.9, "score={}", d.score);
    }

    /// **回归测试（真实缺陷）**：no-object 占优时必须抑制。
    ///
    /// 若实现误用「按类独立 sigmoid」，sigmoid(4.1)=0.984 会产出误检；
    /// 正确的含 no-object softmax 只有约 0.31，必须被阈值滤掉。
    #[test]
    fn suppresses_query_where_no_object_dominates() {
        let rows = [[4.1, -8.0, 4.9]];
        let boxes = [[0.5, 0.5, 0.5, 0.5]];
        let (logits, bx) = raw3(1, &rows, &boxes);
        let raw = RawOutputs {
            logits: &logits,
            boxes: &bx,
            num_queries: 1,
            num_columns: 3,
            image_width: 100,
            image_height: 100,
        };
        let out = decode_detections(&raw, &cfg()).unwrap();
        assert!(
            out.detections.is_empty(),
            "no-object 占优的 query 必须被抑制，实际得到 {:?}",
            out.detections
        );
        assert_eq!(out.stats.dropped_low_score, 1);

        // 同一组 logits 下，sigmoid 口径会得到 >0.98（用以说明两者差异）
        let sigmoid = 1.0 / (1.0 + (-4.1f32).exp());
        assert!(sigmoid > 0.98, "sigmoid 口径: {sigmoid}");
    }

    #[test]
    fn rotated_class_is_filtered_when_disabled() {
        let rows = [[-6.0, 6.0, -6.0]]; // 强 rotated
        let boxes = [[0.5, 0.5, 0.4, 0.4]];
        let (logits, bx) = raw3(1, &rows, &boxes);
        let raw = RawOutputs {
            logits: &logits,
            boxes: &bx,
            num_queries: 1,
            num_columns: 3,
            image_width: 100,
            image_height: 100,
        };
        let with = decode_detections(&raw, &cfg()).unwrap();
        assert_eq!(with.detections.len(), 1);
        assert_eq!(with.detections[0].label, Label::TableRotated);

        let mut no_rot = cfg();
        no_rot.include_rotated = false;
        let without = decode_detections(&raw, &no_rot).unwrap();
        assert!(without.detections.is_empty());
        assert_eq!(without.stats.dropped_rotated, 1);
    }

    #[test]
    fn out_of_page_boxes_are_clamped_and_degenerate_ones_dropped() {
        let rows = [[6.0, -6.0, -6.0], [6.0, -6.0, -6.0]];
        // 第一个框一半出界（应被裁剪）；第二个框完全在页外（应被丢弃）
        let boxes = [[0.5, 0.5, 1.4, 0.5], [5.0, 5.0, 0.1, 0.1]];
        let (logits, bx) = raw3(2, &rows, &boxes);
        let raw = RawOutputs {
            logits: &logits,
            boxes: &bx,
            num_queries: 2,
            num_columns: 3,
            image_width: 100,
            image_height: 100,
        };
        let out = decode_detections(&raw, &cfg()).unwrap();
        assert_eq!(out.detections.len(), 1, "越界框应被裁剪而非丢弃");
        let b = out.detections[0].bbox;
        assert!(b.x >= 0.0 && b.x + b.w <= 100.0 + 1e-3);
        assert_eq!(out.stats.dropped_degenerate_box, 1);
    }

    #[test]
    fn full_query_budget_of_model_decodes_without_panicking() {
        let rows = [[-6.0, -6.0, -6.0]; NUM_QUERIES];
        let boxes = [[0.5, 0.5, 0.2, 0.2]; NUM_QUERIES];
        let (logits, bx) = raw3(NUM_QUERIES, &rows, &boxes);
        let raw = RawOutputs {
            logits: &logits,
            boxes: &bx,
            num_queries: NUM_QUERIES,
            num_columns: 3,
            image_width: 612,
            image_height: 792,
        };
        let out = decode_detections(&raw, &cfg()).unwrap();
        assert!(out.detections.is_empty(), "全 no-object 不应产出检测");
        assert_eq!(out.stats.queries, NUM_QUERIES);
    }

    #[test]
    fn rejects_inconsistent_buffer_lengths() {
        let logits = vec![0f32; 3 * 3 - 1];
        let boxes = vec![0f32; 3 * 4];
        let raw = RawOutputs {
            logits: &logits,
            boxes: &boxes,
            num_queries: 3,
            num_columns: 3,
            image_width: 10,
            image_height: 10,
        };
        assert!(matches!(
            decode_detections(&raw, &cfg()),
            Err(TatrError::UnexpectedOutputShape(_))
        ));
    }
}
