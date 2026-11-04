import { Palette } from "./palette.js";
import { AssetPackage, Sheet } from "./sheet.js";
import { Level } from "./level.js";

function baseUrl(base) { return base.endsWith("/") ? base : `${base}/`; }
async function json(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.json();
}

async function rgba(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const bitmap = await createImageBitmap(await response.blob());
  try {
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    context.drawImage(bitmap, 0, 0);
    return {
      width: bitmap.width,
      height: bitmap.height,
      bytes: context.getImageData(0, 0, bitmap.width, bitmap.height).data,
    };
  } finally {
    bitmap.close();
  }
}

export async function loadPackage(base) {
  const root = new URL(baseUrl(base), location.href);
  const document = await json(new URL("asset-package.json", root));
  const palette = new Palette(await json(new URL(document.palette.path, root)));
  const sheets = new Map();
  for (const entry of document.sheets ?? []) {
    const url = new URL(entry.path, root);
    const sheetDocument = await json(url);
    const imageUrl = new URL(sheetDocument.meta.image, url);
    const image = await rgba(imageUrl);
    sheets.set(entry.id, new Sheet(entry.id, sheetDocument, image.bytes, image.width, image.height));
  }
  if (document.visual?.effective_style?.path) {
    const style = await json(new URL(document.visual.effective_style.path, root));
    document.visual.effective_style = { ...document.visual.effective_style, ...style };
  }
  let levelFiles = document.levels?.files;
  if (!levelFiles && document.levels?.path) {
    const manifest = await json(new URL(`${document.levels.path}/index.json`, root));
    levelFiles = manifest.files;
  }
  const known = new Set((document.assets ?? []).map((asset) => asset.id));
  const levels = [];
  for (const file of levelFiles ?? []) {
    const path = file.includes("/") ? file : `${document.levels.path}/${file}`;
    levels.push(new Level(await json(new URL(path, root)), known));
  }
  return new AssetPackage(document, palette, sheets, levels);
}
