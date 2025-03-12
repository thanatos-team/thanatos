use std::sync::Mutex;

use glam::{Mat4, Quat, Vec2, Vec3};

use crate::{input::Mouse, system::System};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    centre: Vec3,
    distance: f32,
    angle: f32,
    pitch: f32,
}

static CAMERA: Mutex<Camera> = Mutex::new(Camera {
    centre: Vec3::ZERO,
    distance: 10.0,
    angle: 0.0,
    pitch: 0.0,
});

impl Camera {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&CAMERA.lock().unwrap())
    }

    fn get_mut<F: FnOnce(&mut Camera)>(f: F) {
        let Ok(mut camera) = CAMERA.lock() else {
            return;
        };
        f(&mut camera);

        camera.pitch = camera.pitch.clamp(0.0, std::f32::consts::FRAC_PI_2 - 0.01);
    }

    pub fn get_matrix() -> Mat4 {
        Self::get(|camera| {
            let rotation = Quat::from_euler(glam::EulerRot::YZX, 0.0, -camera.angle, camera.pitch);
            let eye = camera.centre + rotation * Vec3::Y * camera.distance;

            Mat4::look_at_rh(eye, camera.centre, Vec3::Z)
        })
    }

    pub fn rotation() -> Quat {
        Self::get(|camera| Quat::from_rotation_z(-camera.angle))
    }

    pub fn set_centre(at: Vec3) {
        Self::get_mut(|camera| camera.centre = at);
    }
}

impl System for Camera {
    fn on_frame_end() {
        if !Mouse::is_left_down() || Mouse::delta() == Vec2::ZERO {
            return;
        }

        Self::get_mut(|camera| {
            camera.angle += Mouse::delta().x * 0.005;
            camera.pitch += Mouse::delta().y * 0.005;
        });
    }
}
