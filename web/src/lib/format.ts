// Formatting and small DSP helpers used by the UI.

export function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

export function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

export function dbToLin(db: number): number {
  return Math.pow(10, db / 20);
}

export function linToDb(lin: number): number {
  return lin <= 1e-6 ? -120 : 20 * Math.log10(lin);
}

export function formatDb(v: number, digits = 1): string {
  if (!Number.isFinite(v)) return '—';
  if (v <= -119.5) return '-∞';
  return `${v > 0 ? '+' : ''}${v.toFixed(digits)}`;
}

export function formatHz(v: number): string {
  if (v >= 1000) {
    const k = v / 1000;
    return `${k >= 10 ? k.toFixed(1) : k.toFixed(2)}k`;
  }
  return `${Math.round(v)}`;
}

export function formatMs(v: number): string {
  if (v >= 1000) return `${(v / 1000).toFixed(2)} s`;
  if (v >= 100) return `${Math.round(v)} ms`;
  return `${v.toFixed(1)} ms`;
}

export function formatPercent(v: number): string {
  return `${Math.round(v)}%`;
}

export function formatUnit(v: number, unit: string): string {
  switch (unit) {
    case 'db':
      return `${formatDb(v)} dB`;
    case 'hz':
      return `${formatHz(v)} Hz`;
    case 'ms':
      return formatMs(v);
    case 'percent':
      return formatPercent(v);
    case 'ratio':
      return `${v.toFixed(1)}:1`;
    case 'seconds':
      return `${v.toFixed(2)} s`;
    case 'octaves':
      return `${v.toFixed(1)} oct`;
    case 'degrees':
      return `${v.toFixed(0)}°`;
    case 'semitones':
      return `${v > 0 ? '+' : ''}${v.toFixed(2)} st`;
    case 'cents':
      return `${v > 0 ? '+' : ''}${v.toFixed(0)} ct`;
    case 'bpm':
      return `${v.toFixed(0)} BPM`;
    case 'db_per_oct':
      return `${v.toFixed(1)} dB/oct`;
    default:
      return v.toFixed(2);
  }
}

export function formatTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });
}

