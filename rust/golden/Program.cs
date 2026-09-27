using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace AiPet.Golden;

/// Golden data for the Rust port of the pet (rust/crates/aipet-sprite): the real AiPet.Core renders a fixed list of
/// cases and the result goes to rust/crates/aipet-sprite/tests/golden/frames.json, which tests/golden.rs replays.
///
///   dotnet run --project rust/golden -c Release
///
/// A case is an avatar, a PetInput (optionally replaced at given steps: "phases") and Update(t) at fixed steps from
/// t = 0 (t = step / rate), or at listed times. Sampled steps record the three layers, PixelsChanged, the motion,
/// AnimState and the idle act in progress. The file also records Avatar.All() over a folder of test avatar files,
/// TryParseColor and Pm.
///
/// Chance never enters the file: the pet's Random is replaced with one that throws. Pet.ComputePose consults it only
/// once t passes lookAt (4), nextIdle (6) or blinkAt + 0.13 (3.13), so the cases stop before 3.13 s (most before 3).
/// The "scripted-*" cases run longer, for the idle acts, glances and blinks: there the Random is ScriptedRandom, a
/// counter-based sequence the Rust test reproduces exactly.
/// Nothing touches the user's pet: AIPET_DATA_DIR points Core at a temp folder before Paths is first used.
static class Program
{
    const string Sprout = "Sprout", Hood = "Hood", Green = "Hood (green)";

    static readonly JsonSerializerOptions Json = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    static int Main()
    {
        string repo = RepoRoot();
        string data = Path.Combine(Path.GetTempPath(), "aipet-golden-" + Guid.NewGuid().ToString("N"));
        // before anything reads Paths.DataDir (Generate checks)
        Environment.SetEnvironmentVariable("AIPET_DATA_DIR", data);
        try
        {
            Generate(repo, data);
            return 0;
        }
        finally
        {
            if (Directory.Exists(data)) Directory.Delete(data, recursive: true);
        }
    }

    static string RepoRoot([CallerFilePath] string self = "") =>
        Path.GetFullPath(Path.Combine(Path.GetDirectoryName(self), "..", ".."));

    [MethodImpl(MethodImplOptions.NoInlining)]
    static void Generate(string repo, string data)
    {
        if (Path.GetFullPath(Paths.DataDir) != Path.GetFullPath(data))
            throw new InvalidOperationException($"AiPet.Core reads {Paths.DataDir}, not the temp folder {data}");

        var files = AvatarFiles(repo);
        Directory.CreateDirectory(Avatar.CustomDir);
        foreach (var f in files) File.WriteAllBytes(Path.Combine(Avatar.CustomDir, f.File), f.Bytes);
        var all = Avatar.All();
        var byName = all.ToDictionary(a => a.Name);
        foreach (var name in new[] { Sprout, Hood, Green })
            if (!byName.ContainsKey(name)) throw new InvalidOperationException($"no avatar {name}");

        var cases = Cases(all);
        var layers = new List<string>();
        var layerIndex = new Dictionary<string, int>();
        int Layer(uint[] px)
        {
            var s = Rle(px);
            if (!layerIndex.TryGetValue(s, out int i)) { layerIndex[s] = i = layers.Count; layers.Add(s); }
            return i;
        }

        var caseLines = new List<string>();
        // idle acts a sample shows (the act is applied: idle, not hovered or held, and the pixels were redrawn)
        var idleActs = new SortedDictionary<string, int>();
        int samples = 0, unchanged = 0;
        var states = new SortedSet<string>();
        foreach (var c in cases)
        {
            var pet = NewPet(c.Seed);
            pet.SetAvatar(byName[c.Avatar]);
            PetInput input = null;
            var lines = new List<string>();
            int n = c.Times?.Length ?? c.Steps;
            for (int i = 0; i < n; i++)
            {
                double t = c.Times != null ? c.Times[i] : i / c.Rate;
                foreach (var ph in c.Phases.Where(p => p.Step == i))
                {
                    input = ph.Input;
                    if (ph.Avatar != null) pet.SetAvatar(byName[ph.Avatar]);
                }
                var f = pet.Update(input, t);
                if (!c.Samples.Contains(i)) continue;
                var act = (string)IdleAct.GetValue(pet);
                if (act != null && pet.AnimState == "idle" && !input.Hover && !(input.Dragging && input.Moved) && f.PixelsChanged)
                    idleActs[act] = idleActs.GetValueOrDefault(act) + 1;
                lines.Add(J(new
                {
                    step = i, t,
                    body = Layer(f.Body), glow = Layer(f.Glow), fx = Layer(f.Fx),
                    changed = f.PixelsChanged,
                    x = f.X, y = f.Y, scale_x = f.ScaleX, scale_y = f.ScaleY,
                    shadow_scale = f.ShadowScale, shadow_opacity = f.ShadowOpacity,
                    anim_state = pet.AnimState,
                    idle_act = act,
                }));
                samples++;
                if (!f.PixelsChanged) unchanged++;
                states.Add(pet.AnimState);
            }
            var head = new Dictionary<string, object> { ["name"] = c.Name, ["avatar"] = c.Avatar };
            if (c.Seed is int seed) head["random_seed"] = seed;
            if (c.Times != null)
            {
                head["times"] = c.Times;
                head["times_bits"] = c.Times.Select(t => BitConverter.DoubleToInt64Bits(t).ToString("X16")).ToArray();
            }
            else
            {
                head["rate"] = c.Rate;
                head["steps"] = c.Steps;
            }
            head["phases"] = c.Phases.Select(p => new { step = p.Step, avatar = p.Avatar, input = InputJson(p.Input) }).ToArray();
            var headJson = J(head);
            caseLines.Add(headJson[..^1] + ",\"samples\":[\n      " + string.Join(",\n      ", lines) + "\n    ]}");
        }

        var pm = typeof(Pet).GetMethod("Pm", BindingFlags.NonPublic | BindingFlags.Static)
                 ?? throw new InvalidOperationException("Pet.Pm not found");
        var pmPairs = PmInputs().Select(v => new[] { v, (uint)pm.Invoke(null, [v]) }).ToArray();
        var colors = ColorInputs().Select(s => new { input = s, argb = Avatar.TryParseColor(s, out uint c) ? c.ToString("X8") : null });

        var sb = new StringBuilder();
        sb.Append("{\n");
        sb.Append("  \"about\": ").Append(J("Written by rust/golden (dotnet run --project rust/golden -c Release) with AiPet.Core; "
            + "replayed by rust/crates/aipet-sprite/tests/golden.rs. Layers are run-length hex: AARRGGBB or AARRGGBB*count, "
            + "row-major 26x24, premultiplied. Don't edit by hand.")).Append(",\n");
        sb.Append("  \"gw\": ").Append(Pet.GW).Append(",\n");
        sb.Append("  \"gh\": ").Append(Pet.GH).Append(",\n");
        sb.Append("  \"pm\": ").Append(J(pmPairs)).Append(",\n");
        sb.Append("  \"colors\": [\n    ").Append(string.Join(",\n    ", colors.Select(c => J(c)))).Append("\n  ],\n");
        sb.Append("  \"avatar_files\": [\n    ")
          .Append(string.Join(",\n    ", files.Select(f => J(new { file = f.File, repo = f.Repo, hex = Convert.ToHexString(f.Bytes) }))))
          .Append("\n  ],\n");
        sb.Append("  \"avatars\": [\n    ").Append(string.Join(",\n    ", all.Select(a => J(AvatarJson(a))))).Append("\n  ],\n");
        sb.Append("  \"layers\": [\n    ").Append(string.Join(",\n    ", layers.Select(l => J(l)))).Append("\n  ],\n");
        sb.Append("  \"cases\": [\n    ").Append(string.Join(",\n    ", caseLines)).Append("\n  ]\n");
        sb.Append("}\n");

        var output = Path.Combine(repo, "rust", "crates", "aipet-sprite", "tests", "golden", "frames.json");
        Directory.CreateDirectory(Path.GetDirectoryName(output));
        File.WriteAllText(output, sb.ToString(), new UTF8Encoding(false));
        Console.WriteLine($"{cases.Count} cases, {samples} samples ({unchanged} without a redraw), {layers.Count} distinct layers, "
            + $"{all.Count} avatars, states: {string.Join(" ", states)}, "
            + $"samples showing an idle act: {string.Join(" ", idleActs.Select(a => $"{a.Key} {a.Value}"))}");
        if (idleActs.Count != 5) throw new InvalidOperationException("no sample shows one of the idle acts: pick other seeds");
        Console.WriteLine($"{unreachableMidpoints} rounding midpoints of the z's and notes are unreachable; the others are in midpoint-*");
        Console.WriteLine($"wrote {output} ({new FileInfo(output).Length / 1024} KB)");
    }

