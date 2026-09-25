using System.Text.Json.Nodes;
using static AiPet.Program;

namespace AiPet;

/// `aipet-hook --agent codex`: run by Codex (CLI/TUI, exec, and the ChatGPT desktop app) on each hook event.
///
/// How Codex runs it (0.156): through a shell (PowerShell `-NoProfile -Command` on Windows, `sh -c` style elsewhere),
/// with the event JSON on stdin. Stdout is parsed: plain text on SessionStart/UserPromptSubmit goes to the model as
/// context, and on Stop it makes the hook fail. So this only observes: it prints nothing, writes nothing to stderr
/// and always exits 0. The hooks are registered to run in the background, and PowerShell starts each one 1-4 s late
/// at random, so events arrive out of order and the hook's own start time orders them poorly. The pet orders them by
/// Codex's own ids in the payload (turn_id, tool_use_id) first, and by that time only after (AgentSessions).
static class CodexHook
{
    public static JsonObject Envelope(JsonObject payload)
    {
        lastEvent = Str(payload, "hook_event_name") ?? "";
        var envelope = Program.Envelope("codex");
        envelope[Ipc.Payload] = Trimmed(payload);
        return envelope;
    }
}
