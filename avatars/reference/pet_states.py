"""What every reference pet must show, shared by the generators (hood_gen.py, hamster_gen.py, sprout_gen.py).

STATES is every state the pixel pet supports today (rust/crates/aipet-sprite/src/pet.rs, compute_pose and draw): the
moods (idle with its five idle acts, sleep, thinking, working at the laptop, working with the magnifier for web
search and reading, attention, done, listen) and the reactions (hover, the hello wave, poke, landing, running while
dragged, held still, blink). Each state is a pose: the parts a pet draws (eyes, arms, feet, prop, headphones) plus
what this module draws on top (effects) and around it (whole-body offset and squash, the ground shadow, mirroring).

Effects, the magnifier and the headphones use the pixel pet's fixed colours (pet.rs, "fixed colours"); the notes take
the pet's accent. Everything is canvas-ready: plain shapes, flat fills and opacity, no filters or gradients.
"""
import os

# pet.rs fixed colours
RING, GLASS, HANDLE = "#D5DCEB", "#A9E4FF", "#7A5236"
SPARK, SPARK_HI, ZZ, DOT, BANG, DUSTC = "#FFE27A", "#FFFFFF", "#E3E8FF", "#FFFFFF", "#FFD24A", "#E6DED3"
HP_DARK, HP_CUP = "#26262C", "#3A3A43"
# fnt-72's fixed colours: they mean something, so themes never change them
TEAR, TEAR_HI = "#7CC8FF", "#E8F7FF"
AURA, AURA_HI, AURA_DEEP = "#FFC83D", "#FFF1B0", "#E39B12"
HEART, HEART_HI = "#FF6F91", "#FFD3DE"
PAPER, PAPER_SHADE = "#FFFDF4", "#E6DCC4"
# Where the golden glow sits about a pet that doesn't say (the owner's choice, "B: soft glow", fnt-72 task 7): centre
# and half sizes, on the 260 x 250 canvas.
AURA_BOX = (130, 132, 96, 104)

# Every state, in the order the pages show them. Pose keys: expr, look (dx, dy), arms (rest, wave, raised, hold,
# lens, run), feet (stand, tap, run, dangle), prop (None, laptop, lens, envelope), headphones, fx (None, zs, dots, notes,
# alert, sparkles, dust, tears, aura, heart), dx/dy (whole-body offset), sx/sy (squash about the ground), shadow
# (scale, opacity), mirror; fnt-72's yawn (tired) and blush (done and the poke's heart, for the pets that blush).
STATES = [
    ("idle", "mood", {}),
    ("idle: look around", "idle act", {"look": (-6, 0)}),
    ("idle: hop", "idle act", {"dy": -22, "sx": 0.97, "sy": 1.04, "shadow": (0.8, 0.7)}),
    ("idle: stretch", "idle act", {"arms": "raised", "expr": "happy", "dy": -8}),
    ("idle: wiggle", "idle act", {"dx": -8}),
    ("idle: foot tap", "idle act", {"feet": "tap", "look": (5, 0)}),
    ("sleep", "mood", {"expr": "sleep", "fx": "zs", "dy": 4}),
    ("thinking", "mood", {"expr": "up", "look": (2, 0), "fx": "dots", "dy": -5}),
    # working with no prop isn't a state: the board falls back to the laptop
    ("working: laptop", "mood", {"expr": "down", "look": (2, 0), "arms": "hold", "prop": "laptop"}),
    ("working: searching the web (magnifier)", "mood", {"arms": "lens", "prop": "lens", "scan": 0, "look": (-4, 0)}),
    ("attention", "mood", {"arms": "wave", "fx": "alert", "dy": -18, "shadow": (0.85, 0.8)}),
    ("done (first 4 s)", "mood", {"expr": "happy", "arms": "wave", "fx": "sparkles", "dy": -16, "blush": True}),
    ("done (after 4 s)", "mood", {"expr": "happy", "fx": "sparkles", "blush": True}),
    ("listen (music)", "mood", {"expr": "happy", "headphones": True, "fx": "notes", "dy": -6, "dx": 3}),
    ("music while busy (headphones)", "mood", {"expr": "down", "look": (2, 0), "arms": "hold", "prop": "laptop",
                                               "headphones": True}),
    # fnt-72's moods: a failed turn, late or long, a review arriving; and the aura that opens every done
    ("sad (a turn failed)", "mood", {"expr": "down", "fx": "tears", "dy": 3}),
    ("tired (yawning)", "mood", {"expr": "sleepy", "yawn": True}),
    ("news (a review arrived)", "mood", {"arms": "hold", "prop": "envelope", "dy": -3}),
    ("done: the aura (first 1.8 s)", "mood", {"expr": "happy", "arms": "wave", "fx": "aura", "dy": -16,
                                              "blush": True}),
    ("hover: eyes follow the pointer", "reaction", {"look": (-5, -4)}),
    ("hello wave", "reaction", {"expr": "happy", "arms": "wave"}),
    ("poked", "reaction", {"expr": "squint", "fx": "sparkles", "dy": -18, "sx": 1.04, "sy": 0.96}),
    ("poked: the heart", "reaction", {"expr": "squint", "fx": "heart", "dy": -18, "sx": 1.04, "sy": 0.96,
                                      "blush": True}),
    ("landing: in the air", "reaction", {"expr": "happy", "dy": -20, "shadow": (0.8, 0.7)}),
    ("landing: impact", "reaction", {"expr": "happy", "sx": 1.08, "sy": 0.9, "dy": 2}),
    ("dragged: running", "reaction", {"arms": "run", "feet": "run", "fx": "dust", "look": (6, 0), "dy": -6}),
    # held: the window follows the pointer, so the body doesn't lift; only the shadow shrinks
    ("held still", "reaction", {"expr": "wide", "arms": "raised", "feet": "dangle", "shadow": (0.55, 0.35)}),
    ("blink", "reaction", {"expr": "blink"}),
    ("mirrored (facing left)", "view", {"look": (5, 0), "mirror": True}),
]

