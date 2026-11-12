"""Writes avatars/reference/aura-prototype.html: the golden aura that would open every done (fnt-72, R3 and R12),
for the owner to approve before any pet draws it. Each variant is shown on the vector Hood (green) beside today's
done (sparkles), animated (the aura plays once, about 1.8 s, then today's sparkles) and as its first frame (what
reduced motion shows: the aura fully built), on light, dark and desktop backgrounds.

The aura is drawn art in fixed gold (themes never change it), with plain shapes only, so it can become pack layers:
a part behind the body and a part in front. The CSS animation is only a preview; the app drives the motion.
Run from anywhere: python avatars/reference/aura_prototype.py
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hood_gen  # noqa: E402
import pet_states  # noqa: E402

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)))

GOLD, GOLD_HI, GOLD_DEEP = "#FFC83D", "#FFF1B0", "#E39B12"
DONE = {"expr": "happy", "arms": "wave", "dy": -16}

# Where each pet's body sits on the 260 x 250 canvas (centre and half sizes of the aura), so the aura hugs it.
PETS = [("Hood (green)", hood_gen, (130, 132, 96, 104))]


def el(tag, **a):
    return pet_states.el(tag, **a)


def star(x, y, s, fill=GOLD_HI):
    d = (f"M{x} {y - 9 * s} Q{x + 1.5 * s} {y - 1.5 * s} {x + 9 * s} {y} Q{x + 1.5 * s} {y + 1.5 * s} {x} {y + 9 * s}"
         f" Q{x - 1.5 * s} {y + 1.5 * s} {x - 9 * s} {y} Q{x - 1.5 * s} {y - 1.5 * s} {x} {y - 9 * s} Z")
    return el("path", d=d, fill=fill, stroke=GOLD_DEEP, stroke_width=1.5, stroke_linejoin="round")


def halo(box):
    """A: a bright golden outline ring around the whole pet, with four small stars on it."""
    cx, cy, rx, ry = box
    back = (el("ellipse", cx=cx, cy=cy, rx=rx + 6, ry=ry + 6, fill=GOLD, opacity="0.10")
            + el("ellipse", cx=cx, cy=cy, rx=rx, ry=ry, fill="none", stroke=GOLD, stroke_width=7, opacity="0.35"))
    front = el("ellipse", cx=cx, cy=cy, rx=rx, ry=ry, fill="none", stroke=GOLD_HI, stroke_width=2.5, opacity="0.9")
    for a in (35, 145, 215, 325):
        t = math.radians(a)
        front += star(round(cx + rx * math.cos(t), 1), round(cy - ry * math.sin(t), 1), 0.9)
    return back, front


def glow(box):
    """B: a soft golden glow behind the pet, three stacked ellipses, no outline; stars above the head."""
    cx, cy, rx, ry = box
    back = "".join(el("ellipse", cx=cx, cy=cy - 4, rx=rx + d, ry=ry + d, fill=GOLD, opacity=op)
                   for d, op in ((22, "0.10"), (12, "0.16"), (2, "0.24")))
    front = star(cx - 52, cy - ry + 4, 0.8) + star(cx + 58, cy - ry - 6, 1.1) + star(cx + 8, cy - ry - 22, 0.7)
    return back, front


def rays(box):
    """C: short golden rays around the pet, like a sunburst, over a faint glow."""
    cx, cy, rx, ry = box
    back = el("ellipse", cx=cx, cy=cy, rx=rx - 4, ry=ry - 4, fill=GOLD, opacity="0.14")
    out = []
    for i in range(16):
        t = math.radians(i * 22.5 + 11.25)
        inner, outer = (rx + 6, ry + 6), (rx + (24 if i % 2 else 14), ry + (24 if i % 2 else 14))
        x1, y1 = cx + inner[0] * math.cos(t), cy - inner[1] * math.sin(t)
        x2, y2 = cx + outer[0] * math.cos(t), cy - outer[1] * math.sin(t)
        out.append(el("path", d=f"M{x1:.1f} {y1:.1f} L{x2:.1f} {y2:.1f}", stroke=GOLD, stroke_width=5,
                      stroke_linecap="round", opacity="0.85"))
        out.append(el("path", d=f"M{x1:.1f} {y1:.1f} L{x2:.1f} {y2:.1f}", stroke=GOLD_HI, stroke_width=2,
                      stroke_linecap="round"))
    return back, "".join(out)


VARIANTS = [("A: golden ring", halo), ("B: soft glow", glow), ("C: sunburst", rays)]


def frame(gen, box, aura, animated, today=False):
    """One done frame of a pet: today's (sparkles) when `today`, else with the aura, animated or as its first frame."""
    if today:
        return gen.frame({**DONE, "fx": "sparkles"})
    back, front = aura(box)
    cls = ' class="aura"' if animated else ""
    fig = (f'<g{cls} style="transform-origin:{box[0]}px {box[1]}px">{back}</g>' + gen.figure(DONE)
           + f'<g{cls} style="transform-origin:{box[0]}px {box[1]}px">{front}</g>')
    body = pet_states.compose(fig, gen.shadow(), gen.GROUND, DONE, gen.C.get("hi", GOLD), gen.C.get("outline", "#000"))
    if animated:
        # today's sparkles take over once the aura has played
        body += f'<g class="after">{pet_states.compose("", "", gen.GROUND, {**DONE, "fx": "sparkles"}, GOLD, "#3a2a00")}</g>'
    return body


