//! Rasgos de una malla a los que se engancha el cursor al medir o poner reglas: aristas
//! vivas, esquinas y centros de agujeros y arcos.
//!
//! Los rasgos se calculan una vez por malla, en coordenadas locales; la pose del objeto
//! los lleva al mundo al buscar el snap.

use std::collections::HashMap;

use three_d::{Camera, InnerSpace, PhysicalPoint, Vec3};

use crate::manipulator::Pose;
use crate::mesh::MeshData;

/// Ángulo entre caras vecinas a partir del cual una arista es "viva". El facetado de un
/// cilindro de 48 segmentos (7.5°) queda bastante por debajo.
const SHARP_DEGREES: f32 = 30.0;
/// Giro entre dos aristas vivas a partir del cual su vértice común es una esquina.
const CORNER_DEGREES: f32 = 30.0;
/// Giro mínimo entre tramos para que cuenten como parte de un arco (no de una recta).
const MIN_ARC_TURN_DEGREES: f32 = 0.5;
/// Radios de snap en píxeles físicos, de mayor a menor prioridad.
const CENTER_RADIUS_PX: f32 = 12.0;
const CORNER_RADIUS_PX: f32 = 10.0;
const EDGE_RADIUS_PX: f32 = 8.0;

#[derive(Clone, Debug, Default)]
pub struct Features {
    pub corners: Vec<Vec3>,
    pub edges: Vec<[Vec3; 2]>,
    /// Centros de circunferencias y arcos formados por aristas vivas (bordes de agujeros,
    /// postes, extremos de una ranura). Cada tapa aporta el suyo.
    pub centers: Vec<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Center,
    Corner,
    Edge,
    Surface,
    Ground,
}

#[derive(Clone, Copy, Debug)]
pub struct Snap {
    pub position: Vec3,
    pub kind: SnapKind,
}

fn vertex(mesh: &MeshData, i: u32) -> Vec3 {
    Vec3::from(mesh.vertices[i as usize])
}

pub fn features(mesh: &MeshData) -> Features {
    let normals: Vec<Option<Vec3>> = mesh
        .triangles
        .iter()
        .map(|&[a, b, c]| {
            let n = (vertex(mesh, b) - vertex(mesh, a)).cross(vertex(mesh, c) - vertex(mesh, a));
            (n.magnitude2() > 0.0).then(|| n.normalize())
        })
        .collect();
    let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (f, t) in mesh.triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edge_faces.entry((a.min(b), a.max(b))).or_default().push(f);
        }
    }

    let cos_sharp = SHARP_DEGREES.to_radians().cos();
    let mut sharp: Vec<(u32, u32)> = edge_faces
        .iter()
        .filter(|(_, faces)| match faces.as_slice() {
            [f, g] => match (normals[*f], normals[*g]) {
                (Some(n), Some(m)) => n.dot(m) < cos_sharp,
                _ => false,
            },
            _ => true, // borde o arista no-manifold
        })
        .map(|(&e, _)| e)
        .collect();
    sharp.sort();

    let mut neighbors: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(a, b) in &sharp {
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
    }
    let cos_corner = CORNER_DEGREES.to_radians().cos();
    let is_corner = |v: u32| match neighbors[&v].as_slice() {
        [p, n] => {
            let (into, out) = (vertex(mesh, v) - vertex(mesh, *p), vertex(mesh, *n) - vertex(mesh, v));
            into.normalize().dot(out.normalize()) < cos_corner
        }
        _ => true,
    };
    let mut corner_ids: Vec<u32> = neighbors.keys().copied().filter(|&v| is_corner(v)).collect();
    corner_ids.sort();

    let (min, max) = mesh.bounding_box();
    let duplicate = (max - min).magnitude() * 1e-5;
    let mut centers: Vec<Vec3> = Vec::new();
    for chain in chains(&neighbors, &corner_ids) {
        let points: Vec<Vec3> = chain.iter().map(|&v| vertex(mesh, v)).collect();
        for c in arc_centers(&points) {
            // Un agujero en una tapa da el mismo centro que el borde exterior del poste.
            if !centers.iter().any(|&o| (o - c).magnitude() <= duplicate) {
                centers.push(c);
            }
        }
    }

    Features {
        corners: corner_ids.iter().map(|&v| vertex(mesh, v)).collect(),
        edges: sharp.iter().map(|&(a, b)| [vertex(mesh, a), vertex(mesh, b)]).collect(),
        centers,
    }
}

