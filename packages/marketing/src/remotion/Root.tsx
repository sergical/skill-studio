import { Composition } from "remotion";

import { OG_CARD_SIZE, OgCard } from "./OgCard";
import { ProductWalkthrough } from "./ProductWalkthrough";

const walkthroughs = [
  ["Sessions", "sessions", 270],
  ["Listing", "listing", 300],
  ["Map", "map", 450],
  ["Repair", "repair", 270],
  ["Install", "install", 300],
  ["Activity", "activity", 270],
] as const;

const themes = [
  ["Dark", "dark"],
  ["Light", "light"],
] as const;
const formats = [
  ["Desktop", "desktop", 1280, 720],
  ["Mobile", "mobile", 768, 768],
] as const;

export function RemotionRoot() {
  return (
    <>
      <Composition id="OgCard" component={OgCard} durationInFrames={1} fps={30} {...OG_CARD_SIZE} />
      {walkthroughs.flatMap(([name, feature, durationInFrames]) =>
        themes.flatMap(([themeName, theme]) =>
          formats.map(([formatName, format, width, height]) => (
            <Composition
              key={`${name}${themeName}${formatName}`}
              id={`${name}${themeName}${formatName}`}
              component={ProductWalkthrough}
              durationInFrames={durationInFrames}
              fps={30}
              width={width}
              height={height}
              defaultProps={{ feature, format, theme }}
            />
          )),
        ),
      )}
    </>
  );
}
