//! Native Aseprite files (`.aseprite`, `.ase`): a direct reader for the subset a pack needs, with every limit
//! checked before anything is allocated or inflated (a 65535 × 65535 cel or a zlib bomb is an error, not a crash).
//! From the spike on `spike/aseprite` (`rust/proofs/aseprite.md`), which chose it over the `asefile` crate.
//!
//! Frames and durations, layers (normal and group), cels (raw, linked, zlib; RGBA, greyscale, indexed),
//! tags (all four directions, repeat), slices (keys, pivots), palettes (0x2019, else 0x0004).
//! Rejected: tilemap layers and tilesets, external files, non-1:1 pixel ratio. Skipped: every other chunk.
use std::io::Read;

pub struct Limits {
    pub max_file: usize,
    pub max_side: u32,
    pub max_frames: u32,
    pub max_layers: usize,
    pub max_decoded: usize,
}

// every field the format gives is kept, used or not
#[allow(dead_code)]
pub struct Layer {
    pub name: String,
    pub group: bool,
    pub child_level: u16,
    pub flags: u16,
    pub opacity: u8,
}
// every field the format gives is kept, used or not
#[allow(dead_code)]
pub struct Image {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub opacity: u8,
    pub rgba: Vec<u8>,
}
pub struct Tag {
    pub name: String,
    pub from: u16,
    pub to: u16,
    pub direction: u8,
    pub repeat: u16,
}
// every field the format gives is kept, used or not
#[allow(dead_code)]
pub struct Key {
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub pivot: Option<(i32, i32)>,
}
pub struct Slice {
    pub name: String,
    pub keys: Vec<Key>,
}

pub struct File {
    pub width: u32,
    pub height: u32,
    pub durations: Vec<u32>,
    pub layers: Vec<Layer>,
    pub tags: Vec<Tag>,
    pub slices: Vec<Slice>,
    /// cels[frame][layer] -> index into `images` (a linked cel shares its target's index).
    pub cels: Vec<Vec<Option<usize>>>,
    pub images: Vec<Image>,
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self
            .b
            .get(self.at..self.at.checked_add(n).ok_or("overflow")?)
            .ok_or("truncated")?;
        self.at += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "a name is not UTF-8".to_string())
    }
}

enum Raw {
    Pixels {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        opacity: u8,
        data: Vec<u8>,
    },
    Link(u16),
}

