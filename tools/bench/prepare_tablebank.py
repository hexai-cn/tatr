#!/usr/bin/env python3
"""从 TableBank parquet 抽出固定评测页集合。

**关键**：parquet 每行是一个标注，同一页可有多个表格框。必须按 `image.path`
聚合，否则多表格页会被低估召回（历史上曾把 F1 从 0.234 虚高到 0.251）。

用法:
  python3 tools/bench/prepare_tablebank.py --parquet test-0.parquet --n 300 --out /tmp/tb300
"""
import argparse
import json
import os

import cv2
import numpy as np
import pyarrow.parquet as pq


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--parquet", required=True, help="TableBank-Detection 的 test-0.parquet")
    ap.add_argument("--n", type=int, default=300, help="抽取页数")
    ap.add_argument("--skip", type=int, default=0, help="跳过前 N 页（用于切独立验证集）")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    f = pq.ParquetFile(args.parquet)
    pages: dict[str, dict] = {}
    for g in range(f.num_row_groups):
        t = f.read_row_group(g, columns=["image", "bbox"])
        for i in range(t.num_rows):
            im = t["image"][i].as_py()
            rec = pages.setdefault(im["path"], {"bytes": im["bytes"], "gt": []})
            rec["gt"].append(t["bbox"][i].as_py())

    os.makedirs(args.out, exist_ok=True)
    gts, seen = [], 0
    for path, rec in pages.items():
        if seen < args.skip:
            seen += 1
            continue
        if len(gts) >= args.n:
            break
        seen += 1
        img = cv2.imdecode(np.frombuffer(rec["bytes"], np.uint8), cv2.IMREAD_COLOR)
        if img is None:
            continue
        p = os.path.join(args.out, f"{len(gts):03d}.png")
        cv2.imwrite(p, img)
        gts.append({"file": p, "gt": rec["gt"], "size": [img.shape[1], img.shape[0]], "src": path})

    with open(os.path.join(args.out, "gt.json"), "w") as fh:
        json.dump(gts, fh)
    tables = sum(len(g["gt"]) for g in gts)
    print(f"抽取 {len(gts)} 页 / {tables} 表格 → {args.out}")


if __name__ == "__main__":
    main()
