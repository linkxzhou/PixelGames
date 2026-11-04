import { CommandBuffer } from "./command-buffer.js";
import { FixedStep } from "./fixed-step.js";
import { Input } from "./input.js";
import { AudioPlayer } from "./audio.js";

export const RUNTIME_VERSION = 2;

export class Graphics {
  constructor(commands) {
    this.commands = commands;
  }
  begin() { this.commands.reset(); }
  clear(rgba) { this.commands.clear(rgba); }
  rect(rect, rgba) { this.commands.rect(rect.x, rect.y, rect.w, rect.h, rgba); }
  sprite(draw) {
    const frame = draw.frame;
    if (!frame || typeof frame.key !== "string" || !frame.rect) {
      throw new TypeError("sprite draw requires a Sheet FrameHandle");
    }
    if (!draw.sheet.texture) return;
    const width = draw.w ?? frame.rect.w;
    const height = draw.h ?? frame.rect.h;
    const x = draw.x + frame.sourceSize.x - frame.pivot.x;
    const y = draw.y + frame.sourceSize.y - frame.pivot.y;
    this.commands.sprite(
      draw.sheet.texture,
      [frame.rect.x, frame.rect.y, frame.rect.w, frame.rect.h],
      [x, y, width, height],
      draw.flipX ?? false,
      draw.flipY ?? false,
      draw.tint ?? 0xffffffff,
    );
  }
  light(light) {
    this.commands.light(light.x, light.y, light.radius, light.rgba, light.levels ?? 4);
  }
  camera(x, y) { this.commands.camera(x, y); }
  end() { this.commands.submit(); }
}

class Context {
  constructor(wasm, audio) {
    this.wasm = wasm;
    this.audio = audio;
  }

  loadPackage(assetPackage) {
    let texture = 1;
    for (const sheet of assetPackage.sheets.values()) {
      const pointer = this.wasm.pxl_alloc(sheet.rgba.length);
      const bytes = new Uint8Array(this.wasm.memory.buffer, pointer, sheet.rgba.length);
      bytes.set(sheet.rgba);
      const result = this.wasm.pxl_upload_texture(
        texture, sheet.width, sheet.height, pointer, sheet.rgba.length,
      );
      if (result !== 0) throw new Error(`cannot upload sheet ${sheet.id}`);
      sheet.texture = texture;
      texture += 1;
    }
    return assetPackage;
  }
}

export async function run(game, config) {
  if (typeof globalThis.miniquad_add_plugin !== "function" || typeof globalThis.load !== "function") {
    throw new Error("load mq_js_bundle.js before pxl_runtime.js");
  }
  const canvas = document.getElementById(config.canvas ?? "glcanvas");
  if (!canvas) throw new Error(`canvas ${config.canvas ?? "glcanvas"} not found`);
  canvas.width = config.width;
  canvas.height = config.height;
  canvas.style.aspectRatio = `${config.width} / ${config.height}`;
  canvas.focus();
  document.title = config.title;
  const audio = config.audio ?? new AudioPlayer(config.audioOptions);
  globalThis.__pxlAudio = audio;
  const audioButton = document.getElementById("pxl-audio-toggle");
  if (audioButton) {
    const updateButton = () => {
      const state = audio.diagnostics();
      audioButton.textContent = !state.unlocked ? "Enable sound" : state.muted ? "Unmute" : "Mute";
    };
    audioButton.addEventListener("click", async () => {
      try {
        if (!audio.diagnostics().unlocked) await audio.unlock();
        else audio.setMuted(!audio.muted);
        updateButton();
        canvas.focus();
      } catch (error) {
        audioButton.textContent = "Audio unavailable";
        console.error(error);
      }
    });
    document.addEventListener("visibilitychange", () => {
      audio.setPageHidden(document.hidden).catch((error) => console.error(error));
    });
    updateButton();
  }

  const testMode = config.test === true || globalThis.__pxlPlaytest === true;
  let ready = false;
  let failed = false;
  let paused = testMode;
  let ticks = 0;
  let input;
  let fixed;
  let graphics;
  let initialize;
  const initialized = new Promise((resolve, reject) => { initialize = { resolve, reject }; });

  function render() {
    graphics.begin();
    game.draw(graphics);
    graphics.end();
  }

  function testStatus() {
    return { paused, tick: ticks, ...game.test?.status?.() };
  }

  if (testMode) {
    globalThis.__pxlTest = {
      pause() {
        paused = true;
        fixed?.reset();
        return testStatus();
      },
      resume() {
        fixed?.reset();
        paused = false;
        return testStatus();
      },
      advanceTicks(count) {
        if (!Number.isInteger(count) || count < 0) throw new Error("tick count must be a non-negative integer");
        this.pause();
        for (let index = 0; index < count; index += 1) {
          game.update(game.context, input);
          ticks += 1;
          input.endTick();
        }
        return testStatus();
      },
      capture() {
        this.pause();
        render();
        return {
          status: testStatus(),
          logical: game.test?.capture?.() ?? canvas.toDataURL("image/png"),
          presentation: canvas.toDataURL("image/png"),
        };
      },
      status: testStatus,
    };
  }

  globalThis.miniquad_add_plugin({
    name: "pxl",
    version: RUNTIME_VERSION,
    register_plugin(imports) {
      imports.env.pxl_js_update = (dt) => {
        if (!ready || failed || paused) return;
        try {
          input.poll();
          fixed.advance(dt, () => {
            game.update(game.context, input);
            ticks += 1;
            input.endTick();
          });
        } catch (error) { fail(error); }
      };
      imports.env.pxl_js_draw = () => {
        if (!ready || failed) return;
        try {
          render();
        } catch (error) { fail(error); }
      };
    },
    on_init() {
      const wasm = globalThis.wasm_exports;
      wasm.pxl_configure(config.width, config.height, config.scale === "integer-fit" ? 0 : config.scale);
      const commands = new CommandBuffer(wasm);
      input = new Input(wasm);
      fixed = new FixedStep();
      graphics = new Graphics(commands);
      game.context = new Context(wasm, audio);
      Promise.resolve(game.init(game.context)).then(() => {
        ready = true;
        globalThis.__pxlReady = true;
        initialize.resolve();
      }).catch(fail);
    },
  });

  function fail(error) {
    failed = true;
    globalThis.__pxlError = String(error?.stack ?? error);
    console.error(error);
    initialize.reject(error);
  }

  globalThis.load(config.kernel ?? "pxl_web.wasm");
  return initialized;
}
