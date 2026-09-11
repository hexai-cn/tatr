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
    # PowerShell 在所有 windows runner 上可用；bsdtar/GNU tar 对 zip 的支持不一致
    powershell.exe -NoProfile -Command \
        "Compress-Archive -Path '$(cygpath -w "$STAGE")\\*' -DestinationPath '$(cygpath -w "$ARCHIVE")' -Force"
else
    ARCHIVE="${OUT_DIR}/tatr-${VERSION}-${TRIPLE}.tar.gz"
    tar -czf "$ARCHIVE" -C "$STAGE" .
fi

echo "已打包: $ARCHIVE"
ls -la "$ARCHIVE"
