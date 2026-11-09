"""Generates avatars/reference/hood-green.svg (neutral pose, canvas-ready geometry), hood-green-working.svg (the
working pose, with experimental typing), hood-green-preview.html (pixel original vs vector, then every state the pet
supports, in 130x120 boxes on three backgrounds) and hood-green-sheet.svg (every state as a labelled frame, plus the
typing frames) from one description of the green Hood; the states come from pet_states.py.

Canvas-ready means: plain paths, rects, circles and ellipses with flat fills and opacity; no filters, masks, clip
paths, gradients or <use>. Run from anywhere: python avatars/reference/hood_gen.py
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pet_states  # noqa: E402
import vector_pack as vp  # noqa: E402

ROOT = sys.argv[1] if len(sys.argv) > 1 and not sys.argv[1].startswith("--") else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")
OUT = os.path.join(ROOT, "avatars", "reference")

C = {  # avatars/hood-green.json
    "outline": "#030805", "deep": "#061F0F", "shade": "#0A3118", "body": "#0F4422", "hi": "#2FD36B",
    "spec": "#A4FFC4", "screen": "#020604", "eye": "#5CFF9C", "eyeHi": "#E8FFF0", "eyeLo": "#22C45E",
    "glow": "#2BFF7A",
    # a step between body and hi, for the cloak's side rim (dark backgrounds)
    "rimMid": "#1C7A3E",
}

# The pixel Hood's colours (Avatar::hood), as the vector Hood's Blue theme; rimMid is the same step between its body
# and hi.
BLUE = {
    "outline": "#04050B", "deep": "#0A1330", "shade": "#0F1C45", "body": "#16285E", "hi": "#3E74E8",
    "spec": "#8DBBFF", "screen": "#030409", "eye": "#66C8FF", "eyeHi": "#E8F7FF", "eyeLo": "#2F8FF0",
    "glow": "#3D9BFF", "rimMid": "#264695",
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

# The drawstrings: from the hood opening's lower corners, curving in a little, to their tips; pulled unevenly, so the
# screen-left one hangs shorter than the screen-right one.
LACES = [("M109 141 C106 151 106 162 108 171", (108, 175)), ("M151 141 C154 156 154 174 152 189", (152, 193))]

EYE_L, EYE_R = (111, 110), (149, 110)   # local origins (eye centres)
HAND_L, HAND_R = (70, 180), (190, 180)  # resting: in front of the robe; waving/raised float outside  # floating free beside the robe  # screen-left / screen-right in the unmirrored drawing


def el(tag, **a):
    attrs = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in a.items())
    return f"<{tag} {attrs}/>"


def eye(name, origin, expr="open", look=(0, 0), glow=True):
    x, y = origin[0] + look[0], origin[1] + look[1]
    parts = []
    g = C["glow"]
    oy = 0
    if expr in ("closed", "blink", "sleep"):  # blink: a short line; sleep: a wider one (pet.rs eye_cells)
        w = 18 if expr != "sleep" else 26
        if glow:
            parts.append(el("rect", x=-w / 2 - 3, y=3, width=w + 6, height=10, rx=5, fill=g, opacity="0.15"))
        parts.append(el("rect", x=-w / 2, y=6, width=w, height=4, rx=2, fill=C["eye"]))
    elif expr == "happy":
        if glow:
            parts.append(el("path", d="M-10 6 C-8 -8 8 -8 10 6", fill="none", stroke=g, stroke_width=11,
                            stroke_linecap="round", opacity="0.15"))
        parts.append(el("path", d="M-8 5 C-6 -5 6 -5 8 5", fill="none", stroke=C["eye"], stroke_width=5,
                        stroke_linecap="round"))
    else:
        w, h, r = {"open": (18, 26, 7), "wide": (22, 34, 9), "squint": (18, 12, 5)}[
            "open" if expr in ("up", "down") else expr]
        oy = 0
        if expr in ("up", "down"):  # the pixel pet's up/down eyes: two of the eye's three rows, at the top or bottom
            oy, h = (-h / 6 if expr == "up" else h / 2), h * 2 / 3
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
    return f'<g id="{name}" transform="translate({x} {y + oy})">' + "".join(parts) + "</g>"


def hand(name, pos, cls="", halo=False):
    x, y = pos
    rim = "M-5 -4 C-3 -7 2 -8 5 -6" if x < 130 else "M-5 -6 C-2 -8 3 -7 5 -4"
    c = f' class="{cls}"' if cls else ""
    h = el("circle", cx=0, cy=0, r=13, fill=C["glow"], opacity="0.10") if halo else ""
    return (f'<g id="{name}" transform="translate({x} {y})"><g{c}>' + h
            + el("circle", cx=0, cy=0, r=10, fill=C["body"], stroke=C["outline"], stroke_width=4)
            + el("path", d=rim, fill="none", stroke=C["hi"], stroke_width=3, stroke_linecap="round")
            + "</g></g>")


def laptop(caret=True):
    """The working prop, as on the pixel Hood: a laptop off to the right, its dark screen showing a green prompt.
    Drawn at its original coordinates, then scaled 1.2x and set so its right end stops just inside the Hood's right outline."""
    return ('<g id="prop"><g transform="translate(158 172) scale(1.2) translate(-185 -172)">'
            + el("rect", x=144, y=190, width=82, height=10, rx=3, fill="#3A3F4E", stroke=C["outline"], stroke_width=4)
            + el("rect", x=152, y=144, width=66, height=50, rx=5, fill="#4A5266", stroke=C["outline"], stroke_width=4)
            + el("rect", x=158, y=150, width=54, height=38, rx=2, fill="#111827")
            # a tight glow hugging the prompt: one translucent stroke, no blur
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=C["glow"], stroke_width=7,
                 stroke_linecap="round", stroke_linejoin="round", opacity="0.3")
            + el("path", d="M168 160 L177 169 L168 178", fill="none", stroke=C["hi"], stroke_width=4,
                 stroke_linecap="round", stroke_linejoin="round")
            + ('<g id="caret" class="caret">' if caret else '<g id="caret" class="caret" opacity="0">')
            + el("rect", x=179.5, y=173.5, width=14, height=6.5, rx=3, fill=C["glow"], opacity="0.3")
            + el("rect", x=181, y=175, width=11, height=3.5, rx=1.5, fill=C["hi"], opacity="0.8")
            + "</g>"
            + "</g></g>")


