struct VertexOutput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec4<f32>,
    @builtin(position) clip_position: vec4<f32>,
}

@group(0)
@binding(0)
var<uniform> view_projection_matrix: mat4x4<f32>;

struct MeshInfo {
    transform: mat4x4<f32>,  
    normal: mat4x4<f32>,
    colour: vec4<f32>,
}

@group(0)
@binding(1)
var<storage> scene: array<MeshInfo>;

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) mesh_index: u32) -> VertexOutput {
    var info: MeshInfo = scene[mesh_index]; 

    var output: VertexOutput;
    var world_position = info.transform * vec4<f32>(position, 1.0);
    output.position = world_position.xyz;
    output.normal = (info.normal * vec4<f32>(normal, 0)).xyz;
    output.colour = info.colour;
    output.clip_position = view_projection_matrix * world_position;
    return output;
}

struct FragmentOutput {
    @location(0) colour: vec4<f32>,
    @location(1) normal: vec4<f32>
}

@fragment
fn fs_main(vertex: VertexOutput) -> FragmentOutput {
    var output: FragmentOutput;
    output.colour = vertex.colour;
    output.normal = vec4<f32>(vertex.normal, 0.0);
    return output;
}
