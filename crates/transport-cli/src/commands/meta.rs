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
        &NODES_CREATE,
        &NODES_DELETE,
        &SLOTS_WRITE,
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
        &AUTH_WHOAMI,
        &UI_NAV,
        &UI_RESOLVE,
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
        "agent nodes list --filter 'kind==acme.core.folder' --sort=-path -o json",
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
            description: "Node kind id, e.g. acme.core.folder",
        },
        ArgInfo {
            name: "name",
            required: true,
            type_name: "identifier",
            description: "Child name segment",
        },
    ],
    examples: &["agent nodes create /station acme.core.folder floor1"],
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
    ],
    examples: &[
        "agent slots write /station/counter in 42",
        "agent slots write /station/counter in '\"hello\"'",
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
                "value": {}
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
        description: "Preset name: count_chain or trigger_demo",
    }],
    examples: &["agent seed count_chain", "agent seed trigger_demo"],
    related: &["nodes list"],
    input_schema: || {
        serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["preset"],
            "properties": {
                "preset": { "type": "string", "enum": ["count_chain", "trigger_demo"] }
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
        "agent kinds list --under acme.core.station",
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

// ---- plugins --------------------------------------------------------------

static PLUGINS_LIST: CommandMeta = CommandMeta {
    name: "plugins list",
    summary: "List all loaded plugins.",
    args: &[],
    examples: &["agent plugins list"],
    related: &["plugins get", "plugins reload"],
    input_schema: empty_input,
    output_schema: schema_for_vec::<types::PluginSummary>,
    errors: &[ErrorInfo {
        code: "agent_unreachable",
        exit_code: 2,
    }],
};

static PLUGINS_GET: CommandMeta = CommandMeta {
    name: "plugins get",
    summary: "Get details for a single plugin by id.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "plugin-id",
        description: "Plugin id",
    }],
    examples: &["agent plugins get acme-plugin"],
    related: &["plugins list", "plugins enable", "plugins disable"],
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
    name: "plugins enable",
    summary: "Enable a plugin.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "plugin-id",
        description: "Plugin id",
    }],
    examples: &["agent plugins enable acme-plugin"],
    related: &["plugins list", "plugins disable"],
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
    name: "plugins disable",
    summary: "Disable a plugin.",
    args: &[ArgInfo {
        name: "id",
        required: true,
        type_name: "plugin-id",
        description: "Plugin id",
    }],
    examples: &["agent plugins disable acme-plugin"],
    related: &["plugins list", "plugins enable"],
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
    name: "plugins reload",
    summary: "Trigger a full plugin reload scan.",
    args: &[],
    examples: &["agent plugins reload"],
    related: &["plugins list"],
    input_schema: empty_input,
    output_schema: status_output,
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
                "auth_subject": { "type": "string" }
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
