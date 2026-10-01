//! Herramientas Medir y Regla: medidas a lo largo de un eje (nunca en diagonal), reglas
//! con marcas regulares, el enganche de objetos en movimiento a esas marcas y su dibujo
//! sobre el visor.

use three_d::egui::{self, Color32, Pos2, Stroke};
use three_d::{InnerSpace, Vec3, vec3};

use crate::snap::SnapKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

    pub fn unit(self) -> Vec3 {
        match self {
            Axis::X => vec3(1.0, 0.0, 0.0),
            Axis::Y => vec3(0.0, 1.0, 0.0),
            Axis::Z => vec3(0.0, 0.0, 1.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        }
    }

    /// Los mismos colores que el triedro del cubo de vista.
    pub fn color(self) -> Color32 {
        match self {
            Axis::X => Color32::from_rgb(230, 80, 80),
            Axis::Y => Color32::from_rgb(80, 200, 80),
            Axis::Z => Color32::from_rgb(90, 140, 240),
        }
    }

    /// Eje en que más se extiende `v`.
    pub fn dominant(v: Vec3) -> Axis {
        let (x, y, z) = (v.x.abs(), v.y.abs(), v.z.abs());
        if x >= y && x >= z {
            Axis::X
        } else if y >= z {
            Axis::Y
        } else {
            Axis::Z
        }
    }
}

/// Eje de una medida de `a` a `b`: el fijado por el usuario o el dominante.
pub fn axis_between(a: Vec3, b: Vec3, locked: Option<Axis>) -> Axis {
    locked.unwrap_or_else(|| Axis::dominant(b - a))
}

/// Medición en curso o terminada. Mientras falta `b`, se mide hasta el punto bajo el cursor.
#[derive(Clone, Copy, Debug, Default)]
pub struct Measurement {
    pub a: Option<Vec3>,
    pub b: Option<Vec3>,
}

impl Measurement {
    /// Un clic: fija A, luego B, y el siguiente empieza una medición nueva.
    pub fn click(&mut self, p: Vec3) {
        *self = match (self.a, self.b) {
            (Some(a), None) => Measurement { a: Some(a), b: Some(p) },
            _ => Measurement { a: Some(p), b: None },
        };
    }
}

/// Regla a lo largo de un eje, desde `origin` en el sentido `sign` (±1) del eje.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ruler {
    pub origin: Vec3,
    pub axis: Axis,
    pub sign: f32,
    pub length: f32,
    pub spacing: f32,
}

pub const MIN_SPACING: f32 = 0.1;

impl Ruler {
    /// Regla de `a` hacia `b` sobre el eje dado: el cero queda en `a`. `None` si `b` no se
    /// separa de `a` en ese eje.
    pub fn between(a: Vec3, b: Vec3, axis: Axis, spacing: f32) -> Option<Ruler> {
        let along = (b - a).dot(axis.unit());
        (along.abs() > 1e-4).then(|| Ruler { origin: a, axis, sign: along.signum(), length: along.abs(), spacing })
    }

    pub fn direction(&self) -> Vec3 {
        self.axis.unit() * self.sign
    }

    pub fn end(&self) -> Vec3 {
        self.origin + self.direction() * self.length
    }

    /// Posición de la marca `k` (a `k · spacing` del origen).
    pub fn mark(&self, k: u32) -> Vec3 {
        self.origin + self.direction() * (k as f32 * self.spacing)
    }

    pub fn mark_count(&self) -> u32 {
        (self.length / self.spacing.max(MIN_SPACING) + 1e-4).floor() as u32 + 1
    }

    /// Marca más cercana a `p` (medida sobre el eje) y la distancia con signo hasta ella.
    fn nearest_mark(&self, p: Vec3) -> (u32, f32) {
        let s = (p - self.origin).dot(self.direction());
        let k = (s / self.spacing.max(MIN_SPACING)).round().clamp(0.0, (self.mark_count() - 1) as f32) as u32;
        (k, k as f32 * self.spacing - s)
    }
}

