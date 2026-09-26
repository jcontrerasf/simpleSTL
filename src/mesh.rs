//! Malla triangular indexada, independiente del renderer.
//!
//! Es la representación sobre la que se harán cortes y booleanas; el render
//! solo consume una copia (ver `to_cpu_mesh`).

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use three_d::{CpuMesh, Indices, InnerSpace, Positions, Vec3, vec3};

#[derive(Clone, Debug)]
pub struct MeshData {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Resumen topológico: si la malla es cerrada y orientable es apta para booleanas.
#[derive(Clone, Copy, Debug)]
pub struct Topology {
    /// Aristas usadas por un solo triángulo (agujeros en la superficie).
    pub boundary_edges: usize,
    /// Aristas compartidas por más de dos triángulos o con orientación inconsistente.
    pub bad_edges: usize,
}

impl Topology {
    pub fn is_closed(&self) -> bool {
        self.boundary_edges == 0 && self.bad_edges == 0
    }
}

impl MeshData {
    /// Carga un STL (ASCII o binario). `stl_io` ya une los vértices idénticos.
    pub fn load_stl(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("no se pudo abrir: {e}"))?;
        let stl = stl_io::read_stl(&mut BufReader::new(file))
            .map_err(|e| format!("STL inválido: {e}"))?;

        let vertices = stl.vertices.iter().map(|v| v.0).collect();
        let triangles = stl
            .faces
            .iter()
            .map(|f| f.vertices.map(|i| i as u32))
            // Descarta triángulos degenerados que referencian el mismo vértice.
            .filter(|[a, b, c]| a != b && b != c && a != c)
            .collect::<Vec<_>>();

        if triangles.is_empty() {
            return Err("el archivo no contiene triángulos".into());
        }
        Ok(Self { vertices, triangles })
    }

    fn vertex(&self, i: u32) -> Vec3 {
        let [x, y, z] = self.vertices[i as usize];
        vec3(x, y, z)
    }

    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        let mut min = Vec3::from([f32::INFINITY; 3]);
        let mut max = Vec3::from([f32::NEG_INFINITY; 3]);
        for &[x, y, z] in &self.vertices {
            min = vec3(min.x.min(x), min.y.min(y), min.z.min(z));
            max = vec3(max.x.max(x), max.y.max(y), max.z.max(z));
        }
        (min, max)
    }

    /// Cuenta, para cada arista dirigida, cuántas veces aparece. En una malla
    /// cerrada y bien orientada cada arista (a,b) aparece una vez y su inversa (b,a) también.
    pub fn topology(&self) -> Topology {
        let mut directed: HashMap<(u32, u32), u32> = HashMap::new();
        for &[a, b, c] in &self.triangles {
            for e in [(a, b), (b, c), (c, a)] {
                *directed.entry(e).or_default() += 1;
            }
        }

        let mut boundary_edges = 0;
        let mut bad_edges = 0;
        for (&(a, b), &count) in &directed {
            let reverse = directed.get(&(b, a)).copied().unwrap_or(0);
            if count > 1 || reverse > 1 {
                bad_edges += 1;
            } else if reverse == 0 {
                boundary_edges += 1;
            }
        }
        Topology { boundary_edges, bad_edges }
    }

    /// Volumen con signo (teorema de la divergencia). Solo tiene sentido si la malla es cerrada.
    pub fn volume(&self) -> f32 {
        self.triangles
            .iter()
            .map(|&[a, b, c]| self.vertex(a).dot(self.vertex(b).cross(self.vertex(c))))
            .sum::<f32>()
            / 6.0
    }

    /// Malla para el renderer con sombreado plano: cada triángulo tiene sus propios
    /// vértices para que la normal sea la de la cara (lo típico en piezas CAD).
    pub fn to_cpu_mesh(&self) -> CpuMesh {
        let mut positions = Vec::with_capacity(self.triangles.len() * 3);
        let mut normals = Vec::with_capacity(self.triangles.len() * 3);
        for &[a, b, c] in &self.triangles {
            let (pa, pb, pc) = (self.vertex(a), self.vertex(b), self.vertex(c));
            let n = (pb - pa).cross(pc - pa);
            let n = if n.magnitude2() > 0.0 { n.normalize() } else { n };
            positions.extend([pa, pb, pc]);
            normals.extend([n, n, n]);
        }
        CpuMesh {
            positions: Positions::F32(positions),
            normals: Some(normals),
            indices: Indices::None,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_is_closed_with_expected_volume() {
        let mesh = MeshData::load_stl(Path::new("samples/cubo.stl")).unwrap();
        assert_eq!(mesh.vertices.len(), 8);
        assert_eq!(mesh.triangles.len(), 12);
        assert!(mesh.topology().is_closed());
        assert!((mesh.volume().abs() - 8000.0).abs() < 1e-2);
    }

    #[test]
    fn missing_face_is_detected() {
        let mut mesh = MeshData::load_stl(Path::new("samples/cubo.stl")).unwrap();
        mesh.triangles.pop();
        let topo = mesh.topology();
        assert!(!topo.is_closed());
        assert_eq!(topo.boundary_edges, 3);
    }
}
