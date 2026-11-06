//! The size baseline for question 7: the same tiny-skia drawing and PNG output without rive-rs.
use tiny_skia as sk;

fn main() {
    let mut pixmap = sk::Pixmap::new(260, 240).unwrap();
    let mut pb = sk::PathBuilder::new();
    pb.push_circle(130.0, 120.0, 80.0);
    let path = pb.finish().unwrap();
    let mut paint = sk::Paint::default();
    paint.shader = sk::LinearGradient::new(
        sk::Point::from_xy(0.0, 0.0),
        sk::Point::from_xy(260.0, 240.0),
        vec![sk::GradientStop::new(0.0, sk::Color::BLACK), sk::GradientStop::new(1.0, sk::Color::WHITE)],
        sk::SpreadMode::Pad,
        sk::Transform::identity(),
    )
    .unwrap();
    pixmap.fill_path(&path, &paint, sk::FillRule::Winding, sk::Transform::identity(), None);
    let mut mask = sk::Mask::new(260, 240).unwrap();
    mask.fill_path(&path, sk::FillRule::Winding, true, sk::Transform::identity());
    pixmap.stroke_path(&path, &paint, &sk::Stroke::default(), sk::Transform::identity(), Some(&mask));
    let out = std::env::args().nth(1).unwrap_or_else(|| "baseline.png".into());
    pixmap.save_png(out).unwrap();
}
