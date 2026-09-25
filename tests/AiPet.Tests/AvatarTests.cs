using System.IO;
using Xunit;

namespace AiPet.Tests;

/// Custom avatar files are hand-edited: one the pet can't draw is skipped, never handed to the pet (Settings draws
/// every avatar, and the pet wears the saved one at start, so a crash there would repeat on every start).
public class AvatarTests
{
    /// Writes an avatar file to the test data folder's avatars folder, runs the check, then removes the file.
    static void WithAvatar(string json, Action check)
    {
        Directory.CreateDirectory(Avatar.CustomDir);
        var file = Path.Combine(Avatar.CustomDir, "test-" + Guid.NewGuid().ToString("N")[..8] + ".json");
        File.WriteAllText(file, json);
        try { check(); }
        finally { File.Delete(file); }
    }

    [Theory]
    [InlineData("""{"name":"Short","shapes":[[13,14,7]]}""")]
    [InlineData("""{"name":"NullShape","shapes":[[13,14,7,7],null]}""")]
    [InlineData("""{"name":"Empty","shapes":[]}""")]
    [InlineData("""{"name":"NoShapes"}""")]
    [InlineData("""{"name":"Flat","shapes":[[13,14,0,7]]}""")]
    [InlineData("""{"name":"Negative","shapes":[[13,14,7,-2]]}""")]
    [InlineData("""{"name":"Huge","shapes":[[13,14,1e400,7]]}""")]
    [InlineData("""{"name":"FaceOff","shapes":[[13,14,7,7]],"faceBottom":2147483647}""")]
    [InlineData("""{"name":"FaceUp","shapes":[[13,14,7,7]],"faceTop":-5}""")]
    [InlineData("""{"name":"FaceFlip","shapes":[[13,14,7,7]],"faceTop":14,"faceBottom":9}""")]
    public void AnAvatarThePetCantDraw_IsSkipped(string json)
    {
        var name = System.Text.Json.JsonDocument.Parse(json).RootElement.GetProperty("name").GetString();
        WithAvatar(json, () =>
        {
            var all = Avatar.All();
            Assert.DoesNotContain(all, a => a.Name == name);
            Assert.All(Avatar.BuiltIn, b => Assert.Contains(all, a => a.Name == b.Name));
        });
    }

    [Fact]
    public void AGoodAvatar_IsListed_AndEveryListedOneDraws()
    {
        WithAvatar("""{"name":"Mine","shapes":[[13,14,7.3,7.3,1],[13,7,4,4]],"faceTop":9,"faceBottom":14,"palette":{"body":"#3366CC"}}""", () =>
        {
            var all = Avatar.All();
            var mine = Assert.Single(all, a => a.Name == "Mine");
            Assert.Equal(0xFF3366CCu, mine.Body);
            foreach (var a in all)
            {
                var pet = new Pet();
                pet.SetAvatar(a);
                var f = pet.Update(new PetInput { State = "idle" }, 0.05);
                Assert.Contains(f.Body, px => px != 0);
            }
        });
    }
}
