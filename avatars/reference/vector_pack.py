"""Turns a reference generator's drawing into a vector pack (rust/crates/aipet-sprite/packs/<folder>/pack.json), shared
by hood_gen.py, hamster_gen.py and sprout_gen.py (run one with --pack).

The generators already draw every state as SVG groups named after their parts (body, eye-left, hand-right, prop,
caret, ...). This module reads those groups back from the generator's own output, so the pack is the approved
drawing itself: each rect, circle, ellipse and path becomes path data with its fill, stroke, width, caps, joins and
opacity; a part's shapes are taken relative to its origin, and its place in each pose is where the generator put
its group. The motion the review pages only preview in CSS (the typing keyframes, the caret's blink, the hands'
bob) is read from the generator's CSS, so the pack's numbers are the drawing's.

Coordinates: the 260x250 drawing is halved into the 130x120 art box, x' = x / 2 and y' = (y - ground) / 2 + 110,
so the pet's ground is on the anchor (65, 110), where every built-in pack stands and the app squashes about. The
ground shadow is the app's, so it is never read. Mirroring flips about x' = 65, the drawing's centre line.
"""

import json
import math
import os
import re
import xml.etree.ElementTree as ET

import pet_states

ANCHOR = (65.0, 110.0)
SCALE = 0.5

# The eyes' look offsets in the drawing's units (pet_states.STATES looks about 4 to 6 across and 4 up): one step
# (left, right) moves 4, a far step 6, and up or down 4, a little less on a diagonal.
NEAR, FAR, UPDOWN = 4.0, 6.0, 4.0
LOOK = {
    "center": (0, 0), "up": (0, -UPDOWN), "down": (0, UPDOWN), "left": (-NEAR, 0), "right": (NEAR, 0),
    "up-left": (-NEAR, -UPDOWN * 0.85), "up-right": (NEAR, -UPDOWN * 0.85),
    "down-left": (-NEAR, UPDOWN * 0.85), "down-right": (NEAR, UPDOWN * 0.85),
    "far-left": (-FAR, 0), "far-right": (FAR, 0),
    "up-far-left": (-FAR, -UPDOWN * 0.7), "up-far-right": (FAR, -UPDOWN * 0.7),
    "down-far-left": (-FAR, UPDOWN * 0.7), "down-far-right": (FAR, UPDOWN * 0.7),
}

# The magnifier's scan, as pet.rs runs it: one lens step (pet_states.LENS_STEP) right, back, left, back, 1.5 steps a
# second.
SCAN = [0, 1, 0, -1]
SCAN_MS = 667

# Each channel value of the pack, and the pet_states pose that draws it (pet_states names the asleep eyes "sleep",
# the pack "closed"; wave, run and dangle have an A and a B frame).
ARMS = {
    "rest": {}, "raised": {"arms": "raised"}, "wave-a": {"arms": "wave"}, "wave-b": {"arms": "wave", "phase": 1},
    "run-a": {"arms": "run"}, "run-b": {"arms": "run", "phase": 1},
    "hold": {"arms": "hold", "prop": "laptop"}, "lens": {"arms": "lens", "prop": "lens"},
}
FEET = {
    "stand": {}, "tap": {"feet": "tap"}, "dangle-a": {"feet": "dangle"}, "dangle-b": {"feet": "dangle", "phase": 1},
    "run-a": {"feet": "run"}, "run-b": {"feet": "run", "phase": 1},
}
EYES = {
    "open": "open", "closed": "sleep", "blink": "blink", "happy": "happy", "squint": "squint", "wide": "wide",
    "up": "up", "down": "down", "sleepy": "sleepy",
}
FX = ["zs", "dots", "notes", "alert"]
# fnt-72's effects replace sparkles rather than come with them
NEW_FX = ["tears", "aura", "heart"]
FX_VALUES = ["none", "dust", "sparkles"] + FX + [f"{v}+sparkles" for v in FX + ["dust"]] + NEW_FX


# ------------------------------------------------------------------ numbers and transforms

