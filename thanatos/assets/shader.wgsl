struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) colour: vec4<f32>,
}

@group(0)
@binding(0)
var<uniform> view_projection_matrix: mat4x4<f32>;

struct MeshInfo {
    transform: mat4x4<f32>,  
    colour: vec4<f32>,
}

@group(0)
@binding(1)
var<storage> scene: array<MeshInfo>;

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) mesh_index: u32) -> VertexOutput {
    var info: MeshInfo = scene[mesh_index]; 

    var output: VertexOutput;
    output.position = view_projection_matrix * info.transform * vec4<f32>(position, 1.0);
    output.normal = normal;
    output.colour = info.colour;
    return output;
}

@fragment
fn fs_main(vertex: VertexOutput) -> @location(0) vec4<f32> {
    return vertex.colour * dot(vertex.normal, normalize(vec3<f32>(0.2, 0.4, 0.6)));
}
