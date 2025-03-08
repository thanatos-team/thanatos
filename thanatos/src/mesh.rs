use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use gltf::{Glb, MeshPrimitive};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
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
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct MeshInfo {
    pub transform: Mat4,
}

#[derive(Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub info: MeshInfo
}

impl Mesh {
    fn from_glb_primitive(glb: &Glb, primitive: &MeshPrimitive) -> Option<Self> {
        let indices = primitive.get_indices_data(glb);

        let positions = primitive.get_attribute_data(glb, "POSITION")?;
        let positions = bytemuck::cast_slice::<u8, Vec3>(&positions);
        let normals = primitive.get_attribute_data(glb, "NORMAL")?;
        let normals = bytemuck::cast_slice::<u8, Vec3>(&normals);

        Some(Self {
            indices,
            vertices: positions
                .into_iter()
                .zip(normals)
                .map(|(position, normal)| Vertex::new(*position, *normal))
                .collect(),
            info: MeshInfo::default(),
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
