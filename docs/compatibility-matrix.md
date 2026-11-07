# Compatibility matrix

Baseline: EasyEffects 8.3.2. `compat` is the level this implementation claims, not a measured acoustic result. `params` counts the effect-specific controls in the catalog (common gain/mix controls are separate).

| Effect | Group | Compat | Latency | Params | Bands | Notes |
|---|---|---|---|---:|---:|---|
| Equalizer | EQ & filters | `exact` | zero | 4 | 32 | RBJ biquads, 32 bands per channel, M/S variant shares the engine. |
| Mid-Side Equalizer | EQ & filters | `exact` | zero | 3 | 32 | Same biquad engine on mid/side; channel labels are mid/side. |
| Filter | EQ & filters | `exact` | zero | 4 | — | Butterworth cascade (12/24/48 dB/oct) plus shelves. |
| Bass Enhancer | EQ & filters | `approximate` | zero | 6 | — | Low-band harmonic generation; not the LSP exciter topology. |
| Bass Loudness | EQ & filters | `approximate` | zero | 2 | — | Level-dependent low shelf; equal-loudness inspired. |
| Loudness | EQ & filters | `approximate` | zero | 2 | — | Two shelves scaled by the reference level. |
| Compressor | Dynamics | `approximate` | zero | 21 | — | Feed-forward detector, soft knee, sidechain HPF/LPF, lookahead. |
| Limiter | Dynamics | `approximate` | adaptive | 8 | — | Lookahead brickwall with 4x interpolated peak detection. |
| Maximizer | Dynamics | `approximate` | adaptive | 4 | — | Upward gain + ceiling clamp. |
| Expander | Dynamics | `approximate` | zero | 8 | — | Downward expansion with knee. |
| Gate | Dynamics | `approximate` | zero | 8 | — | Hold, hysteresis and range. |
| Multiband Compressor | Dynamics | `approximate` | zero | 27 | — | LR4 crossovers, per-band compressor. |
| Multiband Gate | Dynamics | `approximate` | zero | 19 | — | LR4 crossovers, per-band gate. |
| De-esser | Dynamics | `approximate` | zero | 8 | — | Split-band sibilance compression. |
| Auto Gain | Dynamics | `approximate` | zero | 4 | — | Slow RMS matching with silence fallback. |
| Convolver | Space & time | `exact` | block | 4 | — | Partitioned FFT convolution; IR must be supplied. |
| Reverberation | Space & time | `approximate` | zero | 7 | — | 8-line FDN with damping and pre-delay. |
| Delay | Space & time | `exact` | zero | 4 | — | Stereo delay with feedback and ping-pong. |
| Crossfeed | Space & time | `exact` | zero | 3 | — | Bauer-style low-pass crossfeed. |
| Crosstalk Canceller | Space & time | `approximate` | zero | 3 | — | Delayed opposite-channel subtraction. |
| Stereo Tools | Space & time | `exact` | zero | 5 | — | M/S width, balance, levels, phase. |
| Exciter | Tone shaping | `approximate` | zero | 6 | — | Band-limited harmonic exciter. |
| Crystalizer | Tone shaping | `approximate` | zero | 2 | — | Transient-aware HF enhancement. |
| Crusher | Tone shaping | `exact` | zero | 3 | — | Bit-depth and sample-rate reduction. |
| Pitch Shift | Voice & pitch | `substitute` | zero | 5 | — | Two-tap granular shifter. |
| Autotune | Voice & pitch | `substitute` | zero | 6 | — | YIN detection + scale quantisation + shifter. |
| Harmonizer | Voice & pitch | `substitute` | zero | 4 | — | Two shifted voices. |
| Speech Processor | Voice & pitch | `substitute` | zero | 7 | — | HPF/LPF + compressor + de-esser. |
| Voice Suppressor | Voice & pitch | `substitute` | zero | 3 | — | Centre-channel attenuation. |
| Noise Reduction | Noise & echo | `substitute` | block | 4 | — | STFT spectral subtraction. |
| Deep Noise Remover | Noise & echo | `substitute` | block | 2 | — | Spectral gate standing in for the neural model. |
| Echo Canceller | Noise & echo | `substitute` | zero | 3 | — | NLMS adaptive filter. |
| Level Meter | Measurement | `meter` | zero | 2 | — | Pass-through measurement. |

## Compatibility levels

* `exact` — same algorithm class and parameter semantics; acoustic comparison expected to match closely.
* `approximate` — same controls and broadly the same behaviour, different detector/oversampling details.
* `substitute` — model-free algorithmic stand-in for a model-based upstream effect.
* `meter` — measurement only.

## Preset coverage

The preset layer maps every catalog key plus the common keys (`input-gain`, `output-gain`, `dry`, `wet`, `bypass`) and the band objects (`left.bandN`, `right.bandN`, `mid.bandN`, `side.bandN`). Keys that are not in the catalog are preserved verbatim and written back on export. Enum values are stored as EasyEffects labels and converted to indices at load time.

Known gaps: some upstream keys are preserved but not yet wired to DSP (for example `mode` on the equalizer, `sidechain-*` routing beyond the internal detector, and `dithering`/`alr` on the limiter). These are listed in `docs/roadmap.md`.
