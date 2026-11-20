// C entry point for Signalsmith Stretch: one whole buffer in, an exact
// length out, deterministic (fixed seed).
#include "signalsmith-stretch.h"
#include <vector>

extern "C" int kfx_stretch(
    unsigned long long seed,
    int channels,
    float sample_rate,
    const float *input, // planar: channels × in_len
    int in_len,
    float *output, // planar: channels × out_len
    int out_len,
    int cheaper
) {
    if (channels <= 0 || in_len <= 0 || out_len <= 0) return 0;
    signalsmith::stretch::SignalsmithStretch<float> s((long)seed);
    if (cheaper) {
        s.presetCheaper(channels, sample_rate);
    } else {
        s.presetDefault(channels, sample_rate);
    }
    std::vector<const float *> in(channels);
    std::vector<float *> out(channels);
    for (int c = 0; c < channels; ++c) {
        in[c] = input + (size_t)c * in_len;
        out[c] = output + (size_t)c * out_len;
    }
    return s.exact(in.data(), in_len, out.data(), out_len) ? 1 : 0;
}