def n(v):
    """A number as the manifest writes it: at most 2 decimals, no trailing zeros."""
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def r2(v):
    v = round(v, 2)
    return 0.0 if v == 0 else v


class Affine:
    """x' = a x + c y + e, y' = b x + d y + f, as SVG writes it."""

    def __init__(self, a=1.0, b=0.0, c=0.0, d=1.0, e=0.0, f=0.0):
        self.m = (a, b, c, d, e, f)

    def then(self, o):
        """This, then `o`."""
        a, b, c, d, e, f = self.m
        A, B, C, D, E, F = o.m
        return Affine(A * a + C * b, B * a + D * b, A * c + C * d, B * c + D * d, A * e + C * f + E,
                      B * e + D * f + F)

    def apply(self, x, y):
        a, b, c, d, e, f = self.m
        return a * x + c * y + e, b * x + d * y + f

    def scale(self):
        a, b, c, d, _, _ = self.m
        return math.sqrt(abs(a * d - b * c))

    def angle(self):
        a, b, _, _, _, _ = self.m
        return math.degrees(math.atan2(b, a))


def translate(x, y=0.0):
    return Affine(e=x, f=y)


def scale(sx, sy=None):
    return Affine(a=sx, d=sx if sy is None else sy)


def rotate(deg):
    c, s = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    return Affine(c, s, -s, c)


def parse_transform(t):
    """An SVG transform list (translate, scale, rotate), as one affine."""
    out = Affine()
    for name, args in re.findall(r"(\w+)\(([^)]*)\)", t or ""):
        v = [float(x) for x in re.split(r"[\s,]+", args.strip()) if x]
        if name == "translate":
            m = translate(v[0], v[1] if len(v) > 1 else 0.0)
        elif name == "scale":
            m = scale(v[0], v[1] if len(v) > 1 else None)
        elif name == "rotate":
            m = rotate(v[0]) if len(v) == 1 else translate(-v[1], -v[2]).then(rotate(v[0])).then(translate(v[1], v[2]))
        else:
            raise ValueError(f"unsupported transform {name}")
        # the list applies right to left: the later ones first
        out = m.then(out)
    return out


def normal(ground):
    """The drawing's coordinates into the art box, with `ground` on the anchor."""
    return scale(SCALE).then(translate(0, ANCHOR[1] - ground * SCALE))


# ------------------------------------------------------------------ path data

_TOKEN = re.compile(r"[MmLlHhVvCcQqAaZz]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")
_ARGS = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "Q": 4, "A": 7, "Z": 0}


def path(d, m):
    """SVG path data (M L H V C Q A Z, absolute and relative) moved by the affine `m` (rotation and even scale
    only), as absolute M L C Q A Z; and the points it passes, for its bounds."""
    toks = _TOKEN.findall(d)
    out, pts = [], []
    i, cmd = 0, None
    cx = cy = sx = sy = 0.0
    k, ang = m.scale(), m.angle()

    def put(c, *xy):
        o = [c]
        for j in range(0, len(xy), 2):
            x, y = m.apply(xy[j], xy[j + 1])
            pts.append((x, y))
            o.append(f"{n(x)} {n(y)}")
        out.append(" ".join(o))

    while i < len(toks):
        if toks[i].isalpha():
            cmd = toks[i]
            i += 1
            if cmd in "Zz":
                out.append("Z")
                cx, cy = sx, sy
                continue
        C, rel = cmd.upper(), cmd.islower()
        v = [float(x) for x in toks[i:i + _ARGS[C]]]
        i += _ARGS[C]
        ox, oy = (cx, cy) if rel else (0.0, 0.0)
        if C == "M":
            cx, cy = v[0] + ox, v[1] + oy
            sx, sy = cx, cy
            put("M", cx, cy)
            cmd = "l" if rel else "L"
        elif C == "L":
            cx, cy = v[0] + ox, v[1] + oy
            put("L", cx, cy)
        elif C == "H":
            cx = v[0] + (cx if rel else 0.0)
            put("L", cx, cy)
        elif C == "V":
            cy = v[0] + (cy if rel else 0.0)
            put("L", cx, cy)
        elif C == "C":
            p = [v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy, v[4] + ox, v[5] + oy]
            put("C", *p)
            cx, cy = p[4], p[5]
        elif C == "Q":
            p = [v[0] + ox, v[1] + oy, v[2] + ox, v[3] + oy]
            put("Q", *p)
            cx, cy = p[2], p[3]
        elif C == "A":
            rx, ry, rot, large, sweep = v[0], v[1], v[2], int(v[3]), int(v[4])
            cx, cy = v[5] + ox, v[6] + oy
            x, y = m.apply(cx, cy)
            pts.append((x, y))
            out.append(f"A {n(rx * k)} {n(ry * k)} {n((rot + ang) % 360)} {large} {sweep} {n(x)} {n(y)}")
    return " ".join(out), pts


