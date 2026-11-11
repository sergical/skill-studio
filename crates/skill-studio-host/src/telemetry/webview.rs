//! The only path webview telemetry has to Sentry. The webview's Sentry JS
//! SDK hands its envelope items to a Tauri command, and that command calls
//! [`WebviewTelemetry::send`]. Raw envelopes skip `before_send`, so every
//! item is rebuilt here from an allowlist of fields before it is sent: no
//! message text, URL, path, breadcrumb, user, or free-text span description
//! survives.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use sentry::{ClientOptions, Envelope, TransportFactory};
use serde_json::{Map, Value};

use super::{resolve_dsn, Consent, ConsentTransportFactory, SHUTDOWN_FLUSH};

/// Run-time fallback for the compile-time webview DSN.
const WEBVIEW_DSN_ENV_VAR: &str = "SKILL_STUDIO_SENTRY_WEBVIEW_DSN";

/// Error events [`WebviewTelemetry::send`] forwards per process. A render
/// loop that throws on every frame must not turn into a flood.
pub const WEBVIEW_ERROR_CAP: u32 = 20;

const MAX_ITEM_BYTES: usize = 1 << 20;
const MAX_ITEMS_PER_ENVELOPE: usize = 8;
const MAX_EXCEPTIONS: usize = 5;
const MAX_FRAMES: usize = 100;
const MAX_SPANS: usize = 1000;
const MAX_COMPONENT_STACK_LINES: usize = 50;

/// One envelope item as the webview transport sends it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct WebviewItem {
    /// The envelope item type: `event`, `transaction`, or `span`. Any other
    /// type is dropped.
    #[serde(rename = "type")]
    pub kind: String,
    /// The item body exactly as the JS SDK built it.
    pub payload: Value,
}

/// Bounds the serialized request before allocating JSON payload trees.
/// Item bodies remain borrowed until their individual byte limits pass.
pub fn parse_webview_items(raw: &str) -> Option<Vec<WebviewItem>> {
    if raw.len() > MAX_ITEM_BYTES * MAX_ITEMS_PER_ENVELOPE {
        return None;
    }
    struct Items<'a>(Vec<&'a serde_json::value::RawValue>);
    impl<'de> serde::Deserialize<'de> for Items<'de> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = Items<'de>;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("at most eight webview items")
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut items = Vec::new();
                    while items.len() < MAX_ITEMS_PER_ENVELOPE {
                        let Some(item) = seq.next_element::<&serde_json::value::RawValue>()? else {
                            return Ok(Items(items));
                        };
                        if item.get().len() > MAX_ITEM_BYTES {
                            return Err(serde::de::Error::custom("webview item too large"));
                        }
                        items.push(item);
                    }
                    if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                        return Err(serde::de::Error::custom("too many webview items"));
                    }
                    Ok(Items(items))
                }
            }
            deserializer.deserialize_seq(Visitor)
        }
    }
    let items: Items<'_> = serde_json::from_str(raw).ok()?;
    items
        .0
        .into_iter()
        .map(|item| serde_json::from_str(item.get()).ok())
        .collect()
}

/// A Sentry client for the webview project, gated by the same [`Consent`]
/// as the Rust client.
pub struct WebviewTelemetry {
    client: Arc<sentry::Client>,
    consent: Consent,
    errors_sent: AtomicU32,
}

impl WebviewTelemetry {
    /// Builds the client when a webview DSN is available (compile-time
    /// `SKILL_STUDIO_SENTRY_WEBVIEW_DSN` wins, then the same variable at run
    /// time). Returns `None` when there is no valid DSN.
    pub fn init(app_version: &str, consent: Consent) -> Option<Self> {
        let dsn = resolve_dsn(
            option_env!("SKILL_STUDIO_SENTRY_WEBVIEW_DSN"),
            std::env::var(WEBVIEW_DSN_ENV_VAR).ok(),
        )?;
        let transport = Arc::new(ConsentTransportFactory {
            consent: consent.clone(),
        });
        Some(Self::with_transport(dsn, app_version, consent, transport))
    }

    fn with_transport(
        dsn: sentry::types::Dsn,
        app_version: &str,
        consent: Consent,
        transport: Arc<dyn TransportFactory>,
    ) -> Self {
        let mut options = ClientOptions::default();
        options.dsn = Some(dsn);
        options.release = Some(format!("skill-studio@{app_version}").into());
        options.environment = Some(super::SentryTelemetry::environment().into());
        options.send_default_pii = false;
        options.max_breadcrumbs = 0;
        options.default_integrations = false;
        options.shutdown_timeout = SHUTDOWN_FLUSH;
        options.transport = Some(transport);
        Self {
            client: Arc::new(sentry::Client::with_options(options)),
            consent,
            errors_sent: AtomicU32::new(0),
        }
    }

    /// Scrubs `items` and sends the ones that survive as one envelope.
    /// Sends nothing while consent is off. Returns how many items it sent.
    pub fn send_json(&self, raw: &str) -> usize {
        if !self.consent.enabled() {
            return 0;
        }
        parse_webview_items(raw).map_or(0, |items| self.send(items))
    }

