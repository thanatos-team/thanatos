use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3, Vec4};
use gltf::{Glb, MeshPrimitive};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
}

impl Vertex {
    pub const fn new(position: Vec3, normal: Vec3) -> Self {
        Self {
            position: [position.x, position.y, position.z],
            normal: [normal.x, normal.y, normal.z],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct VertexData {
    pub vertex: Vertex,
    pub mesh_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Debug, Zeroable, Default)]
pub struct MeshInfo {
    transform: Mat4,
    normal: Mat4,
    pub colour: Vec4,
}

impl MeshInfo {
    pub fn transform(&self) -> Mat4 {
        self.transform
    }

    pub fn set_transform(&mut self, transform: Mat4) {
        self.transform = transform;
        self.normal = Mat4::from_quat(transform.to_scale_rotation_translation().1);
    }
}

#[derive(Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub info: MeshInfo,
}

impl Mesh {
    fn from_glb_primitive(glb: &Glb, primitive: &MeshPrimitive) -> Option<Self> {
        let indices = primitive.get_indices_data(glb);

        let positions = primitive.get_attribute_data(glb, "POSITION")?;
        let positions = bytemuck::cast_slice::<u8, Vec3>(&positions);
        let normals = primitive.get_attribute_data(glb, "NORMAL")?;
        let normals = bytemuck::cast_slice::<u8, Vec3>(&normals);

        let colour = primitive
            .material
            .and_then(|index| glb.gltf.materials.get(index))
            .and_then(|material| material.pbr.base_color_factor)
            .map(Vec4::from_array)
            .unwrap_or(Vec4::ONE);

        Some(Self {
            indices,
            vertices: positions
                .into_iter()
                .zip(normals)
                .map(|(position, normal)| Vertex::new(*position, *normal))
                .collect(),
            info: MeshInfo {
                transform: Mat4::IDENTITY,
                normal: Mat4::IDENTITY,
                colour,
            },
        })
    }

    pub fn from_glb(glb: &Glb) -> Vec<Self> {
        glb.gltf
            .meshes
            .iter()
            .flat_map(|mesh| &mesh.primitives)
            .filter_map(|primitive| Self::from_glb_primitive(glb, primitive))
            .collect()
    }
}
