@group(0)
@binding(0)
var s_sampler: sampler;

@group(0)
@binding(1)
var t_colour: texture_2d<f32>;

@group(0)
@binding(2)
var t_normal: texture_2d<f32>;

@group(0)
@binding(3)
var t_depth: texture_depth_2d;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var uv: vec2<f32> = vec2<f32>(vec2<u32>((index << 1) & 2, index & 2));
    return vec4<f32>(uv * vec2<f32>(2, -2) + vec2<f32>(-1, 1), 0, 1);
}

fn edge_darken(uv: vec2<f32>, screen_size: vec2<f32>) -> f32 {
    var depth: f32 = textureSample(t_depth, s_sampler, uv);

    var pixel_width: vec2<f32> = vec2<f32>(1 / screen_size.x, 0);
    var pixel_height: vec2<f32> = vec2<f32>(0, 1 / screen_size.y);

    var top: f32 = textureSample(t_depth, s_sampler, uv + pixel_height); 
    var bottom: f32 = textureSample(t_depth, s_sampler, uv - pixel_height); 
    var left: f32 = textureSample(t_depth, s_sampler, uv + pixel_width); 
    var right: f32 = textureSample(t_depth, s_sampler, uv - pixel_width); 

    var depth_diff: f32 = (top + bottom + left + right) - (depth * 4);
    return step(0.005, depth_diff) * 0.7;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var screen_size: vec2<f32> = vec2<f32>(textureDimensions(t_colour));
    var uv: vec2<f32> = position.xy / screen_size;
    var colour: vec4<f32> = textureSample(t_colour, s_sampler, uv);
    var normal: vec4<f32> = textureSample(t_normal, s_sampler, uv);

    var darken: f32 = edge_darken(uv, screen_size);

    if darken > 0 {
        colour = mix(colour, vec4<f32>(0, 0, 0, 1), darken);
    }

    var lightDirection: vec3<f32> = normalize(vec3<f32>(0.3, 0.6, 0.9));
    var diffuseStrength: f32 = max(dot(normal.xyz, lightDirection), 0.0);
    var ambientStrength: f32 = 0.3;
    var lightStrength: f32 = min(diffuseStrength + ambientStrength, 1.0);

    return vec4<f32>(colour.rgb * lightStrength, 1.0);
}
