# tatr — Rust 表格检测（Table Transformer / DETR）

**纯 Rust、CPU-first 的表格区域检测**：输入文档页面图像，输出 N 个表格外接框。
多 crate 设计——可作**引擎嵌入上层进程**，也可作为**独立 HTTP 服务**发布。

* 模型：`microsoft/table-transformer-detection`（DETR + ResNet-18，PubTables-1M 预训练，**MIT**）
* 运行时：ONNX Runtime（`ort`），**无 Python / torch 依赖**
* 二进制：CLI + HTTP 服务 ≈ 26 MB；模型 110 MB（Release 资产，不入 git）

## 快速开始

```bash
cargo build --release

# CLI（首次运行自动下载模型并校验 sha256）
./target/release/tatr detect page.png --out result.json

# 定位可视化：为每张输入写出带框标注图（绿=table，橙=table_rotated）
./target/release/tatr detect --viz /tmp/viz page1.png page2.png

# HTTP 服务
TATR_MODEL=~/.cache/tatr/table_detector.onnx TATR_BIND=0.0.0.0:8080 \
  ./target/release/tatr-http
curl -s -X POST --data-binary @page.png localhost:8080/v1/detect | jq '.result.detections'
```

```json
{ "bbox": {"x": 88.8, "y": 87.2, "w": 447.9, "h": 572.4},
  "score": 0.998, "label": "table" }
```

详见 [`docs/guides/quickstart.md`](docs/guides/quickstart.md)。

## 作为库使用（上层集成）

```rust
use tatr_core::DetectorConfig;
use tatr_engine::{EngineOptions, TableDetectionEngine};

let engine = TableDetectionEngine::new(EngineOptions::default())?;  // 坏模型在启动期即失败
let img = tatr_engine::decode_image_file("page.png".as_ref())?;
let out = engine.detect(&img, &DetectorConfig::default())?;
for d in out.detections {
    println!("{:?} {:.3} {:?}", d.bbox, d.score, d.label);
}
```

## 指标与性能（实测）

**TableBank test-0 前 300 页 / 375 表**（IoU ≥ 0.5 计数口径）：

| 配置 | P | R | F1 | 面积口径 F1 |
|---|---|---|---|---|
| 仅 `table` | 0.763 | 0.797 | **0.780** | 0.794 |
| 含 `table_rotated`（**默认**） | 0.765 | 0.808 | **0.786** | 0.807 |

**CPU 性能**（Apple M5 Pro，release，800px）：

| 指标 | 值 |
|---|---|
| 单页延迟 | **46 ms**（8 线程，常驻进程）；159 ms（6 线程饱和点） |
| 批量吞吐 | 300 页 **15.1 s** ≈ 20 页/秒 |
| 线程扩展 | 1→248ms，4→170ms，**6→159ms（饱和）** |

> ⚠ 模型训于 PubTables-1M（科学 PDF）。跨域（Word/LaTeX/表单）会掉点；
> 且 TableBank 每页 ≤5 表，**测不到**单页 10–40 表的表单场景——这是已知盲区，
> 见 [`docs/PRD.md`](docs/PRD.md) §7。

复跑：`docs/testing/baselines.md`；门禁：`cargo test --workspace`（28 项）。

## 仓库结构

```
crates/tatr-core     纯算法：类型 / 预处理 / DETR 解码 / NMS（可无模型单测）
crates/tatr-engine   推理与资源：模型获取(sha256) / ORT 会话 / 检测门面
apps/tatr-cli        命令行
apps/tatr-http       axum 服务（探针 / 并发闸门 / 优雅退出）
tools/bench          评测脚本（基线复跑）
docs/                规范文档（PRD / DESIGN / WBS / specs / testing / ADR / guides / runbooks）
```

## 两条不可违反的契约（源自真实缺陷）

1. **解码用含 no-object 的 softmax**，不是按类 sigmoid——
   否则 no-object 占优时会产出"高置信度误检"（[ADR-0002](docs/decisions/0002-detr-softmax-decoding.md)）。
2. **缩放必须抗混叠**，2-tap 双线性会混叠并让指标失真
   （[ADR-0003](docs/decisions/0003-antialiased-resize.md)）。

两者都有回归测试钉住，不要"简化"。

## 许可

- 本仓库代码：MIT OR Apache-2.0
- 模型权重：MIT（`microsoft/table-transformer-detection`）——可商用、可再分发
