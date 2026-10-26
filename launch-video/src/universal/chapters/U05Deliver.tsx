import { useCurrentFrame } from "remotion";
import { beatFrame } from "../timing";
import { Delivery } from "../parts/Delivery";

const B0 = beatFrame(32);
const LENGTH = beatFrame(44) - B0;

export const U05Deliver: React.FC = () => {
  const frame = useCurrentFrame();
  return <Delivery which="first" frame={frame} length={LENGTH} />;
};
