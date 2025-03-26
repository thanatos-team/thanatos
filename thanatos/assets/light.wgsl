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

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var texture_size: vec2<f32> = vec2<f32>(textureDimensions(t_colour));
    var uv: vec2<f32> = position.xy / texture_size;
    var colour: vec4<f32> = textureSample(t_colour, s_sampler, uv);
    var normal: vec4<f32> = textureSample(t_normal, s_sampler, uv);

    var lightDirection: vec3<f32> = normalize(vec3<f32>(0.3, 0.6, 0.9));
    var diffuseStrength: f32 = max(dot(normal.xyz, lightDirection), 0.0);
    var ambientStrength: f32 = 0.3;

    return vec4<f32>(colour.rgb * (diffuseStrength + ambientStrength), 1.0);
}