def ellipse_d(cx, cy, rx, ry):
    return (f"M {cx - rx} {cy} A {rx} {ry} 0 1 0 {cx + rx} {cy} A {rx} {ry} 0 1 0 {cx - rx} {cy} Z")


def rect_d(x, y, w, h, rx, ry):
    rx, ry = min(rx, w / 2), min(ry, h / 2)
    if rx <= 0 or ry <= 0:
        return f"M {x} {y} H {x + w} V {y + h} H {x} Z"
    return (f"M {x + rx} {y} H {x + w - rx} A {rx} {ry} 0 0 1 {x + w} {y + ry} V {y + h - ry} "
            f"A {rx} {ry} 0 0 1 {x + w - rx} {y + h} H {x + rx} A {rx} {ry} 0 0 1 {x} {y + h - ry} "
            f"V {y + ry} A {rx} {ry} 0 0 1 {x + rx} {y} Z")


# ------------------------------------------------------------------ reading the drawing

def tree(svg):
    """A generator's SVG fragment (a group or several) as an element tree."""
    return ET.fromstring(f"<g>{svg}</g>")


def find(root, gid, m=None):
    """The group with id `gid` and the transform from its contents to the drawing (its own and its ancestors')."""
    m = m or Affine()
    for el in root:
        here = parse_transform(el.get("transform")).then(m)
        if el.get("id") == gid:
            return el, here
        got = find(el, gid, here)
        if got:
            return got
    return None


def _f(el, k, default=0.0):
    v = el.get(k)
    return float(v) if v is not None else default


def shapes(el, m, skip=(), opacity=1.0, inside=False):
    """The shapes of `el` (a group or a shape), drawn by `m`, bottom to top: a list of (shape, points). Groups whose
    id is in `skip` are left out, and so is anything at no opacity."""
    out = []
    if el.get("id") in skip:
        return out
    if inside:
        m = parse_transform(el.get("transform")).then(m)
    opacity *= _f(el, "opacity", 1.0)
    if opacity <= 0:
        return out
    tag = el.tag
    if tag == "g":
        for child in el:
            out += shapes(child, m, skip, opacity, inside=True)
        return out
    if tag == "path":
        d = el.get("d")
    elif tag == "rect":
        rx = el.get("rx")
        ry = el.get("ry")
        rx = float(rx) if rx is not None else (float(ry) if ry is not None else 0.0)
        ry = float(ry) if ry is not None else rx
        d = rect_d(_f(el, "x"), _f(el, "y"), _f(el, "width"), _f(el, "height"), rx, ry)
    elif tag == "circle":
        d = ellipse_d(_f(el, "cx"), _f(el, "cy"), _f(el, "r"), _f(el, "r"))
    elif tag == "ellipse":
        d = ellipse_d(_f(el, "cx"), _f(el, "cy"), _f(el, "rx"), _f(el, "ry"))
    else:
        raise ValueError(f"unsupported element {tag}")
    data, pts = path(d, m)
    s = {"d": data}
    fill = el.get("fill", "#000")
    if fill != "none":
        s["fill"] = fill
    stroke = el.get("stroke", "none")
    if stroke != "none":
        s["stroke"] = stroke
        s["stroke_width"] = r2(_f(el, "stroke-width", 1.0) * m.scale())
        s["linecap"] = el.get("stroke-linecap", "butt")
        s["linejoin"] = el.get("stroke-linejoin", "miter")
    if opacity != 1.0:
        s["opacity"] = r2(opacity)
    out.append((s, pts))
    return out


