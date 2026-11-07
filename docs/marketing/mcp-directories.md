# MCP directories: steps

For the project owner. Cold Open is in the Official MCP Registry since alpha.3 (2026-11-07), as `io.github.TheYungStump/cold-open`. The formats were read from each directory's own pages on 2026-11-06 (`docs/specs/release.md` §11.6).

## 1. The Official MCP Registry (do this first)

The registry at registry.modelcontextprotocol.io is where MCP clients and most directories look. Cold Open is listed by its **MCP bundle**, `ColdOpen-<version>-mcp.mcpb`, which every release now carries, holding `coldopen mcp` for Windows, macOS and Linux.

1. **After a release is published** (alpha.3 is the first with the bundle), download its **store-files** artifact from the release's Actions run. `mcp-registry/server.json` in it is filled in with that release's bundle address and SHA-256.
2. **Install `mcp-publisher`**, the registry's own tool: `brew install mcp-publisher` on a Mac, or the binary for your OS from the releases of github.com/modelcontextprotocol/registry. On Windows, the registry's quickstart has a one-line PowerShell download (`modelcontextprotocol-io/quickstart.mdx` in that repository), which puts `mcp-publisher.exe` in the current folder.
3. In the folder holding `server.json`:

   ```
   mcp-publisher login github
   mcp-publisher publish
   ```

   `login github` shows a code to enter at github.com/login/device, signed in as **TheYungStump**: the name `io.github.TheYungStump/cold-open` proves the account. `mcp-publisher validate` checks the file first, if you like.
4. Each new release: the new release's `server.json`, and `mcp-publisher publish` again.

## 2. Glama

Glama lists servers from the Official MCP Registry among its sources, so step 1 should bring Cold Open in. To claim the listing or add it sooner, use **Add Server** at glama.ai/mcp/servers with `https://github.com/TheYungStump/cold-open`. Glama's pages didn't document a `glama.json` file when this was written, so none is in the repository.

## 3. Smithery

Smithery is now part of Arcade. Its site says its connection services closed on October 31, 2026, and that it "will remain as a public (non-proxied) MCP registry". Its submission pages weren't reachable when this was written. Look at smithery.ai for an **Add server** link; if there isn't one, the Official MCP Registry listing is the one that matters.

## What the listing says

- **Name:** Cold Open
- **Description (registry, under 100 characters):** Screenplay editing for agents: Fountain and .fdx; edits arrive as tracked changes in Cold Open.
- **Website:** https://theyungstump.github.io/cold-open/ai/
- **What it can do:** read and edit scenes, characters, dialogue and the title page; reports; compare drafts; export PDFs. With the app open, it attaches to the script and its edits arrive as tracked changes the writer accepts or rejects.
