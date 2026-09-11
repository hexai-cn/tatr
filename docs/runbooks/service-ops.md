---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: tatr-http 运维手册：探针、容量、故障处置
---

# Runbook — tatr-http 运维

每条针对一个**具体症状**。命令中的 `$SVC` 为服务基址（如 `http://127.0.0.1:8080`）。

## 1. 启动即失败

| 症状 | 原因 | 处置 |
|---|---|---|
| 日志出现 `初始化检测引擎失败` + `模型文件不存在` | `TATR_MODEL` 指向不存在的路径 | 检查路径；或去掉 `TATR_MODEL` 走缓存/下载 |
| `下载的模型 sha256 与期望不符` | 下载损坏 / 上游被替换 | 删除缓存文件重试；确认网络代理未改写内容；必要时改用离线模型 |
| `模型缺少输入 pixel_values` / `缺少输出 logits` | 用了错误的 ONNX（非 tatr 导出的 TATR 检测模型） | 换回 `table_detector.onnx`（sha256 见 `models/README.md`） |
| `绑定 0.0.0.0:8080 失败` | 端口被占用 | 改 `TATR_BIND`，或查占用：`lsof -iTCP:8080 -sTCP:LISTEN` |

> 设计约束：**坏模型不进服务**。所有模型问题都在启动期暴露，不会等到第一个请求。

## 2. 就绪探针

```bash
curl -fsS $SVC/healthz   # 进程活着
curl -fsS $SVC/readyz    # 引擎已加载（能构造出 State 即就绪）
curl -s $SVC/v1/model | jq .   # 模型路径 / 线程 / 默认配置
```

K8s 建议：`livenessProbe` → `/healthz`；`readinessProbe` → `/readyz`；
`startupProbe` → `/readyz`（首启需加载 110 MB 模型，`failureThreshold` 给足）。

## 3. 延迟高

| 观察 | 判断 | 处置 |
|---|---|---|
| `/v1/model` 的 `threads` 远大于容器 CPU 限额 | 线程超订 | 设 `TATR_THREADS` = 可用核数 |
| `threads` 已 ≥6 仍慢 | 正常（6 线程即饱和） | 增加**实例数**，不要加线程 |
| 单页 > 1s | 输入尺寸过大 | 降 `short_side`/`long_side`（注意会掉召回，见基线） |
| 偶发慢 | 并发排队 | 增大 `TATR_MAX_CONCURRENCY` 或加实例 |

```bash
# 实测单页耗时
time curl -s -o /dev/null -X POST --data-binary @page.png $SVC/v1/detect
# 看响应内耗时
curl -s -X POST --data-binary @page.png $SVC/v1/detect | jq '.total_ms, .result.elapsed_ms'
```

## 4. 请求被拒

| HTTP | `error` | 处置 |
|---|---|---|
| 400 `invalid_image` | 上传的不是图像或已损坏 | 检查上游产出的图片字节；确认 Content-Type 不参与格式判断（按内容嗅探） |
| 400 `invalid_input` | 参数非法（阈值越界 / `long_side < short_side`） | 修正调用参数 |
| 400 `missing_file` | multipart 用了别的字段名 | 字段名必须是 `file` |
| 413 | 请求体 > 32 MiB | 压缩或降分辨率后再传 |
| 503 `inference_failed` | 推理异常 | 可重试；持续出现则抓日志与输入样本 |
| 503 `overloaded` | 服务正在关闭 | 停止发新请求，等待实例重启 |

## 5. 吞吐不足

```bash
# 观察队列压力：并发压测
for i in $(seq 1 16); do curl -s -o /dev/null -X POST --data-binary @page.png $SVC/v1/detect & done; wait
```

- 先确认 `TATR_MAX_CONCURRENCY` 与实际吞吐匹配（排队过深只增延迟）。
- 水平扩展：多实例 + LB。**同一进程内并行推理无收益**（Mutex 串行化是刻意的）。

## 6. 优雅退出

服务响应 `SIGINT` / `SIGTERM`：停止接收新连接，等待在途请求完成。
容器 `terminationGracePeriodSeconds` 建议 ≥ 30s（单页最长约 0.2s + 排队）。

```bash
docker stop -t 30 <container>   # 或 kubectl delete（默认先 SIGTERM）
```

## 7. 版本与模型升级

1. 先更新模型：`tatr model fetch`（或替换镜像内 `/opt/tatr/table_detector.onnx`）；
2. 校验：`shasum -a 256` 对比 `models/README.md`；
3. 滚动重启；
4. 重启后确认 `/v1/model` 的模型路径，并用固定样本页对比检测数（回归基线见
   `docs/testing/baselines.md`）。
