//! Where things sit on the pet's surface, in logical px: `MainWindow.axaml`'s 380 × 600 window.
//!
//! The view places its widgets at these positions and the shells build their input region from the same numbers
//! ([`crate::PetUi::hit_rects`]), so what takes the mouse is what is drawn.

use iced::{Point, Rectangle, Size, Vector};

/// The pet's surface (window).
pub const SURFACE: Size = Size::new(380.0, 600.0);
/// The vertical line everything is centred on (Root's margins are the same on both sides).
pub const CENTER_X: f32 = SURFACE.width / 2.0;

/// The sprite unscaled: 26 × 24 grid pixels of 5 px.
pub const SPRITE: Size = Size::new(130.0, 120.0);
/// The sprite's top-left: at the top of the 140 × 124 pet panel, which is centred in the bottom row (124 px) inside
/// Root's 12,8,12,8 margins.
pub const SPRITE_POS: Point = Point::new(CENTER_X - SPRITE.width / 2.0, SURFACE.height - 8.0 - 124.0);
/// The point in the sprite that it squashes and stretches around (its RenderTransformOrigin, 50 % × 91.7 %).
pub const SPRITE_ORIGIN: Vector = Vector::new(SPRITE.width * 0.5, SPRITE.height * 0.917);
/// The bounding box of the sprite's hit ellipse (86 × 88, centred, 14 px from the top), in the sprite.
pub const HIT_ELLIPSE: Rectangle = Rectangle {
    x: 22.0,
    y: 14.0,
    width: 86.0,
    height: 88.0,
};
/// How far the glow's halo reaches past the sprite on each side.
pub const HALO_PAD: f32 = 10.0;

/// The ground shadow: an ellipse 76 × 13 whose top is 103 px down the pet panel.
pub const SHADOW: Size = Size::new(76.0, 13.0);
pub const SHADOW_CENTER: Point = Point::new(CENTER_X, SPRITE_POS.y + 103.0 + SHADOW.height / 2.0);

/// The bottom of the bubble stacks: Sections' 8 px margin above the pet's row.
pub const STACKS_BOTTOM: f32 = SPRITE_POS.y - 8.0;
/// A bubble's height and corner radius.
pub const CARD_H: f32 = 50.0;
pub const CARD_RADIUS: f32 = 25.0;
/// The dismiss button on a bubble's top-left corner: 22 px round, 7 px out from the corner.
pub const CLOSE_SIZE: f32 = 22.0;
const CLOSE_OUT: f32 = 7.0;

/// The reviews header, a pill above the reviews stack: 23 px high, 8 px above the stack, corner radius 10.
pub const HEADER_H: f32 = 23.0;
pub const HEADER_GAP: f32 = 8.0;
pub const HEADER_RADIUS: f32 = 10.0;

/// The menu: 230 wide, 5 px padding and a 1 px border around five 32 px items and a 9 px separator.
pub const MENU: Size = Size::new(230.0, 2.0 * 6.0 + 5.0 * MENU_ITEM_H + MENU_SEPARATOR_H);
pub const MENU_ITEM_H: f32 = 32.0;
pub const MENU_SEPARATOR_H: f32 = 9.0;
pub const MENU_RADIUS: f32 = 10.0;
/// The menu when it is drawn in the pet's surface: centred right above the sprite, where the C# opens it.
pub const INLINE_MENU: Rectangle = Rectangle {
    x: CENTER_X - MENU.width / 2.0,
    y: SPRITE_POS.y - MENU.height,
    width: MENU.width,
    height: MENU.height,
};

/// How the sprite is moved this frame: the pet frame's offset, squash and stretch, and shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub offset: Vector,
    pub scale: Vector,
    pub shadow_scale: f32,
    pub shadow_opacity: f32,
}

impl Motion {
    /// Standing still, unscaled.
    pub const REST: Motion = Motion {
        offset: Vector::new(0.0, 0.0),
        scale: Vector::new(1.0, 1.0),
        shadow_scale: 1.0,
        shadow_opacity: 1.0,
    };

