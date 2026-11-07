//! The pixel pet without a UI: avatars ([`Avatar`]), the animation state machine and the renderer ([`Pet`]).
//!
//! A port of `src/AiPet.Core/Pet.cs` and `src/AiPet.Core/Avatar.cs`. The pixels match the C# exactly; the golden
//! tests (`tests/golden.rs`) replay frames that the C# rendered (`rust/golden`) and compare every pixel.
//!
//! Each frame the UI passes a [`PetInput`] and the time to [`Pet::update`], and gets a [`PetFrame`] back: three
//! 26×24 layers of premultiplied `0xAARRGGBB` pixels (body, glow, fx), plus the motion to apply to them (offset,
//! squash and stretch, shadow). The glow layer gets a coloured blur on top ([`Avatar::glow`]).

mod avatar;
mod pet;

pub use avatar::{Avatar, AvatarJson, drawable, read_all_text, try_parse_color};
pub use pet::{Cell, GH, GW, P, PIXELS, Pet, PetFrame, PetInput, PetRng, XorShift64, pm};