# The second frame of each state that alternates or moves through phases (pet.rs: wave A/B, run A/B, dangle A/B,
# tap A/B, the magnifier's four-step scan). The timing stays in the behaviour layer; these are the extra poses.
# While searching, pet.rs runs the eyes ([-1, 0, 1, 0]) and the lens ([0, 1, 0, -1]) on offset cycles, so the scan
# frames pair them as it does: eyes left + lens centre (the main state), eyes centre + lens right, eyes right + lens
# centre, eyes centre + lens left.
PHASES = [
    ("hello wave: B", {"expr": "happy", "arms": "wave", "phase": 1}),
    ("dragged: running B", {"arms": "run", "feet": "run", "fx": "dust", "look": (6, 0), "dy": -2, "phase": 1}),
    ("held still: dangle B", {"expr": "wide", "arms": "raised", "feet": "dangle", "shadow": (0.55, 0.35),
                              "phase": 1}),
    ("idle: foot tap B", {"feet": "stand", "look": (5, 0)}),
    ("searching: scan 2 (eyes centre, lens right)", {"arms": "lens", "prop": "lens", "scan": 1, "look": (0, 0)}),
    ("searching: scan 3 (eyes right, lens centre)", {"arms": "lens", "prop": "lens", "scan": 0, "look": (4, 0)}),
    ("searching: scan 4 (eyes centre, lens left)", {"arms": "lens", "prop": "lens", "scan": -1, "look": (0, 0)}),
]


def el(tag, **a):
    attrs = " ".join(f'{k.replace("_", "-")}="{v}"' for k, v in a.items())
    return f"<{tag} {attrs}/>"


LENS_STEP = 10  # canvas units per scan step (pet.rs moves the lens one grid cell, -1..1)


