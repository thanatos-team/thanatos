use glam::Mat4;

use crate::mesh::{Mesh, MeshInfo, VertexData};

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub vertices: Vec<VertexData>,
    pub indices: Vec<u32>,
    pub infos: Vec<MeshInfo>,
}

impl Scene {
    pub fn add(&mut self, mesh: &Mesh, transform: Mat4) {
        self.indices.extend_from_slice(
            &mesh
                .indices
                .iter()
                .map(|index| index + self.vertices.len() as u32)
                .collect::<Vec<_>>(),
        );

        let mesh_index = self.infos.len() as u32;
        self.vertices.extend_from_slice(
            &mesh
                .vertices
                .clone()
                .into_iter()
                .map(|vertex| VertexData { vertex, mesh_index })
                .collect::<Vec<_>>(),
        );

        let mut info = mesh.info.clone();
        info.transform = transform * info.transform;
        self.infos.push(info);
    }
}