def main():
    bgs = [("light", "#f6f6f4"), ("dark", "#101214"),
           ("desktop", "linear-gradient(160deg, #3f6e8c 0%, #2b4a63 55%, #7a5a6e 100%)")]
    svg = pet_states.svg
    rows = []
    for vname, aura in VARIANTS:
        rows.append(f'<tr><td colspan="7" class="grp">{vname}</td></tr>')
        for pname, gen, box in PETS:
            cells = []
            for _, bg in bgs:
                cells.append(f'<td><div class="box" style="background:{bg}">{svg(frame(gen, box, aura, True), 150, 145)}</div></td>')
            cells.append(f'<td><div class="box" style="background:{bgs[1][1]}">{svg(frame(gen, box, aura, False), 150, 145)}</div></td>')
            cells.append(f'<td><div class="box" style="background:{bgs[1][1]}">{svg(frame(gen, box, aura, False, True), 150, 145)}</div></td>')
            rows.append(f"<tr><th>{pname}</th>" + "".join(cells) + "</tr>")
    html = f"""<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Aura prototype</title>
<style>
  body {{ font: 14px system-ui, sans-serif; margin: 16px; background: #f4f4f4; color: #111; }}
  p {{ max-width: 760px; }}
  .wrap {{ overflow-x: auto; }}
  table {{ border-collapse: collapse; }}
  th {{ text-align: left; padding: 4px 10px 4px 0; font-weight: 600; white-space: nowrap; }}
  td {{ padding: 4px; }}
  thead th {{ font-weight: 500; color: #555; font-size: 12px; }}
  .grp {{ padding-top: 18px; font-size: 16px; font-weight: 700; }}
  .box {{ width: 150px; height: 145px; border-radius: 8px; }}
  .aura {{ animation: aura 3.2s ease-out infinite; }}
  .after {{ opacity: 0; animation: after 3.2s steps(1, end) infinite; }}
  @keyframes aura {{ 0% {{ opacity: 0; transform: scale(.7); }} 18% {{ opacity: 1; transform: scale(1); }}
                    45% {{ opacity: 1; transform: scale(1.03); }} 56% {{ opacity: 0; transform: scale(1.08); }}
                    100% {{ opacity: 0; }} }}
  @keyframes after {{ 0% {{ opacity: 0; }} 56% {{ opacity: 1; }} 100% {{ opacity: 1; }} }}
  @media (prefers-reduced-motion: reduce) {{ .aura {{ animation: none; }} .after {{ animation: none; }} }}
</style></head><body>
<h1>The aura that opens every done</h1>
<p>Three ways to draw it, on the vector Hood (green). The animated columns loop for review:
the aura builds, holds and fades in about 1.8 s, then today's sparkles take over (in the pet it plays once per done).
"First frame" is what reduced motion shows. "Today" is done as it is now. Gold is fixed, so a theme never changes it.
Pick a variant, or say what to change.</p>
<div class="wrap"><table>
<thead><tr><th></th><th>animated, light</th><th>animated, dark</th><th>animated, desktop</th><th>first frame</th><th>today (sparkles)</th></tr></thead>
{chr(10).join(rows)}
</table></div>
</body></html>
"""
    with open(os.path.join(OUT, "aura-prototype.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(html)


if __name__ == "__main__":
    main()