    static readonly FieldInfo Rng = typeof(Pet).GetField("rng", BindingFlags.NonPublic | BindingFlags.Instance)
                                    ?? throw new InvalidOperationException("Pet.rng not found");
    static readonly FieldInfo IdleAct = typeof(Pet).GetField("idleAct", BindingFlags.NonPublic | BindingFlags.Instance)
                                        ?? throw new InvalidOperationException("Pet.idleAct not found");

    /// A pet whose Random throws (the golden frames can't depend on chance), or is scripted from `seed`.
    static Pet NewPet(int? seed)
    {
        var pet = new Pet();
        Rng.SetValue(pet, seed is int s ? new ScriptedRandom(s) : new NoRandom());
        return pet;
    }

    /// The same numbers in C# and Rust: call k (counting on from the seed) gives NextDouble() = k * 0.6180339887498949 % 1
    /// and Next(max) = k * 7 % max. The pet calls nothing else.
    sealed class ScriptedRandom(int seed) : NoRandom
    {
        int k = seed;
        public override double NextDouble() { k++; return k * 0.6180339887498949 % 1.0; }
        public override int Next(int maxValue) { k++; return k * 7 % maxValue; }
    }

    class NoRandom : Random
    {
        static InvalidOperationException Used() => new("a golden case reached the pet's Random: end it before 3.13 s");
        public override int Next() => throw Used();
        public override int Next(int maxValue) => throw Used();
        public override int Next(int minValue, int maxValue) => throw Used();
        public override long NextInt64() => throw Used();
        public override long NextInt64(long maxValue) => throw Used();
        public override long NextInt64(long minValue, long maxValue) => throw Used();
        public override double NextDouble() => throw Used();
        public override float NextSingle() => throw Used();
        public override void NextBytes(byte[] buffer) => throw Used();
        public override void NextBytes(Span<byte> buffer) => throw Used();
        protected override double Sample() => throw Used();
    }

    // ------------------------------------------------------------------ cases

    sealed class Case
    {
        public string Name, Avatar;
        public double Rate;
        public int Steps;
        public double[] Times;
        public int? Seed;  // ScriptedRandom instead of NoRandom
        public readonly List<Phase> Phases = new();
        public readonly SortedSet<int> Samples = new();
    }

    sealed record Phase(int Step, PetInput Input, string Avatar);

    static PetInput In(string state, Action<PetInput> set = null)
    {
        var i = new PetInput { State = state };
        set?.Invoke(i);
        return i;
    }

