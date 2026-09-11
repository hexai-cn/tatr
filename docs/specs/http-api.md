---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: HTTP 服务契约——端点、参数、状态码与错误体
---

# Spec — HTTP API（`apps/tatr-http`）

## 1. 端点

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/healthz` | 存活探针，恒 200 |
| GET | `/readyz` | 就绪探针（引擎已加载完成） |
| GET | `/v1/model` | 模型路径、线程数、默认配置、query/类别数 |
| POST | `/v1/detect` | 请求体为图像字节（Content-Type 任意，按内容嗅探格式） |
| POST | `/v1/detect/multipart` | `multipart/form-data`，字段名 `file` |

## 2. 查询参数（检测端点通用）

| 参数 | 默认 | 约束 |
|---|---|---|
| `threshold` | 0.5 | `0..=1`，否则 400 |
| `short_side` | 800 | `>0` |
| `long_side` | 800 | `>= short_side`，否则 400 |
| `drop_rotated` | false | true 时丢弃 `table_rotated` |
| `nms_iou` | 1.0 | `1.0` 表示关闭 NMS |

## 3. 成功响应

```json
{
  "source_content_type": "image/png",
  "total_ms": 54.22,
  "result": {
    "width": 596, "height": 842,
    "input_size": [566, 800],
    "elapsed_ms": 52.91,
    "detections": [
      { "bbox": {"x":88.79,"y":87.19,"w":447.89,"h":572.44},
        "score": 0.9983, "label": "table" }
    ]
  }
}
```

`label ∈ {table, table_rotated}`；`detections` 按 `score` 降序。

## 4. 错误响应

统一体： `{"error": "<code>", "detail": "<人类可读>"}`。

| HTTP | `error` | 触发 |
|---|---|---|
| 400 | `empty_body` | 空请求体 |
| 400 | `invalid_image` | 无法解码为图像 |
| 400 | `invalid_input` | 参数/配置非法 |
| 400 | `missing_file` / `bad_multipart` | multipart 缺字段或格式错 |
| 413 | — | 请求体超过 32 MiB（由 `DefaultBodyLimit` 返回） |
| 503 | `inference_failed` | 运行期推理异常 |
| 503 | `overloaded` | 服务正在关闭（信号量已关闭） |

## 5. 运行时约束

| 约束 | 值 | 说明 |
|---|---|---|
| 请求体上限 | 32 MiB | 页面渲染图远小于此值 |
| 并发 | `TATR_MAX_CONCURRENCY`（默认 4） | 超出排队；闸门防止内存无界增长 |
| 优雅退出 | SIGINT / SIGTERM | 停止接收新请求，等待在途请求完成 |

## 6. 环境变量

| 变量 | 默认 | 说明 |
|---|---|---|
| `TATR_BIND` | `0.0.0.0:8080` | 监听地址 |
| `TATR_MODEL` | —（走下载） | 本地模型路径 |
| `TATR_THREADS` | 物理核数 | ORT intra-op 线程 |
| `TATR_MAX_CONCURRENCY` | 4 | 并发上限 |
| `RUST_LOG` | `tatr_http=info,tower_http=info` | 日志过滤 |