GROUND = 242  # the robe's hem, where the figure stands (figure coordinates + 22)


def laces():
    """The two drawstrings, one hanging from each side of the hood opening over the robe: a cord on a dark outline,
    with a bright tip."""
    out = []
    for d, tip in LACES:
        out.append(el("path", d=d, fill="none", stroke=C["outline"], stroke_width=5, stroke_linecap="round"))
        out.append(el("path", d=d, fill="none", stroke=C["rimMid"], stroke_width=2.5, stroke_linecap="round"))
        out.append(el("ellipse", cx=tip[0], cy=tip[1], rx=2.6, ry=4, fill=C["hi"], stroke=C["outline"],
                      stroke_width=1.5))
    return '<g id="laces">' + "".join(out) + "</g>"


def shadow():
    """The ground shadow: stays put while the figure moves."""
    return ('<g id="shadow">'
            + el("ellipse", cx=130, cy=244, rx=84, ry=7, fill="#000", opacity="0.10")
            + el("ellipse", cx=130, cy=244, rx=68, ry=5.5, fill="#000", opacity="0.12")
            + el("ellipse", cx=130, cy=244, rx=50, ry=4, fill="#000", opacity="0.16") + "</g>")


def figure(pose, standalone=False):
    """The Hood for one pose (pet_states.STATES): aura, body, face, eyes, prop, headphones, hands. It has no feet,
    so foot poses don't change it."""
    expr, look, glow = pose.get("expr", "open"), pose.get("look", (0, 0)), pose.get("glow", True)
    hands, prop, tap = pose.get("arms", "rest"), pose.get("prop"), pose.get("tap", (0, 0))
    phase, scan = pose.get("phase", 0), pose.get("scan", 0)
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
               + el("path", d="M49 203 C81 211 179 211 211 203", fill="none", stroke=C["body"], stroke_width=2,
                    stroke_linecap="round")
               + el("path", d=CROWN, fill="none", stroke=C["spec"], stroke_width=3, stroke_linecap="round",
                    opacity="0.45")
               + el("path", d=SILHOUETTE, fill="none", stroke=C["outline"], stroke_width=4,
                    stroke_linejoin="round")
               + "</g>")
    fig.append('<g id="face">' + el("path", d=FACE_LIP, fill=C["deep"]) + el("path", d=FACE, fill="#010302")
               + "</g>")
    fig.append(laces())
    fig.append('<g id="eyes">' + eye("eye-left", EYE_L, expr, look, glow) + eye("eye-right", EYE_R, expr, look, glow)
               + "</g>")
    hl, hr = HAND_L, HAND_R
    if hands == "wave":  # two frames, A and B
        hr = (232, 126) if not phase else (226, 142)
    elif hands == "raised":
        hl, hr = (28, 126), (232, 126)
    elif hands == "hold":
        # screen-left hand types at the near edge (tap: a keystroke's offset); screen-right hand holds the far end
        hl, hr = (119 + tap[0], 196 + tap[1]), (209, 196)
    elif hands == "lens":  # screen-right hand holds the magnifier's handle
        hr = (234 + scan * pet_states.LENS_STEP, 188)
    elif hands == "run":  # hands swing: one forward and up, one back and down
        hl, hr = ((32, 196), (222, 140)) if not phase else ((38, 140), (216, 196))
    if prop == "laptop":
        fig.append(laptop(pose.get("caret", True)))
    elif prop == "lens":
        fig.append(pet_states.magnifier(scan))
    if pose.get("headphones"):
        fig.append(pet_states.headphones(64, 196, 112, 24, C["hi"], C["outline"]))
    floating = hands in ("wave", "raised", "run")
    fig.append('<g id="arms">'
               + hand("hand-left", hl, "type" if hands == "hold" else ("bob-l" if standalone else ""),
                      hands in ("raised", "run"))
               + hand("hand-right", hr, "" if hands == "hold" else ("bob-r" if standalone else ""), floating)
               + "</g>")
    cls = ' class="breathe"' if standalone else ""
    return f'<g id="figure" transform="translate(0 22)"><g{cls}>{"".join(fig)}</g></g>'


