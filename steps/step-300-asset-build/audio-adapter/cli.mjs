#!/usr/bin/env node
// Adapter entry point. One process renders exactly one asset.
//
//   stdin : one JSON request  {"kind":"sfx"|"bgm", ...}
//   stdout: one JSON header line, "\n", then `bytes` of float32 little-endian PCM
//   stderr: diagnostics only; the Rust side caps and discards it
//
// The Rust caller runs this under `node --permission` with read access limited
// to this directory, so the adapter cannot write files or spawn processes.

import os from "node:os";
import { renderBgm } from "./bgm.mjs";
import { renderSfx } from "./sfx.mjs";

export const ADAPTER_VERSION = "1.0.0";

async function readStdin() {
  const chunks = [];
  let total = 0;
  for await (const chunk of process.stdin) {
    total += chunk.length;
    if (total > 1024 * 1024) throw new Error("request exceeds 1 MiB");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

function fail(message) {
  process.stderr.write(`pxl-audio-adapter: ${message}\n`);
  process.exit(3);
}

async function main() {
  if (os.endianness() !== "LE") fail("big-endian hosts are not supported");
  if (process.argv[2] === "--version") {
    process.stdout.write(`${ADAPTER_VERSION}\n`);
    return;
  }
  let request;
  try {
    request = JSON.parse(await readStdin());
  } catch (error) {
    fail(`invalid request: ${error.message}`);
  }
  let result;
  try {
    if (request.kind === "sfx") result = await renderSfx(request);
    else if (request.kind === "bgm") result = await renderBgm(request);
    else throw new Error(`unknown kind '${request.kind}'`);
  } catch (error) {
    fail(error.message);
  }
  for (let i = 0; i < result.pcm.length; i++) {
    if (!Number.isFinite(result.pcm[i])) fail(`non-finite sample at index ${i}`);
  }
  const bytes = Buffer.from(result.pcm.buffer, result.pcm.byteOffset, result.pcm.byteLength);
  const header = JSON.stringify({
    adapter_version: ADAPTER_VERSION,
    node: process.version,
    channels: result.channels,
    frames: result.frames,
    bytes: bytes.length,
  });
  process.stdout.write(`${header}\n`);
  process.stdout.write(bytes);
}

main().catch((error) => fail(error.stack || String(error)));