export function formatUptime(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s % 60}s`;
  return `${s}s`;
}

export function noteName(midi: number): string {
  const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
  const n = Math.round(midi);
  return `${names[((n % 12) + 12) % 12]}${Math.floor(n / 12) - 1}`;
}

export function midiToHz(note: number): number {
  return 440 * Math.pow(2, (note - 69) / 12);
}

export function hzToMidi(hz: number): number {
  return 69 + 12 * Math.log2(Math.max(hz, 1e-6) / 440);
}

// ---------------------------------------------------------------------------
// Biquad magnitude response (RBJ cookbook), mirroring dsp-core/src/biquad.rs.
// ---------------------------------------------------------------------------

interface Coeffs {
  b0: number;
  b1: number;
  b2: number;
  a1: number;
  a2: number;
}

const BAND_TYPES = ['bell', 'lowshelf', 'highshelf', 'lowpass', 'highpass', 'notch', 'bandpass', 'allpass'] as const;

export function biquadCoeffs(
  type: number,
  sr: number,
  freq: number,
  q: number,
  gainDb: number,
): Coeffs {
  const f = clamp(freq, 5, sr * 0.49);
  const w0 = (2 * Math.PI * f) / sr;
  const cos = Math.cos(w0);
  const sin = Math.sin(w0);
  const qq = clamp(q, 0.05, 40);
  const alpha = sin / (2 * qq);
  const a = Math.pow(10, gainDb / 40);
  let b0 = 1;
  let b1 = 0;
  let b2 = 0;
  let a0 = 1;
  let a1 = 0;
  let a2 = 0;

  switch (BAND_TYPES[type] ?? 'bell') {
    case 'lowpass':
      b0 = (1 - cos) / 2;
      b1 = 1 - cos;
      b2 = (1 - cos) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    case 'highpass':
      b0 = (1 + cos) / 2;
      b1 = -(1 + cos);
      b2 = (1 + cos) / 2;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    case 'bandpass':
      b0 = alpha;
      b1 = 0;
      b2 = -alpha;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    case 'notch':
      b0 = 1;
      b1 = -2 * cos;
      b2 = 1;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    case 'allpass':
      b0 = 1 - alpha;
      b1 = -2 * cos;
      b2 = 1 + alpha;
      a0 = 1 + alpha;
      a1 = -2 * cos;
      a2 = 1 - alpha;
      break;
    case 'lowshelf': {
      const s = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 - (a - 1) * cos + s);
      b1 = 2 * a * (a - 1 - (a + 1) * cos);
      b2 = a * (a + 1 - (a - 1) * cos - s);
      a0 = a + 1 + (a - 1) * cos + s;
      a1 = -2 * (a - 1 + (a + 1) * cos);
      a2 = a + 1 + (a - 1) * cos - s;
      break;
    }
    case 'highshelf': {
      const s = 2 * Math.sqrt(a) * alpha;
      b0 = a * (a + 1 + (a - 1) * cos + s);
      b1 = -2 * a * (a - 1 + (a + 1) * cos);
      b2 = a * (a + 1 + (a - 1) * cos - s);
      a0 = a + 1 - (a - 1) * cos + s;
      a1 = 2 * (a - 1 - (a + 1) * cos);
      a2 = a + 1 - (a - 1) * cos - s;
      break;
    }
    default: {
      b0 = 1 + alpha * a;
      b1 = -2 * cos;
      b2 = 1 - alpha * a;
      a0 = 1 + alpha / a;
      a1 = -2 * cos;
      a2 = 1 - alpha / a;
      break;
    }
  }

  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

export function biquadMagnitudeDb(c: Coeffs, sr: number, freq: number): number {
  const w = (2 * Math.PI * freq) / sr;
  const c1 = Math.cos(w);
  const s1 = Math.sin(w);
  const c2 = Math.cos(2 * w);
  const s2 = Math.sin(2 * w);
  const numRe = c.b0 + c.b1 * c1 + c.b2 * c2;
  const numIm = -(c.b1 * s1 + c.b2 * s2);
  const denRe = 1 + c.a1 * c1 + c.a2 * c2;
  const denIm = -(c.a1 * s1 + c.a2 * s2);
  const num = Math.hypot(numRe, numIm);
  const den = Math.hypot(denRe, denIm);
  return 20 * Math.log10(Math.max(num / Math.max(den, 1e-9), 1e-9));
}

export interface EqBandLike {
  type: number;
  frequency: number;
  gain: number;
  q: number;
  mute?: boolean;
}

/** Combined magnitude response of a band list, in dB. */
export function eqResponse(bands: EqBandLike[], sr: number, freqs: number[]): number[] {
  const active = bands.filter((b) => !b.mute && Math.abs(b.gain) > 1e-4);
  const coeffs = active.map((b) => biquadCoeffs(b.type, sr, b.frequency, b.q, b.gain));
  return freqs.map((f) => {
    let db = 0;
    for (const c of coeffs) db += biquadMagnitudeDb(c, sr, f);
    return db;
  });
}

/** Log-spaced frequency points for plots. */
export function logFreqs(count: number, min = 20, max = 20000): number[] {
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    out.push(min * Math.pow(max / min, i / (count - 1)));
  }
  return out;
}

export function xForFreq(f: number, width: number, min = 20, max = 20000): number {
  return (Math.log(f / min) / Math.log(max / min)) * width;
}

export function freqForX(x: number, width: number, min = 20, max = 20000): number {
  return min * Math.pow(max / min, clamp(x / width, 0, 1));
}

export function yForDb(db: number, height: number, min = -24, max = 24): number {
  return height - ((db - min) / (max - min)) * height;
}

export function dbForY(y: number, height: number, min = -24, max = 24): number {
  return min + (1 - clamp(y / height, 0, 1)) * (max - min);
}
