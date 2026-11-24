//! Small immediate-mode 2D HUD geometry and a compact 5x7 bitmap font.

/// Mission information displayed by the native HUD.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HudData {
    /// Number of recovered shards.
    pub collected: usize,
    /// Total number of shards in the mission.
    pub total: usize,
    /// Short uppercase objective or completion message.
    pub status: String,
    /// Contextual interaction hint, when the player is in range.
    pub prompt: Option<String>,
    /// Current player health points.
    pub health: u8,
    /// Maximum player health points.
    pub max_health: u8,
}

/// One colored vertex in the immediate-mode HUD mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudVertex {
    /// Clip-space vertex position.
    pub position: [f32; 2],
    /// RGBA vertex color.
    pub color: [f32; 4],
}

impl HudVertex {
    /// Vertex attributes consumed by the HUD pipeline.
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    /// Returns the GPU vertex-buffer layout.
    pub const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Builds screen-space triangles for the current mission HUD in tests.
#[cfg(test)]
pub fn build_vertices(data: &HudData, width: u32, height: u32) -> Vec<HudVertex> {
    let mut out = Vec::new();
    build_vertices_into(data, width, height, &mut out);
    out
}

/// Rebuilds HUD geometry into an existing vector, retaining its allocation across frames.
pub fn build_vertices_into(data: &HudData, width: u32, height: u32, out: &mut Vec<HudVertex>) {
    let width = f32::from(u16::try_from(width.max(1)).unwrap_or(u16::MAX));
    let height = f32::from(u16::try_from(height.max(1)).unwrap_or(u16::MAX));
    out.clear();
    let cyan = [0.25, 0.88, 1.0, 1.0];
    let white = [0.86, 0.93, 1.0, 1.0];
    let muted = [0.35, 0.52, 0.66, 1.0];
    let panel = [0.015, 0.035, 0.075, 0.88];
    rect(out, 22.0, 20.0, 310.0, 164.0, panel, width, height);
    rect(out, 22.0, 20.0, 3.0, 164.0, cyan, width, height);
    text(out, "RIFTBOUND", 38.0, 34.0, 2.0, cyan, width, height);
    text(
        out,
        &format!("SHARDS {:02}/{:02}", data.collected, data.total),
        38.0,
        62.0,
        2.0,
        white,
        width,
        height,
    );
    rect(out, 38.0, 86.0, 270.0, 9.0, muted, width, height);
    let ratio = if data.total == 0 {
        0.0
    } else {
        let collected = u16::try_from(data.collected).unwrap_or(u16::MAX);
        let total = u16::try_from(data.total).unwrap_or(u16::MAX);
        (f32::from(collected) / f32::from(total)).clamp(0.0, 1.0)
    };
    rect(out, 38.0, 86.0, 270.0 * ratio, 9.0, cyan, width, height);
    text(
        out,
        &format!("HEALTH {}/{}", data.health, data.max_health),
        38.0,
        112.0,
        1.5,
        white,
        width,
        height,
    );
    rect(out, 38.0, 136.0, 270.0, 8.0, muted, width, height);
    let health_ratio = if data.max_health == 0 {
        0.0
    } else {
        (f32::from(data.health.min(data.max_health)) / f32::from(data.max_health)).clamp(0.0, 1.0)
    };
    let health_color = if health_ratio <= 0.25 {
        [1.0, 0.28, 0.22, 1.0]
    } else {
        cyan
    };
    rect(
        out,
        38.0,
        136.0,
        270.0 * health_ratio,
        8.0,
        health_color,
        width,
        height,
    );
    text(out, &data.status, 38.0, 154.0, 1.5, white, width, height);
    if let Some(prompt) = &data.prompt {
        let panel_y = height - 72.0;
        rect(out, 22.0, panel_y, 300.0, 48.0, panel, width, height);
        rect(
            out,
            22.0,
            panel_y,
            3.0,
            48.0,
            [1.0, 0.62, 0.28, 1.0],
            width,
            height,
        );
        text(out, prompt, 40.0, panel_y + 16.0, 2.0, white, width, height);
    }
}

#[allow(clippy::many_single_char_names, clippy::too_many_arguments)]
fn rect(
    out: &mut Vec<HudVertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
    sw: f32,
    sh: f32,
) {
    let x0 = x / sw * 2.0 - 1.0;
    let x1 = (x + w) / sw * 2.0 - 1.0;
    let y0 = 1.0 - y / sh * 2.0;
    let y1 = 1.0 - (y + h) / sh * 2.0;
    let a = [x0, y0];
    let b = [x1, y0];
    let c = [x1, y1];
    let d = [x0, y1];
    for p in [a, b, c, a, c, d] {
        out.push(HudVertex { position: p, color });
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    out: &mut Vec<HudVertex>,
    value: &str,
    x: f32,
    y: f32,
    scale: f32,
    color: [f32; 4],
    sw: f32,
    sh: f32,
) {
    let mut cursor = x;
    for ch in value.chars().take(36) {
        let glyph = bitmap(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    rect(
                        out,
                        cursor + f32::from(u8::try_from(col).unwrap_or(0)) * scale,
                        y + f32::from(u8::try_from(row).unwrap_or(0)) * scale,
                        scale,
                        scale,
                        color,
                        sw,
                        sh,
                    );
                }
            }
        }
        cursor += 6.0 * scale;
    }
}

fn bitmap(ch: char) -> [u8; 7] {
    match ch {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '!' => [4, 4, 4, 4, 4, 0, 4],
        ' ' => [0; 7],
        _ => [31, 17, 5, 4, 0, 4, 0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reusable_builder_rewrites_geometry_without_growing_after_warmup() {
        let data = HudData {
            collected: 3,
            total: 5,
            status: "RETURN TO BEACON".into(),
            prompt: Some("F - EXTRACT".into()),
            health: 75,
            max_health: 100,
        };
        let mut reused = Vec::with_capacity(4_096);
        build_vertices_into(&data, 1280, 720, &mut reused);
        let expected = build_vertices(&data, 1280, 720);
        assert_eq!(reused, expected);
        let warmed_capacity = reused.capacity();

        build_vertices_into(&data, 1280, 720, &mut reused);

        assert_eq!(reused, expected);
        assert_eq!(reused.capacity(), warmed_capacity);
    }

    #[test]
    fn hud_geometry_is_non_empty_and_finite() {
        let data = HudData {
            collected: 3,
            total: 5,
            status: "RETURN TO BEACON".into(),
            prompt: Some("F - EXTRACT".into()),
        };
        let vertices = build_vertices(&data, 1280, 720);
        assert_ne!(vertices.len(), 0);
        assert!(
            vertices
                .iter()
                .all(|v| v.position.iter().all(|n| n.is_finite())
                    && v.color.iter().all(|n| n.is_finite()))
        );
        assert_eq!(vertices.len() % 3, 0);
    }
}
