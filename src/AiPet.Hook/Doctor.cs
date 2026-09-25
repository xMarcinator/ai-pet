using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet;

/// `aipet-hook --doctor claude|codex [--probe]`: checks that the agent will really run AiPet's hook, without changing
/// anything. It reads the agent's settings, asks Codex which hooks it trusts, flags hook commands that can't work, and
/// runs AiPet's hook with a harmless test event: directly for Claude (as Claude does), and for Codex only with --probe,
/// since that means starting PowerShell/sh the way Codex does. Exit code 1 when something is broken.
static class Doctor
{
    static int fails, warns;
    static bool probe;
    static void Ok(string s) => Console.WriteLine("  [ok]   " + s);
    static void Info(string s) => Console.WriteLine("         " + s);
    static void Warn(string s) { warns++; Console.WriteLine("  [warn] " + s); }
    static void Fail(string s) { fails++; Console.WriteLine("  [FAIL] " + s); }
    static void Section(string s) => Console.WriteLine(System.Environment.NewLine + s);
    static bool True(JsonNode n) => n is JsonValue v && v.TryGetValue(out bool b) && b;

    public static int Run(string agent, bool probeShells)
    {
        probe = probeShells;
        // UTF-8 while the doctor writes, but the console's code page is the shell's too, so it's put back afterwards
        Encoding encoding = null;
        try { encoding = Console.OutputEncoding; Console.OutputEncoding = new UTF8Encoding(false); } catch { }
        try
        {
            switch (agent)
            {
                case "claude": Claude(); break;
                case "codex": Codex(); break;
                default: Console.Error.WriteLine("usage: aipet-hook --doctor claude|codex"); return 2;
            }
            Section(fails > 0 ? $"{fails} problem(s), {warns} warning(s)." : warns > 0 ? $"No problems; {warns} warning(s)." : "All good.");
            return fails > 0 ? 1 : 0;
        }
        finally { try { if (encoding != null) Console.OutputEncoding = encoding; } catch { } }
    }

    // ------------------------------------------------------------------ Claude Code
    static void Claude()
    {
        Section($"Claude Code settings ({ClaudeConfig.Settings})");
        JsonObject root = null;
        try { root = JsonNode.Parse(File.ReadAllText(ClaudeConfig.Settings)) as JsonObject; }
        catch (FileNotFoundException) { Fail("settings.json doesn't exist: run aipet-hook --install claude"); }
        catch (Exception ex) { Fail("settings.json can't be read: " + ex.Message); }
        if (root == null) return;
        if (True(root["disableAllHooks"])) Fail("\"disableAllHooks\": true switches every hook off");

        var ours = new List<(string Event, JsonObject Hook)>();
        if (root["hooks"] is JsonObject hooks)
            foreach (var (ev, value) in hooks)
                if (value is JsonArray groups)
                    foreach (var h in groups.OfType<JsonObject>().SelectMany(g => (g["hooks"] as JsonArray)?.OfType<JsonObject>() ?? Enumerable.Empty<JsonObject>()))
                        if (ClaudeConfig.IsOurs(h)) ours.Add((ev, h));
        // the AiPet plugin brings its own hooks (plugins/claude-code/hooks/hooks.json), so settings.json needs none
        bool plugin = root["enabledPlugins"] is JsonObject plugins && plugins.Any(p => p.Key.StartsWith("aipet@") && True(p.Value));
        string exe = null;
        if (ours.Count == 0 && plugin) Ok("hooks come from the AiPet plugin (enabledPlugins), not from settings.json");
        else if (ours.Count == 0) { Fail("AiPet's hooks aren't registered: run aipet-hook --install claude"); return; }
        else
        {
            var missing = ClaudeConfig.Events.Select(e => e.Event).Where(e => !ours.Any(o => o.Event == e)).ToList();
            if (missing.Count > 0) Warn("missing events: " + string.Join(", ", missing) + " (run aipet-hook --install claude)");
            else Ok($"registered for all {ClaudeConfig.Events.Length} events");
            exe = (string)ours[0].Hook["command"];
            if (!File.Exists(exe)) Fail($"the registered hook doesn't exist: {exe}");
            else Ok("hook: " + exe);
            if (plugin) Warn("the AiPet plugin is also enabled, so every event is reported twice");
        }

        Section("Policies set by your organisation");
        foreach (var path in ManagedClaudeSettings().Where(File.Exists))
        {
            try
            {
                var m = JsonNode.Parse(File.ReadAllText(path)) as JsonObject;
                if (True(m?["allowManagedHooksOnly"])) Fail($"{path}: allowManagedHooksOnly is on, so hooks in your own settings never run");
                else if (True(m?["disableAllHooks"])) Fail($"{path}: disableAllHooks is on");
                else Ok($"{path} allows your hooks");
            }
            catch (Exception ex) { Warn($"{path} can't be read: {ex.Message}"); }
        }
        if (!ManagedClaudeSettings().Any(File.Exists)) Ok("no managed settings on this machine");

        bool running = CheckAiPet();
        Section("Running the hook the way Claude does");
        if (exe == null) Info("skipped: the plugin runs its own copy of the hook from Claude's plugin folder (the events the pet got show below)");
        else if (File.Exists(exe)) Probe(exe, new[] { "--agent", "claude" }, null, 5, running, quickMs: 1000);
        RecentEvents("claude");
    }

