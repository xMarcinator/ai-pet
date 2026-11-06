"""Generates avatars/reference/hamster.svg (neutral pose, canvas-ready geometry), hamster-working.svg (the working
pose, with experimental typing), hamster-preview.html (reference comparison, then every state the pet supports, in
130x120 boxes on three backgrounds) and hamster-sheet.svg (every state as a labelled frame) from one description of
the Hamster, in the same layered system as the green Hood; the states come from pet_states.py.

The shapes are traced from the pixel reference (avatars/reference/hamster.png, 5 px cells): coordinates below are
written in reference cells (column, row) and mapped to the 260x250 canvas, so the vector keeps the original's
proportions, its slight turn to the left, its colours and its soft brown outline.

Canvas-ready: plain paths, rects, circles and ellipses with flat fills and opacity; no filters, masks, clip paths,
gradients or <use>. Run from anywhere: python avatars/reference/hamster_gen.py
"""
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pet_states  # noqa: E402

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
    if expr in ("closed", "blink"):  # blink: a short flat line
        p.append(el("rect", x=-6, y=0, width=12, height=3.5, rx=1.75, fill=e))
    elif expr == "sleep":  # asleep: a wider, curved line
        p.append(el("path", d="M-8 1 C-4 5 4 5 8 1", fill="none", stroke=e, stroke_width=3.5, stroke_linecap="round"))
    elif expr in ("up", "down"):  # looking up or down: the square eye's top or bottom two thirds
        p.append(el("rect", x=-6, y=-6 if expr == "up" else -2, width=12, height=8, rx=3, fill=e))
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


def laptop(caret=True):
    """The working prop, as on the other pets: a laptop at the lower right, its dark screen showing a cyan prompt."""
    hi = "#7FEFFF"
    return ('<g id="prop"><g transform="translate(160 170) scale(1.2) translate(-185 -172)">'
            + el("rect", x=144, y=190, width=82, height=10, rx=3, fill="#C9CDD6", stroke=C["dark"], stroke_width=4)
            + el("rect", x=152, y=144, width=66, height=50, rx=5, fill="#3A3A43", stroke=C["dark"], stroke_width=4)
            + el("rect", x=158, y=150, width=54, height=38, rx=2, fill="#1E2236")
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=hi, stroke_width=7,
                 stroke_linecap="round", stroke_linejoin="round", opacity="0.3")
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=hi, stroke_width=4,
                 stroke_linecap="round", stroke_linejoin="round")
            + ('<g id="caret" class="caret">' if caret else '<g id="caret" class="caret" opacity="0">')
            + el("rect", x=179.5, y=173.5, width=14, height=6.5, rx=3, fill=hi, opacity="0.3")
            + el("rect", x=181, y=175, width=11, height=3.5, rx=1.5, fill=hi, opacity="0.8")
            + "</g></g></g>")


GROUND = 217  # where the feet stand


def shadow():
    return ('<g id="shadow">'
            + el("ellipse", cx=X(23), cy=Y(36.2), rx=86, ry=7, fill="#000", opacity="0.10")
            + el("ellipse", cx=X(23), cy=Y(36.2), rx=70, ry=5.5, fill="#000", opacity="0.12")
            + el("ellipse", cx=X(23), cy=Y(36.2), rx=52, ry=4, fill="#000", opacity="0.16") + "</g>")


