import { BaselineHero } from "./BaselineHero";
import { HeroSurfaceShowcase } from "./HeroSurfaceShowcase";
import { KineticCalculator } from "./KineticCalculator";
import { HomeShell, type HomePageProps } from "./HomeShell";

export function HomePage(props: HomePageProps) {
  return (
    <HomeShell
      {...props}
      hero={<BaselineHero theme={props.theme} />}
      product={<HeroSurfaceShowcase {...props} />}
      calculator={<KineticCalculator />}
    />
  );
}
