// One rectangle, drawn once: fill, rounded corners, border and shadow are all
// the same signed-distance field evaluated in the same pass. Drawing a box and
// then its border as two quads is where the seam between them comes from.

struct Globals {
    // The framebuffer, in device pixels. Positions arrive in the same units,
    // so this is the only thing needed to reach clip space.
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};

struct Quad {
    // The rectangle itself, in device pixels: origin then size.
    bounds: vec4<f32>,
    // The area actually rasterised, which is larger when there is a shadow to
    // make room for.
    painted: vec4<f32>,
    // Clockwise from the top left.
    radii: vec4<f32>,
    background: vec4<f32>,
    border_colour: vec4<f32>,
    shadow_colour: vec4<f32>,
    // x: border width. y: shadow blur. z: shadow spread. w: the row of the
    // gradient lookup table to sample, or -1 for a solid fill.
    params: vec4<f32>,
    // Where the shadow sits relative to the quad, and the direction of the
    // gradient as a unit vector.
    shadow_offset_and_gradient: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<storage, read> quads: array<Quad>;
@group(0) @binding(2) var gradients: texture_2d<f32>;
@group(0) @binding(3) var gradient_sampler: sampler;

struct Vertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) point: vec2<f32>,
    @location(1) @interpolate(flat) index: u32,
};

// Two triangles from the vertex index alone: no vertex buffer, no index
// buffer, nothing to keep in step with the instance data.
const CORNERS = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
);

@vertex
fn vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> Vertex {
    let quad = quads[instance];
    let corner = CORNERS[vertex];
    let point = quad.painted.xy + corner * quad.painted.zw;

    var out: Vertex;
    // Device pixels to clip space. y is flipped: this library counts downwards
    // from the top left and the GPU counts upwards from the centre.
    out.clip = vec4<f32>(
        point.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - point.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.point = point;
    out.index = instance;
    return out;
}

// Distance from `point` to the edge of a rounded rectangle. Negative inside.
fn rounded_box(point: vec2<f32>, half: vec2<f32>, radii: vec4<f32>) -> f32 {
    // The radius belonging to the corner this point is nearest.
    // radii is top-left, top-right, bottom-right, bottom-left.
    let top = select(radii.x, radii.y, point.x > 0.0);
    let bottom = select(radii.w, radii.z, point.x > 0.0);
    let radius = select(top, bottom, point.y > 0.0);

    let inner = abs(point) - half + radius;
    return min(max(inner.x, inner.y), 0.0) + length(max(inner, vec2<f32>(0.0))) - radius;
}

fn fill_at(quad: Quad, point: vec2<f32>) -> vec4<f32> {
    let row = quad.params.w;
    if (row < 0.0) {
        return quad.background;
    }
    // Project the point onto the gradient's direction to get how far along it
    // is, then read the colour from the row baked for this gradient. The
    // mixing itself was done on the CPU, in Oklab, by code that is tested.
    let direction = quad.shadow_offset_and_gradient.zw;
    let relative = (point - quad.bounds.xy) / max(quad.bounds.zw, vec2<f32>(1.0));
    let along = clamp(dot(relative - vec2<f32>(0.5), direction) + 0.5, 0.0, 1.0);
    let rows = f32(textureDimensions(gradients).y);
    return textureSampleLevel(
        gradients,
        gradient_sampler,
        vec2<f32>(along, (row + 0.5) / rows),
        0.0,
    );
}

@fragment
fn fragment(in: Vertex) -> @location(0) vec4<f32> {
    let quad = quads[in.index];
    let centre = quad.bounds.xy + quad.bounds.zw * 0.5;
    let half = quad.bounds.zw * 0.5;
    let local = in.point - centre;
    let distance = rounded_box(local, half, quad.radii);

    // One pixel of coverage either side of the edge. Without it every rounded
    // corner in the window is a staircase.
    let coverage = 1.0 - smoothstep(-0.5, 0.5, distance);

    var colour = vec4<f32>(0.0);

    // The shadow first, behind everything, and only outside the quad itself --
    // a shadow drawn under an opaque box is bandwidth spent on nothing, and
    // under a translucent one it darkens the fill.
    let blur = quad.params.y;
    if (quad.shadow_colour.a > 0.0 && blur + quad.params.z > 0.0) {
        let shadow_point = local - quad.shadow_offset_and_gradient.xy;
        let spread = quad.params.z;
        let shadow_distance = rounded_box(
            shadow_point,
            half + vec2<f32>(spread),
            quad.radii + vec4<f32>(spread),
        );
        let softness = max(blur, 0.5);
        let shadow_alpha = 1.0 - smoothstep(-softness, softness, shadow_distance);
        colour = quad.shadow_colour * shadow_alpha * (1.0 - coverage);
    }

    let fill = fill_at(quad, in.point);
    var surface = fill;

    let border_width = quad.params.x;
    if (border_width > 0.0 && quad.border_colour.a > 0.0) {
        // Inside the border: the outer edge is the quad's own edge, so a
        // border never grows the shape it is drawn on.
        let inner = distance + border_width;
        let on_border = smoothstep(-0.5, 0.5, inner);
        surface = mix(fill, quad.border_colour, on_border);
    }

    // Premultiplied, which is what the surface is configured for: blending
    // straight alpha darkens every edge against a transparent background, and
    // this library's whole point is windows that are transparent.
    let front = vec4<f32>(surface.rgb * surface.a, surface.a) * coverage;
    return front + colour * (1.0 - front.a);
}
