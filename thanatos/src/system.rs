pub trait System {
    fn on_window_event(event: &winit::event::WindowEvent) {}
    fn on_frame_end() {}
}

#[derive(Default)]
pub struct Systems {
    on_window_event: Vec<fn(&winit::event::WindowEvent)>,
    on_frame_end: Vec<fn()>,
}

impl Systems {
    pub fn register<T: System>(&mut self) {
        self.on_window_event.push(T::on_window_event);
        self.on_frame_end.push(T::on_frame_end);
    }

    pub fn on_window_event(&self, event: &winit::event::WindowEvent) {
        self.on_window_event.iter().for_each(|f| f(event));
    }

    pub fn on_frame_end(&self) {
        self.on_frame_end.iter().for_each(|f| f());
    }
}

