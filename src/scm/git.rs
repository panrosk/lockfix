use git2::{build::CheckoutBuilder, Cred, RemoteCallbacks, Repository, ResetType, Signature};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("failed to open repository at '{path}': {source}")]
    Open {
        path: String,
        #[source]
        source: git2::Error,
    },

    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

pub struct Git {
    repo: Repository,
    author_name: String,
    author_email: String,
    token: Option<String>,
}

impl Git {
    pub fn open(
        project_path: &Path,
        author_name: &str,
        author_email: &str,
        token: Option<&str>,
    ) -> Result<Self, GitError> {
        let repo = Repository::open(project_path).map_err(|e| GitError::Open {
            path: project_path.display().to_string(),
            source: e,
        })?;

        Ok(Self {
            repo,
            author_name: author_name.to_string(),
            author_email: author_email.to_string(),
            token: token.map(|s| s.to_string()),
        })
    }

    pub fn fetch(&self, branch: &str) -> Result<(), GitError> {
        let mut remote = self.repo.find_remote("origin")?;

        let mut callbacks = RemoteCallbacks::new();
        let token = self.token.clone();
        callbacks.credentials(move |_url, username, _allowed| {
            if let Some(ref t) = token {
                Cred::userpass_plaintext(&"oauth2", t)
            } else {
                Cred::ssh_key_from_agent(username.unwrap_or("git"))
            }
        });

        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote.fetch(&[branch], Some(&mut fetch_opts), None)?;

        Ok(())
    }

    /// Checkout a branch and reset --hard to origin/<branch>.
    /// Origin is the source of truth — any local divergence is discarded.
    pub fn checkout_and_reset(&self, branch: &str) -> Result<(), GitError> {
        // find origin/<branch> reference
        let origin_ref = format!("refs/remotes/origin/{branch}");
        let origin_commit = self.repo.find_reference(&origin_ref)?.peel_to_commit()?;

        // checkout the local branch (create if needed)
        let local_ref = format!("refs/heads/{branch}");
        if self.repo.find_reference(&local_ref).is_err() {
            self.repo.branch(branch, &origin_commit, false)?;
        }

        self.repo.set_head(&local_ref)?;
        self.repo
            .checkout_head(Some(CheckoutBuilder::default().force()))?;

        // reset --hard to origin/<branch>
        self.repo
            .reset(origin_commit.as_object(), ResetType::Hard, None)?;

        Ok(())
    }

    /// Create a new branch from the current HEAD and check it out.
    pub fn create_and_checkout_branch(&self, branch_name: &str) -> Result<(), GitError> {
        let head_commit = self.repo.head()?.peel_to_commit()?;
        self.repo.branch(branch_name, &head_commit, true)?; // force = overwrite if exists

        let branch_ref = format!("refs/heads/{branch_name}");
        self.repo.set_head(&branch_ref)?;
        self.repo
            .checkout_head(Some(CheckoutBuilder::default().force()))?;

        Ok(())
    }

    /// Create a branch from the current HEAD, or if it already exists reset it hard to current HEAD.
    /// This ensures the fix branch is always a clean slate on top of the target branch.
    pub fn create_or_reset_branch(&self, branch_name: &str) -> Result<(), GitError> {
        let head_commit = self.repo.head()?.peel_to_commit()?;
        let branch_ref = format!("refs/heads/{branch_name}");

        if let Ok(mut reference) = self.repo.find_reference(&branch_ref) {
            // Branch already exists — reset its tip to the current HEAD (target branch tip)
            reference.set_target(head_commit.id(), "reset to target branch")?;
        } else {
            self.repo.branch(branch_name, &head_commit, false)?;
        }

        self.repo.set_head(&branch_ref)?;
        self.repo
            .checkout_head(Some(CheckoutBuilder::default().force()))?;

        Ok(())
    }

    /// Stage all changes and create a commit.
    pub fn stage_and_commit(&self, message: &str) -> Result<(), GitError> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        let sig = Signature::now(&self.author_name, &self.author_email)?;
        let parent = self.repo.head()?.peel_to_commit()?;

        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])?;

        Ok(())
    }

    pub fn push(&self, branch_name: &str) -> Result<(), GitError> {
        let mut remote = self.repo.find_remote("origin")?;

        let mut callbacks = RemoteCallbacks::new();
        let token = self.token.clone();
        callbacks.credentials(move |_url, username, _allowed| {
            if let Some(ref t) = token {
                Cred::userpass_plaintext(&"oauth2", t)
            } else {
                Cred::ssh_key_from_agent(username.unwrap_or("git"))
            }
        });

        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        let refspec = format!("refs/heads/{branch_name}:refs/heads/{branch_name}");
        remote.push(&[&refspec], Some(&mut push_opts))?;

        Ok(())
    }

    pub fn get_remote_url(&self) -> Result<String, GitError> {
        let remote = self.repo.find_remote("origin")?;
        let url = remote
            .url()
            .ok_or_else(|| git2::Error::from_str("remote has no URL"))?;
        Ok(url.to_string())
    }
}
