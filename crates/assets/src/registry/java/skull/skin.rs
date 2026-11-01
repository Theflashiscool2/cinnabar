//! Head-only skin preparation, following 26.3 SkinTextureDownloader's alpha rules.
//! Legacy body limbs are intentionally not synthesized: skulls sample only the top 32 rows.
use super::super::{source::Error, textures::Image};

pub const SIDE: usize = 64;
pub const PIXEL_BYTES: usize = SIDE * SIDE * 4;

pub fn decode(bytes: &[u8]) -> Result<Image, Error> {
    // Check dimensions before PNG decoding can allocate pixel buffers.
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err(Error::Model("Java head skin PNG header expected".into()));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width != SIDE as u32 || (height != (SIDE / 2) as u32 && height != SIDE as u32) {
        return Err(Error::Model("Java head skin must be 64x32 or 64x64".into()));
    }
    prepare(Image::decode(bytes)?)
}
fn prepare(mut image: Image) -> Result<Image, Error> {
    if image.width != SIDE as u32
        || (image.height != (SIDE / 2) as u32 && image.height != SIDE as u32)
        || image.rgba.len() != (image.width * image.height * 4) as usize
    {
        return Err(Error::Model("invalid Java head skin pixels".into()));
    }
    let legacy = image.height == (SIDE / 2) as u32;
    // SkullModel's base head must remain opaque even when downloaded pixels have alpha.
    for y in 0..SIDE / 4 {
        for x in 0..SIDE / 2 {
            image.rgba[(y * SIDE + x) * 4 + 3] = 255;
        }
    }
    if legacy {
        // Original legacy skins used an opaque unused hat region. Preserve deliberate
        // transparent hats, but clear a uniformly nontransparent region as vanilla does.
        if (0..SIDE / 2)
            .all(|y| (SIDE / 2..SIDE).all(|x| image.rgba[(y * SIDE + x) * 4 + 3] >= 128))
        {
            for y in 0..SIDE / 2 {
                for x in SIDE / 2..SIDE {
                    image.rgba[(y * SIDE + x) * 4 + 3] = 0;
                }
            }
        }
        image.rgba.resize(PIXEL_BYTES, 0);
        image.height = SIDE as u32;
    }
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn image(height: u32, alpha: u8) -> Image {
        let mut rgba = vec![73; (64 * height * 4) as usize];
        for a in rgba.iter_mut().skip(3).step_by(4) {
            *a = alpha;
        }
        Image {
            width: 64,
            height,
            rgba,
        }
    }
    #[test]
    fn legacy_opaque_unused_hat_clears_but_intended_alpha_survives() {
        let result = prepare(image(32, 255)).unwrap();
        assert_eq!([result.width, result.height], [64, 64]);
        assert_eq!(&result.rgba[(8 * 64 + 8) * 4..][..4], &[73, 73, 73, 255]);
        assert_eq!(result.rgba[(8 * 64 + 40) * 4 + 3], 0);
        let mut deliberate = image(32, 255);
        deliberate.rgba[(8 * 64 + 40) * 4 + 3] = 127;
        let result = prepare(deliberate).unwrap();
        assert_eq!(result.rgba[(8 * 64 + 40) * 4 + 3], 127);
        assert_eq!(result.rgba[(9 * 64 + 40) * 4 + 3], 255);
    }
    #[test]
    fn modern_hat_alpha_preserved_base_forced_opaque_and_invalid_sizes_rejected() {
        let result = prepare(image(64, 12)).unwrap();
        assert_eq!(result.rgba[(8 * 64 + 8) * 4 + 3], 255);
        assert_eq!(result.rgba[(8 * 64 + 40) * 4 + 3], 12);
        assert!(decode(b"not a PNG").is_err());
        let mut bad = image(64, 255);
        bad.width = 128;
        assert!(prepare(bad).is_err());
    }
}
