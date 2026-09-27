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

/// Caras planas estables de la malla (en el mundo), de mayor a menor área.
pub fn find_facets(world: &MeshData) -> Vec<Facet> {
    let Some(hull) = csg::convex_hull(&world.vertices) else {
        return Vec::new();
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
    let centroid = world.centroid();
    let mut facets: Vec<Facet> = groups
        .into_iter()
        .map(|(_, f)| f)
        .filter(|f| f.area >= total_area * MIN_AREA_FRACTION && is_stable(f, centroid, size))
        .collect();
    facets.sort_by(|a, b| b.area.total_cmp(&a.area));
    facets.truncate(MAX_FACETS);
    facets
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
        assert_eq!(facets.len(), 6);
        for f in &facets {
            assert!((f.area - 400.0).abs() < 1e-2, "área {}", f.area);
        }
    }

    #[test]
    fn sphere_has_no_flat_facets() {
        assert!(find_facets(&load("esfera.stl")).is_empty());
    }

    #[test]
    fn rotation_points_normal_down() {
        for n in [vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0), vec3(1.0, 0.0, 0.0), vec3(0.3, -0.5, 0.8)] {
            let rotated = rotation_to_floor(n) * n.normalize();
            assert!((rotated - vec3(0.0, 0.0, -1.0)).magnitude() < 1e-4, "{n:?} → {rotated:?}");
        }
    }
}
