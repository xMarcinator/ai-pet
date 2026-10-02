//! CPU-side bitmaps: soft elliptical shadow and a blurred "glow" from a sprite's glow layer.
//! iced 0.14 has no blur filter for images, so these are precomputed once and drawn with `image`.
use iced::widget::image;

/// Straight-alpha RGBA ellipse whose alpha falls off smoothly towards the rim.
pub fn soft_ellipse(w: u32, h: u32) -> image::Handle {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    for y in 0..h {
        for x in 0..w {
            let dx = (x as f32 + 0.5 - cx) / cx;
            let dy = (y as f32 + 0.5 - cy) / cy;
            let d = (dx * dx + dy * dy).sqrt(); // 0 at centre, 1 at rim
            let a = (1.0 - d).clamp(0.0, 1.0);
            let a = a * a * (3.0 - 2.0 * a) * 0.45; // smoothstep, max 45% opacity
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[0, 0, 0, (a * 255.0) as u8]);
        }
    }
    image::Handle::from_rgba(w, h, px)
}

/// Blurs a straight-alpha RGBA glow layer (w x h) upscaled by `scale`, padded by `radius`
/// on each side, with three box-blur passes (close to a gaussian). Returns (w', h', rgba).
pub fn blurred_glow(src: &[u8], w: u32, h: u32, scale: u32, radius: u32) -> (u32, u32, Vec<u8>) {
    let (ow, oh) = (w * scale + 2 * radius, h * scale + 2 * radius);
    // Premultiplied float buffer, nearest-neighbour upscale into the padded canvas.
    let mut buf = vec![[0f32; 4]; (ow * oh) as usize];
    for y in 0..h * scale {
        for x in 0..w * scale {
            let s = (((y / scale) * w + x / scale) * 4) as usize;
            let a = src[s + 3] as f32 / 255.0;
            let d = ((y + radius) * ow + x + radius) as usize;
            buf[d] = [src[s] as f32 * a, src[s + 1] as f32 * a, src[s + 2] as f32 * a, a];
        }
    }
    let r = (radius / 3).max(1) as i32;
    for _ in 0..3 {
        box_blur(&mut buf, ow as i32, oh as i32, r, true);
        box_blur(&mut buf, ow as i32, oh as i32, r, false);
    }
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for (i, p) in buf.iter().enumerate() {
        let a = p[3].clamp(0.0, 1.0);
        if a > 0.0 {
            // back to straight alpha, which is what Handle::from_rgba expects
            out[i * 4] = (p[0] / a).clamp(0.0, 255.0) as u8;
            out[i * 4 + 1] = (p[1] / a).clamp(0.0, 255.0) as u8;
            out[i * 4 + 2] = (p[2] / a).clamp(0.0, 255.0) as u8;
            out[i * 4 + 3] = (a * 255.0) as u8;
        }
    }
    (ow, oh, out)
}

fn box_blur(buf: &mut [[f32; 4]], w: i32, h: i32, r: i32, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: i32, i: i32| -> usize {
        if horizontal { (line * w + i) as usize } else { (i * w + line) as usize }
    };
    let mut tmp = vec![[0f32; 4]; len as usize];
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut acc = [0f32; 4];
        for i in -r..=r {
            let p = buf[idx(line, i.clamp(0, len - 1))];
            for c in 0..4 {
                acc[c] += p[c];
            }
        }
        for i in 0..len {
            tmp[i as usize] = acc.map(|v| v * norm);
            let add = buf[idx(line, (i + r + 1).min(len - 1))];
            let sub = buf[idx(line, (i - r).max(0))];
            for c in 0..4 {
                acc[c] += add[c] - sub[c];
            }
        }
        for i in 0..len {
            buf[idx(line, i)] = tmp[i as usize];
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn glow_spreads_and_stays_in_bounds() {
        // 2x2 opaque white source, scale 5, radius 6 -> 22x22 output
        let src = vec![255u8; 2 * 2 * 4];
        let (w, h, out) = super::blurred_glow(&src, 2, 2, 5, 6);
        assert_eq!((w, h), (22, 22));
        let a = |x: u32, y: u32| out[((y * w + x) * 4 + 3) as usize];
        assert!(a(11, 11) > 200, "centre alpha {}", a(11, 11));
        assert!(a(3, 11) > 0 && a(3, 11) < a(11, 11), "halo {}", a(3, 11));
        assert_eq!(a(0, 0), 0);
        assert_eq!(out[((11 * w + 11) * 4) as usize], 255, "straight-alpha colour preserved");
    }
}
