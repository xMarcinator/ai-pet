"""Generates avatars/reference/sprout.svg (neutral pose, canvas-ready geometry), sprout-working.svg (the working pose,
with experimental typing), sprout-preview.html (pixel original vs vector, then every state the pet supports, in
130x120 boxes on three backgrounds) and sprout-sheet.svg (every state as a labelled frame) from one description of
Sprout; the states come from pet_states.py.

The shapes come from the pixel Sprout itself (rust/crates/aipet-sprite/src/avatar.rs, Avatar::sprout): its four
ellipses on the 26x24 grid, x10 here, are traced into one smooth outline; the face, eyes, cheeks, arms and feet sit on
the pixel pet's cells. Colours are Sprout's palette. Canvas-ready: plain paths, rects, circles and ellipses with flat
fills and opacity; no filters, masks, clip paths, gradients or <use>. Run from anywhere:
python avatars/reference/sprout_gen.py
"""
import base64
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pet_states  # noqa: E402
import vector_pack as vp  # noqa: E402

ROOT = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
OUT = os.path.join(ROOT, "avatars", "reference")

C = {  # Avatar::sprout's palette
    "outline": "#3E1C12", "deep": "#A24A2C", "shade": "#C65F3B", "body": "#E47E55", "hi": "#F5A57D",
    "spec": "#FFD6BC", "blush": "#F08A7C", "screen": "#171A2C", "bezel": "#2A2F4D", "glint": "#3D4674",
    "eye": "#7FEFFF", "eyeHi": "#E4FFFF", "eyeLo": "#3FB8D8", "glow": "#7FEFFF", "accent": "#E27A52",
}

S = 10.0  # canvas units per grid cell
LOBES = [(13.0, 14.0, 7.3, 7.3), (8.6, 9.2, 4.0, 4.0), (13.0, 7.2, 4.6, 4.6), (17.4, 9.2, 4.0, 4.0)]  # cx, cy, rx, ry


CENTRE = (13.0, 12.5)  # grid cell the outline is traced from


def ray(a):
    """The outline's distance (in cells) from CENTRE along angle a: the furthest point inside any of the ellipses."""
    cx, cy = CENTRE
    dx, dy = math.cos(a), math.sin(a)
    best = 0.0
    for ex, ey, rx, ry in LOBES:
        # furthest t with (cx+t dx - ex)^2/rx^2 + (cy+t dy - ey)^2/ry^2 <= 1
        A = dx * dx / rx ** 2 + dy * dy / ry ** 2
        B = 2 * (dx * (cx - ex) / rx ** 2 + dy * (cy - ey) / ry ** 2)
        Cc = (cx - ex) ** 2 / rx ** 2 + (cy - ey) ** 2 / ry ** 2 - 1
        disc = B * B - 4 * A * Cc
        if disc >= 0:
            best = max(best, (-B + math.sqrt(disc)) / (2 * A))
    return best


def point(a, inset=0.0):
    """The outline point at angle a, moved `inset` canvas units towards the centre."""
    cx, cy = CENTRE
    r = ray(a) * S - inset
    return (cx * S + r * math.cos(a), cy * S + r * math.sin(a))


def smooth(pts, closed=True):
    """Catmull-Rom through the points, as cubic Beziers."""
    n = len(pts)
    d = f"M{pts[0][0]:.1f} {pts[0][1]:.1f}"
    for i in range(n if closed else n - 1):
        p0, p1 = pts[i - 1] if (closed or i > 0) else pts[0], pts[i]
        p2 = pts[(i + 1) % n]
        p3 = pts[(i + 2) % n] if (closed or i + 2 < n) else p2
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f" C{c1[0]:.1f} {c1[1]:.1f} {c2[0]:.1f} {c2[1]:.1f} {p2[0]:.1f} {p2[1]:.1f}"
    return d


def silhouette(n=96):
    """Trace the union of Sprout's ellipses by rays from its centre, then smooth it into cubic curves."""
    return smooth([point(2 * math.pi * i / n) for i in range(n)]) + " Z"


SILHOUETTE = silhouette()
# no shading bands: one flat body colour, with the outline as its only edge (the owner's choice)
SPEC = "M116 34 C124 30 136 30 144 34"
FACE = (80, 90, 100, 60)  # face cells 8-17 x 9-14: x, y, w, h