    static IEnumerable<string> ManagedClaudeSettings()
    {
        if (OperatingSystem.IsWindows())
        {
            yield return @"C:\Program Files\ClaudeCode\managed-settings.json";
            yield return @"C:\ProgramData\ClaudeCode\managed-settings.json";
        }
        else if (OperatingSystem.IsMacOS()) yield return "/Library/Application Support/ClaudeCode/managed-settings.json";
        else yield return "/etc/claude-code/managed-settings.json";
    }

    // ------------------------------------------------------------------ Codex
    static void Codex()
    {
        var codex = FindOnPath(OperatingSystem.IsWindows() ? new[] { "codex.exe", "codex.cmd" } : new[] { "codex" })
                    ?? (OperatingSystem.IsWindows() ? Existing(Path.Combine(System.Environment.GetFolderPath(System.Environment.SpecialFolder.LocalApplicationData), "Programs", "OpenAI", "Codex", "bin", "codex.exe")) : null);
        Section("Codex");
        if (codex == null) Warn("the codex CLI isn't on PATH (needed to trust hooks, and for this check)");
        else
        {
            var (code, version, error) = Capture(codex, new[] { "--version" }, 10);
            if (code == 0) Ok($"{version.Trim()} ({codex})");
            else Warn($"codex --version didn't work: {(code == -1 ? "no answer within 10 s" : code == -2 ? error : $"exit code {code}")} ({codex})");
        }

        Section($"Codex config ({CodexConfig.ConfigToml})");
        var toml = File.Exists(CodexConfig.ConfigToml) ? File.ReadAllText(CodexConfig.ConfigToml) : "";
        var features = Features(toml);
        if (features.Any(f => f.Key is "hooks" or "codex_hooks" && f.Value == "false")) Fail("hooks are switched off in [features]");
        if (features.Any(f => f.Key == "codex_hooks")) Warn("[features] codex_hooks is deprecated; Codex wants hooks instead");
        bool inlineHooks = Regex.IsMatch(toml, @"^\s*\[\[\s*hooks\.[A-Za-z]+\s*\]\]", RegexOptions.Multiline);
        if (inlineHooks && File.Exists(CodexConfig.HooksJson)) Warn("hooks are in both config.toml and hooks.json; Codex warns about that at every start");

        Section("Hooks Codex knows about");
        var listed = HooksList(codex);
        if (listed == null)
        {
            // fall back to what the files say (trust can only be read from Codex itself)
            Warn("couldn't ask Codex (codex app-server hooks/list), so whether the hooks are trusted is unknown");
            var files = CodexConfig.AllHandlers();
            var mineInFiles = files.Where(h => h.Command.Contains("aipet-hook", StringComparison.OrdinalIgnoreCase)).ToList();
            foreach (var h in files)
            {
                bool mine = mineInFiles.Contains(h);
                var line = $"{h.Event,-18} {Short(h.Command, 90)}  ({Path.GetFileName(h.File)})";
                if (mine) Ok(line); else Info(line);
                foreach (var problem in Lint(h.Command)) (mine ? (Action<string>)Fail : Warn)($"   {problem}");
            }
            if (mineInFiles.Count == 0) Fail("AiPet's hooks aren't registered: run aipet-hook --install codex");
            else if (CodexConfig.Events.Any(e => !mineInFiles.Any(m => m.Event == e.Event)))
                Warn("AiPet isn't registered for every event (run aipet-hook --install codex)");
            bool up = CheckAiPet();
            if (mineInFiles.Count > 0) ProbeCodex(mineInFiles[0].Command, up);
            RecentEvents("codex");
            return;
        }
        var ours = listed.Where(h => ((string)h["command"] ?? "").Contains("aipet-hook", StringComparison.OrdinalIgnoreCase)).ToList();
        foreach (var h in listed)
        {
            var cmd = (string)h["command"] ?? "(" + (string)h["handlerType"] + ")";
            var trust = (string)h["trustStatus"];
            bool mine = ours.Contains(h);
            var line = $"{(string)h["eventName"],-18} {trust,-9} {Short(cmd, 90)}";
            if (mine && trust is "untrusted" or "modified") Fail(line + "  <- not trusted yet: run codex and choose Review hooks (or /hooks)");
            else if (mine && (bool?)h["enabled"] == false) Fail(line + "  <- disabled in /hooks");
            else if (mine) Ok(line);
            else Info(line);
            foreach (var problem in Lint(cmd)) (mine ? (Action<string>)Fail : Warn)($"   {problem}");
        }
        var missing = CodexConfig.Events.Select(e => e.Event).Where(e => !ours.Any(o => string.Equals((string)o["eventName"], char.ToLowerInvariant(e[0]) + e[1..], StringComparison.Ordinal))).ToList();
        if (ours.Count == 0) { Fail("AiPet's hooks aren't registered: run aipet-hook --install codex"); return; }
        if (missing.Count > 0) Warn("AiPet isn't registered for: " + string.Join(", ", missing) + " (run aipet-hook --install codex)");

        bool running = CheckAiPet();
        ProbeCodex((string)ours[0]["command"], running);
        RecentEvents("codex");
    }

