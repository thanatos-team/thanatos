use std::{collections::HashMap, path::Path};

use anyhow::Result;
use glam::{Vec3, Vec4};
use gltf::Glb;

use crate::renderer::{Renderer, Vertex};

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub num_indices: u32,
}

impl Mesh {
    pub fn load<T: AsRef<Path>>(path: T) -> Result<(Self, String)> {
        let model = Glb::load(&std::fs::read(path.as_ref().clone()).unwrap()).unwrap();

        let positions: Vec<Vec3> = bytemuck::cast_slice::<u8, f32>(
            &model.gltf.meshes[0].primitives[0]
                .get_attribute_data(&model, "POSITION")
                .unwrap(),
        )
        .chunks(3)
        .map(Vec3::from_slice)
        .collect();

        let normals: Vec<Vec3> = bytemuck::cast_slice::<u8, f32>(
            &model.gltf.meshes[0].primitives[0]
                .get_attribute_data(&model, "NORMAL")
                .unwrap(),
        )
        .chunks(3)
        .map(Vec3::from_slice)
        .collect();

        let vertices: Vec<Vertex> = positions
            .into_iter()
            .zip(normals)
            .map(|(position, normal)| Vertex { position, normal })
            .collect();

        let indices: Vec<u32> = model.gltf.meshes[0].primitives[0]
            .get_indices_data(&model)
            .unwrap();

        Ok((Mesh {
            vertices,
            num_indices: indices.len() as u32,
            indices,
        },path.as_ref().file_name().unwrap().to_str().map(|s| s.to_string()).unwrap()))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Material {
    pub colour: Vec4,
}

#[derive(Clone, Copy, Debug)]
pub struct MeshId(usize);
#[derive(Clone, Copy, Debug)]
pub struct MaterialId(usize);

#[derive(Default)]
pub struct Manager {
    meshes: Vec<Mesh>,
    materials: Vec<Material>,
    mesh_register: HashMap<String, MeshId>,
    default_mat: Option<MaterialId>,
}

impl Manager {
    pub fn new() -> Self {
        let mut x = Self::default();
        let default_mat = x.add_material(Material { colour: Vec4::new(1.0, 0.0, 0.86, 1.0) });
        x.default_mat = Some(default_mat);
        return x;
    }

    pub fn default_mat_id(&self) -> MaterialId {
        return self.default_mat.unwrap();
    }

    pub fn add_mesh(&mut self, mesh_and_name: (Mesh, String)) -> MeshId {
        let mesh: Mesh = mesh_and_name.0;
        let file_name: String = mesh_and_name.1;
        self.meshes.push(mesh);
        let id = MeshId(self.meshes.len() - 1);
        self.mesh_register.insert(file_name, id);
        return id;
    }

    pub fn get_mesh(&self, id: MeshId) -> Option<&Mesh> {
        self.meshes.get(id.0)
    }

    pub fn get_mesh_id_file_name(&self, file_name: String) -> Option<&MeshId> {
        match self.mesh_register.get(&file_name) {
            None => return None,
            Some(id) => return Some(id),
        }
    }

    pub fn is_mesh_loaded_file_name(&self, file_name: String) -> bool {
        match self.mesh_register.get(&file_name) {
            None => { return false; }
            Some(id) => { 
                // Until meshes can be unloaded this isnt needed
                // return self.meshes.get(id.0).is_some();
                return true;
            }
        }
    }

    // pub fn is_mesh_loaded(&self, id: MeshId) -> bool {
    //     // Until meshes can be unloaded this isnt needed
    //     return self.meshes.get(id.0).is_some();
    // }

    pub fn add_material(&mut self, material: Material) -> MaterialId {
        self.materials.push(material);
        MaterialId(self.materials.len() - 1)
    }

    pub fn get_material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0)
    }
}
