//! Reparación de mallas defectuosas, típicas de STL exportados sin cuidado.
//!
//! Pasos, en orden: soldar vértices casi iguales, quitar triángulos degenerados y
//! duplicados, orientar las caras de forma consistente, rellenar agujeros y orientar
//! cada pieza hacia afuera. No resuelve aristas compartidas por más de dos caras
//! (geometría no-manifold real); eso queda indicado en la topología final.

use std::collections::{HashMap, VecDeque};

use three_d::{InnerSpace, Vec3};

use crate::mesh::{MeshData, Topology};

#[derive(Clone, Copy, Debug)]
pub struct RepairReport {
    pub welded_vertices: usize,
    pub removed_triangles: usize,
    pub flipped_triangles: usize,
    pub holes_filled: usize,
    /// Topología después de reparar.
    pub topology: Topology,
}

impl RepairReport {
    pub fn changed_anything(&self) -> bool {
        self.welded_vertices + self.removed_triangles + self.flipped_triangles + self.holes_filled > 0
    }
}

pub fn repair(mesh: &MeshData) -> (MeshData, RepairReport) {
    let (mut vertices, remap) = weld(&mesh.vertices);
    let welded_vertices = mesh.vertices.len() - vertices.len();
    let triangles: Vec<[u32; 3]> = mesh.triangles.iter().map(|t| t.map(|i| remap[i as usize])).collect();

    let before = triangles.len();
    let mut triangles = remove_degenerate_and_duplicates(triangles);
    let removed_triangles = before - triangles.len();

    let mut flipped_triangles = orient_consistently(&mut triangles);
    let holes_filled = fill_holes(&mut vertices, &mut triangles);
    flipped_triangles += orient_outward(&vertices, &mut triangles);

    let repaired = MeshData { vertices, triangles };
    let topology = repaired.topology();
    (repaired, RepairReport { welded_vertices, removed_triangles, flipped_triangles, holes_filled, topology })
}

fn to_vec(p: [f32; 3]) -> Vec3 {
    Vec3::from(p)
}

/// Une vértices a menos de una tolerancia relativa al tamaño. Devuelve los vértices
/// nuevos y, para cada vértice original, su índice nuevo.
fn weld(vertices: &[[f32; 3]]) -> (Vec<[f32; 3]>, Vec<u32>) {
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in vertices {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    let diagonal = (to_vec(max) - to_vec(min)).magnitude();
    let tolerance = (diagonal * 1e-6).max(f32::MIN_POSITIVE);

    // Grilla con celdas del tamaño de la tolerancia: basta mirar las 27 celdas vecinas.
    let cell = |v: [f32; 3]| v.map(|c| (c / tolerance).floor() as i64);
    let mut grid: HashMap<[i64; 3], Vec<u32>> = HashMap::new();
    let mut welded: Vec<[f32; 3]> = Vec::new();
    let mut remap = Vec::with_capacity(vertices.len());
    for &v in vertices {
        let [cx, cy, cz] = cell(v);
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for &candidate in grid.get(&[cx + dx, cy + dy, cz + dz]).into_iter().flatten() {
                        if (to_vec(welded[candidate as usize]) - to_vec(v)).magnitude() <= tolerance {
                            found = Some(candidate);
                            break 'search;
                        }
                    }
                }
            }
        }
        let index = found.unwrap_or_else(|| {
            welded.push(v);
            let index = (welded.len() - 1) as u32;
            grid.entry([cx, cy, cz]).or_default().push(index);
            index
        });
        remap.push(index);
    }
    (welded, remap)
}