def frame(pose, standalone=False):
    return pet_states.compose(figure(pose, standalone), shadow(), GROUND, pose, C["hi"], C["outline"])


HEADER = """<!-- Hood (green): a smooth, flat-shaded cartoon vector of the pixel Hood. Round hood with no peak, upright sides,
     a robe that falls straight from the shoulders, a dark recessed face, tall glowing eyes, small round hands resting
     in front of the robe that float out when waving, bright rim light. Colours: avatars/hood-green.json. Generated
     with plain shapes only (no filters, masks, clip paths, gradients or <use>), so each group maps to a canvas part:
     shadow, aura, body, face, eyes (eye-left, eye-right, each translated to its centre), prop, headphones, arms
     (hand-left, hand-right, translated to their centres; left/right are screen sides in this unmirrored drawing;
     mirror around x=130), fx. Every state: pet_states.STATES. The CSS below is only a preview of breathing and the
     hands' bobbing; the app drives motion itself. -->"""

ANIM = [
    "    .type { animation: type 1.1s steps(1, end) infinite; }",
    "    @keyframes type { 0% { transform: translateY(0); } 8% { transform: translate(-2px, 3px); }",
    "      16% { transform: translateY(0); } 26% { transform: translate(3px, 3px); } 34% { transform: translateY(0); }",
    "      44% { transform: translate(-1px, 3px); } 52% { transform: translateY(0); } 100% { transform: translateY(0); } }",
    "    .caret { animation: caret 1s steps(1, end) infinite; }",
    "    @keyframes caret { 0% { opacity: 1; } 50% { opacity: 0; } }",
]
STYLE = ("<style>\n"
         "    .breathe { transform-origin: 130px 220px; animation: breathe 3.6s ease-in-out infinite; }\n"
         "    @keyframes breathe { 0%, 100% { transform: scale(1, 1); } 50% { transform: scale(1.01, 0.988); } }\n"
         "    .bob-l, .bob-r { animation: bob 2.8s ease-in-out infinite; }\n"
         "    .bob-r { animation-delay: -1.4s; }\n"
         "    @keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-4px); } }\n"
         + "\n".join(ANIM) + "\n"
         "    @media (prefers-reduced-motion: reduce) { .breathe, .bob-l, .bob-r, .type, .caret { animation: none; } }\n"
         "  </style>")


# ------------------------------------------------------------------ the vector pack (--pack)

