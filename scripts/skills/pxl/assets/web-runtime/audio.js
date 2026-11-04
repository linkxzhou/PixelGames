const DEFAULT_CACHE_BUDGET = 32 * 1024 * 1024;
const DEFAULT_SFX_VOICES = 16;
const MAX_EVENT_IDS = 4096;

export class AudioError extends Error {
  constructor(code, message, audioId = null) {
    super(message);
    this.name = "AudioError";
    this.code = code;
    this.audioId = audioId;
  }
}

function clampVolume(value) {
  if (!Number.isFinite(value) || value < 0 || value > 2) throw new AudioError("volume", "volume must be between 0 and 2");
  return value;
}

function decodedBytes(buffer) {
  return buffer.length * buffer.numberOfChannels * 4;
}

function claimEvent(set, eventId) {
  if (!eventId) return true;
  if (set.has(eventId)) return false;
  set.add(eventId);
  if (set.size > MAX_EVENT_IDS) set.delete(set.values().next().value);
  return true;
}

function fade(gain, value, now, seconds) {
  gain.cancelScheduledValues(now);
  gain.setValueAtTime(gain.value, now);
  gain.linearRampToValueAtTime(value, now + Math.max(0, seconds));
}

export class AudioPlayer {
  constructor(options = {}) {
    this.contextFactory = options.contextFactory ?? (() => {
      const Context = globalThis.AudioContext ?? globalThis.webkitAudioContext;
      if (!Context) throw new AudioError("unsupported", "Web Audio is not available");
      return new Context();
    });
    this.fetchImpl = options.fetchImpl ?? globalThis.fetch?.bind(globalThis);
    this.cacheBudget = options.cacheBudget ?? DEFAULT_CACHE_BUDGET;
    this.maxSfx = options.maxSfx ?? DEFAULT_SFX_VOICES;
    this.onError = options.onError ?? null;
    this.context = null;
    this.master = null;
    this.sfxBus = null;
    this.bgmBus = null;
    this.entries = new Map();
    this.encoded = new Map();
    this.buffers = new Map();
    this.cacheBytes = 0;
    this.encodedBytes = 0;
    this.sfxVoices = new Set();
    this.bgmVoices = new Set();
    this.currentBgm = null;
    this.eventIds = new Set();
    this.errors = [];
    this.masterVolume = 1;
    this.sfxVolume = 1;
    this.bgmVolume = 1;
    this.muted = false;
    this.destroyed = false;
    this.evicted = 0;
    this.resumeOnVisible = false;
    this.base = null;
  }

  async preload(document, baseUrl = "./assets/") {
    this._assertAlive();
    if (!this.fetchImpl) throw new AudioError("unsupported", "fetch is not available");
    this.base = new URL(baseUrl, globalThis.location?.href ?? "http://pxl.local/");
    const entries = document.audio ?? [];
    const total = entries.reduce((sum, entry) => sum + entry.frames * entry.channels * 4, 0);
    if (total > this.cacheBudget) {
      throw this._error("cache_budget", `audio pack needs ${total} decoded bytes, budget is ${this.cacheBudget}; load scenes individually`);
    }
    let requiredFailure = null;
    for (const entry of entries) {
      this.entries.set(entry.id, entry);
      try {
        await this._fetch(entry);
      } catch (error) {
        this._report(error);
        if (entry.required) requiredFailure ??= error;
      }
    }
    if (requiredFailure) throw requiredFailure;
    return this.diagnostics();
  }

  async unlock() {
    this._assertAlive();
    if (!this.context) {
      this.context = this.contextFactory();
      this.master = this.context.createGain();
      this.sfxBus = this.context.createGain();
      this.bgmBus = this.context.createGain();
      this.sfxBus.connect(this.master);
      this.bgmBus.connect(this.master);
      this.master.connect(this.context.destination);
      this._applyVolumes();
    }
    if (this.context.state === "suspended") await this.context.resume();
    let requiredFailure = null;
    for (const id of this.encoded.keys()) {
      try {
        await this._decode(id);
      } catch (error) {
        this._report(error);
        if (this.entries.get(id)?.required) requiredFailure ??= error;
      }
    }
    if (requiredFailure) throw requiredFailure;
    return this.diagnostics();
  }

  async load(id) {
    this._assertAlive();
    const entry = this._entry(id);
    await this._fetch(entry);
    if (this.context?.state === "running") await this._decode(id);
    return this.diagnostics();
  }

  release(id) {
    const buffer = this.buffers.get(id);
    if (buffer) this.cacheBytes -= decodedBytes(buffer);
    this.buffers.delete(id);
  }

  async consume(intent) {
    const options = {
      eventId: intent.eventId ?? intent.event_id,
      priority: intent.priority ?? 0,
      gain: intent.gain ?? 1,
      fadeSec: intent.fadeSec ?? intent.fade_sec ?? 0.1,
    };
    if (intent.kind === "bgm") return this.playBgm(intent.audioId ?? intent.audio_id, options);
    return this.playSfx(intent.audioId ?? intent.audio_id, options);
  }

