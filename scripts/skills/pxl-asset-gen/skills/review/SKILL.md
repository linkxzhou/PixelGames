---
name: pxl-asset-gen-review
description: Run mechanical validate plus by-eye checks; report evidence before calling art done. Use before export and when art looks off.
---

# Review

"Done" is a claim — earn it.

## Procedure

### 0. Cold read

Open the preview PNG (scaled). One sentence: what a stranger sees. Three first eye-stops. If the sentence misses the brief's subject, fail review regardless of QA exit codes.

### 1. Mechanical

```bash
$PXL validate art.pxl.json --json
$PXL inspect art.pxl.json
# ase:
$PXL ase inspect file.aseprite --json
```

Exit 5 from step-300 = QA fail. Treat skipped checks as **NOT_ASSESSED**, never PASS.

### 2. By-eye checklist

Follow [../../references/review-checklist.md](../../references/review-checklist.md): silhouette, cluster noise, pillow shading, outline breaks, palette sprawl, animation pops.

### 3. Report

List concrete defects with file paths and which fix skill applies. Do not declare success without validate + cold read.
