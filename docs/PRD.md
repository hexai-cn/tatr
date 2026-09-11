---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: tatr 产品定位、目标用户、范围、非功能需求与成功标准
---

# PRD — tatr 表格检测

## 1. 定位

**tatr 是一个纯 Rust 的表格区域检测库与可独立部署的服务**，用 Table Transformer
（DETR + ResNet-18，Microsoft，MIT 权重）定位文档图像中的表格区域，
面向 **CPU-first** 的部署环境。

它是**检测（detection）**组件：输入一张页面图像，输出 N 个表格外接框。
表格**结构识别**（行列单元格恢复）不在本仓库范围内。

## 2. 目标用户与集成形态

| 用户 | 集成形态 | 关注点 |
|---|---|---|
| 上层文档处理流水线（Rust） | 进程内 crate：`tatr-engine` | 无子进程、无 Python、类型安全、可嵌入 |
| 平台/服务团队 | 独立 HTTP 服务：`apps/tatr-http` | 容器化、就绪探针、并发控制 |
| 算法/评测 | CLI：`tatr detect` | 批量跑分、JSON 结果、可脚本化 |

## 3. 范围

### 3.1 做什么

- 页面图像 → 表格外接框（含置信度与正向/旋转类别）。
- 模型自动获取（本地 / 缓存 / 下载并校验 sha256）。
- CPU 部署与吞吐调优（线程配置、多实例）。
- 纯 CPU 推理，无 GPU 强依赖。

### 3.2 不做什么（明确边界）

| 不做 | 原因 |
|---|---|
| 表格结构识别（单元格/合并/表头） | 属另一个模型与任务（TATR-v1.1 结构模型） |
| 训练/微调流程 | 本仓库只做推理；微调在训练侧完成 |
| OCR / 文本提取 | 与检测解耦 |
| 版面其他类别（图、标题、公式） | 换模型（如 DocLayout-YOLO）属独立决策，见 DESIGN §7 |
| GPU 专用优化 | CPU-first；GPU 直接复用 ONNX Runtime 的 provider 机制 |

## 4. 核心能力

| 能力 | 说明 |
|---|---|
| 检测 | 输出框（像素坐标）、置信度、`table` / `table_rotated` 类别 |
| 定位可视化 | CLI `--viz <dir>` 为每张输入写出带框标注图（PNG），供人工核对定位效果 |
| 阈值与缩放可调 | `threshold`、`short_side`、`long_side`、`nms_iou` |
| 旋转表格 | 默认保留（实测零误检代价提升召回，见 `testing/baselines.md`） |
| 模型治理 | sha256 校验、缓存、离线路径，启动期即失败 |
| 可观测 | 结构化日志、解码统计、耗时字段 |

## 5. 非功能需求

| 维度 | 要求 |
|---|---|
| **正确性** | 解码语义必须与参考实现（HF `DetrImageProcessor`）逐图一致；Rust 与 ONNX Runtime/PyTorch 对拍 0 差异 |
| **性能** | CPU 单页 ≤ 200 ms @800px（M5 Pro 基准，8 线程）；≥ 5 页/秒/进程 |
| **可移植** | macOS arm64 / Linux x86_64 / Linux arm64；无 Python 运行时依赖 |
| **许可** | 代码 MIT OR Apache-2.0；模型 MIT（可商用、可再分发） |
| **可运维** | 探针（liveness/readiness）、优雅退出、并发闸门、错误体可机读 |
| **可复现** | 基线数字附复跑命令；重采样/解码行为有 in-repo 回归测试钉住 |

## 6. 成功标准（v0.1）

| 编号 | 标准 | 验证方式 |
|---|---|---|
| S1 | CLI 与 HTTP 均可在纯 CPU 上端到端检出表格 | `guides/quickstart.md` 步骤 |
| S2 | 在 TableBank test-0（300 页）复现 F1 **0.780**（仅 `table`）/ **0.786**（含 rotated） | `testing/baselines.md` |
| S3 | 决策级行为有回归测试：softmax 解码、抗混叠缩放、退化框处理 | `cargo test` |
| S4 | 模型可自动获取且 sha256 校验通过 | `tatr model fetch` |
| S5 | 服务具备探针、并发闸门与优雅退出 | `runbooks/service-ops.md` |

## 7. 已知限制（诚实披露）

1. **域差**：模型在 PubTables-1M（科学 PDF）训练，迁移到其他域（Word/LaTeX/表单）
   会掉点。零样本跨域到 TableBank 的实测见 `testing/baselines.md`。
2. **表格密集页**：TableBank 每页 ≤5 表，**测不到**单页 10–40 表的表单场景；
   该场景的表现尚未建立基线（见 WBS 的后续项）。
3. **检测 ≠ 结构**：本仓库只出框，不保证单元格正确。