    /// A point of the unscaled sprite (0..130 × 0..120) on the surface: scaled around the origin, then moved.
    /// (Written so that at scale 1 no rounding creeps in.)
    pub fn to_surface(self, p: Point) -> Point {
        Point::new(
            SPRITE_POS.x + p.x * self.scale.x + SPRITE_ORIGIN.x * (1.0 - self.scale.x) + self.offset.x,
            SPRITE_POS.y + p.y * self.scale.y + SPRITE_ORIGIN.y * (1.0 - self.scale.y) + self.offset.y,
        )
    }

    /// A surface point in the unscaled sprite: the inverse of [`Motion::to_surface`].
    pub fn to_sprite(self, p: Point) -> Point {
        Point::new(
            SPRITE_ORIGIN.x + (p.x - self.offset.x - SPRITE_POS.x - SPRITE_ORIGIN.x) / self.scale.x,
            SPRITE_ORIGIN.y + (p.y - self.offset.y - SPRITE_POS.y - SPRITE_ORIGIN.y) / self.scale.y,
        )
    }

    /// A rectangle of the unscaled sprite on the surface (the scales are positive, so corners stay corners).
    pub fn rect_to_surface(self, r: Rectangle) -> Rectangle {
        let top_left = self.to_surface(r.position());
        let size = Size::new(r.width * self.scale.x, r.height * self.scale.y);
        Rectangle::new(top_left, size)
    }

    /// The ground shadow: scaled around its centre, and moved sideways with the sprite.
    pub fn shadow(self) -> Rectangle {
        let size = SHADOW * self.shadow_scale;
        Rectangle::new(
            Point::new(
                SHADOW_CENTER.x + self.offset.x - size.width / 2.0,
                SHADOW_CENTER.y - size.height / 2.0,
            ),
            size,
        )
    }
}

/// Where a bubble is drawn: its unscaled width, and its scale around the middle of its bottom edge, which sits at
/// `bottom_center` (a bubble's RenderTransformOrigin is 50 % × 100 %).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardFrame {
    pub bottom_center: Point,
    pub width: f32,
    pub scale: f32,
}

impl CardFrame {
    /// A point of the unscaled bubble (0..width × 0..50) on the surface.
    pub fn map(&self, p: Point) -> Point {
        Point::new(
            self.bottom_center.x + (p.x - self.width / 2.0) * self.scale,
            self.bottom_center.y + (p.y - CARD_H) * self.scale,
        )
    }

    /// The bubble's rounded body.
    pub fn body(&self) -> Rectangle {
        Rectangle::new(self.map(Point::ORIGIN), Size::new(self.width, CARD_H) * self.scale)
    }

    /// The dismiss button over the top-left corner.
    pub fn close(&self) -> Rectangle {
        Rectangle::new(
            self.map(Point::new(-CLOSE_OUT, -CLOSE_OUT)),
            Size::new(CLOSE_SIZE, CLOSE_SIZE) * self.scale,
        )
    }
}

/// A rectangle of the input region, in whole logical px (surface-local coordinates on Wayland).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    /// The whole pixels that `r` touches, cut to the surface; `None` when none of it is on the surface. A hair of
    /// float error is forgiven, so an edge on a pixel boundary doesn't take the next pixel.
    pub fn covering(r: Rectangle) -> Option<Rect> {
        const EPS: f32 = 1e-3;
        let x0 = (r.x + EPS).floor().max(0.0);
        let y0 = (r.y + EPS).floor().max(0.0);
        let x1 = (r.x + r.width - EPS).ceil().min(SURFACE.width);
        let y1 = (r.y + r.height - EPS).ceil().min(SURFACE.height);
        (x1 > x0 && y1 > y0).then_some(Rect {
            x: x0 as i32,
            y: y0 as i32,
            width: (x1 - x0) as i32,
            height: (y1 - y0) as i32,
        })
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x as f32
            && p.x < (self.x + self.width) as f32
            && p.y >= self.y as f32
            && p.y < (self.y + self.height) as f32
    }

    fn contains_rect(&self, r: &Rect) -> bool {
        r.x >= self.x && r.y >= self.y && r.x + r.width <= self.x + self.width && r.y + r.height <= self.y + self.height
    }

    fn bottom(&self) -> i32 {
        self.y + self.height
    }
}

