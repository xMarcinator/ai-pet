using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace AiPet;

/// Registers AiPet's hooks with Codex (user layer: ~/.codex/config.toml, or hooks.json if that's where the user's
/// hooks already are, since Codex warns when one layer uses both).
///
/// Rules, from how Codex 0.156 loads and trusts hooks:
/// - Codex trusts a hook by "<file>:<event>:<group>:<index>" plus a hash of its definition, and silently skips
///   hooks that aren't trusted. So other tools' hooks must keep their position and bytes: the file is edited as
///   text (never re-serialised), and AiPet's groups are always appended after everything else.
/// - AiPet's hooks are found by their command (it contains "aipet-hook"), not only by the marker comments, since
///   Codex rewrites the file when it records trust.
/// - Re-installing an identical definition changes nothing, wherever it sits, so trust is kept.
static class CodexConfig
{
    public static string ConfigToml => Path.Combine(Paths.CodexHome, "config.toml");
    public static string HooksJson => Path.Combine(Paths.CodexHome, "hooks.json");
    const string Begin = "# >>> AiPet hooks (written by aipet-hook --install codex; Codex asks you to trust them again if they change) >>>";
    const string End = "# <<< AiPet hooks <<<";
    const char CR = (char)13, LF = (char)10;

    /// The events AiPet listens to. All run in the background (the hook prints nothing, so Codex needn't wait).
    /// On Windows each run starts PowerShell (1-4 s), so events it can do without are left out there: PostToolUse
    /// (the next event supersedes it), and Interrupt/SessionEnd, whose 3 s limit PowerShell can't reliably meet.
    /// Compaction and sub-agents are rare enough to be worth it everywhere.
    public static (string Event, bool Async, int Timeout)[] Events => OperatingSystem.IsWindows()
        ? new[] { ("SessionStart", true, 30), ("UserPromptSubmit", true, 30), ("PreToolUse", true, 30), ("PermissionRequest", true, 30), ("Stop", true, 30),
                  ("PreCompact", true, 30), ("PostCompact", true, 30), ("SubagentStart", true, 30), ("SubagentStop", true, 30) }
        : new[] { ("SessionStart", true, 30), ("UserPromptSubmit", true, 30), ("PreToolUse", true, 30), ("PermissionRequest", true, 30),
                  ("PostToolUse", true, 30), ("Stop", true, 30), ("PreCompact", true, 30), ("PostCompact", true, 30),
                  ("SubagentStart", true, 30), ("SubagentStop", true, 30), ("Interrupt", false, 3), ("SessionEnd", false, 3) };

