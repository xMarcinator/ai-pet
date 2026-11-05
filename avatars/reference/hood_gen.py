"""Generates avatars/reference/hood-green.svg (neutral pose, canvas-ready geometry) and
avatars/reference/hood-green-preview.html (pixel comparison, 130x120 boxes on three backgrounds, poses and
expressions) from one description of the green Hood.

Canvas-ready means: plain paths, rects, circles and ellipses with flat fills and opacity; no filters, masks, clip
paths, gradients or <use>. Run from anywhere: python avatars/reference/hood_gen.py
"""
import base64
import os
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
OUT = os.path.join(ROOT, "avatars", "reference")

C = {  # avatars/hood-green.json
    "outline": "#030805", "deep": "#061F0F", "shade": "#0A3118", "body": "#0F4422", "hi": "#2FD36B",
    "spec": "#A4FFC4", "screen": "#020604", "eye": "#5CFF9C", "eyeHi": "#E8FFF0", "eyeLo": "#22C45E",
    "glow": "#2BFF7A",
    # a step between body and hi, for the cloak's side rim (dark backgrounds)
    "rimMid": "#1C7A3E",
}

# One outer silhouette: a round, slightly low hood with upright sides, flowing into a broad, grounded cloak.
SILHOUETTE = ("M130 40 C166 40 190 62 192 100 L194 138 C204 146 210 158 212 174 C214 190 214 202 212 210 "
              "C190 218 166 220 130 220 C94 220 70 218 48 210 C46 202 46 190 48 174 C50 158 56 146 66 138 "
              "L68 100 C70 62 94 40 130 40 Z")
# The cloak below the shoulder line (darker); one garment, so the step is a soft curve, not a collar.
CLOAK = ("M64 145 C90 156 170 156 196 145 C206 152 210 162 212 174 C214 190 214 202 212 210 "
         "C190 218 166 220 130 220 C94 220 70 218 48 210 C46 202 46 190 48 174 C50 162 54 152 64 145 Z")
HEM = ("M48 202 C80 210 180 210 212 202 C213 206 213 209 212 210 C190 218 166 220 130 220 "
       "C94 220 70 218 48 210 C47 209 47 206 48 202 Z")
# Rim light as explicit bands just inside the edge (about 3.5 px stays visible past the 4 px outline).
HOOD_RIM = ("M66 138 L68 100 C70 62 94 40 130 40 C166 40 190 62 192 100 L194 138 "
            "L189 139 L186 100 C184 67 162 46 130 46 C98 46 76 67 74 100 L71 139 Z")
SHOULDER_RIM_L = ("M66 138 C56 146 50 158 48 174 C46 186 46 192 47 198 L52 198 C51 192 51 186 53 175 "
                  "C55 161 61 151 70 144 Z")
SHOULDER_RIM_R = ("M194 138 C204 146 210 158 212 174 C214 186 214 192 213 198 L208 198 C209 192 209 186 207 175 "
                  "C205 161 199 151 190 144 Z")
CROWN = "M121 51 C126 49 134 49 139 51"
# The face: a recess in the hood. A darker lip above it, then black.
FACE_LIP = ("M84 104 C85 80 105 64 130 64 C155 64 175 80 176 104 C170 86 152 74 130 74 C108 74 90 86 84 104 Z")
FACE = "M130 72 C155 72 174 88 174 110 C174 132 155 148 130 148 C105 148 86 132 86 110 C86 88 105 72 130 72 Z"

EYE_L, EYE_R = (111, 110), (149, 110)   # local origins (eye centres)
HAND_L, HAND_R = (70, 180), (190, 180)  # resting: in front of the robe; waving/raised float outside  # floating free beside the robe  # screen-left / screen-right in the unmirrored drawing


def el(tag, **a):
    attrs = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in a.items())
    return f"<{tag} {attrs}/>"


