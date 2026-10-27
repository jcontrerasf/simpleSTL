//! Sección de una malla por un plano: el contorno donde el plano la cruza, como
//! polilíneas 3D, su proyección a 2D en los ejes del plano y su escritura en SVG o DXF.
//!
//! No usa Manifold: es la intersección directa triángulo–plano, así que es barata (sirve
//! para la vista previa) y funciona también con mallas abiertas, donde los contornos
//! pueden quedar abiertos.

use std::collections::HashMap;
use std::fmt::Write;

use three_d::{InnerSpace, Vec3, vec3};

use crate::mesh::MeshData;

/// Contorno de la sección en el mundo.
#[derive(Clone, Debug)]
pub struct Polyline {
    pub points: Vec<Vec3>,
    pub closed: bool,
}

/// Contorno proyectado en el plano, en mm.
#[derive(Clone, Debug)]
pub struct Loop2 {
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
}

/// Arista de la malla (índices ordenados): identifica el punto donde el plano la cruza,
/// que así es el mismo para los dos triángulos que la comparten.
type EdgeKey = (u32, u32);

fn edge_key(a: u32, b: u32) -> EdgeKey {
    (a.min(b), a.max(b))
}

/// Corta la malla con el plano `normal · p = offset` y devuelve los contornos.
pub fn slice(mesh: &MeshData, normal: Vec3, offset: f32) -> Vec<Polyline> {
    let dist: Vec<f32> = mesh.vertices.iter().map(|&[x, y, z]| normal.dot(vec3(x, y, z)) - offset).collect();
    // Un vértice justo sobre el plano cuenta como del lado positivo: así cada vértice tiene
    // un solo lado y no hay casos degenerados.
    let above = |i: u32| dist[i as usize] >= 0.0;
    let mut points: HashMap<EdgeKey, Vec3> = HashMap::new();
    let mut segments: Vec<(EdgeKey, EdgeKey)> = Vec::new();
    for t in &mesh.triangles {
        let crossings: Vec<EdgeKey> = (0..3)
            .map(|k| (t[k], t[(k + 1) % 3]))
            .filter(|&(a, b)| above(a) != above(b))
            .map(|(a, b)| {
                let key = edge_key(a, b);
                points.entry(key).or_insert_with(|| {
                    let (pa, pb) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
                    let (da, db) = (dist[a as usize], dist[b as usize]);
                    let s = da / (da - db);
                    vec3(pa[0] + (pb[0] - pa[0]) * s, pa[1] + (pb[1] - pa[1]) * s, pa[2] + (pb[2] - pa[2]) * s)
                });
                key
            })
            .collect();
        if let [a, b] = crossings[..] {
            segments.push((a, b));
        }
    }
    chain(&segments)
        .into_iter()
        .map(|(keys, closed)| Polyline { points: simplify(keys.iter().map(|k| points[k]).collect(), closed), closed })
        .collect()
}

/// Une los segmentos que comparten extremo en cadenas. Sin depender de la orientación de
/// los triángulos, para que también sirva con mallas mal orientadas.
fn chain(segments: &[(EdgeKey, EdgeKey)]) -> Vec<(Vec<EdgeKey>, bool)> {
    let mut at: HashMap<EdgeKey, Vec<usize>> = HashMap::new();
    for (i, &(a, b)) in segments.iter().enumerate() {
        at.entry(a).or_default().push(i);
        at.entry(b).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    // Sigue la cadena desde `end` mientras haya un segmento sin usar; devuelve si volvió a `stop`.
    let extend = |keys: &mut Vec<EdgeKey>, used: &mut Vec<bool>, stop: EdgeKey| -> bool {
        loop {
            let end = *keys.last().unwrap();
            let Some(&next) = at[&end].iter().find(|&&s| !used[s]) else { return false };
            used[next] = true;
            let (a, b) = segments[next];
            let other = if a == end { b } else { a };
            if other == stop {
                return true;
            }
            keys.push(other);
        }
    };
    let mut chains = Vec::new();
    for i in 0..segments.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let (a, b) = segments[i];
        let mut keys = vec![a, b];
        let closed = extend(&mut keys, &mut used, a);
        if !closed {
            // Contorno abierto: también se extiende hacia atrás desde el primer punto.
            let mut back = vec![a];
            extend(&mut back, &mut used, b);
            back.reverse();
            back.pop();
            back.extend(keys);
            keys = back;
        }
        chains.push((keys, closed));
    }
    chains
}

