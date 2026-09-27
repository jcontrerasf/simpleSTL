//! "Apoyar en cara": caras planas sobre las que el objeto puede descansar, y la rotación
//! que deja una de ellas mirando hacia el suelo.
//!
//! Como en PrusaSlicer, las candidatas salen de la envolvente convexa: un objeto apoyado
//! toca el suelo con su envolvente, así que las caras cóncavas nunca sirven.

use three_d::{InnerSpace, Quat, Rad, Rotation, Rotation3, Vec3, vec3};

use crate::csg;
use crate::mesh::MeshData;

#[derive(Clone, Debug)]
pub struct Facet {
    /// Normal hacia afuera, en coordenadas del mundo.
    pub normal: Vec3,
    pub area: f32,
    pub triangles: Vec<[Vec3; 3]>,
}

/// Fracción mínima del área de la envolvente para mostrar una cara (descarta el facetado
/// de superficies curvas).
const MIN_AREA_FRACTION: f32 = 0.005;
const MAX_FACETS: usize = 64;

/// Resultado del análisis de caras planas de un objeto.
pub struct Facets {
    /// Caras sobre las que el objeto puede apoyarse (estables), de mayor a menor área.
    pub candidates: Vec<Facet>,
    /// Todas las caras planas significativas de la envolvente (estables o no), de mayor a
    /// menor área. Sirven de referencia para alinear el objeto con los ejes.
    pub planes: Vec<Facet>,
}

/// Caras planas de la malla (en el mundo).
pub fn find_facets(world: &MeshData) -> Facets {
    let Some(hull) = csg::convex_hull(&world.vertices) else {
        return Facets { candidates: Vec::new(), planes: Vec::new() };
    };
    let (min, max) = world.bounding_box();
    let size = (max - min).magnitude().max(1e-6);
    let cos_tolerance = 1f32.to_radians().cos();
    let offset_tolerance = size * 1e-4;

    // Agrupa los triángulos de la envolvente por plano (normal + distancia al origen).
    let mut groups: Vec<(f32, Facet)> = Vec::new();
    for &[a, b, c] in &hull.triangles {
        let [a, b, c] = [a, b, c].map(|i| Vec3::from(hull.vertices[i as usize]));
        let cross = (b - a).cross(c - a);
        let area = cross.magnitude() / 2.0;
        if area <= f32::EPSILON {
            continue;
        }
        let normal = cross.normalize();
        let offset = normal.dot(a);
        match groups
            .iter_mut()
            .find(|(d, f)| f.normal.dot(normal) > cos_tolerance && (d - offset).abs() < offset_tolerance)
        {
            Some((_, facet)) => {
                facet.area += area;
                facet.triangles.push([a, b, c]);
            }
            None => groups.push((offset, Facet { normal, area, triangles: vec![[a, b, c]] })),
        }
    }

    let total_area: f32 = groups.iter().map(|(_, f)| f.area).sum();
    let mut planes: Vec<Facet> =
        groups.into_iter().map(|(_, f)| f).filter(|f| f.area >= total_area * MIN_AREA_FRACTION).collect();
    planes.sort_by(|a, b| b.area.total_cmp(&a.area));

    let centroid = world.centroid();
    let mut candidates: Vec<Facet> = planes.iter().filter(|f| is_stable(f, centroid, size)).cloned().collect();
    candidates.truncate(MAX_FACETS);
    Facets { candidates, planes }
}

/// Estable si el centro de masa, proyectado sobre el plano de la cara, cae dentro de ella.
fn is_stable(facet: &Facet, centroid: Vec3, size: f32) -> bool {
    let n = facet.normal;
    let tolerance = size * 1e-5;
    facet.triangles.iter().any(|&[a, b, c]| {
        let p = centroid - n * n.dot(centroid - a);
        [(a, b), (b, c), (c, a)].iter().all(|&(u, v)| (v - u).cross(p - u).dot(n) >= -tolerance)
    })
}

