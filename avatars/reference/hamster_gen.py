"""Generates avatars/reference/hamster.svg (neutral pose, canvas-ready geometry) and
avatars/reference/hamster-preview.html (reference comparison, 130x120 boxes on three backgrounds, poses and
expressions) from one description of the Hamster, in the same layered system as the green Hood.

The shapes are traced from the pixel reference (avatars/reference/hamster.png, 5 px cells): coordinates below are
written in reference cells (column, row) and mapped to the 260x250 canvas, so the vector keeps the original's
proportions, its slight turn to the left, its colours and its soft brown outline.

Canvas-ready: plain paths, rects, circles and ellipses with flat fills and opacity; no filters, masks, clip paths,
gradients or <use>. Run from anywhere: python avatars/reference/hamster_gen.py
"""
import base64
import os
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
OUT = os.path.join(ROOT, "avatars", "reference")

C = {  # sampled from the reference
    "yellow": "#F5CD75", "orange": "#F4B236", "rust": "#D89163", "hi": "#F8EAB6",
    "cream": "#F2EBD9", "creamShade": "#E0D3B8", "line": "#A5967A",
    "edgeLight": "#D89163", "edgeDark": "#90664C", "eye": "#A5663A", "dark": "#3B3A3A",
}

S = 6.0  # canvas units per reference cell


def X(c):
    return round(130 + (c - 22.5) * S, 1)


def Y(r):
    return round(44 + (r - 8) * S, 1)


def P(d):
    """A path written in reference cells: every 'c,r' pair becomes canvas 'x y'."""
    return re.sub(r"(-?\d+(?:\.\d+)?),(-?\d+(?:\.\d+)?)", lambda m: f"{X(float(m[1]))} {Y(float(m[2]))}", d)


# body and head as one blob (the ears sit behind it)
BODY = P("M12,13 C14,11 24,11 28,13 C33,13.4 37,16.4 37.4,21 C37.7,25.5 37.4,29.6 36,32 "
         "C34.6,34 32.6,34.3 30,34.4 C25,34.7 20,34.7 16,34.3 C13,34 11,31.4 10,29.4 C8.4,27 8,24.6 8,22 "
         "C8,19.4 9.4,14.6 12,13 Z")
EDGE_RIGHT = P("M28,13 C33,13.4 37,16.4 37.4,21 C37.7,25.5 37.4,29.6 36,32 C34.6,34 32.6,34.3 30,34.4")
EDGE_BOTTOM = P("M30,34.4 C25,34.7 20,34.7 16,34.3 C13,34 11,31.4 10,29.4")
# the back is the head's yellow; orange is only the shadow along its edge (light from the upper left)
ORANGE_R = P("M30.5,14 C34,15 37,17.4 37.2,21 C37.4,23.5 37.3,25.5 37,27.2 C36.2,27.2 35.6,26.9 35.2,26.4 "
             "C35.4,24.6 35.5,22.6 35.2,20.6 C34.8,18 33.2,15.6 30.5,14 Z")
RUST_R = P("M34.6,17.6 C36.4,19 37.2,21 37.2,23 C37.2,24.6 37.1,26 36.8,27 C35.8,26.4 35.4,25 35.4,23.2 "
           "C35.4,21 35.2,19.2 34.6,17.6 Z")
ORANGE_L = P("M8.5,18.6 C9.5,18.4 11,18.6 12,19.2 C11,20.2 10,20.8 8.5,21 Z")
HEAD_HI = P("M9.6,21 C9.2,17 10.2,14.6 12.4,13.4 M17,12.2 C18.5,11.8 20,11.8 21.5,12.2")
CHEEK_HI = P("M30.2,15.4 C31,15.2 31.8,15.4 32.4,15.9")
# the cream belly and muzzle, its shaded lower half, and the cream haunch at the bottom right
BELLY = P("M14,17.6 C16,16.6 19,16.6 21,17.4 C25,17.4 28,17.8 30,19 C31,20.8 31,23.4 31.4,25.6 "
          "C33.6,26 36.6,26.8 37.3,28.8 C37.5,30 36.9,31.2 36,32 C34.6,34 32.6,34.3 30,34.4 "
          "C25,34.7 20,34.7 16,34.3 C13,34 11,31.4 10,29.4 C8.6,27 8.6,24 9,22 C10,20 12,18.4 14,17.6 Z")
BELLY_SHADE = P("M8.8,23 C15,23.6 24,23.6 30.8,23.2 C31,24.4 31.1,25 31.4,25.6 C31.2,28.6 30.4,31.6 29.2,34.4 "
                "C25,34.7 20,34.7 16,34.3 C13,34 11,31.4 10,29.4 C8.7,27 8.6,25 8.8,23 Z")
