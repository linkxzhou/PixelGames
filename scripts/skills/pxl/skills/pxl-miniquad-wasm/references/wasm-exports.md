# wasm exports

Generated from `src/native/wasm.rs` and `src/fs.rs`; run `extract_interfaces.py --check` after upgrades.

- `fn allocate_vec_u8(len: usize) -> *mut u8`
- `fn crate_version() -> u32`
- `fn file_loaded(file_id: u32)`
- `fn focus(has_focus: bool)`
- `fn frame()`
- `fn key_down(key: u32, modifiers: u32, repeat: bool)`
- `fn key_press(key: u32)`
- `fn key_up(key: u32, modifiers: u32)`
- `fn mouse_down(x: i32, y: i32, btn: i32)`
- `fn mouse_move(x: i32, y: i32)`
- `fn mouse_up(x: i32, y: i32, btn: i32)`
- `fn mouse_wheel(dx: i32, dy: i32)`
- `fn on_clipboard_paste(msg: *mut u8, len: usize)`
- `fn on_file_dropped( path: *mut u8, path_len: usize, bytes: *mut u8, bytes_len: usize, )`
- `fn on_files_dropped_finish()`
- `fn on_files_dropped_start()`
- `fn raw_mouse_move(dx: i32, dy: i32)`
- `fn resize(width: i32, height: i32)`
- `fn touch(phase: u32, id: u32, x: f32, y: f32)`
- `fn main()`
