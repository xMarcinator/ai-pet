# Rive

Rive packs are not supported. The app has no Rive runtime: the spike (`rust/proofs/rive.md`) found that rendering
Rive on the CPU costs too much for an always-on pet, and that damaged files could crash or hang the runtime, so the
work moved to its own spec, parked.

- Don't build a Rive pack, and don't offer Rive as a kind. Offer pixel, painted or vector.
- A manifest with `"kind": "rive"` fails with `unknown kind "rive"`.
- If the user has a Rive file, take its art as a reference and redraw it as a vector pack (`vector.md`), with the
  state machine's states as channel values (`channels.md`).
- The design kept for later (the state machine and inputs a pet needs, and the manifest's input mapping) is in
  `.flow/specs/fnt-67-rive-packs.md`.
