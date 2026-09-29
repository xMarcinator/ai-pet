using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace AiPet.Golden;

/// Mode data: the data files as the .NET app reads and writes them, both ways, for rust/crates/aipet-core
/// (tests/data.rs).
///
///   data     writes rust/crates/aipet-core/tests/golden/data/:
///              files.json          config.json, jira.json and github.json: each fixture file read by the app, with
///                                  its read status and the text the app writes back after reading it
///              secrets.json        the dictionary calls Linux's SecretTool makes on its fallback secrets.json
///              hook_cleanup.json   HookCleanup.Names and HookCleanup.Agents
///              update_status.json  UpdateStatus' texts
///   data read config|jira|github|secrets FILE...
///            the app reading files the Rust wrote: one JSON line per file, {"status", "written"}, where written is
///            what the app writes after reading it (so it holds every value the app read)
///   data secret write TARGET USER SECRET | read TARGET | delete TARGET
///            the app's Credential Manager code (Windows), on targets named AiPet:GoldenTest:… only
///
/// A read's status is "loaded", "missing" (no file) or "corrupt" (the app couldn't read it, and kept its defaults).
/// config.json's Config and the Credential Manager store are private to the app (src/AiPet.UI), which this project
/// doesn't build, so they are copied below; every run checks the copies, and the lines the app reads and writes
/// config.json with, against the app's source.
static class DataMode
{
    // ------------------------------------------------------------------ copied from src/AiPet.UI/MainWindow.axaml.cs
    sealed class Config
    {
        public int? Left { get; set; }
        public int? Top { get; set; }
        public bool Pills { get; set; } = true;
        /// Only in files from before the toolbar under the pet was removed: whether it showed when the position was
        /// saved (see PlaceWindow). Never written again.
        [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
        public bool? Toolbar { get; set; }
        public bool OnTop { get; set; } = true;
        public string Avatar { get; set; } = "Sprout";
        public bool Music { get; set; } = true;
        /// Window height (DIPs) the saved position was taken with, so a taller window keeps the pet in place.
        public double WindowHeight { get; set; } = 420;
    }

    /// MainWindow's constructor reads config.json with this line, and SaveConfig writes it with the second.
    const string ReadLine = "cfg = JsonSerializer.Deserialize<Config>(File.ReadAllText(Paths.Config)) ?? new Config();";
    const string WriteLine = "File.WriteAllText(Paths.Config, JsonSerializer.Serialize(cfg));";

    // ------------------------------------------------------------------ copied from src/AiPet.UI/Platform/WindowsPlatform.cs
    sealed class CredentialManager : ISecretStore
    {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        struct CREDENTIAL
        {
            public uint Flags, Type;
            public string TargetName, Comment;
            public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
            public uint CredentialBlobSize;
            public IntPtr CredentialBlob;
            public uint Persist, AttributeCount;
            public IntPtr Attributes;
            public string TargetAlias, UserName;
        }

        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredWrite(ref CREDENTIAL cred, uint flags);
        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredRead(string target, uint type, uint flags, out IntPtr cred);
        [DllImport("advapi32.dll")] static extern void CredFree(IntPtr cred);
        [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool CredDelete(string target, uint type, uint flags);

        public void Delete(string key) => CredDelete(key, 1, 0);

        public void Write(string key, string user, string secret)
        {
            var blob = Marshal.StringToCoTaskMemUni(secret);
            try
            {
                var c = new CREDENTIAL
                {
                    Type = 1, TargetName = key, UserName = user, Persist = 2,
                    CredentialBlob = blob, CredentialBlobSize = (uint)(secret.Length * 2),
                };
                CredWrite(ref c, 0);
            }
            finally { Marshal.ZeroFreeCoTaskMemUnicode(blob); }
        }

        public string Read(string key)
        {
            if (!CredRead(key, 1, 0, out var p)) return null;
            try
            {
                var c = Marshal.PtrToStructure<CREDENTIAL>(p);
                return c.CredentialBlob == IntPtr.Zero ? null : Marshal.PtrToStringUni(c.CredentialBlob, (int)c.CredentialBlobSize / 2);
            }
            finally { CredFree(p); }
        }
    }

    /// The only targets `data secret` touches.
    const string TestTargets = "AiPet:GoldenTest:";

