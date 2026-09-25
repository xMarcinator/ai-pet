using Cell = (int X, int Y);

namespace AiPet;

/// What the UI tells the pet each frame.
public sealed class PetInput
{
    /// Mood from the chats: sleep, idle, thinking, working, attention, done.
    public string State = "sleep";
    /// Seconds (on the pet clock) the state has held.
    public double StateSince;
    /// "laptop" or "lens" while working.
    public string Prop;
    public bool Hover;
    /// Mouse position over the sprite in DIPs (0..GW*P, 0..GH*P), or null.
    public (double X, double Y)? Mouse;
    public bool Dragging, Moved;
    public double LastMove;
    public int Facing = 1;
    public double PokeUntil, LandUntil, WaveUntil, AlertUntil;
    public bool Music;
}

/// Motion + the three pixel layers for one frame.
public sealed class PetFrame
{
    public uint[] Body, Glow, Fx;   // premultiplied BGRA (0xAARRGGBB in memory order for little-endian)
    public bool PixelsChanged;
    public double X, Y, ScaleX = 1, ScaleY = 1;
    public double ShadowScale = 1, ShadowOpacity = 1;
}

/// The pixel pet: sprite geometry per avatar, the animation state machine, and the renderer.
/// Platform-neutral; the UI just blits the three layers (Glow gets a coloured blur on top).
public sealed class Pet
{
    public const int GW = 26, GH = 24, P = 5;

    // ------------------------------------------------------------------ fixed colours
    const uint LID = 0xFF2E3354, LID_HI = 0xFF3F477A, LID_IN = 0xFF14172E, BASE = 0xFFC9CFE2, BASE_SH = 0xFF8E95AE,
        RING = 0xFFD5DCEB, GLASS = 0x88A9E4FF, HANDLE = 0xFF7A5236,
        SPARK = 0xFFFFE27A, SPARK_HI = 0xFFFFFFFF, ZZ = 0xFFE3E8FF, DOT = 0xFFFFFFFF, BANG = 0xFFFFD24A, DUSTC = 0xB0E6DED3,
        HP_DARK = 0xFF26262C, HP_CUP = 0xFF3A3A43;

    // ------------------------------------------------------------------ limb and effect cells
    static readonly Cell[] FEET = Xs(21, 8, 9, 10, 15, 16, 17);
    static readonly Cell[] FOOT_TAP = Xs(21, 8, 9, 10).Concat(Xs(20, 16, 17, 18)).ToArray();
    static readonly Cell[] DANGLE_A = { (9, 22), (10, 22), (15, 21), (16, 21) };
    static readonly Cell[] DANGLE_B = { (9, 21), (10, 21), (15, 22), (16, 22) };
    static readonly Cell[] ARMS_DOWN = { (4, 15), (5, 15), (4, 16), (5, 16), (20, 15), (21, 15), (20, 16), (21, 16) };
    static readonly Cell[] ARM_L = ARMS_DOWN[..4];
    static readonly Cell[] WAVE_A = { (21, 10), (22, 10), (21, 11), (22, 11) };
    static readonly Cell[] WAVE_B = { (22, 9), (23, 9), (22, 10), (23, 10) };
    static readonly Cell[] ARMS_UP = new Cell[] { (3, 10), (4, 10), (3, 11), (4, 11) }.Concat(WAVE_A).ToArray();
    static readonly Cell[][] RUN_FEET = {
        new Cell[] { (17, 21), (18, 21), (19, 21), (7, 20), (8, 20), (9, 20) },
        new Cell[] { (16, 20), (17, 20), (18, 20), (6, 21), (7, 21), (8, 21) } };
    static readonly Cell[][] RUN_ARMS = {
        new Cell[] { (21, 12), (22, 12), (21, 13), (22, 13), (4, 16), (5, 16), (4, 17), (5, 17) },
        new Cell[] { (20, 16), (21, 16), (20, 17), (21, 17), (3, 13), (4, 13), (3, 14), (4, 14) } };
    static readonly Cell[] DUST = { (4, 21), (2, 20), (0, 19) };
    static readonly Cell[] ZGLYPH = { (0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2) };
    static readonly Cell[] BLUSH_CELLS = { (6, 13), (7, 13), (18, 13), (19, 13) };
    static readonly Cell[] NOTE = { (2, 0), (3, 0), (3, 1), (2, 1), (2, 2), (2, 3), (0, 3), (1, 3), (0, 4), (1, 4) };

