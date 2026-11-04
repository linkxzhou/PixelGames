import { parseHex } from "./palette.js";

const LAYERS = ["background", "stage", "world", "effect", "ui"];

function sceneOf(pkg) {
  const scenes = pkg.visual?.effective_style?.scene_previews;
  if (!Array.isArray(scenes) || !scenes.length) {
    throw new Error("asset package has no visual scene preview");
  }
  return scenes[0];
}

function firstAnim(pkg, assetId) {
  const asset = pkg.assets.find((candidate) => candidate.id === assetId);
  return Object.keys(asset?.anims ?? {})[0];
}

export function selectFrame(pkg, instance, time = 0) {
  const sheet = pkg.sheetFor(instance.asset_id);
  if (!sheet) throw new Error(`missing sheet for asset '${instance.asset_id}'`);
  if (instance.frame) {
    const frame = sheet.frame(instance.frame);
    if (!frame) throw new Error(`missing frame '${instance.frame}' for '${instance.asset_id}'`);
    return { sheet, frame };
  }
  const anim = firstAnim(pkg, instance.asset_id);
  if (anim) {
    const selected = pkg.anim(instance.asset_id, anim, instance.dir ?? "N", { time });
    if (selected.frame) return selected;
  }
  const key = sheet.order[0];
  if (!key) throw new Error(`asset '${instance.asset_id}' has no frames`);
  return { sheet, frame: sheet.frame(key) };
}

function instances(scene) {
  return [...scene.instances].sort((a, b) => (
    LAYERS.indexOf(a.layer) - LAYERS.indexOf(b.layer)
  ));
}

function clearColor(scene) {
  return scene.background === "light" ? 0xf0f0e8ff : 0x181820ff;
}

function rgbaNumber(color) {
  const [r, g, b, a] = parseHex(color);
  return ((r << 24) | (g << 16) | (b << 8) | a) >>> 0;
}

function drawInstances(pkg, graphics, list, time) {
  for (const instance of list) {
    const selected = selectFrame(pkg, instance, time);
    graphics.sprite({
      ...selected,
      x: Math.round(instance.position[0]),
      y: Math.round(instance.position[1]),
      flipX: instance.flip_x ?? false,
      flipY: instance.flip_y ?? false,
    });
  }
}

function drawScene(pkg, scene, graphics, time) {
  graphics.clear(clearColor(scene));
  const world = instances(scene).filter((instance) => instance.layer !== "ui");
  const ui = instances(scene).filter((instance) => instance.layer === "ui");
  drawInstances(pkg, graphics, world, time);
  if (scene.render_mode === "L1") {
    graphics.rect({ x: 0, y: 0, w: scene.size[0], h: scene.size[1] }, 0x80808010);
  } else if (scene.render_mode === "L2") {
    for (const light of scene.lights ?? []) {
      graphics.light({
        x: light.position[0],
        y: light.position[1],
        radius: light.radius,
        rgba: rgbaNumber(light.color),
        levels: 4,
      });
    }
  }
  drawInstances(pkg, graphics, ui, time);
}

/**
 * Copy one frame out of a sheet as tight RGBA bytes, optionally mirrored.
 * Pure (no DOM) so the flip behaviour used by the logical capture can be
 * tested without a browser and compared with what the GPU path draws.
 */
export function frameRgba(sheet, rect, flipX = false, flipY = false) {
  const out = new Uint8ClampedArray(rect.w * rect.h * 4);
  for (let y = 0; y < rect.h; y += 1) {
    const sourceY = flipY ? rect.h - 1 - y : y;
    for (let x = 0; x < rect.w; x += 1) {
      const sourceX = flipX ? rect.w - 1 - x : x;
      const from = ((rect.y + sourceY) * sheet.width + rect.x + sourceX) * 4;
      out.set(sheet.rgba.subarray(from, from + 4), (y * rect.w + x) * 4);
    }
  }
  return out;
}

function logicalCanvas(pkg, scene, time) {
  const canvas = document.createElement("canvas");
  canvas.width = scene.size[0];
  canvas.height = scene.size[1];
  const context = canvas.getContext("2d");
  context.imageSmoothingEnabled = false;
  context.fillStyle = scene.background === "light" ? "#f0f0e8" : "#181820";
  context.fillRect(0, 0, canvas.width, canvas.height);
  const draw = (list) => {
    for (const instance of list) {
      const { sheet, frame } = selectFrame(pkg, instance, time);
      const source = document.createElement("canvas");
      source.width = frame.rect.w;
      source.height = frame.rect.h;
      const sourceContext = source.getContext("2d");
      const pixels = frameRgba(
        sheet,
        frame.rect,
        instance.flip_x ?? false,
        instance.flip_y ?? false,
      );
      sourceContext.putImageData(new ImageData(pixels, frame.rect.w, frame.rect.h), 0, 0);
      const x = Math.round(instance.position[0]) + frame.sourceSize.x - frame.pivot.x;
      const y = Math.round(instance.position[1]) + frame.sourceSize.y - frame.pivot.y;
      context.drawImage(source, x, y);
    }
  };
  draw(instances(scene).filter((instance) => instance.layer !== "ui"));
  if (scene.render_mode === "L1") {
    context.fillStyle = "rgba(128,128,128,0.0625)";
    context.fillRect(0, 0, canvas.width, canvas.height);
  } else if (scene.render_mode === "L2") {
    for (const light of scene.lights ?? []) {
      const [r, g, b, a] = parseHex(light.color);
      for (let band = 0; band < 4; band += 1) {
        const extent = light.radius * (4 - band) / 4;
        context.fillStyle = `rgba(${r},${g},${b},${a * (band + 1) / 4 / 255})`;
        context.fillRect(light.position[0] - extent, light.position[1] - extent, extent * 2, extent * 2);
      }
    }
  }
  draw(instances(scene).filter((instance) => instance.layer === "ui"));
  return canvas;
}

export function createShowcase(pkg) {
  const scene = sceneOf(pkg);
  const state = { tick: 0, draws: 0 };
  const game = {
    init(context) {
      context.loadPackage(pkg);
    },
    update() {
      state.tick += 1;
    },
    draw(graphics) {
      state.draws += 1;
      graphics.begin();
      drawScene(pkg, scene, graphics, state.tick / 60);
      graphics.end();
    },
    test: {
      status() {
        return {
          scene_id: scene.id,
          capture_tick: scene.tick,
          tick: state.tick,
          time: state.tick / 60,
          draws: state.draws,
          render_mode: scene.render_mode ?? "L0",
          lights: (scene.lights ?? []).length,
          assets: scene.instances.length,
          logical_size: [...scene.size],
          scale: scene.scale,
        };
      },
      capture() {
        return logicalCanvas(pkg, scene, state.tick / 60).toDataURL("image/png");
      },
    },
  };
  return {
    game,
    config: {
      title: `Pxl showcase: ${scene.id}`,
      width: scene.size[0],
      height: scene.size[1],
      scale: scene.scale,
    },
  };
}