def bounds(items):
    xs = [x for _, pts in items for x, _ in pts]
    ys = [y for _, pts in items for _, y in pts]
    return min(xs), min(ys), max(xs), max(ys)


def centre(items):
    x0, y0, x1, y1 = bounds(items)
    return r2((x0 + x1) / 2), r2((y0 + y1) / 2)


class Drawing:
    """One generator's drawing: `figure(pose)` gives its SVG for a pose; `ground` is where it stands."""

    def __init__(self, figure, ground):
        self.figure = figure
        self.norm = normal(ground)
        self._cache = {}

    def root(self, pose):
        key = json.dumps(pose, sort_keys=True)
        if key not in self._cache:
            self._cache[key] = tree(self.figure(pose))
        return self._cache[key]

    def group(self, pose, gid):
        got = find(self.root(pose), gid)
        if not got:
            raise KeyError(f"no group {gid!r} in pose {pose}")
        return got

    def art(self, pose, *gids, skip=()):
        """Groups' shapes in the art box, with their points."""
        items = []
        for gid in gids:
            el, m = self.group(pose, gid)
            items += shapes(el, m.then(self.norm), skip)
        return items

    def at(self, pose, gid):
        """Where a group's own (0, 0) is in the art box, and its turn in degrees."""
        _, m = self.group(pose, gid)
        x, y = m.then(self.norm).apply(0, 0)
        return (r2(x), r2(y)), r2(m.angle())

    def inner(self, pose, gid):
        """A group's contents in its own frame (without its place and turn), at the art box's scale."""
        el, _ = self.group(pose, gid)
        items = []
        for child in el:
            items += shapes(child, scale(SCALE), inside=True)
        return items


def plain(items):
    return [s for s, _ in items]


def relative(items, origin):
    """Shapes moved so `origin` (in the art box) is their (0, 0)."""
    m = translate(-origin[0], -origin[1])
    out = []
    for s, pts in items:
        t = dict(s)
        t["d"], _ = path(s["d"], m)
        out.append((t, [(x - origin[0], y - origin[1]) for x, y in pts]))
    return out


def drawing(items):
    """A value's drawing as the manifest writes it: one shape, or a list."""
    shapes_ = plain(items)
    return shapes_[0] if len(shapes_) == 1 else shapes_


# ------------------------------------------------------------------ the generators' CSS motion

def css_period(css, cls):
    """The seconds and delay of `.cls { animation: name Ns ... }` in a generator's CSS."""
    m = re.search(r"\." + re.escape(cls) + r"\b[^{]*\{\s*animation:\s*\w+\s+([\d.]+)s", css)
    delay = re.search(r"\." + re.escape(cls) + r"\s*\{\s*animation-delay:\s*(-?[\d.]+)s", css)
    return float(m[1]), float(delay[1]) if delay else 0.0


def css_stops(css, name):
    """The stops of `@keyframes name`: (percent, {"dx", "dy", "opacity"}), in order."""
    body = re.search(r"@keyframes " + re.escape(name) + r" \{(.*?\})\s*\}", css, re.S)[1]
    stops = []
    for at, decl in re.findall(r"([\d%, ]+?)\s*\{\s*([^}]*)\}", body):
        v = {}
        t = re.search(r"translate\(\s*(-?[\d.]+)(?:px)?\s*,\s*(-?[\d.]+)(?:px)?\s*\)", decl)
        ty = re.search(r"translateY\(\s*(-?[\d.]+)(?:px)?\s*\)", decl)
        if t:
            v["dx"], v["dy"] = float(t[1]), float(t[2])
        elif ty:
            v["dx"], v["dy"] = 0.0, float(ty[1])
        op = re.search(r"opacity:\s*([\d.]+)", decl)
        if op:
            v["opacity"] = float(op[1])
        for p in at.split(","):
            stops.append((float(p.strip().rstrip("%")), v))
    return sorted(stops, key=lambda s: s[0])