    static Cell[] Xs(int y, params int[] xs) => xs.Select(x => (x, y)).ToArray();

    // ------------------------------------------------------------------ avatar geometry
    public Avatar Avatar { get; private set; }
    HashSet<Cell> body = new(), face = new();
    Cell[] spec = Array.Empty<Cell>(), hpBand = Array.Empty<Cell>(), hpCups = Array.Empty<Cell>(), hpShine = Array.Empty<Cell>();

    public void SetAvatar(Avatar a)
    {
        Avatar = a;
        body = BuildBody(a.Shapes);
        face = BuildFace(a.FaceTop, a.FaceBottom);
        spec = FindSpec(body);
        BuildHeadphones(body, out hpBand, out hpCups, out hpShine);
        lastDrawFrame = -1;
    }

    static HashSet<Cell> BuildBody(double[][] shapes)
    {
        var s = new HashSet<Cell>();
        for (int y = 0; y < GH; y++)
            for (int x = 0; x < GW; x++)
            {
                double cx = x + 0.5, cy = y + 0.5;
                foreach (var e in shapes)
                {
                    double dx = (cx - e[0]) / e[2], dy = (cy - e[1]) / e[3];
                    if (dx * dx + dy * dy <= 1) { s.Add((x, y)); break; }
                }
            }
        return s;
    }

    static HashSet<Cell> BuildFace(int top, int bottom)
    {
        var s = new HashSet<Cell>();
        for (int x = 8; x < 18; x++) for (int y = top; y <= bottom; y++) s.Add((x, y));
        s.ExceptWith(new Cell[] { (8, top), (17, top), (8, bottom), (17, bottom) });
        return s;
    }

    /// Top-left-most body cells on each bump: little shine pixels for the "soft" style.
    static Cell[] FindSpec(HashSet<Cell> body) =>
        body.Where(c => !body.Contains((c.X, c.Y - 1)) && body.Contains((c.X + 1, c.Y + 1)) && c.X < 13 && c.Y < 9)
            .OrderBy(c => c.Y).Take(3).Select(c => (c.X + 1, c.Y + 1)).ToArray();

    /// Headphones that hug the head: ear cups at the sides of rows 9–12, a band just above the silhouette.
    static void BuildHeadphones(HashSet<Cell> body, out Cell[] band, out Cell[] cups, out Cell[] shine)
    {
        int Left(int y) => body.Where(c => c.Y == y).Select(c => c.X).DefaultIfEmpty(5).Min();
        int Right(int y) => body.Where(c => c.Y == y).Select(c => c.X).DefaultIfEmpty(20).Max();
        int lx = Left(10), rx = Right(10);
        var cupList = new List<Cell>();
        for (int y = 9; y <= 12; y++) { cupList.Add((lx - 1, y)); cupList.Add((lx, y)); cupList.Add((rx, y)); cupList.Add((rx + 1, y)); }
        var b = new HashSet<Cell>();
        int? prev = null;
        for (int x = lx; x <= rx; x++)
        {
            var col = body.Where(c => c.X == x).Select(c => c.Y).DefaultIfEmpty(9).Min();
            int y = Math.Min(col - 1, 8);
            b.Add((x, y));
            if (prev is int py)
                for (int yy = Math.Min(py, y) + 1; yy < Math.Max(py, y); yy++) b.Add((py < y ? x - 1 : x, yy));
            prev = y;
        }
        for (int y = b.Where(c => c.X == lx).Select(c => c.Y).DefaultIfEmpty(8).Min() + 1; y < 9; y++) b.Add((lx, y));
        for (int y = b.Where(c => c.X == rx).Select(c => c.Y).DefaultIfEmpty(8).Min() + 1; y < 9; y++) b.Add((rx, y));
        band = b.ToArray();
        cups = cupList.ToArray();
        shine = new Cell[] { (lx - 1, 10), (lx - 1, 11), (rx + 1, 10), (rx + 1, 11) };
    }