def magnifier(scan=0):
    """The working prop for web search and reading: a ring, pale glass with a glint, a wooden handle. It is one
    movable part, `lens`, scanning left and right by `scan` steps (-1, 0, 1). Its handle end is at (232 + scan step,
    186), where a hand holds it."""
    return (f'<g id="prop"><g id="lens" transform="translate({scan * LENS_STEP} 0)">'
            + el("path", d="M222 176 L238 192", fill="none", stroke="#3B2618", stroke_width=12, stroke_linecap="round")
            + el("path", d="M222 176 L238 192", fill="none", stroke=HANDLE, stroke_width=7, stroke_linecap="round")
            + el("circle", cx=205, cy=158, r=26, fill=GLASS, opacity="0.55")
            + el("circle", cx=205, cy=158, r=26, fill="none", stroke="#30344A", stroke_width=10)
            + el("circle", cx=205, cy=158, r=26, fill="none", stroke=RING, stroke_width=6)
            + el("path", d="M192 148 C195 142 200 139 206 139", fill="none", stroke="#FFFFFF", stroke_width=4,
                 stroke_linecap="round", opacity="0.85")
            + "</g></g>")


def heart_path(x, y, s):
    """A heart about (x, y), `s` times 12 units wide each side."""
    return (f"M{x} {y + 8 * s} C{x - 12 * s} {y - 2 * s} {x - 6 * s} {y - 12 * s} {x} {y - 5 * s} "
            f"C{x + 6 * s} {y - 12 * s} {x + 12 * s} {y - 2 * s} {x} {y + 8 * s} Z")


def envelope(box, accent, outline):
    """The news prop: a white envelope held where the pet's hands hold the laptop, sealed with a heart in the pet's
    accent. `box` is its (left, top, right, bottom) on the canvas."""
    x0, y0, x1, y1 = box
    w, h, cx = x1 - x0, y1 - y0, (x0 + x1) / 2
    return ('<g id="prop">'
            + el("rect", x=x0, y=y0, width=w, height=h, rx=5, fill=PAPER, stroke=outline, stroke_width=4)
            + el("path", d=f"M{x0 + 4} {y1 - 4} L{cx} {y0 + h * 0.5} L{x1 - 4} {y1 - 4}", fill="none",
                 stroke=PAPER_SHADE, stroke_width=3, stroke_linejoin="round")
            + el("path", d=f"M{x0 + 2} {y0 + 2} L{cx} {y0 + h * 0.55} L{x1 - 2} {y0 + 2}", fill="none",
                 stroke=outline, stroke_width=3.5, stroke_linejoin="round")
            + el("path", d=heart_path(cx, y0 + h * 0.55, 0.62), fill=accent, stroke=outline, stroke_width=2.5,
                 stroke_linejoin="round")
            + "</g>")


def aura_glow(box=AURA_BOX):
    """The golden glow behind the pet that opens a done: three stacked gold ellipses, each a part of its own."""
    cx, cy, rx, ry = box
    rings = [(22, "0.10"), (12, "0.16"), (2, "0.24")]
    return ('<g id="fxback">'
            + "".join(f'<g id="glow-{i}">' + el("ellipse", cx=cx, cy=cy - 4, rx=rx + d, ry=ry + d, fill=AURA,
                                                  opacity=op) + "</g>"
                      for i, (d, op) in enumerate(rings, 1))
            + "</g>")


def headphones(left, right, cup_y, top_y, accent, outline):
    """A band over the head from cup to cup, and two cups at the head's sides."""
    cx = (left + right) / 2
    band = f"M{left} {cup_y - 8} C{left} {top_y} {right} {top_y} {right} {cup_y - 8}"
    cups = "".join(
        el("rect", x=x - 10, y=cup_y - 16, width=20, height=32, rx=8, fill=HP_CUP, stroke=outline, stroke_width=4)
        + el("rect", x=x - 4 + (3 if x < cx else -3), y=cup_y - 10, width=5, height=18, rx=2.5, fill=accent,
             opacity="0.9")
        for x in (left, right))
    return ('<g id="headphones">'
            + el("path", d=band, fill="none", stroke=outline, stroke_width=11, stroke_linecap="round")
            + el("path", d=band, fill="none", stroke=HP_DARK, stroke_width=6, stroke_linecap="round")
            + cups + "</g>")


