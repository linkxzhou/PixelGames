pub const VERTEX: &str = r#"#version 100
attribute vec2 pos;
attribute vec2 uv;
attribute vec4 color0;
varying lowp vec2 texcoord;
varying lowp vec4 color;
void main() {
    gl_Position = vec4(pos, 0.0, 1.0);
    texcoord = uv;
    color = color0;
}
"#;

pub const FRAGMENT: &str = r#"#version 100
varying lowp vec2 texcoord;
varying lowp vec4 color;
uniform sampler2D tex;
void main() {
    gl_FragColor = texture2D(tex, texcoord) * color;
}
"#;