    /// The events of the aipet plugin (plugins/aipet/hooks/codex.json): a plugin can't pick them per OS, so it has
    /// the Windows set everywhere. Frozen with the plugin's hook definitions.
    public static readonly string[] PluginEvents =
        { "SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "Stop", "PreCompact", "PostCompact", "SubagentStart", "SubagentStop" };

    public static string Snake(string ev) => Regex.Replace(ev, "(?<=[a-z])([A-Z])", "_$1").ToLowerInvariant();

    /// AiPet's hooks: the command runs aipet-hook, or AIPET-~1.EXE (older installs shortened the file name too).
    internal static bool IsOurs(string command) => command != null
        && (command.Contains("aipet-hook", StringComparison.OrdinalIgnoreCase)
            || Regex.IsMatch(command, @"AIPET-~\d+\.EXE\b.*--agent\s+codex", RegexOptions.IgnoreCase));

    // ------------------------------------------------------------------ the command
    /// The command Codex runs. Windows runs it with PowerShell (`pwsh -NoProfile -Command <command>`), and with
    /// cmd.exe in a rare fallback: a quoted program path is a PowerShell syntax error (exit 1 before anything
    /// starts), so the path is written without quotes, with its folder in 8.3 short form if it has spaces. The file
    /// name stays aipet-hook.exe (that's how AiPet knows its own hooks), and so do the backslashes, since cmd.exe
    /// takes "/" for a switch (older installs used "/", so Codex asks to trust the hooks once more after that).
    /// On Linux/macOS it runs with `sh -c` (or bash/zsh), where a single-quoted path is always safe.
    public static string BuildCommand(string exe)
    {
        if (OperatingSystem.IsWindows())
        {
            static bool Plain(string p) => Regex.IsMatch(p, @"^[A-Za-z0-9_.:/\\~-]+$");
            var p = exe;
            if (!Plain(p) && Path.GetDirectoryName(p) is { } dir && ShortPath(dir) is { } shortDir)
                p = Path.Combine(shortDir, Path.GetFileName(p));
            return Plain(p) ? $"{p} --agent codex" : $"& '{p.Replace("'", "''")}' --agent codex";  // PowerShell only
        }
        return $"'{exe.Replace("'", "'\\''")}' --agent codex";
    }

    /// C:\Program Files\... -> C:\PROGRA~1\... so the path needs no quotes in any shell. Null if there's no short name.
    static string ShortPath(string path)
    {
        if (!OperatingSystem.IsWindows()) return null;
        try
        {
            var buf = new char[1024];
            int n = GetShortPathNameW(path, buf, buf.Length);
            return n > 0 && n < buf.Length ? new string(buf, 0, n) : null;
        }
        catch { return null; }
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] static extern int GetShortPathNameW(string path, char[] buf, int size);

    // ------------------------------------------------------------------ install / uninstall
    public static int Install(string exe)
    {
        // the pet gets every event either way, so this isn't a failure: installers run --install without checking
        if (Plugin() is { } plugin)
        {
            // hooks registered here earlier would report every event a second time: the plugin replaces them
            bool removed = EditToml(ConfigToml, null, removeTrustFor: new[] { HooksJson });
            removed |= EditHooksJson(null);
            Console.WriteLine(removed
                ? $"The {plugin} plugin is enabled in {ConfigToml} and reports every event, so the AiPet hooks registered earlier were removed (the plugin replaces them)."
                : $"The {plugin} plugin is enabled in {ConfigToml} and already reports every event, so no hooks were registered (they would report each one twice).");
            Console.WriteLine($"To use hooks in Codex's config instead, remove the plugin (codex plugin remove {plugin}) and install again.");
            return 0;
        }
        var command = BuildCommand(exe);
        bool useJson = ChooseHooksJson();
        var target = useJson ? HooksJson : ConfigToml;
        // TOML doesn't allow adding [[hooks.<Event>]] tables to hooks defined as values: appending would break the file
        if (!useJson && InlineHooks(ParseToml(Read(ConfigToml))) is { } clash)
        {
            Console.Error.WriteLine($"{ConfigToml} defines hooks in a form AiPet can't add to without breaking the file:");
            Console.Error.WriteLine($"  {clash}");
            Console.Error.WriteLine("Write those hooks as [[hooks.<Event>]] tables (or move them to hooks.json), then install again. Nothing was changed.");
            return 1;
        }
        // config.toml first: it holds the trust entries, which are found from the hooks as they are before any edit
        bool changed;
        if (useJson)
        {
            changed = EditToml(ConfigToml, null, removeTrustFor: Array.Empty<string>());  // AiPet leftovers in config.toml
            changed |= EditHooksJson(command);
        }
        else
        {
            changed = EditToml(ConfigToml, command, removeTrustFor: new[] { HooksJson });
            changed |= EditHooksJson(null);  // an older install's hooks.json entries
        }
        Console.WriteLine(changed ? $"AiPet hooks registered for Codex in {target}" : $"AiPet hooks for Codex are already registered in {target} (unchanged)");
        Console.WriteLine($"  command: {command}");
        Console.WriteLine($"  events:  {string.Join(", ", Events.Select(e => e.Event))}");
        Console.WriteLine();
        Console.WriteLine("Codex runs new hooks only after you trust them:");
        Console.WriteLine("  1. In a terminal, run: codex");
        Console.WriteLine("  2. At \"Hooks need review\" choose Review hooks and trust the AiPet ones (\"aipet-hook ... --agent codex\").");
        Console.WriteLine("     \"Trust all\" would also trust any other hooks waiting for review. Later: /hooks.");
        Console.WriteLine("  3. Restart the ChatGPT/Codex desktop app so it picks up the trust.");
        Console.WriteLine("Then check with: aipet-hook --doctor codex");
        return 0;
    }

    public static int Uninstall()
    {
        bool changed = EditToml(ConfigToml, null, removeTrustFor: new[] { HooksJson });
        changed |= EditHooksJson(null);
        Console.WriteLine(changed ? "AiPet hooks removed from Codex" : "No AiPet hooks were registered with Codex");
        return 0;
    }

    /// hooks.json only when the user's other hooks already live there and config.toml has none inline.
    static bool ChooseHooksJson()
    {
        if (ParseToml(Read(ConfigToml)).Any(s => s.Handler != null && !IsOurs(s.Command))) return false;
        try
        {
            if (JsonNode.Parse(File.ReadAllText(HooksJson))?["hooks"] is JsonObject h)
                return JsonPositions(h).Any(x => !IsOurs(CommandOf(x.Node)));
        }
        catch { }
        return false;
    }

    /// The AiPet plugin whose hooks Codex runs (its id, e.g. aipet@aipet), or null. Codex runs them while config.toml
    /// has a [plugins."<id>"] entry that isn't `enabled = false` and the plugin is in its cache
    /// (plugins/cache/<marketplace>/<plugin>/<version>/); `codex plugin remove` takes both away.
    public static string Plugin()
    {
        var found = new Dictionary<string, bool>();  // id -> disabled
        foreach (var s in ParseToml(Read(ConfigToml)))
        {
            if (s.IsArray || (s.Header != null && s.Table == null)) continue;
            var table = s.Table ?? Array.Empty<string>();
            if (table is ["plugins", var name]) found.TryAdd(name, false);
            foreach (var (key, value, _) in s.Values)
                switch (table.Concat(key).ToArray())
                {
                    case ["plugins", var id, "enabled"]: found[id] = value == "false"; break;
                    case ["plugins", var id]: found[id] = Regex.IsMatch(value, @"^\{.*\benabled\s*=\s*false\b"); break;  // an inline table
                    case ["plugins", var id, ..]: found.TryAdd(id, false); break;
                }
        }
        return found.Where(p => !p.Value && p.Key.Split('@') is ["aipet", var market]
                                && Directory.Exists(Path.Combine(Paths.CodexHome, "plugins", "cache", market, "aipet"))
                                && Directory.EnumerateDirectories(Path.Combine(Paths.CodexHome, "plugins", "cache", market, "aipet")).Any())
                    .Select(p => p.Key).FirstOrDefault();
    }

    static string Read(string path) { try { return File.ReadAllText(path); } catch { return ""; } }

    /// The file a symlinked config file (e.g. from a dotfiles repo) points at, so an edit replaces that file and
    /// not the link.
    static string RealPath(string path)
    {
        try { return File.ResolveLinkTarget(path, returnFinalTarget: true)?.FullName ?? path; }
        catch { return path; }
    }

    /// Every command hook in the user's config.toml and hooks.json (for the doctor, when Codex itself can't be asked).
    public static List<(string Event, string Command, string File)> AllHandlers() =>
        TomlHandlers(ParseToml(Read(ConfigToml))).Select(h => (h.Event, h.Command, ConfigToml))
            .Concat(JsonHandlers(HooksJson).Select(h => (h.Event, h.Command, HooksJson)))
            .Where(h => h.Command != null).ToList();

    // ------------------------------------------------------------------ config.toml as text
    /// One table of the file: its header line and the lines up to the next header.
    sealed class Segment
    {
        public string Header;           // e.g. "[[hooks.Stop.hooks]]"; null for the text before the first table
        public string[] Table;          // the header's key, e.g. ["hooks", "Stop", "hooks"]; null if none or unreadable
        public bool IsArray;            // [[...]]: an entry of an array of tables
        public bool Unreadable;         // a hooks table whose header AiPet can't make sense of
        public List<string> Lines = new();
        public int LastContent = -1;    // index in Lines of the last line that isn't blank or a comment
        public List<(string[] Key, string Value, string Line)> Values = new();  // key/value lines, strings by value
        public string Group;            // event name, for [[hooks.<Event>]]
        public string Handler;          // event name, for [[hooks.<Event>.hooks]]
        public string StateKey;         // for [hooks.state.'<key>']
        public string Command;          // a handler's command value
    }

    static List<Segment> ParseToml(string text)
    {
        var segs = new List<Segment> { new() };
        string open = null;  // the delimiter of a multi-line string still open at the start of the line
        foreach (var line in SplitLines(text))
        {
            var t = line.Trim();
            if (open == null && t.StartsWith('['))
            {
                var s = new Segment { Header = t };
                s.Table = HeaderKey(t, out s.IsArray);
                if (s.IsArray && s.Table is ["hooks", var ev]) s.Group = ev;
                else if (s.IsArray && s.Table is ["hooks", var ev2, "hooks"]) s.Handler = ev2;
                else if (!s.IsArray && s.Table is ["hooks", "state", var key]) s.StateKey = key;
                else s.Unreadable = s.Table == null ? t.Contains("hooks", StringComparison.OrdinalIgnoreCase)
                    : s.Table[0] == "hooks" && (s.IsArray || s.Table.Length > 2 || (s.Table.Length == 2 && s.Table[1] != "state"));
                segs.Add(s);
            }
            var seg = segs[^1];
            seg.Lines.Add(line);
            // track strings, so a "[" or "#" inside one (even a multi-line one) isn't taken for a header or a comment
            bool inString = open != null;
            open = ScanLine(line, open, out int comment);
            if (inString || (t.Length > 0 && t[0] != '#')) seg.LastContent = seg.Lines.Count - 1;
            if (inString || t.Length == 0 || t[0] is '[' or '#') continue;
            int i = 0;
            if (ReadKey(line, ref i) is { } k && i < line.Length && line[i] == '=')
            {
                var value = line[(i + 1)..(comment > i ? comment : line.Length)].Trim();
                var str = StringValue(value);
                seg.Values.Add((k, str != null ? "s:" + str : value, line));
                if (seg.Handler != null && k is ["command"]) seg.Command = str;
            }
        }
        return segs;
    }

    static IEnumerable<string> SplitLines(string text)
    {
        int start = 0;
        for (int i = 0; i < text.Length; i++)
            if (text[i] == LF) { yield return text[start..(i + 1)]; start = i + 1; }
        if (start < text.Length) yield return text[start..];
    }

    /// Walks one line and returns the delimiter (""" or ''') of a multi-line string still open at its end, or null.
    /// comment: where a comment starts on the line, or -1.
    static string ScanLine(string line, string open, out int comment)
    {
        comment = -1;
        for (int i = 0; i < line.Length; )
        {
            if (open != null)
            {
                i = CloseMultiline(line, i, open);
                if (i < 0) return open;
                open = null;
            }
            else if (line[i] == '#') { comment = i; return null; }
            else if (line[i] is '"' or '\'')
            {
                var triple = new string(line[i], 3);
                if (string.CompareOrdinal(line, i, triple, 0, 3) == 0) { open = triple; i += 3; }
                else
                {
                    i = SkipString(line, i);
                    if (i < 0) return null;
                }
            }
            else i++;
        }
        return open;
    }

    /// Index just past a one-line string that starts at s[i] ("basic" or 'literal'); -1 if it isn't closed.
    static int SkipString(string s, int i)
    {
        char q = s[i];
        for (int j = i + 1; j < s.Length; j++)
        {
            if (s[j] == '\\' && q == '"') j++;
            else if (s[j] == q) return j + 1;
        }
        return -1;
    }

    /// Index just past the delimiter that closes a multi-line string, looking from s[i]; -1 if it's not on this line.
    static int CloseMultiline(string s, int i, string delim)
    {
        for (int j = i; j < s.Length; j++)
        {
            if (s[j] == '\\' && delim[0] == '"') { j++; continue; }
            if (string.CompareOrdinal(s, j, delim, 0, 3) != 0) continue;
            int k = j + 3;
            while (k < s.Length && k < j + 5 && s[k] == delim[0]) k++;  // up to two more quotes are the string's own
            return k;
        }
        return -1;
    }

    static void Ws(string s, ref int i) { while (i < s.Length && (s[i] == ' ' || s[i] == (char)9)) i++; }

    /// A dotted key from s[i] (bare or quoted parts, spaces around the dots allowed): `hooks . "Stop"` -> ["hooks",
    /// "Stop"]. i ends past it and the spaces after it. Null if there's no well-formed key there.
    static string[] ReadKey(string s, ref int i)
    {
        var parts = new List<string>();
        while (true)
        {
            Ws(s, ref i);
            if (i >= s.Length) return null;
            int start = i;
            if (s[i] is '"' or '\'')
            {
                if (string.CompareOrdinal(s, i, new string(s[i], 3), 0, 3) == 0) return null;  // keys can't be multi-line
                int end = SkipString(s, i);
                if (end < 0) return null;
                var part = s[i] == '"' ? Unescape(s[(i + 1)..(end - 1)]) : s[(i + 1)..(end - 1)];
                if (part == null) return null;
                parts.Add(part);
                i = end;
            }
            else
            {
                while (i < s.Length && (char.IsAsciiLetterOrDigit(s[i]) || s[i] is '_' or '-')) i++;
                if (i == start) return null;
                parts.Add(s[start..i]);
            }
            Ws(s, ref i);
            if (i < s.Length && s[i] == '.') { i++; continue; }
            return parts.ToArray();
        }
    }

    /// The key of a table header, e.g. `[[ hooks . "Stop" ]]` -> ["hooks", "Stop"] with isArray set; null if the
    /// line isn't a well-formed header.
    static string[] HeaderKey(string header, out bool isArray)
    {
        isArray = header.StartsWith("[[", StringComparison.Ordinal);
        int i = isArray ? 2 : 1;
        var key = ReadKey(header, ref i);
        var close = isArray ? "]]" : "]";
        if (key == null || string.CompareOrdinal(header, i, close, 0, close.Length) != 0) return null;
        i += close.Length;
        Ws(header, ref i);
        return i == header.Length || header[i] == '#' ? key : null;
    }

    /// A one-line TOML string's value ("basic" or 'literal'); null for anything else.
    static string StringValue(string v)
    {
        if (v.Length < 2 || v[0] is not ('"' or '\'') || string.CompareOrdinal(v, 0, new string(v[0], 3), 0, 3) == 0) return null;
        if (SkipString(v, 0) != v.Length) return null;
        return v[0] == '"' ? Unescape(v[1..^1]) : v[1..^1];
    }

    /// What a TOML basic string (the part between the quotes) stands for; null if it has an escape TOML doesn't know.
    static string Unescape(string s)
    {
        var sb = new StringBuilder(s.Length);
        for (int i = 0; i < s.Length; i++)
        {
            if (s[i] != '\\') { sb.Append(s[i]); continue; }
            if (++i == s.Length) return null;
            char c = s[i];
            if (c is 'x' or 'u' or 'U')
            {
                int n = c == 'x' ? 2 : c == 'u' ? 4 : 8;
                if (i + n >= s.Length
                    || !uint.TryParse(s.AsSpan(i + 1, n), System.Globalization.NumberStyles.AllowHexSpecifier, null, out uint cp)
                    || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) return null;
                sb.Append(char.ConvertFromUtf32((int)cp));
                i += n;
                continue;
            }
            char? plain = c switch
            {
                'b' => (char)8, 't' => (char)9, 'n' => LF, 'f' => (char)12, 'r' => CR, 'e' => (char)27, '"' => '"', '\\' => '\\',
                _ => null,
            };
            if (plain == null) return null;
            sb.Append(plain.Value);
        }
        return sb.ToString();
    }

    /// The trust keys Codex gives AiPet's handlers in a hooks file: "<file>:<event>:<group>:<index>".
    static HashSet<string> OurKeys(string file, IEnumerable<(string Event, int Group, int Index, string Command)> handlers) =>
        handlers.Where(h => IsOurs(h.Command)).Select(h => $"{file}:{Snake(h.Event)}:{h.Group}:{h.Index}")
                .ToHashSet(StringComparer.OrdinalIgnoreCase);

    /// AiPet's handlers in config.toml by trust key, with their definitions.
    static Dictionary<string, string> OurDefs(string file, List<Segment> segs)
    {
        var defs = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var (s, g, i) in TomlPositions(segs))
            if (IsOurs(s.Command)) defs[$"{file}:{Snake(s.Handler)}:{g}:{i}"] = Normal(s);
        return defs;
    }

