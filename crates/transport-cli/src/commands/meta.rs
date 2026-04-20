//! Command metadata registry — one source of truth for `agent schema`
//! and `--help-json`.
//!
//! Each subcommand's metadata is declared here as a static
//! [`CommandMeta`]. Schema generation and help-json both pull from
//! these declarations — drift is impossible.

use agent_client::types;
use serde::Serialize;
use serde_json::Value;

// ---- registry types -------------------------------------------------------

/// Internal metadata for a CLI subcommand.
pub struct CommandMeta {
    pub name: &'static str,
    pub summary: &'static str,
    pub args: &'static [ArgInfo],
    pub examples: &'static [&'static str],
    pub related: &'static [&'static str],
    pub input_schema: fn() -> Value,
    pub output_schema: fn() -> Value,
    pub errors: &'static [ErrorInfo],
}

pub struct ArgInfo {
    pub name: &'static str,
    pub required: bool,
    pub type_name: &'static str,
    pub description: &'static str,
}

pub struct ErrorInfo {
    pub code: &'static str,
    pub exit_code: i32,
}

// ---- output shapes --------------------------------------------------------

/// `agent schema <cmd>` output.
#[derive(Serialize)]
pub struct SchemaOutput {
    pub command: String,
    pub input: Value,
    pub output: Value,
    pub errors: Vec<SchemaErrorEntry>,
}

#[derive(Serialize)]
pub struct SchemaErrorEntry {
    pub code: String,
    pub exit: i32,
}

/// `agent schema --all -o json` output.
#[derive(Serialize)]
pub struct SchemaAllOutput {
    pub commands: Vec<SchemaOutput>,
}

/// `--help-json` output.
#[derive(Serialize)]
pub struct HelpJsonOutput {
    pub command: String,
    pub summary: String,
    pub args: Vec<HelpJsonArg>,
    pub examples: Vec<String>,
    pub related_commands: Vec<String>,
    pub output_schema_ref: String,
}

#[derive(Serialize)]
pub struct HelpJsonArg {
    pub name: String,
    pub required: bool,
    #[serde(rename = "type")]
    pub type_name: String,
    pub description: String,
}

// ---- conversions ----------------------------------------------------------

impl CommandMeta {
    pub fn to_schema_output(&self) -> SchemaOutput {
        SchemaOutput {
            command: self.name.to_string(),
            input: (self.input_schema)(),
            output: (self.output_schema)(),
            errors: self
                .errors
                .iter()
                .map(|e| SchemaErrorEntry {
                    code: e.code.to_string(),
                    exit: e.exit_code,
                })
                .collect(),
        }
    }

    pub fn to_help_json(&self) -> HelpJsonOutput {
        HelpJsonOutput {
            command: format!("agent {}", self.name),
            summary: self.summary.to_string(),
            args: self
                .args
                .iter()
                .map(|a| HelpJsonArg {
                    name: a.name.to_string(),
                    required: a.required,
                    type_name: a.type_name.to_string(),
                    description: a.description.to_string(),
                })
                .collect(),
            examples: self.examples.iter().map(|e| e.to_string()).collect(),
            related_commands: self.related.iter().map(|r| r.to_string()).collect(),
            output_schema_ref: format!("agent schema {}", self.name),
        }
    }
}

// ---- lookup ---------------------------------------------------------------

pub fn all_commands() -> &'static [&'static CommandMeta] {
    static ALL: &[&CommandMeta] = &[
        &HEALTH,
        &CAPABILITIES,
        &NODES_LIST,
        &NODES_GET,
        &NODES_SCHEMA,
        &NODES_CREATE,
        &NODES_DELETE,
        &SLOTS_WRITE,
        &SLOTS_HISTORY_LIST,
        &SLOTS_HISTORY_RECORD,
        &SLOTS_TELEMETRY_LIST,
        &SLOTS_TELEMETRY_RECORD,
        &CONFIG_SET,
        &LINKS_LIST,
        &LINKS_CREATE,
        &LINKS_REMOVE,
        &LIFECYCLE,
        &SEED,
        &KINDS_LIST,
        &PLUGINS_LIST,
        &PLUGINS_GET,
        &PLUGINS_ENABLE,
        &PLUGINS_DISABLE,
        &PLUGINS_RELOAD,
        &PLUGINS_RUNTIME,
        &PLUGINS_RUNTIME_ALL,
        &AUTH_WHOAMI,
        &UI_NAV,
        &UI_RESOLVE,
        &UI_ACTION,
        &UI_TABLE,
        &UI_RENDER,
        &UI_VOCABULARY,
        &UI_COMPOSE,
        &FLOWS_LIST,
        &FLOWS_GET,
        &FLOWS_CREATE,
        &FLOWS_DELETE,
        &FLOWS_EDIT,
        &FLOWS_UNDO,
        &FLOWS_REDO,
        &FLOWS_REVERT,
        &FLOWS_REVISIONS,
        &FLOWS_DOCUMENT_AT,
        &AI_PROVIDERS,
        &AI_RUN,
        &AI_STREAM,
    ];
    ALL
}

pub fn find_command(name: &str) -> Option<&'static CommandMeta> {
    all_commands().iter().find(|c| c.name == name).copied()
}

// ---- schema helper fns ----------------------------------------------------

fn empty_input() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {}
    })
}

fn status_output() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "required": ["status"],
        "properties": {
            "status": { "type": "string" }
        }
    })
}

