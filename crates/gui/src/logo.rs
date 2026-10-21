// The ShoutingMonkey logo, drawn in code: a monkey face mid-shout on a
// rounded blue badge. Shared by the window icon (at run time) and the
// .exe icon (by build.rs, which `include!`s this file), so both match.

/// RGBA pixels (straight alpha) of the logo at `size` x `size`.
pub fn render(size: u32) -> Vec<u8> {
    const SAMPLES: u32 = 4; // 4 x 4 supersampling for smooth edges
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = (px as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    if let Some([cr, cg, cb]) = shade(x, y) {
                        r += cr as f32;
                        g += cg as f32;
                        b += cb as f32;
                        a += 1.0;
                    }
                }
            }
            if a > 0.0 {
                let i = ((py * size + px) * 4) as usize;
                let coverage = a / (SAMPLES * SAMPLES) as f32;
                out[i] = (r / a).round() as u8;
                out[i + 1] = (g / a).round() as u8;
                out[i + 2] = (b / a).round() as u8;
                out[i + 3] = (coverage * 255.0).round() as u8;
            }
        }
    }
    out
}

const BADGE: [u8; 3] = [0x1E, 0x63, 0xB8];
const FUR: [u8; 3] = [0x7A, 0x4A, 0x2A];
const SKIN: [u8; 3] = [0xEE, 0xCD, 0xA3];
const DARK: [u8; 3] = [0x2B, 0x1A, 0x10];
const MOUTH: [u8; 3] = [0x4A, 0x16, 0x12];
const TONGUE: [u8; 3] = [0xE0, 0x5A, 0x55];
const SHINE: [u8; 3] = [0xFF, 0xFF, 0xFF];

fn circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0
}

fn rounded_square(x: f32, y: f32, inset: f32, radius: f32) -> bool {
    let (lo, hi) = (inset + radius, 1.0 - inset - radius);
    let dx = (x.clamp(lo, hi) - x).abs();
    let dy = (y.clamp(lo, hi) - y).abs();
    x >= inset && x <= 1.0 - inset && y >= inset && y <= 1.0 - inset && dx * dx + dy * dy <= radius * radius
}

/// Colour at unit coordinates (x right, y down), or `None` if transparent.
/// Later shapes paint over earlier ones.
fn shade(x: f32, y: f32) -> Option<[u8; 3]> {
    let mouth = ellipse(x, y, 0.5, 0.675, 0.085, 0.078);
    if mouth {
        return Some(if ellipse(x, y, 0.5, 0.725, 0.055, 0.035) { TONGUE } else { MOUTH });
    }
    for cx in [0.405, 0.595] {
        if circle(x, y, cx - 0.012, 0.425, 0.014) {
            return Some(SHINE);
        }
        if circle(x, y, cx, 0.435, 0.045) {
            return Some(DARK);
        }
    }
    if ellipse(x, y, 0.475, 0.575, 0.013, 0.010) || ellipse(x, y, 0.525, 0.575, 0.013, 0.010) {
        return Some(DARK);
    }
    let face = circle(x, y, 0.405, 0.44, 0.125) || circle(x, y, 0.595, 0.44, 0.125) || ellipse(x, y, 0.5, 0.62, 0.215, 0.155);
    if face {
        return Some(SKIN);
    }
    if circle(x, y, 0.5, 0.49, 0.305) {
        return Some(FUR);
    }
    for cx in [0.185, 0.815] {
        if circle(x, y, cx, 0.47, 0.075) {
            return Some(SKIN);
        }
        if circle(x, y, cx, 0.47, 0.125) {
            return Some(FUR);
        }
    }
    rounded_square(x, y, 0.03, 0.22).then_some(BADGE)
}