def figure(pose, standalone=False):
    """The Hamster for one pose (pet_states.STATES): ears, feet, body, face, eyes, prop, headphones, arms."""
    expr, look = pose.get("expr", "open"), pose.get("look", (0, 0))
    arms, feet_pose, prop, tap = pose.get("arms", "rest"), pose.get("feet", "stand"), pose.get("prop"), pose.get("tap", (0, 0))
    phase, scan = pose.get("phase", 0), pose.get("scan", 0)
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
    feet_cells = {"stand": FEET, "tap": [FEET[0], FEET[1], (32.6, 33.3)],
                  "run": [(17, 34.4), (27.5, 34.8), (33.8, 33.6)] if not phase else [(18, 34.8), (26.5, 34.2), (32.6, 34.8)],
                  "dangle": [(19, 36.3), (26.2, 35.6), (32, 36.3)] if not phase else [(19, 35.6), (26.2, 36.3), (32, 35.6)]}[feet_pose]
    feet = ""
    for i, (c, r) in enumerate(feet_cells, 1):
        feet += (f'<g id="foot-{i}">'
                 + el("rect", x=X(c) - 6, y=Y(r) - 6, width=12, height=14, rx=4, fill=C["rust"], stroke=C["edgeDark"],
                      stroke_width=2.5)
                 + el("rect", x=X(c) - 5, y=Y(r) + 4, width=10, height=4, rx=2, fill=C["dark"]) + "</g>")
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
    if arms == "wave":  # two frames, A and B
        ar, lr = (-140, (16, -8)) if not phase else (-115, (18, -2))
    elif arms == "raised":
        al, ar, ll, lr = 140, -140, (-16, -8), (16, -8)
    elif arms == "hold":  # screen-left arm types at the laptop's near edge; screen-right arm holds its far end
        al, ll = -15, (26 + tap[0], 32 + tap[1])
        ar, lr = -10, (58, 38)
    elif arms == "lens":  # screen-right arm holds the magnifier's handle
        ar, lr = 0, (80 + scan * pet_states.LENS_STEP, 30)
    elif arms == "run":  # arms swing: one forward and up, one back
        al, ar, ll, lr = (40, -70, (-8, 6), (14, -4)) if not phase else (-60, 40, (-12, -4), (10, 6))
    if prop == "laptop":
        fig.append(laptop(pose.get("caret", True)))
    elif prop == "lens":
        fig.append(pet_states.magnifier(scan))
    if pose.get("headphones"):
        fig.append(pet_states.headphones(46, 214, 104, 22, C["orange"], C["edgeDark"]))
    cl = ("type" if arms == "hold" else ("bob-l" if standalone else ""),
          "" if arms == "hold" else ("bob-r" if standalone else ""))
    fig.append('<g id="arms">' + arm("arm-left", SHOULDER_L, al, cl[0], ll) + arm("arm-right", SHOULDER_R, ar, cl[1], lr)
               + "</g>")
    cls = ' class="breathe"' if standalone else ""
    return f'<g id="figure"><g{cls}>' + "".join(fig) + "</g></g>"


def frame(pose, standalone=False):
    return pet_states.compose(figure(pose, standalone), shadow(), GROUND, pose, C["orange"], C["edgeDark"])


HEADER = """<!-- Hamster: a vector trace of the pixel hamster (avatars/reference/hamster.png): a round orange-and-cream body
     turned slightly to the left, big round ears, small brown square eyes, a brown nose and mouth, cream arms meeting
     in small paws on the belly, thin feet, and a soft brown outline (lighter on the lit left, darker on the right).
     Same layered system as the green Hood, with plain shapes only (no filters, masks, clip paths, gradients or <use>).
     Groups map to canvas parts: shadow, ears, feet, body, face, eyes (eye-left, eye-right, translated to their
     centres), prop, headphones, arms (arm-left, arm-right, translated to their shoulder pivots and rotated for
     poses; left/right are screen sides in this unmirrored drawing; mirror around x=130), fx. Every state:
     pet_states.STATES. The CSS is only a preview. -->"""

ANIM = [
    "    .type { animation: type 1.1s steps(1, end) infinite; }",
    "    @keyframes type { 0% { transform: translateY(0); } 8% { transform: translate(-2px, 3px); }",
    "      16% { transform: translateY(0); } 26% { transform: translate(3px, 3px); } 34% { transform: translateY(0); }",
    "      44% { transform: translate(-1px, 3px); } 52% { transform: translateY(0); } 100% { transform: translateY(0); } }",
    "    .caret { animation: caret 1s steps(1, end) infinite; }",
    "    @keyframes caret { 0% { opacity: 1; } 50% { opacity: 0; } }",
]
STYLE = ("<style>\n"
         "    .breathe { transform-origin: 130px 212px; animation: breathe 2.6s ease-in-out infinite; }\n"
         "    @keyframes breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.015, 0.985); } }\n"
         "    .bob-l, .bob-r { animation: bob 2.6s ease-in-out infinite; }\n"
         "    .bob-r { animation-delay: -1.3s; }\n"
         "    @keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-1.5px); } }\n"
         + "\n".join(ANIM) + "\n"
         "    @media (prefers-reduced-motion: reduce) { .breathe, .bob-l, .bob-r, .type, .caret { animation: none; } }\n"
         "  </style>")


def main():
    def write(name, text):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)

    svg = pet_states.svg
    working = {"arms": "hold", "prop": "laptop", "look": (4, 3)}
    write("hamster.svg", svg("\n  <title>Hamster</title>\n  " + HEADER + "\n  " + STYLE + "\n  "
                             + frame({}, standalone=True) + "\n") + "\n")
    write("hamster-working.svg", svg("\n  <title>Hamster, working</title>\n  <!-- Experimental: the working pose, "
                                     "typing, with a blinking cursor. Generated by hamster_gen.py. -->\n  " + STYLE
                                     + "\n  " + frame(working, standalone=True) + "\n") + "\n")
    pet_states.write_pages(OUT, "hamster", "Hamster", frame, os.path.join(OUT, "hamster.png"), "pixel reference",
                           "\n".join("  " + l.strip() for l in ANIM))


if __name__ == "__main__":
    main()
