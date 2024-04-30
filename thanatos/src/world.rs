// world_file struct
#[derive(Deserialize, Debug)]
pub struct WorldFile {
    pub(crate) version: u32,
    pub(crate) assets: Vec<WorldFileAsset>,
    pub(crate) world: Vec<WorldFileAssetSpawn> 
}

#[derive(Deserialize, Debug)]
pub struct WorldFileAsset {
    pub(crate) id: u32,
    pub(crate)file_name: String,
    pub(crate)asset_type: WorldFileAssetType
}

#[derive(Deserialize, Debug)]
pub enum WorldFileAssetType {
    StaticMesh,
    Archetype,
}
#[derive(Deserialize, Debug)]
pub enum WorldFileAssetSpawn {
    StaticMesh(WorldFileStaticMesh),
    Archetype { asset_id: u32 }, 
}

// static_mesh struct
#[derive(Deserialize, Debug)]
pub struct WorldFileStaticMesh {
    pub(crate) transform: Transform,
    pub(crate) asset_id: u32,
}
// asset struct

#[derive(Archetype)]
pub struct StaticMesh {
    pub render: RenderObject,
    pub transform: Transform,
}

use std::{collections::HashMap, fs::File, io::BufReader, path::Path};
use glam::Vec4;
use tecs::impl_archetype;
use anyhow::Ok;
use log::warn;
use serde::Deserialize;
use thanatos_macros::Archetype;

use crate::{assets::{self, Material, MeshId}, renderer::RenderObject, transform::Transform, World};

pub fn read_world_from_file<P: AsRef<Path>>(path: P) -> Result<WorldFile, anyhow::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let world = serde_json::from_reader::<_, WorldFile>(reader)?;
    
    Ok(world)
}

pub fn spawn_from_world_file_struct(game_world: &World, file: WorldFile) -> bool {
    let WorldFile { version, assets, world } = file;
    if version != 1 { return false; }

    // Verify assets contained are found locally
    let asset_manager = game_world.get::<assets::Manager>().unwrap();
    let mut asset_id_to_mesh_id: HashMap<u32, &MeshId> = HashMap::new();
    for asset in assets {
        match asset.asset_type {
            WorldFileAssetType::Archetype => {
                panic!()
                // TODO!()
            },
            WorldFileAssetType::StaticMesh => {
                let m_id = asset_manager.get_mesh_id_file_name(asset.file_name);
                if m_id.is_some() {
                    asset_id_to_mesh_id.insert(asset.id, m_id.unwrap());                    
                    println!("Mesh Found, Moving on");
                } else {
                    warn!("Failed to load world file struct into world due to not having the required mesh");
                    return false
                }
                
            }
        }
    }
    let default_mat = asset_manager.default_mat_id();
    // iterate over the SMs and instantiate
    for spawn in world {
        match spawn {
            WorldFileAssetSpawn::StaticMesh(sm) => {
                let mesh_id = **asset_id_to_mesh_id.get(&sm.asset_id).unwrap();
                game_world.spawn(StaticMesh {
                    render: RenderObject { mesh: mesh_id, material: default_mat },
                    transform: sm.transform,
                });
            },
            WorldFileAssetSpawn::Archetype { asset_id } => todo!(),
        }
    }

    return false;
}