HAUNCH = P("M30.4,27.4 C32.4,26.2 35.6,26.6 36.8,28.6 C37.4,30.2 36.9,31.4 36,32.2 C34.6,34 32.6,34.3 30,34.4 "
           "C29.2,32 29.4,29.2 30.4,27.4 Z")
BELLY_LINE = P("M15,33.6 C20,33.9 25,33.9 29,33.6")
HAUNCH_LINE = P("M30.6,27.6 C29.6,29.4 29.4,31.8 29.8,34")

EARS = [(13, 11.2, 4.1, 3.3), (25.3, 11.2, 4.4, 3.4)]  # centre col, row, radius in cells
EYE_L, EYE_R = (X(14), Y(16)), (X(23), Y(16))
NOSE = (X(17.5), Y(16.5))
SHOULDER_L, SHOULDER_R = (X(14), Y(22.4)), (X(26), Y(22.4))  # arm pivots; screen sides, unmirrored
FEET = [(18.5, 34.8), (26.2, 33.8), (32.6, 34.8)]


def el(tag, **a):
    attrs = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in a.items())
    return f"<{tag} {attrs}/>"


def eye(name, origin, expr="open", look=(0, 0)):
    x, y = origin[0] + look[0], origin[1] + look[1]
    e, p = C["eye"], []
    if expr == "closed":
        p.append(el("path", d="M-7 2 C-4 5 4 5 7 2", fill="none", stroke=e, stroke_width=3.5, stroke_linecap="round"))
    elif expr == "happy":
        p.append(el("path", d="M-7 3 C-4 -4 4 -4 7 3", fill="none", stroke=e, stroke_width=3.5,
                    stroke_linecap="round"))
    elif expr == "squint":
        p.append(el("rect", x=-7, y=-2, width=14, height=4, rx=2, fill=e))
    else:
        s = 18 if expr == "wide" else 12  # the reference's 2x2-cell eye
        p.append(el("rect", x=-s / 2, y=-s / 2, width=s, height=s, rx=4 if expr == "wide" else 3, fill=e))
    return f'<g id="{name}" transform="translate({x} {y})">' + "".join(p) + "</g>"


def arm(name, pivot, angle=0, cls="", lift=(0, 0)):
    """An arm hangs from its shoulder pivot: a cream limb outlined in grey-brown, ending in a small brown paw."""
    x, y = pivot[0] + lift[0], pivot[1] + lift[1]
    c = f' class="{cls}"' if cls else ""
    rx, ry = (8, 6) if lift != (0, 0) else (6.5, 5)  # a lifted paw is a little bigger, so the gesture reads
    limb = "M-16 0 C-16 12 -10 22 -2 26 C6 22 14 12 16 0"
    return (f'<g id="{name}" transform="translate({x} {y}) rotate({angle})"><g{c}>'
            + el("path", d=limb + " Z", fill=C["cream"])
            + el("path", d=limb, fill="none", stroke=C["line"], stroke_width=2.5, stroke_linecap="round")
            + el("ellipse", cx=-2, cy=26, rx=rx, ry=ry, fill=C["rust"], stroke=C["edgeDark"], stroke_width=2)
            + "</g></g>")


def laptop():
    return ('<g id="prop">'
            + el("rect", x=94, y=150, width=72, height=44, rx=5, fill="#3A3A43", stroke=C["dark"], stroke_width=3.5)
            + el("rect", x=100, y=156, width=60, height=31, rx=2, fill="#1E2236")
            + el("rect", x=106, y=163, width=28, height=3, rx=1.5, fill="#7FEFFF", opacity="0.8")
            + el("rect", x=106, y=170, width=42, height=3, rx=1.5, fill="#7FEFFF", opacity="0.5")
            + el("rect", x=106, y=177, width=20, height=3, rx=1.5, fill="#7FEFFF", opacity="0.5")
            + "</g>")


