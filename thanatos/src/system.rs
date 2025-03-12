use crate::scene::Scene;

pub trait System {
    fn on_window_event(event: &winit::event::WindowEvent) {}
    fn on_frame_end() {}
    fn draw(scene: &mut Scene) {}
}

#[derive(Default)]
pub struct Systems {
    on_window_event: Vec<fn(&winit::event::WindowEvent)>,
    on_frame_end: Vec<fn()>,
    draw: Vec<fn(&mut Scene)>,
}

impl Systems {
    pub fn register<T: System>(&mut self) {
        self.on_window_event.push(T::on_window_event);
        self.on_frame_end.push(T::on_frame_end);
        self.draw.push(T::draw);
    }

    pub fn on_window_event(&self, event: &winit::event::WindowEvent) {
        self.on_window_event.iter().for_each(|f| f(event));
    }

    pub fn on_frame_end(&self) {
        self.on_frame_end.iter().for_each(|f| f());
    }

    pub fn draw(&self) -> Scene {
        let mut scene = Scene::default();
        self.draw.iter().for_each(|f| f(&mut scene));
        scene
    }
}
