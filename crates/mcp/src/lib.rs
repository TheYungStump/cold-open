//! The Cold Open MCP server, run as `coldopen mcp` (docs/specs/mcp.md).
//! Every command is a tool; the script, its blocks, pages and scenes are
//! resources. When the desktop app has the script open, the server attaches
//! to it over the private agent link, so the writer sees each edit live and
//! can undo it.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod backend;
mod server;
pub mod tools;

use std::path::PathBuf;

pub use backend::Backend;
pub use server::ColdOpen;

/// How `coldopen mcp` starts.
#[derive(Clone, Debug, Default)]
pub struct McpOptions {
    /// A script to open (or attach to) before the first request.
    pub doc: Option<PathBuf>,
    /// Never attach to the app; always open the file in this process.
    pub no_attach: bool,
    /// Where the app's session files are; `None` means never attach.
    pub run_dir: Option<PathBuf>,
}

/// Serves MCP over standard input and output until the client goes away.
pub fn run_stdio(opts: McpOptions) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async move {
        use rmcp::ServiceExt;
        let server = ColdOpen::new(backend_for(&opts));
        let running = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| e.to_string())?;
        running.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// The backend `coldopen mcp` starts with.
pub fn backend_for(opts: &McpOptions) -> Backend {
    let run = if opts.no_attach {
        None
    } else {
        opts.run_dir.clone()
    };
    Backend::new(run, opts.doc.clone().map(|d| (d, !opts.no_attach)))
}
