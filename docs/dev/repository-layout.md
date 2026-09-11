---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 仓库布局与分层边界
---

# 仓库布局

```
tatr/
├── Cargo.toml               # workspace（4 个成员）
├── rustfmt.toml             # max_width 120
├── rust-toolchain.toml      # 固定 1.95.0 + rustfmt/clippy
├── AGENTS.md                # 活索引：模块边界 + 验证命令
├── README.md                # 用户入口
├── crates/
│   ├── tatr-core/           # 纯算法：类型 / 预处理 / 解码 / NMS
│   └── tatr-engine/         # 推理与资源：模型来源 / ORT 会话 / 检测门面
├── apps/
│   ├── tatr-cli/            # 命令行（bin: tatr）
│   └── tatr-http/           # HTTP 服务（bin: tatr-http）
├── models/                  # 模型说明（*.onnx 不入 git，见 models/README.md）
├── tools/
│   └── bench/               # 评测脚本（prepare_tablebank.py / score.py）
└── docs/                    # 见 docs/README.md
```

## 分层边界

| 层 | crate/app | 允许 | 禁止 |
|---|---|---|---|
| 算法 | `tatr-core` | 几何、张量约定、解码语义、数学 | ORT、文件/网络 IO、日志、环境变量 —— **必须能无模型单测** |
| 推理 | `tatr-engine` | ort 调用、模型下载/校验、图像解码、会话配置 | 业务规则、HTTP/CLI 关注点 |
| 应用 | `apps/*` | 参数解析、路由、并发闸门、错误映射 | 复刻算法 |

依赖方向：`tatr-core ← tatr-engine ← {tatr-cli, tatr-http, 上层集成}`（禁止反向）。

## 改动放哪

| 症状 | 落点 |
|---|---|
| 框坐标/类别/阈值语义不对 | `tatr-core`（`decode.rs`） |
| 缩放/归一化不对 | `tatr-core`（`preprocess.rs`） |
| 模型下载/缓存/会话配置 | `tatr-engine`（`model.rs` / `session.rs`） |
| 命令行参数/输出格式 | `apps/tatr-cli` |
| 端点/状态码/并发行为 | `apps/tatr-http` |

## 契约与文档同步

改动下列任一内容，必须同步 `docs/specs/detection-contract.md` 与 `docs/testing/baselines.md`：

- 阈值/尺寸默认值
- 缩放或归一化语义
- 类别处理（rotated 的默认策略）
- 坐标映射与后处理顺序
- 模型文件与其 sha256