/// Recorre el grafo de aristas vivas en cadenas: de esquina a esquina, y los lazos
/// cerrados sin esquinas (un lazo se devuelve con su primer vértice repetido al final).
fn chains(neighbors: &HashMap<u32, Vec<u32>>, corners: &[u32]) -> Vec<Vec<u32>> {
    let key = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut used: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let corner_set: std::collections::HashSet<u32> = corners.iter().copied().collect();
    let walk = |start: u32, first: u32, used: &mut std::collections::HashSet<(u32, u32)>| {
        let mut chain = vec![start];
        let (mut prev, mut current) = (start, first);
        used.insert(key(prev, current));
        loop {
            chain.push(current);
            if current == start || corner_set.contains(&current) {
                break;
            }
            // Vértice de grado 2: seguir por la otra arista.
            let Some(&next) = neighbors[&current].iter().find(|&&n| n != prev && !used.contains(&key(current, n))) else {
                break;
            };
            used.insert(key(current, next));
            (prev, current) = (current, next);
        }
        chain
    };

    let mut result = Vec::new();
    for &c in corners {
        for &n in &neighbors[&c] {
            if !used.contains(&key(c, n)) {
                result.push(walk(c, n, &mut used));
            }
        }
    }
    let mut rest: Vec<u32> = neighbors.keys().copied().collect();
    rest.sort();
    for v in rest {
        if let Some(&n) = neighbors[&v].iter().find(|&&n| !used.contains(&key(v, n))) {
            result.push(walk(v, n, &mut used));
        }
    }
    result
}

/// Centros de los arcos de una cadena de puntos. Recorre la cadena y, desde cada punto,
/// extiende el arco mientras los puntos sigan sobre una misma circunferencia y la cadena
/// siga girando; los tramos rectos cortan los arcos.
fn arc_centers(points: &[Vec3]) -> Vec<Vec3> {
    let closed = points.len() > 3 && points.first() == points.last();
    let min_turn = MIN_ARC_TURN_DEGREES.to_radians().cos();
    let turns = |k: usize, pts: &[Vec3]| {
        let (into, out) = (pts[k] - pts[k - 1], pts[k + 1] - pts[k]);
        into.normalize().dot(out.normalize()) < min_turn
    };

    let rotated;
    let mut points = points;
    if closed {
        let ring = &points[..points.len() - 1];
        // Un lazo cerrado que es una circunferencia completa.
        let all_turn = (0..ring.len()).all(|k| {
            let pts = [ring[(k + ring.len() - 1) % ring.len()], ring[k], ring[(k + 1) % ring.len()]];
            turns(1, &pts)
        });
        if all_turn && ring.len() >= 6 {
            let n = ring.len();
            if let Some((center, _)) = circle_fit(ring, [0, n / 3, 2 * n / 3]) {
                return vec![center];
            }
        }
        // Si no, se empieza por el tramo más largo (una recta, típicamente), para no
        // partir un arco en dos.
        let length = |k: usize| (ring[(k + 1) % ring.len()] - ring[k]).magnitude();
        let start = (0..ring.len()).max_by(|&a, &b| length(a).total_cmp(&length(b))).unwrap_or(0);
        rotated = ring[start..].iter().chain(&ring[..=start]).copied().collect::<Vec<_>>();
        points = &rotated;
    }

    let mut centers = Vec::new();
    let mut i = 0;
    while i + 3 < points.len() {
        let mut best = None;
        let mut j = i + 3;
        while j < points.len() && (i + 1..j).all(|k| turns(k, points)) {
            match circle_fit(&points[i..=j], [0, (j - i) / 2, j - i]) {
                Some((center, _)) => best = Some((j, center)),
                None => break,
            }
            j += 1;
        }
        match best {
            Some((end, center)) => {
                centers.push(center);
                i = end;
            }
            None => i += 1,
        }
    }
    centers
}

