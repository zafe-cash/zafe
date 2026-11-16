import type React from "react";
import { Composition } from "remotion";
import { Loop, type LoopProps } from "./Loop";

const LOOPS: { id: string; frames: number; props: LoopProps }[] = [
  { id: "sign", frames: 126, props: { bg: "sign", clip: "request_card", press: [30, 44], settle: [104, 126] } },
  { id: "sent", frames: 132, props: { bg: "sent", clip: "send_card" } },
  { id: "setup", frames: 150, props: { bg: "setup", clip: "keys_card" } },
  { id: "live", frames: 126, props: { bg: "live", clip: "vault_card_dark", dark: true, at: 0 } },
];

export const RemotionRoot: React.FC = () => (
  <>
    {LOOPS.flatMap(({ id, frames, props }) => [
      <Composition key={id} id={id} component={Loop} durationInFrames={frames} fps={30} width={1600} height={900} defaultProps={props} />,
      <Composition key={`${id}-tall`} id={`${id}-tall`} component={Loop} durationInFrames={frames} fps={30} width={720} height={1280} defaultProps={props} />,
    ])}
  </>
);
