//! Prints how high each built-in pack's effects reach above its art box (fn-2.21's measurement).
use aipet_sprite::pose::Fx;
fn main() {
    for p in aipet_sprite::load::all(std::path::Path::new("/nonexistent")).pets {
        print!("{:<18}", p.pack.name);
        for fx in [Fx::Zs, Fx::Dots, Fx::Notes, Fx::Alert, Fx::Sparkles, Fx::Dust] {
            print!(" {}={:>6.1}", fx.key(), aipet_sprite::compose::fx_top(&p.pack, &p.sheets, fx, false).unwrap_or(f64::NAN));
        }
        println!();
    }
}