    fn send(&self, items: Vec<WebviewItem>) -> usize {
        if !self.consent.enabled() {
            return 0;
        }
        let options = self.client.options();
        let stamp = Stamp {
            release: options.release.as_deref(),
            environment: options.environment.as_deref(),
        };
        let mut body = Vec::new();
        let mut trace_id = None;
        let mut sent = 0;
        for item in items.into_iter().take(MAX_ITEMS_PER_ENVELOPE) {
            let Some(payload) = scrub_item(&item.kind, &item.payload, &stamp) else {
                continue;
            };
            let Ok(bytes) = serde_json::to_vec(&payload) else {
                continue;
            };
            if bytes.len() > MAX_ITEM_BYTES {
                continue;
            }
            if item.kind == "event"
                && self.errors_sent.fetch_add(1, Ordering::Relaxed) >= WEBVIEW_ERROR_CAP
            {
                continue;
            }
            trace_id = trace_id.or_else(|| item_trace_id(&payload));
            let mut header = serde_json::json!({ "type": item.kind, "length": bytes.len() });
            if item.kind == "span" && payload.get("version") == Some(&Value::from(2)) {
                header["item_count"] = Value::from(payload["items"].as_array().map_or(0, Vec::len));
                header["content_type"] = "application/vnd.sentry.items.span.v2+json".into();
            }
            body.extend_from_slice(header.to_string().as_bytes());
            body.push(b'\n');
            body.extend_from_slice(&bytes);
            body.push(b'\n');
            sent += 1;
        }
        if sent == 0 {
            return 0;
        }
        let mut envelope_header = Map::new();
        if let (Some(trace_id), Some(dsn)) = (trace_id, options.dsn.as_ref()) {
            let mut trace = Map::new();
            trace.insert("trace_id".into(), Value::String(trace_id));
            trace.insert("public_key".into(), dsn.public_key().into());
            trace.insert("sampled".into(), "true".into());
            if let Some(release) = stamp.release {
                trace.insert("release".into(), release.into());
            }
            if let Some(environment) = stamp.environment {
                trace.insert("environment".into(), environment.into());
            }
            envelope_header.insert("trace".into(), Value::Object(trace));
        }
        let mut bytes = Value::Object(envelope_header).to_string().into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(&body);
        if let Ok(envelope) = Envelope::from_bytes_raw(bytes) {
            self.client.send_envelope(envelope);
        }
        sent
    }

    /// Flushes queued envelopes for at most [`SHUTDOWN_FLUSH`] and closes
    /// the client.
    pub fn shutdown(&self) -> bool {
        self.client.close(Some(SHUTDOWN_FLUSH))
    }
}

struct Stamp<'a> {
    release: Option<&'a str>,
    environment: Option<&'a str>,
}