EYE_L, EYE_R = (110, 115), (150, 115)  # eye centres (cells 10-11 and 14-15, rows 10-12)
# fnt-72: where a tear starts (under the screen-right eye), the news envelope (where the laptop sits) and the glow
FACE_TEAR = (156, 137)
ENVELOPE = (114, 146, 204, 198)
AURA = (130, 135, 96, 92)
ARM_L, ARM_R = (58, 166), (202, 166)  # arm nubs (cells 4-5 and 20-21, rows 15-16); screen sides, unmirrored
FEET = [(95, 214), (165, 214)]  # feet (cells 8-10 and 15-17, row 21)


def el(tag, **a):
    attrs = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in a.items())
    return f"<{tag} {attrs}/>"


def eye(name, origin, expr="open", look=(0, 0), glow=True):
    x, y = origin[0] + look[0], origin[1] + look[1]
    p, g = [], C["glow"]
    oy = 0
    if expr in ("closed", "blink", "sleep"):  # blink: a short line; sleep: a wider one (pet.rs eye_cells)
        w = 20 if expr != "sleep" else 28
        if glow:
            p.append(el("rect", x=-w / 2 - 2, y=3, width=w + 4, height=10, rx=5, fill=g, opacity="0.15"))
        p.append(el("rect", x=-w / 2, y=6, width=w, height=4, rx=2, fill=C["eye"]))
    elif expr == "sleepy":  # tired (fnt-72): the eye's lower half, under a flat lid
        if glow:
            p.append(el("rect", x=-13, y=-1, width=26, height=17, rx=6, fill=g, opacity="0.14"))
        p.append(el("rect", x=-10, y=2, width=20, height=11, rx=3, fill=C["eye"]))
        p.append(el("rect", x=-11, y=1, width=22, height=3.5, rx=1.5, fill=C["eyeLo"]))
    elif expr == "happy":
        if glow:
            p.append(el("path", d="M-10 6 C-8 -8 8 -8 10 6", fill="none", stroke=g, stroke_width=11,
                        stroke_linecap="round", opacity="0.15"))
        p.append(el("path", d="M-8 5 C-6 -5 6 -5 8 5", fill="none", stroke=C["eye"], stroke_width=5,
                    stroke_linecap="round"))
    else:
        w, h, r = {"open": (20, 28, 4), "wide": (24, 34, 5), "squint": (20, 12, 4)}[
            "open" if expr in ("up", "down") else expr]
        oy = 0
        if expr in ("up", "down"):  # the pixel pet's up/down eyes: two of the eye's three rows, at the top or bottom
            oy, h = (-h / 6 if expr == "up" else h / 2), h * 2 / 3
        if glow:
            p.append(el("rect", x=-w / 2 - 4, y=-h / 2 - 4, width=w + 8, height=h + 8, rx=r + 4, fill=g,
                        opacity="0.16"))
        p.append(el("rect", x=-w / 2, y=-h / 2, width=w, height=h, rx=r, fill=C["eye"]))
        if expr != "squint":  # the pixel eye: a lighter top row, a deeper lower third
            p.append(el("rect", x=-w / 2, y=-h / 2, width=w, height=h / 4, rx=r, fill=C["eyeHi"]))
            p.append(el("rect", x=-w / 2, y=h / 2 - h / 3, width=w, height=h / 3, rx=r, fill=C["eyeLo"]))
    return f'<g id="{name}" transform="translate({x} {y + oy})">' + "".join(p) + "</g>"


def arm(name, pos, cls="", side=-1):
    x, y = pos
    c = f' class="{cls}"' if cls else ""
    rim = "M-6 -5 C-3 -9 3 -9 6 -6"
    return (f'<g id="{name}" transform="translate({x} {y})"><g{c}>'
            + el("ellipse", cx=0, cy=0, rx=12, ry=11, fill=C["body"], stroke=C["outline"], stroke_width=4)
            + el("path", d=rim, fill="none", stroke=C["hi"], stroke_width=3, stroke_linecap="round")
            + "</g></g>")


