---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 五分钟上手：构建、CLI 检测、作为库集成、起 HTTP 服务
---

# 快速上手

## 1. 构建

```bash
cargo build --release
# 产物：target/release/tatr（CLI）、target/release/tatr-http（服务）
```

首次构建会编译 ONNX Runtime 绑定，耗时数分钟。

## 2. CLI 检测

```bash
# 首次运行会下载模型到 ~/.cache/tatr/ 并校验 sha256
./target/release/tatr detect page.png

# 多页 + 写文件
./target/release/tatr detect --out result.json page1.png page2.png

# 只用已下载的本地模型（离线/容器内预置）
./target/release/tatr detect --model /opt/tatr/table_detector.onnx page.png
```

输出（stdout 为**纯 JSON**，日志走 stderr，可直接 `| jq`）：

```json
{
  "engine": { "threads": 8, "model": "/opt/tatr/table_detector.onnx" },
  "results": [{
    "image": "page.png", "width": 596, "height": 842,
    "input_size": [566, 800], "elapsed_ms": 46.2,
    "detections": [
      { "bbox": {"x":88.8,"y":87.2,"w":447.9,"h":572.4},
        "score": 0.998, "label": "table" }
    ]
  }]
}
```

常用开关：

| 开关 | 作用 |
|---|---|
| `--threshold 0.3` | 降低阈值换召回（会增误检） |
| `--viz out/` | 为每张输入写出标注框 PNG（`out/<名字>.viz.png`），用于人工核对定位效果 |
| `--drop-rotated` | 丢弃 `table_rotated` 类（**默认保留**，实测更好） |
| `--threads N` | ORT 线程数（默认物理核数） |
| `--short-side/--long-side` | 输入缩放（默认 800/800） |

### 定位可视化

```bash
./target/release/tatr detect --viz /tmp/viz page1.png page2.png
# /tmp/viz/page1.viz.png、/tmp/viz/page2.viz.png
```

标注图画在原图副本上，边框颜色区分类别：**绿 = `table`**、**橙 = `table_rotated`**；
线宽随页面尺寸自适应（2–8 px）。JSON 仍是唯一的结果事实来源（stdout 保持纯 JSON），
标注图只做可视化，不参与指标计算。

模型管理：

```bash
tatr model info                 # 模型路径、sha256、线程
tatr model fetch                # 预下载到缓存
tatr model fetch --cache-dir /opt/tatr/models
```

## 3. 作为库集成（进程内，无子进程）

```toml
[dependencies]
tatr-core = { git = "https://github.com/hexai-cn/tatr", tag = "v0.1.0" }
tatr-engine = { git = "https://github.com/hexai-cn/tatr", tag = "v0.1.0" }
```

```rust
use tatr_core::{DetectorConfig, RasterImage};
use tatr_engine::{EngineOptions, TableDetectionEngine};

let engine = TableDetectionEngine::new(EngineOptions::default())?;   // 启动期即失败
let img = tatr_engine::decode_image_file("page.png".as_ref())?;
let cfg = DetectorConfig::default();                                  // 阈值 0.5 / 800px / 含 rotated
for d in engine.detect(&img, &cfg)?.detections {
    println!("{:?} {:.3} {:?}", d.bbox, d.score, d.label);
}
```

`TableDetectionEngine` 是 `Send + Sync`，放 `Arc` 后可在多任务间共享
（内部 `Mutex` 串行化推理，CPU 场景是期望行为）。

## 4. HTTP 服务

```bash
# 本地模型（推荐生产）
TATR_MODEL=/opt/tatr/table_detector.onnx TATR_BIND=0.0.0.0:8080 \
  ./target/release/tatr-http

curl -s localhost:8080/healthz
curl -s -X POST --data-binary @page.png localhost:8080/v1/detect | jq '.result.detections'
curl -s -X POST -F file=@page.png 'localhost:8080/v1/detect/multipart?threshold=0.3' | jq .
```

## 5. 验证

```bash
cargo test --workspace       # 28 项
cargo clippy --workspace --all-targets -- -D warnings
```