fn item_trace_id(payload: &Value) -> Option<String> {
    payload
        .pointer("/items/0/trace_id")
        .or_else(|| payload.pointer("/contexts/trace/trace_id"))
        .or_else(|| payload.get("trace_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn scrub_item(kind: &str, payload: &Value, stamp: &Stamp<'_>) -> Option<Value> {
    match kind {
        "event" => scrub_event(payload, stamp),
        "transaction" => scrub_transaction(payload, stamp),
        "span" => scrub_span_container(payload),
        _ => None,
    }
}

fn scrub_span_container(payload: &Value) -> Option<Value> {
    if payload.get("version").and_then(Value::as_u64) != Some(2) {
        return scrub_span(payload);
    }
    let items: Vec<Value> = payload
        .get("items")?
        .as_array()?
        .iter()
        .take(MAX_SPANS)
        .filter_map(scrub_streamed_span)
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(serde_json::json!({"version": 2, "ingest_settings": {
        "infer_ip": "never", "infer_user_agent": "never"
    }, "items": items}))
}

fn scrub_streamed_span(span: &Value) -> Option<Value> {
    let mut out = Map::new();
    out.insert("trace_id".into(), hex_id(span.get("trace_id"), 32)?);
    out.insert("span_id".into(), hex_id(span.get("span_id"), 16)?);
    put(
        &mut out,
        "parent_span_id",
        hex_id(span.get("parent_span_id"), 16),
    );
    out.insert(
        "start_timestamp".into(),
        number(span.get("start_timestamp"))?,
    );
    out.insert("end_timestamp".into(), number(span.get("end_timestamp"))?);
    put(&mut out, "is_segment", boolean(span.get("is_segment")));
    let attrs = span.get("attributes")?.as_object()?;
    let untyped: Map<String, Value> = attrs
        .iter()
        .filter_map(|(key, value)| value.get("value").map(|v| (key.clone(), v.clone())))
        .collect();
    let op = untyped.get("sentry.op").and_then(Value::as_str);
    let name = if op == Some("ipc") {
        Some(command_name(span.get("name")).unwrap_or_else(|| "ipc.unknown".into()))
    } else {
        Some(view_name(span.get("name")))
    };
    put(&mut out, "name", name);
    let data = scrub_span_data(Some(&Value::Object(untyped)));
    let attributes: Map<String, Value> = data
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| {
            let kind = if v.is_string() {
                "string"
            } else if v.is_boolean() {
                "boolean"
            } else if v.is_u64() || v.is_i64() {
                "integer"
            } else {
                "double"
            };
            (k, serde_json::json!({"type": kind, "value": v}))
        })
        .collect();
    out.insert("attributes".into(), Value::Object(attributes));
    put(
        &mut out,
        "status",
        allowed(span.get("status"), &["ok", "error"]),
    );
    Some(Value::Object(out))
}

fn allowed(value: Option<&Value>, names: &[&str]) -> Option<Value> {
    let raw = value?.as_str()?;
    names.contains(&raw).then(|| raw.into())
}

fn span_op(value: Option<&Value>) -> Option<Value> {
    allowed(
        value,
        &[
            "ipc",
            "navigation",
            "pageload",
            "ui.long-task",
            "ui.interaction.click",
            "ui.interaction.keypress",
            "ui.interaction",
            "ui.long-animation-frame",
        ],
    )
}
fn span_origin(value: Option<&Value>) -> Option<Value> {
    allowed(
        value,
        &[
            "manual",
            "auto.navigation.browser",
            "auto.pageload.browser",
            "auto.ui.browser",
            "auto.ui.browser.inp",
            "auto.ui.browser.metrics",
        ],
    )
}
fn span_status(value: Option<&Value>) -> Option<Value> {
    allowed(
        value,
        &[
            "ok",
            "error",
            "cancelled",
            "unknown_error",
            "invalid_argument",
            "deadline_exceeded",
            "not_found",
            "permission_denied",
            "internal_error",
            "unavailable",
        ],
    )
}

fn command_name(value: Option<&Value>) -> Option<Value> {
    allowed(value, COMMAND_NAMES)
}

const COMMAND_NAMES: &[&str] = &[
    "add_skill",
    "app_version",
    "apply_skill_frontmatter_repair",
    "cancel_add_skill_operation",
    "cancel_update_all",
    "check_for_update",
    "check_skill_updates_now",
    "confirm_add_skill_trust",
    "data_folder_status",
    "detect_harnesses",
    "discard_skill_copy",
    "fix_skill",
    "fork_skill",
    "get_add_method_defaults",
    "get_add_skill_operation",
    "get_background_status",
    "get_discovery_sources",
    "get_editor_choices",
    "get_harnesses_choice",
    "get_install_counts",
    "get_installed_skills",
    "get_popular_skills",
    "get_skill_details",
    "get_skill_snapshot",
    "get_telemetry_enabled",
    "get_tracked_projects",
    "get_update_status",
    "get_webview_telemetry",
    "import_tracked_projects",
    "install_update",
    "list_github_skills",
    "list_project_folders",
    "list_skill_events",
    "make_skill_independent_copy",
    "materialize_harness_root",
    "materialize_harness_root_then_disable",
    "open_conflict_paths",
    "open_skill_path",
    "park_check",
    "park_skill",
    "park_skills",
    "preview_skill_frontmatter_repair",
    "pull_fork_upstream",
    "read_installed_skill_md",
    "register_skill_projects",
    "remove_skill",
    "remove_skill_project",
    "repair_skill_link",
    "request_skill_rescan",
    "rescan_skills_now",
    "restore_moved_deployment",
    "restore_skill_event",
    "save_harnesses_choice",
    "scan_skills",
    "search_skills",
    "send_webview_envelope",
    "set_discovery_source",
    "set_harness_enabled",
    "set_plugin_enabled",
    "set_preferred_editor",
    "set_skills_invocation",
    "set_telemetry_enabled",
    "skill_local_edits",
    "split_skill",
    "split_skill_targets",
    "start_add_skill_operation",
    "start_add_skills_operation",
    "turn_off_check",
    "turn_off_for_agent",
    "unfork_skill",
    "uninstall_plugin",
    "unlist_parked_dotagents",
    "unpark_skill",
    "unpark_skills",
    "unregister_skill_project",
    "update_all_skills",
    "update_plugin",
    "update_skill",
    "write_installed_skill_md_if_unchanged",
];

fn hex_id(value: Option<&Value>, len: usize) -> Option<Value> {
    let raw = value?.as_str()?;
    (raw.len() == len && raw.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| Value::String(raw.to_ascii_lowercase()))
}

fn number(value: Option<&Value>) -> Option<Value> {
    value.filter(|value| value.is_number()).cloned()
}

fn boolean(value: Option<&Value>) -> Option<Value> {
    value.filter(|value| value.is_boolean()).cloned()
}

/// A JS identifier, dotted path of identifiers, or error class name, such
/// as `TypeError`, `SkillRow`, or `Object.render`.
fn is_identifier(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 64
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.'))
}

/// A code-defined name such as an op (`ui.long-task`), a status
/// (`deadline_exceeded`), or a view (`view.dashboard`).
fn is_code_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 64
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

fn identifier(value: Option<&Value>) -> Option<Value> {
    let raw = value?.as_str()?;
    is_identifier(raw).then(|| Value::String(raw.to_string()))
}

fn code_name(value: Option<&Value>) -> Option<Value> {
    let raw = value?.as_str()?;
    is_code_name(raw).then(|| Value::String(raw.to_string()))
}

/// Keeps only `view.<name>`, the names the webview gives its screens.
fn view_name(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_str)
        .filter(|raw| {
            matches!(
                *raw,
                "view.home"
                    | "view.skills"
                    | "view.plugins"
                    | "view.activity"
                    | "view.learn"
                    | "view.settings"
                    | "view.skill"
                    | "view.unknown"
            )
        })
        .map_or_else(|| "view.unknown".into(), |raw| raw.to_string().into())
}

fn put(out: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        out.insert(key.to_string(), value);
    }
}

fn stamp_common(out: &mut Map<String, Value>, payload: &Value, stamp: &Stamp<'_>) {
    out.insert("platform".into(), "javascript".into());
    put(out, "release", stamp.release.map(Into::into));
    put(out, "environment", stamp.environment.map(Into::into));
    let mut tags = Map::new();
    tags.insert("surface".into(), "desktop".into());
    tags.insert("source".into(), "webview".into());
    out.insert("tags".into(), Value::Object(tags));
    let sdk = payload.get("sdk");
    let mut sdk_out = Map::new();
    put(
        &mut sdk_out,
        "name",
        code_name(sdk.and_then(|sdk| sdk.get("name"))),
    );
    put(
        &mut sdk_out,
        "version",
        code_name(sdk.and_then(|sdk| sdk.get("version"))),
    );
    if !sdk_out.is_empty() {
        out.insert("sdk".into(), Value::Object(sdk_out));
    }
}

