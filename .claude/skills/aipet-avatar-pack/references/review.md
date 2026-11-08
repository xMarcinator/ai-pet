# Review: the review page, the state sheet and the Codex review

`--check-pack` only says the pack loads. Whether it looks right is decided by the owner, on the review page, and
helped by a second opinion from Codex.

## The review page and the state sheet

Every generator writes both (`avatars/reference/pet_states.write_pages`):

- `<prefix>-preview.html`: the user's original (pixel art, sketch or image) beside the new drawing at comparable
  height on near-black, then every state of `pet_states.STATES` and `PHASES` in a 130 × 120 logical px box (the art
  box, dashed), on white, mid-grey (`#808080`) and near-black (`#101214`), grouped as moods, idle acts, reactions,
  the mirrored view and the second frames.
- `<prefix>-sheet.svg`: every state as a labelled frame, six to a row, plus any extra frames (`sheet_extras`, such
  as typing frames).

Examples: `avatars/reference/hood-green-preview.html`, `hamster-preview.html`, `sprout-preview.html` and their
`-sheet.svg`. A generator that writes the manifest directly draws its page from the manifest it wrote, through a
small resolver that follows the fallbacks as the app does: `avatars/samples/skill_example_gen.py` and its
`skill-example-preview.html`. Its page has no original to compare with; add one when the user gave a reference.

The page shows the first frame of each state without the app's whole-body motion; animation is checked in the app.

**Screenshots** of the page, for the user and for Codex (Chrome is installed; use the browser tools if they are
available instead):

```sh
google-chrome-stable --headless=new --disable-gpu --hide-scrollbars --window-size=760,2200 \
  --screenshot=/tmp/<scratch>/review-1.png file://$PWD/avatars/reference/<prefix>-preview.html
rsvg-convert avatars/reference/<prefix>-sheet.svg -o /tmp/<scratch>/sheet.png
```

Raise the window height to take the whole page, or take it in parts; keep each image readable (a few thousand px
tall at most). Write screenshots to the scratchpad, never into the repository.

**In the app.** Before calling a pet done, wear it: copy the pack into the avatars folder, **Reload**, click its
tile, and watch it idle, sleep, think, work, get poked, dragged and held. The app is the truth for motion,
transitions and the glow.

## What to look for

Go through `design-conventions.md`, then:

- Every state reads: the eyes and the expression at a glance, on all three backgrounds, in every theme.
- Nothing pokes out of the dashed box, and the feet stand on the ground in `idle`.
- Fallbacks do what was meant: no default drawn where `null` was meant (a laptop while idle, Zs while awake).
- Parts line up in every pose: hands on the laptop and the magnifier handle, the headset on the head (on a turned
  pet, on the head's own centre), eyes on the face at the far looks.
- The mirrored view still looks right (asymmetric details, a turned head).
- The new drawing is recognisably the original: proportions, colours, the turn.

## Iterating with the user

Show the screenshots and ask for notes. Keep a short list of the owner's decisions as they come ("the outline stays
brown, not black", "no shade bands on the body", "the ears stay behind the head"): these go to Codex as fixed, and
into the generator's docstring or comments so the next agent keeps them. Change the generator, never the generated
pack; run it again; re-shoot the page.

## The Codex review

A second, independent read of the drawing, run read-only, so Codex can look but not change anything. `codex exec`
is installed (`codex exec --help`). Images go with `-i` (repeat it for each image); give the prompt on stdin, since a
prompt after `-i` would be read as another image.

```sh
cat > /tmp/<scratch>/codex-review.md <<'EOF'
You are reviewing the drawing of an AiPet pet, <name>, for its owner. The screenshots show its review page: every
state in a 130 × 120 box on white, mid-grey and near-black, and the original it is drawn from. The pack is
<path>/pack.json, written by <generator>; docs/MAKING-A-PET.md describes the format.

These are the owner's decisions and are fixed; do not suggest changing them:
- cartoon style, thick outline, flat cel shading, rim light; no blur, filters, masks or gradients
- <each decision from the iteration, one per line>

Review only: list concrete problems you can see, each with the state, the background and what is wrong (for example
"done, near-black: the right hand's outline disappears"), most important first. Say which ones you are unsure about.
Don't edit any file.
EOF
codex exec --sandbox read-only --ephemeral -C "$PWD" \
  -i /tmp/<scratch>/review-1.png -i /tmp/<scratch>/review-2.png -i /tmp/<scratch>/sheet.png \
  -o /tmp/<scratch>/codex-review.txt < /tmp/<scratch>/codex-review.md
```

- `--sandbox read-only`: Codex may read the repository but not write. `--ephemeral` keeps no session files. `-o`
  saves its last message.
- Bring its findings to the user as a list, with your own view of each. Apply only what the user agrees with; a
  finding that contradicts an owner's decision is dropped, not argued.
- It takes a few minutes. If `codex` is missing or fails (not signed in, no network), say so and go on without it;
  don't install or sign in on the user's behalf.
