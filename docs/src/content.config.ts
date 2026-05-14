// Starlight content collection bindings. Required by Astro 5's typed
// content collections so the build picks up every Markdown / MDX file
// under `src/content/docs/` and validates its frontmatter against the
// Starlight schema (title, description, sidebar, etc.).
import { defineCollection } from "astro:content";
import { docsLoader } from "@astrojs/starlight/loaders";
import { docsSchema } from "@astrojs/starlight/schema";

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
