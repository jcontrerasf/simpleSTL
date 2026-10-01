//! Malla triangular indexada, independiente del renderer.
//!
//! Es la representación sobre la que se harán cortes y booleanas; el render
//! solo consume una copia (ver `to_cpu_mesh`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use three_d::{CpuMesh, Indices, InnerSpace, Positions, Vec3, vec3};

use crate::i18n::tr;

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
    /// Aristas cuyos triángulos no se pueden emparejar de a dos con orientaciones opuestas
    /// (caras invertidas o más caras en un sentido que en el otro). Una arista con cuatro
    /// caras bien emparejadas (dos sólidos que se tocan por una arista) no cuenta: Manifold
    /// la acepta, y al cargar un STL `stl_io` une los vértices de esas aristas.
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
        let file = File::open(path).map_err(|e| format!("{}: {e}", tr("no se pudo abrir", "could not open")))?;
        let stl = stl_io::read_stl(&mut BufReader::new(file))
            .map_err(|e| format!("{}: {e}", tr("STL inválido", "invalid STL")))?;

        let vertices = stl.vertices.iter().map(|v| v.0).collect();
        let triangles = stl
            .faces
            .iter()
            .map(|f| f.vertices.map(|i| i as u32))
            // Descarta triángulos degenerados que referencian el mismo vértice.
            .filter(|[a, b, c]| a != b && b != c && a != c)
            .collect::<Vec<_>>();

        if triangles.is_empty() {
            return Err(tr("el archivo no contiene triángulos", "the file contains no triangles").into());
        }
        Ok(Self { vertices, triangles })
    }

    pub fn save_stl(&self, path: &Path) -> Result<(), String> {
        let triangles: Vec<stl_io::Triangle> = self
            .triangles
            .iter()
            .map(|&[a, b, c]| {
                let n = (self.vertex(b) - self.vertex(a)).cross(self.vertex(c) - self.vertex(a));
                let n = if n.magnitude2() > 0.0 { n.normalize() } else { n };
                stl_io::Triangle {
                    normal: stl_io::Normal::new([n.x, n.y, n.z]),
                    vertices: [a, b, c].map(|i| stl_io::Vertex::new(self.vertices[i as usize])),
                }
            })
            .collect();
        let file = File::create(path).map_err(|e| format!("{}: {e}", tr("no se pudo crear", "could not create")))?;
        stl_io::write_stl(&mut BufWriter::new(file), triangles.iter()).map_err(|e| format!("{}: {e}", tr("error al escribir", "write error")))
    }

    /// Copia con cada vértice transformado por `f`. `f` debe preservar la orientación
    /// (traslaciones y rotaciones), o las normales quedarían invertidas.
    pub fn transformed(&self, f: impl Fn(Vec3) -> Vec3) -> MeshData {
        let vertices = self
            .vertices
            .iter()
            .map(|&[x, y, z]| {
                let p = f(vec3(x, y, z));
                [p.x, p.y, p.z]
            })
            .collect();
        MeshData { vertices, triangles: self.triangles.clone() }
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
            if count == 1 && reverse == 0 {
                boundary_edges += 1;
            } else if count != reverse {
                bad_edges += 1;
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

    /// Centro de masa de un sólido homogéneo: suma de los centroides de los tetraedros
    /// (origen, a, b, c) ponderados por su volumen con signo. Requiere malla cerrada.
    pub fn centroid(&self) -> Vec3 {
        let mut weighted = vec3(0.0, 0.0, 0.0);
        let mut total = 0.0;
        for &[a, b, c] in &self.triangles {
            let (pa, pb, pc) = (self.vertex(a), self.vertex(b), self.vertex(c));
            let volume = pa.dot(pb.cross(pc));
            weighted += (pa + pb + pc) * (volume / 4.0);
            total += volume;
        }
        if total.abs() > f32::EPSILON { weighted / total } else { self.bounding_box().0 }
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
        assert!((mesh.centroid() - vec3(10.0, 10.0, 10.0)).magnitude() < 1e-3);
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