def effects(fx, accent, outline, face=None, box=AURA_BOX):
    """The effects layer: drawn over the pet, at the pixel pet's places (top right, or behind a runner). Every
    particle is its own part with a stable id (z-1, dot-1, note-1, spark-1, dust-1, ...), so it can animate alone.
    `face` is where a tear starts (under the pet's screen-right eye), `box` the glow's (for the aura's stars)."""
    if not fx:
        return ""
    o = []

    def part(pid, inner):
        o.append(f'<g id="{pid}">{inner}</g>')

    if fx == "zs":
        for i, (x, y, s, op) in enumerate([(196, 64, 1.0, 1), (214, 40, 1.25, 1), (234, 14, 1.5, 0.6)], 1):
            d = f"M{x} {y} h{12 * s} l{-12 * s} {12 * s} h{12 * s}"
            part(f"z-{i}", el("path", d=d, fill="none", stroke=outline, stroke_width=7, stroke_linejoin="round",
                              stroke_linecap="round", opacity=op)
                 + el("path", d=d, fill="none", stroke=ZZ, stroke_width=3.5, stroke_linejoin="round",
                      stroke_linecap="round", opacity=op))
    elif fx == "dots":
        for i, (x, y, r) in enumerate([(205, 56, 5), (224, 36, 6.5), (244, 16, 8)], 1):
            part(f"dot-{i}", el("circle", cx=x, cy=y, r=r, fill=DOT, stroke=outline, stroke_width=3))
    elif fx == "notes":
        for i, (x, y, op) in enumerate([(206, 60, 1), (36, 44, 0.6)], 1):
            note = f"M{x + 10} {y} v26 M{x + 10} {y} l12 4 v8 l-12 -4"
            part(f"note-{i}", el("path", d=note, fill="none", stroke=outline, stroke_width=7, stroke_linecap="round",
                                 stroke_linejoin="round", opacity=op)
                 + el("path", d=note, fill="none", stroke=accent, stroke_width=3.5, stroke_linecap="round",
                      stroke_linejoin="round", opacity=op)
                 + el("ellipse", cx=x + 5, cy=y + 27, rx=7, ry=5.5, fill=accent, stroke=outline, stroke_width=3,
                      opacity=op))
    elif fx == "alert":
        part("alert", el("rect", x=236, y=8, width=12, height=34, rx=5, fill=BANG, stroke=outline, stroke_width=3.5)
             + el("circle", cx=242, cy=54, r=7, fill=BANG, stroke=outline, stroke_width=3.5))
    elif fx == "sparkles":
        for i, (x, y, s) in enumerate([(34, 66, 1.0), (224, 34, 1.2)], 1):
            star = (f"M{x} {y - 14 * s} Q{x + 2 * s} {y - 2 * s} {x + 14 * s} {y} Q{x + 2 * s} {y + 2 * s} {x} {y + 14 * s}"
                    f" Q{x - 2 * s} {y + 2 * s} {x - 14 * s} {y} Q{x - 2 * s} {y - 2 * s} {x} {y - 14 * s} Z")
            part(f"spark-{i}", el("path", d=star, fill=SPARK, stroke=outline, stroke_width=2.5, stroke_linejoin="round")
                 + el("circle", cx=x, cy=y, r=3 * s, fill=SPARK_HI))
    elif fx == "dust":
        for i, (x, y, r, op) in enumerate([(46, 222, 9, 0.75), (26, 212, 7, 0.6), (10, 202, 5, 0.45)], 1):
            part(f"dust-{i}", el("circle", cx=x, cy=y, r=r, fill=DUSTC, opacity=op))
    elif fx == "tears":
        # one on the cheek, one falling: the cue is there from the first frame (reduced motion)
        fx_, fy = face or (150, 150)
        for i, (x, y, s, op) in enumerate([(fx_, fy, 1.0, 1), (fx_ + 1, fy + 16, 0.75, 0.85)], 1):
            drop = (f"M{x} {y - 8 * s} C{x + 6 * s} {y - 1 * s} {x + 6 * s} {y + 6 * s} {x} {y + 7 * s} "
                    f"C{x - 6 * s} {y + 6 * s} {x - 6 * s} {y - 1 * s} {x} {y - 8 * s} Z")
            part(f"tear-{i}", el("path", d=drop, fill=TEAR, stroke=outline, stroke_width=2.5, stroke_linejoin="round",
                                 opacity=op)
                 + el("circle", cx=x - 1.5 * s, cy=y + 1.5 * s, r=1.6 * s, fill=TEAR_HI, opacity=op))
    elif fx == "heart":
        for i, (x, y, s) in enumerate([(222, 40, 1.25), (40, 70, 0.8)], 1):
            part(f"heart-{i}", el("path", d=heart_path(x, y, s), fill=HEART, stroke=outline, stroke_width=2.5,
                                  stroke_linejoin="round")
                 + el("circle", cx=x - 4 * s, cy=y - 4 * s, r=2 * s, fill=HEART_HI))
    elif fx == "aura":
        # the soft glow's stars over the head; the glow itself is behind the pet (aura_glow)
        cx, cy, rx, ry = box
        for i, (x, y, s) in enumerate([(cx - 52, cy - ry + 4, 0.8), (cx + 58, cy - ry - 6, 1.1),
                                       (cx + 8, cy - ry - 22, 0.7)], 1):
            star = (f"M{x} {y - 9 * s} Q{x + 1.5 * s} {y - 1.5 * s} {x + 9 * s} {y} Q{x + 1.5 * s} {y + 1.5 * s} {x} "
                    f"{y + 9 * s} Q{x - 1.5 * s} {y + 1.5 * s} {x - 9 * s} {y} Q{x - 1.5 * s} {y - 1.5 * s} {x} {y - 9 * s} Z")
            part(f"aura-star-{i}", el("path", d=star, fill=AURA_HI, stroke=AURA_DEEP, stroke_width=1.5,
                                      stroke_linejoin="round"))
    return '<g id="fx">' + "".join(o) + "</g>"