    static Cell[] Mirror(IEnumerable<Cell> cells, int facing) =>
        facing > 0 ? cells.ToArray() : cells.Select(c => (GW - 1 - c.X, c.Y)).ToArray();

    static HashSet<Cell> Outline(ICollection<Cell> cells)
    {
        var ring = new HashSet<Cell>();
        foreach (var (x, y) in cells)
            foreach (var n in new Cell[] { (x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1) })
                if (!cells.Contains(n)) ring.Add(n);
        return ring;
    }

    static List<Cell> EyeCells(string kind, int lx)
    {
        int L = 10 + lx, R = 14 + lx;
        IEnumerable<Cell> Block(int y0, int y1) =>
            from x in new[] { L, L + 1, R, R + 1 } from y in Enumerable.Range(y0, y1 - y0 + 1) select (x, y);
        return kind switch
        {
            "wide" => Block(10, 13).ToList(),
            "squint" => new List<Cell> { (L, 10), (L + 1, 11), (L, 12), (R + 1, 10), (R, 11), (R + 1, 12) },
            "blink" => new List<Cell> { (L, 12), (L + 1, 12), (R, 12), (R + 1, 12) },
            "happy" => new List<Cell> { (L - 1, 11), (L, 10), (L + 1, 11), (R, 11), (R + 1, 10), (R + 2, 11) },
            "sleep" => new List<Cell> { (L - 1, 12), (L, 12), (L + 1, 12), (R, 12), (R + 1, 12), (R + 2, 12) },
            "up" => Block(10, 11).ToList(),
            "down" => Block(12, 13).ToList(),
            _ => Block(10, 12).ToList(),
        };
    }

    // ------------------------------------------------------------------ pixel buffers
    readonly uint[] bodyPx = new uint[GW * GH], glowPx = new uint[GW * GH], fxPx = new uint[GW * GH];

    static uint Pm(uint argb)
    {
        uint a = argb >> 24;
        if (a == 255) return argb;
        uint r = ((argb >> 16) & 255) * a / 255, g = ((argb >> 8) & 255) * a / 255, b = (argb & 255) * a / 255;
        return (a << 24) | (r << 16) | (g << 8) | b;
    }

    static void Put(uint[] buf, Cell c, uint color)
    {
        if (c.X >= 0 && c.X < GW && c.Y >= 0 && c.Y < GH) buf[c.Y * GW + c.X] = Pm(color);
    }

    static void PutAll(uint[] buf, IEnumerable<Cell> cells, uint color)
    {
        foreach (var c in cells) Put(buf, c, color);
    }

    void Outlined(uint[] buf, IEnumerable<Cell> cells, uint color)
    {
        var set = cells.ToHashSet();
        PutAll(buf, Outline(set), Avatar.Outline);
        PutAll(buf, set, color);
    }

    // ------------------------------------------------------------------ animation state
    readonly Random rng = new();
    double R(double a, double b) => a + rng.NextDouble() * (b - a);
    string idleAct, animState = "sleep";
    double idleStart, idleUntil, nextIdle = 6, blinkAt = 3, lookAt = 4, lastFrame, prevY, squash;
    int look, lastDrawFrame = -1;

    /// The state the pet is animating (e.g. "listen" when music replaces idling).
    public string AnimState => animState;