    /// With --probe, AiPet's registered command run through the shell Codex uses.
    static void ProbeCodex(string command, bool running)
    {
        Section("Running AiPet's hook the way Codex does");
        if (!probe) Info("skipped: add --probe to run it through the shell Codex uses (PowerShell on Windows)");
        else if (OperatingSystem.IsWindows())
        {
            var shell = FindOnPath(new[] { "pwsh.exe" }) ?? Existing(@"C:\Program Files\PowerShell\7\pwsh.exe") ?? "powershell.exe";
            Probe(shell, new[] { "-NoProfile", "-Command", command }, null, 30, running);
            // "& '...'" is how the installer writes a path it can't write plainly: PowerShell only, by design. cmd.exe is
            // only Codex's rare fallback, so a failure there is a warning, not a problem
            if (command.TrimStart().StartsWith('&')) Info("not through the cmd.exe fallback: the command uses PowerShell's & '...' form, which cmd.exe can't run");
            else
            {
                Info("and through the cmd.exe fallback:");
                Probe(System.Environment.GetEnvironmentVariable("COMSPEC") ?? "cmd.exe", null, $"/C \"{command}\"", 30, running, onlyWarn: true);
            }
        }
        else
        {
            Probe("/bin/sh", new[] { "-c", command }, null, 30, running);
            if (System.Environment.GetEnvironmentVariable("SHELL") is { Length: > 0 } sh && sh != "/bin/sh")
                Probe(sh, new[] { "-lc", command }, null, 30, running);
        }
    }

    /// The [features] settings in config.toml however they're spelled: in a [features] table, as a root-level
    /// `features.x = ...`, or as an inline `features = { ... }`. Keys in other tables (a profile's, say) don't count.
    static List<(string Key, string Value)> Features(string toml)
    {
        const string Pair = @"[""']?([A-Za-z0-9_-]+)[""']?\s*=\s*([^\s,#}]+)";
        var found = new List<(string Key, string Value)>();
        string table = "", open = null;  // open: the delimiter of the multi-line string we're in
        foreach (var raw in toml.Split('\n'))
        {
            var line = raw.Trim();
            if (open != null) { if (line.Contains(open)) open = null; continue; }
            if (Regex.IsMatch(line, @"^\[.*\]\s*(#.*)?$")) table = Regex.Replace(line.Split('#')[0], @"[\[\]""'\s]", "");
            else if (table == "features" && Regex.Match(line, "^" + Pair) is { Success: true } m) found.Add((m.Groups[1].Value, m.Groups[2].Value));
            else if (table == "" && Regex.Match(line, @"^[""']?features[""']?\s*\.\s*" + Pair) is { Success: true } d) found.Add((d.Groups[1].Value, d.Groups[2].Value));
            else if (table == "" && Regex.Match(line, @"^[""']?features[""']?\s*=\s*\{([^}]*)") is { Success: true } inline)
                foreach (Match kv in Regex.Matches(inline.Groups[1].Value, @"(?:^|,)\s*" + Pair)) found.Add((kv.Groups[1].Value, kv.Groups[2].Value));
            // a multi-line string opened and not closed on this line: the lines up to its end are text, not keys
            if (Regex.Match(line, "=\\s*(\"\"\"|''')") is { Success: true } ml && !line[(ml.Index + ml.Length)..].Contains(ml.Groups[1].Value))
                open = ml.Groups[1].Value;
        }
        return found;
    }

