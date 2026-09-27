//! Manipulador 3D (trasladar/rotar) sobre el objeto seleccionado, usando `transform-gizmo`.
//!
//! No se usa `GizmoExt::interact` de transform-gizmo-egui: registra un widget de egui
//! bajo el cursor en todo momento, y three-d marcaría todos los arrastres como consumidos
//! por la interfaz, bloqueando la cámara. En su lugar se alimenta el gizmo con los eventos
//! de three-d y egui solo se usa para dibujarlo.

use three_d::egui;
use three_d::{Camera, Event, Mat4, MouseButton, Quat, Vec3, vec3};
use transform_gizmo_egui::math::{DQuat, DVec3, Transform};
use transform_gizmo_egui::prelude::*;

/// Posición y orientación de un objeto. Las mallas se guardan centradas en su origen
/// local, así que la traslación es también el centro de rotación.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub translation: Vec3,
    pub rotation: Quat,
}

impl Pose {
    pub fn at(translation: Vec3) -> Self {
        Self { translation, rotation: Quat::new(1.0, 0.0, 0.0, 0.0) }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_translation(self.translation) * Mat4::from(self.rotation)
    }

    pub fn apply(&self, p: Vec3) -> Vec3 {
        self.rotation * p + self.translation
    }

    fn to_gizmo(self) -> Transform {
        let (t, q) = (self.translation, self.rotation);
        Transform::from_scale_rotation_translation(
            DVec3::ONE,
            DQuat::from_xyzw(q.v.x as f64, q.v.y as f64, q.v.z as f64, q.s as f64),
            DVec3::new(t.x as f64, t.y as f64, t.z as f64),
        )
    }

    fn from_gizmo(t: &Transform) -> Self {
        let (p, q) = (t.translation, t.rotation);
        Self {
            translation: vec3(p.x as f32, p.y as f32, p.z as f32),
            rotation: Quat::new(q.s as f32, q.v.x as f32, q.v.y as f32, q.v.z as f32),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Translate,
    Rotate,
    Both,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Translate, Mode::Rotate, Mode::Both];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Translate => "Trasladar",
            Mode::Rotate => "Rotar",
            Mode::Both => "Ambos",
        }
    }

    fn gizmo_modes(self) -> EnumSet<GizmoMode> {
        let translate = GizmoMode::TranslateX | GizmoMode::TranslateY | GizmoMode::TranslateZ;
        let rotate = GizmoMode::RotateX | GizmoMode::RotateY | GizmoMode::RotateZ;
        match self {
            Mode::Translate => translate,
            Mode::Rotate => rotate,
            Mode::Both => translate | rotate,
        }
    }
}

/// Clic izquierdo en el visor que no tocó el gizmo ni se arrastró: sirve para seleccionar.
#[derive(Clone, Copy, Debug)]
pub struct Click {
    /// Posición en las coordenadas físicas de three-d (origen abajo a la izquierda).
    pub position: three_d::PhysicalPoint,
}

pub struct Manipulator {
    gizmo: Gizmo,
    pub mode: Mode,
    /// Cursor en puntos de egui (origen arriba a la izquierda).
    cursor: (f32, f32),
    left_down: bool,
    pressed: bool,
    released: bool,
    snapping: bool,
    /// El arrastre en curso empezó sobre el gizmo: la cámara no debe recibirlo.
    dragging_gizmo: bool,
    press_position: Option<three_d::PhysicalPoint>,
    moved_since_press: f32,
}

impl Manipulator {
    pub fn new() -> Self {
        Self {
            gizmo: Gizmo::default(),
            mode: Mode::Both,
            cursor: (-1.0, -1.0),
            left_down: false,
            pressed: false,
            released: false,
            snapping: false,
            dragging_gizmo: false,
            press_position: None,
            moved_since_press: 0.0,
        }
    }

