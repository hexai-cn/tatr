#!/usr/bin/env bash
# 打包发布制品。
#
# 用法: tools/release/package.sh <version> <target-triple> <bin-dir> <out-dir>
#   version      版本号，如 0.1.0（不含前缀 v）
#   target-triple 目标三元组，如 aarch64-apple-darwin（用于命名与自检）
#   bin-dir      已构建的可执行文件目录（含 tatr 与 tatr-http）
#   out-dir      输出目录
#
# 产物: <out-dir>/tatr-<version>-<triple>.tar.gz（windows 为 .zip）
#       归档内直接是可执行文件与文档（无顶层目录）
set -euo pipefail

VERSION="${1:?version}"
TRIPLE="${2:?target triple}"
BIN_DIR="${3:?bin dir}"
OUT_DIR="${4:?out dir}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXE=""
if [[ "$TRIPLE" == *windows* ]]; then
    EXE=".exe"
fi

for f in "tatr${EXE}" "tatr-http${EXE}"; do
    if [[ ! -f "${BIN_DIR}/${f}" ]]; then
        echo "错误: 缺少二进制 ${BIN_DIR}/${f}" >&2
        exit 1
    fi
done

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/docs"

cp "${BIN_DIR}/tatr${EXE}" "${BIN_DIR}/tatr-http${EXE}" "$STAGE/"
cp "$ROOT/README.md" "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$STAGE/"
cp "$ROOT/docs/guides/quickstart.md" "$STAGE/docs/quickstart.md"
cp "$ROOT/docs/testing/baselines.md" "$STAGE/docs/baselines.md"

mkdir -p "$OUT_DIR"
if [[ "$TRIPLE" == *windows* ]]; then
    ARCHIVE="${OUT_DIR}/tatr-${VERSION}-${TRIPLE}.zip"
    # 不用 PowerShell 的 Compress-Archive：它写入**反斜杠**路径分隔符，
    # Linux/macOS 解压会得到名为 `docs\baselines.md` 的文件（zip 规范要求正斜杠）。
    # 7-Zip 是 GitHub Windows runner 预装工具，产出规范 zip。
    if command -v 7z >/dev/null 2>&1; then
        ( cd "$STAGE" && 7z a -tzip "$ARCHIVE" ./* >/dev/null )
    elif [[ -x "/c/Program Files/7-Zip/7z.exe" ]]; then
        ( cd "$STAGE" && "/c/Program Files/7-Zip/7z.exe" a -tzip "$ARCHIVE" ./* >/dev/null )
    else
        echo "错误: 需要 7z 才能产出路径分隔符规范的 zip" >&2
        echo "      （不要退回 Compress-Archive：它会写反斜杠，破坏跨平台解压）" >&2
        exit 1
    fi
else
    ARCHIVE="${OUT_DIR}/tatr-${VERSION}-${TRIPLE}.tar.gz"
    tar -czf "$ARCHIVE" -C "$STAGE" .
fi

echo "已打包: $ARCHIVE"
ls -la "$ARCHIVE"
