# avatars

Art and examples for AiPet's pets. The app doesn't read this folder: your own pets go in the data folder's
`avatars/` (Settings → Avatars → **Open folder**). [docs/MAKING-A-PET.md](../docs/MAKING-A-PET.md) explains how to
make one.

| Path | What |
|---|---|
| `hood-green.json` | A legacy avatar file, in the format of 0.1.0: the green Hood as ellipses and a palette. It still loads, and is now also the built-in Hood's Green theme. |
| `samples/basic/` | A small pixel pack: a body and eyes on PNG sheets, with explicit frames. |
| `samples/grid/` | The same pet, with its sheets described as grids. |
| `samples/native/` | The same pet from a native `sample.aseprite` file, with a `pack.json` for the rest. |
| `samples/themes/hood-red.json` | A theme file: Hood (vector) in red. |
| `samples/skill-example/` | Dewdrop, a small vector pack with a palette, a second theme, part poses, the eyes' look and fallbacks for every state: the worked example of the agent skill (`.claude/skills/aipet-avatar-pack/`). `samples/skill_example_gen.py` writes it and its review page, `samples/skill-example-preview.html`. |
| `reference/` | The built-in pets' reference drawings and the scripts that write their packs: `hood_gen.py`, `hamster_gen.py` and `sprout_gen.py` (with `vector_pack.py`), `pip_gen.py`, `hamster_pixel_gen.py`, and `pet_states.py`, the states every pet draws. Each `*-preview.html` shows a pet's every state. |

A pet can come in several styles (pixel, vector, …): put each style's pack folder in a folder named for the pet,
`avatars/<Pet>/<style>/`, and the Avatars page shows the pet once with a Style choice. See
[docs/MAKING-A-PET.md](../docs/MAKING-A-PET.md#pets-and-styles).

To try a sample, check it, then copy it into your avatars folder and click **Reload** on the Avatars page:

```sh
AiPet --check-pack avatars/samples/basic
cp -r avatars/samples/basic ~/.local/share/AiPet/avatars/sample
```

On Windows the avatars folder is `%LOCALAPPDATA%\AiPet\avatars`. A theme file goes in the avatars folder's `themes/`
folder instead:

```sh
mkdir -p ~/.local/share/AiPet/avatars/themes
cp avatars/samples/themes/hood-red.json ~/.local/share/AiPet/avatars/themes/
```

Packs are read by the Rust build of AiPet; the .NET app reads only avatar `.json` files.

## Credits

The Hamster's design, and the ideas for its newer states, come from JLandbo's [hamster-pet](https://github.com/JLandbo/hamster-pet), used with its author's permission.
