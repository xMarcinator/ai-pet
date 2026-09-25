using System.Text.Json.Nodes;
using static AiPet.Program;

namespace AiPet;

/// `aipet-hook --agent claude`: run by Claude Code (the desktop app, the CLI, IDE extensions) on each hook event.
///
/// Claude starts the exe directly with its arguments and the event JSON on stdin. The hook only observes: it prints
/// nothing (so Claude acts on nothing from it), and always exits 0. Most events run in the background and can finish
/// out of order, so each is sent with when the hook started, which with no shell in between is when Claude started
/// it, and the pet places it by that (AgentSessions).
static class ClaudeHook
{
    public static JsonObject Envelope(JsonObject payload)
    {
        lastEvent = Str(payload, "hook_event_name") ?? "";
        var envelope = Program.Envelope("claude");
        // where the chat runs, and the desktop app's own id for it, are only in the hook's environment
        var env = new JsonObject();
        foreach (var name in Ipc.ClaudeEnv)
            if (Environment.GetEnvironmentVariable(name) is { } value) env[name] = value;
        envelope[Ipc.Env] = env;
        envelope[Ipc.Payload] = Trimmed(payload);
        return envelope;
    }
}