    static readonly JsonSerializerOptions Pretty = new() { WriteIndented = true, Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static int Run(string repo, string data, string[] args)
    {
        if (Path.GetFullPath(Paths.DataDir) != Path.GetFullPath(data))
            throw new InvalidOperationException($"AiPet.Core reads {Paths.DataDir}, not the temp folder {data}");
        CheckCopies(repo);
        Directory.CreateDirectory(Paths.DataDir);
        return args switch
        {
            [] => Generate(repo, data),
            ["read", "config" or "jira" or "github" or "secrets", _, ..] => ReadFiles(args[1], args[2..]),
            ["secret", _, _, ..] => Secret(args[1..]),
            _ => Program.Usage(),
        };
    }

    // ------------------------------------------------------------------ the copies are the app's
    /// Throws unless the copies above are the app's classes, line for line (indentation aside), and the app still
    /// reads and writes config.json with ReadLine and WriteLine.
    static void CheckCopies(string repo, [CallerFilePath] string self = "")
    {
        var mainWindow = File.ReadAllText(Path.Combine(repo, "src", "AiPet.UI", "MainWindow.axaml.cs"));
        var windows = File.ReadAllText(Path.Combine(repo, "src", "AiPet.UI", "Platform", "WindowsPlatform.cs"));
        var copies = File.ReadAllText(self);
        foreach (var (source, name) in new[] { (mainWindow, "Config"), (windows, "CredentialManager") })
            if (ClassText(source, name) != ClassText(copies, name))
                throw new InvalidOperationException($"the copy of {name} in {self} isn't the app's any more: copy it again");
        foreach (var line in new[] { ReadLine, WriteLine })
            if (!mainWindow.Contains(line))
                throw new InvalidOperationException($"MainWindow.axaml.cs no longer has `{line}`: update Data.cs and the Rust port");
    }

    /// The text of `sealed class <name>` up to its closing brace, each line trimmed. The Rust test finds the same
    /// text in MainWindow.axaml.cs, to know the fixtures still hold.
    static string ClassText(string source, string name)
    {
        var lines = source.Split('\n').Select(l => l.Trim()).ToList();
        int start = lines.FindIndex(l => Regex.IsMatch(l, $@"\bsealed class {name}\b"));
        if (start < 0) throw new InvalidOperationException($"no class {name}");
        var text = new List<string>();
        int depth = 0;
        for (int i = start; i < lines.Count; i++)
        {
            text.Add(lines[i]);
            depth += lines[i].Count(c => c == '{') - lines[i].Count(c => c == '}');
            if (depth == 0 && lines[i].Contains('}')) return string.Join("\n", text);
        }
        throw new InvalidOperationException($"class {name} doesn't end");
    }

    // ------------------------------------------------------------------ the golden files
    [MethodImpl(MethodImplOptions.NoInlining)]
    static int Generate(string repo, string data)
    {
        var dir = Path.Combine(repo, "rust", "crates", "aipet-core", "tests", "golden", "data");
        Directory.CreateDirectory(dir);
        Save(dir, "files.json", new
        {
            config_class = ClassText(File.ReadAllText(Path.Combine(repo, "src", "AiPet.UI", "MainWindow.axaml.cs")), "Config"),
            cases = FileCases().Select(f =>
            {
                var (status, written) = ReadFile(f.Kind, f.Bytes);
                return new
                {
                    name = f.Name,
                    kind = f.Kind,
                    input = f.Text,
                    input_hex = f.Text == null && f.Bytes != null ? Convert.ToHexString(f.Bytes) : null,
                    status,
                    // indented files end their lines with Environment.NewLine; the Rust writes its platform's too
                    written = written?.Replace(Environment.NewLine, "\n"),
                };
            }).ToList(),
        });
        Save(dir, "secrets.json", SecretsCases());
        Save(dir, "hook_cleanup.json", new { names = NamesCases(), agents = AgentsCases(data) });
        Save(dir, "update_status.json", new
        {
            repo = UpdateStatus.Repo,
            channel = UpdateStatus.Channel,
            first_check_seconds = UpdateStatus.FirstCheck.TotalSeconds,
            every_seconds = UpdateStatus.Every.TotalSeconds,
            cases = (from kind in Enum.GetValues<UpdateStatus.Kinds>()
                     from version in new[] { null, "0.3.0" }
                     from error in new[] { null, "No such host is known." }
                     let s = new UpdateStatus(kind, version, 42, error)
                     select new { kind = kind.ToString(), version, percent = 42, error, text = s.Text, busy = s.Busy }).ToList(),
        });
        return 0;
    }

    static void Save(string dir, string file, object value) =>
        File.WriteAllText(Path.Combine(dir, file), JsonSerializer.Serialize(value, Pretty).Replace("\r\n", "\n") + "\n", new UTF8Encoding(false));

    /// A data file: its text (written as UTF-8) and bytes, or only bytes, or neither for a file that isn't there.
    sealed record Fixture(string Name, string Kind, string Text, byte[] Bytes);

    static Fixture Text(string name, string kind, string text) => new(name, kind, text, Encoding.UTF8.GetBytes(text));
    static Fixture Bytes(string name, string kind, byte[] bytes) => new(name, kind, null, bytes);

    static IEnumerable<Fixture> FileCases()
    {
        // an unknown value nested this deep, counting its own [ ]
        string Nested(int depth) => "{\"Left\":1,\"Deep\":" + new string('[', depth) + new string(']', depth) + "}";
        const string special = "\"Ünï <&> '+' \\\"q\\\" \\\\ / ` = ! \\n \\r \\t \\b \\f \\u0001 \\u001f \\u007f ~ é 😀 \\u2028\"";
        var ascii = "\"" + string.Concat(Enumerable.Range(0x20, 0x7F - 0x20).Select(c => c is '"' or '\\' ? "\\" + (char)c : ((char)c).ToString())) + "\"";
        return new[]
        {
            new Fixture("missing", "config", null, null),
            Text("empty object", "config", "{}"),
            Text("every field", "config", """{"Left":1200,"Top":640,"Pills":false,"OnTop":true,"Avatar":"Sprout","Music":false,"WindowHeight":420}"""),
            Text("legacy toolbar", "config", """{"Left":10,"Top":20,"Pills":true,"Toolbar":true,"OnTop":false,"Avatar":"Moss","Music":true,"WindowHeight":462}"""),
            Text("legacy toolbar off", "config", """{"Left":10,"Top":20,"Toolbar":false}"""),
            Text("toolbar null", "config", """{"Toolbar":null,"Left":3}"""),
            Text("position null", "config", """{"Left":null,"Top":null,"Pills":true}"""),
            Text("negative position", "config", """{"Left":-1920,"Top":-5}"""),
            Text("int extremes", "config", """{"Left":2147483647,"Top":-2147483648}"""),
            Text("negative zero int", "config", """{"Left":-0,"Top":0}"""),
            Text("fractional height", "config", """{"WindowHeight":419.5}"""),
            Text("whole double height", "config", """{"WindowHeight":420.0}"""),
            Text("tiny height", "config", """{"WindowHeight":1e-7}"""),
            Text("small height", "config", """{"WindowHeight":0.0001}"""),
            Text("huge height", "config", """{"WindowHeight":1e20}"""),
            Text("long height", "config", """{"WindowHeight":0.30000000000000004}"""),
            Text("big integer height", "config", """{"WindowHeight":12345678901234567890}"""),
            Text("negative zero height", "config", """{"WindowHeight":-0}"""),
            Text("exponent height", "config", """{"WindowHeight":4.2E+2}"""),
            Text("unknown fields", "config", """{"Left":1,"Extra":{"nested":[1,2,{"x":null}],"s":"\u00e9"},"Theme":"dark","Top":2,"Big":1e400}"""),
            Text("case differs", "config", """{"left":5,"TOP":6,"pills":false}"""),
            Text("duplicate", "config", """{"Left":1,"Avatar":"A","Left":2,"Avatar":"B"}"""),
            Text("escaped name", "config", """{"\u004Ceft":7,"Av\u0061tar":"x"}"""),
            Text("special avatar", "config", "{\"Avatar\":" + special + "}"),
            Text("printable ascii avatar", "config", "{\"Avatar\":" + ascii + "}"),
            Text("avatar null", "config", """{"Avatar":null}"""),
            Text("indented with CRLF", "config", "{\r\n  \"Left\": 5,\r\n  \"Top\": 6,\r\n  \"Music\": false\r\n}\r\n"),
            Text("whitespace around", "config", " \t\n{\"Left\":5}\n\n"),
            Text("top-level null", "config", "null"),
            Text("lone surrogate in an unknown field", "config", """{"Extra":"\ud800","Left":4}"""),
            Text("unknown nested 62", "config", Nested(62)),
            Text("unknown nested 63", "config", Nested(63)),
            Text("unknown nested 64", "config", Nested(64)),
            Bytes("utf-8 BOM", "config", [0xEF, 0xBB, 0xBF, .. Encoding.UTF8.GetBytes("""{"Left":8,"Avatar":"é"}""")]),
            Bytes("utf-16 LE BOM", "config", [0xFF, 0xFE, .. Encoding.Unicode.GetBytes("""{"Left":9,"Avatar":"é😀"}""")]),
            Bytes("utf-16 BE BOM", "config", [0xFE, 0xFF, .. Encoding.BigEndianUnicode.GetBytes("""{"Left":10}""")]),
            Bytes("utf-32 LE BOM", "config", [0xFF, 0xFE, 0x00, 0x00, .. Encoding.UTF32.GetBytes("""{"Left":11}""")]),
            Bytes("invalid utf-8", "config", [.. Encoding.UTF8.GetBytes("{\"Avatar\":\"a"), 0xFF, 0xC3, .. Encoding.UTF8.GetBytes("b\"}")]),
            Text("empty", "config", ""),
            Text("whitespace only", "config", "  \n"),
            Text("array", "config", "[]"),
            Text("number", "config", "5"),
            Text("string int", "config", """{"Left":"5"}"""),
            Text("fractional int", "config", """{"Left":1.5}"""),
            Text("whole fractional int", "config", """{"Left":1.0}"""),
            Text("exponent int", "config", """{"Left":1e2}"""),
            Text("int overflow", "config", """{"Left":2147483648}"""),
            Text("null bool", "config", """{"Pills":null}"""),
            Text("number bool", "config", """{"Toolbar":1}"""),
            Text("number string", "config", """{"Avatar":5}"""),
            Text("string height", "config", """{"WindowHeight":"420"}"""),
            Text("height overflow", "config", """{"WindowHeight":1e400}"""),
            Text("lone surrogate in the avatar", "config", """{"Avatar":"\ud800"}"""),
            Text("trailing comma", "config", """{"Left":1,}"""),
            Text("comment", "config", "{\"Left\":1} // saved"),
            Text("trailing text", "config", """{"Left":1} x"""),
            Text("two objects", "config", """{"Left":1}{"Left":2}"""),
            Text("truncated", "config", """{"Left":1"""),
            Text("unknown nested 65", "config", Nested(65)),

            new Fixture("missing", "jira", null, null),
            Text("every field", "jira", """{"Enabled":true,"Site":"acme.atlassian.net","Email":"ann@acme.example","Jql":"project = ABC AND statusCategory != Done ORDER BY updated DESC","PollSeconds":30}"""),
            Text("as the app writes it", "jira", "{\n  \"Enabled\": false,\n  \"Site\": \"\",\n  \"Email\": \"\",\n  \"Jql\": \"assignee = currentUser()\",\n  \"PollSeconds\": 120\n}"),
            Text("nulls", "jira", """{"Site":null,"Email":null,"Jql":null}"""),
            Text("special jql", "jira", """{"Jql":"text ~ \"a&b\" AND summary ~ '<x>' + é"}"""),
            Text("unknown fields", "jira", """{"Enabled":true,"Token":"secret","PollSeconds":60}"""),
            Text("string poll", "jira", """{"PollSeconds":"120"}"""),
            Text("empty", "jira", ""),

            new Fixture("missing", "github", null, null),
            Text("every field", "github", """{"Enabled":true,"Host":"github.example.com","Orgs":["acme","beta"],"JiraProjects":["ABC"],"PollSeconds":300}"""),
            Text("empty lists", "github", """{"Orgs":[],"JiraProjects":[]}"""),
            Text("nulls", "github", """{"Host":null,"Orgs":null,"JiraProjects":[null,"X"]}"""),
            Text("unknown fields", "github", """{"Enabled":true,"Repos":["a/b"]}"""),
            Text("string orgs", "github", """{"Orgs":"acme"}"""),
            Text("number in orgs", "github", """{"Orgs":[1]}"""),
        };
    }

    /// Reads a data file of this kind as the app does, from `bytes` (null: no file), and writes it back as the app
    /// does. Returns the read's status and the text written: null when the app can't write what it read (a height
    /// of 1e400 reads as infinity, which System.Text.Json refuses to write; SaveConfig swallows that and writes
    /// nothing).
    static (string Status, string Written) ReadFile(string kind, byte[] bytes)
    {
        string path = PathOf(kind);
        if (File.Exists(path)) File.Delete(path);
        if (bytes != null) File.WriteAllBytes(path, bytes);
        switch (kind)
        {
            case "config":
            {
                // MainWindow's constructor and SaveConfig (CheckCopies holds them to ReadLine and WriteLine)
                Config cfg = new();
                var status = Status(() => cfg = JsonSerializer.Deserialize<Config>(File.ReadAllText(Paths.Config)) ?? new Config());
                File.Delete(Paths.Config);
                try { File.WriteAllText(Paths.Config, JsonSerializer.Serialize(cfg)); } catch { }
                return (status, File.Exists(Paths.Config) ? File.ReadAllText(Paths.Config) : null);
            }
            case "jira":
            {
                // the line the watcher's constructor reads with (it swallows what that throws), and its Save
                var status = Status(() => _ = JsonSerializer.Deserialize<JiraWatcher.Settings>(File.ReadAllText(path)) ?? new JiraWatcher.Settings());
                var watcher = new JiraWatcher(new NoSecrets());
                watcher.Save(watcher.Config, null);
                return (status, File.ReadAllText(path));
            }
            case "github":
            {
                var status = Status(() => _ = JsonSerializer.Deserialize<GitHubWatcher.Settings>(File.ReadAllText(path)) ?? new GitHubWatcher.Settings());
                var watcher = new GitHubWatcher(new NoSecrets());
                watcher.Save(watcher.Config, null);
                return (status, File.ReadAllText(path));
            }
            default:
            {
                // SecretTool's fallback file (LinuxPlatform.cs) is read with this line
                Dictionary<string, string> all = null;
                var status = Status(() => all = JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(path)));
                return (status, JsonSerializer.Serialize(all));
            }
        }
    }

