// The hero: text drawn from an atlas that Typesetter's generator produced, with
// the same distance-field formula as addons/typesetter/shaders.
const canvas = document.getElementById("stage");
const input = document.getElementById("sample");
const sliders = {
  dilate: document.getElementById("dilate"),
  softness: document.getElementById("softness"),
  outline: document.getElementById("outline"),
  underlay: document.getElementById("underlay"),
};
const fieldToggle = document.getElementById("field");

const VERT = `#version 300 es
in vec2 pos; in vec2 uv; out vec2 v_uv;
uniform vec2 screen;
void main() {
  v_uv = uv;
  gl_Position = vec4(pos / screen * vec2(2.0, -2.0) + vec2(-1.0, 1.0), 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
in vec2 v_uv; out vec4 frag;
uniform sampler2D atlas;
uniform float gradientScale, dilate, softness, outline, underlay, showField;
const vec3 FACE = vec3(1.0, 0.894, 0.227);
const vec3 EDGE = vec3(1.0, 0.243, 0.541);
const vec3 SHADOW = vec3(0.039, 0.082, 0.314);

// Pixels per unit of normalised distance (0..1 spans 2 x gradientScale texels).
float pixelScale(vec2 uv) {
  vec2 size = vec2(textureSize(atlas, 0));
  vec2 dx = dFdx(uv) * size, dy = dFdy(uv) * size;
  float texelsPerPixel = max(sqrt(0.5 * (dot(dx, dx) + dot(dy, dy))), 1e-4);
  return 2.0 * gradientScale / texelsPerPixel;
}

void main() {
  float d = texture(atlas, v_uv).r;
  if (showField > 0.5) {
    frag = vec4(mix(vec3(0.071, 0.22, 0.788), vec3(0.949, 0.961, 1.0), d), 1.0);
    return;
  }
  float scale = pixelScale(v_uv);
  float sd = (d - (0.5 - dilate * 0.5)) * scale;
  float soft = softness * scale;
  float w = outline * 0.5 * scale;
  float faceA = clamp((sd - w) / (1.0 + soft) + 0.5, 0.0, 1.0);
  float outerA = clamp((sd + w) / (1.0 + soft) + 0.5, 0.0, 1.0);
  vec4 c = vec4(mix(EDGE, FACE, w > 0.0 ? faceA : 1.0) * outerA, outerA);
  if (underlay > 0.0) {
    vec2 off = vec2(-1.0, 1.0) * underlay * 0.012;
    float u = texture(atlas, v_uv + off).r;
    float ua = clamp((u - 0.5) * scale / (1.0 + underlay * 0.25 * scale) + 0.5, 0.0, 1.0);
    c += vec4(SHADOW * ua * 0.85, ua * 0.85) * (1.0 - c.a);
  }
  frag = c;
}`;

function compile(gl, type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
  return shader;
}

async function start() {
  const gl = canvas.getContext("webgl2", { alpha: true, premultipliedAlpha: true, antialias: false });
  if (!gl) {
    document.body.classList.add("no-webgl");
    return;
  }
  const [meta, image] = await Promise.all([
    fetch("hero-atlas.json").then((r) => r.json()),
    new Promise((resolve, reject) => {
      const i = new Image();
      i.onload = () => resolve(i);
      i.onerror = reject;
      i.src = "hero-atlas.png";
    }),
  ]);

  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(program);
  gl.useProgram(program);
  const u = (name) => gl.getUniformLocation(program, name);

  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  gl.pixelStorei(gl.UNPACK_COLORSPACE_CONVERSION_WEBGL, gl.NONE);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
  gl.generateMipmap(gl.TEXTURE_2D);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.uniform1f(u("gradientScale"), meta.gradientScale);

  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  const aPos = gl.getAttribLocation(program, "pos");
  const aUv = gl.getAttribLocation(program, "uv");
  gl.enableVertexAttribArray(aPos);
  gl.enableVertexAttribArray(aUv);
  gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 16, 0);
  gl.vertexAttribPointer(aUv, 2, gl.FLOAT, false, 16, 8);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

  const pad = meta.padding;

  function advanceOf(text) {
    let x = 0;
    for (const ch of text) {
      const g = meta.glyphs[ch.codePointAt(0)] || meta.glyphs[63];
      x += g[8];
    }
    return x;
  }

  function draw() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.round(canvas.clientWidth * dpr);
    const h = Math.round(canvas.clientHeight * dpr);
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    gl.viewport(0, 0, w, h);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const text = input.value || " ";
    const width = Math.max(advanceOf(text), 1);
    const scale = Math.min((w * 0.92) / width, (h * 0.8) / meta.pointSize);
    let x = (w - width * scale) / 2;
    const baseline = h / 2 + meta.pointSize * 0.36 * scale;
    const v = [];
    for (const ch of text) {
      const g = meta.glyphs[ch.codePointAt(0)] || meta.glyphs[63];
      const [rx, ry, rw, rh, gw, gh, bx, by, adv] = g;
      if (gw > 0 && gh > 0) {
        const x0 = x + (bx - pad) * scale;
        const x1 = x + (bx + gw + pad) * scale;
        const y0 = baseline - (by + pad) * scale;
        const y1 = baseline - (by - gh - pad) * scale;
        const u0 = (rx - pad) / meta.size;
        const u1 = (rx + rw + pad) / meta.size;
        const v0 = (ry - pad) / meta.size;
        const v1 = (ry + rh + pad) / meta.size;
        v.push(x0, y0, u0, v0, x1, y0, u1, v0, x1, y1, u1, v1, x0, y0, u0, v0, x1, y1, u1, v1, x0, y1, u0, v1);
      }
      x += adv * scale;
    }
    gl.uniform2f(u("screen"), w, h);
    gl.uniform1f(u("dilate"), sliders.dilate.value / 100);
    gl.uniform1f(u("softness"), sliders.softness.value / 100);
    gl.uniform1f(u("outline"), sliders.outline.value / 100);
    gl.uniform1f(u("underlay"), sliders.underlay.value / 100);
    gl.uniform1f(u("showField"), fieldToggle.checked ? 1 : 0);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(v), gl.DYNAMIC_DRAW);
    gl.drawArrays(gl.TRIANGLES, 0, v.length / 4);
  }

  for (const el of [input, ...Object.values(sliders), fieldToggle]) el.addEventListener("input", draw);
  new ResizeObserver(draw).observe(canvas);
  document.body.classList.add("ready");
  draw();
}

start().catch((e) => {
  console.error(e);
  document.body.classList.add("no-webgl");
});
