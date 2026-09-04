// Anything read from the coverage atlas: glyphs, icons, any shape cached once
// and drawn in whatever colour it is wanted in.
//
// The atlas holds one channel -- how much of each pixel the shape covers --
// and the colour comes from the instance. That is what lets one cached glyph
// serve every colour of text in the window instead of being rasterised per
// colour.

struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

struct Masked {
    // Where it goes, in device pixels: origin then size.
    bounds: vec4<f32>,
    // Where to read it from, as texture coordinates: min then max.
    uv: vec4<f32>,
    colour: vec4<f32>,
    /// left, top, right, bottom. Outside it, nothing.
    clip: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> items: array<Masked>;
@group(0) @binding(2) var coverage: texture_2d<f32>;
@group(0) @binding(3) var coverage_sampler: sampler;

struct Vertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) index: u32,
    @location(2) point: vec2<f32>,
};

const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let item = items[instance];
    let corner = CORNERS[vertex];
    let point = item.bounds.xy + corner * item.bounds.zw;

    var out: Vertex;
    out.clip = vec4<f32>(
        point.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - point.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = mix(item.uv.xy, item.uv.zw, corner);
    out.index = instance;
    out.point = point;
    return out;
}

@fragment
fn fragment(in: Vertex) -> @location(0) vec4<f32> {
    let item = items[in.index];
    if (in.point.x < item.clip.x || in.point.x >= item.clip.z
        || in.point.y < item.clip.y || in.point.y >= item.clip.w) {
        discard;
    }
    let mask = textureSample(coverage, coverage_sampler, in.uv).r;
    let alpha = item.colour.a * mask;
    // Premultiplied, matching the rest of the frame. Straight alpha here is
    // what makes text look bitten into a transparent background.
    return vec4<f32>(item.colour.rgb * alpha, alpha);
}