fn event_id(payload: &Value) -> Value {
    hex_id(payload.get("event_id"), 32)
        .unwrap_or_else(|| sentry::types::random_uuid().simple().to_string().into())
}

/// An error event keeps its exception types, mechanism, stack frames
/// (function name, file basename, line, column), trace context, and the
/// React component names. It loses the exception message, `message`,
/// breadcrumbs, request, user, extra, tags, and every other field. An event
/// with no exception (a `captureMessage`) is dropped.
fn scrub_event(payload: &Value, stamp: &Stamp<'_>) -> Option<Value> {
    let exceptions: Vec<Value> = payload
        .pointer("/exception/values")?
        .as_array()?
        .iter()
        .rev()
        .take(MAX_EXCEPTIONS)
        .rev()
        .map(scrub_exception)
        .collect();
    if exceptions.is_empty() {
        return None;
    }
    let mut out = Map::new();
    out.insert("event_id".into(), event_id(payload));
    put(&mut out, "timestamp", number(payload.get("timestamp")));
    let level = payload
        .get("level")
        .and_then(Value::as_str)
        .filter(|level| matches!(*level, "fatal" | "error" | "warning"))
        .unwrap_or("error");
    out.insert("level".into(), level.into());
    out.insert(
        "exception".into(),
        serde_json::json!({ "values": exceptions }),
    );
    if payload.get("transaction").is_some() {
        out.insert("transaction".into(), view_name(payload.get("transaction")));
    }
    let mut contexts = Map::new();
    put(
        &mut contexts,
        "trace",
        scrub_trace_context(payload.pointer("/contexts/trace"), false),
    );
    put(
        &mut contexts,
        "react",
        scrub_react_context(payload.pointer("/contexts/react/componentStack")),
    );
    out.insert("contexts".into(), Value::Object(contexts));
    stamp_common(&mut out, payload, stamp);
    Some(Value::Object(out))
}

fn scrub_exception(exception: &Value) -> Value {
    let mut out = Map::new();
    out.insert(
        "type".into(),
        identifier(exception.get("type")).unwrap_or_else(|| "Error".into()),
    );
    if let Some(mechanism) = exception.get("mechanism") {
        let mut mechanism_out = Map::new();
        mechanism_out.insert(
            "type".into(),
            code_name(mechanism.get("type")).unwrap_or_else(|| "generic".into()),
        );
        put(
            &mut mechanism_out,
            "handled",
            boolean(mechanism.get("handled")),
        );
        out.insert("mechanism".into(), Value::Object(mechanism_out));
    }
    if let Some(frames) = exception
        .pointer("/stacktrace/frames")
        .and_then(Value::as_array)
    {
        let frames: Vec<Value> = frames
            .iter()
            .rev()
            .take(MAX_FRAMES)
            .rev()
            .map(scrub_frame)
            .collect();
        out.insert("stacktrace".into(), serde_json::json!({ "frames": frames }));
    }
    Value::Object(out)
}

fn scrub_frame(frame: &Value) -> Value {
    let mut out = Map::new();
    put(&mut out, "function", identifier(frame.get("function")));
    let file = frame
        .get("abs_path")
        .or_else(|| frame.get("filename"))
        .and_then(Value::as_str)
        .and_then(app_file);
    if let Some(file) = file {
        out.insert("filename".into(), file.clone().into());
        out.insert("abs_path".into(), file.into());
    }
    put(&mut out, "lineno", number(frame.get("lineno")));
    put(&mut out, "colno", number(frame.get("colno")));
    put(&mut out, "in_app", boolean(frame.get("in_app")));
    Value::Object(out)
}

/// `tauri://localhost/assets/index-abc.js?v=1` becomes
/// the fixed bundle name `app:///assets/index.js`. Other filenames are dropped.
fn app_file(raw: &str) -> Option<String> {
    let path = raw.split(['?', '#']).next()?;
    let base = path
        .strip_prefix("tauri://localhost/assets/")
        .or_else(|| path.strip_prefix("http://tauri.localhost/assets/"))
        .or_else(|| path.strip_prefix("app:///assets/"))?;
    let hash = base.strip_prefix("index-")?.strip_suffix(".js")?;
    if hash.is_empty()
        || hash.len() > 16
        || !hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return None;
    }
    Some("app:///assets/index.js".into())
}

/// Rebuilds React's component stack from component names only. Each line
/// of React's own stack (`at SkillRow (tauri://localhost/assets/x.js:1:2)`)
/// becomes `at SkillRow`.
fn scrub_react_context(component_stack: Option<&Value>) -> Option<Value> {
    let raw = component_stack?.as_str()?;
    let names: Vec<&str> = raw
        .lines()
        .filter_map(|line| line.trim().strip_prefix("at "))
        .filter_map(|rest| rest.split([' ', '(']).next())
        .filter(|name| is_identifier(name))
        .take(MAX_COMPONENT_STACK_LINES)
        .collect();
    if names.is_empty() {
        return None;
    }
    let stack: String = names.iter().flat_map(|name| ["\n    at ", name]).collect();
    Some(serde_json::json!({ "componentStack": stack }))
}

