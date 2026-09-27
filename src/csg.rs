//! Booleanas y cortes por plano, delegados a Manifold.
//!
//! Manifold exige mallas cerradas y orientables; `MeshData::topology` permite
//! avisarle al usuario antes de intentar la operación.

use manifold3d::{Manifold, OpType};

use crate::mesh::MeshData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Difference,
    Intersection,
}

impl BooleanOp {
    pub const ALL: [BooleanOp; 3] = [BooleanOp::Union, BooleanOp::Difference, BooleanOp::Intersection];

    pub fn label(self) -> &'static str {
        match self {
            BooleanOp::Union => "Unión",
            BooleanOp::Difference => "Resta",
            BooleanOp::Intersection => "Intersección",
        }
    }

    /// Operador para nombrar el resultado. La fuente de egui no trae ∪ ni ∩.
    pub fn symbol(self) -> &'static str {
        match self {
            BooleanOp::Union => "+",
            BooleanOp::Difference => "−",
            BooleanOp::Intersection => "&",
        }
    }
}

fn to_manifold(mesh: &MeshData) -> Result<Manifold, String> {
    let vertices: Vec<f64> = mesh.vertices.iter().flatten().map(|&c| c as f64).collect();
    let triangles: Vec<u64> = mesh.triangles.iter().flatten().map(|&i| i as u64).collect();
    Manifold::from_mesh_f64(&vertices, 3, &triangles)
        .map_err(|e| format!("Manifold rechazó la malla (¿no es cerrada?): {e}"))
}

fn from_manifold(manifold: &Manifold) -> Option<MeshData> {
    if manifold.is_empty() {
        return None;
    }
    let (props, n_props, triangles) = manifold.to_mesh_f64();
    let vertices = props
        .chunks_exact(n_props)
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    let triangles = triangles
        .chunks_exact(3)
        .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
        .collect();
    Some(MeshData { vertices, triangles })
}

/// Devuelve `Ok(None)` si el resultado es vacío (p. ej. intersección de objetos disjuntos).
pub fn boolean(a: &MeshData, b: &MeshData, op: BooleanOp) -> Result<Option<MeshData>, String> {
    let op = match op {
        BooleanOp::Union => OpType::Add,
        BooleanOp::Difference => OpType::Subtract,
        BooleanOp::Intersection => OpType::Intersect,
    };
    let result = to_manifold(a)?.boolean(&to_manifold(b)?, op);
    result.status().map_err(|e| format!("falló la operación: {e}"))?;
    Ok(from_manifold(&result))
}

/// Corta por el plano `normal · p = offset` y cierra ambas caras del corte.
/// Devuelve (lado positivo, lado negativo); un lado es `None` si queda vacío.
pub fn split(mesh: &MeshData, normal: [f64; 3], offset: f64) -> Result<(Option<MeshData>, Option<MeshData>), String> {
    let (positive, negative) = to_manifold(mesh)?.split_by_plane(normal, offset);
    Ok((from_manifold(&positive), from_manifold(&negative)))
}

/// Envolvente convexa de un conjunto de puntos.
pub fn convex_hull(points: &[[f32; 3]]) -> Option<MeshData> {
    let points: Vec<[f64; 3]> = points.iter().map(|p| p.map(|c| c as f64)).collect();
    from_manifold(&Manifold::hull_pts(&points))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn cube() -> MeshData {
        MeshData::load_stl(Path::new("samples/cubo.stl")).unwrap()
    }

    fn sphere() -> MeshData {
        MeshData::load_stl(Path::new("samples/esfera.stl")).unwrap()
    }

    #[test]
    fn boolean_volumes_are_consistent() {
        let (a, b) = (cube(), sphere());
        let volume = |op| boolean(&a, &b, op).unwrap().unwrap().volume().abs();
        let (union, difference, intersection) =
            (volume(BooleanOp::Union), volume(BooleanOp::Difference), volume(BooleanOp::Intersection));

        // A = (A − B) + (A ∩ B)  y  A ∪ B = A + B − (A ∩ B)
        let (va, vb) = (a.volume().abs(), b.volume().abs());
        assert!((difference + intersection - va).abs() < 1.0);
        assert!((union - (va + vb - intersection)).abs() < 1.0);
        assert!(intersection > 0.0 && intersection < vb);
    }

    #[test]
    fn split_cube_in_half() {
        let (top, bottom) = split(&cube(), [0.0, 0.0, 1.0], 5.0).unwrap();
        let (top, bottom) = (top.unwrap(), bottom.unwrap());
        assert!(top.topology().is_closed() && bottom.topology().is_closed());
        assert!((top.volume().abs() - 20.0 * 20.0 * 15.0).abs() < 1e-2);
        assert!((bottom.volume().abs() - 20.0 * 20.0 * 5.0).abs() < 1e-2);
    }

    #[test]
    fn hull_of_cube_is_the_cube() {
        let hull = convex_hull(&cube().vertices).unwrap();
        assert!(hull.topology().is_closed());
        assert!((hull.volume().abs() - 8000.0).abs() < 1e-2);
    }

    #[test]
    fn tilted_cut_preserves_volume() {
        let normal: [f64; 3] = [1.0, 1.0, 2.0];
        let len = normal.iter().map(|c| c * c).sum::<f64>().sqrt();
        let normal = normal.map(|c| c / len);
        // Plano por el centro del cubo (10, 10, 10).
        let offset = 10.0 * (normal[0] + normal[1] + normal[2]);
        let (a, b) = split(&cube(), normal, offset).unwrap();
        let total = a.unwrap().volume().abs() + b.unwrap().volume().abs();
        assert!((total - 8000.0).abs() < 1e-1);
    }

    #[test]
    fn plane_outside_leaves_one_side_empty() {
        let (top, bottom) = split(&cube(), [0.0, 0.0, 1.0], 100.0).unwrap();
        assert!(top.is_none());
        assert!(bottom.is_some());
    }
}
