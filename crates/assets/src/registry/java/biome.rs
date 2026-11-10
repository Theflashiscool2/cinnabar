//! Java 26.3 GrassColorModifier and Biome.BIOME_INFO_NOISE, before sRGB averaging.
//! Verified against the pinned native GradientNoise/SimplexNoise and modifier oracle.
use super::selectors::Random;
use std::sync::LazyLock;

pub const SWAMP_COLORS: [[u8; 3]; 2] = [[76, 118, 60], [106, 112, 57]];

/// Native DARK_FOREST applies its packed-channel average before biome blending.
pub fn dark_forest(color: [u8; 3]) -> [u8; 3] {
    let rgb = (u32::from(color[0]) << 16) | (u32::from(color[1]) << 8) | u32::from(color[2]);
    let rgb = ((rgb & 0xfefefe) + 0x28340a) >> 1;
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

static PERMUTATION: LazyLock<[u32; 256]> = LazyLock::new(|| {
    let mut random = Random::new(2345);
    // GradientNoise consumes three nextDouble calls even when the offset scale is zero.
    for _ in 0..6 {
        random.bits(26);
    }
    let mut p = std::array::from_fn(|i| i as u32);
    for i in 0..256 {
        let other = i + random.bounded((256 - i) as u32) as usize;
        p.swap(i, other);
    }
    p
});

/// Selects the native swamp color at the individual blend sample's block position.
pub fn swamp_variant([x, z]: [i32; 2]) -> usize {
    usize::from(f64::from(noise(f64::from(x) * 0.0225, f64::from(z) * 0.0225)) >= -0.1)
}
fn noise(x: f64, z: f64) -> f32 {
    let f = (3f64.sqrt() - 1.) * 0.5;
    let g = (3. - 3f64.sqrt()) / 6.;
    let skew = (x + z) * f;
    let i = (x + skew).floor() as i32;
    let j = (z + skew).floor() as i32;
    let unskew = f64::from(i + j) * g;
    let x0 = x - (f64::from(i) - unskew);
    let z0 = z - (f64::from(j) - unskew);
    let (di, dj) = if x0 > z0 { (1, 0) } else { (0, 1) };
    let perm = |n: i32| PERMUTATION[(n & 255) as usize] as i32;
    let corner = |i: i32, j: i32, x: f64, z: f64| {
        let gradient = perm(i + perm(j)) % 12;
        let dot = match gradient {
            0 => x + z,
            1 => -x + z,
            2 => x - z,
            3 => -x - z,
            4 | 6 => x,
            5 | 7 => -x,
            8 | 10 => z,
            _ => -z,
        };
        let mut t = 0.5 - x * x - z * z;
        if t < 0. {
            0.
        } else {
            t *= t;
            t * t * dot
        }
    };
    (70. * (corner(i, j, x0, z0)
        + corner(
            i + di,
            j + dj,
            x0 - f64::from(di) + g,
            z0 - f64::from(dj) + g,
        )
        + corner(i + 1, j + 1, x0 - 1. + 2. * g, z0 - 1. + 2. * g))) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifiers_match_pinned_native_oracle_at_negative_edges_and_world_border() {
        let cases: Vec<[i64; 4]> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/java-biome-grass.json"
        ))
        .unwrap();
        let mut seen = [false; 2];
        for [x, z, swamp, dark] in cases {
            let variant = swamp_variant([x as i32, z as i32]);
            seen[variant] = true;
            let color = SWAMP_COLORS[variant];
            let packed =
                |c: [u8; 3]| (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]);
            assert_eq!(packed(color), swamp as u32, "{x},{z}");
            assert_eq!(packed(dark_forest([0x79, 0xc0, 0x5a])), dark as u32);
        }
        assert_eq!(seen, [true, true]);
    }
}
