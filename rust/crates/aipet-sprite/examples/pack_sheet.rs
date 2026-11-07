//! Draws a vector pack in every state and expression as one PNG, for looking at it while drawing:
//! `cargo run -p aipet-sprite --example pack_sheet -- <pack folder> <out.png> [scale]`; `PACK_CASES=8,9` draws only
//! those.

use std::path::Path;

use aipet_sprite::bake::Sheet;
use aipet_sprite::compose::Parts;
use aipet_sprite::pose::{
    ART_H, ART_W, Arms, Expression, Facing, Feet, Fx, HALO_PAD, IdleAct, Look, Mood, Prop, StateValue, Values,
};
use aipet_sprite::vector::{self, View};
use aipet_sprite::{Pose, load, manifest};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (dir, out) = (Path::new(&args[1]), Path::new(&args[2]));
    let scale: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let loaded = load::load_folder(dir).unwrap_or_else(|e| panic!("{e}"));
    let pack = &loaded.pack;

    let base = Values::IDLE;
    let mood = |mood, act| Values {
        state: StateValue { mood, act },
        ..base
    };
    let eyes = |expression, look| Values {
        eyes: aipet_sprite::pose::Eyes { expression, look },
        ..base
    };
    let cases: Vec<Values> = vec![
        base,
        Values {
            eyes: aipet_sprite::pose::Eyes {
                expression: Expression::Closed,
                look: Look::CENTER,
            },
            fx: Fx::Zs,
            ..mood(Mood::Sleep, None)
        },
        Values {
            fx: Fx::Dots,
            eyes: aipet_sprite::pose::Eyes {
                expression: Expression::Up,
                look: Look::CENTER,
            },
            ..mood(Mood::Thinking, None)
        },
        Values {
            arms: Arms::Hold,
            prop: Prop::Laptop,
            eyes: aipet_sprite::pose::Eyes {
                expression: Expression::Down,
                look: Look::CENTER,
            },
            ..mood(Mood::Working, None)
        },
        Values {
            arms: Arms::Lens,
            prop: Prop::Lens,
            ..mood(Mood::Working, None)
        },
        Values {
            fx: Fx::Alert,
            arms: Arms::WaveA,
            eyes: aipet_sprite::pose::Eyes {
                expression: Expression::Wide,
                look: Look::CENTER,
            },
            ..mood(Mood::Attention, None)
        },
        Values {
            sparkles: true,
            arms: Arms::Raised,
            eyes: aipet_sprite::pose::Eyes {
                expression: Expression::Happy,
                look: Look::CENTER,
            },
            ..mood(Mood::Done, None)
        },
        Values {
            fx: Fx::Notes,
            headphones: true,
            ..mood(Mood::Listen, None)
        },
        Values {
            arms: Arms::RunA,
            feet: Feet::RunA,
            fx: Fx::Dust,
            ..base
        },
        Values {
            arms: Arms::RunB,
            feet: Feet::RunB,
            fx: Fx::Dust,
            facing: Facing::Left,
            ..base
        },
        Values {
            feet: Feet::DangleA,
            arms: Arms::Raised,
            ..eyes(Expression::Wide, Look::CENTER)
        },
        mood(Mood::Idle, Some(IdleAct::Wiggle)),
        eyes(Expression::Blink, Look::CENTER),
        eyes(Expression::Squint, Look::CENTER),
        eyes(Expression::Open, Look { dx: -2, dy: -1 }),
        eyes(Expression::Open, Look { dx: 2, dy: 1 }),
    ];

    // PACK_CASES=8,9 draws only those cases (by index), bigger
    let cases: Vec<Values> = match std::env::var("PACK_CASES") {
        Ok(list) => list.split(',').filter_map(|i| cases.get(i.trim().parse::<usize>().ok()?).copied()).collect(),
        Err(_) => cases,
    };
    let cols = 4.min(cases.len());
    let (cw, ch) = (
        ((ART_W + 2.0 * HALO_PAD) * scale) as u32,
        ((ART_H + 2.0 * HALO_PAD) * scale) as u32,
    );
    let rows = cases.len().div_ceil(cols) as u32;
    let (w, h) = (cw * cols as u32, ch * rows);
    let mut pixels = vec![0xFF1F2640u32; (w * h) as usize];
    for (i, values) in cases.into_iter().enumerate() {
        let pose = Pose::scripted(values, 0.0);
        let placed = Parts::default().place(pack, &pose, 0.0);
        let items = vector::scene(pack, &placed, &pose);
        let view = View {
            w: cw,
            h: ch,
            x0: -HALO_PAD,
            y0: -HALO_PAD,
            scale,
        };
        let rgba = vector::render(&items, view, |_| true);
        let (ox, oy) = ((i % cols) as u32 * cw, (i / cols) as u32 * ch);
        for y in 0..ch {
            for x in 0..cw {
                let p = &rgba[((y * cw + x) * 4) as usize..][..4];
                let dst = &mut pixels[((oy + y) * w + ox + x) as usize];
                // premultiplied over the background
                let a = u32::from(p[3]);
                let mix = |shift: u32, c: u8| (u32::from(c) + ((*dst >> shift) & 0xFF) * (255 - a) / 255).min(255);
                *dst = 0xFF00_0000 | mix(16, p[0]) << 16 | mix(8, p[1]) << 8 | mix(0, p[2]);
            }
        }
    }
    let png = manifest::encode_png(&Sheet {
        width: w,
        height: h,
        pixels,
    });
    std::fs::write(out, png).unwrap();
}
