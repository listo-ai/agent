//! `agent auth …` — identity introspection + first-boot setup
//! commands. Thin wrapper around `agent_client::Auth`; every
//! operation is a single call + output format, so this module stays
//! under the 50-line-function ceiling.

use agent_client::{AgentClient, EnrollRequest, SetupRequest};
use anyhow::Result;
use clap::{Args, Subcommand};

use crate::output::{self, OutputFormat};

#[derive(Debug, Subcommand)]
pub enum AuthCmd {
    /// Show the resolved auth context for this call — actor, tenant,
    /// scopes, provider.
    Whoami,
    /// Run first-boot setup. Mode must match how the agent was
    /// started — the agent's `/agent/setup.mode` slot is the source
    /// of truth; the CLI only forwards the user's declared mode so
    /// the server can sanity-check.
    Setup(SetupArgs),
    /// Enroll an edge agent with a cloud controller. Phase A returns
    /// `501`; Phase B lands the Zitadel provider + cloud-side
    /// endpoint.
    Enroll(EnrollArgs),
}

#[derive(Debug, Args)]
pub struct SetupArgs {
    /// `cloud`, `edge`, or `standalone`. Must match the agent's role.
    #[arg(long)]
    pub mode: String,
    /// Required for `--mode cloud`.
    #[arg(long)]
    pub org_name: Option<String>,
    /// Required for `--mode cloud`.
    #[arg(long)]
    pub admin_email: Option<String>,
    /// Accepted in Phase A but ignored. Phase B begins verifying.
    #[arg(long)]
    pub admin_password: Option<String>,
}

#[derive(Debug, Args)]
pub struct EnrollArgs {
    #[arg(long)]
    pub cloud_url: String,
    #[arg(long)]
    pub enrollment_token: String,
}

impl AuthCmd {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Whoami => "auth whoami",
            Self::Setup(_) => "auth setup",
            Self::Enroll(_) => "auth enroll",
        }
    }
}

pub async fn run(client: &AgentClient, fmt: OutputFormat, cmd: &AuthCmd) -> Result<()> {
    match cmd {
        AuthCmd::Whoami => {
            let who = client.auth().whoami().await?;
            output::ok(fmt, &who)?;
        }
        AuthCmd::Setup(args) => {
            let req = build_setup_request(args)?;
            let resp = client.auth().setup(&req).await?;
            output::ok(fmt, &resp)?;
        }
        AuthCmd::Enroll(args) => {
            let req = EnrollRequest {
                cloud_url: args.cloud_url.clone(),
                enrollment_token: args.enrollment_token.clone(),
            };
            let resp = client.auth().enroll(&req).await?;
            output::ok(fmt, &resp)?;
        }
    }
    Ok(())
}

fn build_setup_request(args: &SetupArgs) -> Result<SetupRequest> {
    match args.mode.as_str() {
        "cloud" => {
            let org_name = args
                .org_name
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--org-name is required for --mode cloud"))?;
            let admin_email = args
                .admin_email
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--admin-email is required for --mode cloud"))?;
            Ok(SetupRequest::Cloud {
                org_name,
                admin_email,
                admin_password: args.admin_password.clone(),
            })
        }
        "edge" => Ok(SetupRequest::Edge {}),
        "standalone" => Ok(SetupRequest::Standalone {}),
        other => Err(anyhow::anyhow!(
            "unknown --mode `{other}`; expected one of cloud / edge / standalone"
        )),
    }
}
