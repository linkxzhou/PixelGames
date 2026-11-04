# Plugins

Register before `load()`:

```js
miniquad_add_plugin({
  name: "demo",
  version: 1,
  register_plugin(importObject) {
    importObject.env.demo_log = value => console.log(value);
  },
  on_init() {
    wasm_exports.demo_set_value(42);
  },
});
load("example.wasm");
```

`register_plugin` runs before instantiation. Declare the same function in Rust `extern "C"`. `on_init` runs after `wasm_exports` and `wasm_memory` are available but before `main()` and the first frame.

When `name` and `version` are present, the loader looks for `<name>_crate_version` and compares its return value when that export exists. Export `demo_crate_version() -> u32` to make the version check explicit. Missing imports are stubbed with warnings, so scan the binary instead of treating successful instantiation as ABI validation.

Allocate shared memory with `allocate_vec_u8` or a custom export, then recreate typed-array views after every possible allocation: `memory.grow` detaches old views.
