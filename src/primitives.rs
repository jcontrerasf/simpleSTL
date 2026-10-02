//! Primitivas paramétricas (cubo, esfera, cilindro, cono, tubo, caja redondeada). El objeto guarda sus parámetros para
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
    /// Cilindro con un agujero pasante, p. ej. un poste para tornillo.
    Tube { radius_outer: f32, radius_inner: f32, height: f32, segments: u32 },
    /// Caja con las aristas verticales redondeadas; con el radio máximo, una ranura extruida.
    RoundedBox { size: [f32; 3], radius: f32, segments: u32 },
}

pub const MIN_SIZE: f32 = 0.01;
pub const SEGMENTS: std::ops::RangeInclusive<u32> = 3..=256;

impl Primitive {
    pub const DEFAULTS: [Primitive; 6] = [
        Primitive::Box { size: [20.0, 20.0, 20.0] },
        Primitive::Sphere { radius: 10.0, segments: 48 },
        Primitive::Cylinder { radius: 10.0, height: 20.0, segments: 48 },
        Primitive::Cone { radius_bottom: 10.0, radius_top: 0.0, height: 20.0, segments: 48 },
        Primitive::Tube { radius_outer: 5.0, radius_inner: 1.5, height: 10.0, segments: 48 },
        Primitive::RoundedBox { size: [30.0, 10.0, 5.0], radius: 5.0, segments: 48 },
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Primitive::Box { .. } => tr("Cubo", "Box"),
            Primitive::Sphere { .. } => tr("Esfera", "Sphere"),
            Primitive::Cylinder { .. } => tr("Cilindro", "Cylinder"),
            Primitive::Cone { .. } => tr("Cono", "Cone"),
            Primitive::Tube { .. } => tr("Tubo", "Tube"),
            Primitive::RoundedBox { .. } => tr("Caja redondeada", "Rounded box"),
        }
    }

    /// Ajusta los parámetros que dependen de otros: el agujero de un tubo más angosto que
    /// el tubo, y el radio de las esquinas no mayor que la mitad del lado menor.
    pub fn normalized(mut self) -> Self {
        match &mut self {
            Primitive::Tube { radius_outer, radius_inner, .. } => {
                *radius_inner = radius_inner.min(*radius_outer - MIN_SIZE).max(MIN_SIZE / 2.0);
                *radius_outer = radius_outer.max(*radius_inner + MIN_SIZE);
            }
            Primitive::RoundedBox { size: [x, y, _], radius, .. } => *radius = radius.clamp(0.0, x.min(*y) / 2.0),
            _ => {}
        }
        self
    }

    /// La misma primitiva con la escala `s` (en sus ejes) absorbida en sus parámetros, si
    /// la forma escalada sigue siendo de ese tipo: un cilindro estirado solo en X sería
    /// ovalado y ya no es un cilindro.
    pub fn scaled(&self, s: [f32; 3]) -> Option<Primitive> {
        let [sx, sy, sz] = s;
        let same = |a: f32, b: f32| (a - b).abs() <= 1e-4 * a.abs().max(b.abs());
        let round_base = same(sx, sy);
        let scaled = match *self {
            Primitive::Box { size: [x, y, z] } => Primitive::Box { size: [x * sx, y * sy, z * sz] },
            Primitive::RoundedBox { size: [x, y, z], radius, segments } if round_base => {
                Primitive::RoundedBox { size: [x * sx, y * sy, z * sz], radius: radius * sx, segments }
            }
            Primitive::Sphere { radius, segments } if round_base && same(sx, sz) => {
                Primitive::Sphere { radius: radius * sx, segments }
            }
            Primitive::Cylinder { radius, height, segments } if round_base => {
                Primitive::Cylinder { radius: radius * sx, height: height * sz, segments }
            }
            Primitive::Cone { radius_bottom, radius_top, height, segments } if round_base => {
                Primitive::Cone { radius_bottom: radius_bottom * sx, radius_top: radius_top * sx, height: height * sz, segments }
            }
            Primitive::Tube { radius_outer, radius_inner, height, segments } if round_base => {
                Primitive::Tube { radius_outer: radius_outer * sx, radius_inner: radius_inner * sx, height: height * sz, segments }
            }
            _ => return None,
        };
        Some(scaled.normalized())
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
            Primitive::Tube { radius_outer, radius_inner, height, segments } => {
                csg::tube(radius_outer, radius_inner, height, segments)
            }
            Primitive::RoundedBox { size: [x, y, z], radius, segments } => csg::rounded_box(x, y, z, radius, segments),
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
        // Tubo: π·h·(R² − r²).
        let tube = volume(Primitive::Tube { radius_outer: 5.0, radius_inner: 2.0, height: 10.0, segments: 128 });
        assert!((tube / (pi * 10.0 * (25.0 - 4.0)) - 1.0).abs() < 0.01, "tubo {tube}");
        // Caja redondeada: caja menos las esquinas cuadradas que no llena el cuarto de círculo.
        let rounded = |radius| volume(Primitive::RoundedBox { size: [30.0, 10.0, 5.0], radius, segments: 128 });
        let exact = |r: f32| (300.0 - (4.0 - pi) * r * r) * 5.0;
        assert!((rounded(2.0) / exact(2.0) - 1.0).abs() < 0.01, "caja redondeada {}", rounded(2.0));
        assert!((rounded(5.0) / exact(5.0) - 1.0).abs() < 0.01, "ranura {}", rounded(5.0));
        assert!((rounded(0.0) - 1500.0).abs() < 1e-2);
    }

    #[test]
    fn scale_is_absorbed_only_when_the_shape_keeps_its_type() {
        let volume = |p: Primitive| p.mesh().unwrap().volume().abs();
        let cube = Primitive::Box { size: [20.0, 20.0, 20.0] };
        let stretched = cube.scaled([1.5, 1.0, 0.5]).unwrap();
        assert_eq!(stretched, Primitive::Box { size: [30.0, 20.0, 10.0] });
        assert!((volume(stretched) - volume(cube) * 0.75).abs() < 1e-2);

        let cylinder = Primitive::Cylinder { radius: 10.0, height: 20.0, segments: 48 };
        assert_eq!(cylinder.scaled([2.0, 2.0, 0.5]), Some(Primitive::Cylinder { radius: 20.0, height: 10.0, segments: 48 }));
        assert_eq!(cylinder.scaled([2.0, 1.0, 1.0]), None);

        let sphere = Primitive::Sphere { radius: 10.0, segments: 48 };
        assert_eq!(sphere.scaled([2.0, 2.0, 2.0]), Some(Primitive::Sphere { radius: 20.0, segments: 48 }));
        assert_eq!(sphere.scaled([2.0, 2.0, 1.0]), None);

        let slot = Primitive::RoundedBox { size: [30.0, 10.0, 5.0], radius: 5.0, segments: 48 };
        assert_eq!(slot.scaled([2.0, 2.0, 1.0]), Some(Primitive::RoundedBox { size: [60.0, 20.0, 5.0], radius: 10.0, segments: 48 }));
        assert_eq!(slot.scaled([2.0, 1.0, 1.0]), None);
    }

    #[test]
    fn dependent_parameters_are_clamped() {
        let tube = Primitive::Tube { radius_outer: 3.0, radius_inner: 4.0, height: 10.0, segments: 32 }.normalized();
        let Primitive::Tube { radius_outer, radius_inner, .. } = tube else { unreachable!() };
        assert!(radius_inner < radius_outer);
        assert!(tube.mesh().is_some_and(|m| m.topology().is_closed()));

        let slot = Primitive::RoundedBox { size: [30.0, 10.0, 5.0], radius: 8.0, segments: 32 }.normalized();
        assert!(matches!(slot, Primitive::RoundedBox { radius: 5.0, .. }));
        assert!(slot.mesh().is_some_and(|m| m.topology().is_closed()));
    }
}
