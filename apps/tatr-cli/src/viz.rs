//! 检测框可视化：把检测结果画到原图副本上，用于人工核对定位效果。
//!
//! 纯像素操作，不依赖推理运行时；线宽随页面尺寸自适应，框一律夹在页面内。

use std::path::Path;

use anyhow::{Context, Result};
use tatr_core::{BBox, Detection, Label, RasterImage};

/// `table` 框颜色（亮绿）。
pub const TABLE_COLOR: [u8; 3] = [0x00, 0xC8, 0x53];
/// `table rotated` 框颜色（亮橙）。
pub const ROTATED_COLOR: [u8; 3] = [0xFF, 0x45, 0x00];

/// 在原图（按值传入，避免额外拷贝）上绘制检测框。
#[must_use]
pub fn render_detections(mut img: RasterImage, detections: &[Detection]) -> RasterImage {
    let thickness = (img.width.min(img.height) / 400).clamp(2, 8);
    for det in detections {
        let color = match det.label {
            Label::Table => TABLE_COLOR,
            Label::TableRotated => ROTATED_COLOR,
        };
        stroke_bbox(&mut img, &det.bbox, thickness, color);
    }
    img
}

/// 把标注图编码为 PNG 写盘。
pub fn write_png(img: RasterImage, path: &Path) -> Result<()> {
    let buf = image::RgbImage::from_raw(img.width, img.height, img.rgb).expect("RasterImage 缓冲恒与尺寸一致");
    buf.save_with_format(path, image::ImageFormat::Png)
        .with_context(|| format!("写入标注图 {}", path.display()))
}

/// 输入图像路径 → 标注图文件名（`<stem>.viz.png`）。
#[must_use]
pub fn overlay_file_name(input: &Path) -> String {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    format!("{stem}.viz.png")
}

/// 绘制一个空心矩形边框：四条边向框内推进 `thickness` 像素。
fn stroke_bbox(img: &mut RasterImage, bbox: &BBox, thickness: u32, color: [u8; 3]) {
    let Some((x0, y0, x1, y1)) = pixel_edges(bbox, img.width, img.height) else {
        return;
    };
    let t = thickness.max(1).min(x1 - x0).min(y1 - y0);
    fill_rect(img, x0, y0, x1, y0 + t, color);
    fill_rect(img, x0, y1 - t, x1, y1, color);
    fill_rect(img, x0, y0, x0 + t, y1, color);
    fill_rect(img, x1 - t, y0, x1, y1, color);
}

/// 框 → 半开像素区间 `[x0,x1) × [y0,y1)`，夹在页面内；退化框返回 `None`。
fn pixel_edges(bbox: &BBox, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let w = width as f32;
    let h = height as f32;
    let x0 = bbox.x.max(0.0).min(w) as u32;
    let y0 = bbox.y.max(0.0).min(h) as u32;
    let x1 = (bbox.x + bbox.w).ceil().clamp(0.0, w) as u32;
    let y1 = (bbox.y + bbox.h).ceil().clamp(0.0, h) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1, y1))
}

/// 填充半开矩形 `[x0,x1) × [y0,y1)`；调用方保证区间在页面内且非空。
fn fill_rect(img: &mut RasterImage, x0: u32, y0: u32, x1: u32, y1: u32, color: [u8; 3]) {
    let stride = img.width as usize * 3;
    for y in y0..y1 {
        let row = y as usize * stride;
        for x in x0..x1 {
            let i = row + x as usize * 3;
            img.rgb[i..i + 3].copy_from_slice(&color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black(w: u32, h: u32) -> RasterImage {
        RasterImage::from_rgb(w, h, vec![0; w as usize * h as usize * 3]).unwrap()
    }

    fn at(img: &RasterImage, x: u32, y: u32) -> [u8; 3] {
        let i = (y as usize * img.width as usize + x as usize) * 3;
        [img.rgb[i], img.rgb[i + 1], img.rgb[i + 2]]
    }

    fn det(bbox: BBox, label: Label) -> Detection {
        Detection {
            bbox,
            score: 0.9,
            label,
        }
    }

    #[test]
    fn strokes_border_by_label_without_touching_interior() {
        let bbox = BBox {
            x: 8.0,
            y: 8.0,
            w: 16.0,
            h: 16.0,
        };
        let out = render_detections(black(32, 32), &[det(bbox, Label::Table)]);
        assert_eq!(at(&out, 8, 8), TABLE_COLOR, "左上角属于边框");
        assert_eq!(at(&out, 23, 23), TABLE_COLOR, "右下角属于边框");
        assert_eq!(at(&out, 16, 16), [0, 0, 0], "框内部不填充");
        assert_eq!(at(&out, 4, 8), [0, 0, 0], "框外不改像素");

        let out = render_detections(black(32, 32), &[det(bbox, Label::TableRotated)]);
        assert_eq!(at(&out, 8, 8), ROTATED_COLOR, "旋转类别用另一颜色");
    }

    #[test]
    fn clamps_out_of_page_and_skips_degenerate_boxes() {
        let outside = BBox {
            x: -40.0,
            y: -40.0,
            w: 5.0,
            h: 5.0,
        };
        let degenerate = BBox {
            x: 5.0,
            y: 5.0,
            w: 0.0,
            h: 0.0,
        };
        let out = render_detections(
            black(16, 16),
            &[det(outside, Label::Table), det(degenerate, Label::Table)],
        );
        assert!(out.rgb.iter().all(|&b| b == 0), "完全出界/退化框不产生像素改动");

        let partial = BBox {
            x: -4.0,
            y: -4.0,
            w: 12.0,
            h: 12.0,
        };
        let out = render_detections(black(16, 16), &[det(partial, Label::Table)]);
        assert_eq!(at(&out, 0, 0), TABLE_COLOR, "部分出界的框夹到页面后仍然绘制");
    }

    #[test]
    fn thin_boxes_collapse_to_filled_rect_without_panicking() {
        let tiny = BBox {
            x: 4.0,
            y: 4.0,
            w: 1.0,
            h: 1.0,
        };
        let out = render_detections(black(16, 16), &[det(tiny, Label::Table)]);
        assert_eq!(at(&out, 4, 4), TABLE_COLOR);
        assert_eq!(at(&out, 5, 5), [0, 0, 0]);
    }

    #[test]
    fn overlay_file_name_uses_input_stem() {
        assert_eq!(overlay_file_name(Path::new("pages/007.png")), "007.viz.png");
        assert_eq!(overlay_file_name(Path::new("a.b.jpeg")), "a.b.viz.png");
    }
}