def held(css, cls, name):
    """A steps(1, end) animation as held keys: each stop lasts until the next (the last until 100 %)."""
    period, _ = css_period(css, cls)
    stops = [s for s in css_stops(css, name) if s[0] < 100]
    out = []
    for i, (p, v) in enumerate(stops):
        nxt = stops[i + 1][0] if i + 1 < len(stops) else 100.0
        out.append((round(period * 10 * (nxt - p)), v))
    return out


def bezier(x1, y1, x2, y2):
    """CSS's cubic-bezier easing as a function of progress."""

    def at(a, b, t):
        return 3 * a * t * (1 - t) ** 2 + 3 * b * t * t * (1 - t) + t ** 3

    def ease(u):
        lo, hi = 0.0, 1.0
        for _ in range(40):
            mid = (lo + hi) / 2
            if at(x1, x2, mid) < u:
                lo = mid
            else:
                hi = mid
        return at(y1, y2, (lo + hi) / 2)

    return ease


EASE_IN_OUT = bezier(0.42, 0, 0.58, 1)


def eased(css, cls, name, steps):
    """An ease-in-out animation sampled into `steps` held keys of its translateY, and each part's phase from its
    delay: (keys, period in ms, {class: phase})."""
    period, _ = css_period(css, cls)
    stops = css_stops(css, name)
    ms = round(period * 1000 / steps)
    out = []
    for i in range(steps):
        u = i / steps * 100
        for (p0, v0), (p1, v1) in zip(stops, stops[1:]):
            if p0 <= u <= p1:
                e = EASE_IN_OUT((u - p0) / (p1 - p0)) if p1 > p0 else 1.0
                out.append((ms, {"dy": v0["dy"] + (v1["dy"] - v0["dy"]) * e}))
                break
    return out


def phase(css, cls):
    period, delay = css_period(css, cls)
    return r2((-delay / period) % 1.0)


# ------------------------------------------------------------------ pack pieces

def keys(frames, extra=None):
    """Part animation keys from (ms, fields) pairs, in art px; `extra` fields are added to every key."""
    out = []
    for ms, f in frames:
        key = dict(extra or {})
        for k, v in f.items():
            key[k] = key.get(k, 0.0) + v
        key = {k: r2(v) for k, v in key.items() if not (k in ("dx", "dy", "rotate") and r2(v) == 0)
               and not (k in ("scale", "opacity") and r2(v) == 1)}
        key["ms"] = ms
        out.append(key)
    return out


def drawn(frames):
    """Keys in the drawing's units (dx, dy) brought to the art box."""
    return [(ms, {k: (v * SCALE if k in ("dx", "dy") else v) for k, v in f.items()}) for ms, f in frames]


def anim(parts, frames, mode="loop"):
    """A part animation: (id, phase) pairs, or one id."""
    if isinstance(parts, str):
        parts = [(parts, 0)]
    out = {"parts": [p for p, _ in parts], "loop": mode, "keys": frames}
    if any(ph for _, ph in parts):
        out["phase"] = [ph for _, ph in parts]
    return out


def scan_keys(extra=None):
    """The magnifier's scan as keys, for the lens and the hand that holds it."""
    return keys([(SCAN_MS, {"dx": s * pet_states.LENS_STEP * SCALE}) for s in SCAN], extra)


def layer(name, channel, default, **rest):
    out = {"name": name, "channel": channel}
    out.update(rest)
    out["default"] = default
    return out


def look():
    return {k: [r2(x * SCALE), r2(y * SCALE)] for k, (x, y) in LOOK.items()}


