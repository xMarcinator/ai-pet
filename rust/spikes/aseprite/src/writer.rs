//! A minimal, test-only `.aseprite` writer for the documented subset:
//! header, frames, palette (0x2019), layers (0x2004), cels (0x2005: raw, linked, zlib), tags (0x2018, each
//! followed by an empty user data chunk 0x2020), slices (0x2022, with pivots). RGBA and indexed.
//! Format: https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md
use std::io::Write;

#[derive(Clone, Copy)]
pub enum Depth { Rgba, Indexed { transparent: u8 } }

pub struct Layer { pub name: String, pub group: bool, pub child_level: u16, pub visible: bool }

pub enum CelData { Raw(Vec<u8>), Zlib(Vec<u8>), Linked(u16) }
pub struct Cel { pub layer: u16, pub x: i16, pub y: i16, pub w: u16, pub h: u16, pub data: CelData }

pub struct Frame { pub ms: u16, pub cels: Vec<Cel> }

/// direction: 0 forward, 1 reverse, 2 ping-pong, 3 ping-pong reverse. repeat: 0 = unspecified (loop).
pub struct Tag { pub name: String, pub from: u16, pub to: u16, pub direction: u8, pub repeat: u16 }

pub struct SliceKey { pub frame: u32, pub x: i32, pub y: i32, pub w: u32, pub h: u32, pub pivot: Option<(i32, i32)> }
pub struct Slice { pub name: String, pub keys: Vec<SliceKey> }

pub struct Sprite {
    pub width: u16, pub height: u16, pub depth: Depth,
    pub palette: Vec<[u8; 4]>,
    pub layers: Vec<Layer>, pub frames: Vec<Frame>, pub tags: Vec<Tag>, pub slices: Vec<Slice>,
    /// Header flag 4: layers carry a UUID (Aseprite 1.3.x). Lets a test check a reader skips it.
    pub layer_uuids: bool,
}

struct Buf(Vec<u8>);
impl Buf {
    fn b(&mut self, v: u8) { self.0.push(v) }
    fn w(&mut self, v: u16) { self.0.extend_from_slice(&v.to_le_bytes()) }
    fn s(&mut self, v: i16) { self.0.extend_from_slice(&v.to_le_bytes()) }
    fn d(&mut self, v: u32) { self.0.extend_from_slice(&v.to_le_bytes()) }
    fn l(&mut self, v: i32) { self.0.extend_from_slice(&v.to_le_bytes()) }
    fn zeros(&mut self, n: usize) { self.0.resize(self.0.len() + n, 0) }
    fn str(&mut self, v: &str) { self.w(v.len() as u16); self.0.extend_from_slice(v.as_bytes()) }
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
    let depth = match sp.depth { Depth::Rgba => 32, Depth::Indexed { .. } => 8 };
    let mut frames_bytes = Vec::new();
    for (fi, frame) in sp.frames.iter().enumerate() {
        let mut chunks = Vec::new();
        if fi == 0 {
            if !sp.palette.is_empty() {
                let mut p = Buf(Vec::new());
                p.d(sp.palette.len() as u32); p.d(0); p.d(sp.palette.len() as u32 - 1); p.zeros(8);
                for c in &sp.palette { p.w(0); p.b(c[0]); p.b(c[1]); p.b(c[2]); p.b(c[3]); }
                chunk(&mut chunks, 0x2019, p);
            }
            for (li, layer) in sp.layers.iter().enumerate() {
                let mut p = Buf(Vec::new());
                p.w(if layer.visible { 1 | 2 } else { 2 }); // visible | editable
                p.w(if layer.group { 1 } else { 0 });
                p.w(layer.child_level);
                p.w(0); p.w(0); // default width/height, ignored
                p.w(0); // blend: normal
                p.b(255); p.zeros(3);
                p.str(&layer.name);
                if sp.layer_uuids { p.0.extend_from_slice(&[li as u8 + 1; 16]); }
                chunk(&mut chunks, 0x2004, p);
            }
        }
        for cel in &frame.cels {
            let mut p = Buf(Vec::new());
            p.w(cel.layer); p.s(cel.x); p.s(cel.y); p.b(255);
            p.w(match cel.data { CelData::Raw(_) => 0, CelData::Linked(_) => 1, CelData::Zlib(_) => 2 });
            p.s(0); // z-index
            p.zeros(5);
            match &cel.data {
                CelData::Raw(px) => { p.w(cel.w); p.w(cel.h); p.0.extend_from_slice(px) }
                CelData::Zlib(px) => { p.w(cel.w); p.w(cel.h); p.0.extend_from_slice(&zlib(px)) }
                CelData::Linked(to) => p.w(*to),
            }
            chunk(&mut chunks, 0x2005, p);
        }
        if fi == 0 && !sp.tags.is_empty() {
            let mut p = Buf(Vec::new());
            p.w(sp.tags.len() as u16); p.zeros(8);
            for t in &sp.tags {
                p.w(t.from); p.w(t.to); p.b(t.direction); p.w(t.repeat); p.zeros(6);
                p.b(0); p.b(0); p.b(0); p.b(0); // deprecated RGB + extra byte
                p.str(&t.name);
            }
            chunk(&mut chunks, 0x2018, p);
            for _ in &sp.tags { let mut u = Buf(Vec::new()); u.d(0); chunk(&mut chunks, 0x2020, u); }
        }
        if fi == 0 {
            for s in &sp.slices {
                let pivot = s.keys.iter().any(|k| k.pivot.is_some());
                let mut p = Buf(Vec::new());
                p.d(s.keys.len() as u32); p.d(if pivot { 2 } else { 0 }); p.d(0);
                p.str(&s.name);
                for k in &s.keys {
                    p.d(k.frame); p.l(k.x); p.l(k.y); p.d(k.w); p.d(k.h);
                    if pivot { let (px, py) = k.pivot.unwrap_or((0, 0)); p.l(px); p.l(py); }
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
        for c in chunks { f.0.extend_from_slice(&c); }
        frames_bytes.push(f.0);
    }
    let total: usize = 128 + frames_bytes.iter().map(Vec::len).sum::<usize>();
    let mut h = Buf(Vec::with_capacity(total));
    h.d(total as u32); h.w(0xA5E0); h.w(sp.frames.len() as u16);
    h.w(sp.width); h.w(sp.height); h.w(depth);
    h.d(1 | if sp.layer_uuids { 4 } else { 0 }); // layer opacity valid
    h.w(100); // deprecated speed
    h.d(0); h.d(0);
    h.b(match sp.depth { Depth::Indexed { transparent } => transparent, Depth::Rgba => 0 });
    h.zeros(3);
    h.w(sp.palette.len() as u16);
    h.b(1); h.b(1); // pixel ratio 1:1
    h.s(0); h.s(0); h.w(16); h.w(16); // grid
    h.zeros(84);
    assert_eq!(h.0.len(), 128);
    for f in frames_bytes { h.0.extend_from_slice(&f); }
    h.0
}