def laptop(caret=True):
    """As on the pixel Sprout at work: a laptop at the lower right, its dark screen showing a cyan prompt."""
    hi = C["eye"]
    return ('<g id="prop"><g transform="translate(158 168) scale(1.2) translate(-185 -172)">'
            + el("rect", x=144, y=190, width=82, height=10, rx=3, fill="#C9CDD6", stroke=C["outline"], stroke_width=4)
            + el("rect", x=152, y=144, width=66, height=50, rx=5, fill=C["bezel"], stroke=C["outline"], stroke_width=4)
            + el("rect", x=158, y=150, width=54, height=38, rx=2, fill=C["screen"])
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=C["glow"], stroke_width=7,
                 stroke_linecap="round", stroke_linejoin="round", opacity="0.3")
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=hi, stroke_width=4,
                 stroke_linecap="round", stroke_linejoin="round")
            + ('<g id="caret" class="caret">' if caret else '<g id="caret" class="caret" opacity="0">')
            + el("rect", x=179.5, y=173.5, width=14, height=6.5, rx=3, fill=C["glow"], opacity="0.3")
            + el("rect", x=181, y=175, width=11, height=3.5, rx=1.5, fill=hi, opacity="0.8")
            + "</g></g></g>")


GROUND = 222  # where the feet stand


def shadow():
    return ('<g id="shadow">'
            + el("ellipse", cx=130, cy=226, rx=80, ry=7, fill="#000", opacity="0.10")
            + el("ellipse", cx=130, cy=226, rx=64, ry=5.5, fill="#000", opacity="0.12")
            + el("ellipse", cx=130, cy=226, rx=46, ry=4, fill="#000", opacity="0.16") + "</g>")


def figure(pose, standalone=False):
    """Sprout for one pose (pet_states.STATES): feet, body, face, eyes, prop, headphones, arms."""
    expr, look, glow = pose.get("expr", "open"), pose.get("look", (0, 0)), pose.get("glow", True)
    arms, feet_pose, prop, tap = pose.get("arms", "rest"), pose.get("feet", "stand"), pose.get("prop"), pose.get("tap", (0, 0))
    phase, scan = pose.get("phase", 0), pose.get("scan", 0)
    fig = []
    feet = {"stand": FEET, "tap": [FEET[0], (165, 206)],
            "run": [(80, 210), (180, 206)] if not phase else [(84, 206), (176, 210)],
            "dangle": [(104, 228), (156, 230)] if not phase else [(104, 230), (156, 228)]}[feet_pose]
    fig.append('<g id="feet">' + "".join(
        f'<g id="foot-{side}">'
        + el("ellipse", cx=x, cy=y, rx=16, ry=8, fill=C["deep"], stroke=C["outline"], stroke_width=4) + "</g>"
        for side, (x, y) in zip(("left", "right"), feet)) + "</g>")
    fig.append('<g id="body">'
               + el("path", d=SILHOUETTE, fill=C["body"])
               + el("path", d=SPEC, fill="none", stroke=C["spec"], stroke_width=5, stroke_linecap="round")
               + el("path", d=SILHOUETTE, fill="none", stroke=C["outline"], stroke_width=5, stroke_linejoin="round")
               + "</g>")
    fx, fy, fw, fh = FACE
    fig.append('<g id="face">'
               + el("rect", x=fx - 5, y=fy - 5, width=fw + 10, height=fh + 10, rx=14, fill=C["bezel"],
                    stroke=C["outline"], stroke_width=4)
               + el("rect", x=fx, y=fy, width=fw, height=fh, rx=10, fill=C["screen"])
               + el("path", d=f"M{fx + 8} {fy + 14} C{fx + 10} {fy + 7} {fx + 16} {fy + 5} {fx + 28} {fy + 5}",
                    fill="none", stroke=C["glint"], stroke_width=3, stroke_linecap="round")
               # tired (fnt-72): a yawn, a glowing open mouth on the screen face
               + (el("ellipse", cx=130, cy=140, rx=9, ry=8, fill=C["glow"], opacity="0.18")
                  + el("ellipse", cx=130, cy=140, rx=6, ry=5.5, fill=C["screen"], stroke=C["eye"], stroke_width=3)
                  if pose.get("yawn") else "")
               + "</g>")
    fig.append('<g id="eyes">' + eye("eye-left", EYE_L, expr, look, glow) + eye("eye-right", EYE_R, expr, look, glow)
               + "</g>")
    al, ar = ARM_L, ARM_R
    if arms == "wave":  # two frames, A and B
        ar = (226, 112) if not phase else (220, 128)
    elif arms == "raised":
        al, ar = (34, 112), (226, 112)
    elif arms == "hold":  # screen-left arm types at the laptop's near edge; screen-right arm holds its far end
        al, ar = (124 + tap[0], 188 + tap[1]), (214, 188)
    elif arms == "lens":  # screen-right arm holds the magnifier's handle
        ar = (236 + scan * pet_states.LENS_STEP, 190)
    elif arms == "run":
        al, ar = ((44, 184), (214, 142)) if not phase else ((46, 142), (214, 184))
    if prop == "laptop":
        fig.append(laptop(pose.get("caret", True)))
    elif prop == "lens":
        fig.append(pet_states.magnifier(scan))
    elif prop == "envelope":  # news (fnt-72): held where the laptop is
        fig.append(pet_states.envelope(ENVELOPE, C["accent"], C["outline"]))
    if pose.get("headphones"):
        fig.append(pet_states.headphones(48, 212, 104, 2, C["accent"], C["outline"]))
    cl = ("type" if arms == "hold" else ("bob-l" if standalone else ""),
          "" if arms == "hold" else ("bob-r" if standalone else ""))
    fig.append('<g id="arms">' + arm("arm-left", al, cl[0]) + arm("arm-right", ar, cl[1]) + "</g>")
    cls = ' class="breathe"' if standalone else ""
    return f'<g id="figure"><g{cls}>' + "".join(fig) + "</g></g>"


