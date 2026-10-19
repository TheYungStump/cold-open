//! The MCP server (docs/specs/mcp.md §2–§4): tools, resources and their
//! update notifications, on top of [`Backend`].

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use coldopen_core::{Block, Document};
use coldopen_screenplay::{ElementKind, Script};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, ResourceUpdatedNotificationParam, ServerCapabilities,
    ServerConfig, SubscribeRequestParams, SubscriptionFilter, UnsubscribeRequestParams,
};
use rmcp::service::{RequestContext, SubscriptionContext};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::tools;

const INSTRUCTIONS: &str = "Cold Open edits screenplays. Start with doc_open on a .fountain file. \
Blocks are named by ids such as \"b_12\" (see doc_get or the coldopen://doc/blocks resource), and \
text offsets count characters, not bytes. Use batch to make several changes one undo step. When \
the writer has the script open in the Cold Open app, doc_open attaches to it: every edit appears \
live in the writer's window, labelled with your name and the tool, and the writer can undo it. You \
may undo only your own steps. Your edits are tracked changes the writer reviews: the `text` of a block \
is its current text, with pending deletions left out, and changes_list shows what is waiting. You may \
withdraw your own changes with changes_reject; accepting is the writer's.";

const URIS: [(&str, &str, &str, &str); 5] = [
    (
        "coldopen://doc/fountain",
        "Script as Fountain",
        "text/plain",
        "The whole script as Fountain text.",
    ),
    (
        "coldopen://doc/blocks",
        "Blocks",
        "application/json",
        "The title page and every block with its id, type and text.",
    ),
    (
        "coldopen://doc/pages",
        "Pages",
        "text/plain",
        "The script laid out on pages, as plain text.",
    ),
    (
        "coldopen://doc/scenes",
        "Scenes",
        "application/json",
        "The scene list: headings, numbers, pages and lengths.",
    ),
    (
        "coldopen://doc/changes",
        "Tracked changes",
        "application/json",
        "The pending tracked changes: who made each, when, and what it inserts and deletes.",
    ),
];

/// The MCP server's state, shared by its request handlers.
#[derive(Clone)]
pub struct ColdOpen {
    backend: Arc<Mutex<Backend>>,
    /// Legacy `resources/subscribe` subscriptions.
    subscribed: Arc<Mutex<BTreeSet<String>>>,
}

impl ColdOpen {
    pub fn new(backend: Backend) -> ColdOpen {
        ColdOpen {
            backend: Arc::new(Mutex::new(backend)),
            subscribed: Arc::default(),
        }
    }