/// Quita los puntos repetidos (aparecen cuando el plano pasa por un vértice) y los que
/// están en línea recta con sus vecinos (donde el plano cruza la diagonal de una cara plana).
fn simplify(points: Vec<Vec3>, closed: bool) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_none_or(|q: &Vec3| (p - q).magnitude2() > 1e-12) {
            out.push(p);
        }
    }
    if closed && out.len() > 1 && (out[0] - out[out.len() - 1]).magnitude2() <= 1e-12 {
        out.pop();
    }
    let straight = |a: Vec3, b: Vec3, c: Vec3| {
        let (u, v) = (b - a, c - b);
        u.dot(v) > 0.0 && u.cross(v).magnitude2() <= 1e-10 * u.magnitude2() * v.magnitude2()
    };
    let mut k = if closed { 0 } else { 1 };
    while out.len() > 2 && k < out.len() {
        let n = out.len();
        if !closed && k == n - 1 {
            break;
        }
        let (a, c) = (out[(k + n - 1) % n], out[(k + 1) % n]);
        if straight(a, out[k], c) {
            out.remove(k);
            // El anterior puede haber quedado en línea recta con el nuevo vecino.
            k = k.saturating_sub(1).max(if closed { 0 } else { 1 });
        } else {
            k += 1;
        }
    }
    out
}

/// Proyecta los contornos en la base `u`, `v` del plano. Descarta los de un solo punto.
pub fn project(polylines: &[Polyline], u: Vec3, v: Vec3) -> Vec<Loop2> {
    polylines
        .iter()
        .filter(|p| p.points.len() >= 2)
        .map(|p| Loop2 { points: p.points.iter().map(|&q| [u.dot(q), v.dot(q)]).collect(), closed: p.closed })
        .collect()
}