pub fn read(bytes: &[u8], lim: &Limits) -> Result<File, String> {
    if bytes.len() > lim.max_file {
        return Err(format!(
            "file is {} bytes, over the {} limit",
            bytes.len(),
            lim.max_file
        ));
    }
    let mut r = R { b: bytes, at: 0 };
    let _size = r.u32()?;
    if r.u16()? != 0xA5E0 {
        return Err("not an Aseprite file".into());
    }
    let frames = r.u16()? as u32;
    let (width, height) = (r.u16()? as u32, r.u16()? as u32);
    let depth = r.u16()?;
    let flags = r.u32()?;
    r.take(2 + 8)?;
    let transparent = r.u8()?;
    r.take(3 + 2)?;
    let (pw, ph) = (r.u8()?, r.u8()?);
    r.take(8 + 84)?;
    // Header checks, before anything is decoded.
    let bpp = match depth {
        32 => 4,
        16 => 2,
        8 => 1,
        d => return Err(format!("colour depth {d} is not supported")),
    };
    if width == 0 || height == 0 || width > lim.max_side || height > lim.max_side {
        return Err(format!("canvas {width}x{height} is outside the limits"));
    }
    if frames == 0 || frames > lim.max_frames {
        return Err(format!("{frames} frames, over the {} limit", lim.max_frames));
    }
    if !(pw == ph || pw == 0 || ph == 0) {
        return Err("non-square pixels are not supported".into());
    }
    let uuids = flags & 4 != 0;

    let mut budget = lim.max_decoded;
    let mut f = File {
        width,
        height,
        durations: vec![],
        layers: vec![],
        tags: vec![],
        slices: vec![],
        cels: vec![],
        images: vec![],
    };
    let (mut palette, mut old_palette): (Vec<[u8; 4]>, Vec<[u8; 4]>) = (vec![], vec![]);
    let mut raw: Vec<Vec<Option<Raw>>> = vec![];
    for _ in 0..frames {
        let fsize = r.u32()? as usize;
        let fstart = r.at - 4;
        if r.u16()? != 0xF1FA {
            return Err("bad frame header".into());
        }
        let old = r.u16()?;
        let ms = r.u16()?;
        r.take(2)?;
        let new = r.u32()?;
        f.durations.push(ms as u32);
        let mut row: Vec<Option<Raw>> = vec![];
        for _ in 0..if new == 0 { old as u32 } else { new } {
            let csize = r.u32()? as usize;
            let kind = r.u16()?;
            let body = r.take(csize.checked_sub(6).ok_or("chunk too small")?)?;
            let mut c = R { b: body, at: 0 };
            match kind {
                0x2004 => {
                    let lflags = c.u16()?;
                    let ty = c.u16()?;
                    let child_level = c.u16()?;
                    c.take(6)?;
                    let opacity = c.u8()?;
                    c.take(3)?;
                    let name = c.str()?;
                    if ty == 2 {
                        return Err(format!("layer {name:?} is a tilemap; tilemaps are not supported"));
                    }
                    let _ = uuids; // the UUID, if any, follows the name and is not needed
                    f.layers.push(Layer {
                        name,
                        group: ty == 1,
                        child_level,
                        flags: lflags,
                        opacity,
                    });
                    if f.layers.len() > lim.max_layers {
                        return Err(format!("over {} layers", lim.max_layers));
                    }
                }
                0x2005 => {
                    let layer = c.u16()? as usize;
                    let x = c.i16()? as i32;
                    let y = c.i16()? as i32;
                    let opacity = c.u8()?;
                    let ty = c.u16()?;
                    c.take(7)?;
                    let cel = match ty {
                        0 | 2 => {
                            let (w, h) = (c.u16()? as u32, c.u16()? as u32);
                            let n = (w as usize) * (h as usize) * bpp;
                            if n > budget {
                                return Err(format!("decoded art over the {} byte limit", lim.max_decoded));
                            }
                            budget -= n;
                            let data = if ty == 0 {
                                c.take(n)?.to_vec()
                            } else {
                                let mut out = Vec::with_capacity(n);
                                flate2::read::ZlibDecoder::new(&body[c.at..])
                                    .take(n as u64 + 1)
                                    .read_to_end(&mut out)
                                    .map_err(|e| format!("bad cel data: {e}"))?;
                                out
                            };
                            if data.len() != n {
                                return Err(format!("cel data is {} bytes, expected {n}", data.len()));
                            }
                            Raw::Pixels {
                                x,
                                y,
                                w,
                                h,
                                opacity,
                                data,
                            }
                        }
                        1 => Raw::Link(c.u16()?),
                        3 => return Err("tilemap cels are not supported".into()),
                        t => return Err(format!("unknown cel type {t}")),
                    };
                    if row.len() <= layer {
                        row.resize_with(layer + 1, || None);
                    }
                    row[layer] = Some(cel);
                }
                0x2018 => {
                    let n = c.u16()?;
                    c.take(8)?;
                    for _ in 0..n {
                        let (from, to, direction, repeat) = (c.u16()?, c.u16()?, c.u8()?, c.u16()?);
                        c.take(6 + 4)?;
                        let name = c.str()?;
                        if from > to || to as u32 >= frames || direction > 3 {
                            return Err(format!("tag {name:?} is malformed"));
                        }
                        f.tags.push(Tag {
                            name,
                            from,
                            to,
                            direction,
                            repeat,
                        });
                    }
                }
                0x2022 => {
                    let n = c.u32()?;
                    let sflags = c.u32()?;
                    c.take(4)?;
                    let name = c.str()?;
                    if n > frames {
                        return Err(format!("slice {name:?} has more keys than frames"));
                    }
                    let mut keys = vec![];
                    for _ in 0..n {
                        let (frame, x, y, w, h) = (c.u32()?, c.i32()?, c.i32()?, c.u32()?, c.u32()?);
                        if sflags & 1 != 0 {
                            c.take(16)?;
                        }
                        let pivot = if sflags & 2 != 0 {
                            Some((c.i32()?, c.i32()?))
                        } else {
                            None
                        };
                        keys.push(Key {
                            frame,
                            x,
                            y,
                            w,
                            h,
                            pivot,
                        });
                    }
                    f.slices.push(Slice { name, keys });
                }
                0x2019 => {
                    let size = c.u32()? as usize;
                    let (first, last) = (c.u32()? as usize, c.u32()? as usize);
                    c.take(8)?;
                    if size > 256 || first > last || last >= size {
                        return Err("bad palette".into());
                    }
                    palette.resize(size, [0; 4]);
                    for entry in &mut palette[first..=last] {
                        let fl = c.u16()?;
                        *entry = [c.u8()?, c.u8()?, c.u8()?, c.u8()?];
                        if fl & 1 != 0 {
                            c.str()?;
                        }
                    }
                }
                0x0004 => {
                    let mut i = 0usize;
                    for _ in 0..c.u16()? {
                        i += c.u8()? as usize;
                        let n = match c.u8()? {
                            0 => 256,
                            n => n as usize,
                        };
                        for _ in 0..n {
                            if i >= 256 {
                                return Err("bad old palette".into());
                            }
                            if old_palette.len() <= i {
                                old_palette.resize(i + 1, [0; 4]);
                            }
                            old_palette[i] = [c.u8()?, c.u8()?, c.u8()?, 255];
                            i += 1;
                        }
                    }
                }
                0x2008 => return Err("the file links external files; those are not supported".into()),
                0x2023 => return Err("the file has a tileset; tilesets are not supported".into()),
                _ => {} // colour profile, user data, cel extra, mask, path, future chunks
            }
        }
        raw.push(row);
        r.at = fstart.checked_add(fsize).ok_or("overflow")?;
    }
    if palette.is_empty() {
        palette = old_palette;
    }

    // Convert to RGBA and resolve links.
    for (fr, cels) in raw.iter_mut().enumerate() {
        let mut row = vec![None; f.layers.len()];
        for (l, cel) in cels.iter_mut().enumerate() {
            if l >= f.layers.len() {
                return Err("a cel names an unknown layer".into());
            }
            match cel.take() {
                None => {}
                Some(Raw::Pixels {
                    x,
                    y,
                    w,
                    h,
                    opacity,
                    data,
                }) => {
                    let bg = f.layers[l].flags & 8 != 0;
                    let rgba = match bpp {
                        4 => data,
                        2 => data
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .flat_map(|p| [p[0], p[0], p[0], p[1]])
                            .collect(),
                        _ => data
                            .iter()
                            .map(|&i| {
                                if i == transparent && !bg {
                                    return Ok([0; 4]);
                                }
                                palette
                                    .get(i as usize)
                                    .copied()
                                    .ok_or(format!("colour index {i} is not in the palette"))
                            })
                            .collect::<Result<Vec<_>, _>>()?
                            .concat(),
                    };
                    f.images.push(Image {
                        x,
                        y,
                        w,
                        h,
                        opacity,
                        rgba,
                    });
                    row[l] = Some(f.images.len() - 1);
                }
                Some(Raw::Link(to)) => {
                    let t = f
                        .cels
                        .get(to as usize)
                        .and_then(|r: &Vec<Option<usize>>| r.get(l).copied().flatten());
                    row[l] = Some(t.ok_or(format!("frame {fr} links to an empty or later cel"))?);
                }
            }
        }
        f.cels.push(row);
    }
    Ok(f)
}