    sealed class Pose
    {
        public string Eyes = "normal";
        public int Lx;
        public Cell[] Arms = ARMS_DOWN, Feet = FEET;
        public string Prop;
        public double X, Y;
        public bool Running, Held, Poked, Headphones;
    }

    Pose ComputePose(PetInput inp, double t)
    {
        var p = new Pose();
        string st = t < inp.AlertUntil ? "attention" : inp.State;
        p.Headphones = inp.Music;
        // with nothing to do and music on, the pet listens along instead of idling or napping
        if (inp.Music && (st is "idle" or "sleep" || (st == "done" && t - inp.StateSince > 4))) st = "listen";
        if (st != animState) idleAct = null;
        animState = st;
        double since = t - inp.StateSince;
        bool drg = inp.Dragging && inp.Moved;
        bool running = drg && t - inp.LastMove < 0.25;
        bool landing = t < inp.LandUntil;
        bool waving = t < inp.WaveUntil && !drg;
        p.Poked = t < inp.PokeUntil;
        p.Prop = st == "working" ? (inp.Prop ?? "laptop") : null;

        if (t > lookAt) { look = new[] { -1, 0, 0, 1 }[rng.Next(4)]; lookAt = t + R(2.5, 6); }
        Cell[] Wave(double rate) => ARM_L.Concat((int)(t * rate) % 2 == 1 ? WAVE_A : WAVE_B).ToArray();

        switch (st)
        {
            case "idle":
                p.Y = (0.5 + 0.5 * Math.Sin(t * 2.2)) * 0.6 * P;
                p.Lx = look;
                if (!inp.Hover && !drg)
                {
                    if (idleAct == null && t > nextIdle)
                    {
                        idleAct = new[] { "look", "hop", "stretch", "wiggle", "tap" }[rng.Next(5)];
                        idleStart = t;
                        idleUntil = t + (idleAct == "hop" ? 0.9 : R(1.6, 2.6));
                    }
                    if (idleAct != null && t > idleUntil) { idleAct = null; nextIdle = t + R(5, 11); }
                    switch (idleAct)
                    {
                        case "look": p.Lx = new[] { -2, -1, 0, 1, 2, 1, 0, -1 }[(int)(t * 4) % 8]; break;
                        case "hop": p.Y = -Math.Sin(Math.PI * (t - idleStart) / (idleUntil - idleStart)) * 3.2 * P; break;
                        case "stretch": p.Arms = ARMS_UP; p.Eyes = "happy"; p.Y = -P; break;
                        case "wiggle": p.X = Math.Sin(t * 16) * 0.8 * P; break;
                        case "tap": p.Feet = (int)(t * 5) % 2 == 1 ? FOOT_TAP : FEET; p.Lx = 1; break;
                    }
                }
                break;
            case "listen":
                // nod to a ~112 bpm beat, sway every other bar, eyes mostly closed in bliss
                const double beat = 60.0 / 112;
                p.Y = -Math.Abs(Math.Sin(Math.PI * t / beat)) * 1.2 * P;
                p.X = Math.Sin(Math.PI * t / (beat * 2)) * 0.6 * P;
                p.Eyes = (int)(t / beat) % 8 < 6 ? "happy" : "normal";
                break;
            case "sleep":
                p.Y = (0.5 + 0.5 * Math.Sin(t * 1.2)) * 0.5 * P;
                p.Eyes = "sleep";
                break;
            case "thinking":
                p.Y = -(0.5 + 0.5 * Math.Sin(t * 2.4)) * 0.7 * P;
                p.Eyes = "up"; p.Lx = 1;
                break;
            case "working":
                p.Y = -Math.Abs(Math.Sin(t * 6.3)) * 0.6 * P;
                if (p.Prop == "lens") p.Lx = new[] { -1, 0, 1, 0 }[(int)(t * 1.5) % 4];
                else { p.Eyes = "down"; p.Lx = 1; }
                break;
            case "attention":
                double ph = t % 1.2;
                p.Y = ph < 0.6 ? -Math.Sin(Math.PI * ph / 0.6) * 3 * P : 0;
                p.Arms = Wave(3);
                break;
            case "done":
                p.Eyes = "happy";
                p.Y = since < 0.7 ? -Math.Sin(Math.PI * since / 0.7) * 3 * P : 0;
                if (since < 4) p.Arms = Wave(3);
                break;
        }

        if (inp.Hover && inp.Mouse is { } m && !drg && st != "working")
        {
            p.Lx = Math.Clamp((int)Math.Round((m.X - 13 * P) / 28.0), -2, 2);
            p.Eyes = m.Y < 11 * P - 40 ? "up" : "normal";
            if (st == "sleep") p.Y = 0;
        }
        if (waving && st is "idle" or "sleep" or "done") { p.Arms = Wave(5); p.Eyes = "happy"; }
        if (p.Poked)
        {
            double pp = (inp.PokeUntil - t) / 1.2;
            p.Eyes = pp > 0.6 ? "squint" : "happy";
            p.Y = -Math.Sin(pp * Math.PI) * 3 * P; p.X = 0;
        }
        if (landing)
        {
            double pp = 1 - (inp.LandUntil - t) / 0.45;
            p.Y = -Math.Sin(Math.PI * pp) * 2.5 * P; p.Eyes = "happy";
        }
        if (running)
        {
            int f = (int)(t * 10) % 2;
            p.Feet = Mirror(RUN_FEET[f], inp.Facing); p.Arms = Mirror(RUN_ARMS[f], inp.Facing);
            p.Y = -Math.Abs(Math.Sin(t * 10 * Math.PI)) * P; p.X = inp.Facing * P;
            p.Eyes = "normal"; p.Lx = 2 * inp.Facing; p.Prop = null; p.Running = true;
        }
        else if (drg)
        {
            p.Feet = (int)(t * 3) % 2 == 1 ? DANGLE_A : DANGLE_B;
            p.Arms = ARMS_UP; p.Eyes = "wide"; p.Lx = 0; p.Y = 0; p.X = 0; p.Prop = null; p.Held = true;
        }

        if (p.Eyes is "normal" or "up" or "down" && t > blinkAt)
        {
            p.Eyes = "blink";
            if (t > blinkAt + 0.13) blinkAt = t + R(2.5, 5.5);
        }
        return p;
    }