    static string PathOf(string kind) => kind == "config" ? Paths.Config : Path.Combine(Paths.DataDir, kind + ".json");

    static string Status(Action read)
    {
        try { read(); return "loaded"; }
        catch (FileNotFoundException) { return "missing"; }
        catch (DirectoryNotFoundException) { return "missing"; }
        catch { return "corrupt"; }
    }

    /// The app's watchers get no token from this, so they never go online.
    sealed class NoSecrets : ISecretStore
    {
        public string Read(string key) => null;
        public void Write(string key, string user, string secret) => throw new InvalidOperationException("the golden writes no token");
        public void Delete(string key) => throw new InvalidOperationException("the golden deletes no token");
    }

    static int ReadFiles(string kind, string[] files)
    {
        foreach (var file in files)
        {
            var (status, written) = ReadFile(kind, File.Exists(file) ? File.ReadAllBytes(file) : null);
            Console.WriteLine(JsonSerializer.Serialize(new { status, written }));
        }
        return 0;
    }

    // ------------------------------------------------------------------ secrets
    /// SecretTool's fallback (src/AiPet.UI/Platform/LinuxPlatform.cs:270-330) on a secrets.json, with its own lines:
    /// Read, Write's dictionary and Delete's. The file handling around them (0600, the rename) runs only on Linux,
    /// where the Rust's own tests hold it.
    static object SecretsCases()
    {
        const string key = "AiPet:Jira", other = "AiPet:GitHub", secret = "tok+en/é<&>\"'😀";
        var inputs = new (string Name, string Text)[]
        {
            ("missing", null),
            ("both", """{"AiPet:Jira":"jira-token","AiPet:GitHub":"ghp_x"}"""),
            ("github only", """{"AiPet:GitHub":"ghp_x"}"""),
            ("jira only", """{"AiPet:Jira":"jira-token"}"""),
            ("escaped", """{"AiPet:Jira":"a\u002Bb\/c\u00e9"}"""),
            ("duplicate", """{"AiPet:Jira":"first","AiPet:GitHub":"g","AiPet:Jira":"second"}"""),
            ("null value", """{"AiPet:Jira":null,"AiPet:GitHub":"g"}"""),
            ("null", "null"),
            ("empty object", "{}"),
            ("number value", """{"AiPet:Jira":5}"""),
            ("array", "[]"),
            ("empty", ""),
        };
        return inputs.Select(i =>
        {
            // a missing file throws in ReadAllText, as null does in Deserialize: the same catch takes both
            var text = i.Text;
            string Read(string k)
            {
                try { return JsonSerializer.Deserialize<Dictionary<string, string>>(text).GetValueOrDefault(k); }
                catch { return null; }
            }
            Dictionary<string, string> all;
            try { all = JsonSerializer.Deserialize<Dictionary<string, string>>(text) ?? new(); } catch { all = new(); }
            all[key] = secret;
            var written = JsonSerializer.Serialize(all);
            // Delete: the file goes when nothing is left in it, and stays as it is when the key isn't in it
            string deleted = "unchanged";
            try
            {
                var left = JsonSerializer.Deserialize<Dictionary<string, string>>(text);
                if (left != null && left.Remove(key)) deleted = left.Count == 0 ? "removed" : JsonSerializer.Serialize(left);
            }
            catch { }
            return new
            {
                name = i.Name,
                input = text,
                read = new Dictionary<string, string> { [key] = Read(key), [other] = Read(other) },
                write = new { key, secret, written },
                delete = new { key, result = deleted },
            };
        }).ToList();
    }

