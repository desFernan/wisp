// Rectangles filled from a picture: avatars, icons that are drawings, a
// character's sprite.
//
// The same shape as the mask pipeline and deliberately a separate one. A mask
// is one channel and takes its colour from the instance; a picture has its own
// colours and is only tinted. Branching between the two in one shader would
// cost a comparison per fragment to save a pipeline that is switched once a
// frame.

struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

struct Textured {
    bounds: vec4<f32>,
    uv: vec4<f32>,
    tint: vec4<f32>,
    clip: vec4<f32>,
    // Rotation in radians, the pivot as a fraction of the box, and a spare.
    turn: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> items: array<Textured>;
@group(0) @binding(2) var picture: texture_2d<f32>;
@group(0) @binding(3) var picture_sampler: sampler;

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
    var point = item.bounds.xy + corner * item.bounds.zw;

    let radians = item.turn.x;
    if (radians != 0.0) {
        // About the pivot rather than the middle. Which corner a thing turns
        // on is most of whether the movement reads as the right one: a
        // character leaning into a walk turns about its feet, and about its
        // middle it slides sideways into the floor.
        let pivot = item.bounds.xy + item.turn.yz * item.bounds.zw;
        let local = point - pivot;
        let c = cos(radians);
        let s = sin(radians);
        point = pivot + vec2<f32>(local.x * c - local.y * s, local.x * s + local.y * c);
    }

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
    let picked = textureSample(picture, picture_sampler, in.uv);
    // The atlas is premultiplied and so is the frame, so the colour is already
    // scaled by its own alpha and only the tint has to be applied: its colour
    // to the colour, and its alpha to both.
    return vec4<f32>(picked.rgb * item.tint.rgb * item.tint.a, picked.a * item.tint.a);
}