    /// Advance to time t (seconds) and produce the frame. Pixels are redrawn at 24 fps; motion every call.
    public PetFrame Update(PetInput inp, double t)
    {
        double dt = Math.Clamp(t - lastFrame, 0.001, 0.05);
        lastFrame = t;
        var pose = ComputePose(inp, t);
        var frame = new PetFrame { Body = bodyPx, Glow = glowPx, Fx = fxPx };

        int n = (int)(t * 24);
        if (n != lastDrawFrame)
        {
            lastDrawFrame = n;
            Draw(pose, inp, t);
            frame.PixelsChanged = true;
        }

        // squash & stretch
        double v = (pose.Y - prevY) / dt;
        if (prevY < -2 && pose.Y >= -0.4 && v > 0) squash = 0.13;
        squash *= Math.Exp(-11 * dt);
        double stretch = Math.Clamp(-v / 2600, 0, 0.06);
        double breathe = inp.State is "idle" or "sleep" && !inp.Dragging ? 0.012 * Math.Sin(t * (inp.State == "sleep" ? 1.2 : 2.2)) : 0;
        prevY = pose.Y;
        frame.ScaleX = 1 + squash - stretch * 0.5 - breathe * 0.5;
        frame.ScaleY = 1 - squash + stretch + breathe;
        frame.X = pose.X;
        frame.Y = pose.Y;
        double lift = Math.Clamp(1 + pose.Y / 45, 0.5, 1);
        frame.ShadowScale = pose.Held ? 0.55 : lift;
        frame.ShadowOpacity = pose.Held ? 0.35 : lift;
        return frame;
    }

