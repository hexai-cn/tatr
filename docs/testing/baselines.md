---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: TableBank 精度基线与 CPU 性能基线（含精确复跑方法）
---

# 基线

> 复跑环境：Apple M5 Pro（18 逻辑核）/ macOS 25.6 / rustc 1.95.0 / ort 2.0.0-rc.13。
> 模型：`table_detector.onnx`，sha256 `cdee2c25b48cfe287703d41b9314a7b458ec8dd757815d73c133f90a2dcdab49`。

## 1. 精度基线（TableBank test-0，前 300 页 / 375 表）

评测口径：IoU ≥ 0.5 计数式 P/R/F1（`docs/testing/acceptance.md` §2）。
配置：`threshold=0.5, short_side=800, long_side=800, nms_iou=1.0`。

| 配置 | P | R | **F1** | TP | FP | FN | 命中 IoU | 面积口径 F1 |
|---|---|---|---|---|---|---|---|---|
| 仅 `table`（`--drop-rotated`） | 0.763 | 0.797 | **0.780** | 299 | 93 | 76 | 0.854 | 0.794 |
| 含 `table_rotated`（**默认**） | 0.765 | 0.808 | **0.786** | 303 | 93 | 72 | 0.854 | 0.807 |

**结论**：默认开启 rotated 类带来 **+0.006 F1 且误检数不变**（+4 TP / −4 FN）。
`table_rotated` 不是罕见角落——在表格密集/OOD 页上，模型倾向于把整页表格判为该类。

### 参考：全量 test split（4500 页 / 5088 表）

| 指标口径 | 值 |
|---|---|
| IoU@0.5 计数式 F1 | 0.778 |
| TableBank 官方面积口径 F1 | 0.793 |
| COCO AP50 | 0.694 |

> 官方榜单（Faster R-CNN X101，Word+LaTeX 全监督）面积口径 F1 = 0.9559。
> 本仓库为**零训练**迁移，与监督基线差约 16 个点。

### 复跑

```bash
# 1) 准备固定页集合（HuggingFace deepcopy/TableBank-Detection 的 test-0.parquet）
#    每行一个标注；必须按 image.path 聚合，否则多表格页会被低估
python3 tools/bench/prepare_tablebank.py --parquet test-0.parquet --n 300 --out /tmp/tb300

# 2) 批量检测（模型路径可用 ~/.cache/tatr/ 或显式 --model）
cargo build --release
python3 -c "import json;print(' '.join(g['file'] for g in json.load(open('/tmp/tb300/gt.json'))))" > /tmp/files.txt
./target/release/tatr detect --out /tmp/pred.json $(cat /tmp/files.txt)

# 3) 计算指标
python3 tools/bench/score.py /tmp/tb300/gt.json /tmp/pred.json --area-metric              # 默认（含 rotated）
python3 tools/bench/score.py /tmp/tb300/gt.json /tmp/pred.json --label table --area-metric # 仅 table
```

## 2. CPU 性能基线

### 2.1 单页延迟（800px）

| intra-op 线程 | 延迟 |
|---|---|
| 1 | 248 ms |
| 2 | 206 ms |
| 4 | 170 ms |
| **6** | **159 ms（饱和点）** |
| 8 | 159 ms |
| 12 | 157 ms |
| 18 | 160 ms |

release 构建、含预处理+推理+解码。**6 线程即饱和**，继续加线程无收益甚至略降。

### 2.2 吞吐

| 场景 | 结果 |
|---|---|
| CLI 批量 300 页（8 线程，进程内计时） | median **46 ms/页**，p90 47 ms |
| CLI 批量 300 页（端到端墙钟，含进程启动+模型加载） | **15.1 s**（≈20 页/秒，8 线程） |
| HTTP 单请求（8 线程） | 约 53 ms（含 JSON 编解码） |

> CLI 的端到端墙钟显著优于逐页进程内延迟之和，因为模型只加载一次。
> 生产建议：**常驻进程**（HTTP 服务或长驻集成），而非逐图起进程。

### 2.3 模型与产物

| 项 | 值 |
|---|---|
| 模型大小 | 110.3 MB |
| release 二进制 | ≈26 MB |
| 二进制动态依赖 | 仅系统库（libc++/System）；**无 python / torch** |

### 2.4 复跑

```bash
cargo build --release
for t in 1 2 4 6 8 12 18; do
  TATR_THREADS=$t ./target/release/tatr detect --model "$MODEL" page.png 2>/dev/null \
    | python3 -c "import json,sys;print('$t', json.load(sys.stdin)['results'][0]['elapsed_ms'])"
done
```

## 3. 基线维护规则

| 规则 | 说明 |
|---|---|
| 精度基线变更 | 必须同时更新 `docs/specs/detection-contract.md` 中受影响的语义，并说明变更依据 |
| 性能基线变更 | 注明硬件与线程数；跨机器不可直接比较 |
| 模型更换 | 更新 sha256、本文档全部数字、`crates/tatr-engine/src/model.rs` 的常量 |
