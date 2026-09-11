# AGENTS.md

## 项目文档入口

- 产品范围与成功标准：[`docs/PRD.md`](docs/PRD.md)
- 架构、模块边界与数据流：[`docs/DESIGN.md`](docs/DESIGN.md)
- 工作分解与进度：[`docs/WBS.md`](docs/WBS.md)
- 文档目录与命名规范：[`docs/README.md`](docs/README.md)
- 仓库布局与分层：[`docs/dev/repository-layout.md`](docs/dev/repository-layout.md)
- 检测契约（**改算法前必读**）：[`docs/specs/detection-contract.md`](docs/specs/detection-contract.md)
- 精度与性能基线：[`docs/testing/baselines.md`](docs/testing/baselines.md)
- 验收口径与门禁：[`docs/testing/acceptance.md`](docs/testing/acceptance.md)

## 模块边界（改代码前先确认落点）

依赖方向（**禁止反向**）：`tatr-core ← tatr-engine ← { tatr-cli, tatr-http, 上层集成 }`

| 层 | 路径 | 允许 | 禁止 |
|---|---|---|---|
| 算法 | `crates/tatr-core` | 几何、张量约定、DETR 解码、NMS、类型 | ONNX Runtime、文件/网络 IO、日志、读环境变量；**必须能无模型单测** |
| 推理 | `crates/tatr-engine` | ort 调用、模型获取与 sha256 校验、ORT 会话、图像解码 | 业务规则；HTTP/CLI 关注点 |
| 应用 | `apps/tatr-cli`、`apps/tatr-http` | 参数解析、路由、并发闸门、错误映射 | 复刻算法（一律调下层） |

**判据**：算法改动 → `tatr-core`；推理/资源改动 → `tatr-engine`；接口形态改动 → `apps/`。

## 两条不可违反的契约（都源自真实缺陷）

1. **解码必须用含 no-object 的 softmax**（不是按类 sigmoid）。见
   [`docs/decisions/0002-detr-softmax-decoding.md`](docs/decisions/0002-detr-softmax-decoding.md)。
2. **缩放必须抗混叠**（Triangle，不是 2-tap 双线性）。见
   [`docs/decisions/0003-antialiased-resize.md`](docs/decisions/0003-antialiased-resize.md)。

两者都有 in-repo 回归测试；**不要"简化"它们**——测试会拦住你。

## 验证命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # 28 项
cargo build --release --workspace

# 基线复跑（需要 TableBank parquet）
python3 tools/bench/prepare_tablebank.py --parquet test-0.parquet --n 300 --out /tmp/tb300
python3 -c "import json;print(' '.join(g['file'] for g in json.load(open('/tmp/tb300/gt.json'))))" > /tmp/files.txt
./target/release/tatr detect --out /tmp/pred.json $(cat /tmp/files.txt)
python3 tools/bench/score.py /tmp/tb300/gt.json /tmp/pred.json --area-metric     # 期望 F1 0.786
```

## 常用环境变量

| 变量 | 作用 |
|---|---|
| `TATR_MODEL` | 本地模型路径（服务）；CLI 用 `--model` |
| `TATR_THREADS` | ORT intra-op 线程（默认物理核数；6 线程即饱和） |
| `TATR_BIND` | 服务监听地址，默认 `0.0.0.0:8080` |
| `TATR_MAX_CONCURRENCY` | 服务并发上限，默认 4 |
| `RUST_LOG` | 日志过滤 |

## 已知盲区（不要据此下结论）

- **表格密集页**（单页 10–40 表）缺少评测轴：TableBank 每页 ≤5 表。
  合成密集页是 OOD 样本（模型塌缩成单框），**不能**作为判据。
- 域差未量化：模型训于科学 PDF，其他域的掉点程度需自建标尺测量。