/// Adds `r` to `rects`, or grows the last rectangle down over it when it spans the same columns and touches it.
fn push_merged(rects: &mut Vec<Rect>, r: Rect) {
    match rects.last_mut() {
        Some(last) if last.x == r.x && last.width == r.width && last.bottom() >= r.y && last.y <= r.y => {
            last.height = last.bottom().max(r.bottom()) - last.y;
        }
        _ => rects.push(r),
    }
}

/// A rounded rectangle as horizontal bands of whole pixels, each as wide as the shape gets within it: every drawn
/// pixel is covered, and the corners let clicks through.
pub fn rounded(r: Rectangle, radius: f32) -> Vec<Rect> {
    const BAND: f32 = 4.0;
    let radius = radius.min(r.width / 2.0).min(r.height / 2.0).max(0.0);
    let bottom = (r.y + r.height).ceil();
    let middle = r.y + r.height / 2.0;
    let mut rects = Vec::new();
    let mut y0 = r.y.floor();
    while y0 < bottom {
        let y1 = (y0 + BAND).min(bottom);
        // the band's row nearest the middle is where the shape is widest in the band
        let y = middle.clamp(y0, y1);
        let into_corner = (radius - (y - r.y).min(r.y + r.height - y)).max(0.0);
        let inset = radius - (radius * radius - into_corner * into_corner).max(0.0).sqrt();
        let band = Rectangle {
            x: r.x + inset,
            y: y0,
            width: r.width - 2.0 * inset,
            height: y1 - y0,
        };
        if let Some(band) = Rect::covering(band.intersection(&r).unwrap_or(band)) {
            push_merged(&mut rects, band);
        }
        y0 = y1;
    }
    rects
}