/// Rotación que lleva `normal` a apuntar hacia -Z (la cara queda apoyada en el suelo).
pub fn rotation_to_floor(normal: Vec3) -> Quat {
    let n = normal.normalize();
    let down = vec3(0.0, 0.0, -1.0);
    if n.dot(down) > 1.0 - 1e-6 {
        Quat::new(1.0, 0.0, 0.0, 0.0)
    } else if n.dot(down) < -1.0 + 1e-6 {
        // Opuestas: cualquier eje perpendicular sirve.
        Quat::from_axis_angle(vec3(1.0, 0.0, 0.0), Rad(std::f32::consts::PI))
    } else {
        Quat::between_vectors(n, down)
    }
}

/// Rotación (en el mundo) para apoyar la cara de normal `down` en el suelo. Con `align`,
/// además gira alrededor de Z lo mínimo para que la mayor cara que quede vertical mire hacia
/// ±X o ±Y; sin eso, el objeto queda apoyado pero con un giro arbitrario sobre Z.
pub fn placement_rotation(down: Vec3, planes: &[Facet], align: bool) -> Quat {
    let to_floor = rotation_to_floor(down);
    if !align {
        return to_floor;
    }
    // `planes` viene ordenado por área: la primera cara casi vertical tras apoyar es la referencia.
    let reference = planes.iter().map(|f| to_floor * f.normal).find(|n| n.z.abs() < 0.02);
    let Some(n) = reference else {
        return to_floor; // sin caras verticales (p. ej. un cilindro): nada que alinear
    };
    let angle = n.y.atan2(n.x);
    let quarter = std::f32::consts::FRAC_PI_2;
    let snapped = (angle / quarter).round() * quarter;
    Quat::from_angle_z(Rad(snapped - angle)) * to_floor
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn load(name: &str) -> MeshData {
        MeshData::load_stl(Path::new(&format!("samples/{name}"))).unwrap()
    }

    #[test]
    fn cube_has_six_facets() {
        let facets = find_facets(&load("cubo.stl"));
        assert_eq!(facets.candidates.len(), 6);
        for f in &facets.candidates {
            assert!((f.area - 400.0).abs() < 1e-2, "área {}", f.area);
        }
    }

    #[test]
    fn sphere_has_no_flat_facets() {
        assert!(find_facets(&load("esfera.stl")).candidates.is_empty());
    }

    #[test]
    fn placing_a_tilted_cube_leaves_it_axis_aligned() {
        // Cubo girado en los tres ejes.
        let tilt = Quat::from_angle_z(Rad(0.4)) * Quat::from_angle_y(Rad(0.7)) * Quat::from_angle_x(Rad(-0.3));
        let cube = load("cubo.stl").transformed(|p| tilt * p);
        let facets = find_facets(&cube);
        for facet in &facets.candidates {
            let rotation = placement_rotation(facet.normal, &facets.planes, true);
            let (min, max) = cube.transformed(|p| rotation * p).bounding_box();
            let size = max - min;
            // Alineado con los ejes, la caja envolvente vuelve a ser exactamente 20 × 20 × 20.
            assert!((size - vec3(20.0, 20.0, 20.0)).magnitude() < 1e-2, "{size:?}");
        }
        // Sin alinear, solo la cara queda abajo y la caja sigue siendo más ancha en X/Y.
        let rotation = placement_rotation(facets.candidates[0].normal, &facets.planes, false);
        let (min, max) = cube.transformed(|p| rotation * p).bounding_box();
        assert!((max.z - min.z - 20.0).abs() < 1e-2);
        assert!(max.x - min.x > 20.5);
    }

    #[test]
    fn rotation_points_normal_down() {
        for n in [vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0), vec3(1.0, 0.0, 0.0), vec3(0.3, -0.5, 0.8)] {
            let rotated = rotation_to_floor(n) * n.normalize();
            assert!((rotated - vec3(0.0, 0.0, -1.0)).magnitude() < 1e-4, "{n:?} → {rotated:?}");
        }
    }
}
