use std::{fs, path::PathBuf};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::bootstrap::Runtime;

pub type GitHubError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub id: i64,
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    pub id: i64,
    pub login: String,
    pub html_url: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub html_url: String,
    #[serde(default)]
    pub description: Option<String>,
    pub owner: Owner,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub fork: bool,
    #[serde(default)]
    pub is_template: bool,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub open_issues_count: u64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub pushed_at: String,
    #[serde(default)]
    pub permissions: RepositoryPermissions,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepositoryPermissions {
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub maintain: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub triage: bool,
    #[serde(default)]
    pub pull: bool,
}

impl Repository {
    pub fn effective_permission(&self) -> &'static str {
        if self.permissions.admin {
            "admin"
        } else if self.permissions.maintain {
            "maintain"
        } else if self.permissions.push {
            "write"
        } else if self.permissions.triage {
            "triage"
        } else if self.permissions.pull {
            "read"
        } else {
            "unknown"
        }
    }
}

fn credentials_path(runtime: &Runtime) -> Result<PathBuf, GitHubError> {
    Ok(runtime
        .directories
        .get("CONF")
        .ok_or("BOREAL CONF directory is not configured")?
        .join("github-token.json"))
}

pub fn configured(runtime: &Runtime) -> bool {
    load_credentials(runtime).is_ok_and(|credentials| !credentials.token.trim().is_empty())
}

pub fn load_credentials(runtime: &Runtime) -> Result<Credentials, GitHubError> {
    let contents = fs::read_to_string(credentials_path(runtime)?)?;
    let credentials: Credentials = serde_json::from_str(&contents)?;
    if credentials.token.trim().is_empty() {
        return Err("GitHub token is empty".into());
    }
    Ok(credentials)
}

pub fn save_credentials(runtime: &Runtime, token: &str) -> Result<(), GitHubError> {
    if token.trim().is_empty() {
        return Err("Enter a GitHub access token".into());
    }
    let path = credentials_path(runtime)?;
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Credentials {
                token: token.trim().to_string()
            })?
        ),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn api(runtime: &Runtime) -> Result<(Client, String), GitHubError> {
    let credentials = load_credentials(runtime)?;
    let client = Client::builder().user_agent("BOREAL").build()?;
    Ok((client, credentials.token))
}

pub fn account(runtime: &Runtime) -> Result<Account, GitHubError> {
    let credentials = load_credentials(runtime)?;
    account_with_token(&credentials.token)
}

pub fn account_with_token(token: &str) -> Result<Account, GitHubError> {
    let client = Client::builder().user_agent("BOREAL").build()?;
    Ok(client
        .get("https://api.github.com/user")
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()?
        .error_for_status()?
        .json()?)
}

pub fn repositories(runtime: &Runtime) -> Result<Vec<Repository>, GitHubError> {
    let (client, token) = api(runtime)?;
    let mut repositories = Vec::new();
    for page in 1..=100_u32 {
        let batch: Vec<Repository> = client
            .get("https://api.github.com/user/repos")
            .bearer_auth(&token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .query(&[
                ("per_page", "100".to_string()),
                ("page", page.to_string()),
                ("visibility", "all".to_string()),
                (
                    "affiliation",
                    "owner,collaborator,organization_member".to_string(),
                ),
            ])
            .send()?
            .error_for_status()?
            .json()?;
        let complete = batch.len() < 100;
        repositories.extend(batch);
        if complete {
            return Ok(repositories);
        }
    }
    Err("GitHub repository listing exceeded 10,000 repositories".into())
}
