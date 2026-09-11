---
status: active
created: 2026-09-11
last_updated: 2026-09-11
summary: CI/CD 流水线：触发方式、平台矩阵、制品与发布流程
---

# CI/CD

两条 GitHub Actions 流水线：`.github/workflows/ci.yml`（门禁）与
`.github/workflows/release.yml`（发布）。

## 1. CI（`ci.yml`）

触发：`push` 到 `main`、`pull_request`、手动 `workflow_dispatch`。

| Job | Runner | 内容 |
|---|---|---|
| `lint` | `ubuntu-latest` | `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` |
| `test` | 四平台矩阵 | `cargo test --workspace --locked` + release 构建 + CLI 冒烟 |

**平台矩阵**（`fail-fast: false`，单平台失败不掩盖其他平台结果）：

| 目标三元组 | Runner | 说明 |
|---|---|---|
| `aarch64-apple-darwin` | `macos-14` | macOS arm64（Apple Silicon） |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | Linux amd64 |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | Linux arm64（GitHub 托管 arm64 runner） |
| `x86_64-pc-windows-msvc` | `windows-latest` | Windows amd64 |

lint 与 test 分离：格式/静态检查平台无关，单独跑可快速反馈，也省下三份重复时间。

**冒烟范围说明**：CI 中**不做**模型下载与真实推理——那需要网络与 110 MB 传输，
不适合每次 push 执行。冒烟只验证二进制可产出且 CLI 参数解析正常。
真实推理的验证在本地基线与人工发布流程中完成（见 `docs/testing/baselines.md`）。

## 2. Release（`release.yml`）

触发：推送 `v*` tag（如 `v0.1.0`）。

```
verify (lint + test)
   └─ build × 4 平台（矩阵）
         └─ publish（汇总 SHA256SUMS → 创建 Release）
```

| Job | 作用 |
|---|---|
| `verify` | 发布前门禁：fmt / clippy / test 全绿，避免坏制品流出 |
| `build` | 四平台 `cargo build --release --locked --target <triple>`；**断言 tag 版本 == Cargo.toml 版本**；运行 `tatr --version` 自检；调用 `tools/release/package.sh` 打包 |
| `publish` | 下载全部矩阵制品 → 断言四平台齐全 → 生成 `SHA256SUMS` → 创建 Release |

### 制品

| 平台 | 文件 |
|---|---|
| macOS arm64 | `tatr-0.1.0-aarch64-apple-darwin.tar.gz` |
| Linux amd64 | `tatr-0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64 | `tatr-0.1.0-aarch64-unknown-linux-gnu.tar.gz` |
| Windows amd64 | `tatr-0.1.0-x86_64-pc-windows-msvc.zip` |

归档内含 `tatr`（CLI）、`tatr-http`（服务）、两份 LICENSE、`README.md`
与 `docs/{quickstart,baselines}.md`。**不含模型**（模型单独发布，见 `models-v1`）。

### 发布步骤

```bash
# 1) 升版本（Cargo.toml 的 workspace.package.version）
# 2) 提交
git commit -am "chore: bump to 0.1.1"
# 3) 打 tag 并推送（tag 必须与 Cargo.toml 版本一致，否则 build job 失败）
git tag -a v0.1.1 -m "tatr v0.1.1" && git push origin main v0.1.1
```

## 3. 本地复现打包

```bash
cargo build --release
./tools/release/package.sh 0.1.0 aarch64-apple-darwin target/release dist
# → dist/tatr-0.1.0-aarch64-apple-darwin.tar.gz
tar -tzf dist/tatr-0.1.0-aarch64-apple-darwin.tar.gz
```

## 4. 分发的可移植性

`ort` 默认在构建时**静态链接** ONNX Runtime，因此二进制只有系统库依赖
（实测 macOS：libc++/Foundation/CoreML 等，**无 onnxruntime 动态依赖**、
无 python/torch）。发布制品是**单个可执行文件**，无需额外运行时。

## 5. 已知维护点

| 项 | 说明 |
|---|---|
| Actions 版本 | 使用 `@v4` 等主版本标签；GitHub 已提示 Node 20 弃用告警（不影响功能） |
| runner 标签 | `macos-14`（arm64）、`ubuntu-24.04-arm`（arm64）为 GitHub 托管；若标签退役需同步更新矩阵 |
| 缓存 | `Swatinem/rust-cache@v2`，按 target 分键 |