    void Draw(Pose p, PetInput inp, double t)
    {
        var a = Avatar;
        Array.Clear(bodyPx); Array.Clear(glowPx); Array.Clear(fxPx);

        var shape = new HashSet<Cell>(body);
        shape.UnionWith(p.Arms);
        shape.UnionWith(p.Feet);
        PutAll(bodyPx, Outline(shape), a.Outline);
        foreach (var (x, y) in shape)
        {
            bool below = shape.Contains((x, y + 1)), right = shape.Contains((x + 1, y));
            bool above = shape.Contains((x, y - 1)), left = shape.Contains((x - 1, y));
            uint col;
            if (a.Shading == "rim")
            {
                // dark figure, lit from behind: bright edge along the top and sides, darker towards the bottom
                bool edge = !below || !right || !above || !left;
                col = edge ? (y < 17 ? a.Hi : a.Shade) : y >= 18 ? a.Deep : y >= 14 ? a.Shade : a.Body;
            }
            else if (!below || !right) col = y >= 19 ? a.Deep : a.Shade;
            else if ((!above || !left) && y < 15) col = a.Hi;
            else col = y >= 19 ? a.Shade : a.Body;
            Put(bodyPx, (x, y), col);
        }
        if (a.Shading == "rim")
            foreach (var c in shape.Where(c => !shape.Contains((c.X, c.Y - 1)) && c.Y < 8)) Put(bodyPx, c, a.Spec);
        else
            foreach (var c in spec) if (shape.Contains(c)) Put(bodyPx, c, a.Spec);
        if (a.Blush) foreach (var c in BLUSH_CELLS) if (shape.Contains(c)) Put(bodyPx, c, a.BlushC);

        if (p.Headphones)
        {
            var hp = hpBand.Concat(hpCups).ToHashSet();
            foreach (var c in Outline(hp)) if (!shape.Contains(c)) Put(bodyPx, c, a.Outline);
            PutAll(bodyPx, hpBand, HP_DARK);
            PutAll(bodyPx, hpCups, HP_CUP);
            PutAll(bodyPx, hpShine, a.Accent | 0xFF000000);  // cups carry the avatar's theme colour
        }
        foreach (var c in face)
        {
            bool edge = !face.Contains((c.X + 1, c.Y)) || !face.Contains((c.X - 1, c.Y)) ||
                        !face.Contains((c.X, c.Y + 1)) || !face.Contains((c.X, c.Y - 1));
            Put(bodyPx, c, edge ? a.Bezel : a.Screen);
        }
        Put(bodyPx, (9, a.FaceTop + 1), a.Glint);
        Put(bodyPx, (10, a.FaceTop + 1), a.Glint);

        var eyes = EyeCells(p.Eyes, p.Lx);
        var eyeSet = eyes.ToHashSet();
        foreach (var (x, y) in eyes)
        {
            bool up = eyeSet.Contains((x, y - 1)), dn = eyeSet.Contains((x, y + 1));
            Put(glowPx, (x, y), !up && dn ? a.EyeHi : up && !dn ? a.EyeLo : a.Eye);
        }

        // props
        if (p.Prop != null && !p.Poked && t >= inp.LandUntil)
        {
            if (p.Prop == "lens")
            {
                int sx = new[] { 0, 1, 0, -1 }[(int)(t * 1.5) % 4];
                var ring = new Cell[] { (19, 13), (20, 13), (21, 13), (18, 14), (22, 14), (18, 15), (22, 15), (18, 16), (22, 16), (19, 17), (20, 17), (21, 17) }
                    .Select(c => (c.X + sx, c.Y)).ToHashSet();
                var glass = (from x in new[] { 19, 20, 21 } from y in new[] { 14, 15, 16 } select (x + sx, y)).ToHashSet();
                PutAll(bodyPx, Outline(ring.Union(glass).ToHashSet()), a.Outline);
                PutAll(bodyPx, glass, GLASS);
                Put(bodyPx, (19 + sx, 14), 0xCCFFFFFF);
                PutAll(bodyPx, ring, RING);
                Outlined(bodyPx, new Cell[] { (23 + sx, 18), (24 + sx, 19) }, HANDLE);
            }
            else
            {
                var lid = (from x in Enumerable.Range(14, 8) from y in Enumerable.Range(15, 6) select (x, y)).ToHashSet();
                var baseRow = Enumerable.Range(12, 11).Select(x => (x, 21)).ToHashSet();
                PutAll(bodyPx, Outline(lid.Union(baseRow).ToHashSet()), a.Outline);
                PutAll(bodyPx, lid, LID);
                PutAll(bodyPx, Enumerable.Range(14, 8).Select(x => (x, 15)), LID_HI);
                PutAll(bodyPx, from x in Enumerable.Range(15, 6) from y in Enumerable.Range(16, 4) select (x, y), LID_IN);
                PutAll(bodyPx, baseRow, BASE);
                PutAll(bodyPx, new Cell[] { (21, 21), (22, 21) }, BASE_SH);
                PutAll(glowPx, new Cell[] { (16, 16), (17, 17), (16, 18) }, a.Eye);
                if ((int)(t * 2.5) % 2 == 1) PutAll(glowPx, new Cell[] { (18, 18), (19, 18) }, a.Eye);
            }
        }

        // effects
        uint acc = a.Accent | 0xFF000000;
        if (p.Running)
        {
            var dust = Mirror(DUST, inp.Facing);
            for (int i = 0; i < dust.Length; i++)
                if (((int)(t * 8) + i) % 3 != 0) Put(fxPx, dust[i], DUSTC);
        }
        else if (animState == "thinking" && !p.Held)
        {
            int n = (int)(t * 3) % 4;
            var dots = new Cell[] { (20, 5), (22, 3), (24, 1) };
            for (int i = 0; i < n; i++) Outlined(fxPx, new[] { dots[i] }, DOT);
        }
        else if (animState == "listen" && !p.Held)
        {
            for (int i = 0; i < 2; i++)
            {
                double ph = (t * 0.45 + i * 0.5) % 1;
                int bx = (i == 0 ? 19 : 2) + (int)Math.Round(ph * (i == 0 ? 3 : -2)), by = 7 - (int)Math.Round(ph * 7);
                PutAll(fxPx, NOTE.Select(n => (bx + n.X, by + n.Y)), ph > 0.7 ? (acc & 0x00FFFFFF) | 0x88000000 : acc);
            }
        }
        else if (animState == "sleep" && !(inp.Hover || p.Held))
        {
            for (int i = 0; i < 3; i++)
            {
                double ph = (t * 0.35 + i / 3.0) % 1;
                int bx = 19 + (int)Math.Round(ph * 4), by = 6 - (int)Math.Round(ph * 6);
                PutAll(fxPx, ZGLYPH.Select(g => (bx + g.X, by + g.Y)), ph > 0.75 ? 0x99E3E8FF : ZZ);
            }
        }
        else if (animState == "attention" && !p.Held)
            Outlined(fxPx, new Cell[] { (24, 1), (24, 2), (24, 3), (24, 5) }, BANG);

        if ((animState == "done" || p.Poked) && !p.Held)
        {
            bool on = (int)(t * 4) % 2 == 1;
            foreach (var (x, y) in on ? new Cell[] { (3, 6), (22, 3) } : new Cell[] { (2, 9), (23, 6) })
            {
                PutAll(fxPx, new Cell[] { (x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1) }, SPARK);
                Put(fxPx, (x, y), SPARK_HI);
            }
        }
    }
}
