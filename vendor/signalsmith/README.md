# Signalsmith Stretch (vendored)

Time stretching for the sampler's Time and Beat modes (`src/stretch.rs`).

- `signalsmith-stretch.h`: github.com/Signalsmith-Audio/signalsmith-stretch
  at 88061fd4eda56a2353c5d65c74aa3f4ccf76f9da, MIT (`LICENSE-stretch.txt`).
- `signalsmith-linear/`: github.com/Signalsmith-Audio/linear at
  7bb1c8f36794d6980080ce817c91f8da8fe56a66, MIT (`LICENSE-linear.txt`):
  `fft.h`, `stft.h`, `linear.h`, `approx.h` only (no platform back ends).
- `wrapper.cpp`: ours. A C entry point with a fixed seed, so a render is
  the same bytes in every process (the library seeds from
  `std::random_device` otherwise).
