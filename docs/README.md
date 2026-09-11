---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: tatr 文档目录结构与文档约定（frontmatter、命名、生命周期、AI 路由）
---

# tatr 文档库

本目录组织 tatr 仓库的全部文档。目录命名与约定沿用 sift 文档规范
（`hexinfo/sift/docs/README.md`）与 xdoc-rs 的实现（`poc/xdoc-rs/docs/README.md`）。

仓库顶层另有 `README.md`（用户入口）与 `AGENTS.md`（模块边界 + 验证命令，**活索引**）。
本文件是文档细则的唯一事实来源。

## 目录结构

```
docs/
├── README.md          ← 本文件（约定）
├── PRD.md             产品需求：定位、用户、范围、能力与成功标准
├── DESIGN.md          架构设计：分层、数据流、模块边界、CPU 部署与验证策略
├── WBS.md             项目主计划：工作分解、依赖、进度
├── specs/             模块规格与契约（代码与测试的共同依据）
├── testing/           验收规范与基线（门禁口径）
├── plans/             实施计划（一次性写入，YYYY-MM-DD-主题-作者）
├── reviews/           评审报告（只读存档）
├── analysis/          过程分析/审计存档
├── guides/            操作指南（how-to）
├── decisions/         ADR（只追加）
├── runbooks/          运维手册
├── dev/               开发者文档（仓库布局、贡献）
├── external/          对外交付件
└── archive/           历史过程文档
```

顶层长期文档职责：

| 文档 | 定位 | 生命周期 |
|---|---|---|
| `PRD.md` | 产品范围、目标用户、版本承诺与成功标准 | 随产品决策更新 |
| `DESIGN.md` | 全局架构、模块边界、数据流与质量策略 | 随架构演进更新 |
| `WBS.md` | 工作分解、依赖与聚合进度 | 贯穿项目更新 |

## 文件头约定（必填）

所有子目录文档以 YAML frontmatter 开头：

```
---
status: draft | active | done | abandoned | superseded
created: YYYY-MM-DD
last_updated: YYYY-MM-DD   # 长期活文档推荐
summary: 30 字内一句话
---
```

## 命名约定

- 长期文档（`specs/`/`testing/`/`guides/`/`external/`）：`<主题>.md`，不加日期。
- 过程文档（`plans/`/`reviews/`/`analysis/`/`archive/`）：`YYYY-MM-DD-<主题>-<作者>.md`。
  作者可为人类 Handle 或 AI Agent 标识（如 `hex`、`pi-k3`）。

## 内容纪律

- **单一事实来源**：同一事实只在一处维护，其他位置引用路径。
- **规格即测试基准**：`testing/` 的口径派生自 `specs/`，spec 变更时同步检查。
- **正文不混 changelog**：状态变化由 git 历史与 frontmatter `status` 表达。
- 当前事实以 `AGENTS.md` + `status: active/draft` 文档为准；`done/superseded` 仅供追溯。

## AI 代理路由

- 默认只加载 `active`/`draft` 文档；`done/superseded/abandoned` 按需追溯。
- 改代码前先读 `AGENTS.md`（模块边界）与本目录相关 active 文档。

## 文档索引

### 顶层产品文档

| 文档 | 内容 |
|---|---|
| [`PRD.md`](PRD.md) | 产品定位、目标用户、范围、非功能需求与成功标准 |
| [`DESIGN.md`](DESIGN.md) | 分层架构、模块边界、数据流、CPU 部署与验证策略 |
| [`WBS.md`](WBS.md) | 工作分解、依赖与实施状态 |

### specs/

| 文档 | 内容 |
|---|---|
| [`detection-contract.md`](specs/detection-contract.md) | **核心契约**：输入张量约定、DETR 解码语义、后处理与坐标映射 |
| [`engine-api.md`](specs/engine-api.md) | 引擎 API 契约：模型来源、会话配置、检测接口与错误语义 |
| [`http-api.md`](specs/http-api.md) | HTTP 服务契约：端点、参数、状态码与错误体 |

### testing/

| 文档 | 内容 |
|---|---|
| [`acceptance.md`](testing/acceptance.md) | 验收口径、门禁命令与退出标准 |
| [`baselines.md`](testing/baselines.md) | TableBank 基线与 CPU 性能基线（含复跑方法） |

### decisions/

| 文档 | 内容 |
|---|---|
| [`0001-onnx-runtime-and-model-distribution.md`](decisions/0001-onnx-runtime-and-model-distribution.md) | 选 ONNX Runtime + 模型走 Release 资产、不入 git |
| [`0002-detr-softmax-decoding.md`](decisions/0002-detr-softmax-decoding.md) | 解码必须用含 no-object 的 softmax（非按类 sigmoid） |
| [`0003-antialiased-resize.md`](decisions/0003-antialiased-resize.md) | 缩放必须抗混叠（Triangle，非 2-tap 双线性） |

### guides/

| 文档 | 内容 |
|---|---|
| [`quickstart.md`](guides/quickstart.md) | 五分钟上手：CLI 与 HTTP |
| [`cpu-deployment.md`](guides/cpu-deployment.md) | CPU 部署与吞吐调优 |

### runbooks/

| 文档 | 内容 |
|---|---|
| [`service-ops.md`](runbooks/service-ops.md) | 服务运维：探针、容量、常见故障处置 |

### dev/

| 文档 | 内容 |
|---|---|
| [`repository-layout.md`](dev/repository-layout.md) | 仓库布局与分层边界 |

### plans/

| 文档 | 内容 |
|---|---|
| [`2026-09-11-initial-implementation-hex.md`](plans/2026-09-11-initial-implementation-hex.md) | v0.1 首个实现计划与交付证据 |
