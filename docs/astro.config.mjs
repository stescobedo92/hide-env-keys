// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

// GitHub Pages serves project repos at https://<user>.github.io/<repo>/.
// Astro needs both `site` (absolute URL) and `base` (path prefix) set or
// every asset and link gets a 404. The repo name is "hide-env-keys", so
// the base is "/hide-env-keys/". If you fork this under a different
// repo name, change both values together.
const SITE = "https://stescobedo92.github.io";
const BASE = "/hide-env-keys";

export default defineConfig({
  site: SITE,
  base: BASE,
  // Build to ./dist; the workflow uploads it as a Pages artifact.
  outDir: "./dist",
  integrations: [
    starlight({
      title: "evault",
      description:
        "Secure cross-platform TUI + CLI for managing environment variables.",
      logo: {
        // Inline logo; the file is in docs/public/.
        src: "./public/logo.svg",
        replacesTitle: false,
      },
      favicon: "/favicon.svg",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/stescobedo92/hide-env-keys",
        },
      ],
      // Edit-this-page link in every doc.
      editLink: {
        baseUrl:
          "https://github.com/stescobedo92/hide-env-keys/edit/master/docs/",
      },
      // Add a "Last updated" footer per page using the file's git mtime.
      lastUpdated: true,
      // Pagefind-powered search shipped with Starlight; no config needed.
      sidebar: [
        {
          label: "Start here",
          items: [
            { label: "Introduction", slug: "index" },
            { label: "Install & first run", slug: "getting-started" },
          ],
        },
        {
          label: "Interactive TUI",
          items: [
            { label: "Keybindings", slug: "tui/keybindings" },
            { label: "Modals & error UX", slug: "tui/modals" },
            { label: "Run a command in a project", slug: "tui/run-in-project" },
          ],
        },
        {
          label: "Command line",
          items: [
            { label: "CLI reference", slug: "cli/reference" },
            { label: "Profiles", slug: "cli/profiles" },
            { label: "Recovery (evault reset)", slug: "cli/recovery" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Project manifest format", slug: "reference/manifest" },
            { label: "Architecture", slug: "reference/architecture" },
            { label: "Security model", slug: "reference/security" },
          ],
        },
      ],
      customCss: ["./src/styles/custom.css"],
    }),
  ],
});
