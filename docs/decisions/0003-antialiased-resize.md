---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 图像缩放必须抗混叠（按比例放大滤波核），2-tap 双线性会掉指标
---

# ADR-0003：缩放必须抗混叠

## 状态

active（2026-09-11 决定；源自一次真实缺陷的修复）

## 背景

预处理需要把页面缩放到模型输入尺寸（典型为 800px 长边，即**降采样**）。
最初手写了「2-tap 双线性」：对每个输出像素取最近两点的线性插值。

## 决策

使用**抗混叠**的重采样：降采样时按缩放比**放大滤波支撑域**
（`image::imageops::FilterType::Triangle`）。等价语义：输出像素是源像素的加权平均，
而非两点插值。

## 理由（量化）

降采样时 2-tap 双线性退化为"带偏移的点采样"，会**混叠**：

| 手段 | 现象 |
|---|---|
| 4×4 黑白棋盘 → 2×2 | 抗混叠得 ≈128（中性灰）；2-tap 得纯 0 或 255（混叠伪影） |
| 9×5 图案 → 5×3，与 Pillow `BILINEAR` 对照 | `image` Triangle：逐像素差 ≤1；2-tap：首像素偏 4（9 vs 13） |

**对指标的实际影响**：手写 2-tap 版本在 TableBank 300 页上得到
`TP307 FP94 FN68`（F1 0.791），而参考实现为 `TP303 FP93 FN72`（F1 0.786）。
差异来自**临界 query 的分数跨越阈值**（实例：页面 022 的 `table` 概率 0.4965、
`no-object` 0.5035，重采样微差即可翻转 0.5 判定）。

⇒ 非抗混叠实现会产出**看似更好但实为噪声**的指标，且与参考实现不可对拍。

## 后果

**正面**
- 与 Pillow/torchvision 行为一致（模型训练时的预处理口径），指标可对拍、可复现。
- 消除边缘/高频区域的混叠伪影。

**负面 / 代价**
- 降采样比 2-tap 慢（卷积核随缩放比变宽），在 800px 量级可忽略。
- 增加一个对 `image` crate 的行为依赖；已用 PIL 参考值做 in-repo 回归钉住。

## 固化

- 实现：`crates/tatr-core/src/preprocess.rs::resize_rgb`
- 回归测试：`resize_matches_pil_bilinear_reference`（PIL 参考值内嵌，容差 1）、
  `downscale_is_antialiased_not_point_sampled`（棋盘必须得到中性灰）