def eye(name, origin, expr="open", look=(0, 0), glow=True):
    x, y = origin[0] + look[0], origin[1] + look[1]
    parts = []
    g = C["glow"]
    if expr == "closed":
        if glow:
            parts.append(el("rect", x=-12, y=3, width=24, height=10, rx=5, fill=g, opacity="0.15"))
        parts.append(el("rect", x=-9, y=6, width=18, height=4, rx=2, fill=C["eye"]))
    elif expr == "happy":
        if glow:
            parts.append(el("path", d="M-10 6 C-8 -8 8 -8 10 6", fill="none", stroke=g, stroke_width=11,
                            stroke_linecap="round", opacity="0.15"))
        parts.append(el("path", d="M-8 5 C-6 -5 6 -5 8 5", fill="none", stroke=C["eye"], stroke_width=5,
                        stroke_linecap="round"))
    else:
        w, h, r = {"open": (18, 26, 7), "wide": (22, 34, 9), "squint": (18, 12, 5)}[expr]
        if glow:
            parts.append(el("rect", x=-w / 2 - 5, y=-h / 2 - 5, width=w + 10, height=h + 10, rx=r + 5, fill=g,
                            opacity="0.10"))
            parts.append(el("rect", x=-w / 2 - 2.5, y=-h / 2 - 2.5, width=w + 5, height=h + 5, rx=r + 2.5,
                            fill=g, opacity="0.18"))
        parts.append(el("rect", x=-w / 2, y=-h / 2, width=w, height=h, rx=r, fill=C["eye"]))
        lo = h / 2 - h / 3  # the lower third, darker
        parts.append(el("path", d=f"M{-w/2} {lo} L{w/2} {lo} L{w/2} {h/2 - r} "
                                  f"C{w/2} {h/2 - r/3} {w/2 - r/3} {h/2} {w/2 - r} {h/2} L{-w/2 + r} {h/2} "
                                  f"C{-w/2 + r/3} {h/2} {-w/2} {h/2 - r/3} {-w/2} {h/2 - r} Z",
                        fill=C["eyeLo"]))
        if expr != "squint":
            parts.append(el("rect", x=-w / 2 + 3, y=-h / 2 + 3, width=4, height=4, rx=1, fill=C["eyeHi"]))
    return f'<g id="{name}" transform="translate({x} {y})">' + "".join(parts) + "</g>"


def hand(name, pos, cls="", halo=False):
    x, y = pos
    rim = "M-5 -4 C-3 -7 2 -8 5 -6" if x < 130 else "M-5 -6 C-2 -8 3 -7 5 -4"
    c = f' class="{cls}"' if cls else ""
    h = el("circle", cx=0, cy=0, r=13, fill=C["glow"], opacity="0.10") if halo else ""
    return (f'<g id="{name}" transform="translate({x} {y})"><g{c}>' + h
            + el("circle", cx=0, cy=0, r=10, fill=C["body"], stroke=C["outline"], stroke_width=4)
            + el("path", d=rim, fill="none", stroke=C["hi"], stroke_width=3, stroke_linecap="round")
            + "</g></g>")


def sleeve(side, to):
    """A short dark sleeve from the cloak's side to a raised hand, so a moved hand stays attached."""
    sx, sy = (40, 176) if side == "l" else (220, 176)
    tx, ty = to
    mx = (sx + tx) / 2 + (-8 if side == "l" else 8)
    my = (sy + ty) / 2
    return el("path", d=f"M{sx} {sy} Q{mx} {my} {tx} {ty}", fill="none", stroke=C["outline"], stroke_width=16,
              stroke_linecap="round") + el("path", d=f"M{sx} {sy} Q{mx} {my} {tx} {ty}", fill="none",
                                            stroke=C["shade"], stroke_width=9, stroke_linecap="round")


def laptop():
    return ('<g id="prop">'
            + el("rect", x=92, y=160, width=76, height=46, rx=5, fill="#26262C", stroke=C["outline"], stroke_width=4)
            + el("rect", x=99, y=166, width=62, height=33, rx=2, fill="#0E2A1A")
            + el("rect", x=105, y=173, width=30, height=3, rx=1.5, fill=C["hi"], opacity="0.8")
            + el("rect", x=105, y=180, width=44, height=3, rx=1.5, fill=C["hi"], opacity="0.5")
            + el("rect", x=105, y=187, width=22, height=3, rx=1.5, fill=C["hi"], opacity="0.5")
            + "</g>")