    /// Runs a command off the async runtime: the link and the journal both
    /// block.
    async fn call(
        &self,
        command: &'static str,
        params: Value,
        client: String,
        tool: String,
    ) -> Value {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || match backend.lock() {
            Ok(mut b) => {
                b.refresh();
                b.call(command, params, &client, &tool)
            }
            Err(_) => json!({"ok": false, "error": {"code": "internal", "message": "the server's state is broken"}}),
        })
        .await
        .unwrap_or_else(|e| json!({"ok": false, "error": {"code": "internal", "message": e.to_string()}}))
    }

    fn has_document(&self) -> bool {
        self.backend
            .lock()
            .map(|mut b| {
                b.refresh();
                b.has_document()
            })
            .unwrap_or(false)
    }

    /// Reads one resource's text, or why it can't be read.
    async fn read(&self, uri: &str, client: String) -> Result<(String, &'static str), McpError> {
        let not_found = || McpError::resource_not_found(format!("no resource {uri}"), None);
        let result = |r: Value| -> Result<Value, McpError> {
            if r["ok"] == true {
                Ok(r["result"].clone())
            } else {
                Err(not_found())
            }
        };
        let tool = "read_resource".to_string();
        match uri {
            "coldopen://doc/fountain" => {
                let r = result(self.call("doc.export", json!({}), client, tool).await)?;
                Ok((r["text"].as_str().unwrap_or("").to_string(), "text/plain"))
            }
            "coldopen://doc/pages" => {
                let r = result(
                    self.call("doc.export", json!({"format": "pages"}), client, tool)
                        .await,
                )?;
                Ok((r["text"].as_str().unwrap_or("").to_string(), "text/plain"))
            }
            "coldopen://doc/blocks" => {
                let r = result(self.call("doc.get", json!({}), client, tool).await)?;
                Ok((pretty(&r), "application/json"))
            }
            "coldopen://doc/scenes" => {
                let r = result(self.call("scene.list", json!({}), client, tool).await)?;
                Ok((pretty(&r), "application/json"))
            }
            "coldopen://doc/changes" => {
                let r = result(self.call("changes.list", json!({}), client, tool).await)?;
                Ok((pretty(&r), "application/json"))
            }
            _ => {
                let id = uri
                    .strip_prefix("coldopen://scene/")
                    .ok_or_else(not_found)?;
                let r = result(self.call("doc.get", json!({}), client, tool).await)?;
                scene_fountain(&r, id)
                    .map(|t| (t, "text/plain"))
                    .ok_or_else(not_found)
            }
        }
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// One scene, heading to the next heading or section, as Fountain.
fn scene_fountain(doc: &Value, id: &str) -> Option<String> {
    let blocks: Vec<Block> = serde_json::from_value(doc["blocks"].clone()).ok()?;
    let start = blocks
        .iter()
        .position(|b| b.id.to_string() == id && b.element.kind == ElementKind::SceneHeading)?;
    let end = blocks[start + 1..]
        .iter()
        .position(|b| {
            matches!(
                b.element.kind,
                ElementKind::SceneHeading | ElementKind::Section
            )
        })
        .map_or(blocks.len(), |k| start + 1 + k);
    let doc = Document {
        blocks: blocks[start..end].to_vec(),
        ..Document::default()
    };
    let script = Script {
        elements: doc.blocks.iter().map(|b| b.element.clone()).collect(),
        ..Script::default()
    };
    Some(coldopen_fountain::write(&script).text)
}

fn client_name(ctx: &RequestContext<RoleServer>) -> String {
    coldopen_link::clean_name(&ctx.client_info().map(|i| i.name).unwrap_or_default())
}

fn is_ours(uri: &str) -> bool {
    uri.starts_with("coldopen://doc/") || uri.starts_with("coldopen://scene/")
}

#[allow(deprecated)]
impl ServerHandler for ColdOpen {
    fn get_info(&self) -> ServerConfig {
        let caps = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_list_changed()
            .enable_resources_subscribe()
            .build();
        let mut server = Implementation::from_build_env();
        server.name = "coldopen".into();
        server.title = Some("Cold Open".into());
        server.version = env!("CARGO_PKG_VERSION").into();
        ServerConfig::new(caps)
            .with_server_info(server)
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools::tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let tool = request.name.to_string();
        let Some(command) = tools::command_for(&tool) else {
            return Err(McpError::invalid_params(
                format!("there is no tool \"{tool}\""),
                None,
            ));
        };
        let params = Value::Object(request.arguments.unwrap_or_default());
        let r = self
            .call(command, params, client_name(&context), tool)
            .await;
        let result = if r["ok"] == true {
            CallToolResult::structured(r["result"].clone())
        } else {
            let e = r["error"].clone();
            let mut res = CallToolResult::structured_error(e.clone());
            res.content = vec![ContentBlock::text(format!(
                "{}: {}",
                e["code"].as_str().unwrap_or("error"),
                e["message"].as_str().unwrap_or("")
            ))];
            res
        };
        Ok(result.into())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        if !self.has_document() {
            return Ok(ListResourcesResult::default());
        }
        Ok(ListResourcesResult::with_all_items(
            URIS.iter()
                .map(|(uri, name, mime, desc)| {
                    Resource::new(*uri, *name)
                        .with_mime_type(*mime)
                        .with_description(*desc)
                })
                .collect(),
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new("coldopen://scene/{id}", "Scene")
                .with_mime_type("text/plain")
                .with_description(
                    "One scene as Fountain, from its heading to the next heading or section. \
                     The id is the heading block's, as scene_list gives it.",
                ),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let (text, mime) = self.read(&request.uri, client_name(&context)).await?;
        let contents = ResourceContents::TextResourceContents {
            uri: request.uri.clone(),
            mime_type: Some(mime.into()),
            text,
            meta: None,
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let mut f = SubscriptionFilter::new();
        f.resources_list_changed = requested.resources_list_changed;
        f.resource_subscriptions = requested
            .resource_subscriptions
            .as_ref()
            .map(|v| v.iter().filter(|u| is_ours(u)).cloned().collect());
        Some(f)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let (mut changes, mut lists) = match self.backend.lock() {
            Ok(b) => (b.signals.changes.subscribe(), b.signals.lists.subscribe()),
            Err(_) => return Ok(()),
        };
        let uris = context
            .accepted()
            .resource_subscriptions
            .clone()
            .unwrap_or_default();
        let lists_wanted = context.accepted().resources_list_changed == Some(true);
        loop {
            tokio::select! {
                () = context.cancelled() => return Ok(()),
                r = changes.changed() => {
                    if r.is_err() { return Ok(()); }
                    for u in &uris {
                        let _ = context.sink().notify_resource_updated(u.clone()).await;
                    }
                }
                r = lists.changed() => {
                    if r.is_err() { return Ok(()); }
                    if lists_wanted {
                        let _ = context.sink().notify_resource_list_changed().await;
                    }
                }
            }
        }
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if !is_ours(&request.uri) {
            return Err(McpError::resource_not_found(
                format!("no resource {}", request.uri),
                None,
            ));
        }
        let first = match self.subscribed.lock() {
            Ok(mut s) => {
                let first = s.is_empty();
                s.insert(request.uri.clone());
                first
            }
            Err(_) => false,
        };
        if first {
            // One task tells the client about every subscribed resource.
            let mut changes = match self.backend.lock() {
                Ok(b) => b.signals.changes.subscribe(),
                Err(_) => return Ok(()),
            };
            let subscribed = self.subscribed.clone();
            let peer = context.peer.clone();
            tokio::spawn(async move {
                while changes.changed().await.is_ok() {
                    let uris: Vec<String> = match subscribed.lock() {
                        Ok(s) => s.iter().cloned().collect(),
                        Err(_) => return,
                    };
                    if uris.is_empty() {
                        return;
                    }
                    for u in uris {
                        if peer
                            .notify_resource_updated(ResourceUpdatedNotificationParam::new(u))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            });
        }
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if let Ok(mut s) = self.subscribed.lock() {
            s.remove(&request.uri);
        }
        Ok(())
    }
}

impl Drop for ColdOpen {
    fn drop(&mut self) {
        if Arc::strong_count(&self.backend) == 1
            && let Ok(mut b) = self.backend.lock()
        {
            b.shutdown();
        }
    }
}