def frame(pose, standalone=False):
    return pet_states.compose(figure(pose, standalone), shadow(), GROUND,
                              {**pose, "face": FACE_TEAR, "aura_box": AURA}, C["accent"], C["outline"])


HEADER = """<!-- Sprout: a smooth, flat-shaded cartoon vector of the pixel Sprout. Its four ellipses traced into one round,
     lobed outline; a dark screen face with a bezel and cyan glowing eyes; stubby arms; dark feet; flat
     colour. Colours: Sprout's palette. Plain shapes only (no filters, masks, clip paths, gradients or <use>), so each
     group maps to a canvas part: shadow, feet, body, face, eyes (eye-left, eye-right, translated to their centres),
     prop, headphones, arms (arm-left, arm-right, translated to their centres; left/right are screen sides in this
     unmirrored drawing; mirror around x=130), fx. Every state: pet_states.STATES. The CSS is only a preview. -->"""

ANIM = [
    "    .type { animation: type 1.1s steps(1, end) infinite; }",
    "    @keyframes type { 0% { transform: translateY(0); } 8% { transform: translate(-2px, 3px); }",
    "      16% { transform: translateY(0); } 26% { transform: translate(3px, 3px); } 34% { transform: translateY(0); }",
    "      44% { transform: translate(-1px, 3px); } 52% { transform: translateY(0); } 100% { transform: translateY(0); } }",
    "    .caret { animation: caret 1s steps(1, end) infinite; }",
    "    @keyframes caret { 0% { opacity: 1; } 50% { opacity: 0; } }",
]
STYLE = ("<style>\n"
         "    .breathe { transform-origin: 130px 214px; animation: breathe 3s ease-in-out infinite; }\n"
         "    @keyframes breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.012, 0.986); } }\n"
         "    .bob-l, .bob-r { animation: bob 3s ease-in-out infinite; }\n"
         "    .bob-r { animation-delay: -1.5s; }\n"
         "    @keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-2px); } }\n"
         + "\n".join(ANIM) + "\n"
         "    @media (prefers-reduced-motion: reduce) { .breathe, .bob-l, .bob-r, .type, .caret { animation: none; } }\n"
         "  </style>")


# ------------------------------------------------------------------ the vector pack (--pack)

