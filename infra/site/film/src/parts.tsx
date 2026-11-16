import { Easing, interpolate } from "remotion";

export const CLIPS = {
  request_card: { frames: 96, h: 1602 },
  send_card: { frames: 132, h: 1008 },
  keys_card: { frames: 150, h: 1188 },
  vault_card_dark: { frames: 1, h: 1293 },
} as const;
export type Clip = keyof typeof CLIPS;

export const MOVE = Easing.bezier(0.77, 0, 0.175, 1);
export const lerp = (f: number, input: number[], output: number[], easing = MOVE) =>
  interpolate(f, input, output, { extrapolateLeft: "clamp", extrapolateRight: "clamp", easing });