fn scrub_trace_context(trace: Option<&Value>, with_data: bool) -> Option<Value> {
    let trace = trace?;
    let mut out = Map::new();
    out.insert("trace_id".into(), hex_id(trace.get("trace_id"), 32)?);
    out.insert("span_id".into(), hex_id(trace.get("span_id"), 16)?);
    put(
        &mut out,
        "parent_span_id",
        hex_id(trace.get("parent_span_id"), 16),
    );
    put(&mut out, "op", span_op(trace.get("op")));
    put(&mut out, "status", span_status(trace.get("status")));
    put(&mut out, "origin", span_origin(trace.get("origin")));
    if with_data {
        put(&mut out, "data", scrub_span_data(trace.get("data")));
    }
    Some(Value::Object(out))
}

/// A transaction keeps its ids, timing, op, status, Web Vitals
/// measurements, and spans. Its name must be a `view.<name>` screen name.
fn scrub_transaction(payload: &Value, stamp: &Stamp<'_>) -> Option<Value> {
    let mut out = Map::new();
    out.insert("event_id".into(), event_id(payload));
    out.insert("type".into(), "transaction".into());
    out.insert(
        "start_timestamp".into(),
        number(payload.get("start_timestamp"))?,
    );
    out.insert("timestamp".into(), number(payload.get("timestamp"))?);
    out.insert("transaction".into(), view_name(payload.get("transaction")));
    out.insert(
        "transaction_info".into(),
        serde_json::json!({ "source": "custom" }),
    );
    let mut contexts = Map::new();
    contexts.insert(
        "trace".into(),
        scrub_trace_context(payload.pointer("/contexts/trace"), true)?,
    );
    out.insert("contexts".into(), Value::Object(contexts));
    let spans: Vec<Value> = payload
        .get("spans")
        .and_then(Value::as_array)
        .map(|spans| {
            spans
                .iter()
                .take(MAX_SPANS)
                .filter_map(scrub_span)
                .collect()
        })
        .unwrap_or_default();
    out.insert("spans".into(), Value::Array(spans));
    put(
        &mut out,
        "measurements",
        scrub_measurements(payload.get("measurements")),
    );
    stamp_common(&mut out, payload, stamp);
    Some(Value::Object(out))
}

/// A span keeps its ids, timing, op, status, and origin. Its description
/// is kept only for an `ipc` span, where it is the Tauri command name.
fn scrub_span(span: &Value) -> Option<Value> {
    let mut out = Map::new();
    out.insert("span_id".into(), hex_id(span.get("span_id"), 16)?);
    out.insert("trace_id".into(), hex_id(span.get("trace_id"), 32)?);
    put(
        &mut out,
        "parent_span_id",
        hex_id(span.get("parent_span_id"), 16),
    );
    put(&mut out, "segment_id", hex_id(span.get("segment_id"), 16));
    out.insert(
        "start_timestamp".into(),
        number(span.get("start_timestamp"))?,
    );
    put(&mut out, "timestamp", number(span.get("timestamp")));
    put(
        &mut out,
        "exclusive_time",
        number(span.get("exclusive_time")),
    );
    put(&mut out, "is_segment", boolean(span.get("is_segment")));
    let op = span_op(span.get("op"));
    if op.as_ref().and_then(Value::as_str) == Some("ipc") {
        put(
            &mut out,
            "description",
            command_name(span.get("description")),
        );
    }
    put(&mut out, "op", op);
    put(&mut out, "status", span_status(span.get("status")));
    put(&mut out, "origin", span_origin(span.get("origin")));
    put(&mut out, "data", scrub_span_data(span.get("data")));
    put(
        &mut out,
        "measurements",
        scrub_measurements(span.get("measurements")),
    );
    Some(Value::Object(out))
}

