# Pitfalls

- Keep custom ABI functions `#[no_mangle] pub extern "C"` and verify the final release wasm export table. LTO and stripping can change what survives.
- Recreate `Uint8Array`, `Float32Array`, and `DataView` objects when `wasm_memory.buffer` changes. Growth detaches old views.
- Use framebuffer pixels for projection and CSS pixels for layout. `high_dpi` changes the relationship; apply `dpi_scale()` deliberately.
- `blocking_event_loop` uses `schedule_update`; do not assume desktop timing behavior in the browser. The normal browser path is rAF-driven.
- `fs::load_file` is asynchronous. Keep the callback alive until `file_loaded(file_id)` fires; HTTP status and decoding errors arrive through the response.
- Canvas keyboard events require focus. Give the canvas `tabindex="0"` and focus it after pointer interaction.
- Prevent arrow/space scrolling through the canvas handlers rather than global handlers.
- Do not mix miniquad's `gl.js` with wasm-bindgen output. They are separate ABI and loading protocols.
- Do not interpret missing-import stubs as success. Run `check_wasm_imports.py` against every final binary.
- Fixed-step simulation belongs above miniquad. Clamp accumulated frame time so a suspended tab cannot trigger an unbounded update burst.
