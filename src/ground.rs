//! Plano base traslúcido en z = 0, con una grilla alineada al origen.

use three_d::*;

pub struct Ground {
    plane: Gm<Mesh, ColorMaterial>,
    grid: Gm<Mesh, ColorMaterial>,
    /// Separación entre líneas de la grilla, para mostrarla en la interfaz.
    pub spacing: f32,
}

impl Ground {
    /// Base centrada (en XY) bajo `center` que cubre al menos `extent` por lado.
    pub fn new(context: &Context, center: Vec3, extent: f32) -> Self {
        let spacing = nice_step(extent / 10.0);
        // Centro y borde en múltiplos del espaciado: las líneas pasan por el origen.
        let (cx, cy) = ((center.x / spacing).round() * spacing, (center.y / spacing).round() * spacing);
        let lines = (extent / spacing / 2.0).ceil().max(1.0) as i32;
        let half = lines as f32 * spacing;
        // Leve desplazamiento hacia abajo para no pelear en profundidad con caras apoyadas en z = 0.
        let z = -half * 1e-4;

        let mut plane = Gm::new(
            Mesh::new(context, &CpuMesh::square()),
            ColorMaterial::new_transparent(context, &CpuMaterial { albedo: Srgba::new(170, 180, 200, 45), ..Default::default() }),
        );
        plane.set_transformation(Mat4::from_translation(vec3(cx, cy, z)) * Mat4::from_nonuniform_scale(half, half, 1.0));

        // Cada línea es un rectángulo delgado en el plano.
        let width = half * 0.002;
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        let mut quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            let base = positions.len() as u32;
            positions.extend([a, b, c, d]);
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        for k in -lines..=lines {
            let offset = k as f32 * spacing;
            let (x, y) = (cx + offset, cy + offset);
            quad(vec3(cx - half, y - width, z), vec3(cx + half, y - width, z), vec3(cx + half, y + width, z), vec3(cx - half, y + width, z));
            quad(vec3(x - width, cy - half, z), vec3(x + width, cy - half, z), vec3(x + width, cy + half, z), vec3(x - width, cy + half, z));
        }
        let grid_mesh = CpuMesh { positions: Positions::F32(positions), indices: Indices::U32(indices), ..Default::default() };
        let grid = Gm::new(
            Mesh::new(context, &grid_mesh),
            ColorMaterial::new_transparent(context, &CpuMaterial { albedo: Srgba::new(200, 210, 230, 70), ..Default::default() }),
        );

        Self { plane, grid, spacing }
    }

    pub fn objects(&self) -> [&dyn Object; 2] {
        [&self.plane, &self.grid]
    }
}

/// Redondea hacia arriba a 1, 2 o 5 × 10ⁿ.
fn nice_step(raw: f32) -> f32 {
    let raw = raw.max(1e-6);
    let magnitude = 10f32.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

#[cfg(test)]
mod tests {
    use super::nice_step;

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(0.7), 1.0);
        assert_eq!(nice_step(1.5), 2.0);
        assert_eq!(nice_step(3.0), 5.0);
        assert_eq!(nice_step(7.0), 10.0);
        assert!((nice_step(23.0) - 50.0).abs() < 1e-3);
    }
}
