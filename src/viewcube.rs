//! Cubo de navegación (estilo FreeCAD) dibujado con egui en la esquina del visor.
//!
//! Cada cara se divide en 3×3 zonas: el centro elige la vista de esa cara, los bordes
//! la vista de la arista y las esquinas la vista isométrica del vértice (26 vistas).
//! Arrastrar sobre el cubo orbita la cámara.

use three_d::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use three_d::{Camera, InnerSpace, Vec3};

struct Face {
    normal: Vec3,
    /// Ejes de la cara vistos desde afuera: `right × up = normal`. Orientan la etiqueta.
    right: Vec3,
    up: Vec3,
    label: &'static str,
}

const X: Vec3 = Vec3::new(1.0, 0.0, 0.0);
const Y: Vec3 = Vec3::new(0.0, 1.0, 0.0);
const Z: Vec3 = Vec3::new(0.0, 0.0, 1.0);
const NX: Vec3 = Vec3::new(-1.0, 0.0, 0.0);
const NY: Vec3 = Vec3::new(0.0, -1.0, 0.0);
const NZ: Vec3 = Vec3::new(0.0, 0.0, -1.0);

// Convención de FreeCAD: la vista "Frente" mira hacia +Y (la cara frontal tiene normal -Y).
const FACES: [Face; 6] = [
    Face { normal: NY, right: X, up: Z, label: "Frente" },
    Face { normal: Y, right: NX, up: Z, label: "Atrás" },
    Face { normal: X, right: Y, up: Z, label: "Derecha" },
    Face { normal: NX, right: NY, up: Z, label: "Izquierda" },
    Face { normal: Z, right: X, up: Y, label: "Superior" },
    Face { normal: NZ, right: X, up: NY, label: "Inferior" },
];

/// Límites de las zonas 3×3 sobre una cara de lado 2 (de -1 a 1).
const CUTS: [f32; 4] = [-1.0, -0.6, 0.6, 1.0];
const SIZE: f32 = 110.0;
const MARGIN: f32 = 8.0;

pub enum Action {
    /// Mirar desde esta dirección (del objetivo hacia la cámara).
    LookFrom(Vec3),
    /// Arrastre sobre el cubo, en píxeles físicos (como los eventos de three-d).
    Orbit((f32, f32)),
}

/// Área que ocupa el cubo dentro del visor (en puntos de egui).
pub fn rect(view: Rect) -> Rect {
    Rect::from_min_size(Pos2::new(view.max.x - SIZE - MARGIN, view.min.y + MARGIN), Vec2::splat(SIZE))
}

struct Cell {
    quad: [Pos2; 4],
    direction: Vec3,
    face_center: bool,
    /// Qué tanto mira la cara a la cámara (0..1); se usa para sombrear.
    facing: f32,
}

pub fn show(ctx: &egui::Context, area: Rect, camera: &Camera) -> Option<Action> {
    let view = camera.view();
    // Solo la rotación de la cámara: el cubo se proyecta ortográficamente.
    let to_view = |v: Vec3| (view * v.extend(0.0)).truncate();
    let center = area.center();
    // Semilado en píxeles; la diagonal del cubo (√3) debe caber en el área.
    let scale = SIZE * 0.27;
    let project = |p: Vec3| {
        let v = to_view(p);
        Pos2::new(center.x + v.x * scale, center.y - v.y * scale)
    };

    let mut action = None;
    egui::Area::new(egui::Id::new("viewcube"))
        .fixed_pos(area.min)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(area.size(), Sense::click_and_drag());
            let hover = response.hover_pos();

            let mut cells = Vec::new();
            for face in &FACES {
                let facing = to_view(face.normal).z;
                if facing <= 1e-3 {
                    continue; // cara de espaldas a la cámara
                }
                for i in 0..3 {
                    for j in 0..3 {
                        let corner = |u: f32, v: f32| project(face.normal + face.right * u + face.up * v);
                        let (u0, u1, v0, v1) = (CUTS[i], CUTS[i + 1], CUTS[j], CUTS[j + 1]);
                        cells.push(Cell {
                            quad: [corner(u0, v0), corner(u1, v0), corner(u1, v1), corner(u0, v1)],
                            direction: face.normal + face.right * (i as f32 - 1.0) + face.up * (j as f32 - 1.0),
                            face_center: i == 1 && j == 1,
                            facing,
                        });
                    }
                }
            }

            // Las aristas y esquinas abarcan varias caras: se resaltan todas sus zonas.
            let hovered = hover.and_then(|p| cells.iter().find(|c| contains(&c.quad, p))).map(|c| c.direction);

            let painter = ui.painter();
            for cell in &cells {
                let shade = (150.0 + 90.0 * cell.facing) as u8;
                let fill = if hovered == Some(cell.direction) {
                    Color32::from_rgb(90, 150, 230)
                } else if cell.face_center {
                    Color32::from_rgb(shade, shade, shade)
                } else {
                    Color32::from_rgb(shade - 25, shade - 25, shade - 20)
                };
                painter.add(Shape::convex_polygon(cell.quad.to_vec(), fill, Stroke::new(1.0_f32, Color32::from_gray(70))));
            }

            for face in &FACES {
                let facing = to_view(face.normal).z;
                if facing < 0.35 {
                    continue; // demasiado de canto para leer la etiqueta
                }
                let right = to_view(face.right);
                let angle = (-right.y).atan2(right.x);
                let galley = painter.layout_no_wrap(
                    face.label.to_string(),
                    egui::FontId::proportional(11.0),
                    Color32::from_gray(30),
                );
                // TextShape rota alrededor de su esquina superior izquierda: se compensa para centrarla.
                let pos = project(face.normal) - egui::emath::Rot2::from_angle(angle) * (galley.size() / 2.0);
                painter.add(egui::epaint::TextShape::new(pos, galley, Color32::from_gray(30)).with_angle(angle));
            }

            // Pequeño triedro de ejes en la esquina inferior izquierda del área.
            let origin = area.left_bottom() + Vec2::new(12.0, -12.0);
            for (axis, color, name) in [(X, Color32::from_rgb(230, 80, 80), "X"), (Y, Color32::from_rgb(80, 200, 80), "Y"), (Z, Color32::from_rgb(90, 140, 240), "Z")] {
                let v = to_view(axis);
                let tip = origin + Vec2::new(v.x, -v.y) * 16.0;
                painter.line_segment([origin, tip], Stroke::new(2.0_f32, color));
                painter.text(tip + Vec2::new(v.x, -v.y) * 5.0, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(9.0), color);
            }

            if response.clicked() {
                action = hovered.map(|d| Action::LookFrom(d.normalize()));
            } else if response.dragged() {
                let delta = response.drag_delta() * ctx.pixels_per_point();
                action = Some(Action::Orbit((delta.x, delta.y)));
            }
        });
    action
}

/// Punto dentro de un cuadrilátero convexo, sin importar el sentido de los vértices.
fn contains(quad: &[Pos2; 4], p: Pos2) -> bool {
    let mut sign = 0.0f32;
    for k in 0..4 {
        let (a, b) = (quad[k], quad[(k + 1) % 4]);
        let cross = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}
