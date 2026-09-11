---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: CPU 部署与吞吐调优：线程、实例数、容器与离线模型
---

# CPU 部署与调优

## 1. 结论先行

| 问题 | 答案 |
|---|---|
| 能否纯 CPU 跑 | **能**。无 GPU、无 Python/torch 依赖 |
| 单页多快 | 800px 约 **160 ms**（6 线程饱和点），release 常驻进程约 **46 ms** |
| 怎么扩吞吐 | **多进程/多实例**，不要单进程猛加线程 |
| 单实例上限 | 约 **20 页/秒**（8 线程，批量流水） |

## 2. 线程配置

优先级：`--threads` / `SessionConfig::intra_threads` > `TATR_THREADS` > 物理核数。

实测（M5 Pro，800px，release）：

| 线程 | 1 | 2 | 4 | **6** | 8 | 12 | 18 |
|---|---|---|---|---|---|---|---|
| ms | 248 | 206 | 170 | **159** | 159 | 157 | 160 |

**6 线程即饱和**。容器里若限制 CPU，请显式设 `TATR_THREADS` 等于可用核数，
避免 ORT 起过多线程造成上下文切换开销。

## 3. 并发模型

- 引擎内部用 `Mutex` 串行化推理（`ort::Session::run` 需 `&mut`）。
  这是**刻意设计**：CPU 上并行推理只会线程超订。
- HTTP 层用信号量限制排队（`TATR_MAX_CONCURRENCY`，默认 4），
  超出的请求排队而非无限堆积。
- 需要真正并发时：**多实例**（多进程 / 多容器 + 负载均衡）。

## 4. 容器化

```dockerfile
FROM rust:1.95 AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/tatr /usr/local/bin/tatr
COPY --from=build /src/target/release/tatr-http /usr/local/bin/tatr-http
# 离线模型：随镜像分发，避免运行期下载
COPY models/table_detector.onnx /opt/tatr/table_detector.onnx
ENV TATR_MODEL=/opt/tatr/table_detector.onnx \
    TATR_BIND=0.0.0.0:8080 \
    TATR_THREADS=4 \
    TATR_MAX_CONCURRENCY=4
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
  CMD curl -fsS http://127.0.0.1:8080/readyz || exit 1
ENTRYPOINT ["tatr-http"]
```

要点：镜像内**预置模型**（`TATR_MODEL`），避免运行期下载依赖网络；
`HEALTHCHECK` 用 `/readyz`（能构造出引擎即就绪）。

## 5. 容量估算

单实例吞吐 ≈ `1000 / 单页毫秒 × 并发效率`。以 46 ms/页（release 常驻，批处理）计
约 20 页/秒；若请求间隔大（冷启动缓存未热）按 160 ms 计约 6 页/秒。

示例：10 万页/天 ≈ 1.16 页/秒 → **单实例足够**；峰值 5 万页/小时 ≈ 14 页/秒
→ 建议 2 实例 + 负载均衡，留余量。

## 6. 内存

单进程常驻内存主要是 ONNX Runtime 会话与模型权重（约数百 MB，取决于 ORT 与优化级别）。
`TATR_MAX_CONCURRENCY` 影响的是排队请求持有的图像缓冲，不改变模型占用。

## 7. 离线/内网

三种方式，任选：

1. 镜像内预置 + `TATR_MODEL`（推荐）；
2. `tatr model fetch --cache-dir /opt/tatr/models` 后挂载该目录为缓存；
3. 把 `DEFAULT_MODEL_URL` 指向内网镜像（改 `crates/tatr-engine/src/model.rs`
   或构造时传 `ModelSource::Download{url,...}`）。
