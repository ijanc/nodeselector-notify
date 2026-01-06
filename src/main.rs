use std::collections::HashSet;
use std::env;

use futures::TryStreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use kube::{
    Client,
    api::Api,
    runtime::watcher::{self, Event},
};
use serde::Serialize;
use tracing::{error, info, warn};

#[derive(Serialize)]
struct SlackMessage {
    text: String,
}

async fn send_slack_notification(
    webhook_url: &str,
    env_name: &str,
    deployment_name: &str,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let message = SlackMessage {
        text: format!(
            "⚠️ Deployment missing nodeSelector\nenv: {}\nname: {}",
            env_name, deployment_name
        ),
    };

    client.post(webhook_url).json(&message).send().await?;

    info!(
        "Sent Slack notification for deployment: {}",
        deployment_name
    );
    Ok(())
}

async fn send_slack_batch_notification(
    webhook_url: &str,
    env_name: &str,
    deployments: &[(String, String)],
) -> Result<(), reqwest::Error> {
    if deployments.is_empty() {
        return Ok(());
    }

    let client = reqwest::Client::new();
    let deployment_list: Vec<String> = deployments
        .iter()
        .map(|(ns, name)| format!("• {}/{}", ns, name))
        .collect();

    let message = SlackMessage {
        text: format!(
            "⚠️ Found {} deployment(s) missing nodeSelector\nenv: {}\n{}",
            deployments.len(),
            env_name,
            deployment_list.join("\n")
        ),
    };

    client.post(webhook_url).json(&message).send().await?;

    info!(
        "Sent batch Slack notification for {} deployments",
        deployments.len()
    );
    Ok(())
}

fn has_node_selector(deployment: &Deployment) -> bool {
    deployment
        .spec
        .as_ref()
        .and_then(|spec| spec.template.spec.as_ref())
        .and_then(|pod_spec| pod_spec.node_selector.as_ref())
        .map(|ns| !ns.is_empty())
        .unwrap_or(false)
}

fn parse_ignored_namespaces() -> HashSet<String> {
    env::var("IGNORED_NAMESPACES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn should_ignore_namespace(namespace: &str, ignored: &HashSet<String>) -> bool {
    ignored.contains(namespace)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install rustls crypto provider
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    tracing_subscriber::fmt::init();

    let slack_webhook_url =
        env::var("SLACK_WEBHOOK_URL").expect("SLACK_WEBHOOK_URL environment variable must be set");
    let env_name = env::var("ENV").unwrap_or_else(|_| "unknown".to_string());
    let ignored_namespaces = parse_ignored_namespaces();

    info!("Starting nodeselector-notify for env: {}", env_name);
    if !ignored_namespaces.is_empty() {
        info!("Ignoring namespaces: {:?}", ignored_namespaces);
    }

    let client = Client::try_default().await?;
    let deployments: Api<Deployment> = Api::all(client);

    let watcher = watcher::watcher(deployments, watcher::Config::default());

    futures::pin_mut!(watcher);

    let mut init_violations: Vec<(String, String)> = Vec::new();

    while let Some(event) = watcher.try_next().await? {
        match event {
            Event::Apply(deployment) => {
                let name = deployment.metadata.name.as_deref().unwrap_or("unknown");
                let namespace = deployment
                    .metadata
                    .namespace
                    .as_deref()
                    .unwrap_or("default");

                if should_ignore_namespace(namespace, &ignored_namespaces) {
                    continue;
                }

                if !has_node_selector(&deployment) {
                    warn!("Deployment {}/{} has no nodeSelector", namespace, name);
                    if let Err(e) =
                        send_slack_notification(&slack_webhook_url, &env_name, name).await
                    {
                        error!("Failed to send Slack notification: {}", e);
                    }
                } else {
                    info!("Deployment {}/{} has nodeSelector set", namespace, name);
                }
            }
            Event::Delete(deployment) => {
                let name = deployment.metadata.name.as_deref().unwrap_or("unknown");
                info!("Deployment deleted: {}", name);
            }
            Event::Init => {
                info!("Watcher initializing, collecting deployments");
                init_violations.clear();
            }
            Event::InitApply(deployment) => {
                let name = deployment.metadata.name.as_deref().unwrap_or("unknown");
                let namespace = deployment
                    .metadata
                    .namespace
                    .as_deref()
                    .unwrap_or("default");

                if should_ignore_namespace(namespace, &ignored_namespaces) {
                    continue;
                }

                if !has_node_selector(&deployment) {
                    warn!("Deployment {}/{} has no nodeSelector", namespace, name);
                    init_violations.push((namespace.to_string(), name.to_string()));
                }
            }
            Event::InitDone => {
                info!(
                    "Watcher initialization complete, found {} violations",
                    init_violations.len()
                );

                if let Err(e) =
                    send_slack_batch_notification(&slack_webhook_url, &env_name, &init_violations)
                        .await
                {
                    error!("Failed to send batch Slack notification: {}", e);
                }

                init_violations.clear();
            }
        }
    }

    Ok(())
}