impl File {
    /// A layer's pixels at a frame on the whole canvas, straight 0xAARRGGBB, with the cel's and the layer's opacity
    /// applied; `None` when the layer has no cel there.
    pub fn layer_pixels(&self, frame: usize, layer: usize) -> Option<Vec<u32>> {
        let img = &self.images[(*self.cels.get(frame)?.get(layer)?)?];
        let op = u32::from(img.opacity) * u32::from(self.layers[layer].opacity);
        let mut out = vec![0u32; (self.width * self.height) as usize];
        for (i, p) in img.rgba.as_chunks::<4>().0.iter().enumerate() {
            let (x, y) = (img.x + (i as u32 % img.w) as i32, img.y + (i as u32 / img.w) as i32);
            if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 || p[3] == 0 {
                continue;
            }
            let a = (u32::from(p[3]) * op + 255 * 255 / 2) / (255 * 255);
            out[(y as u32 * self.width + x as u32) as usize] =
                (a << 24) | (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
        }
        Some(out)
    }
}

#[cfg(test)]
#[allow(dead_code)] // a writer for every case the format has, whichever the tests use
pub(crate) mod writer {
    /// A minimal, test-only `.aseprite` writer for the documented subset:
    /// header, frames, palette (0x2019), layers (0x2004), cels (0x2005: raw, linked, zlib), tags (0x2018, each
    /// followed by an empty user data chunk 0x2020), slices (0x2022, with pivots). RGBA and indexed.
    /// Format: https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md
    use std::io::Write;

