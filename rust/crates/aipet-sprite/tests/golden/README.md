# Golden frames: frozen

`frames.json` is the C# pet's own frames (AiPet.Core's Pet and Avatar), which `tests/golden.rs` replays: every pixel,
PixelsChanged, AnimState and idle act, and the doubles bit for bit. The golden generator that wrote it (its `sprite`
mode), and the C# it ran, were removed at the cutover to Rust (spec fn-1-migrate-aipet-from-net-to-rust, task 25). It
can't be written again: it is the contract as it stands, and nothing edits it by hand. The avatar files it recorded
from the repository can't change either.

The C# is at commit `bf3696ea4a` (`git show bf3696ea4a:rust/golden/Sprite.cs`; the C# projects are under `src/`
there). The file's `about` still says how it was written then.
