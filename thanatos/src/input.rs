use std::sync::Mutex;

use glam::Vec2;

use crate::system::System;

#[derive(Clone, Copy, Debug)]
pub struct Mouse {
    pub position: Vec2,
    pub delta: Vec2,
    pub left_down: bool,
}

impl Mouse {
    pub fn get() -> Mouse {
        *MOUSE.lock().unwrap()
    }

    pub fn update<F: FnOnce(&mut Mouse)>(f: F) {
        let Ok(mut mouse) = MOUSE.lock() else { return };
        f(&mut mouse)
    }
}

impl System for Mouse {
    fn on_window_event(event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::CursorMoved { position, .. } => Self::update(|mouse| {
                let position = Vec2::new(position.x as f32, position.y as f32);
                mouse.delta += position - mouse.position;
                mouse.position = position;
            }),
            winit::event::WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                Self::update(|mouse| mouse.left_down = state.is_pressed());
            }
            _ => (),
        }
    }

    fn on_frame_end() {
        Self::update(|mouse| mouse.delta = Vec2::ZERO);
    }
}

pub static MOUSE: Mutex<Mouse> = Mutex::new(Mouse {
    position: Vec2::ZERO,
    delta: Vec2::ZERO,
    left_down: false,
});
