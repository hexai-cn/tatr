#!/usr/bin/env python3
"""按 IoU 阈值计数口径评测 tatr 输出。

用法:
  python3 tools/bench/score.py gt.json pred.json [--iou 0.5] [--label table|all]
"""
import argparse
import json
import sys


def iou(a, b) -> float:
    ix = max(0.0, min(a[0] + a[2], b[0] + b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[1] + a[3], b[1] + b[3]) - max(a[1], b[1]))
    inter = ix * iy
    union = a[2] * a[3] + b[2] * b[3] - inter
    return inter / union if union > 0 else 0.0


def area_metric(gts, preds_by_file) -> tuple[float, float, float]:
    """TableBank 官方面积口径：Σ交/Σ预测面积、Σ交/ΣGT面积。"""
    inter = pred_area = gt_area = 0.0
    for g in gts:
        w, h = g["size"]
        gt_mask = set()
        for x, y, bw, bh in g["gt"]:
            for yy in range(max(0, y), min(h, y + bh)):
                for xx in range(max(0, x), min(w, x + bw)):
                    gt_mask.add((xx, yy))
        pred_mask = set()
        for d in preds_by_file.get(g["file"], []):
            b = d["bbox"]
            for yy in range(max(0, int(b["y"])), min(h, int(b["y"] + b["h"]))):
                for xx in range(max(0, int(b["x"])), min(w, int(b["x"] + b["w"]))):
                    pred_mask.add((xx, yy))
        inter += len(gt_mask & pred_mask)
        gt_area += len(gt_mask)
        pred_area += len(pred_mask)
    p = inter / pred_area if pred_area else 0.0
    r = inter / gt_area if gt_area else 0.0
    f = 2 * p * r / (p + r) if p + r else 0.0
    return p, r, f


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("gt")
    ap.add_argument("pred")
    ap.add_argument("--iou", type=float, default=0.5)
    ap.add_argument("--label", default="all", choices=["all", "table"])
    ap.add_argument("--area-metric", action="store_true", help="额外计算 TableBank 面积口径")
    args = ap.parse_args()

    gts = json.load(open(args.gt))
    preds_by_file: dict[str, list] = {}
    for r in json.load(open(args.pred))["results"]:
        det = r["detections"]
        if args.label == "table":
            det = [d for d in det if d["label"] == "table"]
        preds_by_file[r["image"]] = det

    tp = fp = fn = 0
    ious: list[float] = []
    for g in gts:
        det = preds_by_file.get(g["file"], [])
        pred = [[d["bbox"]["x"], d["bbox"]["y"], d["bbox"]["w"], d["bbox"]["h"]] for d in det]
        for t in g["gt"]:
            hits = [iou(t, p) for p in pred]
            if any(v >= args.iou for v in hits):
                tp += 1
                ious.append(max(hits))
            else:
                fn += 1
        for p in pred:
            if not any(iou(t, p) >= args.iou for t in g["gt"]):
                fp += 1

    p = tp / (tp + fp) if tp + fp else 0.0
    r = tp / (tp + fn) if tp + fn else 0.0
    f = 2 * p * r / (p + r) if p + r else 0.0
    hit = sum(ious) / len(ious) if ious else 0.0
    print(f"pages={len(gts)} label={args.label} IoU>={args.iou}")
    print(f"  P={p:.3f} R={r:.3f} F1={f:.3f}  (TP{tp} FP{fp} FN{fn})  hitIoU={hit:.3f}")
    if args.area_metric:
        ap_, ar_, af_ = area_metric(gts, preds_by_file)
        print(f"  面积口径: P={ap_:.3f} R={ar_:.3f} F1={af_:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
