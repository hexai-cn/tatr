---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 采用 ONNX Runtime 做推理；模型走 GitHub Release 资产而非入库
---

# ADR-0001：ONNX Runtime + 模型走 Release 资产

## 状态

active（2026-09-11 决定）

## 背景

需要选择推理运行时与模型分发方式。候选运行时：`ort`（ONNX Runtime）、`tch`（libtorch）、
`candle`（纯 Rust）、`burn`（纯 Rust）。候选分发：模型入库 git / git-lfs / Release 资产 /
运行时下载。

## 决策

1. **推理用 `ort`（ONNX Runtime）**，模型导出为 ONNX（opset 17，动态 H/W）。
2. **模型不入 git**：发布为 GitHub Release 资产，引擎首次使用时下载到
   `~/.cache/tatr/` 并做 **sha256 校验**；同时支持 `LocalFile` 离线路径。

## 理由

| 方案 | 评估 |
|---|---|
| `ort` | 成熟、CPU 性能好；二进制约 26 MB；与参考实现（ONNX Runtime / PyTorch）可逐图对拍，便于验证 |
| `tch` | 需 libtorch（数百 MB）与 C++ 运行时；与"纯 Rust、无重依赖"目标冲突 |
| `candle`/`burn` | 生态较新；DETR 动态轴与算子支持不确定，验证成本高 |

模型分发：110 MB 二进制入库会让每次 clone 变重且历史不可回收；git-lfs 需要额外客户端。
Release 资产 + 哈希校验在"可获取性"与"仓库轻量"之间取平衡。

## 后果

**正面**
- 二进制小、无 Python/torch 运行时依赖，CPU 部署直接。
- 与参考实现对拍容易（同一 ONNX 可在 ORT/PyTorch 双跑），验证成本低。
- 模型可独立升级，不必改代码。

**负面 / 代价**
- 首次运行需网络（或离线预置）；因此在 `LocalFile` 与容器镜像内预置两条路都保留。
- 下载依赖系统 `curl`（有意为之：避免引入 TLS 栈依赖；企业内网可替换代理）。
- 校验失败会删除缓存文件，需要网络恢复后才能重试。

## 被否决的方案

- **模型入库**：仓库体积与历史膨胀。
- **git-lfs**：要求使用者安装 lfs，CI/容器构建链路变复杂。
- **纯 Rust 运行时**：验证成本高，收益（去掉 ONNX Runtime）不足以抵消。