/// The sprite's input region: the hit ellipse's bounding box plus the body's solid pixels, where the sprite is drawn
/// this frame. `runs` are the body's rows of solid pixels as (x, y, length) in grid pixels.
pub fn sprite_rects(motion: Motion, runs: &[(i32, i32, i32)]) -> Vec<Rect> {
    let ellipse = Rect::covering(motion.rect_to_surface(HIT_ELLIPSE));
    let mut rects: Vec<Rect> = ellipse.into_iter().collect();
    for &(x, y, len) in runs {
        let run = Rectangle {
            x: x as f32 * 5.0,
            y: y as f32 * 5.0,
            width: len as f32 * 5.0,
            height: 5.0,
        };
        let Some(r) = Rect::covering(motion.rect_to_surface(run)) else {
            continue;
        };
        if !ellipse.is_some_and(|e| e.contains_rect(&r)) {
            push_merged(&mut rects, r);
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sprite_sits_where_the_csharp_puts_it() {
        assert_eq!(SPRITE_POS, Point::new(125.0, 468.0));
        assert_eq!(SHADOW_CENTER, Point::new(190.0, 577.5));
        assert_eq!(STACKS_BOTTOM, 460.0);
        assert_eq!(MENU.height, 181.0);
        let rest = Motion::REST.rect_to_surface(Rectangle::new(Point::ORIGIN, SPRITE));
        assert_eq!(rest, Rectangle::new(SPRITE_POS, SPRITE));
    }

    #[test]
    fn squash_keeps_the_origin_and_inverts() {
        let m = Motion {
            offset: Vector::new(3.0, -7.5),
            scale: Vector::new(1.13, 0.87),
            ..Motion::REST
        };
        let origin = Point::new(SPRITE_ORIGIN.x, SPRITE_ORIGIN.y);
        let o = m.to_surface(origin);
        assert!((o.x - (190.0 + 3.0)).abs() < 1e-4 && (o.y - (468.0 + 110.04 - 7.5)).abs() < 1e-3);
        for p in [Point::new(0.0, 0.0), Point::new(130.0, 120.0), Point::new(40.0, 90.0)] {
            let back = m.to_sprite(m.to_surface(p));
            assert!(
                (back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3,
                "{p:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn shadow_scales_around_its_centre_and_follows_sideways() {
        let m = Motion {
            offset: Vector::new(5.0, -20.0),
            shadow_scale: 0.5,
            ..Motion::REST
        };
        assert_eq!(
            m.shadow(),
            Rectangle::new(Point::new(176.0, 574.25), Size::new(38.0, 6.5))
        );
    }

    #[test]
    fn covering_rounds_outwards_and_cuts_to_the_surface() {
        let r = Rect::covering(Rectangle {
            x: 10.2,
            y: 5.0,
            width: 3.0,
            height: 2.5,
        });
        assert_eq!(
            r,
            Some(Rect {
                x: 10,
                y: 5,
                width: 4,
                height: 3
            })
        );
        let edge = Rect::covering(Rectangle {
            x: 370.0,
            y: 590.0,
            width: 30.0,
            height: 30.0,
        });
        assert_eq!(
            edge,
            Some(Rect {
                x: 370,
                y: 590,
                width: 10,
                height: 10
            })
        );
        assert_eq!(
            Rect::covering(Rectangle {
                x: -20.0,
                y: 0.0,
                width: 10.0,
                height: 10.0
            }),
            None
        );
    }

    #[test]
    fn rounded_bands_cover_the_pill_and_skip_its_corners() {
        let pill = Rectangle {
            x: 70.0,
            y: 400.0,
            width: 240.0,
            height: 50.0,
        };
        let rects = rounded(pill, 25.0);
        let hit = |x: f32, y: f32| rects.iter().any(|r| r.contains(Point::new(x, y)));
        // inside, including the ends of the middle line and just inside a corner arc
        assert!(hit(190.0, 425.0) && hit(70.5, 425.0) && hit(309.5, 425.0));
        assert!(hit(70.0 + 25.0 - 17.0, 400.0 + 25.0 - 17.0));
        // the corners' outside
        assert!(!hit(71.0, 401.0) && !hit(309.0, 449.0));
        // bands never reach past the bounding box, and the straight middle is one rectangle
        assert!(
            rects
                .iter()
                .all(|r| r.x >= 70 && r.x + r.width <= 310 && r.y >= 400 && r.y + r.height <= 450)
        );
        assert!(rects.len() < 14, "{rects:?}");
    }

    #[test]
    fn sprite_rects_are_the_ellipse_box_plus_body_runs_outside_it() {
        // one run inside the ellipse box, one sticking out left of it (an arm), two rows of a foot below it
        let runs = [(8, 6, 4), (1, 10, 3), (10, 21, 2), (10, 22, 2)];
        let rects = sprite_rects(Motion::REST, &runs);
        assert_eq!(
            rects,
            vec![
                Rect {
                    x: 147,
                    y: 482,
                    width: 86,
                    height: 88
                },
                Rect {
                    x: 130,
                    y: 518,
                    width: 15,
                    height: 5
                },
                Rect {
                    x: 175,
                    y: 573,
                    width: 10,
                    height: 10
                },
            ]
        );
        // they move with the sprite
        let up = Motion {
            offset: Vector::new(0.0, -10.0),
            ..Motion::REST
        };
        assert_eq!(sprite_rects(up, &runs)[0].y, 472);
    }

    #[test]
    fn card_frame_scales_around_its_bottom_centre() {
        let f = CardFrame {
            bottom_center: Point::new(190.0, 460.0),
            width: 300.0,
            scale: 0.9,
        };
        assert_eq!(
            f.body(),
            Rectangle::new(Point::new(55.0, 415.0), Size::new(270.0, 45.0))
        );
        let full = CardFrame { scale: 1.0, ..f };
        assert_eq!(
            full.close(),
            Rectangle::new(Point::new(33.0, 403.0), Size::new(22.0, 22.0))
        );
    }
}
