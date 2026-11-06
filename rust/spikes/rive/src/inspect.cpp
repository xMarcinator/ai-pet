// What rive-rs's binding doesn't expose: the artboard and state machine names, the view models, and whether each
// asset is embedded or referenced (out-of-band). It imports the file a second time with rive-cpp's no-op factory
// and an asset loader that records what it is offered. Linked against the rive-cpp that rive-rs builds.
#include "rive/file.hpp"
#include "rive/artboard.hpp"
#include "rive/assets/file_asset.hpp"
#include "rive/file_asset_loader.hpp"
#include "utils/no_op_factory.hpp"

#include <cstring>
#include <string>

typedef void (*spike_cb)(void* ctx, int kind, const char* text, size_t len, size_t value);

enum Kind
{
    ARTBOARD = 0,      // text: name, value: index
    STATE_MACHINE = 1, // text: name, value: index within the last artboard
    ANIMATION = 2,     // text: name, value: index within the last artboard
    ASSET = 3,         // text: unique file name, value: in-band byte count (0: referenced, or a CDN asset)
    VIEW_MODELS = 4,   // value: count
    CDN_ASSET = 5,     // text: unique file name, value: CDN uuid length
};

namespace
{
class RecordingLoader : public rive::FileAssetLoader
{
public:
    RecordingLoader(spike_cb cb, void* ctx) : m_cb(cb), m_ctx(ctx) {}
    bool loadContents(rive::FileAsset& asset,
                      rive::Span<const uint8_t> inBandBytes,
                      rive::Factory*) override
    {
        std::string name = asset.uniqueFilename();
        m_cb(m_ctx, ASSET, name.data(), name.size(), inBandBytes.size());
        if (asset.cdnUuid().size() > 0)
        {
            m_cb(m_ctx, CDN_ASSET, name.data(), name.size(), asset.cdnUuid().size());
        }
        return false; // let rive-cpp decode in-band bytes as usual
    }

private:
    spike_cb m_cb;
    void* m_ctx;
};
} // namespace

extern "C" int spike_inspect(const uint8_t* data, size_t len, spike_cb cb, void* ctx)
{
    rive::NoOpFactory factory;
    auto loader = rive::make_rcp<RecordingLoader>(cb, ctx);
    rive::ImportResult result = rive::ImportResult::success;
    auto file = rive::File::import({data, len}, &factory, &result, loader);
    if (!file)
    {
        return result == rive::ImportResult::success ? -1 : (int)result;
    }
    for (size_t i = 0; i < file->artboardCount(); i++)
    {
        auto ab = file->artboard(i);
        std::string name = file->artboardNameAt(i);
        cb(ctx, ARTBOARD, name.data(), name.size(), i);
        for (size_t s = 0; s < ab->stateMachineCount(); s++)
        {
            std::string sm = ab->stateMachineNameAt(s);
            cb(ctx, STATE_MACHINE, sm.data(), sm.size(), s);
        }
        for (size_t a = 0; a < ab->animationCount(); a++)
        {
            std::string an = ab->animationNameAt(a);
            cb(ctx, ANIMATION, an.data(), an.size(), a);
        }
    }
    cb(ctx, VIEW_MODELS, "", 0, file->viewModelCount());
    return 0;
}
