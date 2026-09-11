---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: 引擎 API 契约——模型来源、会话配置、检测接口与错误语义
---

# Spec — 引擎 API（`tatr-engine`）

## 1. 构造

```rust
let engine = TableDetectionEngine::new(EngineOptions {
    model: ModelSource::default(),          // 默认：缓存 + 下载 + sha256
    session: SessionConfig::default(),
    apply_env_overrides: false,
})?;
```

| 阶段 | 行为 |
|---|---|
| 模型解析 | `LocalFile` 校验存在；`Download` 查缓存 → 命中且哈希通过则复用 → 否则下载 → 校验 → 不通过则删除并报错 |
| 会话建立 | 加载 ONNX、设置优化级别与线程数 |
| **契约校验** | 必须存在输入 `pixel_values`/`pixel_mask` 与输出 `logits`/`pred_boxes`，否则**构造期失败** |

**设计原则：坏模型不进服务。** 全部失败都在构造期暴露，运行期只做推理。

## 2. 模型来源

| 变体 | 语义 |
|---|---|
| `ModelSource::LocalFile(path)` | 直接使用，不校验哈希（离线/内网分发） |
| `ModelSource::Download{url, sha256, cache_dir}` | 缓存优先；校验失败则删除并重下；`cache_dir=None` 用平台缓存目录（`~/.cache/tatr`） |

默认：`DEFAULT_MODEL_URL` + `DEFAULT_MODEL_SHA256`（见 `crates/tatr-engine/src/model.rs`）。

## 3. 线程配置

优先级：`SessionConfig::intra_threads` > 环境变量 `TATR_THREADS` > 物理核数。

实测（Apple M5 Pro，800px）：1 线程 248ms → 6 线程 159ms → 18 线程 160ms
（**6 线程饱和**，继续加线程无收益）。因此默认取物理核数即可，容器内建议显式设小值。

## 4. 检测接口

```rust
fn detect(&self, image: &RasterImage, cfg: &DetectorConfig) -> Result<DetectionResult, TatrEngineError>
fn detect_bytes(&self, bytes: &[u8], cfg: &DetectorConfig) -> Result<DetectionResult, TatrEngineError>
fn decode_image_file(path: &Path) -> Result<RasterImage, TatrError>
```

`DetectionResult` 字段：`width`/`height`（原图）、`input_size`（缩放后）、
`detections`（按分数降序）、`elapsed_ms`（含预处理+推理+解码）。

## 5. 并发语义

`ort::Session::run` 需 `&mut`，引擎内部以 `Mutex` 串行化。**这是有意设计**：
CPU 场景并行推理会线程超订。需要并发请创建多个引擎实例（多进程/多实例）。

`TableDetectionEngine` 因此是 `Send + Sync`（`Mutex` 保护），可放 `Arc` 共享。

## 6. 错误语义

| 错误 | 触发 | 可重试 |
|---|---|---|
| `Core(InvalidImage)` | 图像解码失败/缓冲长度不符 | 否 |
| `Core(InvalidConfig)` | 配置非法（阈值越界、尺寸为 0、long<short） | 否 |
| `Core(UnexpectedOutputShape)` | 模型输出形状与约定不符 | 否（模型错） |
| `Session(_)` | ONNX 会话创建失败 | 否 |
| `Contract(_)` | 模型 IO 名称/形状不符 | 否（模型错） |
| `Inference(_)` | 运行期推理失败 | 是 |

## 7. 无隐性全局状态

除 `TATR_THREADS`（仅当 `SessionConfig::intra_threads` 为 `None`）与可选的
`TATR_NMS_IOU`（仅当 `apply_env_overrides=true`）外，引擎不读环境变量。
默认不启用 `apply_env_overrides`。