    /// Where Codex puts each handler: by TOML rules it belongs to the latest [[hooks.<Event>]] of its event above
    /// it, whatever other tables come between.
    static IEnumerable<(Segment Seg, int Group, int Index)> TomlPositions(List<Segment> segs)
    {
        var groupCount = new Dictionary<string, int>();
        var handlerCount = new Dictionary<string, int>();
        foreach (var s in segs)
        {
            if (s.Group != null) { groupCount[s.Group] = groupCount.GetValueOrDefault(s.Group) + 1; handlerCount[s.Group] = 0; }
            else if (s.Handler != null && groupCount.TryGetValue(s.Handler, out var g))
                yield return (s, g - 1, handlerCount[s.Handler]++);
        }
    }

    static IEnumerable<(string Event, int Group, int Index, string Command)> TomlHandlers(List<Segment> segs) =>
        TomlPositions(segs).Select(p => (p.Seg.Handler, p.Group, p.Index, p.Seg.Command));

    /// A line (or header) of config.toml that defines hooks as values (`hooks = {...}`, `hooks.Stop = [...]`, or
    /// `Stop = [...]` under [hooks]) or as a plain [hooks.Stop] table, for an event AiPet adds [[hooks.<Event>]]
    /// tables to. TOML doesn't allow both. Null if there's none.
    static string InlineHooks(List<Segment> segs)
    {
        var events = Events.Select(e => e.Event).ToHashSet();
        bool Clash(string[] key) => key is ["hooks"] || (key is ["hooks", var ev, ..] && events.Contains(ev));
        foreach (var s in segs)
        {
            if (s.IsArray || (s.Header != null && s.Table == null)) continue;  // keys under [[...]] belong to its entry
            var table = s.Table ?? Array.Empty<string>();
            if (table.Length >= 2 && Clash(table)) return s.Header;
            foreach (var (key, _, line) in s.Values)
                if (Clash(table.Concat(key).ToArray())) return line.Trim();
        }
        return null;
    }