def compose(figure, shadow, ground, pose, accent, outline):
    """One frame: the ground shadow, then the figure and its effects together under the whole-body offset and squash
    (about the ground), as the app moves body and effect layers together; all mirrored when facing left. `figure` and
    `shadow` are the pet's SVG for this pose; `ground` its ground line."""
    dx, dy = pose.get("dx", 0), pose.get("dy", 0)
    sx, sy = pose.get("sx", 1), pose.get("sy", 1)
    ss, so = pose.get("shadow", (1, 1))
    body = (f'<g transform="translate(130 {ground}) scale({ss} {ss}) translate(-130 {-ground})" opacity="{so}">'
            f"{shadow}</g>"
            f'<g transform="translate({dx} {dy}) translate(130 {ground}) scale({sx} {sy}) translate(-130 {-ground})">'
            f"{aura_glow(pose.get('aura_box', AURA_BOX)) if pose.get('fx') == 'aura' else ''}{figure}"
            f"{effects(pose.get('fx'), accent, outline, pose.get('face'), pose.get('aura_box', AURA_BOX))}</g>")
    if pose.get("mirror"):
        body = f'<g transform="translate(260 0) scale(-1 1)">{body}</g>'
    return body


def svg(content, w=520, h=500):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 250" width="{w}" height="{h}">{content}</svg>'


