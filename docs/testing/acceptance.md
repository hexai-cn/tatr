---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 验收口径、门禁命令与退出标准
---

# 验收规范

## 1. 门禁命令（合并前必须全绿）

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
```

| 门禁 | 覆盖 |
|---|---|
| `fmt --check` | 格式（`rustfmt.toml`：max_width 120） |
| `clippy -D warnings` | 静态缺陷；**零警告**是硬要求 |
| `test` | 28 项单元/集成测试（见 §3） |
| `build --release` | 发布构建可产出（LTO thin） |

## 2. 行为验收口径

### 2.1 精度

- 数据集：TableBank test-0（HuggingFace `deepcopy/TableBank-Detection`）。
- **必须按 `image.path` 聚合标注**：该 parquet 每行是一个标注，同一页可有 2–5 个表格框；
  按行取前 N 会导致多表格页被低估（历史上曾把 F1 从 0.234 虚高到 0.251）。
- 主口径：IoU ≥ 0.5 计数式 P/R/F1（与 `docs/testing/baselines.md` 一致）。
- 辅助口径（对外可比）：TableBank 官方面积口径、COCO AP50。

### 2.2 性能

- CPU 单页 ≤ **200 ms** @800px（8 线程，M5 Pro 基准）。
- 常驻进程吞吐 ≥ **5 页/秒**（单实例）。

### 2.3 契约

| 检查 | 门槛 |
|---|---|
| 解码语义 | 含 no-object softmax；`decode.rs` 回归测试通过 |
| 缩放 | 抗混叠；与 Pillow 参考逐像素差 ≤ 1；棋盘降采样不混叠 |
| 模型契约 | 构造期校验 IO 名称与形状，坏模型必须启动失败 |
| 服务 | 探针可用；非法输入返回 400 且错误体可机读；SIGTERM 优雅退出 |

## 3. 测试清单（28 项）

| 模块 | 数量 | 覆盖 |
|---|---|---|
| `tatr-core::decode` | 6 | 坐标映射、**no-object 抑制回归**、rotated 过滤、越界裁剪与退化框、满 query 预算、缓冲长度校验 |
| `tatr-core::preprocess` | 9 | 缩放语义（短边/长边）、非零尺寸、恒等缩放、NCHW/mask/归一化、单调性、**PIL 参考对照**、**抗混叠性质**、非法缓冲与非法配置 |
| `tatr-core::nms` | 3 | 去重保留高分、不相交保留、阈值 1.0 等价关闭 |
| `tatr-engine` | 5 | 模型缺失构造期失败、垃圾字节拒绝、线程优先级、sha256 已知向量、哈希校验 |
| `tatr-cli::viz` | 4 | 标注图绘制（按类别着色、四边描边且内部不填充、框外不改像素、越界夹取、退化框、文件名派生） |
| `tatr-engine` 文档示例 | 1 | `lib.rs` 用法示例可编译（doc-test） |

## 4. 退出标准（v0.1）

| 编号 | 标准 | 状态 |
|---|---|---|
| S1 | CLI 与 HTTP 纯 CPU 端到端可用 | ✅ 见 `guides/quickstart.md` |
| S2 | 复现 F1 0.780（仅 table）/ 0.786（默认） | ✅ 见 `baselines.md` |
| S3 | 决策级行为有回归测试 | ✅ 24 项通过 |
| S4 | 模型自动获取 + sha256 校验 | ✅ |
| S5 | 探针 / 并发闸门 / 优雅退出 | ✅ |

## 5. 不通过条件

- 任何门禁命令失败；
- 精度基线回退（无正当理由与依据）；
- 默认配置变更但未同步 `docs/specs/detection-contract.md` 与基线文档；
- 引入 Python/torch 运行时依赖（违背 PRD §5 可移植性）。
