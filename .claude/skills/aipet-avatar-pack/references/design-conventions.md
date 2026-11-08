# The owner's design conventions

These are the owner's decisions for every AiPet pet. Treat them as fixed unless the user says otherwise for one pet,
and list them as fixed in the Codex review.

## Style

- **Cartoon**: simple, round, friendly shapes; a big head or face, small limbs, expressive eyes. Not realistic.
- **Thick outline**: one dark outline round the silhouette and each separate part (hands, feet, props), about 2 to
  4 logical px in the 130 × 120 box (4 to 8 in a generator's 260 × 250 drawing; the built-ins use 2 to 2.5 for the
  body and 3.5 round effects). Darker than every fill; the palette's `outline` (or `line`, `edgeDark`) colour, not
  black, unless the pet is black.
- **Flat cel shading**: flat fills with one or two hard-edged shade bands (a darker `shade`, perhaps a `deep`), on
  the side away from the light (light from the upper left). No soft shading. A pet may have none: Sprout (vector) is
  one flat body colour with the outline as its only edge, by the owner's choice.
- **Rim light**: a thin, light stroke or band just inside the outline along one edge (`rim`, `hi`, `rimMid`; the
  Hood has it down the cloak's side), and perhaps a small specular highlight (`spec`). It separates the pet from
  dark backgrounds.
- **Vector packs**: no blur, filters, masks, clip paths or gradients (the format has none, and the generators'
  SVG must not use them either: the reference drawings are "canvas-ready"). Plain paths, rects, circles and
  ellipses with flat fills, strokes and opacity.
- Each expression and pose is drawn, never a scaled copy of another.
- The eyes carry the character: they must read at the smallest size; give them a glint, and keep `look` offsets
  small enough that they stay on the face.

## Readability

- At app size: the pet is drawn in a 130 × 120 box at Pet size 100 %, and users can go down to 75 % (Settings →
  General → Pet size; Bubble size scales the chat bubbles only). Review at 130 × 120, and look at the smallest size
  too: details under 2 px vanish.
- On **white**, **mid-grey** (`#808080`) and **near-black** (`#101214`): the review page shows every state on all
  three. The outline must hold on white, the rim light on near-black, and neither fill nor outline may disappear on
  mid-grey. Check every theme, not only the default.
- Effects (Zs, dots, notes, the alert, sparkles, dust) keep their fixed colours from `pet_states.py` (the notes take
  the accent), each with the pet's outline.

## The pet's own shape

- Fit head-anchored things to the head, not the body. The **Hamster** is drawn in a three-quarter view: its head is
  off-centre and narrower than its body, turned slightly left. Its headset band and cups, the eyes' look offsets, and
  the props held in front (laptop, magnifier) follow the head's own centre, width and turn. Don't reuse the
  front-facing pets' coordinates (Sprout, Hood) on a turned pet; check this first on any pet that isn't drawn
  straight on.
- Keep the pet's ground on the anchor, and its feet (or robe) on the ground in `idle`. The shadow is the app's: don't
  draw one in the pack.
- Draw facing right; mirroring makes the left. Put anything that must not mirror (text) on a layer with
  `"mirror": "none"`, or avoid it.
- Keep a pet recognisable from its original: the user's reference shows the proportions, colours and the turn;
  the review page shows the original beside the new drawing so drift is caught.