/// Circunferencia por los tres puntos `pick` de `points`, si todos los demás también
/// están sobre ella y en su plano (con una tolerancia del 1 % del radio). Los tramos deben
/// tener largos parecidos: así una recta larga entre dos arcos no pasa por arco.
fn circle_fit(points: &[Vec3], pick: [usize; 3]) -> Option<(Vec3, f32)> {
    let lengths = points.windows(2).map(|w| (w[1] - w[0]).magnitude());
    let (shortest, longest) = lengths.fold((f32::MAX, 0.0f32), |(lo, hi), l| (lo.min(l), hi.max(l)));
    if longest > 2.0 * shortest {
        return None;
    }
    let [a, b, c] = pick.map(|k| points[k]);
    let (ab, ac) = (b - a, c - a);
    let normal = ab.cross(ac);
    let n2 = normal.magnitude2();
    if n2 <= f32::EPSILON * ab.magnitude2() * ac.magnitude2() {
        return None; // alineados
    }
    // Circuncentro: a + (|ac|²·(n × ab) + |ab|²·(ac × n)) / (2|n|²).
    let center = a + (normal.cross(ab) * ac.magnitude2() + ac.cross(normal) * ab.magnitude2()) / (2.0 * n2);
    let radius = (a - center).magnitude();
    let normal = normal / n2.sqrt();
    let tolerance = radius * 0.01;
    points
        .iter()
        .all(|&p| ((p - center).magnitude() - radius).abs() <= tolerance && (p - center).dot(normal).abs() <= tolerance)
        .then_some((center, radius))
}

