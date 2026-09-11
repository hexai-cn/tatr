---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 解码必须用含 no-object 列的 softmax，而非按类独立 sigmoid
---

# ADR-0002：解码用含 no-object 的 softmax

## 状态

active（2026-09-11 决定；源自一次真实缺陷的修复）

## 背景

DETR 类检测头的 `logits` 有 `num_labels + 1` 列，**最后一列是 no-object**
（"此处无目标"）。要把 query 转成检测，必须决定如何取类别分数。

曾采用「按类独立 sigmoid + argmax」——即对每个类别列独立取 `sigmoid(logit)`。该做法错误。

## 决策

使用**包含 no-object 列在内**的 softmax，再在真实类别子集上取 max/argmax：

```
p          = softmax(logits, dim=-1)     # 含 no-object 列
score, cls = max(p[..., :-1], dim=-1)
```

与 HF `DetrImageProcessor.post_process_object_detection` 一致。

## 理由（量化）

反例：某 query 的 `table` logit = 4.1，`no-object` logit = 4.9。

| 口径 | `table` 分数 | 后果 |
|---|---|---|
| 按类 sigmoid | `sigmoid(4.1)` = **0.984** | 远超阈值 → **误检** |
| 含 no-object softmax | ≈ **0.31** | 低于 0.5 → **正确抑制** |

即 no-object 占优时，sigmoid 会给出"看起来极自信"的错误检测。
此外 sigmoid 改变了 query 之间的相对排序，会选错框。

修复后开发集 F1 0.760 → **0.780**，且 Rust 输出与 ONNX Runtime/PyTorch 参考逐图一致
（80 页仅 1 处 IoU 0.975 的边界差，源于重采样量级）。

## 后果

**正面**
- 分数语义正确（是归一化后的类概率），阈值具备可解释性。
- 与参考实现一致，可用对拍做回归。

**负面 / 代价**
- 分数分布与"独立 sigmoid"不同，**不能沿用**按 sigmoid 调出来的阈值；
  默认阈值 0.5 需按 softmax 口径标定（已在基线上标定）。

## 固化

- 实现：`crates/tatr-core/src/decode.rs`
- 回归测试：`suppresses_query_where_no_object_dominates`（断言该 query 必须被抑制，
  并显式断言 sigmoid 口径会给出 >0.98，避免将来有人"简化"回 sigmoid）
