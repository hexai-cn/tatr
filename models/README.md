# 模型目录

模型二进制（`table_detector.onnx`，110 MB）**不入 git**。三种获取方式：

1. **自动下载（推荐）**：直接运行 `tatr detect`，引擎会下载到缓存目录
   （`~/.cache/tatr/`）并校验 sha256。
2. **手动下载**：
   ```bash
   tatr model fetch                 # 下载到默认缓存并校验
   tatr model fetch --cache-dir /opt/tatr/models
   ```
3. **离线分发**：把 `table_detector.onnx` 放到任意路径，用 `--model <path>`
   或环境变量 `TATR_MODEL=<path>` 指定；容器镜像里随镜像分发即可。

## 校验

```bash
shasum -a 256 table_detector.onnx
# cdee2c25b48cfe287703d41b9314a7b458ec8dd757815d73c133f90a2dcdab49
```

哈希同时固化在 `crates/tatr-engine/src/model.rs` 的 `DEFAULT_MODEL_SHA256`，
下载后不匹配会拒绝加载并删除文件。