/// Caja envolvente 2D de varias secciones: (mínimo, máximo), o `None` si están vacías.
pub fn bounds<'a>(loops: impl IntoIterator<Item = &'a Loop2>) -> Option<([f32; 2], [f32; 2])> {
    let mut points = loops.into_iter().flat_map(|l| &l.points).peekable();
    points.peek()?;
    let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for p in points {
        for k in 0..2 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    Some((min, max))
}

/// Desplazamientos de las secciones paralelas: los de `current + k·step` que caen
/// estrictamente dentro de (`lo`, `hi`), de menor a mayor.
pub fn offsets(lo: f32, hi: f32, current: f32, step: f32) -> Vec<f32> {
    if step <= 0.0 {
        return vec![current];
    }
    let eps = step * 1e-4;
    let first = ((lo - current) / step).ceil() as i64;
    let last = ((hi - current) / step).floor() as i64;
    (first..=last).map(|k| current + k as f32 * step).filter(|&d| d > lo + eps && d < hi - eps).collect()
}

/// SVG en mm. El origen (0, 0) del dibujo es la esquina `min` de la caja de las secciones,
/// abajo a la izquierda; el eje Y del SVG crece hacia abajo, así que se invierte.
pub fn to_svg(loops: &[Loop2], min: [f32; 2], max: [f32; 2]) -> String {
    let (w, h) = (max[0] - min[0], max[1] - min[1]);
    let mut out = String::new();
    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#).unwrap();
    for l in loops {
        let mut d = String::new();
        for (i, p) in l.points.iter().enumerate() {
            write!(d, "{}{} {} ", if i == 0 { "M" } else { "L" }, p[0] - min[0], max[1] - p[1]).unwrap();
        }
        if l.closed {
            d.push('Z');
        }
        writeln!(out, r#"  <path d="{}" fill="none" stroke="black" stroke-width="0.1"/>"#, d.trim_end()).unwrap();
    }
    out.push_str("</svg>\n");
    out
}

/// DXF ASCII (R12, el más compatible), en mm, con una POLYLINE por contorno. El origen es
/// la esquina `min` de la caja de las secciones.
pub fn to_dxf(loops: &[Loop2], min: [f32; 2]) -> String {
    let mut out = String::new();
    let mut pair = |code: u32, value: &str| writeln!(out, "{code}\n{value}").unwrap();
    for (code, value) in [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1009"), (9, "$INSUNITS"), (70, "4"), (0, "ENDSEC")] {
        pair(code, value);
    }
    pair(0, "SECTION");
    pair(2, "ENTITIES");
    for l in loops {
        for (code, value) in [(0, "POLYLINE"), (8, "0"), (66, "1"), (70, if l.closed { "1" } else { "0" })] {
            pair(code, value);
        }
        for (code, value) in [(10, "0"), (20, "0"), (30, "0")] {
            pair(code, value);
        }
        for p in &l.points {
            pair(0, "VERTEX");
            pair(8, "0");
            pair(10, &(p[0] - min[0]).to_string());
            pair(20, &(p[1] - min[1]).to_string());
            pair(30, "0");
        }
        pair(0, "SEQEND");
        pair(8, "0");
    }
    pair(0, "ENDSEC");
    pair(0, "EOF");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csg;

    fn perimeter(p: &Polyline) -> f32 {
        let n = p.points.len();
        let segments = if p.closed { n } else { n - 1 };
        (0..segments).map(|i| (p.points[(i + 1) % n] - p.points[i]).magnitude()).sum()
    }

    #[test]
    fn cube_section_is_one_closed_square() {
        let cube = csg::cube(10.0, 20.0, 30.0).unwrap();
        let loops = slice(&cube, vec3(0.0, 0.0, 1.0), 3.0);
        assert_eq!(loops.len(), 1);
        assert!(loops[0].closed);
        assert!((perimeter(&loops[0]) - 60.0).abs() < 1e-3);
        assert!(loops[0].points.iter().all(|p| (p.z - 3.0).abs() < 1e-5));
    }

    #[test]
    fn section_through_vertices_stays_closed() {
        let cube = csg::cube(10.0, 10.0, 10.0).unwrap();
        // Plano diagonal vertical: pasa justo por las aristas verticales de dos esquinas.
        let loops = slice(&cube, vec3(1.0, 1.0, 0.0).normalize(), 0.0);
        assert_eq!(loops.len(), 1);
        assert!(loops[0].closed);
        assert!((perimeter(&loops[0]) - 2.0 * (200.0f32.sqrt() + 10.0)).abs() < 1e-3);
    }

    #[test]
    fn tube_section_has_two_loops() {
        let tube = csg::tube(10.0, 5.0, 20.0, 32).unwrap();
        let loops = slice(&tube, vec3(0.0, 0.0, 1.0), 1.0);
        assert_eq!(loops.len(), 2);
        assert!(loops.iter().all(|l| l.closed));
    }

    #[test]
    fn plane_outside_gives_nothing() {
        let cube = csg::cube(10.0, 10.0, 10.0).unwrap();
        assert!(slice(&cube, vec3(0.0, 0.0, 1.0), 6.0).is_empty());
    }

    #[test]
    fn open_mesh_gives_open_polyline() {
        // Cubo sin la tapa superior (la mitad de sus triángulos con z máximo).
        let mut cube = csg::cube(10.0, 10.0, 10.0).unwrap();
        let top: Vec<usize> = (0..cube.triangles.len())
            .filter(|&i| cube.triangles[i].iter().all(|&v| cube.vertices[v as usize][2] > 4.9))
            .collect();
        assert_eq!(top.len(), 2);
        cube.triangles.remove(top[1]);
        cube.triangles.remove(top[0]);
        // Quitando además una cara lateral, el contorno a media altura queda abierto.
        let side: Vec<usize> = (0..cube.triangles.len())
            .filter(|&i| cube.triangles[i].iter().all(|&v| cube.vertices[v as usize][0] > 4.9))
            .collect();
        for &i in side.iter().rev() {
            cube.triangles.remove(i);
        }
        let loops = slice(&cube, vec3(0.0, 0.0, 1.0), 0.0);
        assert_eq!(loops.len(), 1);
        assert!(!loops[0].closed);
        assert!((perimeter(&loops[0]) - 30.0).abs() < 1e-3);
    }

    #[test]
    fn offsets_are_aligned_with_the_current_plane() {
        assert_eq!(offsets(0.0, 10.0, 4.0, 3.0), vec![1.0, 4.0, 7.0]);
        // Los extremos exactos quedan fuera: la sección sería degenerada.
        assert_eq!(offsets(0.0, 10.0, 5.0, 5.0), vec![5.0]);
        assert_eq!(offsets(0.0, 10.0, 4.0, 0.0), vec![4.0]);
    }

    #[test]
    fn writers_put_the_box_corner_at_the_origin() {
        let cube = csg::cube(10.0, 20.0, 30.0).unwrap();
        let loops = project(&slice(&cube, vec3(0.0, 0.0, 1.0), 0.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0));
        let (min, max) = bounds(&loops).unwrap();
        assert_eq!((min, max), ([-5.0, -10.0], [5.0, 10.0]));

        let svg = to_svg(&loops, min, max);
        assert!(svg.contains(r#"width="10mm" height="20mm""#));
        assert_eq!(svg.matches("<path").count(), 1);
        assert!(svg.contains('Z'));

        let dxf = to_dxf(&loops, min);
        assert_eq!(dxf.matches("\nPOLYLINE\n").count(), 1);
        assert_eq!(dxf.matches("\nVERTEX\n").count(), 4);
        assert!(dxf.trim_end().ends_with("EOF"));
        // Ninguna coordenada negativa: la esquina quedó en (0, 0).
        assert!(!dxf.lines().any(|l| l.starts_with('-')));
    }
}
