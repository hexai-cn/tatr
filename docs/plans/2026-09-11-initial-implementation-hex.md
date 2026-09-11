---
status: done
created: 2026-09-11
last_updated: 2026-09-11
summary: v0.1 首个实现计划：多 crate 引擎库 + CLI + HTTP 服务，含 CPU 验证证据
---

# 实施计划 — tatr v0.1（2026-09-11，hex）

## 目标

按 xdoc-rs 文档规范建立生产级 Rust 仓库，实现 Table Transformer 表格检测：
多 crate、可作引擎嵌入上层、可独立 HTTP 发布、CPU 部署、模型走 GitHub Release。

## 交付物与证据

| 项 | 证据 |
|---|---|
| workspace（4 成员） | `cargo build --release` 通过 |
| `tatr-core` 纯算法 | 18 项单测（含解码语义与重采样回归） |
| `tatr-engine` 推理与资源 | 5 项单测 + 端到端检测 |
| CLI | 300 页批量检测 15.1s，stdout 纯 JSON |
| HTTP 服务 | `/healthz` `/readyz` `/v1/model` `/v1/detect` `/v1/detect/multipart` 实测 |
| 文档体系 | 本目录 + PRD/DESIGN/WBS + 3 specs + 2 testing + 3 ADR + 2 guides + 1 runbook + dev |
| 评测工具 | `tools/bench/{prepare_tablebank,score}.py` 复跑基线 |
| 模型发布 | GitHub Release 资产 + sha256 固化 |

## 验收结果

| 标准 | 结果 |
|---|---|
| CPU 端到端（CLI + HTTP） | ✅ |
| TableBank 300 页 F1 | ✅ 仅 table **0.780**；默认（含 rotated）**0.786** |
| 行为回归测试 | ✅ 24 项通过 |
| 模型自动获取 + sha256 | ✅ |
| 探针 / 并发闸门 / 优雅退出 | ✅ |

性能（M5 Pro，release）：单页 median 46 ms（8 线程）；6 线程即饱和；
二进制 26 MB、模型 110 MB，**无 Python/torch 运行时依赖**。

## 过程中发现并修复的两个真实缺陷

1. **CLI 日志写 stdout**，污染 JSON 输出（`tatr detect | jq` 失败）。
   改为 `with_writer(std::io::stderr)`。
2. **手写 2-tap 双线性在降采样时混叠**，导致指标与参考实现偏离
   （F1 0.791 vs 0.786，临界 query 分数跨阈值翻转）。
   改用抗混叠 Triangle，并加两条回归测试（PIL 对照 + 棋盘性质）。
   详见 ADR-0003。

## 偏离与说明

- 任务要求"在 `/Users/hex/github/hexinfo` 下初始化新仓库"：`hexinfo` 本身是**项目父目录**
  （已含 `sift/`、`next-ocr/`、`openclash/`），故新仓库建于
  `/Users/hex/github/hexinfo/tatr`。
- 任务要求"按 xdoc-rs 的 README 规范"：该文档是 xdoc-rs 的**实例**，
  其规范源头是 `hexinfo/sift/docs/README.md`。本仓库按该规范裁剪适配（无 `exts/` 层）。
- `tools/bench/` 由 hocr 项目的评测脚本整理而来，去掉其特定路径依赖。

## 后续

见 `docs/WBS.md` §后续。首要项是 **W-1.1 表格密集页评测轴**（当前测量盲区）。