def hood(expr="open", look=(0, 0), glow=True, hands="rest", prop=False, mirror=False, standalone=False):
    o = []
    # ground shadow: stays put while the figure breathes
    o.append('<g id="shadow">'
             + el("ellipse", cx=130, cy=244, rx=84, ry=7, fill="#000", opacity="0.10")
             + el("ellipse", cx=130, cy=244, rx=68, ry=5.5, fill="#000", opacity="0.12")
             + el("ellipse", cx=130, cy=244, rx=50, ry=4, fill="#000", opacity="0.16") + "</g>")
    fig = []
    if glow:  # the back aura: behind the body, around the hood only
        fig.append('<g id="aura">'
                   + el("ellipse", cx=130, cy=96, rx=78, ry=66, fill=C["glow"], opacity="0.05")
                   + el("ellipse", cx=130, cy=94, rx=72, ry=60, fill=C["glow"], opacity="0.07")
                   + el("ellipse", cx=130, cy=92, rx=67, ry=55, fill=C["glow"], opacity="0.09") + "</g>")
    fig.append('<g id="body">'
               + el("path", d=SILHOUETTE, fill=C["shade"])
               + el("path", d=CLOAK, fill=C["deep"])
               + el("path", d=HEM, fill=C["outline"])
               + el("path", d=HOOD_RIM, fill=C["hi"])
               + el("path", d=SHOULDER_RIM_L, fill=C["rimMid"])
               + el("path", d=SHOULDER_RIM_R, fill=C["rimMid"])
               + el("path", d="M116 158 C115 172 113 187 111 201", fill="none", stroke=C["body"], stroke_width=3,
                    stroke_linecap="round")
               + el("path", d="M49 203 C81 211 179 211 211 203", fill="none", stroke=C["body"], stroke_width=2,
                    stroke_linecap="round")
               + el("path", d=CROWN, fill="none", stroke=C["spec"], stroke_width=3, stroke_linecap="round",
                    opacity="0.45")
               + el("path", d=SILHOUETTE, fill="none", stroke=C["outline"], stroke_width=4,
                    stroke_linejoin="round")
               + "</g>")
    fig.append('<g id="face">' + el("path", d=FACE_LIP, fill=C["deep"]) + el("path", d=FACE, fill="#010302")
               + "</g>")
    fig.append('<g id="eyes">' + eye("eye-left", EYE_L, expr, look, glow) + eye("eye-right", EYE_R, expr, look, glow)
               + "</g>")
    hl, hr = HAND_L, HAND_R
    arms = []
    if hands == "wave":
        hr = (232, 126)
    elif hands == "raised":
        hl, hr = (28, 126), (232, 126)
    elif hands == "hold":
        hl, hr = (96, 186), (164, 186)
    if prop:
        fig.append(laptop())
    fig.append('<g id="arms">' + "".join(arms) + hand("hand-left", hl, "bob-l" if standalone else "", hands == "raised")
                + hand("hand-right", hr, "bob-r" if standalone else "", hands in ("wave", "raised")) + "</g>")
    inner = "".join(fig)
    cls = ' class="breathe"' if standalone else ""
    o.append(f'<g id="figure" transform="translate(0 22)"><g{cls}>{inner}</g></g>')
    body = "".join(o)
    if mirror:
        body = f'<g transform="translate(260 0) scale(-1 1)">{body}</g>'
    return body


HEADER = """<!-- Hood (green): a smooth, flat-shaded cartoon vector of the pixel Hood. Round hood with no peak, upright sides,
     a robe that falls straight from the shoulders, a dark recessed face, tall glowing eyes, small round hands resting in front of the robe that float out when waving, bright rim light.
     Colours: avatars/hood-green.json. Generated with plain shapes only (no filters, masks, clip paths, gradients or
     <use>), so each group maps to a canvas part: shadow, aura, body, face, eyes (eye-left, eye-right, each translated
     to its centre), arms (hand-left, hand-right, translated to their centres; left/right are screen sides in this
     unmirrored drawing; mirror around x=130). Draw order: shadow, aura, body, face, eyes, prop, arms. The CSS below is
     only a preview of breathing and the hands' bobbing; the app drives motion itself. -->"""

