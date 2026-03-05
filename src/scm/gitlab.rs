use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum GitLabError {
    #[error("failed to parse remote URL: {0}")]
    ParseUrl(String),

    #[error("GitLab API request failed: {0}")]
    Api(String),

    #[error("failed to parse GitLab API response: {0}")]
    ParseResponse(String),
}

#[derive(Debug, Clone)]
pub struct GitLabConfig {
    pub base_url: String,
    pub token: String,
    pub target_branch: String,
}

pub struct GitLabClient {
    config: GitLabConfig,
}

#[derive(Serialize)]
struct CreateMRRequest {
    source_branch: String,
    target_branch: String,
    title: String,
    remove_source_branch: bool,
}

#[derive(Deserialize)]
struct CreateMRResponse {
    web_url: String,
}

impl GitLabClient {
    pub fn new(config: GitLabConfig) -> Self {
        Self { config }
    }

    pub fn extract_project_path(remote_url: &str) -> Result<String, GitLabError> {
        let url = remote_url.trim();

        let path: String = if url.starts_with("git@") {
            let ssh_url = url.strip_prefix("git@").unwrap_or(url);
            let parts: Vec<&str> = ssh_url.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(GitLabError::ParseUrl(format!(
                    "invalid SSH URL format: {}",
                    remote_url
                )));
            }
            parts[1].trim_end_matches(".git").to_string()
        } else if url.starts_with("https://") || url.starts_with("http://") {
            let parsed = Url::parse(url)
                .map_err(|e| GitLabError::ParseUrl(format!("failed to parse HTTP URL: {}", e)))?;
            parsed
                .path()
                .trim_start_matches('/')
                .trim_end_matches(".git")
                .to_string()
        } else {
            return Err(GitLabError::ParseUrl(format!(
                "unsupported URL format: {}",
                remote_url
            )));
        };

        Ok(urlencoding::encode(&path))
    }

    pub fn create_merge_request(
        &self,
        project_path: &str,
        source_branch: &str,
        title: &str,
    ) -> Result<String, GitLabError> {
        let url = format!(
            "{}/api/v4/projects/{}/merge_requests",
            self.config.base_url, project_path
        );

        let body = CreateMRRequest {
            source_branch: source_branch.to_string(),
            target_branch: self.config.target_branch.clone(),
            title: title.to_string(),
            remove_source_branch: true,
        };

        let response = ureq::post(&url)
            .set("PRIVATE-TOKEN", &self.config.token)
            .set("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| GitLabError::Api(format!("request failed: {}", e)))?;

        let status = response.status();
        if status >= 400 {
            let body = response.into_string().unwrap_or_default();
            return Err(GitLabError::Api(format!(
                "API returned status {}: {}",
                status, body
            )));
        }

        let mr: CreateMRResponse = response
            .into_json()
            .map_err(|e| GitLabError::ParseResponse(format!("failed to parse response: {}", e)))?;

        Ok(mr.web_url)
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut encoded = String::new();
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    encoded.push(c);
                }
                '/' => encoded.push_str("%2F"),
                _ => {
                    for byte in c.to_string().as_bytes() {
                        encoded.push_str(&format!("%{:02X}", byte));
                    }
                }
            }
        }
        encoded
    }
}
