use std::sync::{Mutex, RwLock};

use crate::scene::Scene;

pub trait System {
    fn on_window_event(event: &winit::event::WindowEvent) {}
    fn on_frame_end() {}
    fn draw(scene: &mut Scene) {}
    fn on_world_update() {}
}

#[derive(Default)]
pub struct Systems {
    on_window_event: Vec<fn(&winit::event::WindowEvent)>,
    on_frame_end: Vec<fn()>,
    draw: Vec<fn(&mut Scene)>,
    on_world_update: Vec<fn()>,
}

static SYSTEMS: RwLock<Systems> = RwLock::new(Systems {
    on_window_event: Vec::new(),
    on_frame_end: Vec::new(),
    draw: Vec::new(),
    on_world_update: Vec::new(),
});

impl Systems {
    fn get<T, F: FnOnce(&Systems) -> T>(f: F) -> T {
        f(&SYSTEMS.read().unwrap())
    }

    fn update<F: FnOnce(&mut Systems)>(f: F) {
        let Ok(mut systems) = SYSTEMS.write() else {
            return;
        };
        f(&mut systems)
    }

    pub fn register<T: System>() {
        Self::update(|systems| {
            systems.on_window_event.push(T::on_window_event);
            systems.on_frame_end.push(T::on_frame_end);
            systems.draw.push(T::draw);
            systems.on_world_update.push(T::on_world_update);
        });
    }

    pub fn on_window_event(event: &winit::event::WindowEvent) {
        Self::get(|systems| systems.on_window_event.iter().for_each(|f| f(event)));
    }

    pub fn on_frame_end() {
        Self::get(|systems| systems.on_frame_end.iter().for_each(|f| f()));
    }

    pub fn draw() -> Scene {
        let mut scene = Scene::default();
        Self::get(|systems| systems.draw.iter().for_each(|f| f(&mut scene)));
        scene
    }

    pub fn on_world_update() {
        Self::get(|systems| systems.on_world_update.iter().for_each(|f| f()));
    }
}
