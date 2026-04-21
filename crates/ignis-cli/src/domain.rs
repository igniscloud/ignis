use anyhow::Result;
use serde_json::json;

use crate::api::ApiClient;
use crate::cli::DomainCommands;
use crate::config;
use crate::context::ProjectContext;
use crate::output;
use crate::project_domain::effective_project_domain_from_response;

pub async fn handle(
    command: DomainCommands,
    token: Option<String>,
    region: Option<config::Region>,
) -> Result<()> {
    let client = ApiClient::new(config::CliConfig::resolve_required_region(
        token, region, "domain",
    )?);
    match command {
        DomainCommands::List { project } => {
            output::success(client.project_domains(&project).await?)
        }
        DomainCommands::Create { project, label } => {
            let response = client
                .create_project_custom_subdomain(&project, &label)
                .await?;
            sync_local_project_domain(&client, &project, response).await
        }
        DomainCommands::Delete { project, label } => {
            let response = client
                .delete_project_custom_subdomain(&project, &label)
                .await?;
            sync_local_project_domain(&client, &project, response).await
        }
    }
}

async fn sync_local_project_domain(
    client: &ApiClient,
    project_ref: &str,
    remote_response: serde_json::Value,
) -> Result<()> {
    let domains = client.project_domains(project_ref).await?;
    let current_domain = effective_project_domain_from_response(&domains)?;
    let local_update = match ProjectContext::load_optional()? {
        Some(context) if context.matches_project_ref(project_ref) => {
            context.set_project_domain(&current_domain)?;
            Some(json!({
                "project_manifest_path": context.manifest_path(),
                "project_domain": current_domain,
            }))
        }
        _ => None,
    };
    output::success(json!({
        "remote": remote_response,
        "current_domain": current_domain,
        "local": {
            "updated": local_update.is_some(),
            "manifest": local_update,
        }
    }))
}