    #[derive(Clone, Copy)]
    pub enum Depth {
        Rgba,
        Indexed { transparent: u8 },
    }

    pub struct Layer {
        pub name: String,
        pub group: bool,
        pub child_level: u16,
        pub visible: bool,
    }

    pub enum CelData {
        Raw(Vec<u8>),
        Zlib(Vec<u8>),
        Linked(u16),
    }
    pub struct Cel {
        pub layer: u16,
        pub x: i16,
        pub y: i16,
        pub w: u16,
        pub h: u16,
        pub data: CelData,
    }

    pub struct Frame {
        pub ms: u16,
        pub cels: Vec<Cel>,
    }

    /// direction: 0 forward, 1 reverse, 2 ping-pong, 3 ping-pong reverse. repeat: 0 = unspecified (loop).
    pub struct Tag {
        pub name: String,
        pub from: u16,
        pub to: u16,
        pub direction: u8,
        pub repeat: u16,
    }

    pub struct SliceKey {
        pub frame: u32,
        pub x: i32,
        pub y: i32,
        pub w: u32,
        pub h: u32,
        pub pivot: Option<(i32, i32)>,
    }
    pub struct Slice {
        pub name: String,
        pub keys: Vec<SliceKey>,
    }

    pub struct Sprite {
        pub width: u16,
        pub height: u16,
        pub depth: Depth,
        pub palette: Vec<[u8; 4]>,
        pub layers: Vec<Layer>,
        pub frames: Vec<Frame>,
        pub tags: Vec<Tag>,
        pub slices: Vec<Slice>,
        /// Header flag 4: layers carry a UUID (Aseprite 1.3.x). Lets a test check a reader skips it.
        pub layer_uuids: bool,
    }

    struct Buf(Vec<u8>);
    impl Buf {
        fn b(&mut self, v: u8) {
            self.0.push(v)
        }
        fn w(&mut self, v: u16) {
            self.0.extend_from_slice(&v.to_le_bytes())
        }
        fn s(&mut self, v: i16) {
            self.0.extend_from_slice(&v.to_le_bytes())
        }
        fn d(&mut self, v: u32) {
            self.0.extend_from_slice(&v.to_le_bytes())
        }
        fn l(&mut self, v: i32) {
            self.0.extend_from_slice(&v.to_le_bytes())
        }
        fn zeros(&mut self, n: usize) {
            self.0.resize(self.0.len() + n, 0)
        }
        fn str(&mut self, v: &str) {
            self.w(v.len() as u16);
            self.0.extend_from_slice(v.as_bytes())
        }
    }

    fn chunk(out: &mut Vec<Vec<u8>>, kind: u16, body: Buf) {
        let mut c = Buf(Vec::with_capacity(body.0.len() + 6));
        c.d(body.0.len() as u32 + 6); // the size includes the 6-byte chunk header
        c.w(kind);
        c.0.extend_from_slice(&body.0);
        out.push(c.0);
    }

    pub fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }

