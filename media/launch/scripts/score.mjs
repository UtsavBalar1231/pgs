import { mkdirSync, writeFileSync } from "node:fs";

const rate = 48000;
const duration = 35;
const samples = new Float32Array(rate * duration);
const beat = 0.5;

function tone(start, length, voice) {
  const first = Math.round(start * rate);
  const count = Math.min(Math.round(length * rate), samples.length - first);
  for (let i = 0; i < count; i++) samples[first + i] += voice(i / rate);
}

for (let step = 0; step < duration / beat; step++) {
  const start = step * beat;
  const full = start >= 4.5 && start < 28;
  const gain = start >= 28 ? 0.6 : full ? 1 : 0.38;
  tone(start, 0.32, (t) => gain * 0.43 * Math.sin(2 * Math.PI * (48 * t + 8 * (1 - Math.exp(-t * 35)))) * Math.exp(-t * 18));
  if (step % 2 === 1) {
    tone(start, 0.13, (t) => gain * 0.11 * Math.sin(t * 117391) * Math.sin(t * 67329) * Math.exp(-t * 38));
  }
  for (let off = 0; off < 2; off++) {
    tone(start + off * 0.25, 0.06, (t) => gain * 0.055 * Math.sin(t * 987239 + step) * Math.sin(t * 583711) * Math.exp(-t * 75));
  }
  const bass = [73.4162, 73.4162, 65.4064, 87.3071][Math.floor(step / 8) % 4];
  tone(start + 0.125, 0.32, (t) => gain * 0.13 * (Math.sin(2 * Math.PI * bass * t) + 0.22 * Math.sin(4 * Math.PI * bass * t)) * Math.min(t * 120, 1) * Math.exp(-t * 11));
  if (start >= 8 && start < 28) {
    const note = [293.6648, 440, 587.3295, 523.2511, 349.2282, 440, 261.6256, 440][step % 8];
    tone(start + 0.25, 0.65, (t) => 0.055 * (Math.sin(2 * Math.PI * note * t) + 0.3 * Math.sin(2 * Math.PI * note * 2 * t)) * Math.min(1, t * 160) * Math.exp(-t * 7));
  }
}
for (const start of [4.5, 8, 15.5, 22.5, 28]) {
  tone(start, 0.65, (t) => 0.1 * Math.sin(2 * Math.PI * 146.8324 * t) * Math.exp(-t * 5));
}
for (const frequency of [146.8324, 220, 293.6648]) {
  tone(28, 6.8, (t) => 0.035 * Math.sin(2 * Math.PI * frequency * t) * Math.min(t * 4, 1) * Math.exp(-t * 0.65));
}

const bytes = Buffer.alloc(44 + samples.length * 4);
bytes.write("RIFF", 0);
bytes.writeUInt32LE(bytes.length - 8, 4);
bytes.write("WAVEfmt ", 8);
bytes.writeUInt32LE(16, 16);
bytes.writeUInt16LE(1, 20);
bytes.writeUInt16LE(2, 22);
bytes.writeUInt32LE(rate, 24);
bytes.writeUInt32LE(rate * 4, 28);
bytes.writeUInt16LE(4, 32);
bytes.writeUInt16LE(16, 34);
bytes.write("data", 36);
bytes.writeUInt32LE(samples.length * 4, 40);
for (let i = 0; i < samples.length; i++) {
  const t = i / rate;
  const fade = Math.min(1, t * 12, (duration - t) * 1.5);
  const value = Math.round(Math.tanh(samples[i] * 1.8) * 27000 * fade);
  bytes.writeInt16LE(value, 44 + i * 4);
  bytes.writeInt16LE(value, 46 + i * 4);
}
mkdirSync("out", { recursive: true });
writeFileSync("out/score.wav", bytes);