def hamster(expr="open", look=(0, 0), arms="rest", prop=False, mirror=False, standalone=False):
    o = ['<g id="shadow">'
         + el("ellipse", cx=X(23), cy=Y(36.2), rx=86, ry=7, fill="#000", opacity="0.10")
         + el("ellipse", cx=X(23), cy=Y(36.2), rx=70, ry=5.5, fill="#000", opacity="0.12")
         + el("ellipse", cx=X(23), cy=Y(36.2), rx=52, ry=4, fill="#000", opacity="0.16") + "</g>"]
    fig = []
    ears = ""
    for c, r, rx, ry in EARS:
        ears += (el("ellipse", cx=X(c), cy=Y(r), rx=rx * S, ry=ry * S, fill=C["yellow"], stroke=C["edgeLight"],
                    stroke_width=4)
                 + el("path", d=P(f"M{c - rx + 0.8},{r + 0.6} C{c - rx + 0.8},{r - ry + 1} {c},{r - ry + 0.7} "
                                  f"{c + rx - 1.2},{r - ry + 1.2}"),
                      fill="none", stroke=C["hi"], stroke_width=4, stroke_linecap="round")
                 + el("ellipse", cx=X(c), cy=Y(r + 0.3), rx=(rx - 1.6) * S, ry=(ry - 1.3) * S, fill=C["orange"])
                 + el("ellipse", cx=X(c), cy=Y(r + 1), rx=(rx - 2.4) * S, ry=(ry - 2.1) * S, fill=C["rust"]))
    fig.append('<g id="ears">' + ears + "</g>")
    feet = ""
    for c, r in FEET:
        feet += (el("rect", x=X(c) - 6, y=Y(r) - 6, width=12, height=14, rx=4, fill=C["rust"], stroke=C["edgeDark"],
                    stroke_width=2.5)
                 + el("rect", x=X(c) - 5, y=Y(r) + 4, width=10, height=4, rx=2, fill=C["dark"]))
    fig.append('<g id="feet">' + feet + "</g>")
    fig.append('<g id="body">'
               + el("path", d=BODY, fill=C["yellow"])
               + el("path", d=ORANGE_R, fill=C["orange"])
               + el("path", d=ORANGE_L, fill=C["orange"])
               + el("path", d=HEAD_HI, fill="none", stroke=C["hi"], stroke_width=4, stroke_linecap="round")
               + el("path", d=CHEEK_HI, fill="none", stroke=C["hi"], stroke_width=4, stroke_linecap="round")
               + el("path", d=BELLY, fill=C["cream"])
               + el("path", d=BELLY_SHADE, fill=C["creamShade"])
               + el("path", d=HAUNCH, fill=C["cream"])
               + el("path", d=BELLY_LINE, fill="none", stroke=C["line"], stroke_width=2.5, stroke_linecap="round")
               + el("path", d=HAUNCH_LINE, fill="none", stroke=C["line"], stroke_width=2.5, stroke_linecap="round")
               + el("path", d=BODY, fill="none", stroke=C["edgeLight"], stroke_width=4, stroke_linejoin="round")
               + el("path", d=EDGE_RIGHT, fill="none", stroke=C["edgeDark"], stroke_width=4, stroke_linecap="round")
               + el("path", d=EDGE_BOTTOM, fill="none", stroke=C["line"], stroke_width=4, stroke_linecap="round")
               + "</g>")
    nx, ny = NOSE
    fig.append('<g id="face">'
               + el("rect", x=nx - 9, y=ny - 3, width=18, height=6, rx=3, fill=C["rust"])
               + el("path", d=f"M{nx - 6} {ny + 12} L{nx} {ny + 6} L{nx + 6} {ny + 12}", fill="none",
                    stroke=C["rust"], stroke_width=3, stroke_linecap="round", stroke_linejoin="round")
               + "</g>")
    fig.append('<g id="eyes">' + eye("eye-left", EYE_L, expr, look) + eye("eye-right", EYE_R, expr, look) + "</g>")
    al, ar, ll, lr = 0, 0, (0, 0), (0, 0)
    if arms == "wave":
        ar, lr = -140, (16, -8)
    elif arms == "raised":
        al, ar, ll, lr = 140, -140, (-16, -8), (16, -8)
    elif arms == "hold":
        al, ar = 12, -12
    if prop:
        fig.append(laptop())
    cl = ("bob-l", "bob-r") if standalone else ("", "")
    fig.append('<g id="arms">' + arm("arm-left", SHOULDER_L, al, cl[0], ll) + arm("arm-right", SHOULDER_R, ar, cl[1], lr)
               + "</g>")
    cls = ' class="breathe"' if standalone else ""
    o.append(f'<g id="figure"{cls}>' + "".join(fig) + "</g>")
    body = "".join(o)
    if mirror:
        body = f'<g transform="translate(260 0) scale(-1 1)">{body}</g>'
    return body


