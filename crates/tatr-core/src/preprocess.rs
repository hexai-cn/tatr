//! 预处理：把 `RasterImage` 变成模型输入张量（与训练配置逐项对齐）。

use crate::types::{RasterImage, TatrError};

/// ImageNet 归一化均值（RGB 顺序）。
pub const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];

/// ImageNet 归一化标准差（RGB 顺序）。
pub const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// 预处理后的模型输入。
///
/// 布局为 NCHW（`[1, 3, h, w]`），`mask` 为 `[1, h, w]` 全 1。
#[derive(Debug, Clone)]
pub struct PreprocessedInput {
    /// RGB 归一化像素，NCHW 行优先。
    pub pixels: Vec<f32>,
    /// `pixels` 的高度。
    pub height: u32,
    /// `pixels` 的宽度。
    pub width: u32,
    /// padding mask，长度 `height * width`，恒为 1.0。
    pub mask: Vec<f32>,
    /// 原始图像尺寸 `(w, h)`，用于把归一化框映射回像素。
    pub source_width: u32,
    /// 原始图像高度。
    pub source_height: u32,
}

impl PreprocessedInput {
    /// NCHW 形状。
    #[must_use]
    pub fn pixel_shape(&self) -> [usize; 4] {
        [1, 3, self.height as usize, self.width as usize]
    }

    /// mask 形状。
    #[must_use]
    pub fn mask_shape(&self) -> [usize; 3] {
        [1, self.height as usize, self.width as usize]
    }

    /// 缩放比例（用于诊断）。
    #[must_use]
    pub fn scale(&self) -> (f32, f32) {
        (
            self.width as f32 / self.source_width as f32,
            self.height as f32 / self.source_height as f32,
        )
    }
}

/// 计算缩放后的输入尺寸。
///
/// 语义对齐 HF `DetrImageProcessor.get_size(shortest_edge, longest_edge)`：
/// 先把**短边**缩放到 `short_side`，若此时**长边**超过 `long_side` 再整体等比缩回。
/// 返回 `(w, h)`，至少为 1。
#[must_use]
pub fn target_size(w: u32, h: u32, short_side: u32, long_side: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (1, 1);
    }
    let (wf, hf) = (w as f32, h as f32);
    let scale = short_side as f32 / wf.min(hf);
    let (mut tw, mut th) = (wf * scale, hf * scale);
    let shrink = long_side as f32 / tw.max(th);
    if shrink < 1.0 {
        tw *= shrink;
        th *= shrink;
    }
    ((tw.round() as u32).max(1), (th.round() as u32).max(1))
}