    /// Lee el estado del ratón de este cuadro. Debe llamarse antes de que egui marque eventos.
    pub fn track_input(&mut self, events: &[Event], device_pixel_ratio: f32, window_height: u32) {
        self.pressed = false;
        self.released = false;
        let to_points = |p: three_d::PhysicalPoint| {
            (p.x / device_pixel_ratio, (window_height as f32 - p.y) / device_pixel_ratio)
        };
        for event in events {
            match *event {
                Event::MouseMotion { position, delta, .. } => {
                    self.cursor = to_points(position);
                    self.moved_since_press += delta.0.abs() + delta.1.abs();
                }
                Event::MousePress { button: MouseButton::Left, position, .. } => {
                    self.cursor = to_points(position);
                    self.left_down = true;
                    self.pressed = true;
                    self.press_position = Some(position);
                    self.moved_since_press = 0.0;
                }
                Event::MouseRelease { button: MouseButton::Left, position, .. } => {
                    self.cursor = to_points(position);
                    self.left_down = false;
                    self.released = true;
                }
                Event::ModifiersChange { modifiers } => self.snapping = modifiers.ctrl,
                Event::MouseLeave => self.cursor = (-1.0, -1.0),
                _ => {}
            }
        }
    }

    /// Actualiza el gizmo para el objeto con pose `pose` y lo dibuja. Devuelve la pose
    /// nueva si el usuario lo está arrastrando. `viewport` es el área del visor en puntos de egui.
    pub fn update(&mut self, ctx: &egui::Context, camera: &Camera, viewport: egui::Rect, pose: Option<Pose>) -> Option<Pose> {
        let pose = pose?;
        self.gizmo.update_config(GizmoConfig {
            view_matrix: row_matrix(camera.view()),
            projection_matrix: row_matrix(camera.projection()),
            viewport,
            modes: self.mode.gizmo_modes(),
            orientation: GizmoOrientation::Global,
            snapping: self.snapping,
            snap_angle: 15f32.to_radians(),
            snap_distance: 1.0,
            pixels_per_point: ctx.pixels_per_point(),
            ..Default::default()
        });

        let hovered = viewport.contains(egui::pos2(self.cursor.0, self.cursor.1));
        let result = self.gizmo.update(
            GizmoInteraction {
                cursor_pos: self.cursor,
                hovered,
                drag_started: self.pressed && hovered,
                dragging: self.left_down,
            },
            &[pose.to_gizmo()],
        );
        if self.pressed && self.gizmo.is_focused() {
            self.dragging_gizmo = true;
        }

        let draw = self.gizmo.draw();
        let mesh = egui::Mesh {
            indices: draw.indices,
            vertices: draw
                .vertices
                .into_iter()
                .zip(draw.colors)
                .map(|(pos, [r, g, b, a])| egui::epaint::Vertex {
                    pos: pos.into(),
                    uv: egui::Pos2::ZERO,
                    color: egui::Rgba::from_rgba_premultiplied(r, g, b, a).into(),
                })
                .collect(),
            ..Default::default()
        };
        ctx.layer_painter(egui::LayerId::new(egui::Order::Middle, egui::Id::new("gizmo")))
            .with_clip_rect(viewport)
            .add(mesh);

        result.and_then(|(_, transforms)| transforms.first().map(Pose::from_gizmo))
    }

    /// Si el gizmo está siendo arrastrado, marca los eventos del ratón como consumidos
    /// para que la cámara no se mueva. Devuelve el clic de selección, si lo hubo.
    pub fn consume_events(&mut self, events: &mut [Event]) -> Option<Click> {
        let mut click = None;
        if self.released {
            // Tolerancia de unos píxeles para no confundir un clic con una órbita corta.
            if !self.dragging_gizmo && self.moved_since_press < 4.0 {
                click = self.press_position.map(|position| Click { position });
            }
            self.press_position = None;
        }
        if self.dragging_gizmo {
            for event in events.iter_mut() {
                if let Event::MouseMotion { handled, .. }
                | Event::MousePress { handled, .. }
                | Event::MouseRelease { handled, .. } = event
                {
                    *handled = true;
                }
            }
        }
        if self.released {
            self.dragging_gizmo = false;
        }
        click
    }
}

/// three-d usa matrices de cgmath (por columnas); el gizmo las espera por filas.
fn row_matrix(m: Mat4) -> mint::RowMatrix4<f64> {
    let row = |i: usize| mint::Vector4 { x: m.x[i] as f64, y: m.y[i] as f64, z: m.z[i] as f64, w: m.w[i] as f64 };
    mint::RowMatrix4 { x: row(0), y: row(1), z: row(2), w: row(3) }
}