    /// Problems in a hook command that make it fail before the program even starts.
    static IEnumerable<string> Lint(string cmd)
    {
        if (!OperatingSystem.IsWindows()) yield break;
        var t = cmd.TrimStart();
        if (t.StartsWith('"') || t.StartsWith('\'')) yield return "starts with a quoted path: PowerShell rejects that (exit code 1); use the path without quotes or with & in front";
        if (Regex.IsMatch(cmd, "%[A-Za-z_]+%")) yield return "%VARIABLE%s aren't expanded by PowerShell";
    }

    /// `codex app-server` over stdio: initialize, then hooks/list for the home folder.
    static List<JsonObject> HooksList(string codex)
    {
        if (codex == null) return null;
        try
        {
            var psi = new ProcessStartInfo(codex)
            {
                UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
                CreateNoWindow = true, StandardOutputEncoding = Encoding.UTF8,
            };
            psi.ArgumentList.Add("app-server");
            using var p = Process.Start(psi);
            p.ErrorDataReceived += (_, _) => { };
            p.BeginErrorReadLine();
            var stdin = new StreamWriter(p.StandardInput.BaseStream, new UTF8Encoding(false)) { AutoFlush = true, NewLine = "\n" };
            var deadline = DateTime.UtcNow.AddSeconds(30);
            JsonObject Await(int id)
            {
                while (DateTime.UtcNow < deadline)
                {
                    var read = p.StandardOutput.ReadLineAsync();
                    var left = deadline - DateTime.UtcNow;
                    if (left <= TimeSpan.Zero || !read.Wait(left) || read.Result == null) return null;
                    try { if (JsonNode.Parse(read.Result) is JsonObject o && o["id"]?.ToString() == id.ToString()) return o; } catch { }
                }
                return null;
            }
            try
            {
                stdin.WriteLine(new JsonObject { ["id"] = 1, ["method"] = "initialize", ["params"] = new JsonObject { ["clientInfo"] = new JsonObject { ["name"] = "aipet-doctor", ["title"] = "AiPet doctor", ["version"] = "1.0" } } }.ToJsonString());
                if (Await(1)?["result"] == null) return null;
                stdin.WriteLine(new JsonObject { ["method"] = "initialized" }.ToJsonString());
                var home = System.Environment.GetFolderPath(System.Environment.SpecialFolder.UserProfile);
                stdin.WriteLine(new JsonObject { ["id"] = 2, ["method"] = "hooks/list", ["params"] = new JsonObject { ["cwds"] = new JsonArray(home) } }.ToJsonString());
                var res = Await(2);
                return (res?["result"]?["data"] as JsonArray)?.OfType<JsonObject>()
                    .SelectMany(d => (d["hooks"] as JsonArray)?.OfType<JsonObject>() ?? Enumerable.Empty<JsonObject>()).ToList();
            }
            finally { try { p.Kill(entireProcessTree: true); } catch { } }
        }
        catch { return null; }
    }

    // ------------------------------------------------------------------ shared
    /// Run the hook like the agent does, with a test event that changes no state; check it starts, prints nothing and
    /// exits 0 well within the agent's time limit, and that the running pet got the event (the pet lists ignored events
    /// too). With the pet closed the hook must just return (within quickMs, when given). onlyWarn: a failure is a
    /// warning (a rare fallback).
    static void Probe(string file, string[] args, string rawArgs, int timeoutSec, bool running, bool onlyWarn = false, int quickMs = 0)
    {
        var bad = onlyWarn ? (Action<string>)Warn : Fail;
        // unique in its first 13 characters, which is what the pet lists
        var id = "doctor" + Guid.NewGuid().ToString("N")[..7] + "-aipet";
        var payload = new JsonObject { ["hook_event_name"] = "AipetDoctor", ["session_id"] = id, ["cwd"] = System.Environment.CurrentDirectory }.ToJsonString();
        var sw = Stopwatch.StartNew();
        var (code, stdout, stderr) = Capture(file, args, timeoutSec, payload, rawArgs);
        var ms = sw.ElapsedMilliseconds;
        var shown = rawArgs ?? string.Join(" ", args ?? Array.Empty<string>());
        var name = Path.GetFileName(file);
        if (code == -1) bad($"{name} {Short(shown, 70)}: didn't finish within {timeoutSec} s");
        else if (code != 0) bad($"{name}: exit code {code} after {ms} ms{(stderr.Length > 0 ? ": " + Short(stderr.Trim(), 200) : "")}");
        else if (stdout.Trim().Length > 0) bad($"{name}: printed output the agent would act on: {Short(stdout.Trim(), 120)}");
        else if (running && !PetGot(id[..13])) bad($"{name}: exited 0 but the pet didn't get the event (it isn't among the pet's recent events)");
        else if (!running && quickMs > 0 && ms > quickMs) Warn($"{name}: exited 0, but took {ms} ms to find the pet isn't running");
        else if (ms > timeoutSec * 1000 / 2) Warn($"{name}: works, but took {ms} ms of the {timeoutSec} s limit");
        else Ok(running ? $"{name}: ran, printed nothing, exit 0, {ms} ms, and the pet got the event"
                        : $"{name}: ran, printed nothing, exit 0, {ms} ms (the pet isn't running, so the hook did nothing)");
    }

