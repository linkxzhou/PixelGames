# Loading protocol

1. Create `<canvas id="glcanvas" tabindex="0"></canvas>`; `gl.js` hard-codes this id.
2. Load `gl.js` as a classic script. Register plugins before calling `load()`.
3. Call `load("example.wasm")`. Do not use wasm-bindgen glue with this module.
4. `load` fetches and instantiates the module, assigns `wasm_memory` and `wasm_exports`, checks `crate_version() == 2`, initializes plugins, then invokes `exports.main()`.
5. Rust `main` calls `miniquad::start`. The backend creates WebGL and starts `requestAnimationFrame → wasm_exports.frame()`.

```html
<canvas id="glcanvas" tabindex="0"></canvas>
<script src="gl.js"></script>
<script>load("example.wasm");</script>
```

Serve over HTTP; `file://` fetch is not a supported loading path. A MIME error usually means the server did not return `application/wasm`. A version mismatch means `gl.js` and the miniquad crate came from different releases. `No <function> in gl.js` means the import is not implemented; `Missed function` means execution reached its stub.