/// Rota el triángulo para que empiece por su índice menor (misma orientación).
fn canonical(t: [u32; 3]) -> [u32; 3] {
    let k = (0..3).min_by_key(|&k| t[k]).unwrap();
    [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
}

/// Quita triángulos con vértices repetidos y duplicados. Un par con los mismos vértices
/// y orientaciones opuestas es una "aleta" interna: se quitan ambos.
fn remove_degenerate_and_duplicates(triangles: Vec<[u32; 3]>) -> Vec<[u32; 3]> {
    let mut groups: HashMap<[u32; 3], Vec<[u32; 3]>> = HashMap::new();
    let mut order = Vec::new();
    for t in triangles {
        let [a, b, c] = t;
        if a == b || b == c || a == c {
            continue;
        }
        let mut key = t;
        key.sort();
        let entry = groups.entry(key).or_default();
        if entry.is_empty() {
            order.push(key);
        }
        entry.push(canonical(t));
    }
    let mut result = Vec::new();
    for key in order {
        let group = &groups[&key];
        let first = group[0];
        let same = group.iter().filter(|&&t| t == first).count();
        let opposite = group.len() - same;
        // Cada orientación opuesta cancela a una de la otra; sobrevive una del resto.
        if same > opposite {
            result.push(first);
        } else if opposite > same {
            result.push(*group.iter().find(|&&t| t != first).unwrap());
        }
    }
    result
}

fn has_directed_edge(t: [u32; 3], a: u32, b: u32) -> bool {
    (0..3).any(|k| t[k] == a && t[(k + 1) % 3] == b)
}

fn flip(t: &mut [u32; 3]) {
    t.swap(1, 2);
}

/// Hace que caras vecinas recorran su arista común en sentidos opuestos, recorriendo
/// cada pieza conexa en anchura. Devuelve cuántas caras invirtió.
fn orient_consistently(triangles: &mut [[u32; 3]]) -> usize {
    let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (f, t) in triangles.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edge_faces.entry((a.min(b), a.max(b))).or_default().push(f);
        }
    }
    let mut visited = vec![false; triangles.len()];
    let mut flipped = 0;
    for seed in 0..triangles.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        let mut queue = VecDeque::from([seed]);
        while let Some(f) = queue.pop_front() {
            let t = triangles[f];
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                let faces = &edge_faces[&(a.min(b), a.max(b))];
                if faces.len() != 2 {
                    continue; // borde o arista no-manifold: no propaga
                }
                let g = if faces[0] == f { faces[1] } else { faces[0] };
                if visited[g] {
                    continue;
                }
                visited[g] = true;
                if has_directed_edge(triangles[g], a, b) {
                    flip(&mut triangles[g]);
                    flipped += 1;
                }
                queue.push_back(g);
            }
        }
    }
    flipped
}

/// Cierra cada ring de aristas de borde: un triángulo si tiene 3 vértices, o un abanico
/// desde un vértice nuevo en su centroide. Devuelve cuántos agujeros cerró.
fn fill_holes(vertices: &mut Vec<[f32; 3]>, triangles: &mut Vec<[u32; 3]>) -> usize {
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for t in triangles.iter() {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    // Arista de borde: aparece una vez y su inversa ninguna.
    let mut next: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&(a, b), &count) in &directed {
        if count == 1 && !directed.contains_key(&(b, a)) {
            next.entry(a).or_default().push(b);
        }
    }

    let mut holes = 0;
    let mut starts: Vec<u32> = next.keys().copied().collect();
    starts.sort();
    for start in starts {
        while let Some(first) = next.get_mut(&start).and_then(Vec::pop) {
            let mut ring = vec![start];
            let mut current = first;
            let mut closed = false;
            for _ in 0..directed.len() {
                if current == start {
                    closed = true;
                    break;
                }
                ring.push(current);
                match next.get_mut(&current).and_then(Vec::pop) {
                    Some(n) => current = n,
                    None => break,
                }
            }
            if !closed || ring.len() < 3 {
                continue; // cadena abierta (vértice no-manifold): no se puede cerrar con seguridad
            }
            // Las caras nuevas recorren cada arista del borde al revés.
            if ring.len() == 3 {
                triangles.push([ring[0], ring[2], ring[1]]);
            } else {
                let centroid = ring.iter().map(|&i| to_vec(vertices[i as usize])).sum::<Vec3>() / ring.len() as f32;
                vertices.push([centroid.x, centroid.y, centroid.z]);
                let c = (vertices.len() - 1) as u32;
                for k in 0..ring.len() {
                    let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                    triangles.push([b, a, c]);
                }
            }
            holes += 1;
        }
    }
    holes
}

