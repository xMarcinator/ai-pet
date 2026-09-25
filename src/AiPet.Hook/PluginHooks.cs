using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AiPet;

/// `aipet-hook --print-plugin-hooks claude|codex`
/// Prints the aipet plugin's hook file for an agent (plugins/aipet/hooks/hooks.json or codex.json), made from the
/// tables the direct hooks come from, byte for byte as committed. CI compares the two (scripts/check-plugin.sh
/// --hook), so the plugin can't drift from the code. Codex's file is frozen (Codex trusts a hook by a hash of its
/// definition): this has to print it as it is, never the other way round. It isn't a hook run, so it may print.
static class PluginHooks
{
    public const string Usage = "usage: aipet-hook --print-plugin-hooks claude|codex";

    public static int Print(string agent)
    {
        if (agent is not ("claude" or "codex"))
        {
            Console.Error.WriteLine(Usage);
            return 2;
        }
        var text = (agent == "claude" ? Claude() : Codex()).ToJsonString(Format) + "\n";
        // the bytes themselves: Console.Out would encode the text in the console's code page
        using var stdout = Console.OpenStandardOutput();
        stdout.Write(Encoding.UTF8.GetBytes(text));
        return 0;
    }

    /// As the files are written: two spaces, LF on every OS, and quotes and non-ASCII text as they are.
    static readonly JsonSerializerOptions Format = new()
    { WriteIndented = true, NewLine = "\n", Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    /// Shell form through bash ("shell": "bash", so Claude never falls back to PowerShell), with the launcher that
    /// picks the binary for the OS. Claude expands ${CLAUDE_PLUGIN_ROOT}.
    const string ClaudeCommand = "sh \"${CLAUDE_PLUGIN_ROOT}/native/aipet-hook.sh\" --agent claude";

    /// ClaudeConfig.Events, each with the group ClaudeConfig registers, but with the plugin's command.
    static JsonObject Claude()
    {
        var hooks = new JsonObject();
        foreach (var e in ClaudeConfig.Events)
            hooks[e.Event] = new JsonArray(ClaudeConfig.Group(e, new JsonObject { ["type"] = "command", ["command"] = ClaudeCommand, ["shell"] = "bash" }));
        return new JsonObject
        {
            ["description"] = "Report each Claude Code chat's status to the AiPet desktop pet (observe only: the hook prints nothing and never answers Claude).",
            ["hooks"] = hooks,
        };
    }

    /// The shell Codex starts expands $PLUGIN_ROOT. On Windows Codex runs commandWindows with PowerShell instead: the
    /// exe itself, no Git Bash.
    const string CodexCommand = "sh \"$PLUGIN_ROOT/native/aipet-hook.sh\" --agent codex";
    const string CodexCommandWindows = @"& (Join-Path $env:PLUGIN_ROOT 'native\win-x64\aipet-hook.exe') --agent codex";

    /// CodexConfig.PluginEvents, each with the handler CodexConfig writes in a hooks.json, plus commandWindows. All
    /// run in the background with 30 s, on every OS: frozen with the plugin's definitions, so not taken from
    /// CodexConfig.Events, which the direct hooks may change per OS.
    static JsonObject Codex()
    {
        var hooks = new JsonObject();
        foreach (var ev in CodexConfig.PluginEvents)
        {
            var handler = CodexConfig.JsonHandler((ev, true, 30), CodexCommand, CodexCommandWindows);
            hooks[ev] = new JsonArray(new JsonObject { ["hooks"] = new JsonArray(handler) });
        }
        return new JsonObject
        {
            ["description"] = "Report each Codex chat's status to the AiPet desktop pet (observe only: the hook prints nothing).",
            ["hooks"] = hooks,
        };
    }
}