HEADER = """<!-- Hamster: a vector trace of the pixel hamster (avatars/reference/hamster.png): a round orange-and-cream body
     turned slightly to the left, big round ears, small brown square eyes, a brown nose and mouth, cream arms meeting
     in small paws on the belly, thin feet, and a soft brown outline (lighter on the lit left, darker on the right).
     Same layered system as the green Hood, with plain shapes only (no filters, masks, clip paths, gradients or <use>).
     Groups map to canvas parts: shadow, ears, feet, body, face, eyes (eye-left, eye-right, translated to their
     centres), arms (arm-left, arm-right, translated to their shoulder pivots and rotated for poses; left/right are
     screen sides in this unmirrored drawing; mirror around x=130). Draw order: shadow, ears, feet, body, face, eyes,
     prop, arms. The CSS is only a preview. -->"""

STYLE = """<style>
    .breathe { transform-origin: 130px 212px; animation: breathe 2.6s ease-in-out infinite; }
    @keyframes breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.015, 0.985); } }
    .bob-l, .bob-r { animation: bob 2.6s ease-in-out infinite; }
    .bob-r { animation-delay: -1.3s; }
    @keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-1.5px); } }
    @media (prefers-reduced-motion: reduce) { .breathe, .bob-l, .bob-r { animation: none; } }
  </style>"""


def svg(content, w=520, h=500):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 250" width="{w}" height="{h}">{content}</svg>'


def main():
    with open(os.path.join(OUT, "hamster.svg"), "w", encoding="utf-8", newline="\n") as f:
        f.write(svg("\n  <title>Hamster</title>\n  " + HEADER + "\n  " + STYLE + "\n  "
                    + hamster(standalone=True) + "\n") + "\n")
    states = [
        ("neutral", {}), ("look up-right", {"look": (4, -3)}), ("look down-left", {"look": (-4, 3)}),
        ("blink (closed)", {"expr": "closed"}), ("happy", {"expr": "happy"}), ("wide", {"expr": "wide"}),
        ("squint", {"expr": "squint"}), ("waving", {"arms": "wave"}), ("paws raised", {"arms": "raised", "expr": "wide"}),
        ("holding laptop", {"arms": "hold", "prop": True, "look": (0, 3)}),
        ("mirrored (look right→left)", {"look": (4, 0), "mirror": True}),
    ]
    bgs = [("white", "#ffffff"), ("mid-grey", "#808080"), ("near-black", "#101214")]
    rows = "".join(
        f"<tr><th>{n}</th>" + "".join(
            f'<td><div class="box" style="background:{bg}">{svg(hamster(**s), 130, 125)}</div></td>' for _, bg in bgs)
        + "</tr>" for n, s in states)
    with open(os.path.join(OUT, "hamster.png"), "rb") as f:
        ref = "data:image/png;base64," + base64.b64encode(f.read()).decode()
    html = f"""<!doctype html>
<html><head><meta charset="utf-8"><title>Hamster review</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font: 14px system-ui, sans-serif; margin: 16px; background: #f4f4f4; color: #111; }}
  h2 {{ margin: 24px 0 8px; font-size: 16px; }}
  .cmp {{ display: flex; gap: 24px; align-items: flex-end; }}
  .cmp figure {{ margin: 0; text-align: center; }}
  .cmp .stage {{ background: #101214; padding: 12px; display: flex; align-items: flex-end; justify-content: center;
                height: 240px; width: 260px; }}
  .ref {{ image-rendering: pixelated; height: 216px; }}
  table {{ border-collapse: collapse; }}
  th {{ text-align: right; padding: 4px 10px; font-weight: 500; white-space: nowrap; }}
  td {{ padding: 4px; }}
  .box {{ width: 130px; height: 120px; overflow: hidden; outline: 1px dashed #bbb; }}
  .box svg {{ display: block; width: 130px; height: 125px; margin-top: -2.5px; }}
</style></head><body>
<h2>Pixel reference vs vector, at comparable character height</h2>
<div class="cmp">
  <figure><div class="stage"><img class="ref" src="{ref}" alt="hamster reference"></div>
    <figcaption>reference</figcaption></figure>
  <figure><div class="stage">{svg(hamster(), 228, 219)}</div><figcaption>vector Hamster</figcaption></figure>
</div>
<h2>App size: 130×120 logical px boxes (dashed), on white, mid-grey and near-black</h2>
<table><tr><th></th>{"".join(f"<th style='text-align:left'>{n}</th>" for n, _ in bgs)}</tr>
{rows}
</table>
</body></html>
"""
    with open(os.path.join(OUT, "hamster-preview.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(html)


if __name__ == "__main__":
    main()
