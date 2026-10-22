//! Java PNG sprites and uniform animation timelines, compiled through shared texture mips.
use super::source::{Error, Identifier, Source};
use serde_json::Value;
use std::io::Cursor;
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
impl Image {
    /// Java 26.3 Biome/ColorMapColorUtil: clamp climate as f32, then sample in double precision.
    pub fn climate_color(&self, temperature: f32, downfall: f32) -> Result<[u8; 3], Error> {
        if self.width != 256
            || self.height != 256
            || !temperature.is_finite()
            || !downfall.is_finite()
        {
            return Err(invalid("Java climate colormap dimensions or climate"));
        }
        let temperature = f64::from(temperature.clamp(0., 1.));
        let rain = f64::from(downfall.clamp(0., 1.)) * temperature;
        let x = ((1. - temperature) * 255.) as usize;
        let y = ((1. - rain) * 255.) as usize;
        self.rgba
            .get((y * 256 + x) * 4..(y * 256 + x) * 4 + 3)
            .ok_or_else(|| invalid("Java climate colormap pixels"))?
            .try_into()
            .map_err(|_| invalid("Java climate RGB"))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info()?;
        let (width, height) = (reader.info().width, reader.info().height);
        let length = u64::from(width) * u64::from(height) * 4;
        if length == 0 || length > 64 * 1024 * 1024 {
            return Err(Error::Budget("decoded Java PNG (64 MiB budget)".into()));
        }
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| Error::Budget("PNG output size".into()))?;
        if size > length as usize {
            return Err(Error::Budget("PNG output channels".into()));
        }
        let mut pixels = vec![0; size];
        let info = reader.next_frame(&mut pixels)?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            _ => return Err(invalid("indexed PNG was not expanded")),
        };
        let mut rgba = Vec::with_capacity(length as usize);
        for pixel in pixels[..info.buffer_size()].chunks(channels) {
            match channels {
                1 => rgba.extend([pixel[0], pixel[0], pixel[0], 255]),
                2 => rgba.extend([pixel[0], pixel[0], pixel[0], pixel[1]]),
                3 => rgba.extend([pixel[0], pixel[1], pixel[2], 255]),
                _ => rgba.extend(pixel),
            }
        }
        if rgba.len() != length as usize {
            return Err(invalid("PNG pixel count"));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
    /// Preserves pixel boundaries when promoting smaller vanilla tiles to a shared array size.
    pub fn square_layer(&self, size: u32) -> Result<Vec<u8>, Error> {
        if self.width != self.height || size < self.width || !size.is_multiple_of(self.width) {
            return Err(invalid("texture array tile sizes are incompatible"));
        }
        let mut out = vec![0; size as usize * size as usize * 4];
        for y in 0..size {
            for x in 0..size {
                let source =
                    ((y * self.height / size) * self.width + x * self.width / size) as usize * 4;
                let dest = (y * size + x) as usize * 4;
                out[dest..dest + 4].copy_from_slice(&self.rgba[source..source + 4]);
            }
        }
        Ok(out)
    }
}
#[derive(Clone, Debug)]
pub struct Sprite {
    pub frames: Vec<Image>,
    pub timeline: Vec<usize>,
    pub ticks: u32,
    pub interpolate: bool,
}
impl Sprite {
    pub fn load(source: &mut dyn Source, id: &Identifier) -> Result<Self, Error> {
        let path = id.asset_path("textures", "png");
        let image = Image::decode(&source.read(&path)?)?;
        let metadata = source.optional_json(&(path + ".mcmeta"))?;
        let Some(animation) = metadata.as_ref().and_then(|m| m.get("animation")) else {
            if image.width != image.height {
                return Err(invalid("non-square unanimated block sprite"));
            }
            return Ok(Self {
                frames: vec![image],
                timeline: vec![0],
                ticks: 1,
                interpolate: false,
            });
        };
        let dimension = |name: &str| animation.get(name).map(positive).transpose();
        let (w, h) = match (dimension("width")?, dimension("height")?) {
            (None, None) => {
                let side = image.width.min(image.height);
                (side, side)
            }
            (Some(w), None) => (w, image.height),
            (None, Some(h)) => (image.width, h),
            (Some(w), Some(h)) => (w, h),
        };
        if w != h || !image.width.is_multiple_of(w) || !image.height.is_multiple_of(h) {
            return Err(invalid("animation frame dimensions"));
        }
        let count = u64::from(image.width / w) * u64::from(image.height / h);
        if count > (crate::MAX_TEXTURE_LAYERS * (crate::MAX_TEXTURE_PAGES - 1)) as u64 {
            return Err(Error::Budget("Java animation frame layers".into()));
        }
        let mut frames = Vec::new();
        for fy in 0..image.height / h {
            for fx in 0..image.width / w {
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for y in 0..h {
                    let start = (((fy * h + y) * image.width + fx * w) * 4) as usize;
                    rgba.extend_from_slice(&image.rgba[start..start + (w * 4) as usize]);
                }
                frames.push(Image {
                    width: w,
                    height: h,
                    rgba,
                });
            }
        }
        let default_ticks = animation
            .get("frametime")
            .map(positive)
            .transpose()?
            .unwrap_or(1);
        let mut ticks = None;
        let mut timeline = Vec::new();
        if let Some(list) = animation.get("frames") {
            for frame in list
                .as_array()
                .ok_or_else(|| invalid("animation frames list"))?
            {
                let (index, time) = if frame.is_object() {
                    (
                        frame
                            .get("index")
                            .and_then(Value::as_u64)
                            .ok_or_else(|| invalid("animation frame index"))?,
                        frame
                            .get("time")
                            .map(positive)
                            .transpose()?
                            .unwrap_or(default_ticks),
                    )
                } else {
                    (
                        frame
                            .as_u64()
                            .ok_or_else(|| invalid("animation frame index"))?,
                        default_ticks,
                    )
                };
                if index >= frames.len() as u64 {
                    return Err(invalid("animation frame index outside PNG"));
                }
                if ticks.is_some_and(|n| n != time) {
                    return Err(invalid(
                        "variable-duration Java texture animation is not yet supported",
                    ));
                }
                ticks = Some(time);
                timeline.push(index as usize);
            }
        }
        if timeline.is_empty() {
            timeline.extend(0..frames.len());
        }
        let interpolate = animation
            .get("interpolate")
            .map(|b| {
                b.as_bool()
                    .ok_or_else(|| invalid("animation interpolation flag"))
            })
            .transpose()?
            .unwrap_or(false);
        Ok(Self {
            frames,
            timeline,
            ticks: ticks.unwrap_or(default_ticks),
            interpolate,
        })
    }
}
fn positive(value: &Value) -> Result<u32, Error> {
    value
        .as_u64()
        .filter(|n| *n > 0 && *n <= i32::MAX as u64)
        .map(|n| n as u32)
        .ok_or_else(|| invalid("positive animation integer"))
}
fn invalid(message: &str) -> Error {
    Error::Model(message.into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_climate_lookup_promotes_float_climate_before_multiplication() {
        let image = Image {
            width: 256,
            height: 256,
            rgba: (0..256u32)
                .flat_map(|y| (0..256u32).flat_map(move |x| [x as u8, y as u8, 7, 255]))
                .collect(),
        };
        assert_eq!(image.climate_color(0.8, 0.4).unwrap(), [50, 173, 7]);
        assert_eq!(image.climate_color(-1., 2.).unwrap(), [255, 255, 7]);
        assert!(image.climate_color(f32::NAN, 0.).is_err());
    }
    #[test]
    fn pixel_promotion_keeps_alpha_and_texel_orientation() {
        let image = Image {
            width: 2,
            height: 2,
            rgba: vec![1, 2, 3, 0, 4, 5, 6, 255, 7, 8, 9, 128, 10, 11, 12, 255],
        };
        let layer = image.square_layer(4).unwrap();
        assert_eq!(&layer[..8], &[1, 2, 3, 0, 1, 2, 3, 0]);
        assert_eq!(&layer[32..36], &[7, 8, 9, 128]);
        assert!(image.square_layer(3).is_err());
    }
}