STYLE = """<style>
    .breathe { transform-origin: 130px 220px; animation: breathe 3.6s ease-in-out infinite; }
    @keyframes breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.01, 0.988); } }
    .bob-l, .bob-r { animation: bob 2.8s ease-in-out infinite; }
    .bob-r { animation-delay: -1.4s; }
    @keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-4px); } }
    @media (prefers-reduced-motion: reduce) { .breathe, .bob-l, .bob-r { animation: none; } }
  </style>"""


def svg(content, w=520, h=500, extra=""):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 250" width="{w}" height="{h}">'
            f"{extra}{content}</svg>")


def main():
    with open(os.path.join(OUT, "hood-green.svg"), "w", encoding="utf-8", newline="\n") as f:
        f.write(svg("\n  <title>Hood (green)</title>\n  " + HEADER + "\n  " + STYLE + "\n  "
                    + hood(standalone=True) + "\n", extra="") + "\n")

    states = [
        ("neutral", {}),
        ("look up-right", {"look": (5, -4)}),
        ("look down-left", {"look": (-5, 4)}),
        ("blink (closed)", {"expr": "closed"}),
        ("happy", {"expr": "happy"}),
        ("wide", {"expr": "wide"}),
        ("squint", {"expr": "squint"}),
        ("waving", {"hands": "wave"}),
        ("hands raised", {"hands": "raised", "expr": "wide"}),
        ("holding laptop", {"hands": "hold", "prop": True, "look": (0, 4)}),
        ("mirrored (look right→left)", {"look": (5, 0), "mirror": True}),
        ("glow off", {"glow": False}),
    ]
    bgs = [("white", "#ffffff"), ("mid-grey", "#808080"), ("near-black", "#101214")]
    box = lambda s, bg: (f'<div class="box" style="background:{bg}">'
                         f'{svg(hood(**s), 130, 125)}</div>')
    rows = []
    for name, s in states:
        rows.append(f'<tr><th>{name}</th>' + "".join(f"<td>{box(s, bg)}</td>" for _, bg in bgs) + "</tr>")
    with open(os.path.join(OUT, "hood-pixel.png"), "rb") as f:
        pixel_src = "data:image/png;base64," + base64.b64encode(f.read()).decode()
    html = f"""<!doctype html>
<html><head><meta charset="utf-8"><title>Hood (green) review</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font: 14px system-ui, sans-serif; margin: 16px; background: #f4f4f4; color: #111; }}
  h2 {{ margin: 24px 0 8px; font-size: 16px; }}
  .cmp {{ display: flex; gap: 24px; align-items: flex-end; }}
  .cmp figure {{ margin: 0; text-align: center; }}
  .cmp .stage {{ background: #101214; padding: 12px; display: flex; align-items: flex-end; justify-content: center;
                height: 240px; width: 260px; }}
  .pixel {{ image-rendering: pixelated; height: 216px; }}
  table {{ border-collapse: collapse; }}
  th {{ text-align: right; padding: 4px 10px; font-weight: 500; white-space: nowrap; }}
  td {{ padding: 4px; }}
  .box {{ width: 130px; height: 120px; overflow: hidden; outline: 1px dashed #bbb; }}
  .box svg {{ display: block; width: 130px; height: 125px; margin-top: -2.5px; }}
</style></head><body>
<h2>Pixel original vs vector, at comparable character height (on near-black)</h2>
<div class="cmp">
  <figure><div class="stage"><img class="pixel" src="{pixel_src}" alt="pixel Hood"></div>
    <figcaption>pixel Hood (screenshot)</figcaption></figure>
  <figure><div class="stage">{svg(hood(), 228, 219)}</div><figcaption>vector Hood (green)</figcaption></figure>
  <figure><div class="stage">{svg(hood(glow=False), 228, 219)}</div><figcaption>vector, glow off</figcaption></figure>
</div>
<h2>App size: 130×120 logical px boxes (dashed), on white, mid-grey and near-black</h2>
<table><tr><th></th>{"".join(f"<th style='text-align:left'>{n}</th>" for n, _ in bgs)}</tr>
{"".join(rows)}
</table>
</body></html>
"""
    with open(os.path.join(OUT, "hood-green-preview.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(html)


if __name__ == "__main__":
    main()