    /// Rewrite config.toml without AiPet's hooks (and their trust entries, also those for removeTrustFor files),
    /// then, if command is given, append AiPet's block. Returns whether the file changed.
    static bool EditToml(string path, string command, string[] removeTrustFor)
    {
        var real = RealPath(path);  // a symlinked config.toml (dotfiles): edit the file it points at, not the link
        for (int attempt = 0; ; attempt++)
        {
            if (!File.Exists(real) && command == null) return false;
            var stamp = File.Exists(real) ? File.GetLastWriteTimeUtc(real) : DateTime.MinValue;
            var text = Read(real);
            var segs = ParseToml(text);

            // trust entries of AiPet hooks in the other file (an older install's hooks.json), found before it's edited
            var otherTrust = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (var other in removeTrustFor.Where(f => !string.Equals(f, path, StringComparison.OrdinalIgnoreCase)))
                otherTrust.UnionWith(OurKeys(other, JsonHandlers(other)));

            // already exactly right: keep AiPet's hooks and their trust, only drop other files' leftovers
            bool keepOurs = command != null && AlreadyRegistered(segs, command);
            if (keepOurs && !segs.Any(s => s.StateKey != null && otherTrust.Contains(s.StateKey))) return false;

            // otherwise drop AiPet's handlers and the groups left with no one else's (counted as Codex does), and
            // see where everything ends up once AiPet's block is appended again
            var drop = new HashSet<Segment>();
            if (!keepOurs)
                for (int i = 0; i < segs.Count; i++)
                    if ((segs[i].Handler != null && IsOurs(segs[i].Command))
                        || (segs[i].Group != null && GroupHandlers(segs, i).Any() && GroupHandlers(segs, i).All(h => IsOurs(h.Command))))
                        drop.Add(segs[i]);
            var after = segs.Where(s => !drop.Contains(s)).ToList();
            if (command != null && !keepOurs) after.AddRange(ParseToml(Block(command, LF.ToString())));

            // trust entries to drop: other files' AiPet leftovers, and AiPet's own whose key won't hold the same
            // definition any more (the rest still apply, so they stay)
            var trustToDrop = new HashSet<string>(otherTrust, StringComparer.OrdinalIgnoreCase);
            if (!keepOurs)
            {
                var was = OurDefs(path, segs);
                var will = OurDefs(path, after);
                var odd = segs.FirstOrDefault(s => s.Unreadable);
                if (odd == null) trustToDrop.UnionWith(was.Where(kv => !will.TryGetValue(kv.Key, out var d) || d != kv.Value).Select(kv => kv.Key));
                else if (was.Count > 0) Console.WriteLine($"Note: AiPet can't make sense of {odd.Header} in config.toml, so it left its old hooks' trust entries alone.");
                WarnIfShifting(segs, after);
            }
            drop.UnionWith(segs.Where(s => s.StateKey != null && trustToDrop.Contains(s.StateKey)));

            var result = Render(segs, drop, markers: keepOurs);
            string nl = text.Contains(CR) ? new string(new[] { CR, LF }) : LF.ToString();

            if (command != null && !keepOurs)
            {
                result = result.TrimEnd(CR, LF, ' ', (char)9) + nl + nl + Block(command, nl);
            }
            else if (result != text && !keepOurs)
            {
                // tidy the gap our block left at the end of the file
                result = result.TrimEnd(CR, LF, ' ', (char)9) + nl;
            }
            if (result == text) return false;

            Directory.CreateDirectory(Path.GetDirectoryName(real)!);
            if (File.Exists(path)) Backup(path);
            if (File.Exists(real) && File.GetLastWriteTimeUtc(real) != stamp && attempt < 3) continue;  // Codex wrote it meanwhile
            AiPet.Install.Save(real, result);
            return true;
        }
    }