    /// Whether the pet is running, by pinging it: without it the hooks do nothing.
    static bool CheckAiPet()
    {
        Section("AiPet");
        var pong = Ping(out bool running);
        if (!running) Warn("The pet isn't running; hooks do nothing until it is.");
        else if (pong == null) Fail($"something listens on {Ipc.Endpoint} but doesn't answer like the pet (another version?)");
        else Ok($"the pet is running (pid {pong[Ipc.Pid]}), listening on {Ipc.Endpoint}");
        return running;
    }

    /// The last events of this agent the running pet got, without the doctor's own.
    static void RecentEvents(string agent)
    {
        Section($"Recent events the pet got (all of them are in {Paths.HookEventsLog})");
        var pong = Ping(out bool running);
        if (!running) { Info("none: the pet isn't running"); return; }
        var lines = Recent(pong).Where(l => l.Contains($" {agent} pid=") && !l.Contains("AipetDoctor")).TakeLast(8).ToList();
        if (lines.Count == 0) Info("none yet");
        foreach (var l in lines) Info(l);
    }

    /// Whether the pet lists an event of this session among its recent ones, waiting up to 5 s for it.
    static bool PetGot(string sid13)
    {
        var until = DateTime.UtcNow.AddSeconds(5);
        while (true)
        {
            if (Recent(Ping(out _)).Any(l => l.Contains(sid13))) return true;
            if (DateTime.UtcNow > until) return false;
            Thread.Sleep(100);
        }
    }

    /// The running pet's answer to a ping ({ok, app, pid, recent}; see Ipc). Null with running false when it isn't
    /// running; null with running true when what listens answers something else.
    static JsonObject Ping(out bool running)
    {
        running = false;
        try
        {
            using var pet = Ipc.Connect();
            if (pet == null) return null;
            running = true;
            var reply = Ipc.Ask(pet, new JsonObject { [Ipc.V] = Ipc.Version, [Ipc.Kind] = "ping" }.ToJsonString());
            return JsonNode.Parse(reply ?? "null") is JsonObject o && True(o[Ipc.Ok]) ? o : null;
        }
        catch { return null; }
    }

    /// The pet's recent hook-events.log lines, from a ping's answer (both agents').
    static IEnumerable<string> Recent(JsonObject pong) =>
        (pong?[Ipc.Recent] as JsonArray)?.Select(n => n is JsonValue v && v.TryGetValue(out string s) ? s : null).Where(s => s != null)
        ?? Enumerable.Empty<string>();

    static (int Code, string Out, string Err) Capture(string file, string[] args, int timeoutSec, string stdin = null, string rawArgs = null)
    {
        try
        {
            var psi = new ProcessStartInfo(file)
            {
                UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
                CreateNoWindow = true, StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8,
            };
            if (rawArgs != null) psi.Arguments = rawArgs;
            else foreach (var a in args ?? Array.Empty<string>()) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi);
            var o = p.StandardOutput.ReadToEndAsync();
            var e = p.StandardError.ReadToEndAsync();
            if (stdin != null) p.StandardInput.Write(stdin);
            p.StandardInput.Close();
            if (!p.WaitForExit(timeoutSec * 1000)) { try { p.Kill(entireProcessTree: true); } catch { } return (-1, "", ""); }
            return (p.ExitCode, o.Result, e.Result);
        }
        catch (Exception ex) { return (-2, "", ex.Message); }
    }

    static string FindOnPath(string[] names)
    {
        foreach (var dir in (System.Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator))
            foreach (var n in names)
                try { var f = Path.Combine(dir.Trim('"'), n); if (File.Exists(f)) return f; } catch { }
        return null;
    }

    static string Existing(string path) => File.Exists(path) ? path : null;

    static string Short(string s, int n) => s.Length <= n ? s : s[..(n - 3)] + "...";
}
