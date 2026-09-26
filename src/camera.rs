//! Control de cámara estilo CAD: botón izquierdo orbita, derecho/central desplaza,
//! la rueda acerca. Eje Z hacia arriba, como en la mayoría de los STL.

use three_d::{Camera, Event, InnerSpace, MetricSpace, MouseButton, Vec3, Viewport, degrees, vec3};

pub const UP: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const FOV_DEGREES: f32 = 45.0;

pub struct CameraController {
    target: Vec3,
    min_distance: f32,
    max_distance: f32,
}

impl CameraController {
    pub fn new(target: Vec3) -> Self {
        Self { target, min_distance: 0.01, max_distance: 1e5 }
    }

    /// Devuelve `true` si la cámara cambió.
    pub fn handle_events(&mut self, camera: &mut Camera, events: &mut [Event]) -> bool {
        let mut changed = false;
        for event in events.iter_mut() {
            match event {
                Event::MouseMotion { handled: true, .. } | Event::MouseWheel { handled: true, .. } => {}
                Event::MouseMotion { button: Some(button), delta, handled, .. } => {
                    match button {
                        MouseButton::Left => {
                            camera.rotate_around_with_fixed_up(self.target, 0.01 * delta.0, 0.01 * delta.1);
                        }
                        MouseButton::Right | MouseButton::Middle => {
                            // Escala para que el punto bajo el cursor siga al ratón en el plano del objetivo.
                            let distance = camera.position().distance(self.target);
                            let fov = FOV_DEGREES.to_radians();
                            let per_pixel = 2.0 * distance * (fov / 2.0).tan() / camera.viewport().height as f32;
                            let offset = (camera.up_orthogonal() * delta.1 - camera.right_direction() * delta.0) * per_pixel;
                            camera.translate(offset);
                            self.target += offset;
                        }
                    }
                    *handled = true;
                    changed = true;
                }
                Event::MouseWheel { delta, handled, .. } => {
                    let speed = 0.01 * self.target.distance(camera.position()) + 0.001;
                    camera.zoom_towards(self.target, speed * delta.1, self.min_distance, self.max_distance);
                    *handled = true;
                    changed = true;
                }
                _ => {}
            }
        }
        changed
    }

    /// Encuadra la caja `min..max` manteniendo la dirección de vista actual.
    pub fn fit(&mut self, camera: &mut Camera, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let radius = ((max - min).magnitude() * 0.5).max(1e-3);
        // El ángulo limitante es el menor entre el vertical y el horizontal (ventanas angostas).
        let half_v = (FOV_DEGREES / 2.0).to_radians();
        let half_h = (half_v.tan() * camera.viewport().aspect()).atan();
        let distance = radius / half_v.min(half_h).sin() * 1.1;

        let direction = camera.view_direction();
        self.target = center;
        self.min_distance = radius * 0.01;
        self.max_distance = radius * 100.0;
        camera.set_view(center - direction * distance, center, UP);
        camera.set_perspective_projection(degrees(FOV_DEGREES), distance * 0.001, distance * 100.0);
    }

    pub fn new_camera(viewport: Viewport) -> Camera {
        Camera::new_perspective(
            viewport,
            vec3(1.0, -1.5, 1.0),
            vec3(0.0, 0.0, 0.0),
            UP,
            degrees(FOV_DEGREES),
            0.01,
            1000.0,
        )
    }
}