/// 预处理：缩放 → `/255` → ImageNet 归一化 → NCHW。
///
/// 缩放使用三角（bilinear 近似）滤波，与参考实现（PIL bilinear）逐像素误差 ≤ 1/255。
pub fn preprocess(img: &RasterImage, short_side: u32, long_side: u32) -> Result<PreprocessedInput, TatrError> {
    let (tw, th) = target_size(img.width, img.height, short_side, long_side);
    let resized = resize_rgb(img, tw, th);
    let (w, h) = (tw as usize, th as usize);
    let mut pixels = vec![0f32; 3 * h * w];
    let plane = h * w;
    for i in 0..plane {
        let r = resized[i * 3] as f32 / 255.0;
        let g = resized[i * 3 + 1] as f32 / 255.0;
        let b = resized[i * 3 + 2] as f32 / 255.0;
        pixels[i] = (r - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
        pixels[plane + i] = (g - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
        pixels[2 * plane + i] = (b - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
    }
    Ok(PreprocessedInput {
        pixels,
        height: th,
        width: tw,
        mask: vec![1.0; plane],
        source_width: img.width,
        source_height: img.height,
    })
}

/// RGB8 缩放到 `(tw, th)`，**抗混叠**（降采样时按比例放大滤波核）。
///
/// 为什么不用手写 2-tap 双线性：降采样时 2-tap 会**混叠**（点采样退化），
/// 与参考实现（Pillow `Image.resize(BILINEAR)` / torchvision）不一致。
/// `image::FilterType::Triangle` 在降采样时按缩放比放大滤波支撑域，
/// 与 Pillow 行为一致（逐像素最大偏差 ≈1/255）。
fn resize_rgb(img: &RasterImage, tw: u32, th: u32) -> Vec<u8> {
    let (sw, sh) = (img.width, img.height);
    if sw == tw && sh == th {
        return img.rgb.clone();
    }
    let src = image::RgbImage::from_raw(sw, sh, img.rgb.clone()).expect("缓冲长度已由 RasterImage 校验");
    let dst = image::imageops::resize(&src, tw, th, image::imageops::FilterType::Triangle);
    dst.into_raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_size_shrinks_long_side_after_short_side_scaling() {
        // 正方形：短边=长边 → 只受 short_side 约束
        assert_eq!(target_size(1000, 1000, 800, 800), (800, 800));
        // 高瘦页：短边(宽)拉到 800 后长边超上限 → 整体缩回，长边=800
        let (w, h) = target_size(600, 1200, 800, 800);
        assert_eq!(h, 800, "长边必须被封顶到 long_side");
        assert_eq!(w, 400, "等比缩放：宽 = 800 * 600/1200");
    }

    #[test]
    fn target_size_never_returns_zero() {
        let (w, h) = target_size(1, 10000, 800, 800);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn preprocess_produces_nchw_with_mask_and_normalisation() {
        // 纯白图：/255=1 → (1-mean)/std，三通道各自可预测
        let white = RasterImage::from_rgb(2, 2, vec![255u8; 12]).unwrap();
        let out = preprocess(&white, 2, 2).unwrap();
        assert_eq!(out.pixel_shape(), [1, 3, 2, 2]);
        assert_eq!(out.mask_shape(), [1, 2, 2]);
        assert_eq!(out.mask.len(), 4);
        let plane = 4;
        let expect_r = (1.0 - IMAGENET_MEAN[0]) / IMAGENET_STD[0];
        let expect_g = (1.0 - IMAGENET_MEAN[1]) / IMAGENET_STD[1];
        let expect_b = (1.0 - IMAGENET_MEAN[2]) / IMAGENET_STD[2];
        for i in 0..plane {
            assert!((out.pixels[i] - expect_r).abs() < 1e-6, "R 平面");
            assert!((out.pixels[plane + i] - expect_g).abs() < 1e-6, "G 平面");
            assert!((out.pixels[2 * plane + i] - expect_b).abs() < 1e-6, "B 平面");
        }
    }

    #[test]
    fn preprocess_resize_preserves_gradient_monotonically() {
        // 水平渐变：缩放后仍应单调（防止轴/步长写错导致交叉串扰）
        // 注意 short_side/long_side 必须有效（long_side >= short_side），否则尺寸退化。
        let (w, h) = (16u32, 4u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = (x * 16) as u8;
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        let img = RasterImage::from_rgb(w, h, rgb).unwrap();
        let out = preprocess(&img, 8, 8).unwrap();
        assert_eq!(out.pixel_shape(), [1, 3, 2, 8], "输出应为 8x2 (H=2, W=8)");
        let plane = out.height as usize * out.width as usize;
        let row: Vec<f32> = (0..8).map(|x| out.pixels[x]).collect();
        for i in 1..row.len() {
            assert!(row[i] > row[i - 1], "缩放后应保持严格单调: {row:?}");
        }
        // 平面之间不得串扰：输入为灰 → 反归一化后三通道应完全一致
        for i in 0..plane {
            let r = out.pixels[i] * IMAGENET_STD[0] + IMAGENET_MEAN[0];
            let g = out.pixels[plane + i] * IMAGENET_STD[1] + IMAGENET_MEAN[1];
            let b = out.pixels[2 * plane + i] * IMAGENET_STD[2] + IMAGENET_MEAN[2];
            assert!(
                (r - g).abs() < 1e-5 && (r - b).abs() < 1e-5,
                "灰度输入的三通道应一致: {r} {g} {b}"
            );
        }
    }

    #[test]
    fn preprocess_rejects_invalid_side_config() {
        let img = RasterImage::from_rgb(2, 2, vec![0u8; 12]).unwrap();
        // long_side < short_side 属配置错误，应由 DetectorConfig::validate 拦截；
        // 此处确认 target_size 不会 panic（防御性）
        let (w, h) = target_size(img.width, img.height, 8, 4);
        assert!(w >= 1 && h >= 1);
    }

    #[test]
    fn resize_matches_pil_bilinear_reference() {
        // 参考真值由 Pillow `Image.resize(..., BILINEAR)` 生成（与 torchvision/HF 预处理同源）。
        // 把 9x5 确定性图案缩到 5x3，逐像素比对；容差 1/255 覆盖浮点取整差异。
        let (w, h) = (9u32, 5u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                rgb.push(((x * 23) % 256) as u8);
                rgb.push(((y * 47) % 256) as u8);
                rgb.push((((x + y) * 31) % 256) as u8);
            }
        }
        let img = RasterImage::from_rgb(w, h, rgb).unwrap();
        let got = resize_rgb(&img, 5, 3);
        let expected: [[u8; 3]; 15] = [
            [13, 20, 31],
            [49, 20, 79],
            [92, 20, 137],
            [135, 20, 195],
            [171, 20, 188],
            [13, 94, 80],
            [49, 94, 128],
            [92, 94, 186],
            [135, 94, 178],
            [171, 94, 75],
            [13, 168, 129],
            [49, 168, 177],
            [92, 168, 200],
            [135, 168, 71],
            [171, 168, 85],
        ];
        for (i, exp) in expected.iter().enumerate() {
            for c in 0..3 {
                let g = got[i * 3 + c] as i32;
                let e = exp[c] as i32;
                assert!(
                    (g - e).abs() <= 1,
                    "像素 {i} 通道 {c}: 得到 {g}，PIL 参考 {e}（容差 1）"
                );
            }
        }
    }

    #[test]
    fn resize_identity_returns_input_unchanged() {
        let img = RasterImage::from_rgb(3, 2, (0..18).map(|v| v as u8).collect()).unwrap();
        assert_eq!(resize_rgb(&img, 3, 2), img.rgb);
    }

    #[test]
    fn downscale_is_antialiased_not_point_sampled() {
        // 4x4 黑白棋盘 → 缩到 2x2：抗混叠实现应得到接近中性的灰（约 127），
        // 点采样/2-tap 双线性会保留纯黑或纯白（混叠）。这是判断重采样质量的关键行为。
        let (w, h) = (4u32, 4u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let v = if (x + y) % 2 == 0 { 0u8 } else { 255u8 };
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        let img = RasterImage::from_rgb(w, h, rgb).unwrap();
        let out = resize_rgb(&img, 2, 2);
        for (i, px) in out.chunks(3).enumerate() {
            let v = px[0];
            assert!(
                (v as i32 - 128).abs() <= 20,
                "像素 {i} 得到 {v}，抗混叠降采样应接近 128（纯 0/255 说明发生混叠）"
            );
        }
    }

    #[test]
    fn raster_image_rejects_wrong_buffer_length() {
        assert!(RasterImage::from_rgb(2, 2, vec![0u8; 11]).is_err());
        assert!(RasterImage::from_rgb(0, 2, vec![]).is_err());
    }
}