def eyes(d, pose_of, part_ids=("eye-left", "eye-right")):
    """The eyes layer: each expression drawn about the left eye's place (both eyes are the same drawing), and the
    look table. `pose_of(expr)` is the generator's pose for a pet_states expression."""
    origins = [d.at({}, pid)[0] for pid in part_ids]
    shapes_ = {}
    for value, expr in EYES.items():
        shapes_[value] = drawing(relative(d.art(pose_of(expr), part_ids[0]), origins[0]))
    return layer("eyes", "eyes", "open", hit=True,
                 parts=[{"id": pid, "origin": list(o)} for pid, o in zip(part_ids, origins)],
                 shapes=shapes_, look=look())


def feet(d, part_ids):
    """The feet layer: each foot one drawing about its standing centre, moved to where the generator puts it in
    each feet pose."""
    parts, shapes_, poses = [], {}, {value: {} for value in FEET}
    for pid in part_ids:
        items = d.art({}, pid)
        o = centre(items)
        parts.append({"id": pid, "origin": list(o), "shape": "foot"})
        shapes_["foot"] = drawing(relative(items, o))
        for value, pose in FEET.items():
            poses[value][pid] = list(centre(d.art(pose, pid)))
    return layer("feet", "feet", "stand", hit=True, parts=parts, shapes=shapes_, poses=poses, transition_ms=90)


def float_up(rise, steps, step_ms, drift=0.0):
    """Rising and fading out, then back."""
    return keys([(step_ms, {"dx": drift * i / steps, "dy": -rise * i / steps, "opacity": 1 - 0.8 * i / steps})
                 for i in range(steps)])


def effect_layers(accent, outline, norm, face=None, box=pet_states.AURA_BOX):
    """The effects, as pet_states draws them (every particle a part about its own centre), each value its own
    layer so a value with sparkles draws both; with a little motion of the pixel pet's kind. `face` is where the
    pet's tears start, `box` its glow (fnt-72); put [`aura_layer`] first, behind the pet."""

    def particles(fx):
        root = tree(pet_states.effects(fx, accent, outline, face, box))
        fxg, m = find(root, "fx")
        parts, shapes_ = [], {}
        for g in fxg:
            items = shapes(g, m.then(norm), inside=True)
            o = centre(items)
            parts.append({"id": g.get("id"), "origin": list(o), "shape": g.get("id")})
            shapes_[g.get("id")] = drawing(relative(items, o))
        return parts, shapes_

    def only(value, animation, **rest):
        parts, shapes_ = particles(value)
        nothing = {v: None for v in FX_VALUES if v not in (value, f"{value}+sparkles")}
        return layer(value, "fx", value, parts=parts, shapes=shapes_, animations={value: animation(parts)},
                     fallback={f"{value}+sparkles": value, **nothing}, **rest)

    def staggered(parts, frames):
        return anim([(p["id"], r2(i / len(parts))) for i, p in enumerate(parts)], frames)

    twinkle = keys([(120, {}), (120, {"scale": 0.8, "opacity": 0.85}), (120, {"scale": 0.6, "opacity": 0.6}),
                    (120, {"scale": 0.8, "opacity": 0.85})])
    drift = keys([(110, {"dx": -3 * i, "dy": -0.8 * i, "scale": 1 + 0.2 * i, "opacity": 1 - 0.22 * i})
                  for i in range(4)])
    sparkle_parts, sparkle_shapes = particles("sparkles")
    return [
        only("zs", lambda ps: staggered(ps, float_up(6, 6, 300, 2))),
        only("dots", lambda ps: staggered(ps, keys([(240, {}), (240, {"opacity": 0.6}), (240, {"opacity": 0.3})]))),
        only("notes", lambda ps: staggered(ps, float_up(8, 8, 180))),
        only("alert", lambda ps: anim(ps[0]["id"], keys([(180, {}), (180, {"dy": -1.5})]))),
        layer("sparkles", "fx", "sparkles", parts=sparkle_parts, shapes=sparkle_shapes,
              animations={"sparkles": staggered(sparkle_parts, twinkle)},
              fallback={v: None for v in ["none", "dust"] + FX + NEW_FX}),
        only("dust", lambda ps: staggered(ps, drift)),
        # fnt-72: the first key shows each cue (reduced motion)
        only("tears", lambda ps: anim([(p["id"], 0) for p in ps],
                                      keys([(400, {}), (300, {"dy": 3}), (300, {"dy": 6, "opacity": 0.7}),
                                            (300, {"dy": 9, "opacity": 0.3})]))),
        only("heart", lambda ps: staggered(ps, float_up(8, 6, 160))),
        only("aura", lambda ps: staggered(ps, twinkle)),
    ]


