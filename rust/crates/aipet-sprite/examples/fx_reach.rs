//! Prints how high each built-in pack's effects reach above its art box, lifted by their moods, and how long that
//! takes to measure (fn-2.21): `cargo run --release -p aipet-sprite --example fx_reach`.
fn main() {
    for p in aipet_sprite::load::all(std::path::Path::new("/nonexistent")).pets {
        let at = std::time::Instant::now();
        let reach = aipet_sprite::fx_reach(&p.pack, &p.sheets);
        println!("{:<18} {:>7.1} px  ({:?})", p.pack.name, reach, at.elapsed());
    }
}
