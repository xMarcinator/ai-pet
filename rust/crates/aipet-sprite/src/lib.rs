//! The pixel pet without a UI: avatars ([`Avatar`]), the animation state machine and the renderer ([`Pet`]), and
//! the pack model it is moving to: the renderer-neutral [`Pose`] ([`pose`]), packs ([`pack`]) and the compositor
//! ([`compose`]).
//!
//! A port of `src/AiPet.Core/Pet.cs` and `src/AiPet.Core/Avatar.cs`. The pixels match the C# exactly; the golden
//! tests (`tests/golden.rs`) replay frames that the C# rendered (`rust/golden`) and compare every pixel.
//!
//! Each frame the UI passes a [`PetInput`] and the time to [`Pet::update`], and gets a [`PetFrame`] back: three
//! 26×24 layers of premultiplied `0xAARRGGBB` pixels (body, glow, fx), plus the motion to apply to them (offset,
//! squash and stretch, shadow). The glow layer gets a coloured blur on top ([`Avatar::glow`]). The same update also
//! computes the frame's [`Pose`] ([`Pet::pose`]), which packs draw from.

mod aseprite_file;
mod avatar;
pub mod bake;
mod builtin;
pub mod compose;
pub mod load;
pub mod manifest;
pub mod pack;
mod pet;
pub mod pose;

pub use avatar::{Avatar, AvatarJson, drawable, read_all_text, try_parse_color};
pub use pet::{Cell, GH, GW, P, PIXELS, Pet, PetFrame, PetInput, PetRng, XorShift64, pm};
pub use pose::Pose;