/// Resultado de enganchar un objeto en movimiento a las reglas.
#[derive(Clone, Debug)]
pub struct RulerSnap {
    /// Desplazamiento a sumar a la traslación del objeto.
    pub offset: Vec3,
    /// Marcas enganchadas, para resaltarlas.
    pub marks: Vec<Vec3>,
}

/// Engancha `points` (puntos de referencia del objeto, en el mundo) a la marca más
/// cercana de las reglas, eje por eje, si está a menos de `tolerance` (unidades del
/// mundo). Solo en los ejes de `axes`: los que el arrastre está moviendo.
pub fn ruler_snap(rulers: &[Ruler], points: &[Vec3], tolerance: f32, axes: &[Axis]) -> RulerSnap {
    let mut result = RulerSnap { offset: vec3(0.0, 0.0, 0.0), marks: Vec::new() };
    for &axis in axes {
        let best = rulers
            .iter()
            .filter(|r| r.axis == axis)
            .flat_map(|r| points.iter().map(move |&p| (r, r.nearest_mark(p))))
            .filter(|(_, (_, d))| d.abs() <= tolerance)
            .min_by(|a, b| a.1.1.abs().total_cmp(&b.1.1.abs()));
        if let Some((ruler, (k, distance))) = best {
            result.offset += ruler.direction() * distance;
            result.marks.push(ruler.mark(k));
        }
    }
    result
}

/// Engancha un plano (de normal unitaria `normal` que pasa por `point`) a las reglas:
/// busca la regla y la marca más cercanas al punto donde el plano cruza la regla, y
/// devuelve cuánto desplazar el plano (a lo largo de su normal) para que pase por esa
/// marca, y la marca. Las reglas casi paralelas al plano no cuentan.
pub fn plane_snap(rulers: &[Ruler], normal: Vec3, point: Vec3, tolerance: f32) -> Option<(Vec3, Vec3)> {
    let offset = normal.dot(point);
    rulers
        .iter()
        .filter_map(|r| {
            let across = normal.dot(r.direction());
            if across.abs() < 0.1 {
                return None;
            }
            // Cruce del plano con la recta de la regla, medido desde su origen.
            let s = (offset - normal.dot(r.origin)) / across;
            let k = (s / r.spacing.max(MIN_SPACING)).round();
            if k < 0.0 || k > (r.mark_count() - 1) as f32 {
                return None;
            }
            let distance = k * r.spacing - s;
            (distance.abs() <= tolerance).then(|| (distance, normal * (distance * across), r.mark(k as u32)))
        })
        .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()))
        .map(|(_, shift, mark)| (shift, mark))
}

