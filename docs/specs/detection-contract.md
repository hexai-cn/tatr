---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 检测契约——输入张量约定、DETR 解码语义、后处理与坐标映射
---

# Spec — 检测契约

本文件是 `tatr-core` 的实现依据，也是 `testing/` 验收口径的来源。

## 1. 输入张量约定（与模型训练配置一致）

| 名称 | 形状 | 说明 |
|---|---|---|
| `pixel_values` | `f32 [1,3,H,W]` | RGB；`/255` 后按 ImageNet `mean=[.485,.456,.406]`、`std=[.229,.224,.225]` 归一化 |
| `pixel_mask` | `f32 [1,H,W]` | 全 1（DETR 的 padding mask） |

**缩放规则**（等价 HF `DetrImageProcessor.get_size`）：

```
1) 短边 → short_side（默认 800），长边等比
2) 若长边 > long_side（默认 800），整体等比缩回，使长边 == long_side
3) 结果四舍五入取整，且 ≥ 1
```

**重采样必须是抗混叠的**（降采样按比例放大滤波核）。实测：2-tap 双线性在
降采样时混叠，会使临界 query 的分数跨越阈值 → 指标漂移。见 ADR-0003。

## 2. 输出张量

| 名称 | 形状 | 说明 |
|---|---|---|
| `logits` | `f32 [1,Q,C]` | `Q=15` queries，`C=num_labels+1=3`（末列 **no-object**） |
| `pred_boxes` | `f32 [1,Q,4]` | 归一化 `cxcywh`（相对输入尺寸） |

## 3. 解码语义（**必须逐字实现**）

```
p          = softmax(logits, dim = -1)     # 含 no-object 列
score, cls = max(p[..., :-1], dim = -1)    # 只在真实类别子集上取 max/argmax
```

**禁止**用按类独立 sigmoid。反例（真实缺陷）：表格 logit 4.1、no-object logit 4.9 时，
sigmoid 给出 0.984（误检），正确 softmax 仅 0.31（正确抑制）。
回归测试：`crates/tatr-core/src/decode.rs::suppresses_query_where_no_object_dominates`。

## 4. 类别

| 索引 | 类别 | 处理 |
|---|---|---|
| 0 | `table` | 总是保留（分数过阈值时） |
| 1 | `table_rotated` | `include_rotated=false` 时丢弃；**默认 true**（实测零误检代价提升召回） |
| 2 | `no-object` | 不产出检测；其概率参与 softmax 归一化 |

## 5. 后处理与坐标映射

```
1) 分数 < threshold（默认 0.5）→ 丢弃
2) 类别为 rotated 且 include_rotated=false → 丢弃
3) 归一化 cxcywh → 像素 xywh：
       x = (cx - w/2) * W ,  y = (cy - h/2) * H
       w = w * W           ,  h = h * H
4) clamp 到 [0,W]×[0,H]；宽或高 ≤ min_box_side（默认 1.0）→ 丢弃
5) 可选 NMS（nms_iou，默认 1.0 = 关闭）
6) 输出按分数降序
```

## 6. 不变量

| 不变量 | 说明 |
|---|---|
| 框在页面内 | 输出的每个框都满足 `0 ≤ x`、`x + w ≤ W`（同理 y/h） |
| 分数语义 | `score ∈ [0,1]`，是**含 no-object 归一化**后的类概率，不是 sigmoid |
| 确定性 | 同一输入 + 同一配置 ⇒ 同一输出（ORT CPU EP，图优化固定 Level3） |
| 无隐性污染 | 解码不读取环境变量；阈值/尺寸等全部来自显式配置 |

## 7. 契约变更流程

任何影响上述语义的改动（阈值默认、缩放规则、类别处理、坐标映射）：
1. 更新本文件；
2. 同步 `testing/baselines.md` 的基线数字（或说明为何不变）；
3. 若涉及默认值，附上支撑数据。