fn schema_for_type<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("schema generation is infallible")
}

fn schema_for_vec<T: schemars::JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(Vec<T>)).expect("schema generation is infallible")
}

// ---- per-command metadata -------------------------------------------------

static HEALTH: CommandMeta = CommandMeta {
    name: "health",
    summary: "Check if the agent is reachable.",
    args: &[],
    examples: &["agent health", "agent -u http://10.0.0.5:8080 health"],
    related: &["capabilities"],
    input_schema: empty_input,
    output_schema: status_output,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static CAPABILITIES: CommandMeta = CommandMeta {
    name: "capabilities",
    summary: "Show the agent's capability manifest.",
    args: &[],
    examples: &["agent capabilities", "agent capabilities -o json"],
    related: &["health"],
    input_schema: empty_input,
    output_schema: schema_for_type::<types::CapabilityManifest>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static NODES_LIST: CommandMeta = CommandMeta {
    name: "nodes list",
    summary: "List nodes in the graph with optional filter, sort, and paging.",
    args: &[
        ArgInfo {
            name: "--filter",
            required: false,
            type_name: "query-filter",
            description: "Filter expression. Supported fields: id, kind, path, parent_id, parent_path, lifecycle. Operators: == != =prefix=. E.g. `parent_path==/station` for direct children, `path=prefix=/station/` for the whole subtree.",
        },
        ArgInfo {
            name: "--sort",
            required: false,
            type_name: "query-sort",
            description: "Sort expression, e.g. path,-kind",
        },
        ArgInfo {
            name: "--page",
            required: false,
            type_name: "u64",
            description: "1-based page number",
        },
        ArgInfo {
            name: "--size",
            required: false,
            type_name: "u64",
            description: "Page size",
        },
    ],
    examples: &[
        "agent nodes list",
        "agent nodes list --filter 'kind==sys.core.folder' --sort=-path -o json",
        "agent nodes list --filter 'path=prefix=/demo' --page 2 --size 50",
        // Direct children only — for tree-view expansion. No subtree walk.
        "agent nodes list --filter 'parent_path==/station' --sort path",
    ],
    related: &["nodes get", "nodes create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "filter": { "type": "string" },
                "sort": { "type": "string" },
                "page": { "type": "integer", "minimum": 1 },
                "size": { "type": "integer", "minimum": 1 }
            }
        })
    },
    output_schema: schema_for_type::<types::NodeListResponse>,
    errors: &[
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
    ],
};