    /// The text without the dropped segments, except for comments that trail one (the user's, about what follows),
    /// and without AiPet's marker lines unless markers is set.
    static string Render(List<Segment> segs, HashSet<Segment> drop, bool markers)
    {
        static bool Marker(string l) => l.TrimStart().StartsWith("# >>> AiPet hooks", StringComparison.Ordinal)
                                        || l.TrimStart().StartsWith("# <<< AiPet hooks", StringComparison.Ordinal);
        var sb = new StringBuilder();
        foreach (var s in segs)
        {
            var lines = s.Lines;
            if (drop.Contains(s))
            {
                var tail = s.Lines.Skip(s.LastContent + 1).ToList();
                lines = tail.Any(l => l.TrimStart().StartsWith('#') && !Marker(l)) ? tail : new List<string>();
            }
            foreach (var line in lines)
                if (markers || !Marker(line)) sb.Append(line);
        }
        return sb.ToString();
    }

    /// A group's handlers by TOML rules: every [[hooks.<Event>.hooks]] after it up to the next [[hooks.<Event>]],
    /// whatever other tables come between.
    static IEnumerable<Segment> GroupHandlers(List<Segment> segs, int groupIndex)
    {
        var ev = segs[groupIndex].Group;
        for (int j = groupIndex + 1; j < segs.Count && segs[j].Group != ev; j++)
            if (segs[j].Handler == ev) yield return segs[j];
    }