def pack():
    """The Hood as a vector pack (vector_pack.py), in Green with a Blue theme: its groups read back from figure() for
    every state. It has no feet. The preview's "glow off" becomes sleep: asleep, the back aura goes out."""
    d = vp.Drawing(figure, GROUND)
    a = vp.ANCHOR
    hold, lens = vp.ARMS["hold"], vp.ARMS["lens"]
    body = vp.layer("body", "state", "idle", hit=True,
                    shapes={"idle": vp.drawing(d.art({}, "aura", "body", "face")),
                            "sleep": vp.drawing(d.art({"glow": False}, "body", "face"))})
    # the Hood faces the viewer, so nothing it wears mirrors when it faces left: only the hands and the dust follow
    laces = vp.layer("laces", "state", "idle", shapes={"idle": vp.drawing(d.art({}, "laces"))})
    eyes = vp.eyes(d, lambda expr: {"expr": expr})
    prop = vp.layer("prop", "prop", "laptop",
                    parts=[{"id": "prop", "origin": list(a)}],
                    shapes={"laptop": vp.drawing(vp.relative(d.art(hold, "prop", skip=("caret",)), a)),
                            "lens": vp.drawing(vp.relative(d.art(lens, "prop"), a))},
                    animations={"lens": vp.anim("prop", vp.scan_keys())},
                    fallback={"none": None})
    at, cursor = vp.caret(d, hold)
    screen = vp.layer("screen", "prop", "laptop",
                      parts=[{"id": "caret", "origin": list(at), "shape": "caret"}], shapes={"caret": cursor},
                      animations={"laptop": vp.anim("caret", vp.keys(vp.held(STYLE, "caret", "caret")))},
                      fallback={"lens": None, "none": None})
    phones = vp.layer("headphones", "headphones", "on", hit=True,
                      shapes={"on": vp.drawing(d.art({"headphones": True}, "headphones"))}, fallback={"off": None})
    bob = vp.keys(vp.drawn(vp.eased(STYLE, "bob-l", "bob", 16)))
    typing = vp.keys(vp.drawn(vp.held(STYLE, "type", "type")))

    def hand(pid, bob_class, motion):
        poses = {value: {pid: list(d.at(pose, pid)[0])} for value, pose in vp.ARMS.items()}
        shapes = {value: vp.drawing(d.inner(pose, pid)) for value, pose in vp.ARMS.items()}
        animations = {"rest": vp.anim([(pid, vp.phase(STYLE, bob_class))], bob)}
        animations.update({value: vp.anim(pid, k) for value, k in motion.items()})
        return vp.layer(pid, "arms", "rest", hit=True, mirror="flip",
                        parts=[{"id": pid, "origin": poses["rest"][pid]}],
                        shapes=shapes, poses=poses, transition_ms=120, animations=animations)

    # facing left (only while dragged), the dust trails the other way, as the pixel Hood's does
    effects = vp.effect_layers(C["hi"], C["outline"], d.norm)
    for fx in effects:
        if fx["name"] == "dust":
            fx["mirror"] = "flip"

    return vp.named({
        "format_version": 1,
        "name": "Hood (vector)",
        "kind": "vector",
        "pixel_size": 1,
        "anchor": list(a),
        "mirror": "none",
        "accent": C["hi"],
        "glow": C["glow"],
        "layers": [body, laces, eyes, prop, screen, phones,
                   # the screen-left hand types; the screen-right one holds the magnifier as it scans
                   hand("hand-left", "bob-l", {"hold": typing}), hand("hand-right", "bob-r", {"lens": vp.scan_keys()}),
                   *effects],
    }, C, theme="Green", themes={"Blue": BLUE})


def main():
    folder = vp.pack_arg("hood-vector")
    if folder:
        vp.write(folder, pack())
        return

    def write(name, text):
        with open(os.path.join(OUT, name), "w", encoding="utf-8", newline="\n") as f:
            f.write(text)

    svg = pet_states.svg
    working = {"arms": "hold", "prop": "laptop", "look": (4, 3)}
    write("hood-green.svg", svg("\n  <title>Hood (green)</title>\n  " + HEADER + "\n  " + STYLE + "\n  "
                                + frame({}, standalone=True) + "\n") + "\n")
    write("hood-green-working.svg", svg("\n  <title>Hood (green), working</title>\n  <!-- Experimental: the working "
                                        "pose, typing, with a blinking cursor. Generated by hood_gen.py. -->\n  "
                                        + STYLE + "\n  " + frame(working, standalone=True) + "\n") + "\n")
    typing = [("typing 1: rest", working), ("typing 2: tap", {**working, "tap": (-2, 3)}),
              ("typing 3: rest, cursor off", {**working, "caret": False}),
              ("typing 4: tap, cursor off", {**working, "tap": (3, 3), "caret": False}),
              ("typing 5: tap", {**working, "tap": (-1, 3)}), ("glow off (preview)", {"glow": False})]
    pet_states.write_pages(OUT, "hood-green", "Hood (green)", frame, os.path.join(OUT, "hood-pixel.png"),
                           "pixel Hood (screenshot)", "\n".join("  " + l.strip() for l in ANIM), typing)


if __name__ == "__main__":
    main()
