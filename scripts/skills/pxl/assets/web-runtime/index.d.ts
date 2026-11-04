export interface AudioLoop {
  start_frame: number;
  end_frame_exclusive: number;
}
export interface AudioEntry {
  id: string;
  kind: "sfx" | "bgm";
  required: boolean;
  path: string;
  sample_rate: number;
  channels: number;
  frames: number;
  loop?: AudioLoop;
}
export interface AudioIntent {
  tick?: number;
  eventId?: string;
  event_id?: string;
  audioId?: string;
  audio_id?: string;
  kind: "sfx" | "bgm";
  priority?: number;
  gain?: number;
  fadeSec?: number;
  fade_sec?: number;
}
export interface AudioPlayOptions {
  eventId?: string;
  priority?: number;
  gain?: number;
  fadeSec?: number;
}
export interface AudioDiagnostics {
  state: string;
  unlocked: boolean;
  cacheBytes?: number;
  cacheBudget?: number;
  encodedBytes?: number;
  loaded?: number;
  activeSfx?: number;
  activeBgm?: number;
  bgmId?: string | null;
  bgmStartedAt?: number | null;
  bgmDuration?: number;
  contextTime?: number;
  evicted?: number;
  events?: number;
  muted: boolean;
  errors: Array<{ code: string; message: string; audioId?: string | null }>;
}
export interface AudioPlayer {
  muted: boolean;
  preload(document: { audio?: AudioEntry[] }, baseUrl?: string): Promise<AudioDiagnostics>;
  unlock(): Promise<AudioDiagnostics>;
  load(id: string): Promise<AudioDiagnostics>;
  release(id: string): void;
  consume(intent: AudioIntent): Promise<boolean>;
  playSfx(id: string, options?: AudioPlayOptions): Promise<boolean>;
  playBgm(id: string, options?: AudioPlayOptions): Promise<boolean>;
  stopSfx(id?: string): void;
  stopBgm(fadeSec?: number): void;
  setMasterVolume(value: number): void;
  setSfxVolume(value: number): void;
  setBgmVolume(value: number): void;
  setMuted(value: boolean): void;
  setPageHidden(hidden: boolean): Promise<void>;
  diagnostics(): AudioDiagnostics;
  destroy(): Promise<void>;
}
export interface NullAudioPlayer extends AudioPlayer { events: AudioIntent[] }
export interface AudioOptions {
  contextFactory?: () => unknown;
  fetchImpl?: (url: URL) => Promise<unknown>;
  cacheBudget?: number;
  maxSfx?: number;
  onError?: (error: Error) => void;
}
export const AudioPlayer: { new(options?: AudioOptions): AudioPlayer };
export const NullAudioPlayer: { new(): NullAudioPlayer };
export class AudioError extends Error {
  code: string;
  audioId: string | null;
}
export interface EngineConfig {
  title: string;
  width: number;
  height: number;
  scale: "integer-fit" | number;
  canvas?: string;
  kernel?: string;
  audio?: AudioPlayer;
  audioOptions?: AudioOptions;
  test?: boolean;
}
export interface Rect { x: number; y: number; w: number; h: number }
export interface Point { x: number; y: number }
export interface FrameHandle {
  key: string;
  rect: Rect;
  duration: number;
  sourceSize: Rect;
  pivot: Point;
}
export interface AnimationInfo {
  fps: number;
  loop: boolean;
  asset: string;
  base: string;
  dirs: Record<string, string[]>;
}
export interface SheetHandle {
  texture: number;
  frame(key: string): FrameHandle | undefined;
  animInfo(sprite: string, anim: string): AnimationInfo | undefined;
  animFrames(sprite: string, anim: string, dir?: string): FrameHandle[];
}
export interface AnimOptions {
  time?: number;
  fps?: number;
  loop?: boolean;
  hold?: boolean;
}
export interface SpriteDraw {
  sheet: SheetHandle;
  frame: FrameHandle;
  x: number; y: number; w?: number; h?: number;
  flipX?: boolean; flipY?: boolean; tint?: number;
}
export interface Input {
  pressed(key: string): boolean;
  down(key: string): boolean;
  released(key: string): boolean;
}
export interface Game {
  init(ctx: Context): void | Promise<void>;
  update(ctx: Context, input: Input): void;
  draw(g: Graphics): void;
}
export interface LightDraw {
  x: number;
  y: number;
  radius: number;
  rgba: number;
  levels?: number;
}
export interface Graphics {
  clear(rgba: number): void;
  light(light: LightDraw): void;
  sprite(draw: SpriteDraw): void;
  rect(rect: Rect, rgba: number): void;
  camera(x: number, y: number): void;
}
export interface Context {
  audio: AudioPlayer;
  loadPackage<T>(assetPackage: T): T;
}
export interface AssetPackage {
  levels: unknown[];
  sheets: Map<string, SheetHandle>;
  audio?: AudioEntry[];
  sheetFor(assetId: string): SheetHandle | undefined;
  anim(assetId: string, anim: string, dir?: string, options?: AnimOptions): {
    sheet?: SheetHandle;
    frame?: FrameHandle;
  };
}
export function run(game: Game, config: EngineConfig): Promise<void>;
export function loadPackage(baseUrl: string): Promise<AssetPackage>;

export interface AdjacencyRule {
  art: string[];
}
export interface DepthObject {
  id: string;
  x: number;
  y: number;
  /** Row, relative to y, where the object touches the ground. */
  sortY: number;
}
export interface DrawLayerRef {
  id: string;
  layer: "back" | "front";
}
export interface DrawCall {
  id: string;
  layer: string;
  order: number;
}
export function hash32(text: string): number;
export function pick(seed: number | bigint, id: string, x: number, y: number, count: number): number;
export function pickAvoidingRepeat(
  seed: number | bigint,
  id: string,
  x: number,
  y: number,
  count: number,
  chosen: Map<string, number>,
): number;
export function selectGrid(
  seed: number | bigint,
  id: string,
  width: number,
  height: number,
  countAt: (x: number, y: number) => number,
): number[];
export function neighbourMask(same: (x: number, y: number) => boolean, x: number, y: number): number;
export function describeMask(mask: number): string;
export function reachableMasks(): number[];
export function resolveTile(rules: Map<number, AdjacencyRule>, mask: number, context?: string): AdjacencyRule;
export function missingMasks(rules: Map<number, AdjacencyRule>): number[];
export function depthOrder<T extends DepthObject>(objects: T[]): T[];
export function layeredDrawList<T extends DepthObject>(objects: T[], layersOf: (object: T) => DrawLayerRef[]): DrawCall[];
export function isBehind(actor: { y: number; sortY?: number }, object: DepthObject): boolean;