def aura_layer(norm, box=pet_states.AURA_BOX):
    """The golden glow that opens a done, behind the pet (the owner's soft glow, fnt-72 task 7): its three rings
    breathe once in, fully built at the first key."""
    root = tree(pet_states.aura_glow(box))
    g, m = find(root, "fxback")
    parts, shapes_ = [], {}
    for ring in g:
        items = shapes(ring, m.then(norm), inside=True)
        o = centre(items)
        parts.append({"id": ring.get("id"), "origin": list(o), "shape": ring.get("id")})
        shapes_[ring.get("id")] = drawing(relative(items, o))
    breathe = keys([(300, {}), (300, {"scale": 1.04}), (300, {"scale": 1.06, "opacity": 0.8}),
                    (300, {"scale": 1.04, "opacity": 0.5}), (300, {"opacity": 0.25})])
    return layer("aura-glow", "fx", "aura", parts=parts, shapes=shapes_,
                 animations={"aura": anim([(p["id"], 0) for p in parts], breathe)},
                 fallback={v: None for v in FX_VALUES if v != "aura"})


def caret(d, pose):
    """The laptop's cursor, a part of its own that blinks as the generator's CSS does."""
    items = d.art(pose, "caret")
    o = centre(items)
    return o, drawing(relative(items, o))


def write(folder, pack):
    os.makedirs(folder, exist_ok=True)
    with open(os.path.join(folder, "pack.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(pack, f, indent=1)
        f.write("\n")


def packs_dir(folder):
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(here, "..", "..", "rust", "crates", "aipet-sprite", "packs", folder)


def pack_arg(folder):
    """The folder to write the pack to when the generator runs with --pack [folder], else None."""
    import sys
    if "--pack" not in sys.argv:
        return None
    i = sys.argv.index("--pack")
    return sys.argv[i + 1] if i + 1 < len(sys.argv) else packs_dir(folder)


def named(pack, colours, theme=None, themes=None):
    """`pack` with its colours named: every fill, stroke, accent and glow that is one of `colours` (name to hex)
    becomes `$name`, and the pack gets a palette of the names it uses, which is its default theme. Where two names
    share a colour, the first wins, so each colour is named once. `theme` names the default theme; `themes` (theme
    name to {name: hex}) are its other themes, each keeping only the names the palette has."""
    by_hex = {}
    for name, hexc in colours.items():
        by_hex.setdefault(hexc.upper(), name)
    used = {}

    def name_of(c):
        name = by_hex.get(c.upper()) if isinstance(c, str) and c.startswith("#") else None
        if name is None:
            return c
        used[name] = colours[name]
        return "$" + name

    for layer in pack["layers"]:
        for drawing in layer.get("shapes", {}).values():
            for shape in drawing if isinstance(drawing, list) else [drawing]:
                for key in ("fill", "stroke"):
                    if key in shape:
                        shape[key] = name_of(shape[key])
    for key in ("accent", "glow"):
        pack[key] = name_of(pack[key])
    out = {}
    for key, value in pack.items():
        if key == "layers":
            out["palette"] = {name: used[name] for name in colours if name in used}
            if theme:
                out["theme"] = theme
            if themes:
                out["themes"] = {t: {name: c for name, c in cs.items() if name in used} for t, cs in themes.items()}
        out[key] = value
    return out