def pack():
    """Sprout as a vector pack (vector_pack.py): its groups read back from figure() for every state. The arms are
    one drawing each, moved to where figure() puts them."""
    d = vp.Drawing(figure, GROUND)
    a = vp.ANCHOR
    hold, lens = vp.ARMS["hold"], vp.ARMS["lens"]
    body = vp.layer("body", "state", "idle", hit=True, shapes={"idle": vp.drawing(d.art({}, "body", "face")),
                                                               "tired": vp.drawing(d.art({"yawn": True}, "body", "face"))})
    eyes = vp.eyes(d, lambda expr: {"expr": expr})
    prop = vp.layer("prop", "prop", "laptop", parts=[{"id": "prop", "origin": list(a)}],
                    shapes={"laptop": vp.drawing(vp.relative(d.art(hold, "prop", skip=("caret",)), a)),
                            "lens": vp.drawing(vp.relative(d.art(lens, "prop"), a)),
                            "envelope": vp.drawing(vp.relative(d.art({**hold, "prop": "envelope"}, "prop"), a))},
                    animations={"lens": vp.anim("prop", vp.scan_keys())},
                    fallback={"none": None})
    at, cursor = vp.caret(d, hold)
    screen = vp.layer("screen", "prop", "laptop",
                      parts=[{"id": "caret", "origin": list(at), "shape": "caret"}], shapes={"caret": cursor},
                      animations={"laptop": vp.anim("caret", vp.keys(vp.held(STYLE, "caret", "caret")))},
                      fallback={"lens": None, "none": None, "envelope": None})
    phones = vp.layer("headphones", "headphones", "on", hit=True,
                      shapes={"on": vp.drawing(d.art({"headphones": True}, "headphones"))}, fallback={"off": None})
    ids = ["arm-left", "arm-right"]
    poses = {value: {pid: list(d.at(pose, pid)[0]) for pid in ids} for value, pose in vp.ARMS.items()}
    bob = vp.keys(vp.drawn(vp.eased(STYLE, "bob-l", "bob", 16)))
    arms = vp.layer("arms", "arms", "rest", hit=True, mirror="flip",
                    parts=[{"id": pid, "origin": poses["rest"][pid], "shape": "arm"} for pid in ids],
                    shapes={"arm": vp.drawing(d.inner({}, "arm-left"))}, poses=poses, transition_ms=120,
                    animations={
                        "rest": vp.anim([(pid, vp.phase(STYLE, c)) for pid, c in zip(ids, ["bob-l", "bob-r"])], bob),
                        # the screen-left arm types; the screen-right one holds the magnifier as it scans
                        "hold": vp.anim("arm-left", vp.keys(vp.drawn(vp.held(STYLE, "type", "type")))),
                        "lens": vp.anim("arm-right", vp.scan_keys()),
                    })
    feet = {**vp.feet(d, ["foot-left", "foot-right"]), "mirror": "flip"}
    effects = [{**fx, "mirror": "flip"} if fx["name"] == "dust" else fx
               for fx in vp.effect_layers(C["accent"], C["outline"], d.norm, FACE_TEAR, AURA)]
    return vp.named({
        "format_version": 1,
        "name": "Sprout (vector)",
        # a style of the Sprout (FNT-74): one entry in Settings, its drawings as styles
        "pet": "Sprout",
        "kind": "vector",
        "pixel_size": 1,
        "anchor": list(a),
        # Sprout faces the viewer: facing left, only its arms, feet and dust follow, never the body
        "mirror": "none",
        "accent": C["accent"],
        "glow": C["glow"],
        # the done's golden glow first, behind everything
        "layers": [vp.aura_layer(d.norm, AURA), feet, body, eyes, prop, screen, phones, arms, *effects],
    }, C)


def main():
    folder = vp.pack_arg("sprout-vector")
    if folder:
        vp.write(folder, pack())
        return

    def write(name, text):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)

    svg = pet_states.svg
    write("sprout.svg", svg("\n  <title>Sprout</title>\n  " + HEADER + "\n  " + STYLE + "\n  "
                            + frame({}, standalone=True) + "\n") + "\n")
    write("sprout-working.svg", svg("\n  <title>Sprout, working</title>\n  <!-- Experimental: the working pose, "
                                    "typing, with a blinking cursor. Generated by sprout_gen.py. -->\n  " + STYLE
                                    + "\n  " + frame({"arms": "hold", "prop": "laptop", "look": (4, 3)}, True)
                                    + "\n") + "\n")
    pet_states.write_pages(OUT, "sprout", "Sprout", frame, os.path.join(OUT, "sprout-pixel.png"),
                           "pixel Sprout (golden frame)", "\n".join("  " + l.strip() for l in ANIM))


if __name__ == "__main__":
    main()
