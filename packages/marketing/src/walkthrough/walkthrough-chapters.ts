export const chapters = [
  {
    id: "sessions",
    title: "See what each session listed.",
    copy: "For each session, see which skills had a description, which had only a name, and which the session used. A possible miss is marked as a guess.",
  },
  {
    id: "listing",
    title: "Try another listing budget.",
    copy: "Model listing shows each skill as described, name only or hidden. Type a budget, and Compare marks each skill that gains or loses a description.",
  },
  {
    id: "map",
    title: "One skill, two copies.",
    copy: "Your project has its own copy of a skill. See it next to the global copy, see which agents read each one, and compare what changed.",
  },
  {
    id: "repair",
    title: "Fix a broken skill.",
    copy: "A project skill points to a folder that no longer exists. Link it to a healthy copy in one click.",
  },
  {
    id: "install",
    title: "Park a skill you don't need yet.",
    copy: "Hide it from all your agents without deleting it. Unpark it to bring it back.",
  },
  {
    id: "activity",
    title: "See which skills run.",
    copy: "See which skills your agents used in the last 30 days, in which projects, and how recently.",
  },
] as const;

export interface WalkthroughProps {
  theme: "dark" | "light";
  title?: string;
}
