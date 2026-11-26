//! A captured frame (rows of RGB) made small enough to send.

/// The server takes no picture taller than this.
const HEIGHT_MAX: u32 = 4320;
const WIDTH_MIN: u32 = 160;

/// Scale down to at most `max_width`, each new pixel the average of the ones it covers.
pub fn shrink(rgb: &[u8], width: u32, height: u32, max_width: u32) -> (Vec<u8>, u32, u32) {
    if width <= max_width {
        return (rgb.to_vec(), width, height);
    }
    let (w, h) = (u64::from(width), u64::from(height));
    let tw = u64::from(max_width.max(1));
    let th = ((h * tw + w / 2) / w).max(1);
    let mut out = Vec::with_capacity((tw * th * 3) as usize);
    for y in 0..th {
        let (y0, y1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1));
            let mut sum = [0u64; 3];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * w + sx) * 3) as usize;
                    for (c, total) in sum.iter_mut().enumerate() {
                        *total += u64::from(rgb[at + c]);
                    }
                }
            }
            let count = (y1 - y0) * (x1 - x0);
            out.extend(sum.map(|total| (total / count) as u8));
        }
    }
    (out, tw as u32, th as u32)
}

fn encode(rgb: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgb).ok()?;
    writer.finish().ok()?;
    Some(out)
}

/// A PNG at most `max_width` wide and `max_bytes` long, or `None` when that cannot be had.
pub fn png(rgb: &[u8], width: u32, height: u32, max_width: u32, max_bytes: usize) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgb.len() as u64 != u64::from(width) * u64::from(height) * 3 {
        return None;
    }
    let mut target = width.min(max_width);
    loop {
        let (small, w, h) = shrink(rgb, width, height, target);
        if h <= HEIGHT_MAX {
            let file = encode(&small, w, h)?;
            if file.len() <= max_bytes {
                return Some(file);
            }
        }
        target /= 2;
        if target < WIDTH_MIN {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(w: u32, h: u32) -> Vec<u8> {
        let mut x = 1u32;
        (0..w * h * 3)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    fn size_of(png_bytes: &[u8]) -> (u32, u32) {
        let reader = png::Decoder::new(std::io::Cursor::new(png_bytes)).read_info().unwrap();
        (reader.info().width, reader.info().height)
    }

    #[test]
    fn shrinking_averages_the_pixels() {
        // two 2x2 blocks: black/white mixed, and plain red
        let rgb = [[0, 0, 0], [255, 255, 255], [255, 0, 0], [255, 0, 0], [255, 255, 255], [0, 0, 0], [255, 0, 0], [255, 0, 0]].concat();
        let (small, w, h) = shrink(&rgb, 4, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(small, vec![127, 127, 127, 255, 0, 0]);
        assert_eq!(shrink(&rgb, 4, 2, 8), (rgb.to_vec(), 4, 2));
    }

    #[test]
    fn the_picture_obeys_the_width_and_the_size() {
        let rgb = noise(800, 400);
        let whole = png(&rgb, 800, 400, 1920, 2_500_000).unwrap();
        assert_eq!(size_of(&whole), (800, 400));
        assert_eq!(size_of(&png(&rgb, 800, 400, 600, 2_500_000).unwrap()), (600, 300));
        // noise does not compress: the picture has to be made smaller to fit
        let small = png(&rgb, 800, 400, 1920, 100_000).unwrap();
        assert!(small.len() <= 100_000);
        assert_eq!(size_of(&small), (200, 100));
        assert_eq!(png(&rgb, 800, 400, 1920, 1000), None);
    }

    #[test]
    fn a_buffer_of_the_wrong_length_gives_nothing() {
        assert_eq!(png(&[0; 10], 4, 4, 1920, 1000), None);
        assert_eq!(png(&[], 0, 0, 1920, 1000), None);
    }
}
