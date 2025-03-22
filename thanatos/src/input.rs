use std::{
    collections::{BTreeSet, HashSet},
    sync::Mutex,
};

use glam::Vec2;

use crate::system::System;

#[derive(Clone, Debug)]
pub struct Mouse {
    pub position: Vec2,
    pub delta: Vec2,
    pub down: BTreeSet<winit::event::MouseButton>,
}

static MOUSE: Mutex<Mouse> = Mutex::new(Mouse {
    position: Vec2::ZERO,
    delta: Vec2::ZERO,
    down: BTreeSet::new(),
});

impl Mouse {
    pub fn position() -> Vec2 {
        Self::get(|mouse| mouse.position)
    }

    pub fn delta() -> Vec2 {
        Self::get(|mouse| mouse.delta)
    }

    pub fn is_down(button: winit::event::MouseButton) -> bool {
        Self::get(|mouse| mouse.down.contains(&button))
    }

    pub fn is_left_down() -> bool {
        Self::is_down(winit::event::MouseButton::Left)
    }

    fn get<T, F: FnOnce(&Mouse) -> T>(f: F) -> T {
        f(&MOUSE.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Mouse)>(f: F) {
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
                state: winit::event::ElementState::Pressed,
                button,
                ..
            } => {
                Self::update(|mouse| {
                    mouse.down.insert(*button);
                });
            }
            winit::event::WindowEvent::MouseInput {
                state: winit::event::ElementState::Released,
                button,
                ..
            } => {
                Self::update(|mouse| {
                    mouse.down.remove(button);
                });
            }
            _ => (),
        }
    }

    fn on_frame_end() {
        Self::update(|mouse| mouse.delta = Vec2::ZERO);
    }
}

#[derive(Clone, Debug)]
pub struct Keyboard {
    pressed: BTreeSet<winit::keyboard::KeyCode>,
    down: BTreeSet<winit::keyboard::KeyCode>,
}

static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard {
    pressed: BTreeSet::new(),
    down: BTreeSet::new(),
});

impl Keyboard {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&KEYBOARD.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut KEYBOARD.lock().unwrap())
    }

    pub fn is_pressed(key: winit::keyboard::KeyCode) -> bool {
        Self::get(|keyboard| keyboard.pressed.contains(&key))
    }

    pub fn is_down(key: winit::keyboard::KeyCode) -> bool {
        Self::get(|keyboard| keyboard.down.contains(&key))
    }
}

impl System for Keyboard {
    fn on_window_event(event: &winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                Self::update(|keyboard| {
                    keyboard.pressed.insert(*code);
                    keyboard.down.insert(*code);
                });
            }
            winit::event::WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state: winit::event::ElementState::Released,
                        ..
                    },
                ..
            } => {
                Self::update(|keyboard| {
                    keyboard.pressed.remove(code);
                    keyboard.down.remove(code);
                });
            }
            _ => (),
        }
    }

    fn on_frame_end() {
        Self::update(|keyboard| keyboard.pressed.clear());
    }
}