def write_pages(out, prefix, title, frame, ref_png=None, ref_caption="pixel original", extra_css="",
                sheet_extras=()):
    """Writes <prefix>-preview.html (pixel original beside the vector, then every state in 130x120 boxes on white,
    mid-grey and near-black) and <prefix>-sheet.svg (every state as a labelled frame, then any `sheet_extras`, such
    as typing frames). `frame(pose)` returns one composed frame's SVG content."""
    import base64
    bgs = [("white", "#ffffff"), ("mid-grey", "#808080"), ("near-black", "#101214")]
    rows, group = [], None
    for name, kind, pose in list(STATES) + [(n, "phase", p) for n, p in PHASES]:
        if kind != group:
            group = kind
            rows.append(f'<tr><td colspan="4" class="grp">{ {"mood": "Moods", "idle act": "Idle acts (random, while idle)", "reaction": "Reactions", "view": "View", "phase": "Second frames and phases (states that alternate)"}[kind] }</td></tr>')
        rows.append(f"<tr><th>{name}</th>" + "".join(
            f'<td><div class="box" style="background:{bg}">{svg(frame(pose), 130, 125)}</div></td>' for _, bg in bgs)
            + "</tr>")
    ref = ""
    if ref_png and os.path.exists(ref_png):
        ref = "data:image/png;base64," + base64.b64encode(open(ref_png, "rb").read()).decode()
    html = f"""<!doctype html>
<html><head><meta charset="utf-8"><title>{title} review</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font: 14px system-ui, sans-serif; margin: 16px; background: #f4f4f4; color: #111; }}
  h2 {{ margin: 24px 0 8px; font-size: 16px; }}
  .cmp {{ display: flex; gap: 24px; align-items: flex-end; flex-wrap: wrap; }}
  .cmp figure {{ margin: 0; text-align: center; }}
  .cmp .stage {{ background: #101214; padding: 12px; display: flex; align-items: flex-end; justify-content: center;
                height: 240px; width: 260px; }}
  .ref {{ image-rendering: pixelated; max-height: 216px; max-width: 236px; }}
  table {{ border-collapse: collapse; }}
  th {{ text-align: right; padding: 4px 10px; font-weight: 500; white-space: nowrap; }}
  td {{ padding: 4px; }}
  td.grp {{ padding: 16px 4px 4px; font-weight: 600; color: #444; }}
  .box {{ width: 130px; height: 120px; overflow: hidden; outline: 1px dashed #bbb; }}
  .box svg {{ display: block; width: 130px; height: 125px; margin-top: -2.5px; }}
{extra_css}
</style></head><body>
<h2>Pixel original vs vector, at comparable character height (on near-black)</h2>
<div class="cmp">
  <figure><div class="stage"><img class="ref" src="{ref}" alt="{ref_caption}"></div><figcaption>{ref_caption}</figcaption></figure>
  <figure><div class="stage">{svg(frame({}), 228, 219)}</div><figcaption>vector {title}</figcaption></figure>
</div>
<h2>Every state the pet supports today, in 130×120 logical px boxes (dashed), on white, mid-grey and near-black</h2>
<table><tr><th></th>{"".join(f"<th style='text-align:left'>{n}</th>" for n, _ in bgs)}</tr>
{"".join(rows)}
</table>
</body></html>
"""
    with open(os.path.join(out, f"{prefix}-preview.html"), "w", encoding="utf-8", newline="\n") as f:
        f.write(html)

    cols, cw, ch = 6, 260, 280
    cells = []
    frames = list(STATES) + [(n, "phase", p) for n, p in PHASES] + [(n, "extra", p) for n, p in sheet_extras]
    for i, (name, _, pose) in enumerate(frames):
        x, y = (i % cols) * cw, (i // cols) * ch
        cells.append(f'<g transform="translate({x} {y})">'
                     + el("rect", x=4, y=4, width=cw - 8, height=ch - 8, rx=8, fill="#EDEDED", stroke="#C8C8C8")
                     + f'<g transform="translate(0 4)">{frame(pose)}</g>'
                     + f'<text x="{cw / 2}" y="{ch - 14}" text-anchor="middle" font-family="system-ui, sans-serif" '
                     + f'font-size="14" fill="#333">{name}</text></g>')
    rows_n = -(-len(frames) // cols)
    with open(os.path.join(out, f"{prefix}-sheet.svg"), "w", encoding="utf-8", newline="\n") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {cols * cw} {rows_n * ch}" '
                f'width="{cols * cw}" height="{rows_n * ch}">\n  <title>{title} state sheet</title>\n'
                f"  <!-- Every state the pet supports today, as frames of the vector {title}; generated from "
                f"pet_states.STATES. -->\n  " + "".join(cells) + "\n</svg>\n")