    pub fn write(sp: &Sprite) -> Vec<u8> {
        let depth = match sp.depth {
            Depth::Rgba => 32,
            Depth::Indexed { .. } => 8,
        };
        let mut frames_bytes = Vec::new();
        for (fi, frame) in sp.frames.iter().enumerate() {
            let mut chunks = Vec::new();
            if fi == 0 {
                if !sp.palette.is_empty() {
                    let mut p = Buf(Vec::new());
                    p.d(sp.palette.len() as u32);
                    p.d(0);
                    p.d(sp.palette.len() as u32 - 1);
                    p.zeros(8);
                    for c in &sp.palette {
                        p.w(0);
                        p.b(c[0]);
                        p.b(c[1]);
                        p.b(c[2]);
                        p.b(c[3]);
                    }
                    chunk(&mut chunks, 0x2019, p);
                }
                for (li, layer) in sp.layers.iter().enumerate() {
                    let mut p = Buf(Vec::new());
                    p.w(if layer.visible { 1 | 2 } else { 2 }); // visible | editable
                    p.w(if layer.group { 1 } else { 0 });
                    p.w(layer.child_level);
                    p.w(0);
                    p.w(0); // default width/height, ignored
                    p.w(0); // blend: normal
                    p.b(255);
                    p.zeros(3);
                    p.str(&layer.name);
                    if sp.layer_uuids {
                        p.0.extend_from_slice(&[li as u8 + 1; 16]);
                    }
                    chunk(&mut chunks, 0x2004, p);
                }
            }
            for cel in &frame.cels {
                let mut p = Buf(Vec::new());
                p.w(cel.layer);
                p.s(cel.x);
                p.s(cel.y);
                p.b(255);
                p.w(match cel.data {
                    CelData::Raw(_) => 0,
                    CelData::Linked(_) => 1,
                    CelData::Zlib(_) => 2,
                });
                p.s(0); // z-index
                p.zeros(5);
                match &cel.data {
                    CelData::Raw(px) => {
                        p.w(cel.w);
                        p.w(cel.h);
                        p.0.extend_from_slice(px)
                    }
                    CelData::Zlib(px) => {
                        p.w(cel.w);
                        p.w(cel.h);
                        p.0.extend_from_slice(&zlib(px))
                    }
                    CelData::Linked(to) => p.w(*to),
                }
                chunk(&mut chunks, 0x2005, p);
            }
            if fi == 0 && !sp.tags.is_empty() {
                let mut p = Buf(Vec::new());
                p.w(sp.tags.len() as u16);
                p.zeros(8);
                for t in &sp.tags {
                    p.w(t.from);
                    p.w(t.to);
                    p.b(t.direction);
                    p.w(t.repeat);
                    p.zeros(6);
                    p.b(0);
                    p.b(0);
                    p.b(0);
                    p.b(0); // deprecated RGB + extra byte
                    p.str(&t.name);
                }
                chunk(&mut chunks, 0x2018, p);
                for _ in &sp.tags {
                    let mut u = Buf(Vec::new());
                    u.d(0);
                    chunk(&mut chunks, 0x2020, u);
                }
            }
            if fi == 0 {
                for s in &sp.slices {
                    let pivot = s.keys.iter().any(|k| k.pivot.is_some());
                    let mut p = Buf(Vec::new());
                    p.d(s.keys.len() as u32);
                    p.d(if pivot { 2 } else { 0 });
                    p.d(0);
                    p.str(&s.name);
                    for k in &s.keys {
                        p.d(k.frame);
                        p.l(k.x);
                        p.l(k.y);
                        p.d(k.w);
                        p.d(k.h);
                        if pivot {
                            let (px, py) = k.pivot.unwrap_or((0, 0));
                            p.l(px);
                            p.l(py);
                        }
                    }
                    chunk(&mut chunks, 0x2022, p);
                }
            }
            let body: usize = chunks.iter().map(Vec::len).sum();
            let mut f = Buf(Vec::with_capacity(body + 16));
            f.d(body as u32 + 16); // the size includes the 16-byte frame header
            f.w(0xF1FA);
            f.w(chunks.len().min(0xFFFF) as u16);
            f.w(frame.ms);
            f.zeros(2);
            f.d(chunks.len() as u32);
            for c in chunks {
                f.0.extend_from_slice(&c);
            }
            frames_bytes.push(f.0);
        }
        let total: usize = 128 + frames_bytes.iter().map(Vec::len).sum::<usize>();
        let mut h = Buf(Vec::with_capacity(total));
        h.d(total as u32);
        h.w(0xA5E0);
        h.w(sp.frames.len() as u16);
        h.w(sp.width);
        h.w(sp.height);
        h.w(depth);
        h.d(1 | if sp.layer_uuids { 4 } else { 0 }); // layer opacity valid
        h.w(100); // deprecated speed
        h.d(0);
        h.d(0);
        h.b(match sp.depth {
            Depth::Indexed { transparent } => transparent,
            Depth::Rgba => 0,
        });
        h.zeros(3);
        h.w(sp.palette.len() as u16);
        h.b(1);
        h.b(1); // pixel ratio 1:1
        h.s(0);
        h.s(0);
        h.w(16);
        h.w(16); // grid
        h.zeros(84);
        assert_eq!(h.0.len(), 128);
        for f in frames_bytes {
            h.0.extend_from_slice(&f);
        }
        h.0
    }
}