    static int Secret(string[] args)
    {
        if (!OperatingSystem.IsWindows()) { Console.Error.WriteLine("data secret runs on Windows only"); return 2; }
        if (!args[1].StartsWith(TestTargets, StringComparison.Ordinal))
        {
            Console.Error.WriteLine($"data secret touches targets named {TestTargets}… only");
            return 2;
        }
        var store = new CredentialManager();
        switch (args)
        {
            case ["write", var target, var user, var secret]:
                store.Write(target, user, secret);
                return 0;
            case ["read", var target]:
                Console.WriteLine(JsonSerializer.Serialize(new { secret = store.Read(target) }));
                return 0;
            case ["delete", var target]:
                store.Delete(target);
                return 0;
            default:
                return Program.Usage();
        }
    }

    // ------------------------------------------------------------------ HookCleanup
    const string Installed = @"C:\Users\Ann\AppData\Local\AiPetApp\current\aipet-hook.exe";

    /// tests/AiPet.Tests/VelopackTests.cs's cases, and a few more.
    static object NamesCases()
    {
        string[] hooks = [Installed];
        const string spaces = @"C:\Users\Ann Smith\AppData\Local\AiPetApp\current\aipet-hook.exe";
        const string shortForm = @"C:\Users\ANNSMI~1\AppData\Local\AiPetApp\current\aipet-hook.exe";
        const string quote = @"C:\Users\O'Brien\AppData\Local\AiPetApp\current\aipet-hook.exe";
        const string nordic = @"C:\Users\Åse\AppData\Local\AiPetApp\current\aipet-hook.exe";
        const string unix = "/home/ann/.local/share/AiPet/aipet-hook";
        var cases = new List<(string Text, string[] Hooks, bool IgnoreCase)>
        {
            ("""{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe","args":["--agent","claude"]}]}]}}""", hooks, true),
            ("""{"command":"C:\\Users\\Ann\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe"}""", hooks, true),
            ("""{"command":"c:/users/ann/appdata/local/aipetapp/current/AIPET-HOOK.EXE"}""", hooks, true),
            ("command = \"C:\\\\Users\\\\Ann\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe --agent codex\"\n", hooks, true),
            ("command = 'C:\\Users\\Ann\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe --agent codex'\n", hooks, true),
            ("command = \"& 'C:\\\\Users\\\\Ann\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe' --agent codex\"\n", hooks, true),
            (null, hooks, true),
            ("", hooks, true),
            ("""{"command":"C:/Tools/AiPet/aipet-hook.exe"}""", hooks, true),
            ("""{"command":"C:/Users/Ann/src/ai-pet/artifacts/win-x64/hook/aipet-hook.exe"}""", hooks, true),
            ("""{"command":"sh","args":["${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh","--agent","claude"]}""", hooks, true),
            ("""{"command":"C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe.old"}""", hooks, true),
            ("""{"command":"D:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe"}""", hooks, true),
            ("""{"command":"c:/users/ann/appdata/local/aipetapp/current/aipet-hook.exe"}""", hooks, false),
            ("command = \"C:\\\\Users\\\\ANNSMI~1\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe --agent codex\"\n", [spaces, shortForm], true),
            ("command = \"C:\\\\Users\\\\ANNSMI~1\\\\AppData\\\\Local\\\\AiPetApp\\\\current\\\\aipet-hook.exe --agent codex\"\n", [spaces], true),
            ("""{"command":"& 'C:\\Users\\O''Brien\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe' --agent codex"}""", [quote], true),
            // beyond VelopackTests: a later mention that is the whole name, and what continues a name or ends it
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe_x C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe", hooks, true),
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe-2", hooks, true),
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe9", hooks, true),
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exeé", hooks, true),
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe😀", hooks, true),
            ("C:/Users/Ann/AppData/Local/AiPetApp/current/aipet-hook.exe/", hooks, true),
            ("C:\\\\\\Users\\Ann\\AppData\\Local\\AiPetApp\\current\\aipet-hook.exe", hooks, true),
            ("C:/USERS/ÅSE/AppData/Local/AiPetApp/current/aipet-hook.exe", [nordic], true),
            ("C:/USERS/ÅSE/AppData/Local/AiPetApp/current/aipet-hook.exe", [nordic], false),
            (unix + " --agent codex", [unix], false),
            (unix, ["", null, unix], false),
        };
        return cases.Select(c => new { text = c.Text, hooks = c.Hooks, ignore_case = c.IgnoreCase, names = HookCleanup.Names(c.Text, c.Hooks, c.IgnoreCase) }).ToList();
    }

