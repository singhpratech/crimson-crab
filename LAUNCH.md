# crimson-crab — Launch & Growth Plan

Goal: become the de-facto Claude SDK for Rust (benchmark: async-openai's 2M+ downloads/90d in the OpenAI slot). Core lesson from market research: **cadence is the moat** — every dead incumbent lost by going quiet, not by being out-engineered.

## Phase 0 — Pre-launch checklist (before `cargo publish`)

- [ ] Cargo.toml metadata is the storefront: `description` = "The production-grade Rust SDK for Anthropic's Claude API — streaming, tools, thinking, prompt caching, batches"; `keywords = ["claude", "anthropic", "ai", "llm", "sdk"]` (crates.io search weighs name/keywords/description heavily); `categories = ["api-bindings", "asynchronous", "web-programming"]`; `repository`, `homepage` (GitHub Pages URL), `documentation` (docs.rs) all set.
- [ ] README = launch page (crates.io renders it — it IS the ad).
- [ ] docs.rs excellence: crate-level docs with runnable-looking quickstart, every module documented, `#[doc = include_str!("../README.md")]` trick considered so README doubles as crate docs.
- [ ] All 6 examples compile; `cargo publish --dry-run` clean.
- [ ] GitHub: topics (rust, claude, anthropic, ai, llm, sdk, api-client), About description, social-preview image exported from the landing page, GitHub Pages serving `site/` at the `homepage` URL.
- [ ] v0.1.0 git tag + GitHub Release with human-written notes.

## Phase 1 — Launch week

Day 1 — publish `v0.1.0`, enable Pages, verify docs.rs built, star-seed from own network.
Day 2 — **r/rust "Show" post**. Title pattern that front-pages (per research: infra crates with a clear pain story): *"crimson-crab: a production-grade Claude API SDK for Rust — streaming, tools, prompt caching, batches, zero-surprise types"*. Body: the 30-second quickstart, why dedicated beats generic (one paragraph, respectful to rig/genai), the forward-compat story (new models work day one). Reply to every comment fast — responsiveness converts.
Day 3 — **This Week in Rust**: PR to the TWiR repo adding the crate to "New Crates"/Updates (weekly Wednesday deadline). Massive targeted reach, zero cost.
Day 4 — **Show HN**: "Show HN: Crimson Crab – a Rust SDK for the Claude API". HN loves Rust + AI infra; the 1,147-pt DeepSeek-OCR and 333-pt Ferrules posts prove the appetite.
Day 5 — list placements: PRs to `awesome-rust`, awesome-claude/awesome-anthropic lists, awesome-mcp-servers (as the Claude client for Rust MCP servers), and arewelearningyet.com.
Ongoing — X/Twitter thread with the landing-page visuals; Rust Discord (#showcase), Anthropic developer Discord.

## Phase 2 — The cadence engine (weeks 2–12)

- **Ship weekly.** Small releases: one feature or fix per release. Every release = "recently updated" visibility on crates.io + a changelog entry + a tweet. Never go 2 weeks silent.
- **Track Anthropic's changelog religiously.** New beta/model/feature → same-week support release → announce "crimson-crab supports X (day N)". This is the differentiation vs multi-provider clients and the entire moat.
- Issue SLA: first response < 24h. Fast maintainer response is the #1 adoption trust signal for young crates.
- Content drip (dev.to / personal blog / r/rust when substantial): "Build a Claude agent in Rust in 60 lines", "Streaming Claude responses through axum SSE", "Cut Claude costs 90% with prompt caching in Rust", "A Rust MCP server that calls Claude (rmcp + crimson-crab)".

## Phase 3 — Transitive-dependency flywheel (the real download multiplier)

CI/bot traffic from *dependents* is what inflates download counts (research: rmcp doubled lifetime downloads in one quarter this way). Get crates and templates to depend on crimson-crab:

1. Publish a companion **template repo**: `crimson-crab-mcp-template` — a ready-to-clone Rust MCP server (rmcp) that calls Claude. Every clone's CI pulls the crate.
2. Offer integration PRs where honest: examples in rmcp's ecosystem docs, cookbook entries, agent-framework adapters.
3. `cargo generate` template registration.
4. Encourage "built with crimson-crab" via a README badge snippet users can copy.

## Phase 4 — v1.0 credibility march

- Roadmap issues public from day 1 (Files API, schemars tool derive, parse::<T>(), tool-runner helper) — visible momentum invites contributors.
- Label `good-first-issue` generously; contributors become evangelists.
- Milestone releases get announcement posts (0.2: derive tools; 0.3: files; 1.0: stability pledge + "one year of weekly releases" story — that's an r/rust front-page post by itself).

## Metrics dashboard (check weekly)

crates.io recent downloads, dependents count (crates.io "Dependents" tab), GitHub stars/issues open-vs-closed velocity, docs.rs traffic (via shields badge hits), TWiR/HN/Reddit referral spikes. North star: **dependents**, not raw downloads.

## GitHub visibility playbook (how to actually show up)

GitHub search & discovery ranks on: name/description/topic matches, stars (and star *velocity* for Trending), recent activity, and community-profile completeness. Work every lever:

1. **Description written for search**: "The production-grade Rust SDK for Anthropic's Claude API — streaming, tool use, thinking, prompt caching, batches." (contains every term people search: rust, claude, anthropic, api, sdk).
2. **Max out topics** (up to 20; browsable pages + search filters): `rust`, `claude`, `claude-api`, `anthropic`, `ai`, `llm`, `sdk`, `api-client`, `async`, `machine-learning`, `generative-ai`, `claude-sdk`, `mcp`, `agents`, `streaming`.
3. **Social preview image** (Settings → General): export a 1280×640 card from the landing-page hero. Every share on X/Reddit/Slack then renders a branded card instead of a gray box — massively higher click-through.
4. **Community profile 100%**: LICENSE ✅, README ✅, CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, issue templates (bug/feature), PR template. GitHub surfaces complete repos better and devs judge by the checklist.
5. **Trending is a star-velocity game**: concentrate launch pushes (r/rust + HN + TWiR + Discord) into a 48-hour window so stars spike together — that's what gets onto github.com/trending/rust (daily), which then compounds for free.
6. **Releases, not just tags**: every version gets a GitHub Release with human notes — releases notify stargazers and populate the repo's activity feed.
7. **Green activity graph**: the weekly-release cadence keeps the pulse/commit graph alive — dead graphs kill adoption at a glance.
8. **Enable Discussions** (Q&A + Show-and-tell categories) — converts drive-by users into community, and Q&A pages rank in Google.
9. **`good-first-issue` + `help-wanted` labels**: aggregators (goodfirstissue.dev, up-for-grabs.net) index these automatically = free contributor inflow.
10. **Pin the repo** on the singhpratech profile; add a profile README that features it.
11. **GitHub Pages** serving `site/` as the `homepage` link — the repo header then shows a real website, a strong quality signal.
12. **Backlinks**: every blog post, awesome-list PR, and template repo links back — GitHub search and Google both reward it.

## Positioning rules (always)

- Lead with strength: "the production-grade Rust SDK for Claude." Never lead with "unofficial" (single footer line only).
- Never trash competitors by name; cede the multi-provider use case openly — it makes the dedicated pitch credible.
- Every public claim must be true in code (tests/fixtures) — the audience is Rust developers; one caught overclaim kills trust permanently.