/// Invierte las piezas conexas cuyo volumen con signo es negativo (normales hacia adentro).
fn orient_outward(vertices: &[[f32; 3]], triangles: &mut [[u32; 3]]) -> usize {
    // Piezas conexas por vértices compartidos (unión-búsqueda).
    let mut parent: Vec<u32> = (0..vertices.len() as u32).collect();
    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    for t in triangles.iter() {
        for k in 1..3 {
            let (ra, rb) = (find(&mut parent, t[0]), find(&mut parent, t[k]));
            parent[ra as usize] = rb;
        }
    }
    let mut volume: HashMap<u32, f32> = HashMap::new();
    for t in triangles.iter() {
        let [a, b, c] = t.map(|i| to_vec(vertices[i as usize]));
        *volume.entry(find(&mut parent, t[0])).or_default() += a.dot(b.cross(c));
    }
    let mut flipped = 0;
    for t in triangles.iter_mut() {
        if volume[&find(&mut parent, t[0])] < 0.0 {
            flip(t);
            flipped += 1;
        }
    }
    flipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn cube() -> MeshData {
        MeshData::load_stl(Path::new("samples/cubo.stl")).unwrap()
    }

    fn assert_repaired_cube(mesh: &MeshData) {
        let (fixed, report) = repair(mesh);
        assert!(report.topology.is_closed(), "{report:?}");
        assert!((fixed.volume() - 8000.0).abs() < 1e-1, "volumen {}", fixed.volume());
    }

    #[test]
    fn healthy_cube_is_left_alone() {
        let (fixed, report) = repair(&cube());
        assert!(!report.changed_anything(), "{report:?}");
        assert_eq!(fixed.triangles.len(), 12);
    }

    #[test]
    fn missing_face_is_filled() {
        let mut mesh = cube();
        mesh.triangles.drain(0..2); // una cara completa (2 triángulos)
        assert!(!mesh.topology().is_closed());
        let (_, report) = repair(&mesh);
        assert_eq!(report.holes_filled, 1);
        assert_repaired_cube(&mesh);
    }

    #[test]
    fn flipped_triangle_is_fixed() {
        let mut mesh = cube();
        mesh.triangles[5].swap(1, 2);
        assert!(!mesh.topology().is_closed());
        let (_, report) = repair(&mesh);
        assert_eq!(report.flipped_triangles, 1);
        assert_repaired_cube(&mesh);
    }

    #[test]
    fn inside_out_cube_is_turned_outward() {
        let mut mesh = cube();
        for t in &mut mesh.triangles {
            t.swap(1, 2);
        }
        assert!(mesh.volume() < 0.0);
        assert_repaired_cube(&mesh);
    }

    #[test]
    fn near_duplicate_vertices_are_welded() {
        // Cada triángulo con sus propios vértices, desplazados una fracción ínfima.
        let cube = cube();
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for (f, t) in cube.triangles.iter().enumerate() {
            let base = vertices.len() as u32;
            for (k, &i) in t.iter().enumerate() {
                let jitter = 1e-5 * ((f * 3 + k) % 7) as f32 / 7.0;
                let [x, y, z] = cube.vertices[i as usize];
                vertices.push([x + jitter, y - jitter, z + jitter]);
            }
            triangles.push([base, base + 1, base + 2]);
        }
        let mesh = MeshData { vertices, triangles };
        assert!(!mesh.topology().is_closed());
        let (fixed, report) = repair(&mesh);
        assert_eq!(fixed.vertices.len(), 8);
        assert_eq!(report.welded_vertices, 36 - 8);
        assert_repaired_cube(&mesh);
    }

    #[test]
    fn broken_sample_file_is_repaired() {
        // samples/cubo_roto.stl: le falta una cara y tiene un triángulo invertido.
        let mesh = MeshData::load_stl(Path::new("samples/cubo_roto.stl")).unwrap();
        assert!(!mesh.topology().is_closed());
        assert_repaired_cube(&mesh);
    }

    #[test]
    fn duplicated_and_fin_triangles_are_removed() {
        let mut mesh = cube();
        let t = mesh.triangles[0];
        mesh.triangles.push(t); // duplicado
        mesh.triangles.push([t[0], t[2], t[1]]); // + opuesto: se cancelan con el duplicado
        let (fixed, _) = repair(&mesh);
        assert_eq!(fixed.triangles.len(), 12);
        assert_repaired_cube(&mesh);
    }
}