    /// Update at `rate` Hz from t = 0 through `until`; every `every`th step and the last one are sampled.
    static Case Steps(string name, string avatar, PetInput input, double rate = 60, double until = 2.9, int every = 9)
    {
        var c = new Case { Name = name, Avatar = avatar, Rate = rate, Steps = (int)Math.Floor(until * rate + 1e-9) + 1 };
        c.Phases.Add(new(0, input, null));
        for (int i = 0; i < c.Steps; i += every) c.Samples.Add(i);
        c.Samples.Add(c.Steps - 1);
        return c;
    }

    /// From `step` on, the input is `input` (and the avatar `avatar`); the step and the next two are sampled.
    static void At(Case c, int step, PetInput input, string avatar = null)
    {
        c.Phases.Add(new(step, input, avatar));
        for (int k = 0; k < 3 && step + k < c.Steps; k++) c.Samples.Add(step + k);
    }

    /// Update at exactly these times (all sampled).
    static Case AtTimes(string name, string avatar, PetInput input, double[] times)
    {
        var c = new Case { Name = name, Avatar = avatar, Times = times };
        c.Phases.Add(new(0, input, null));
        for (int i = 0; i < times.Length; i++) c.Samples.Add(i);
        return c;
    }

    static List<Case> Cases(List<Avatar> all)
    {
        var cases = new List<Case>();
        Case Add(Case c) { cases.Add(c); return c; }

        // every state
        foreach (var av in new[] { Sprout, Hood, Green })
        {
            Add(Steps($"sleep/{av}", av, In("sleep")));
            Add(Steps($"idle/{av}", av, In("idle")));
        }
        Add(Steps("thinking/Sprout", Sprout, In("thinking")));
        Add(Steps("thinking/Hood", Hood, In("thinking")));
        Add(Steps("working-laptop/Sprout", Sprout, In("working")));  // no prop: the laptop
        Add(Steps("working-laptop/Hood", Hood, In("working", i => i.Prop = "laptop")));
        Add(Steps("working-laptop/Hood (green)", Green, In("working")));
        Add(Steps("working-lens/Sprout", Sprout, In("working", i => i.Prop = "lens")));
        Add(Steps("working-lens/Hood", Hood, In("working", i => i.Prop = "lens")));
        Add(Steps("working-other-prop/Sprout", Sprout, In("working", i => i.Prop = "kettle")));  // not "lens": the laptop
        Add(Steps("attention/Sprout", Sprout, In("attention")));
        Add(Steps("alert-then-idle/Hood", Hood, In("idle", i => i.AlertUntil = 1.5)));
        Add(Steps("alert-then-lens/Hood (green)", Green, In("working", i => { i.Prop = "lens"; i.AlertUntil = 2; })));
        Add(Steps("done-fresh/Sprout", Sprout, In("done")));  // jumps until 0.7 s, waves until 4 s
        Add(Steps("done-older/Hood", Hood, In("done", i => i.StateSince = -1.5)));  // 4 s old at t = 2.5: stops waving
        Add(Steps("done-starts-later/Hood (green)", Green, In("done", i => i.StateSince = 0.5)));  // since < 0 at first
        Add(Steps("unknown-state/Sprout", Sprout, In("mystery")));

        // music: listen instead of idle/sleep/old done, headphones always
        Add(Steps("listen-idle/Sprout", Sprout, In("idle", i => i.Music = true)));
        Add(Steps("listen-sleep/Hood", Hood, In("sleep", i => i.Music = true)));
        Add(Steps("listen-idle/Hood (green)", Green, In("idle", i => i.Music = true)));
        Add(Steps("music-done/Sprout", Sprout, In("done", i => { i.Music = true; i.StateSince = -2; })));  // listens from t = 2
        Add(Steps("music-working/Hood (green)", Green, In("working", i => i.Music = true)));
        Add(Steps("music-thinking/Hood", Hood, In("thinking", i => i.Music = true)));
        Add(Steps("music-attention/Sprout", Sprout, In("attention", i => i.Music = true)));
        Add(Steps("alert-music/Hood", Hood, In("idle", i => { i.Music = true; i.AlertUntil = 1.5; })));  // the alert wins: attention
        // done turns to listen once it is more than 4 s old: exactly 4 s old at t = 2.0 it is still done
        Add(AtTimes("music-done-edge/Sprout", Sprout, In("done", i => { i.Music = true; i.StateSince = -2; }), [1.9, 2.0, 2.05]));

        // hover: the mouse moves through the rounding midpoints of (m.X - 65) / 28 (±0.5, ±1.5, ±2.5) and around the
        // "up" line m.Y < 15
        (double, double)[] mice = [(79, 10), (51, 60), (107, 15), (23, 14.5), (135, 30), (-5, 100), (65, 0)];
        Case Hover(string name, string avatar, string state, Action<PetInput> set = null)
        {
            PetInput At(int k) => In(state, i => { i.Hover = true; i.Mouse = mice[k]; set?.Invoke(i); });
            var c = Steps(name, avatar, At(0));
            for (int k = 1; k < mice.Length; k++) Program.At(c, k * 25, At(k));
            return c;
        }
        Add(Hover("hover-idle/Sprout", Sprout, "idle"));
        Add(Hover("hover-sleep/Hood", Hood, "sleep"));
        Add(Hover("hover-done/Hood (green)", Green, "done"));
        Add(Hover("hover-listen/Sprout", Sprout, "idle", i => i.Music = true));
        Add(Hover("hover-thinking/Hood", Hood, "thinking"));
        Add(Hover("hover-working/Sprout", Sprout, "working"));  // working ignores the mouse
        Add(Hover("hover-attention/Hood (green)", Green, "attention"));
        Add(Steps("hover-no-mouse-sleep/Sprout", Sprout, In("sleep", i => i.Hover = true)));  // no z's, no look
        Add(Hover("hover-listen-sleep/Sprout", Sprout, "sleep", i => i.Music = true));  // listening, not asleep: keeps nodding
        // the alert over working: attention, so the eyes follow the mouse
        Add(Steps("hover-alert-working/Hood", Hood, In("working", i => { i.Hover = true; i.Mouse = (79, 10); i.AlertUntil = 1.5; })));
        // outside the sprite the look is clamped to ±2 (-3.04 and 3.39 round to -3 and 3); (37, 60) looks one to the left
        var far = Add(Steps("hover-far/Sprout", Sprout, In("idle", i => { i.Hover = true; i.Mouse = (-20, 60); })));
        At(far, 60, In("idle", i => { i.Hover = true; i.Mouse = (160, 60); }));
        At(far, 120, In("idle", i => { i.Hover = true; i.Mouse = (37, 60); }));

        // dragging: running while the last move is under 0.25 s old (and before it), held after
        Add(Steps("drag-run-then-held/Sprout", Sprout, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = 1.0; })));
        Add(Steps("drag-run-left-lens/Hood", Hood,
            In("working", i => { i.Prop = "lens"; i.Dragging = i.Moved = true; i.LastMove = 1.0; i.Facing = -1; })));
        Add(Steps("drag-run-left-sleep/Hood (green)", Green,
            In("sleep", i => { i.Dragging = i.Moved = true; i.LastMove = 2.0; i.Facing = -1; })));
        Add(Steps("drag-held-thinking/Sprout", Sprout, In("thinking", i => { i.Dragging = i.Moved = true; i.LastMove = -10; })));
        Add(Steps("drag-done/Hood", Hood, In("done", i => { i.Dragging = i.Moved = true; i.LastMove = 1.5; })));
        Add(Steps("drag-not-moved/Hood (green)", Green, In("idle", i => i.Dragging = true)));  // pressed only: no breathing
        Add(Steps("drag-hover-wave/Sprout", Sprout, In("idle", i =>
        {
            i.Dragging = i.Moved = true; i.LastMove = -10; i.Hover = true; i.Mouse = (79, 10); i.WaveUntil = 2;
        })));
        Add(Steps("drag-listen/Hood", Hood, In("idle", i => { i.Music = true; i.Dragging = i.Moved = true; i.LastMove = 0.8; })));
        Add(Steps("drag-attention-left/Sprout", Sprout,
            In("attention", i => { i.Dragging = i.Moved = true; i.LastMove = 1.2; i.Facing = -1; })));
        // running dust takes the place of the thinking dots
        Add(Steps("drag-run-thinking/Sprout", Sprout, In("thinking", i => { i.Dragging = i.Moved = true; i.LastMove = 1.0; })));
        // a whole drag: pick up, run right, turn left, hold still, drop and land
        var drag = Add(Steps("drag-flow/Sprout", Sprout, In("idle")));
        for (int s = 30; s <= 90; s += 5)
        {
            double lastMove = s / 60.0;
            int facing = s < 60 ? 1 : -1;
            At(drag, s, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = lastMove; i.Facing = facing; }));
        }
        At(drag, 125, In("idle", i => { i.LandUntil = 125 / 60.0 + 0.45; i.Facing = -1; }));

        // poke, land, wave
        Add(Steps("poke-idle/Sprout", Sprout, In("idle", i => i.PokeUntil = 1.2)));
        Add(Steps("poke-laptop/Hood", Hood, In("working", i => i.PokeUntil = 1.2)));
        Add(Steps("poke-done/Hood (green)", Green, In("done", i => { i.PokeUntil = 1.2; i.StateSince = -5; })));
        Add(Steps("poke-sleep/Sprout", Sprout, In("sleep", i => i.PokeUntil = 1.2)));
        var poke = Add(Steps("poke-flow/Hood", Hood, In("idle")));
        At(poke, 60, In("idle", i => i.PokeUntil = 60 / 60.0 + 1.2));
        Add(Steps("land-sleep/Sprout", Sprout, In("sleep", i => i.LandUntil = 0.45)));
        Add(Steps("land-lens/Hood", Hood, In("working", i => { i.Prop = "lens"; i.LandUntil = 0.45; })));
        Add(Steps("land-idle-30hz/Hood (green)", Green, In("idle", i => i.LandUntil = 0.45), rate: 30, every: 4));  // squash
        Add(Steps("land-early-thinking/Sprout", Sprout, In("thinking", i => i.LandUntil = 1.45)));  // pp < 0 before t = 1
        Add(Steps("wave-idle/Sprout", Sprout, In("idle", i => i.WaveUntil = 1.5)));
        Add(Steps("wave-sleep/Hood", Hood, In("sleep", i => i.WaveUntil = 1.5)));
        Add(Steps("wave-done/Hood (green)", Green, In("done", i => i.WaveUntil = 1.5)));
        Add(Steps("wave-thinking/Sprout", Sprout, In("thinking", i => i.WaveUntil = 1.5)));  // only idle, sleep, done wave
        Add(Steps("wave-listen/Sprout", Sprout, In("idle", i => { i.Music = true; i.WaveUntil = 1.5; })));  // listening: no wave
        Add(Steps("wave-alert/Hood (green)", Green, In("idle", i => { i.WaveUntil = 2.5; i.AlertUntil = 1.5; })));  // waves after
        Add(Steps("poke-listen/Hood (green)", Green, In("idle", i => { i.Music = true; i.PokeUntil = 1.2; })));  // no sway: X = 0
        Add(Steps("poke-held/Hood", Hood, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = -10; i.PokeUntil = 1.2; })));
        // the later rule wins: landing over poke, poke over wave, running over landing
        Add(Steps("poke-land/Sprout", Sprout, In("idle", i => { i.PokeUntil = 1.2; i.LandUntil = 0.45; })));
        Add(Steps("poke-wave/Hood", Hood, In("idle", i => { i.PokeUntil = 1.2; i.WaveUntil = 1.5; })));
        Add(Steps("land-run/Sprout", Sprout, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = 1.0; i.LandUntil = 0.45; })));
        // every step: a threshold inside a phase (squint while more than 0.6 of the poke is left) can fall between the
        // sampled steps, or on a sample whose pixels were drawn at an earlier step
        Add(Steps("poke-every-step/Sprout", Sprout, In("idle", i => i.PokeUntil = 1.2), until: 1.25, every: 1));

        // step sizes: squash on landing, dt clamped to 0.05 and to 0.001
        Add(Steps("attention-20hz/Sprout", Sprout, In("attention"), rate: 20, every: 3));
        Add(Steps("attention-12.5hz/Hood", Hood, In("attention"), rate: 12.5, every: 2));
        Add(Steps("done-20hz/Hood (green)", Green, In("done"), rate: 20, every: 2));
        Add(Steps("done-2000hz/Sprout", Sprout, In("done"), rate: 2000, until: 0.25, every: 37));

        // SetAvatar redraws at once: step 91 is in the same 1/24 s frame as step 90
        var sw = Add(Steps("switch-avatar/Sprout", Sprout, In("sleep")));
        At(sw, 91, In("sleep"), Hood);
        At(sw, 131, In("idle", i => i.Music = true), Green);

        // the moods of a chat, one after the other
        var flow = Add(Steps("mood-flow/Hood", Hood, In("sleep")));
        At(flow, 20, In("thinking", i => i.StateSince = 20 / 60.0));
        At(flow, 45, In("working", i => i.StateSince = 45 / 60.0));
        At(flow, 70, In("working", i => { i.StateSince = 45 / 60.0; i.Prop = "lens"; }));
        At(flow, 95, In("done", i => i.StateSince = 95 / 60.0));
        At(flow, 140, In("idle", i => i.StateSince = 140 / 60.0));
        At(flow, 160, In("idle", i => { i.StateSince = 140 / 60.0; i.AlertUntil = 160 / 60.0 + 1; }));

        // the first blink: eyes "normal", "up" or "down" blink once t > blinkAt (3); the Random is consulted only
        // after 3.13 s, so these run to 3.1167 s and sample every step after 2.95 s
        Case Blink(string name, string avatar, PetInput input)
        {
            var c = Steps(name, avatar, input, until: 3.12);
            for (int i = 0; i < c.Steps; i++) if (i / 60.0 > 2.95) c.Samples.Add(i);
            return c;
        }
        Add(Blink("blink-idle/Sprout", Sprout, In("idle")));
        Add(Blink("blink-thinking/Hood", Hood, In("thinking")));
        Add(Blink("blink-laptop/Hood (green)", Green, In("working")));
        Add(Blink("blink-hover-up/Hood", Hood, In("idle", i => { i.Hover = true; i.Mouse = (65, 5); })));
        Add(Blink("no-blink-sleep/Sprout", Sprout, In("sleep")));
        Add(Blink("no-blink-listen/Hood (green)", Green, In("idle", i => i.Music = true)));

        // past 3 s with a scripted Random: idle acts (look, hop, stretch, wiggle, tap), glances, blinks, listen's open eyes
        Case Scripted(string name, string avatar, PetInput input, int seed)
        {
            var c = Steps(name, avatar, input, rate: 30, until: 40, every: 7);
            c.Seed = seed;
            return c;
        }
        Add(Scripted("scripted-idle/Sprout", Sprout, In("idle"), 0));
        Add(Scripted("scripted-idle/Hood", Hood, In("idle"), 1));
        Add(Scripted("scripted-idle/Hood (green)", Green, In("idle"), 2));
        var hover = Add(Scripted("scripted-hover/Sprout", Sprout, In("idle"), 3));
        // 10-14 s: hovered between two acts (tap ends at 8.6 s, the next starts at 14.7 s), so the eyes follow the mouse
        // and no act starts; scripted-hover-holds-act has an act held by hovering
        At(hover, 300, In("idle", i => { i.Hover = true; i.Mouse = (100, 40); }));
        At(hover, 420, In("idle"));
        // 23.3-25.3 s: dragged, then dropped, also between acts (the stretch starts at 25.9 s); see scripted-drag-holds-act
        At(hover, 700, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = 700 / 30.0; }));
        At(hover, 760, In("idle", i => i.LandUntil = 760 / 30.0 + 0.45));
        Add(Scripted("scripted-listen/Hood", Hood, In("idle", i => i.Music = true), 4));
        var moods = Add(Scripted("scripted-moods/Hood (green)", Green, In("sleep"), 5));
        At(moods, 150, In("thinking", i => i.StateSince = 150 / 30.0));
        At(moods, 330, In("working", i => i.StateSince = 330 / 30.0));
        At(moods, 480, In("working", i => { i.StateSince = 330 / 30.0; i.Prop = "lens"; }));
        At(moods, 600, In("done", i => i.StateSince = 600 / 30.0));
        At(moods, 900, In("idle", i => i.StateSince = 900 / 30.0));
        // seed 0 idles with a wiggle from 6.03 s to 7.73 s: hovering or holding the pet holds the act (not shown, not
        // ended, so the next act's time moves), and a new state drops it (back to idle, the next act starts at once)
        var hoverAct = Add(Scripted("scripted-hover-holds-act/Sprout", Sprout, In("idle"), 0));
        At(hoverAct, 195, In("idle", i => { i.Hover = true; i.Mouse = (100, 40); }));
        At(hoverAct, 240, In("idle"));
        var dragAct = Add(Scripted("scripted-drag-holds-act/Sprout", Sprout, In("idle"), 0));
        At(dragAct, 195, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = -10; }));
        At(dragAct, 240, In("idle"));
        var stateAct = Add(Scripted("scripted-state-change-mid-act/Sprout", Sprout, In("idle"), 0));
        At(stateAct, 195, In("thinking", i => i.StateSince = 195 / 30.0));
        At(stateAct, 210, In("idle", i => i.StateSince = 210 / 30.0));

        // times where Draw's Math.Round gets an exact .5: the z's and the music notes
        int k = 0;
        foreach (var times in ByDrawFrame(ZTimes())) Add(AtTimes($"midpoint-z/{k++}", Sprout, In("sleep"), times));
        k = 0;
        foreach (var times in ByDrawFrame(NoteTimes()))
            Add(AtTimes($"midpoint-notes/{k++}", Hood, In("idle", i => i.Music = true), times));

        // every avatar Avatar.All() lists: a few frames of the Settings still (idle), with headphones, held, working
        foreach (var a in all)
        {
            Add(Steps($"still/{a.Name}", a.Name, In("idle"), rate: 20, until: 0.15, every: 100));
            Add(Steps($"still-listen/{a.Name}", a.Name, In("idle", i => i.Music = true), rate: 20, until: 0.15, every: 100));
            Add(Steps($"still-held/{a.Name}", a.Name, In("idle", i => { i.Dragging = i.Moved = true; i.LastMove = -10; }),
                rate: 20, until: 0.15, every: 100));
            Add(Steps($"still-run-left-lens/{a.Name}", a.Name,
                In("working", i => { i.Prop = "lens"; i.Dragging = i.Moved = true; i.LastMove = 1; i.Facing = -1; }),
                rate: 20, until: 0.15, every: 100));
            Add(Steps($"still-laptop/{a.Name}", a.Name, In("working"), rate: 20, until: 0.15, every: 100));
        }

        foreach (var c in cases)
        {
            double last = c.Times?[^1] ?? (c.Steps - 1) / c.Rate;
            if (last >= 3.13 && c.Seed == null) throw new InvalidOperationException($"{c.Name} runs to {last} s: the Random would decide");
            int steps = c.Times?.Length ?? c.Steps;
            if (c.Phases.Any(p => p.Step >= steps)) throw new InvalidOperationException($"{c.Name} has a phase past its last step");
            if (c.Samples.Count == 0 || c.Samples.Max >= steps) throw new InvalidOperationException($"{c.Name}: bad samples");
        }
        return cases;
    }

    /// Midpoints no double t reaches: past the % 1 wrap, ph moves in steps too coarse to hit them (nor can the C#).
    static int unreachableMidpoints;

    /// A time near t0 where value(t) is exactly target, searching outward one ulp at a time.
    static double? FindExact(Func<double, double> value, double target, double t0)
    {
        long bits = BitConverter.DoubleToInt64Bits(t0);
        for (long d = 0; d <= 1 << 16; d++)
            foreach (long s in d == 0 ? new[] { 0L } : new[] { d, -d })
            {
                double t = BitConverter.Int64BitsToDouble(bits + s);
                if (value(t) == target) return t;
            }
        return null;
    }

    /// Pet.Draw's z's: ph = (t * 0.35 + i / 3.0) % 1, then Math.Round(ph * 4) and Math.Round(ph * 6).
    static IEnumerable<double> ZTimes()
    {
        for (int i = 0; i < 3; i++)
            foreach (var mul in new[] { 4.0, 6.0 })
                for (double target = 0.5; target < mul; target++)
                    for (int m = 0; m < 2; m++)
                    {
                        double t0 = (target / mul + m - i / 3.0) / 0.35;
                        if (t0 <= 0.05 || t0 >= 2.95) continue;
                        int ii = i;
                        double mm = mul;
                        if (FindExact(t => ((t * 0.35 + ii / 3.0) % 1) * mm, target, t0) is double found) yield return found;
                        else unreachableMidpoints++;
                    }
    }

    /// Pet.Draw's notes: ph = (t * 0.45 + i * 0.5) % 1, then Math.Round(ph * (i == 0 ? 3 : -2)) and Math.Round(ph * 7).
    static IEnumerable<double> NoteTimes()
    {
        for (int i = 0; i < 2; i++)
            foreach (var mul in new[] { i == 0 ? 3.0 : -2.0, 7.0 })
                for (double target = 0.5; target < Math.Abs(mul); target++)
                    for (int m = 0; m < 3; m++)
                    {
                        double signed = mul < 0 ? -target : target;
                        double t0 = (target / Math.Abs(mul) + m - i * 0.5) / 0.45;
                        if (t0 <= 0.05 || t0 >= 2.95) continue;
                        int ii = i;
                        double mm = mul;
                        if (FindExact(t => ((t * 0.45 + ii * 0.5) % 1) * mm, signed, t0) is double found) yield return found;
                        else unreachableMidpoints++;
                    }
    }

    /// Sorted times split so that each list has one time per 1/24 s draw frame (a second time in the same frame
    /// wouldn't redraw).
    static List<double[]> ByDrawFrame(IEnumerable<double> times)
    {
        var lists = new List<List<double>>();
        foreach (var t in times.Distinct().OrderBy(t => t))
        {
            int n = (int)(t * 24);
            var list = lists.FirstOrDefault(l => (int)(l[^1] * 24) != n);
            if (list == null) lists.Add(list = new List<double>());
            list.Add(t);
        }
        return lists.Select(l => l.ToArray()).ToList();
    }

    // ------------------------------------------------------------------ avatars, colours, Pm

    sealed record AvatarFile(string File, byte[] Bytes, string Repo = null);

    /// The example avatar plus files that test Avatar.All(): some load, some are skipped.
    static List<AvatarFile> AvatarFiles(string repo)
    {
        var list = new List<AvatarFile>
        {
            new("hood-green.json", File.ReadAllBytes(Path.Combine(repo, "avatars", "hood-green.json")), "avatars/hood-green.json"),
        };
        void Add(string file, string json) => list.Add(new(file, Encoding.UTF8.GetBytes(json)));
        // skipped: the pet can't draw them (AvatarTests), or they don't parse
        Add("short.json", """{"name":"Short","shapes":[[13,14,7]]}""");
        Add("nullshape.json", """{"name":"NullShape","shapes":[[13,14,7,7],null]}""");
        Add("empty.json", """{"name":"Empty","shapes":[]}""");
        Add("noshapes.json", """{"name":"NoShapes"}""");
        Add("flat.json", """{"name":"Flat","shapes":[[13,14,0,7]]}""");
        Add("negative.json", """{"name":"Negative","shapes":[[13,14,7,-2]]}""");
        Add("huge.json", """{"name":"Huge","shapes":[[13,14,1e400,7]]}""");
        Add("faceoff.json", """{"name":"FaceOff","shapes":[[13,14,7,7]],"faceBottom":2147483647}""");
        Add("facelow.json", """{"name":"FaceLow","shapes":[[13,14,7,7]],"faceBottom":24}""");
        Add("faceup.json", """{"name":"FaceUp","shapes":[[13,14,7,7]],"faceTop":-5}""");
        Add("faceflip.json", """{"name":"FaceFlip","shapes":[[13,14,7,7]],"faceTop":14,"faceBottom":9}""");
        Add("null.json", "null");
        Add("array.json", "[]");
        Add("broken.json", """{"name":"Broken","shapes":[[13,14,7,7]]""");
        Add("comment.json", """{"name":"Comment","shapes":[[13,14,7,7]]} // hi""");
        Add("trailing.json", """{"name":"Trailing","shapes":[[13,14,7,7]],}""");
        Add("types.json", """{"name":"Types","shapes":[[13,14,7,7]],"blush":"yes"}""");
        Add("floatface.json", """{"name":"FloatFace","shapes":[[13,14,7,7]],"faceTop":9.0}""");
        Add("stringface.json", """{"name":"StringFace","shapes":[[13,14,7,7]],"faceTop":"9"}""");
        Add("badcolor.json", """{"name":"BadColor","shapes":[[13,14,7,7]],"palette":{"body":5}}""");
        // System.Text.Json's MaxDepth is 64 (the root object is level 1), even inside a property it ignores
        Add("deep.json", """{"name":"Deep","shapes":[[13,14,7,7]],"x":""" + new string('[', 64) + new string(']', 64) + "}");
        Add("deepobjects.json", """{"name":"DeepObjects","shapes":[[13,14,7,7]],"x":"""
            + string.Concat(Enumerable.Repeat("""{"a":""", 64)) + "1" + new string('}', 64) + "}");
        list.Add(new("notes.txt", Encoding.UTF8.GetBytes("""{"name":"NotJson","shapes":[[13,14,7,7]]}""")));
        // loaded
        Add("mine.json", """{"name":"Mine","shapes":[[13,14,7.3,7.3,1],[13,7,4,4]],"faceTop":9,"faceBottom":14,"palette":{"body":"#3366CC"}}""");
        Add("cased.json", """
            {"NAME":"First","Name":"Cased","Shapes":[[13,14,7,7]],"SHADING":"rim","Blush":false,"FaceTOP":8,"facebottom":15,
             "face_top":3,"outline":"#FF0000","Glow":"#FFFFFF","unknown":{"a":[1,null]},
             "PALETTE":{"Body":"#112233","body":"#445566","EYEHI":"#FFFFFF","eye":null}}
            """);
        Add("noname.json", """{"shapes":[[13,14,7,7]],"blush":false}""");
        Add("nullname.json", """{"name":null,"shapes":[[13,15,8,7]],"shading":null,"palette":null}""");
        Add("emptyname.json", """{"name":"","shapes":[[13,14,6,8]],"blush":false}""");
        Add("dupes.json", """{"name":"First","name":"Dupes","shapes":[[1,1,1,1]],"shapes":[[13,14,7,7]],"palette":{"eye":"#010203","eye":"#040506"}}""");
        Add("colors.json", """
            {"name":"Colors","shapes":[[13,13,8,8]],"palette":{"outline":"  #80FF0000  ","deep":"##00FF00","shade":"#  FFFFF",
             "body":"#   FFFFF","hi":"fff","spec":"#GGGGGG","blush":"#12345678","screen":"","bezel":"   ","glint":"#+1234567",
             "eye":"#ff00ff","eyeHi":null,"eyeLo":"#0x123456","accent":"#ABCDEF0\u0000"}}
            """);
        Add("blobby.json", """
            {"name":"Blobby","shapes":[[13,15,7,6],[7,8,2.5,2.5],[10,8,2.5,2.5],[13,8,2.5,2.5],[16,8,2.5,2.5],[19,8,2.5,2.5]],
             "palette":{"body":"#C04080","accent":"#80FFFFFF"}}
            """);
        Add("edge.json", """{"name":"Edge","shapes":[[13,12,14,13]],"faceTop":0,"faceBottom":23}""");
        Add("tiny.json", """{"name":"Tiny","shapes":[[13,20,2,2]],"shading":"Rim","faceTop":23,"faceBottom":23}""");
        Add("rimwide.json", """{"name":"Rim wide","shapes":[[13,14,10,7]],"shading":"rim","faceTop":10,"faceBottom":14}""");
        Add("extra.json", """{"name":"Extra","shapes":[[13,14,7,7]],"outline":"#FF0000","unknown":{"a":[1,2]}}""");
        // the body starts on row 9: no spec (FindSpec takes only cells above row 9)
        Add("lowtop.json", """{"name":"LowTop","shapes":[[13,15,7,6]],"faceTop":11,"faceBottom":14}""");
        // -0 is the int 0; a number too large for a double is Infinity, and Drawable checks only the first four
        Add("minuszero.json", """{"name":"MinusZero","shapes":[[13,14,7,7]],"faceTop":-0,"faceBottom":-0}""");
        Add("overflow.json", """{"name":"Overflow","shapes":[[13,14,7,7,1e400],[13,8,4,4,-1e400,1.8e308]],"x":1e400}""");
        // 63 levels under the root: loads (brackets in strings don't count)
        Add("notdeep.json", """{"name":"NotDeep","shapes":[[13,14,7,7]],"x":""" + new string('[', 63) + """ "[[\"{{" """
            + new string(']', 63) + "}");
        list.Add(new("bom.json", [0xEF, 0xBB, 0xBF, .. Encoding.UTF8.GetBytes("""{"name":"Bom","shapes":[[13,14,7,7]]}""")]));
        list.Add(new("utf16.json", [.. Encoding.Unicode.GetPreamble(), .. Encoding.Unicode.GetBytes("""{"name":"Utf16 é","shapes":[[12,14,7,7]]}""")]));
        return list;
    }

    static string[] ColorInputs() =>
    [
        "#3366CC", "3366cc", "#80FF0000", "80ff0000", "#00000000", "#000000", "#FFFFFFFF", "#ffffff",
        "  #3366CC\t", "\u00A0#3366CC\u2028", "###3366CC", "#FFFFFF\r\n", "\r\n#FFFFFF", "#12345678 ", " #12345678",
        "#", "##", "", " ", "\t\n", "#3366C", "#3366CCD", "#1122334455", "#GG0000", "fff", "#+1234567", "#-1234567",
        "#0x123456", "#0X12345", "#FF 00000", "#FF 0000", "#  FFFFF", "#   FFFFF", "#\tFFFFFFF", "#\v3366CCF",
        "#\u00A0FFFFFFF", "#\u3000FFFFFFF", "#ABCDEF0\0", "#ABCDEF \0", "#ABCDEF\0 ", "#ABCD\0\0\0\0", "#\0\0FFFFFF",
        "#\uFF26F0000", "#\u00E912345", "#FF00FF\u0085", "#\n\n\nFFFFF", "#000000000", "0x3366CC", "#3366cc ",
    ];

    static IEnumerable<uint> PmInputs()
    {
        uint[] fixedOnes = [0, 0xFFFFFFFF, 0x00FFFFFF, 0x01FFFFFF, 0xFEFFFFFF, 0xFE010101, 0x80FF8040, 0x7F7F7F7F,
            0x88A9E4FF, 0xCCFFFFFF, 0xB0E6DED3, 0x99E3E8FF, 0x88E27A52, 0x12345678];
        foreach (var v in fixedOnes) yield return v;
        uint x = 12345;
        for (int i = 0; i < 200; i++) { x = x * 1664525 + 1013904223; yield return x; }
    }

    // ------------------------------------------------------------------ JSON

    static string J(object value) => JsonSerializer.Serialize(value, Json);

    static object InputJson(PetInput i) => new
    {
        state = i.State, state_since = i.StateSince, prop = i.Prop, hover = i.Hover,
        mouse = i.Mouse is { } m ? new[] { m.X, m.Y } : null,
        dragging = i.Dragging, moved = i.Moved, last_move = i.LastMove, facing = i.Facing,
        poke_until = i.PokeUntil, land_until = i.LandUntil, wave_until = i.WaveUntil, alert_until = i.AlertUntil,
        music = i.Music,
    };

    static object AvatarJson(Avatar a) => new
    {
        // the four numbers the pet reads (a fifth can be anything, even Infinity, which JSON can't hold)
        name = a.Name, shading = a.Shading, blush = a.Blush, face_top = a.FaceTop, face_bottom = a.FaceBottom,
        shapes = a.Shapes.Select(e => e[..4]).ToArray(),
        colors = new Dictionary<string, string>
        {
            ["outline"] = a.Outline.ToString("X8"), ["deep"] = a.Deep.ToString("X8"), ["shade"] = a.Shade.ToString("X8"),
            ["body"] = a.Body.ToString("X8"), ["hi"] = a.Hi.ToString("X8"), ["spec"] = a.Spec.ToString("X8"),
            ["blush"] = a.BlushC.ToString("X8"), ["screen"] = a.Screen.ToString("X8"), ["bezel"] = a.Bezel.ToString("X8"),
            ["glint"] = a.Glint.ToString("X8"), ["eye"] = a.Eye.ToString("X8"), ["eyeHi"] = a.EyeHi.ToString("X8"),
            ["eyeLo"] = a.EyeLo.ToString("X8"), ["glow"] = a.Glow.ToString("X8"), ["accent"] = a.Accent.ToString("X8"),
        },
    };

    /// A layer as run-length hex: "AARRGGBB" or "AARRGGBB*count", space-separated, row-major.
    static string Rle(uint[] px)
    {
        var sb = new StringBuilder();
        for (int i = 0; i < px.Length;)
        {
            int j = i;
            while (j < px.Length && px[j] == px[i]) j++;
            if (sb.Length > 0) sb.Append(' ');
            sb.Append(px[i].ToString("X8"));
            if (j - i > 1) sb.Append('*').Append(j - i);
            i = j;
        }
        return sb.ToString();
    }
}