    /// VelopackTests' HookCleanupTests: the configs the hook's own registration code writes, and the agents whose
    /// configs name a hook.
    static object AgentsCases(string data)
    {
        string[][] hookLists = [[Installed], [@"C:\Tools\AiPet\aipet-hook.exe"]];
        string claude = Path.Combine(data, "claude"), codex = Path.Combine(data, "codex");
        string oldClaude = Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR"), oldCodex = Environment.GetEnvironmentVariable("CODEX_HOME");
        Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", claude);
        Environment.SetEnvironmentVariable("CODEX_HOME", codex);
        var cases = new List<object>();
        try
        {
            if (HookCleanup.ClaudeSettings != ClaudeConfig.Settings ||
                !HookCleanup.CodexFiles.SequenceEqual(new[] { CodexConfig.ConfigToml, CodexConfig.HooksJson }))
                throw new InvalidOperationException("HookCleanup doesn't read the files the hook registers in");
            void Fresh()
            {
                foreach (var d in new[] { claude, codex })
                {
                    if (Directory.Exists(d)) Directory.Delete(d, recursive: true);
                    Directory.CreateDirectory(d);
                }
            }
            string ReadOrNull(string f) => File.Exists(f) ? File.ReadAllText(f) : null;
            void Record(string step) => cases.Add(new
            {
                step,
                claude_settings = ReadOrNull(HookCleanup.ClaudeSettings),
                config_toml = ReadOrNull(CodexConfig.ConfigToml),
                hooks_json = ReadOrNull(CodexConfig.HooksJson),
                results = (from hooks in hookLists
                           from ignoreCase in new[] { true, false }
                           select new { hooks, ignore_case = ignoreCase, agents = HookCleanup.Agents(hooks, ignoreCase) }).ToList(),
            });
            Fresh();
            Record("nothing registered");
            Quiet(() => ClaudeConfig.Install(Installed));
            Record("claude");
            Quiet(() => CodexConfig.Install(Installed));
            Record("claude and codex");
            Quiet(() => ClaudeConfig.Uninstall());
            Record("codex");
            Fresh();
            File.WriteAllText(CodexConfig.HooksJson, """{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other-tool"}]}]}}""");
            Quiet(() => CodexConfig.Install(Installed));
            Record("codex in hooks.json");
            return cases;
        }
        finally
        {
            Environment.SetEnvironmentVariable("CLAUDE_CONFIG_DIR", oldClaude);
            Environment.SetEnvironmentVariable("CODEX_HOME", oldCodex);
        }
    }

    /// Runs a registration step without its messages, and throws when it fails.
    static void Quiet(Func<int> step)
    {
        TextWriter output = Console.Out, errors = Console.Error;
        var said = new StringWriter();
        Console.SetOut(said);
        Console.SetError(said);
        int exit;
        try { exit = step(); }
        finally
        {
            Console.SetOut(output);
            Console.SetError(errors);
        }
        if (exit != 0) throw new InvalidOperationException($"a registration step failed ({exit}): {said}");
    }
}
