// One section loop: an illustrated scene (infra/site/scenes, rendered into public/gen/<bg>/,
// "<bg>_tall" for the 9:16 cut) behind one app component, a card the app renders itself.
// The scene's action is timed to the card, so both share one length. The card sits still
// (the scene does the moving) except where it is pressed. Wide: the card is centred at a
// fixed scale; tall: it is wider and sits higher, the scene's action happens below it.
import type React from "react";
import { AbsoluteFill, Img, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import { CLIPS, type Clip, lerp } from "./parts";

export type LoopProps = {
  bg: string;
  clip: Clip;
  dark?: boolean;
  /** Hold one frame of the card. */
  at?: number;
  /** The card is pressed over these frames (a small squeeze). */
  press?: [number, number];
  /** Over these frames the card crossfades back to its first frame, so the loop closes. */
  settle?: [number, number];
};

const INK = "#1E2B2A";

export const Loop: React.FC<LoopProps> = ({ bg, clip, dark, at, press, settle }) => {
  const f = useCurrentFrame();
  const { width: W, height: H } = useVideoConfig();
  const tall = H > W;
  const { frames, h } = CLIPS[clip];
  const w = tall ? W * 0.58 : (H * 0.78 * 1080) / 1602; // wide: the request card fills 78% of the height
  const ch = (h * w) / 1080;
  const top = tall ? H * 0.4 - ch / 2 : (H - ch) / 2;
  const i = Math.max(0, Math.min(frames - 1, Math.floor(at ?? f)));
  const squeeze = press ? lerp(f, [press[0], press[0] + 4], [0, 1]) * (1 - lerp(f, [press[0] + 6, press[1]], [0, 1])) : 0;
  const back = settle ? lerp(f, settle, [0, 1]) : 0;
  const card = (n: number) => staticFile(`frames/${clip}/f_${String(n + 1).padStart(4, "0")}.png`);
  return (
    <AbsoluteFill style={{ background: "#9DC5BD", overflow: "hidden" }}>
      <Img
        src={staticFile(`gen/${tall ? `${bg}_tall` : bg}/f_${String(f + 1).padStart(4, "0")}.png`)}
        style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover" }}
      />
      <div
        style={{
          position: "absolute",
          width: w,
          height: ch,
          left: (W - w) / 2,
          top,
          scale: String(1 - 0.015 * squeeze),
          borderRadius: w * 0.078,
          border: `1.6px solid ${dark ? "#C9D3CC" : INK}`,
          boxShadow: `${tall ? 12 : 16}px ${tall ? 14 : 18}px 0 -2px rgba(30,60,56,0.28)`,
          overflow: "hidden",
        }}
      >
        <Img src={card(i)} style={{ display: "block", width: "100%", height: "100%" }} />
        {back > 0 && (
          <Img src={card(0)} style={{ position: "absolute", inset: 0, width: "100%", height: "100%", opacity: back }} />
        )}
      </div>
    </AbsoluteFill>
  );
};