/// Punto con snap bajo el cursor. `objects` son los rasgos y la pose de cada objeto
/// visible; `surface` es el punto de la malla bajo el cursor (de `pick`), si lo hay.
pub fn find_snap(camera: &Camera, cursor: PhysicalPoint, objects: &[(&Features, Pose)], surface: Option<Vec3>) -> Option<Snap> {
    let origin = camera.position_at_pixel(cursor);
    let ray = camera.view_direction_at_pixel(cursor);
    let cursor = three_d::vec2(cursor.x, cursor.y);
    let view = camera.view_direction();
    let depth = |p: Vec3| (p - camera.position()).dot(view);
    // Lo que queda detrás de la superficie bajo el cursor está tapado.
    let max_depth = surface.map(|s| depth(s) * 1.01 + 1e-3);
    let visible = |p: Vec3| depth(p) > camera.z_near() && max_depth.is_none_or(|m| depth(p) <= m);
    let pixel_distance = |p: Vec3| {
        let px = camera.pixel_at_position(p);
        (three_d::vec2(px.x, px.y) - cursor).magnitude()
    };

    let nearest = |candidates: &mut dyn Iterator<Item = Vec3>, radius: f32| {
        candidates
            .filter(|&p| visible(p))
            .map(|p| (pixel_distance(p), p))
            .filter(|&(d, _)| d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
    };
    let world = |pose: Pose, points: &[Vec3]| points.iter().map(move |&p| pose.apply(p)).collect::<Vec<_>>();

    let mut centers = objects.iter().flat_map(|(f, pose)| world(*pose, &f.centers));
    if let Some(p) = nearest(&mut centers, CENTER_RADIUS_PX) {
        return Some(Snap { position: p, kind: SnapKind::Center });
    }
    let mut corners = objects.iter().flat_map(|(f, pose)| world(*pose, &f.corners));
    if let Some(p) = nearest(&mut corners, CORNER_RADIUS_PX) {
        return Some(Snap { position: p, kind: SnapKind::Corner });
    }
    let mut on_edges = objects.iter().flat_map(|(f, pose)| {
        f.edges.iter().map(move |&[a, b]| closest_on_segment(origin, ray, pose.apply(a), pose.apply(b)))
    });
    if let Some(p) = nearest(&mut on_edges, EDGE_RADIUS_PX) {
        return Some(Snap { position: p, kind: SnapKind::Edge });
    }
    if let Some(p) = surface {
        return Some(Snap { position: p, kind: SnapKind::Surface });
    }
    // Suelo z = 0.
    if ray.z.abs() > 1e-6 {
        let t = -origin.z / ray.z;
        if t > 0.0 {
            return Some(Snap { position: origin + ray * t, kind: SnapKind::Ground });
        }
    }
    None
}

/// Punto del segmento `a`–`b` más cercano a la recta `origin + t·dir`.
fn closest_on_segment(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let u = b - a;
    let w = a - origin;
    let (uu, ud, dd) = (u.dot(u), u.dot(dir), dir.dot(dir));
    let (uw, dw) = (u.dot(w), dir.dot(w));
    let denom = uu * dd - ud * ud;
    let s = if denom.abs() < 1e-12 { 0.0 } else { (ud * dw - dd * uw) / denom };
    a + u * s.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::Primitive;
    use three_d::vec3;

    fn contains(points: &[Vec3], p: Vec3) -> bool {
        points.iter().any(|&q| (q - p).magnitude() < 1e-2)
    }

    #[test]
    fn cube_has_corners_and_edges_but_no_centers() {
        let f = features(&Primitive::Box { size: [20.0, 20.0, 20.0] }.mesh().unwrap());
        assert_eq!(f.corners.len(), 8);
        assert_eq!(f.edges.len(), 12);
        assert!(f.centers.is_empty(), "{:?}", f.centers);
    }

    #[test]
    fn sphere_has_no_corners() {
        let f = features(&Primitive::Sphere { radius: 10.0, segments: 48 }.mesh().unwrap());
        assert!(f.corners.is_empty());
    }

    #[test]
    fn tube_has_centers_on_both_caps() {
        let f = features(&Primitive::Tube { radius_outer: 5.0, radius_inner: 1.5, height: 10.0, segments: 48 }.mesh().unwrap());
        assert!(f.corners.is_empty());
        // Borde exterior y del agujero en cada tapa: todos centrados en el eje.
        assert!(contains(&f.centers, vec3(0.0, 0.0, 5.0)), "{:?}", f.centers);
        assert!(contains(&f.centers, vec3(0.0, 0.0, -5.0)), "{:?}", f.centers);
        assert_eq!(f.centers.len(), 2, "{:?}", f.centers);
    }

    #[test]
    fn slot_has_a_center_at_each_end() {
        let f = features(&Primitive::RoundedBox { size: [30.0, 10.0, 5.0], radius: 5.0, segments: 48 }.mesh().unwrap());
        for p in [vec3(10.0, 0.0, 2.5), vec3(-10.0, 0.0, 2.5), vec3(10.0, 0.0, -2.5), vec3(-10.0, 0.0, -2.5)] {
            assert!(contains(&f.centers, p), "falta {p:?} en {:?}", f.centers);
        }
        assert_eq!(f.centers.len(), 4, "{:?}", f.centers);
    }

    #[test]
    fn rounded_box_has_a_center_per_corner() {
        let f = features(&Primitive::RoundedBox { size: [30.0, 20.0, 5.0], radius: 3.0, segments: 48 }.mesh().unwrap());
        assert_eq!(f.centers.len(), 8, "{:?}", f.centers);
        assert!(contains(&f.centers, vec3(12.0, 7.0, 2.5)));
    }

    #[test]
    fn closest_point_on_segment_to_ray() {
        let p = closest_on_segment(vec3(5.0, -10.0, 1.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 0.0), vec3(10.0, 0.0, 0.0));
        assert!((p - vec3(5.0, 0.0, 0.0)).magnitude() < 1e-5);
        // Fuera del segmento: se queda en el extremo.
        let p = closest_on_segment(vec3(20.0, -10.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 0.0), vec3(10.0, 0.0, 0.0));
        assert!((p - vec3(10.0, 0.0, 0.0)).magnitude() < 1e-5);
    }
}
