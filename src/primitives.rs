//! Primitivas paramétricas (cubo, esfera, cilindro). El objeto guarda sus parámetros para
//! poder cambiarle el tamaño regenerando la malla.

use crate::csg;
use crate::i18n::tr;
use crate::mesh::MeshData;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Box { size: [f32; 3] },
    Sphere { radius: f32, segments: u32 },
    Cylinder { radius: f32, height: f32, segments: u32 },
    /// Radio superior 0: cono con punta; mayor que 0: cono truncado.
    Cone { radius_bottom: f32, radius_top: f32, height: f32, segments: u32 },
}

pub const MIN_SIZE: f32 = 0.01;
pub const SEGMENTS: std::ops::RangeInclusive<u32> = 3..=256;

impl Primitive {
    pub const DEFAULTS: [Primitive; 4] = [
        Primitive::Box { size: [20.0, 20.0, 20.0] },
        Primitive::Sphere { radius: 10.0, segments: 48 },
        Primitive::Cylinder { radius: 10.0, height: 20.0, segments: 48 },
        Primitive::Cone { radius_bottom: 10.0, radius_top: 0.0, height: 20.0, segments: 48 },
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Primitive::Box { .. } => tr("Cubo", "Box"),
            Primitive::Sphere { .. } => tr("Esfera", "Sphere"),
            Primitive::Cylinder { .. } => tr("Cilindro", "Cylinder"),
            Primitive::Cone { .. } => tr("Cono", "Cone"),
        }
    }

    /// Malla centrada en el origen (cerrada, apta para booleanas).
    pub fn mesh(&self) -> Option<MeshData> {
        match *self {
            Primitive::Box { size: [x, y, z] } => csg::cube(x, y, z),
            Primitive::Sphere { radius, segments } => csg::sphere(radius, segments),
            Primitive::Cylinder { radius, height, segments } => csg::cylinder(radius, radius, height, segments),
            Primitive::Cone { radius_bottom, radius_top, height, segments } => {
                csg::cylinder(radius_bottom, radius_top, height, segments)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_closed_with_expected_volume() {
        let volume = |p: Primitive| {
            let mesh = p.mesh().unwrap();
            assert!(mesh.topology().is_closed(), "{p:?}");
            mesh.volume().abs()
        };
        assert!((volume(Primitive::Box { size: [2.0, 3.0, 4.0] }) - 24.0).abs() < 1e-3);
        // Con muchos segmentos el poliedro se acerca al volumen exacto.
        let pi = std::f32::consts::PI;
        let sphere = volume(Primitive::Sphere { radius: 10.0, segments: 128 });
        assert!((sphere / (4.0 / 3.0 * pi * 1000.0) - 1.0).abs() < 0.01, "esfera {sphere}");
        let cylinder = volume(Primitive::Cylinder { radius: 10.0, height: 20.0, segments: 128 });
        assert!((cylinder / (pi * 100.0 * 20.0) - 1.0).abs() < 0.01, "cilindro {cylinder}");
        let cone = volume(Primitive::Cone { radius_bottom: 10.0, radius_top: 0.0, height: 20.0, segments: 128 });
        assert!((cone / (pi * 100.0 * 20.0 / 3.0) - 1.0).abs() < 0.01, "cono {cone}");
        // Tronco de cono: π·h/3·(R² + R·r + r²).
        let frustum = volume(Primitive::Cone { radius_bottom: 10.0, radius_top: 5.0, height: 20.0, segments: 128 });
        assert!((frustum / (pi * 20.0 / 3.0 * (100.0 + 50.0 + 25.0)) - 1.0).abs() < 0.01, "tronco {frustum}");
    }
}