  async playSfx(id, options = {}) {
    this._assertAlive();
    if (!claimEvent(this.eventIds, options.eventId)) return false;
    const buffer = await this._buffer(id);
    if (!buffer) return false;
    if (this.sfxVoices.size >= this.maxSfx) {
      const candidate = [...this.sfxVoices].sort((a, b) => a.priority - b.priority || a.order - b.order)[0];
      if ((options.priority ?? 0) < candidate.priority) return false;
      this._retireSfx(candidate);
      this.evicted += 1;
    }
    const source = this.context.createBufferSource();
    const gain = this.context.createGain();
    source.buffer = buffer;
    gain.gain.value = clampVolume(options.gain ?? 1);
    source.connect(gain);
    gain.connect(this.sfxBus);
    const voice = {
      id,
      source,
      gain,
      priority: options.priority ?? 0,
      order: (this.sfxVoices.size ? Math.max(...[...this.sfxVoices].map((item) => item.order)) : 0) + 1,
      closed: false,
    };
    source.onended = () => this._closeSfx(voice);
    this.sfxVoices.add(voice);
    source.start();
    return true;
  }

  stopSfx(id = null) {
    for (const voice of [...this.sfxVoices]) {
      if (id === null || voice.id === id) this._retireSfx(voice);
    }
  }

  async playBgm(id, options = {}) {
    this._assertAlive();
    if (!claimEvent(this.eventIds, options.eventId)) return false;
    if (this.currentBgm?.id === id) return true;
    const buffer = await this._buffer(id);
    if (!buffer) return false;
    const entry = this._entry(id);
    const seconds = Math.max(0, options.fadeSec ?? 0.1);
    if (this.currentBgm) this._fadeBgm(this.currentBgm, seconds);
    const source = this.context.createBufferSource();
    const gain = this.context.createGain();
    source.buffer = buffer;
    source.loop = true;
    source.loopStart = (entry.loop?.start_frame ?? 0) / entry.sample_rate;
    source.loopEnd = (entry.loop?.end_frame_exclusive ?? Math.round(buffer.duration * entry.sample_rate)) / entry.sample_rate;
    gain.gain.value = seconds ? 0 : 1;
    source.connect(gain);
    gain.connect(this.bgmBus);
    const voice = { id, source, gain, startedAt: this.context.currentTime, closed: false };
    source.onended = () => this._closeBgm(voice);
    this.bgmVoices.add(voice);
    this.currentBgm = voice;
    source.start();
    if (seconds) fade(gain.gain, 1, this.context.currentTime, seconds);
    return true;
  }

  stopBgm(fadeSec = 0.1) {
    if (this.currentBgm) {
      this._fadeBgm(this.currentBgm, fadeSec);
      this.currentBgm = null;
    }
  }

  setMasterVolume(value) {
    this.masterVolume = clampVolume(value);
    this._applyVolumes();
  }

  setSfxVolume(value) {
    this.sfxVolume = clampVolume(value);
    this._applyVolumes();
  }

  setBgmVolume(value) {
    this.bgmVolume = clampVolume(value);
    this._applyVolumes();
  }

  setMuted(value) {
    this.muted = Boolean(value);
    this._applyVolumes();
  }

  async setPageHidden(hidden) {
    if (!this.context) return;
    if (hidden) {
      this.resumeOnVisible = this.context.state === "running";
      if (this.resumeOnVisible) await this.context.suspend();
    } else if (this.resumeOnVisible) {
      this.resumeOnVisible = false;
      await this.context.resume();
    }
  }

  diagnostics() {
    return {
      state: this.destroyed ? "destroyed" : this.context?.state ?? "locked",
      unlocked: this.context?.state === "running",
      cacheBytes: this.cacheBytes,
      cacheBudget: this.cacheBudget,
      encodedBytes: this.encodedBytes,
      loaded: this.buffers.size,
      activeSfx: this.sfxVoices.size,
      activeBgm: this.bgmVoices.size,
      bgmId: this.currentBgm?.id ?? null,
      bgmStartedAt: this.currentBgm?.startedAt ?? null,
      bgmDuration: this.currentBgm ? this.currentBgm.source.loopEnd - this.currentBgm.source.loopStart : 0,
      contextTime: this.context?.currentTime ?? 0,
      muted: this.muted,
      masterVolume: this.masterVolume,
      sfxVolume: this.sfxVolume,
      bgmVolume: this.bgmVolume,
      evicted: this.evicted,
      errors: this.errors.map((error) => ({ code: error.code, message: error.message, audioId: error.audioId })),
    };
  }

  async destroy() {
    if (this.destroyed) return;
    this.stopSfx();
    for (const voice of [...this.bgmVoices]) this._stopBgm(voice, this.context?.currentTime ?? 0);
    for (const node of [this.sfxBus, this.bgmBus, this.master]) node?.disconnect();
    if (this.context && this.context.state !== "closed") await this.context.close();
    this.buffers.clear();
    this.encoded.clear();
    this.cacheBytes = 0;
    this.encodedBytes = 0;
    this.destroyed = true;
  }

  _entry(id) {
    const entry = this.entries.get(id);
    if (!entry) throw new AudioError("unknown_audio", `unknown audio id '${id}'`, id);
    return entry;
  }

