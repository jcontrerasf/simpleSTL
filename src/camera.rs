//! Control de cámara estilo CAD: botón izquierdo orbita, derecho/central desplaza,
//! la rueda acerca. Eje Z hacia arriba, como en la mayoría de los STL.
//!
//! La órbita es tipo "tornamesa": la dirección de vista se guarda como azimut (alrededor
//! de Z) y elevación. Así las vistas superior/inferior quedan bien definidas (a diferencia
//! de un vector "arriba" fijo en +Z, que degenera al mirar en vertical).

use std::f32::consts::{FRAC_PI_2, PI};

use three_d::{Camera, Event, InnerSpace, MetricSpace, MouseButton, Vec3, Viewport, degrees, vec3};

const FOV_DEGREES: f32 = 45.0;
/// Duración de la transición al elegir una vista con el cubo, en milisegundos.
const ANIMATION_MS: f64 = 250.0;

struct Animation {
    from: (f32, f32),
    to: (f32, f32),
    start_ms: Option<f64>,
}

pub struct CameraController {
    target: Vec3,
    /// Ángulo alrededor de Z, desde +X, hacia donde está la cámara vista desde el objetivo.
    azimuth: f32,
    /// Ángulo sobre el plano XY, en [-π/2, π/2].
    elevation: f32,
    min_distance: f32,
    max_distance: f32,
    animation: Option<Animation>,
    orthographic: bool,
}

impl CameraController {
    pub fn new(target: Vec3) -> Self {
        // Vista isométrica desde el frente-derecha-arriba.
        Self {
            target,
            azimuth: (-60f32).to_radians(),
            elevation: 30f32.to_radians(),
            min_distance: 0.01,
            max_distance: 1e5,
            animation: None,
            orthographic: false,
        }
    }

    pub fn new_camera(&self, viewport: Viewport) -> Camera {
        let mut camera = Camera::new_perspective(
            viewport,
            vec3(1.0, 1.0, 1.0),
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 0.0, 1.0),
            degrees(FOV_DEGREES),
            0.01,
            1000.0,
        );
        self.apply(&mut camera, 2.0);
        camera
    }

    /// Dirección unitaria del objetivo hacia la cámara.
    fn direction(azimuth: f32, elevation: f32) -> Vec3 {
        let (cos_el, sin_el) = (elevation.cos(), elevation.sin());
        vec3(cos_el * azimuth.cos(), cos_el * azimuth.sin(), sin_el)
    }

    /// Coloca la cámara según azimut/elevación a la distancia dada. El vector "arriba" es la
    /// derivada respecto a la elevación: en los polos sigue definido por el azimut.
    fn apply(&self, camera: &mut Camera, distance: f32) {
        let (az, el) = (self.azimuth, self.elevation);
        let up = vec3(-el.sin() * az.cos(), -el.sin() * az.sin(), el.cos());
        camera.set_view(self.target + Self::direction(az, el) * distance, self.target, up);
    }

    fn orbit(&mut self, camera: &mut Camera, dx: f32, dy: f32) {
        self.animation = None;
        let distance = camera.position().distance(self.target);
        self.azimuth -= 0.01 * dx;
        self.elevation = (self.elevation + 0.01 * dy).clamp(-FRAC_PI_2, FRAC_PI_2);
        self.apply(camera, distance);
    }

    /// Rota la vista como si se arrastrara con el botón izquierdo (lo usa el cubo de vista).
    pub fn orbit_by_drag(&mut self, camera: &mut Camera, delta: (f32, f32)) {
        self.orbit(camera, delta.0, delta.1);
    }

    /// Inicia una transición animada para mirar desde `direction` (del objetivo a la cámara).
    pub fn look_from(&mut self, direction: Vec3) {
        let d = direction.normalize();
        let elevation = d.z.clamp(-1.0, 1.0).asin();
        // Vistas verticales: se orientan como en FreeCAD (superior con +Y hacia arriba en pantalla).
        let horizontal = (d.x * d.x + d.y * d.y).sqrt();
        let azimuth = if horizontal < 1e-4 { -FRAC_PI_2 } else { d.y.atan2(d.x) };
        // Camino más corto en azimut.
        let mut delta = (azimuth - self.azimuth) % (2.0 * PI);
        if delta > PI {
            delta -= 2.0 * PI;
        } else if delta < -PI {
            delta += 2.0 * PI;
        }
        self.animation = Some(Animation {
            from: (self.azimuth, self.elevation),
            to: (self.azimuth + delta, elevation),
            start_ms: None,
        });
    }

    /// Avanza la animación en curso. Devuelve `true` mientras siga animando (hay que redibujar).
    pub fn animate(&mut self, camera: &mut Camera, time_ms: f64) -> bool {
        let Some(animation) = &mut self.animation else {
            return false;
        };
        let start = *animation.start_ms.get_or_insert(time_ms);
        let t = ((time_ms - start) / ANIMATION_MS).clamp(0.0, 1.0) as f32;
        let s = t * t * (3.0 - 2.0 * t); // suavizado
        let (from, to) = (animation.from, animation.to);
        self.azimuth = from.0 + (to.0 - from.0) * s;
        self.elevation = from.1 + (to.1 - from.1) * s;
        if t >= 1.0 {
            self.animation = None;
        }
        let distance = camera.position().distance(self.target);
        self.apply(camera, distance);
        true
    }

    /// Devuelve `true` si la cámara cambió.
    pub fn handle_events(&mut self, camera: &mut Camera, events: &mut [Event]) -> bool {
        let mut changed = false;
        for event in events.iter_mut() {
            match event {
                Event::MouseMotion { handled: true, .. } | Event::MouseWheel { handled: true, .. } => {}
                Event::MouseMotion { button: Some(button), delta, handled, .. } => {
                    match button {
                        MouseButton::Left => self.orbit(camera, delta.0, delta.1),
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

    pub fn is_orthographic(&self) -> bool {
        self.orthographic
    }

    pub fn set_orthographic(&mut self, camera: &mut Camera, orthographic: bool) {
        self.orthographic = orthographic;
        let distance = camera.position().distance(self.target);
        self.apply_projection(camera, distance);
    }

    /// En three-d, la altura de la proyección ortográfica se multiplica por la distancia al
    /// objetivo (y se recalcula en cada `set_view`). Con altura 2·tan(fov/2) el encuadre es el
    /// mismo que en perspectiva, así que zoom, desplazamiento y encuadre no cambian.
    fn apply_projection(&self, camera: &mut Camera, distance: f32) {
        let far = distance * 100.0;
        if self.orthographic {
            let height = 2.0 * (FOV_DEGREES.to_radians() / 2.0).tan();
            // Plano cercano negativo: no recorta lo que queda entre la cámara y el objetivo.
            camera.set_orthographic_projection(height, -far, far);
        } else {
            camera.set_perspective_projection(degrees(FOV_DEGREES), distance * 0.001, far);
        }
    }

    /// Encuadra la caja `min..max` manteniendo la dirección de vista actual.
    pub fn fit(&mut self, camera: &mut Camera, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let radius = ((max - min).magnitude() * 0.5).max(1e-3);
        // El ángulo limitante es el menor entre el vertical y el horizontal (ventanas angostas).
        let half_v = (FOV_DEGREES / 2.0).to_radians();
        let half_h = (half_v.tan() * camera.viewport().aspect()).atan();
        let distance = radius / half_v.min(half_h).sin() * 1.1;

        self.target = center;
        self.min_distance = radius * 0.01;
        self.max_distance = radius * 100.0;
        self.apply(camera, distance);
        self.apply_projection(camera, distance);
    }
}
