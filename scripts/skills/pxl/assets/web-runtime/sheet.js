function point(value, fallback = { x: 0, y: 0 }) {
  if (Array.isArray(value)) return { x: value[0] ?? fallback.x, y: value[1] ?? fallback.y };
  return { x: value?.x ?? fallback.x, y: value?.y ?? fallback.y };
}

function normalizeFrame(key, value, defaultPivot) {
  const rect = value?.frame ?? value;
  if (!rect || ![rect.x, rect.y, rect.w, rect.h].every(Number.isFinite) || rect.w <= 0 || rect.h <= 0) {
    throw new TypeError(`invalid frame '${key}' rectangle`);
  }
  const duration = value?.duration ?? 100;
  if (!Number.isFinite(duration) || duration <= 0) {
    throw new TypeError(`invalid frame '${key}' duration`);
  }
  const placed = value?.spriteSourceSize ?? { x: 0, y: 0 };
  const source = value?.sourceSize ?? rect;
  return {
    key,
    rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h },
    duration,
    sourceSize: {
      x: placed.x ?? 0,
      y: placed.y ?? 0,
      w: source.w ?? rect.w,
      h: source.h ?? rect.h,
    },
    pivot: point(value?.pivot, defaultPivot),
  };
}

function normalizeTags(tags) {
  if (Array.isArray(tags)) return tags;
  return Object.entries(tags ?? {}).map(([name, value]) => ({ name, ...value }));
}

export class Sheet {
  constructor(id, document, rgba, width, height) {
    this.id = id;
    this.document = document;
    this.rgba = rgba;
    this.width = width ?? document.w ?? document.meta?.size?.w;
    this.height = height ?? document.h ?? document.meta?.size?.h;
    const defaultPivot = point(document.meta?.pxl?.pivot);
    this.frames = new Map(Object.entries(document.frames ?? {}).map(
      ([key, value]) => [key, normalizeFrame(key, value, defaultPivot)],
    ));
    this.order = [...this.frames.keys()];
    this.tags = normalizeTags(document.meta?.frameTags);
    this.texture = 0;
  }

  frame(key) {
    if (typeof key !== "string") throw new TypeError("frame key must be a string");
    return this.frames.get(key);
  }

  animInfo(sprite, anim) {
    return Object.values(this.document.meta?.pxl?.anims ?? {}).find(
      (entry) => entry.asset === sprite && entry.base === anim,
    );
  }

  animFrames(sprite, anim, dir = "N") {
    const declared = this.animInfo(sprite, anim)?.dirs?.[dir]
      ?? this.animInfo(sprite, anim)?.dirs?._;
    if (declared?.length) return declared.map((key) => this.frame(key));
    for (const name of [`${anim}_${dir}`, anim]) {
      const tag = this.tags.find((candidate) => candidate.name === name);
      if (tag) {
        const frames = this.order.slice(tag.from, tag.to + 1).map((key) => this.frame(key));
        if (frames.length) return frames;
      }
    }
    const prefix = `${sprite}_${anim}_${dir}_`;
    return this.order.filter((key) => key.startsWith(prefix)).map((key) => this.frame(key));
  }

  firstFrame(sprite, anim, dir = "N") {
    return this.animFrames(sprite, anim, dir)[0];
  }
}

export class AssetPackage {
  constructor(document, palette, sheets, levels) {
    Object.assign(this, document);
    this.palette = palette;
    this.sheets = sheets;
    this.levels = levels;
    this.assets = document.assets ?? [];
  }

  sheetFor(assetId) {
    const asset = this.assets.find((candidate) => candidate.id === assetId);
    return asset ? this.sheets.get(asset.sheet) : undefined;
  }

  anim(assetId, anim, dir = "N", options = {}) {
    const sheet = this.sheetFor(assetId);
    const frames = sheet?.animFrames(assetId, anim, dir) ?? [];
    if (!frames.length) return {};
    const info = sheet.animInfo(assetId, anim);
    const looped = options.loop ?? info?.loop ?? true;
    const hold = options.hold ?? true;
    const time = options.time ?? 0;
    if (!Number.isFinite(time) || time < 0) throw new RangeError("animation time must be finite and non-negative");
    const durations = frames.map((frame) => (
      options.fps === undefined ? frame.duration : 1000 / Math.max(1, options.fps)
    ));
    const total = durations.reduce((sum, duration) => sum + duration, 0);
    let elapsed = time * 1000;
    if (looped) elapsed %= total;
    else if (elapsed >= total) {
      if (!hold) return {};
      elapsed = total;
    }
    let index = frames.length - 1;
    let start = 0;
    for (let i = 0; i < frames.length; i += 1) {
      if (elapsed < start + durations[i]) {
        index = i;
        break;
      }
      start += durations[i];
    }
    return { sheet, frame: frames[index] };
  }
}