static NODES_GET: CommandMeta = CommandMeta {
    name: "nodes get",
    summary: "Get a single node by path.",
    args: &[ArgInfo {
        name: "path",
        required: true,
        type_name: "node-path",
        description: "Node path, e.g. /station/floor1",
    }],
    examples: &["agent nodes get /station/floor1"],
    related: &["nodes list", "nodes create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "format": "node-path" }
            }
        })
    },
    output_schema: schema_for_type::<types::NodeSnapshot>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static NODES_SCHEMA: CommandMeta = CommandMeta {
    name: "nodes schema",
    summary: "Show the kind-declared slot schemas for one node — name, role, value kind, writable/internal/emit-on-init flags, and per-slot JSON Schema.",
    args: &[ArgInfo {
        name: "path",
        required: true,
        type_name: "node-path",
        description: "Node path, e.g. /flow-1/heartbeat",
    }],
    examples: &[
        "agent nodes schema /flow-1/heartbeat",
        "agent nodes schema /flow-1/heartbeat --include-internal",
        "agent nodes schema /flow-1/heartbeat -o json | jq '.slots[].name'",
    ],
    related: &["nodes get", "kinds list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "format": "node-path" },
                "include_internal": { "type": "boolean", "default": false }
            }
        })
    },
    output_schema: schema_for_type::<types::NodeSchema>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static NODES_CREATE: CommandMeta = CommandMeta {
    name: "nodes create",
    summary: "Create a child node under a parent path.",
    args: &[
        ArgInfo {
            name: "parent",
            required: true,
            type_name: "node-path",
            description: "Parent path, e.g. /station/floor1",
        },
        ArgInfo {
            name: "kind",
            required: true,
            type_name: "kind-id",
            description: "Node kind id, e.g. sys.core.folder",
        },
        ArgInfo {
            name: "name",
            required: true,
            type_name: "identifier",
            description: "Child name segment",
        },
    ],
    examples: &["agent nodes create /station sys.core.folder floor1"],
    related: &["nodes list", "nodes get"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["parent", "kind", "name"],
            "properties": {
                "parent": { "type": "string", "format": "node-path" },
                "kind":   { "type": "string", "format": "kind-id"   },
                "name":   { "type": "string", "pattern": "^[a-zA-Z_][a-zA-Z0-9_-]*$" }
            }
        })
    },
    output_schema: schema_for_type::<types::CreatedNode>,
    errors: &[
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "kind_not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "placement_refused",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static NODES_DELETE: CommandMeta = CommandMeta {
    name: "nodes delete",
    summary: "Delete a node and its children.",
    args: &[ArgInfo {
        name: "path",
        required: true,
        type_name: "node-path",
        description: "Node path",
    }],
    examples: &["agent nodes delete /station/floor1"],
    related: &["nodes list", "nodes get", "nodes create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "format": "node-path" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SLOTS_WRITE: CommandMeta = CommandMeta {
    name: "slots write",
    summary: "Write a value to a node slot.",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/counter",
        },
        ArgInfo {
            name: "slot",
            required: true,
            type_name: "identifier",
            description: "Slot name, e.g. in",
        },
        ArgInfo {
            name: "value",
            required: true,
            type_name: "json",
            description: "Value as JSON (e.g. 42, \"hello\", {\"x\":1})",
        },
        ArgInfo {
            name: "--expected-generation",
            required: false,
            type_name: "u64",
            description: "OCC guard: require the slot's current generation to match",
        },
    ],
    examples: &[
        "agent slots write /station/counter in 42",
        "agent slots write /station/counter in '\"hello\"'",
        "agent slots write /station/counter in 42 --expected-generation 7",
    ],
    related: &["nodes get"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "slot", "value"],
            "properties": {
                "path":  { "type": "string", "format": "node-path" },
                "slot":  { "type": "string" },
                "value": {},
                "expected_generation": { "type": "integer", "minimum": 0 }
            }
        })
    },
    output_schema: schema_for_type::<types::WriteSlotResponse>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "generation_mismatch",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SLOTS_HISTORY_LIST: CommandMeta = CommandMeta {
    name: "slots history list",
    summary: "Query structured history records for a slot (String / Json / Binary).",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/sensor",
        },
        ArgInfo {
            name: "slot",
            required: true,
            type_name: "identifier",
            description: "Slot name, e.g. label",
        },
    ],
    examples: &[
        "agent slots history list /station/sensor label",
        "agent slots history list /station/sensor label --from 1700000000000 --to 1700003600000",
        "agent slots history list /station/sensor label --limit 50",
    ],
    related: &["slots history record", "slots telemetry list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "slot"],
            "properties": {
                "path":  { "type": "string", "format": "node-path" },
                "slot":  { "type": "string" },
                "from":  { "type": "integer", "description": "Start Unix ms" },
                "to":    { "type": "integer", "description": "End Unix ms" },
                "limit": { "type": "integer", "minimum": 1 }
            }
        })
    },
    output_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["id", "node_id", "slot_name", "slot_kind", "ts_ms", "byte_size", "ntp_synced"],
                        "properties": {
                            "id":        { "type": "integer" },
                            "node_id":   { "type": "string" },
                            "slot_name": { "type": "string" },
                            "slot_kind": { "type": "string", "enum": ["string", "json", "binary"] },
                            "ts_ms":     { "type": "integer" },
                            "value":     {},
                            "byte_size": { "type": "integer" },
                            "ntp_synced":{ "type": "boolean" }
                        }
                    }
                }
            }
        })
    },
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "service_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SLOTS_HISTORY_RECORD: CommandMeta = CommandMeta {
    name: "slots history record",
    summary: "Record the slot's current value on-demand (String / Json slots).",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/sensor",
        },
        ArgInfo {
            name: "slot",
            required: true,
            type_name: "identifier",
            description: "Slot name, e.g. label",
        },
    ],
    examples: &["agent slots history record /station/sensor label"],
    related: &["slots history list", "slots telemetry record"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "slot"],
            "properties": {
                "path": { "type": "string", "format": "node-path" },
                "slot": { "type": "string" }
            }
        })
    },
    output_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["recorded", "kind"],
            "properties": {
                "recorded": { "type": "boolean" },
                "kind":     { "type": "string" }
            }
        })
    },
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable",
            exit_code: 1,
        },
        ErrorInfo {
            code: "service_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SLOTS_TELEMETRY_LIST: CommandMeta = CommandMeta {
    name: "slots telemetry list",
    summary: "Query scalar telemetry records for a slot (Bool / Number).",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/sensor",
        },
        ArgInfo {
            name: "slot",
            required: true,
            type_name: "identifier",
            description: "Slot name, e.g. temperature",
        },
    ],
    examples: &[
        "agent slots telemetry list /station/sensor temperature",
        "agent slots telemetry list /station/sensor temperature --from 1700000000000",
        "agent slots telemetry list /station/sensor enabled --limit 100",
    ],
    related: &["slots telemetry record", "slots history list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "slot"],
            "properties": {
                "path":  { "type": "string", "format": "node-path" },
                "slot":  { "type": "string" },
                "from":  { "type": "integer", "description": "Start Unix ms" },
                "to":    { "type": "integer", "description": "End Unix ms" },
                "limit": { "type": "integer", "minimum": 1 }
            }
        })
    },
    output_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["node_id", "slot_name", "ts_ms", "value", "ntp_synced"],
                        "properties": {
                            "node_id":    { "type": "string" },
                            "slot_name":  { "type": "string" },
                            "ts_ms":      { "type": "integer" },
                            "value":      {},
                            "ntp_synced": { "type": "boolean" }
                        }
                    }
                }
            }
        })
    },
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "service_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SLOTS_TELEMETRY_RECORD: CommandMeta = CommandMeta {
    name: "slots telemetry record",
    summary: "Record the slot's current value on-demand (Bool / Number slots).",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/sensor",
        },
        ArgInfo {
            name: "slot",
            required: true,
            type_name: "identifier",
            description: "Slot name, e.g. temperature",
        },
    ],
    examples: &["agent slots telemetry record /station/sensor temperature"],
    related: &["slots telemetry list", "slots history record"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "slot"],
            "properties": {
                "path": { "type": "string", "format": "node-path" },
                "slot": { "type": "string" }
            }
        })
    },
    output_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["recorded", "kind"],
            "properties": {
                "recorded": { "type": "boolean" },
                "kind":     { "type": "string" }
            }
        })
    },
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable",
            exit_code: 1,
        },
        ErrorInfo {
            code: "service_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static CONFIG_SET: CommandMeta = CommandMeta {
    name: "config set",
    summary: "Set a node's config blob and re-fire on_init.",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path",
        },
        ArgInfo {
            name: "config",
            required: true,
            type_name: "json",
            description: "Config as JSON string, e.g. {\"step\":5}",
        },
    ],
    examples: &["agent config set /station/counter '{\"step\":5}'"],
    related: &["nodes get"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "config"],
            "properties": {
                "path":   { "type": "string", "format": "node-path" },
                "config": { "type": "object" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static LINKS_LIST: CommandMeta = CommandMeta {
    name: "links list",
    summary: "List all links in the graph.",
    args: &[],
    examples: &["agent links list", "agent links list -o json"],
    related: &["links create", "links remove"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::Link>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static LINKS_CREATE: CommandMeta = CommandMeta {
    name: "links create",
    summary: "Create a link between two slot endpoints.",
    args: &[
        ArgInfo {
            name: "source-path",
            required: true,
            type_name: "node-path",
            description: "Source node path",
        },
        ArgInfo {
            name: "source-slot",
            required: true,
            type_name: "identifier",
            description: "Source slot name",
        },
        ArgInfo {
            name: "target-path",
            required: true,
            type_name: "node-path",
            description: "Target node path",
        },
        ArgInfo {
            name: "target-slot",
            required: true,
            type_name: "identifier",
            description: "Target slot name",
        },
    ],
    examples: &[
        "agent links create --source-path /a --source-slot out --target-path /b --target-slot in",
    ],
    related: &["links list", "links remove"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["source-path", "source-slot", "target-path", "target-slot"],
            "properties": {
                "source-path": { "type": "string", "format": "node-path" },
                "source-slot": { "type": "string" },
                "target-path": { "type": "string", "format": "node-path" },
                "target-slot": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::CreatedLink>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static LINKS_REMOVE: CommandMeta = CommandMeta {
    name: "links remove",
    summary: "Remove a link by ID.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "uuid",
        description: "Link UUID",
    }],
    examples: &["agent links remove <uuid>"],
    related: &["links list", "links create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string", "format": "uuid" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static LIFECYCLE: CommandMeta = CommandMeta {
    name: "lifecycle",
    summary: "Transition a node's lifecycle state.",
    args: &[
        ArgInfo {
            name: "path",
            required: true,
            type_name: "node-path",
            description: "Node path, e.g. /station/floor1/ahu-5",
        },
        ArgInfo {
            name: "to",
            required: true,
            type_name: "lifecycle-state",
            description: "Target state (e.g. active, disabled)",
        },
    ],
    examples: &[
        "agent lifecycle /station/counter active",
        "agent lifecycle /station/counter disabled",
    ],
    related: &["nodes get"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["path", "to"],
            "properties": {
                "path": { "type": "string", "format": "node-path" },
                "to":   { "type": "string", "enum": ["created", "active", "disabled", "fault", "removing"] }
            }
        })
    },
    output_schema: schema_for_type::<types::LifecycleResponse>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "illegal_transition",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_path",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static SEED: CommandMeta = CommandMeta {
    name: "seed",
    summary: "Seed a preset graph for testing.",
    args: &[ArgInfo {
        name: "preset",
        required: true,
        type_name: "identifier",
        description: "Preset name: count_chain, trigger_demo, or ui_demo",
    }],
    examples: &[
        "agent seed count_chain",
        "agent seed trigger_demo",
        "agent seed ui_demo",
    ],
    related: &["nodes list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["preset"],
            "properties": {
                "preset": { "type": "string", "enum": ["count_chain", "trigger_demo", "ui_demo"] }
            }
        })
    },
    output_schema: schema_for_type::<types::SeedResult>,
    errors: &[
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

// ---- kinds ----------------------------------------------------------------

static KINDS_LIST: CommandMeta = CommandMeta {
    name: "kinds list",
    summary: "List all registered kinds.",
    args: &[],
    examples: &[
        "agent kinds list",
        "agent kinds list --facet isContainer",
        "agent kinds list --under sys.core.station",
    ],
    related: &["nodes create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "facet": { "type": "string" },
                "under": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_vec::<types::KindDto>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

// ---- blocks --------------------------------------------------------------

static PLUGINS_LIST: CommandMeta = CommandMeta {
    name: "blocks list",
    summary: "List all loaded blocks.",
    args: &[],
    examples: &["agent blocks list"],
    related: &["blocks get", "blocks reload"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::PluginSummary>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static PLUGINS_GET: CommandMeta = CommandMeta {
    name: "blocks get",
    summary: "Get details for a single block by id.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "block-id",
        description: "Block id",
    }],
    examples: &["agent blocks get acme-block"],
    related: &["blocks list", "blocks enable", "blocks disable"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::PluginSummary>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static PLUGINS_ENABLE: CommandMeta = CommandMeta {
    name: "blocks enable",
    summary: "Enable a block.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "block-id",
        description: "Block id",
    }],
    examples: &["agent blocks enable acme-block"],
    related: &["blocks list", "blocks disable"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static PLUGINS_DISABLE: CommandMeta = CommandMeta {
    name: "blocks disable",
    summary: "Disable a block.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "block-id",
        description: "Block id",
    }],
    examples: &["agent blocks disable acme-block"],
    related: &["blocks list", "blocks enable"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static PLUGINS_RELOAD: CommandMeta = CommandMeta {
    name: "blocks reload",
    summary: "Trigger a full block reload scan.",
    args: &[],
    examples: &["agent blocks reload"],
    related: &["blocks list"],
    input_schema: empty_input,
    output_schema: status_output,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static PLUGINS_RUNTIME: CommandMeta = CommandMeta {
    name: "blocks runtime",
    summary: "Get the process-runtime state for one block.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "block-id",
        description: "Block id",
    }],
    examples: &["agent blocks runtime com.acme.bacnet"],
    related: &["blocks runtime-all", "blocks list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::PluginRuntimeState>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static PLUGINS_RUNTIME_ALL: CommandMeta = CommandMeta {
    name: "blocks runtime-all",
    summary: "Snapshot runtime state of every process block.",
    args: &[],
    examples: &["agent blocks runtime-all"],
    related: &["blocks runtime", "blocks list"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::PluginRuntimeState>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

// ---- auth -----------------------------------------------------------------

// ---- ui (dashboard) -------------------------------------------------------

static UI_NAV: CommandMeta = CommandMeta {
    name: "ui nav",
    summary: "Fetch the ui.nav subtree rooted at a node id.",
    args: &[ArgInfo {
        name: "--root",
        required: true,
        type_name: "uuid",
        description: "Root nav node id",
    }],
    examples: &[
        "agent ui nav --root 11111111-2222-3333-4444-555555555555",
        "agent ui nav --root <id> -o json",
    ],
    related: &["ui resolve"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["root"],
            "properties": {
                "root": { "type": "string", "format": "uuid" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiNavNode>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static UI_RESOLVE: CommandMeta = CommandMeta {
    name: "ui resolve",
    summary: "Resolve a ui.page into a render tree and cache metadata.",
    args: &[
        ArgInfo {
            name: "--page",
            required: true,
            type_name: "uuid",
            description: "Page node id",
        },
        ArgInfo {
            name: "--stack",
            required: false,
            type_name: "uuid-list",
            description: "Comma-separated ui.nav ids forming the context stack",
        },
        ArgInfo {
            name: "--page-state",
            required: false,
            type_name: "json",
            description: "Page-local state as a JSON object",
        },
        ArgInfo {
            name: "--dry-run",
            required: false,
            type_name: "bool",
            description: "Validate only; return structured errors",
        },
        ArgInfo {
            name: "--auth-subject",
            required: false,
            type_name: "string",
            description: "Opaque subject id; threads into cache key and audit",
        },
    ],
    examples: &[
        "agent ui resolve --page <page-id>",
        "agent ui resolve --page <page-id> --stack <nav1>,<nav2> --page-state '{\"row\":3}'",
        "agent ui resolve --page <page-id> --dry-run -o json",
    ],
    related: &["ui nav"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["page"],
            "properties": {
                "page":         { "type": "string", "format": "uuid" },
                "stack":        { "type": "string" },
                "page_state":   { "type": "object" },
                "dry_run":      { "type": "boolean" },
                "auth_subject": { "type": "string" },
                "layout":       { "description": "Optional candidate layout to validate in dry_run mode" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiResolveResponse>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "payload_too_large",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable_entity",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static UI_ACTION: CommandMeta = CommandMeta {
    name: "ui action",
    summary: "Dispatch a named action handler and receive a response.",
    args: &[
        ArgInfo {
            name: "--handler",
            required: true,
            type_name: "string",
            description: "Fully-qualified handler name (e.g. com.acme.hello.greet)",
        },
        ArgInfo {
            name: "--args",
            required: false,
            type_name: "json",
            description: "Handler arguments as a JSON value",
        },
        ArgInfo {
            name: "--target",
            required: false,
            type_name: "string",
            description: "Originating component id",
        },
        ArgInfo {
            name: "--stack",
            required: false,
            type_name: "uuid-list",
            description: "Comma-separated ui.nav ids forming the context stack",
        },
        ArgInfo {
            name: "--page-state",
            required: false,
            type_name: "json",
            description: "Page-local state as a JSON object",
        },
        ArgInfo {
            name: "--auth-subject",
            required: false,
            type_name: "string",
            description: "Opaque subject id threaded into audit events",
        },
    ],
    examples: &[
        "agent ui action --handler com.acme.hello.greet",
        "agent ui action --handler com.acme.hello.greet --args '{\"name\":\"World\"}'",
    ],
    related: &["ui resolve", "ui nav"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["handler"],
            "properties": {
                "handler":      { "type": "string" },
                "args":         {},
                "target":       { "type": "string" },
                "stack":        { "type": "string" },
                "page_state":   { "type": "object" },
                "auth_subject": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiActionResponse>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable_entity",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static UI_TABLE: CommandMeta = CommandMeta {
    name: "ui table",
    summary: "Fetch a paginated table of nodes matching an RSQL query.",
    args: &[
        ArgInfo {
            name: "--query",
            required: false,
            type_name: "rsql",
            description: "Base RSQL query string",
        },
        ArgInfo {
            name: "--filter",
            required: false,
            type_name: "rsql",
            description: "Additional client-side RSQL filter",
        },
        ArgInfo {
            name: "--sort",
            required: false,
            type_name: "string",
            description: "Sort expression (field asc|desc)",
        },
        ArgInfo {
            name: "--page",
            required: false,
            type_name: "usize",
            description: "1-based page number",
        },
        ArgInfo {
            name: "--size",
            required: false,
            type_name: "usize",
            description: "Page size (max 200)",
        },
        ArgInfo {
            name: "--source-id",
            required: false,
            type_name: "string",
            description: "Optional table component id for audit",
        },
    ],
    examples: &[
        "agent ui table",
        "agent ui table --query 'kind==\"ui.page\"'",
        "agent ui table --size 20 --page 2 -o json",
    ],
    related: &["ui resolve", "ui nav"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {
                "query":     { "type": "string" },
                "filter":    { "type": "string" },
                "sort":      { "type": "string" },
                "page":      { "type": "integer", "minimum": 1 },
                "size":      { "type": "integer", "minimum": 1, "maximum": 200 },
                "source_id": { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiTableResponse>,
    errors: &[
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static UI_RENDER: CommandMeta = CommandMeta {
    name: "ui render",
    summary: "Render a node using its kind's default SDUI view.",
    args: &[
        ArgInfo {
            name: "--target",
            required: true,
            type_name: "uuid",
            description: "Target node id",
        },
        ArgInfo {
            name: "--view",
            required: false,
            type_name: "string",
            description: "View id (defaults to highest-priority view on the kind)",
        },
    ],
    examples: &[
        "agent ui render --target <node-id>",
        "agent ui render --target <node-id> --view settings",
    ],
    related: &["ui resolve", "ui nav"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["target"],
            "properties": {
                "target": { "type": "string", "format": "uuid" },
                "view":   { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiResolveResponse>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable_entity",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static UI_VOCABULARY: CommandMeta = CommandMeta {
    name: "ui vocabulary",
    summary: "Dump the ui_ir::Component JSON Schema.",
    args: &[],
    examples: &["agent ui vocabulary", "agent ui vocabulary -o json"],
    related: &["ui resolve", "ui render"],
    input_schema: empty_input,
    output_schema: schema_for_type::<types::UiVocabulary>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static UI_COMPOSE: CommandMeta = CommandMeta {
    name: "ui compose",
    summary: "Generate or edit a ui.page layout with AI.",
    args: &[
        ArgInfo {
            name: "prompt",
            required: true,
            type_name: "string",
            description: "Natural-language instruction",
        },
        ArgInfo {
            name: "--page",
            required: false,
            type_name: "string",
            description: "Page id or path to use as edit context",
        },
        ArgInfo {
            name: "--context",
            required: false,
            type_name: "string",
            description: "Free-text hints about surrounding graph state",
        },
        ArgInfo {
            name: "--apply",
            required: false,
            type_name: "bool",
            description: "Write the generated layout back to --page with an OCC guard",
        },
        ArgInfo {
            name: "--provider",
            required: false,
            type_name: "string",
            description: "Override the default AI provider (anthropic, openai, claude, codex)",
        },
        ArgInfo {
            name: "--model",
            required: false,
            type_name: "string",
            description: "Override the model (e.g. claude-opus-4-5, gpt-4o)",
        },
    ],
    examples: &[
        "agent ui compose \"heartbeat dashboard for /flow-1/heartbeat\"",
        "agent ui compose \"add a severity filter above the table\" --page /dashboards/alarms",
        "agent ui compose \"refresh the dashboard\" --page /pages/overview --apply",
        "agent ui compose \"status board\" --provider openai --model gpt-4o",
    ],
    related: &["ui resolve", "ui vocabulary", "ai providers", "slots write"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt":         { "type": "string" },
                "page":           { "type": "string" },
                "context":        { "type": "string" },
                "apply":          { "type": "boolean" },
                "provider":       { "type": "string" },
                "model":          { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::UiComposeResponse>,
    errors: &[
        ErrorInfo {
            code: "compose_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "upstream_error",
            exit_code: 2,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "generation_mismatch",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static AUTH_WHOAMI: CommandMeta = CommandMeta {
    name: "auth whoami",
    summary: "Show the resolved auth context — actor, tenant, scopes, provider.",
    args: &[],
    examples: &[
        "agent auth whoami",
        "AGENT_TOKEN=my-token agent auth whoami",
    ],
    related: &["capabilities"],
    input_schema: empty_input,
    output_schema: schema_for_type::<types::WhoAmIDto>,
    errors: &[
        ErrorInfo {
            code: "unauthorized",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

// ---- flows ----------------------------------------------------------------

static FLOWS_LIST: CommandMeta = CommandMeta {
    name: "flows list",
    summary: "List all flows, newest-first by head_seq.",
    args: &[
        ArgInfo {
            name: "--limit",
            required: false,
            type_name: "u32",
            description: "Maximum number of flows to return (default: 50)",
        },
        ArgInfo {
            name: "--offset",
            required: false,
            type_name: "u32",
            description: "Skip this many flows (default: 0)",
        },
    ],
    examples: &["agent flows list", "agent flows list --limit 10"],
    related: &["flows get", "flows create"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::FlowDto>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static FLOWS_GET: CommandMeta = CommandMeta {
    name: "flows get",
    summary: "Fetch a single flow by id.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "string",
        description: "Flow id (UUID)",
    }],
    examples: &["agent flows get <id>"],
    related: &["flows list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": { "id": { "type": "string", "format": "uuid" } }
        })
    },
    output_schema: schema_for_type::<types::FlowDto>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_CREATE: CommandMeta = CommandMeta {
    name: "flows create",
    summary: "Create a new flow with an optional initial document.",
    args: &[
        ArgInfo {
            name: "name",
            required: true,
            type_name: "string",
            description: "Human-readable name",
        },
        ArgInfo {
            name: "--document",
            required: false,
            type_name: "string",
            description: "Initial document as JSON (default: {})",
        },
        ArgInfo {
            name: "--author",
            required: false,
            type_name: "string",
            description: "Author tag (default: cli)",
        },
    ],
    examples: &[
        "agent flows create my-flow",
        "agent flows create my-flow --document '{\"nodes\":[]}' --author alice",
    ],
    related: &["flows list", "flows edit"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["name"],
            "properties": {
                "name":     { "type": "string" },
                "document": { "type": "object" },
                "author":   { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::FlowDto>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static FLOWS_DELETE: CommandMeta = CommandMeta {
    name: "flows delete",
    summary: "Delete a flow and its entire revision history.",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--expected-head",
            required: false,
            type_name: "string",
            description: "Expected head revision id (OCC guard; omit to bypass)",
        },
    ],
    examples: &[
        "agent flows delete <id>",
        "agent flows delete <id> --expected-head <rev-id>",
    ],
    related: &["flows create"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id":            { "type": "string", "format": "uuid" },
                "expected_head": { "type": "string", "format": "uuid" }
            }
        })
    },
    output_schema: status_output,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_EDIT: CommandMeta = CommandMeta {
    name: "flows edit",
    summary: "Append a forward edit revision to a flow.",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "document",
            required: true,
            type_name: "string",
            description: "New document as a JSON string",
        },
        ArgInfo {
            name: "--summary",
            required: false,
            type_name: "string",
            description: "Short description (default: \"edited via CLI\")",
        },
        ArgInfo {
            name: "--expected-head",
            required: false,
            type_name: "string",
            description: "Expected head revision id (OCC guard)",
        },
        ArgInfo {
            name: "--author",
            required: false,
            type_name: "string",
            description: "Author tag (default: cli)",
        },
    ],
    examples: &["agent flows edit <id> '{\"nodes\":[]}' --expected-head <rev-id>"],
    related: &["flows undo", "flows redo", "flows revert"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id", "document"],
            "properties": {
                "id":            { "type": "string", "format": "uuid" },
                "document":      { "type": "object" },
                "summary":       { "type": "string" },
                "expected_head": { "type": "string", "format": "uuid" },
                "author":        { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::FlowMutationResult>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_UNDO: CommandMeta = CommandMeta {
    name: "flows undo",
    summary: "Undo the last logical edit (appends an undo revision — non-destructive).",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--expected-head",
            required: false,
            type_name: "string",
            description: "Expected head revision id (OCC guard)",
        },
        ArgInfo {
            name: "--author",
            required: false,
            type_name: "string",
            description: "Author tag (default: cli)",
        },
    ],
    examples: &["agent flows undo <id> --expected-head <rev-id>"],
    related: &["flows redo", "flows revert"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id":            { "type": "string", "format": "uuid" },
                "expected_head": { "type": "string", "format": "uuid" },
                "author":        { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::FlowMutationResult>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable_entity",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_REDO: CommandMeta = CommandMeta {
    name: "flows redo",
    summary: "Redo the next undone edit.",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--expected-head",
            required: false,
            type_name: "string",
            description: "Expected head revision id (OCC guard)",
        },
        ArgInfo {
            name: "--expected-target",
            required: false,
            type_name: "string",
            description: "Expected redo-target revision id (stale-cursor guard)",
        },
        ArgInfo {
            name: "--author",
            required: false,
            type_name: "string",
            description: "Author tag (default: cli)",
        },
    ],
    examples: &["agent flows redo <id> --expected-head <rev-id>"],
    related: &["flows undo"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id":              { "type": "string", "format": "uuid" },
                "expected_head":   { "type": "string", "format": "uuid" },
                "expected_target": { "type": "string", "format": "uuid" },
                "author":          { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::FlowMutationResult>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "unprocessable_entity",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_REVERT: CommandMeta = CommandMeta {
    name: "flows revert",
    summary: "Revert a flow to the document state at a specific revision (non-destructive).",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--to",
            required: false,
            type_name: "string",
            description: "Target revision id to revert to",
        },
        ArgInfo {
            name: "--expected-head",
            required: false,
            type_name: "string",
            description: "Expected head revision id (OCC guard)",
        },
        ArgInfo {
            name: "--author",
            required: false,
            type_name: "string",
            description: "Author tag (default: cli)",
        },
    ],
    examples: &["agent flows revert <id> --to <rev-id>"],
    related: &["flows undo", "flows revisions"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id", "to"],
            "properties": {
                "id":            { "type": "string", "format": "uuid" },
                "to":            { "type": "string", "format": "uuid" },
                "expected_head": { "type": "string", "format": "uuid" },
                "author":        { "type": "string" }
            }
        })
    },
    output_schema: schema_for_type::<types::FlowMutationResult>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "conflict",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_REVISIONS: CommandMeta = CommandMeta {
    name: "flows revisions",
    summary: "List revisions for a flow, newest first.",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--limit",
            required: false,
            type_name: "u32",
            description: "Maximum revisions to return (default: 50)",
        },
        ArgInfo {
            name: "--offset",
            required: false,
            type_name: "u32",
            description: "Skip this many revisions (default: 0)",
        },
    ],
    examples: &[
        "agent flows revisions <id>",
        "agent flows revisions <id> --limit 20",
    ],
    related: &["flows get", "flows document-at"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id"],
            "properties": {
                "id":     { "type": "string", "format": "uuid" },
                "limit":  { "type": "integer", "minimum": 1 },
                "offset": { "type": "integer", "minimum": 0 }
            }
        })
    },
    output_schema: schema_for_vec::<types::FlowRevisionDto>,
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static FLOWS_DOCUMENT_AT: CommandMeta = CommandMeta {
    name: "flows document-at",
    summary: "Return the materialised flow document at a specific revision.",
    args: &[
        ArgInfo {
            name: "id",
            required: true,
            type_name: "string",
            description: "Flow id (UUID)",
        },
        ArgInfo {
            name: "--rev-id",
            required: false,
            type_name: "string",
            description: "Revision id",
        },
    ],
    examples: &["agent flows document-at <id> --rev-id <rev-id>"],
    related: &["flows revisions"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["id", "rev_id"],
            "properties": {
                "id":     { "type": "string", "format": "uuid" },
                "rev_id": { "type": "string", "format": "uuid" }
            }
        })
    },
    output_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "description": "Materialised flow document at the requested revision"
        })
    },
    errors: &[
        ErrorInfo {
            code: "not_found",
            exit_code: 1,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

// ---- ai -------------------------------------------------------------------

static AI_PROVIDERS: CommandMeta = CommandMeta {
    name: "ai providers",
    summary: "List registered AI providers and their availability.",
    args: &[],
    examples: &["agent ai providers"],
    related: &["ai run", "ui compose"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::AiProviderStatus>,
    errors: &[
        ErrorInfo {
            code: "ai_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static AI_RUN: CommandMeta = CommandMeta {
    name: "ai run",
    summary: "Run a one-shot prompt through the shared registry.",
    args: &[
        ArgInfo {
            name: "prompt",
            required: true,
            type_name: "string",
            description: "The user prompt",
        },
        ArgInfo {
            name: "--system",
            required: false,
            type_name: "string",
            description: "Optional system / instruction prompt",
        },
        ArgInfo {
            name: "--provider",
            required: false,
            type_name: "string",
            description: "Override the default provider (anthropic, openai, claude, codex)",
        },
        ArgInfo {
            name: "--model",
            required: false,
            type_name: "string",
            description: "Override the model (e.g. claude-opus-4-5, gpt-4o)",
        },
        ArgInfo {
            name: "--max-tokens",
            required: false,
            type_name: "integer",
            description: "Generation cap; runner default when omitted",
        },
    ],
    examples: &[
        "agent ai run \"explain rust lifetimes\"",
        "agent ai run \"summarise this\" --provider openai --model gpt-4o",
    ],
    related: &["ai providers", "ui compose"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt":        { "type": "string" },
                "system":        { "type": "string" },
                "provider":      { "type": "string" },
                "model":         { "type": "string" },
                "max_tokens":    { "type": "integer" }
            }
        })
    },
    output_schema: schema_for_type::<types::AiRunResponse>,
    errors: &[
        ErrorInfo {
            code: "ai_unavailable",
            exit_code: 2,
        },
        ErrorInfo {
            code: "bad_request",
            exit_code: 1,
        },
        ErrorInfo {
            code: "upstream_error",
            exit_code: 2,
        },
        ErrorInfo {
            code: "agent_unreachable",
            exit_code: 2,
        },
    ],
};

static AI_STREAM: CommandMeta = CommandMeta {
    name: "ai stream",
    summary: "Stream a prompt (SSE). Text deltas print live; --output json emits the final result.",
    args: &[
        ArgInfo {
            name: "prompt",
            required: true,
            type_name: "string",
            description: "The user prompt",
        },
        ArgInfo {
            name: "--system",
            required: false,
            type_name: "string",
            description: "Optional system / instruction prompt",
        },
        ArgInfo {
            name: "--provider",
            required: false,
            type_name: "string",
            description: "Override the default provider (anthropic, openai, claude, codex)",
        },
        ArgInfo {
            name: "--model",
            required: false,
            type_name: "string",
            description: "Override the model",
        },
        ArgInfo {
            name: "--max-tokens",
            required: false,
            type_name: "integer",
            description: "Generation cap",
        },
    ],
    examples: &[
        "agent ai stream \"write a haiku about lifetimes\"",
        "agent ai stream \"explain SSE\" --provider openai --output json",
    ],
    related: &["ai run", "ai providers"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt":     { "type": "string" },
                "system":     { "type": "string" },
                "provider":   { "type": "string" },
                "model":      { "type": "string" },
                "max_tokens": { "type": "integer" }
            }
        })
    },
    output_schema: schema_for_type::<types::AiRunResponse>,
    errors: &[
        ErrorInfo { code: "ai_unavailable", exit_code: 2 },
        ErrorInfo { code: "bad_request", exit_code: 1 },
        ErrorInfo { code: "upstream_error", exit_code: 2 },
        ErrorInfo { code: "agent_unreachable", exit_code: 2 },
    ],
};