/// Número sin decimales sobrantes: "10", "2.5", "0.25".
pub fn format_length(v: f32) -> String {
    let text = format!("{v:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ── Dibujo ─────────────────────────────────────────────────────────────────────

/// Proyección del mundo a puntos de egui; `None` si el punto está detrás de la cámara.
pub type Project<'a> = &'a dyn Fn(Vec3) -> Option<Pos2>;

fn perpendicular(a: Pos2, b: Pos2) -> egui::Vec2 {
    let d = b - a;
    let len = d.length();
    if len < 1e-3 { egui::vec2(0.0, 1.0) } else { egui::vec2(-d.y, d.x) / len }
}

/// Texto con fondo oscuro, centrado en `pos`.
fn label(painter: &egui::Painter, pos: Pos2, text: String, color: Color32) {
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), color);
    let rect = egui::Rect::from_center_size(pos, galley.size()).expand2(egui::vec2(4.0, 2.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(190));
    painter.galley(rect.min + egui::vec2(4.0, 2.0), galley, color);
}

/// Cota de `a` a `b` sobre `axis`: la medida desde `a` y una línea de extensión hasta `b`.
pub fn draw_measurement(painter: &egui::Painter, project: Project, a: Vec3, b: Vec3, axis: Axis) {
    let along = (b - a).dot(axis.unit());
    let end = a + axis.unit() * along;
    let (Some(pa), Some(pe)) = (project(a), project(end)) else { return };
    let color = axis.color();
    if let Some(pb) = project(b) {
        if (pb - pe).length() > 1.0 {
            painter.extend(egui::Shape::dashed_line(&[pe, pb], Stroke::new(1.0_f32, Color32::from_gray(170)), 4.0, 3.0));
        }
    }
    painter.line_segment([pa, pe], Stroke::new(2.0_f32, color));
    let n = perpendicular(pa, pe) * 6.0;
    for p in [pa, pe] {
        painter.line_segment([p - n, p + n], Stroke::new(2.0_f32, color));
    }
    label(painter, pa + (pe - pa) * 0.5 + n * 2.5, format!("{:.2} {}", along.abs(), axis.name()), color);
}

/// Regla con sus marcas; las más juntas que unos píxeles se omiten, y los números se
/// muestran solo donde caben. `highlight` resalta marcas enganchadas.
pub fn draw_ruler(painter: &egui::Painter, project: Project, ruler: &Ruler, highlight: &[Vec3]) {
    let (Some(start), Some(end)) = (project(ruler.origin), project(ruler.end())) else { return };
    let color = ruler.axis.color();
    painter.line_segment([start, end], Stroke::new(2.0_f32, color));
    let count = ruler.mark_count();
    // Píxeles entre marcas consecutivas, para decidir cuáles dibujar.
    let step_px = project(ruler.mark(1)).map_or(0.0, |p| (p - start).length());
    let every = |min_px: f32| [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000].into_iter().find(|&n| n as f32 * step_px >= min_px).unwrap_or(u32::MAX);
    let (tick_every, label_every) = (every(4.0), every(36.0));
    let n = perpendicular(start, end);
    for k in (0..count).filter(|k| k % tick_every == 0) {
        let Some(p) = project(ruler.mark(k)) else { continue };
        let size = if k % (tick_every * 5) == 0 { 10.0 } else { 5.0 };
        painter.line_segment([p, p + n * size], Stroke::new(1.5_f32, color));
        if k % label_every == 0 {
            painter.text(p + n * 18.0, egui::Align2::CENTER_CENTER, format_length(k as f32 * ruler.spacing), egui::FontId::proportional(11.0), color);
        }
    }
    for &mark in highlight {
        if let Some(p) = project(mark) {
            painter.circle_stroke(p, 6.0, Stroke::new(2.0_f32, Color32::from_rgb(255, 220, 60)));
        }
    }
}

/// Marcador del punto con snap bajo el cursor; la forma indica a qué se enganchó.
pub fn draw_snap(painter: &egui::Painter, p: Pos2, kind: SnapKind) {
    let color = Color32::from_rgb(255, 220, 60);
    let stroke = Stroke::new(2.0_f32, color);
    match kind {
        SnapKind::Center => {
            painter.circle_stroke(p, 7.0, stroke);
            painter.line_segment([p - egui::vec2(4.0, 0.0), p + egui::vec2(4.0, 0.0)], stroke);
            painter.line_segment([p - egui::vec2(0.0, 4.0), p + egui::vec2(0.0, 4.0)], stroke);
        }
        SnapKind::Corner => {
            painter.rect_stroke(egui::Rect::from_center_size(p, egui::vec2(11.0, 11.0)), 0.0, stroke, egui::StrokeKind::Middle);
        }
        SnapKind::Edge => {
            let d = [egui::vec2(0.0, -7.0), egui::vec2(7.0, 0.0), egui::vec2(0.0, 7.0), egui::vec2(-7.0, 0.0)];
            painter.add(egui::Shape::closed_line(d.iter().map(|&v| p + v).collect(), stroke));
        }
        SnapKind::Surface | SnapKind::Ground => {
            painter.circle_filled(p, 3.5, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominant_axis_and_lock() {
        assert_eq!(Axis::dominant(vec3(-5.0, 3.0, 1.0)), Axis::X);
        assert_eq!(Axis::dominant(vec3(0.0, 0.0, -2.0)), Axis::Z);
        assert_eq!(axis_between(vec3(0.0, 0.0, 0.0), vec3(5.0, 1.0, 0.0), Some(Axis::Y)), Axis::Y);
    }

    #[test]
    fn measurement_clicks_cycle() {
        let mut m = Measurement::default();
        m.click(vec3(1.0, 0.0, 0.0));
        m.click(vec3(2.0, 0.0, 0.0));
        assert_eq!((m.a, m.b), (Some(vec3(1.0, 0.0, 0.0)), Some(vec3(2.0, 0.0, 0.0))));
        m.click(vec3(3.0, 0.0, 0.0));
        assert_eq!((m.a, m.b), (Some(vec3(3.0, 0.0, 0.0)), None));
    }

    #[test]
    fn ruler_towards_negative_side_keeps_zero_at_start() {
        let r = Ruler::between(vec3(10.0, 0.0, 0.0), vec3(-15.0, 2.0, 0.0), Axis::X, 5.0).unwrap();
        assert_eq!((r.sign, r.length), (-1.0, 25.0));
        assert_eq!(r.mark_count(), 6);
        assert!((r.mark(2) - vec3(0.0, 0.0, 0.0)).magnitude() < 1e-5);
        assert!(Ruler::between(vec3(0.0, 0.0, 0.0), vec3(0.0, 5.0, 0.0), Axis::X, 5.0).is_none());
    }

    #[test]
    fn objects_snap_to_nearby_marks_only() {
        let rulers = [
            Ruler { origin: vec3(0.0, 0.0, 0.0), axis: Axis::X, sign: 1.0, length: 50.0, spacing: 10.0 },
            Ruler { origin: vec3(0.0, 0.0, 0.0), axis: Axis::Y, sign: 1.0, length: 50.0, spacing: 10.0 },
        ];
        // Punto a 0.4 de la marca 20 en X y a 0.3 de la marca 10 en Y.
        let points = [vec3(19.6, 10.3, 0.0)];
        let snap = ruler_snap(&rulers, &points, 0.5, &[Axis::X, Axis::Y]);
        assert!((snap.offset - vec3(0.4, -0.3, 0.0)).magnitude() < 1e-4, "{:?}", snap.offset);
        assert_eq!(snap.marks.len(), 2);
        // Fuera de la tolerancia no engancha; y solo en los ejes pedidos.
        assert_eq!(ruler_snap(&rulers, &points, 0.2, &[Axis::X, Axis::Y]).marks.len(), 0);
        let only_x = ruler_snap(&rulers, &points, 0.5, &[Axis::X]);
        assert!((only_x.offset - vec3(0.4, 0.0, 0.0)).magnitude() < 1e-4);
        // Más allá del final de la regla no hay marcas.
        assert!(ruler_snap(&rulers, &[vec3(70.0, 0.0, 0.0)], 0.5, &[Axis::X]).marks.is_empty());
    }

    #[test]
    fn cut_plane_snaps_where_it_crosses_a_ruler() {
        let ruler = [Ruler { origin: vec3(0.0, 0.0, 0.0), axis: Axis::Z, sign: 1.0, length: 30.0, spacing: 5.0 }];
        // Plano horizontal en z = 9.7: engancha a la marca 10.
        let (shift, mark) = plane_snap(&ruler, vec3(0.0, 0.0, 1.0), vec3(3.0, 4.0, 9.7), 0.5).unwrap();
        assert!((shift - vec3(0.0, 0.0, 0.3)).magnitude() < 1e-4, "{shift:?}");
        assert!((mark - vec3(0.0, 0.0, 10.0)).magnitude() < 1e-4);
        // Plano inclinado 45°: cruza la regla en z = 9.7 y también engancha a la marca 10.
        let n = vec3(1.0, 0.0, 1.0).normalize();
        let (shift, _) = plane_snap(&ruler, n, vec3(0.0, 0.0, 9.7), 0.5).unwrap();
        let moved = vec3(0.0, 0.0, 9.7) + shift;
        assert!((n.dot(moved) - n.dot(vec3(0.0, 0.0, 10.0))).abs() < 1e-4);
        // Lejos de una marca, o paralelo a la regla, no engancha.
        assert!(plane_snap(&ruler, vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, 7.5), 0.5).is_none());
        assert!(plane_snap(&ruler, vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, 10.0), 0.5).is_none());
    }

    #[test]
    fn lengths_are_formatted_without_trailing_zeros() {
        assert_eq!(format_length(10.0), "10");
        assert_eq!(format_length(2.5), "2.5");
        assert_eq!(format_length(0.25), "0.25");
    }
}