  async _fetch(entry) {
    if (this.encoded.has(entry.id)) return;
    const response = await this.fetchImpl(new URL(entry.path, this.base));
    if (!response.ok) throw new AudioError("http", `${entry.path}: HTTP ${response.status}`, entry.id);
    const bytes = await response.arrayBuffer();
    this.encoded.set(entry.id, bytes);
    this.encodedBytes += bytes.byteLength;
  }

  async _decode(id) {
    if (this.buffers.has(id)) return this.buffers.get(id);
    const encoded = this.encoded.get(id);
    if (!encoded || !this.context) return null;
    let buffer;
    try {
      buffer = await this.context.decodeAudioData(encoded.slice(0));
    } catch (cause) {
      throw new AudioError("decode", `${id}: cannot decode WAV: ${cause.message ?? cause}`, id);
    }
    const bytes = decodedBytes(buffer);
    if (this.cacheBytes + bytes > this.cacheBudget) {
      throw new AudioError("cache_budget", `${id}: decoded cache budget ${this.cacheBudget} would be exceeded`, id);
    }
    this.buffers.set(id, buffer);
    this.cacheBytes += bytes;
    return buffer;
  }

  async _buffer(id) {
    this._entry(id);
    if (this.context?.state !== "running") return null;
    return this.buffers.get(id) ?? this._decode(id);
  }

  _applyVolumes() {
    if (!this.context) return;
    const now = this.context.currentTime;
    this.master.gain.setTargetAtTime(this.muted ? 0 : this.masterVolume, now, 0.01);
    this.sfxBus.gain.setTargetAtTime(this.sfxVolume, now, 0.01);
    this.bgmBus.gain.setTargetAtTime(this.bgmVolume, now, 0.01);
  }

  _retireSfx(voice) {
    this.sfxVoices.delete(voice);
    try {
      voice.source.stop();
    } catch (error) {
      this._report(new AudioError("stop", `${voice.id}: ${error.message ?? error}`, voice.id));
    }
  }

  _closeSfx(voice) {
    if (voice.closed) return;
    voice.closed = true;
    this.sfxVoices.delete(voice);
    voice.source.disconnect();
    voice.gain.disconnect();
  }

  _fadeBgm(voice, seconds) {
    if (this.currentBgm === voice) this.currentBgm = null;
    const now = this.context.currentTime;
    fade(voice.gain.gain, 0, now, seconds);
    this._stopBgm(voice, now + seconds);
  }

  _stopBgm(voice, when) {
    try {
      voice.source.stop(when);
    } catch (error) {
      this._report(new AudioError("stop", `${voice.id}: ${error.message ?? error}`, voice.id));
    }
  }

  _closeBgm(voice) {
    if (voice.closed) return;
    voice.closed = true;
    this.bgmVoices.delete(voice);
    if (this.currentBgm === voice) this.currentBgm = null;
    voice.source.disconnect();
    voice.gain.disconnect();
  }

  _error(code, message, audioId = null) {
    return new AudioError(code, message, audioId);
  }

  _report(error) {
    const wrapped = error instanceof AudioError ? error : new AudioError("backend", error.message ?? String(error));
    this.errors.push(wrapped);
    this.onError?.(wrapped);
  }

  _assertAlive() {
    if (this.destroyed) throw new AudioError("destroyed", "audio player has been destroyed");
  }
}

export class NullAudioPlayer {
  constructor() {
    this.events = [];
    this.eventIds = new Set();
    this.muted = false;
    this.masterVolume = 1;
    this.sfxVolume = 1;
    this.bgmVolume = 1;
    this.destroyed = false;
  }

  async preload() { return this.diagnostics(); }
  async unlock() { return this.diagnostics(); }
  async load() { return this.diagnostics(); }
  release() {}

  async consume(intent) {
    const eventId = intent.eventId ?? intent.event_id;
    if (!claimEvent(this.eventIds, eventId)) return false;
    this.events.push({
      tick: intent.tick ?? null,
      eventId: eventId ?? null,
      audioId: intent.audioId ?? intent.audio_id,
      kind: intent.kind,
    });
    return true;
  }

  async playSfx(audioId, options = {}) { return this.consume({ audioId, kind: "sfx", ...options }); }
  async playBgm(audioId, options = {}) { return this.consume({ audioId, kind: "bgm", ...options }); }
  stopSfx() {}
  stopBgm() {}
  setMasterVolume(value) { this.masterVolume = clampVolume(value); }
  setSfxVolume(value) { this.sfxVolume = clampVolume(value); }
  setBgmVolume(value) { this.bgmVolume = clampVolume(value); }
  setMuted(value) { this.muted = Boolean(value); }
  async setPageHidden() {}

  diagnostics() {
    return {
      state: this.destroyed ? "destroyed" : "null",
      unlocked: true,
      events: this.events.length,
      muted: this.muted,
      masterVolume: this.masterVolume,
      sfxVolume: this.sfxVolume,
      bgmVolume: this.bgmVolume,
      errors: [],
    };
  }

  async destroy() { this.destroyed = true; }
}
