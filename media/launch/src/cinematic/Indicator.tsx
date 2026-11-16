import {
  CheckCircleIcon,
  CircleIcon,
  DiffModifiedIcon,
  DotFillIcon,
  PlugIcon,
} from "@primer/octicons-react";
import { palette as p } from "../design";

const states = {
  pending: { Icon: CircleIcon, color: p.muted },
  running: { Icon: DotFillIcon, color: p.accent },
  complete: { Icon: CheckCircleIcon, color: p.green },
  modified: { Icon: DiffModifiedIcon, color: "#d29922" },
  connected: { Icon: PlugIcon, color: p.green },
} as const;

export type IndicatorState = keyof typeof states;

export const Indicator = ({
  state,
  size = 18,
}: {
  readonly state: IndicatorState;
  readonly size?: number;
}) => {
  const { Icon, color } = states[state];
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: size,
        height: size,
        flexShrink: 0,
        color,
      }}
    >
      <Icon size={size} aria-hidden="true" />
    </span>
  );
};