/// Keeps fixed SDK attributes and validates each value for its field.
fn scrub_span_data(data: Option<&Value>) -> Option<Value> {
    let data = data?.as_object()?;
    let mut out = Map::new();
    for (key, value) in data {
        let kept = match key.as_str() {
            "transaction" | "sentry.transaction" | "sentry.segment.name" => {
                Some(view_name(Some(value)))
            }
            "sentry.op" => span_op(Some(value)),
            "sentry.origin" => span_origin(Some(value)),
            "sentry.source" => allowed(Some(value), &["custom", "component", "route", "task"]),
            "sentry.exclusive_time"
            | "sentry.sample_rate"
            | "browser.web_vital.inp.value"
            | "browser.web_vital.lcp.value"
            | "browser.web_vital.cls.value"
            | "browser.web_vital.fcp.value"
            | "browser.web_vital.ttfb.value" => number(Some(value)),
            "sentry.segment.id" => hex_id(Some(value), 16),
            _ => None,
        };
        put(&mut out, key, kept);
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

fn scrub_measurements(measurements: Option<&Value>) -> Option<Value> {
    let measurements = measurements?.as_object()?;
    let mut out = Map::new();
    for (name, measurement) in measurements {
        if !matches!(
            name.as_str(),
            "lcp" | "fcp" | "fp" | "cls" | "inp" | "ttfb" | "fid"
        ) {
            continue;
        }
        let Some(value) = number(measurement.get("value")) else {
            continue;
        };
        let mut kept = Map::new();
        kept.insert("value".into(), value);
        put(
            &mut kept,
            "unit",
            allowed(measurement.get("unit"), &["millisecond", "second", "none"]),
        );
        out.insert(name.clone(), Value::Object(kept));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use sentry::Transport;

    #[test]
    fn sdk11_streamed_screen_ipc_long_task_and_inp_envelope_survives_without_schema_loss() {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/sdk11-spans.json")).unwrap();
        let consent = Consent::new(true);
        let (telemetry, recording) = webview(&consent);
        let sdk_item = &fixture[1][0];
        let raw =
            serde_json::json!([{"type": sdk_item[0]["type"], "payload": sdk_item[1]}]).to_string();
        assert_eq!(telemetry.send_json(&raw), 1);
        let bodies = sent(&recording);
        let header: Value = serde_json::from_str(bodies[0].lines().nth(1).unwrap()).unwrap();
        assert_eq!(header["item_count"], 4);
        assert_eq!(
            header["content_type"],
            "application/vnd.sentry.items.span.v2+json"
        );
        let payload = &items_of(&bodies[0])[0];
        assert_eq!(payload["version"], 2);
        for (actual, expected) in payload["items"]
            .as_array()
            .unwrap()
            .iter()
            .zip(sdk_item[1]["items"].as_array().unwrap())
        {
            for key in [
                "trace_id",
                "span_id",
                "parent_span_id",
                "start_timestamp",
                "end_timestamp",
                "status",
            ] {
                assert_eq!(actual[key], expected[key], "lost SDK field {key}");
            }
            assert_eq!(
                actual["attributes"]["sentry.op"],
                expected["attributes"]["sentry.op"]
            );
        }
        assert_eq!(payload["items"][1]["name"], "scan_skills");
        assert_eq!(
            payload["items"][3]["attributes"]["browser.web_vital.inp.value"]["value"],
            100
        );
    }

    #[test]
    fn webview_permitted_character_secrets_are_removed_from_names_attributes_and_frames() {
        let stamp = Stamp {
            release: None,
            environment: None,
        };
        let payload = serde_json::json!({
            "transaction": "view.secret-skill", "start_timestamp": 1, "timestamp": 2,
            "contexts": {"trace": {"trace_id": "0123456789abcdef0123456789abcdef", "span_id": "0123456789abcdef",
                "data": {"sentry.skill": "secret-skill", "sentry.user": "alice", "sentry.op": "secret-skill", "sentry.origin": "alice"}}},
            "measurements": {"secret-skill": {"value": 1}, "lcp": {"value": 2, "unit": "alice"}},
            "spans": [{"trace_id": "0123456789abcdef0123456789abcdef", "span_id": "0123456789abcdef",
                "start_timestamp": 1, "op": "ipc", "description": "secret_skill"}]
        });
        let result = scrub_transaction(&payload, &stamp).unwrap().to_string();
        for secret in ["secret-skill", "secret_skill", "alice"] {
            assert!(!result.contains(secret));
        }
        for file in [
            "file:///Users/alice/private-skill.js",
            "tauri://localhost/assets/private-skill.js",
            "file:///Users/alice/assets/index-alice.js",
        ] {
            assert!(app_file(file).is_none());
        }
    }

    #[test]
    fn webview_oversized_discarded_field_is_rejected_before_scrubbing_can_hide_its_size() {
        let raw = serde_json::json!([{"type": "event", "payload": {"discarded": "x".repeat(MAX_ITEM_BYTES)}}]).to_string();
        assert!(parse_webview_items(&raw).is_none());
        assert!(
            parse_webview_items(&" ".repeat(MAX_ITEM_BYTES * MAX_ITEMS_PER_ENVELOPE + 1)).is_none()
        );
    }

    #[test]
    fn webview_excess_items_are_rejected_before_payload_trees_are_allocated() {
        let item = serde_json::json!({"type": "event", "payload": {}});
        assert_eq!(
            parse_webview_items(
                &serde_json::to_string(&vec![item.clone(); MAX_ITEMS_PER_ENVELOPE]).unwrap()
            )
            .unwrap()
            .len(),
            MAX_ITEMS_PER_ENVELOPE
        );
        assert!(parse_webview_items(
            &serde_json::to_string(&vec![item; MAX_ITEMS_PER_ENVELOPE + 1]).unwrap()
        )
        .is_none());
    }

    const SENTINELS: &[&str] = &[
        "Users",
        "alice",
        "secret-skill",
        "alices-mbp",
        ".claude",
        "Cannot read",
    ];

    #[derive(Default)]
    struct RecordingTransport {
        bodies: Mutex<Vec<String>>,
    }

    impl Transport for RecordingTransport {
        fn send_envelope(&self, envelope: Envelope) {
            let mut bytes = Vec::new();
            envelope
                .to_writer(&mut bytes)
                .expect("an envelope serializes");
            self.bodies
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(String::from_utf8_lossy(&bytes).into_owned());
        }
    }

    struct FixedTransportFactory(Arc<RecordingTransport>);

    impl TransportFactory for FixedTransportFactory {
        fn create_transport_with_options(
            &self,
            _options: sentry::TransportOptions,
        ) -> Arc<dyn Transport> {
            self.0.clone()
        }
    }

    fn webview(consent: &Consent) -> (WebviewTelemetry, Arc<RecordingTransport>) {
        let recording = Arc::new(RecordingTransport::default());
        let dsn = "https://examplePublicKey@o0.ingest.sentry.io/0"
            .parse()
            .expect("placeholder dsn parses");
        let telemetry = WebviewTelemetry::with_transport(
            dsn,
            "9.9.9",
            consent.clone(),
            Arc::new(FixedTransportFactory(recording.clone())),
        );
        (telemetry, recording)
    }

    fn sent(recording: &RecordingTransport) -> Vec<String> {
        recording
            .bodies
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The item bodies of one sent envelope, parsed back to JSON.
    fn items_of(body: &str) -> Vec<Value> {
        body.lines()
            .skip(1)
            .skip(1)
            .step_by(2)
            .map(|line| serde_json::from_str(line).expect("each item body is JSON"))
            .collect()
    }

    fn item(kind: &str, payload: Value) -> WebviewItem {
        WebviewItem {
            kind: kind.to_string(),
            payload,
        }
    }

    fn sensitive_error_event() -> Value {
        serde_json::json!({
            "event_id": "0123456789abcdef0123456789abcdef",
            "timestamp": 1_700_000_000.5,
            "level": "error",
            "platform": "javascript",
            "message": "failed to read /Users/alice/.claude/skills/secret-skill",
            "logentry": { "message": "secret-skill" },
            "server_name": "alices-mbp",
            "user": { "username": "alice", "ip_address": "10.0.0.1" },
            "request": { "url": "tauri://localhost/Users/alice", "headers": { "User-Agent": "alices-mbp" } },
            "breadcrumbs": [{ "message": "opened secret-skill", "category": "ui.click" }],
            "extra": { "path": "/Users/alice/.claude/skills/secret-skill" },
            "tags": { "skill": "secret-skill" },
            "fingerprint": ["secret-skill"],
            "transaction": "/Users/alice/secret-skill",
            "exception": { "values": [{
                "type": "TypeError",
                "value": "Cannot read properties of undefined (reading 'secret-skill')",
                "mechanism": { "type": "auto.function.react.error_handler", "handled": false, "data": { "path": "/Users/alice" } },
                "stacktrace": { "frames": [
                    {
                        "function": "renderRow",
                        "filename": "tauri://localhost/assets/index-abc123.js?v=1",
                        "abs_path": "tauri://localhost/assets/index-abc123.js?v=1",
                        "lineno": 12, "colno": 34, "in_app": true,
                        "context_line": "const name = 'secret-skill'",
                        "vars": { "home": "/Users/alice" }
                    },
                    {
                        "function": "load /Users/alice/secret-skill",
                        "filename": "file:///Users/alice/.claude/skills/secret-skill/run.js",
                        "lineno": 1, "colno": 1
                    }
                ] }
            }] },
            "contexts": {
                "trace": { "trace_id": "0af7651916cd43dd8448eb211c80319c", "span_id": "b7ad6b7169203331", "data": { "url": "/Users/alice" } },
                "react": { "componentStack": "\n    at SkillRow (tauri://localhost/assets/index-abc123.js:1:2)\n    at /Users/alice/secret-skill (file:///Users/alice/x.js:1:1)\n    at div" },
                "os": { "name": "alices-mbp" },
                "culture": { "timezone": "Europe/Kyiv" }
            },
            "sdk": { "name": "sentry.javascript.react", "version": "11.11.0", "packages": [{ "name": "npm:@sentry/react" }] }
        })
    }

    /// guards: the webview path sending anything while the telemetry switch
    /// is off. JS envelopes skip the Rust client's own gate unless `send`
    /// checks consent itself.
    #[test]
    fn a_webview_envelope_is_dropped_while_the_switch_is_off() {
        let consent = Consent::new(false);
        let (telemetry, recording) = webview(&consent);

        let sent_while_off = telemetry.send(vec![item("event", sensitive_error_event())]);

        assert_eq!(sent_while_off, 0, "send reported an item while off");
        assert!(
            sent(&recording).is_empty(),
            "an envelope reached the transport while the switch was off"
        );

        consent.set(true);
        assert_eq!(
            telemetry.send(vec![item("event", sensitive_error_event())]),
            1,
            "an event must be sent once the switch is on"
        );
        assert_eq!(sent(&recording).len(), 1);
    }

    /// guards: a path, skill name, user name, or machine name in a JS
    /// error's message, exception value, breadcrumb, frame, request, tag,
    /// or React component stack reaching Sentry. Also guards the scrub
    /// dropping the parts that make the error useful: the exception type,
    /// the in-app frame, and the component names.
    #[test]
    fn a_webview_error_with_a_path_or_skill_name_is_scrubbed_before_sending() {
        let consent = Consent::new(true);
        let (telemetry, recording) = webview(&consent);

        telemetry.send(vec![item("event", sensitive_error_event())]);

        let bodies = sent(&recording);
        let body = bodies.first().expect("one envelope was sent");
        for sentinel in SENTINELS {
            assert!(
                !body.contains(sentinel),
                "the sent envelope still contains {sentinel:?}: {body}"
            );
        }
        let event = &items_of(body)[0];
        let exception = &event["exception"]["values"][0];
        assert_eq!(exception["type"], "TypeError");
        assert_eq!(exception["mechanism"]["handled"], false);
        let frames = exception["stacktrace"]["frames"]
            .as_array()
            .expect("frames are kept");
        assert_eq!(frames[0]["function"], "renderRow");
        assert_eq!(frames[0]["filename"], "app:///assets/index.js");
        assert_eq!(frames[0]["lineno"], 12);
        assert!(frames[1].get("filename").is_none());
        assert_eq!(
            event["contexts"]["react"]["componentStack"],
            "\n    at SkillRow\n    at div"
        );
        assert_eq!(
            event["contexts"]["trace"]["trace_id"],
            "0af7651916cd43dd8448eb211c80319c"
        );
        assert_eq!(event["tags"]["source"], "webview");
        assert_eq!(event["release"], "skill-studio@9.9.9");
    }

    /// guards: a span description, span attribute, or transaction name that
    /// carries a path or a skill name reaching Sentry, and an `ipc` span
    /// losing the command name that links it to the Rust command root.
    #[test]
    fn a_webview_transaction_with_a_path_in_a_span_is_scrubbed_before_sending() {
        let consent = Consent::new(true);
        let (telemetry, recording) = webview(&consent);
        let transaction = serde_json::json!({
            "event_id": "1123456789abcdef0123456789abcdef",
            "type": "transaction",
            "transaction": "view.home",
            "start_timestamp": 1_700_000_000.0,
            "timestamp": 1_700_000_001.0,
            "contexts": { "trace": {
                "trace_id": "0af7651916cd43dd8448eb211c80319c",
                "span_id": "b7ad6b7169203331",
                "op": "navigation",
                "data": { "sentry.origin": "manual", "url": "tauri://localhost/Users/alice" }
            } },
            "spans": [
                {
                    "span_id": "c7ad6b7169203331", "parent_span_id": "b7ad6b7169203331",
                    "trace_id": "0af7651916cd43dd8448eb211c80319c",
                    "op": "ipc", "description": "scan_skills",
                    "start_timestamp": 1_700_000_000.1, "timestamp": 1_700_000_000.2,
                    "data": { "sentry.op": "ipc", "args": "/Users/alice/secret-skill" }
                },
                {
                    "span_id": "d7ad6b7169203331", "parent_span_id": "b7ad6b7169203331",
                    "trace_id": "0af7651916cd43dd8448eb211c80319c",
                    "op": "ui.long-task", "description": "/Users/alice/secret-skill",
                    "start_timestamp": 1_700_000_000.3, "timestamp": 1_700_000_000.4
                },
                {
                    "span_id": "e7ad6b7169203331", "parent_span_id": "b7ad6b7169203331",
                    "trace_id": "0af7651916cd43dd8448eb211c80319c",
                    "op": "ipc", "description": "read /Users/alice",
                    "start_timestamp": 1_700_000_000.5, "timestamp": 1_700_000_000.6
                }
            ],
            "measurements": { "lcp": { "value": 120.5, "unit": "millisecond" }, "/Users/alice": { "value": 1 } },
            "request": { "url": "tauri://localhost/Users/alice" },
            "breadcrumbs": [{ "message": "secret-skill" }]
        });
        let inp = serde_json::json!({
            "span_id": "f7ad6b7169203331",
            "trace_id": "0af7651916cd43dd8448eb211c80319c",
            "op": "ui.interaction.click",
            "description": "body > div.secret-skill > button",
            "start_timestamp": 1_700_000_002.0, "timestamp": 1_700_000_002.1,
            "is_segment": true,
            "data": { "transaction": "view.home", "sentry.exclusive_time": 80, "user_agent.original": "alices-mbp" },
            "measurements": { "inp": { "value": 80, "unit": "millisecond" } }
        });

        let count = telemetry.send(vec![item("transaction", transaction), item("span", inp)]);

        assert_eq!(count, 2);
        let bodies = sent(&recording);
        let body = bodies.first().expect("one envelope was sent");
        for sentinel in SENTINELS {
            assert!(
                !body.contains(sentinel),
                "the sent envelope still contains {sentinel:?}: {body}"
            );
        }
        let items = items_of(body);
        let spans = items[0]["spans"].as_array().expect("spans are kept");
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0]["description"], "scan_skills");
        assert!(spans[1].get("description").is_none());
        assert!(spans[2].get("description").is_none());
        assert_eq!(items[0]["transaction"], "view.home");
        assert_eq!(items[0]["measurements"]["lcp"]["value"], 120.5);
        assert_eq!(items[1]["measurements"]["inp"]["value"], 80);
        assert_eq!(items[1]["data"]["transaction"], "view.home");
    }

    /// guards: a replay, attachment, session, or client report from the JS
    /// SDK passing through the webview path. Only events, transactions, and
    /// spans are allowed.
    #[test]
    fn webview_items_other_than_events_transactions_and_spans_are_dropped() {
        let consent = Consent::new(true);
        let (telemetry, recording) = webview(&consent);
        let payload = serde_json::json!({ "anything": "/Users/alice" });

        let count = telemetry.send(
            [
                "replay_event",
                "replay_recording",
                "attachment",
                "session",
                "client_report",
            ]
            .iter()
            .map(|kind| item(kind, payload.clone()))
            .collect(),
        );

        assert_eq!(count, 0);
        assert!(sent(&recording).is_empty());
    }

    /// guards: a `captureMessage` from the webview sending its free-text
    /// message. An event with no exception is dropped whole.
    #[test]
    fn a_webview_event_without_an_exception_is_dropped() {
        let consent = Consent::new(true);
        let (telemetry, recording) = webview(&consent);

        let count = telemetry.send(vec![item(
            "event",
            serde_json::json!({ "message": "secret-skill", "level": "info" }),
        )]);

        assert_eq!(count, 0);
        assert!(sent(&recording).is_empty());
    }

    /// guards: a render loop that throws on every frame flooding Sentry.
    /// Transactions do not count against the error cap.
    #[test]
    fn webview_error_events_stop_after_the_process_cap() {
        let consent = Consent::new(true);
        let (telemetry, _recording) = webview(&consent);

        let errors: usize = (0..=WEBVIEW_ERROR_CAP)
            .map(|_| telemetry.send(vec![item("event", sensitive_error_event())]))
            .sum();
        let transaction = telemetry.send(vec![item(
            "transaction",
            serde_json::json!({
                "start_timestamp": 1.0, "timestamp": 2.0, "transaction": "view.skills",
                "contexts": { "trace": { "trace_id": "0af7651916cd43dd8448eb211c80319c", "span_id": "b7ad6b7169203331" } }
            }),
        )]);

        assert_eq!(errors, WEBVIEW_ERROR_CAP as usize);
        assert_eq!(transaction, 1);
    }
}