    /// AiPet's handlers are exactly the ones we'd write (same events and definitions), wherever they are: moving
    /// them to the end would only shift the hooks after them and cost their trust.
    static bool AlreadyRegistered(List<Segment> segs, string command)
    {
        var ours = segs.Where(s => s.Handler != null && IsOurs(s.Command)).Select(s => (s.Handler, Normal(s))).ToList();
        var want = ParseToml(Block(command, LF.ToString())).Where(s => s.Handler != null).Select(s => (s.Handler, Normal(s))).ToList();
        return ours.Count == want.Count && want.All(ours.Contains);
    }

    /// A handler's definition, for comparing: its keys and values, sorted, strings by their value (so quoting and
    /// escapes don't matter), without comments or layout.
    static string Normal(Segment s) =>
        string.Join("|", s.Values.Select(v => string.Join(".", v.Key) + "=" + v.Value).OrderBy(l => l, StringComparer.Ordinal));

    static IEnumerable<string> HandlerLines((string Event, bool Async, int Timeout) e, string command, string nl)
    {
        yield return $"[[hooks.{e.Event}.hooks]]{nl}";
        yield return $"type = \"command\"{nl}";
        yield return $"command = \"{command.Replace("\\", "\\\\").Replace("\"", "\\\"")}\"{nl}";
        yield return $"timeout = {e.Timeout}{nl}";
        if (e.Async) yield return $"async = true{nl}";
    }

    static string Block(string command, string nl)
    {
        var sb = new StringBuilder().Append(Begin).Append(nl);
        foreach (var e in Events)
        {
            sb.Append($"[[hooks.{e.Event}]]{nl}");
            foreach (var l in HandlerLines(e, command, nl)) sb.Append(l);
            sb.Append(nl);
        }
        return sb.Append(End).Append(nl).ToString();
    }

    /// Removing AiPet's hooks moves the other handlers after them in the same event (to another group or index),
    /// which breaks their recorded trust. AiPet appends its groups last, so this only happens if something was
    /// added after them; say so instead of hiding it.
    static void WarnIfShifting(List<Segment> before, List<Segment> after)
    {
        var now = TomlPositions(after).ToDictionary(p => p.Seg, p => (p.Group, p.Index));
        var warned = new HashSet<string>();
        foreach (var (s, g, i) in TomlPositions(before))
            if (!IsOurs(s.Command) && now.TryGetValue(s, out var at) && at != (g, i) && warned.Add(s.Handler))
                Console.WriteLine($"Note: another {s.Handler} hook comes after AiPet's in config.toml; Codex will ask you to trust it again.");
    }

    /// config.toml.aipet-<time>.bak before each change; the newest three are kept.
    static void Backup(string path)
    {
        try
        {
            File.Copy(path, $"{path}.aipet-{DateTime.Now:yyyyMMdd-HHmmss}.bak", overwrite: true);
            var dir = Path.GetDirectoryName(path)!;
            foreach (var old in Directory.GetFiles(dir, Path.GetFileName(path) + ".aipet-*.bak").OrderByDescending(f => f).Skip(3))
                File.Delete(old);
        }
        catch { }
    }

    // ------------------------------------------------------------------ hooks.json
    static string CommandOf(JsonNode hook) => hook is JsonObject o && o["command"] is JsonValue v && v.TryGetValue(out string s) ? s : null;

    /// Every handler in a hooks.json "hooks" object, with the group and index its trust key uses.
    static IEnumerable<(string Event, int Group, int Index, JsonNode Node)> JsonPositions(JsonObject hooks)
    {
        foreach (var (ev, value) in hooks)
            if (value is JsonArray groups)
                for (int g = 0; g < groups.Count; g++)
                    if (groups[g] is JsonObject group && group["hooks"] is JsonArray list)
                        for (int i = 0; i < list.Count; i++)
                            yield return (ev, g, i, list[i]);
    }

    static IEnumerable<(string Event, int Group, int Index, string Command)> JsonHandlers(string path)
    {
        JsonObject hooks = null;
        try { hooks = JsonNode.Parse(File.ReadAllText(path))?["hooks"] as JsonObject; } catch { }
        if (hooks == null) yield break;
        foreach (var h in JsonPositions(hooks)) yield return (h.Event, h.Group, h.Index, CommandOf(h.Node));
    }

    /// A handler in a hooks file. Only the plugin's has commandWindows (PluginHooks), which Codex runs instead of
    /// command on Windows: a user's hooks.json is written for the OS it's on.
    internal static JsonObject JsonHandler((string Event, bool Async, int Timeout) e, string command, string commandWindows = null)
    {
        var h = new JsonObject { ["type"] = "command", ["command"] = command };
        if (commandWindows != null) h["commandWindows"] = commandWindows;
        h["timeout"] = e.Timeout;
        if (e.Async) h["async"] = true;
        return h;
    }

    /// AiPet's handlers in hooks.json are exactly the ones we'd write, one per event, wherever they are.
    static bool JsonRegistered(JsonObject hooks, string command)
    {
        var ours = JsonPositions(hooks).Where(h => IsOurs(CommandOf(h.Node))).ToList();
        return ours.Count == Events.Length
            && Events.All(e => ours.Any(o => o.Event == e.Event && JsonNode.DeepEquals(o.Node, JsonHandler(e, command))));
    }

    /// Remove AiPet's handlers from hooks.json and, if command is given, append AiPet's groups. Nothing is written
    /// when AiPet's handlers are already exactly right (wherever they are, so the trust Codex recorded holds), and
    /// the file is deleted when nothing but an empty "hooks" object is left. Returns whether it changed.
    static bool EditHooksJson(string command)
    {
        var path = HooksJson;
        if (!File.Exists(path) && command == null) return false;
        var before = Read(path);
        JsonObject root;
        try { root = (string.IsNullOrWhiteSpace(before) ? null : JsonNode.Parse(before) as JsonObject) ?? new JsonObject(); }
        catch (JsonException) { Console.Error.WriteLine($"{path} isn't valid JSON; leaving it alone"); return false; }
        if (root["hooks"] is not JsonObject hooks)
        {
            if (command == null) return false;  // nothing of AiPet's to remove, and no empty "hooks" to add
            root["hooks"] = hooks = new JsonObject();
        }
        if (command != null && JsonRegistered(hooks, command)) return false;

        // where the other hooks are now, to say so if removing AiPet's moves them (their trust is tied to that)
        var was = new Dictionary<JsonNode, (int Group, int Index)>(ReferenceEqualityComparer.Instance);
        foreach (var h in JsonPositions(hooks))
            if (h.Node != null && !IsOurs(CommandOf(h.Node))) was[h.Node] = (h.Group, h.Index);

        bool removed = false;
        foreach (var (ev, value) in hooks.ToList())
        {
            if (value is not JsonArray groups) continue;
            bool here = false;
            foreach (var g in groups.OfType<JsonObject>().ToList())
                if (g["hooks"] is JsonArray list && list.Where(x => IsOurs(CommandOf(x))).ToList() is { Count: > 0 } mine)
                {
                    foreach (var h in mine) list.Remove(h);
                    if (list.Count == 0) groups.Remove(g);
                    here = true;
                }
            if (here && groups.Count == 0) hooks.Remove(ev);  // only events AiPet emptied, not the user's own empty ones
            removed |= here;
        }
        if (!removed && command == null) return false;
        if (command != null)
            foreach (var e in Events)
            {
                if (hooks[e.Event] is not JsonArray groups) hooks[e.Event] = groups = new JsonArray();
                groups.Add((JsonNode)new JsonObject { ["hooks"] = new JsonArray(JsonHandler(e, command)) });
            }
        var warned = new HashSet<string>();
        foreach (var h in JsonPositions(hooks))
            if (h.Node != null && was.TryGetValue(h.Node, out var at) && at != (h.Group, h.Index) && warned.Add(h.Event))
                Console.WriteLine($"Note: another {h.Event} hook comes after AiPet's in hooks.json; Codex will ask you to trust it again.");

        var real = RealPath(path);  // a symlinked hooks.json (dotfiles): write the file it points at
        if (root.Count == 1 && hooks.Count == 0 && real == path)
        {
            if (!File.Exists(path)) return false;
            Backup(path);
            File.Delete(path);
            return true;
        }
        // other tools' commands keep their characters (& ' < > + and non-ASCII aren't escaped), and the file its line endings
        string nl = before.Contains(CR) ? new string(new[] { CR, LF }) : before.Length > 0 ? LF.ToString() : Environment.NewLine;
        var after = root.ToJsonString(new JsonSerializerOptions
        { WriteIndented = true, NewLine = nl, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping });
        if (before.EndsWith(LF)) after += nl;
        if (JsonNode.DeepEquals(JsonNode.Parse(string.IsNullOrWhiteSpace(before) ? "{}" : before), JsonNode.Parse(after))) return false;
        if (File.Exists(path)) Backup(path);
        AiPet.Install.Save(real, after);
        return true;
    }
}
