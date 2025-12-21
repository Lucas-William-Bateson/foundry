use hmac::{Hmac, Mac};
use sha2::Sha256;

pub fn verify_github_signature(secret: &str, body: &[u8], header: &str) -> bool {
    let Some(sig_hex) = header.strip_prefix("sha256=") else {
        return false;
    };

    let Ok(sig_bytes) = hex::decode(sig_hex) else {
        return false;
    };

    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };

    mac.update(body);
    mac.verify_slice(&sig_bytes).is_ok()
}

// ============================================
// GitHub Push Event - Comprehensive Data Model
// ============================================

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PushEvent {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub before: String,
    pub after: String,
    pub created: bool,
    pub deleted: bool,
    pub forced: bool,
    pub compare: String,
    pub commits: Vec<Commit>,
    pub head_commit: Option<HeadCommit>,
    pub repository: Repository,
    pub pusher: Pusher,
    pub sender: Option<Sender>,
    pub installation: Option<Installation>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct HeadCommit {
    pub id: String,
    pub tree_id: String,
    pub message: String,
    pub timestamp: String,
    pub url: String,
    pub author: CommitPerson,
    pub committer: CommitPerson,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Commit {
    pub id: String,
    pub tree_id: String,
    pub message: String,
    pub timestamp: String,
    pub url: String,
    pub author: CommitPerson,
    pub committer: CommitPerson,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    #[serde(default)]
    pub distinct: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CommitPerson {
    pub name: String,
    pub email: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Repository {
    pub id: i64,
    pub node_id: Option<String>,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub owner: Owner,
    pub html_url: String,
    pub description: Option<String>,
    pub fork: bool,
    pub url: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub default_branch: String,
    pub language: Option<String>,
    pub topics: Option<Vec<String>>,
    pub visibility: Option<String>,
    pub pushed_at: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Owner {
    pub login: String,
    pub id: i64,
    pub node_id: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(rename = "type")]
    pub owner_type: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Pusher {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Sender {
    pub login: String,
    pub id: i64,
    pub node_id: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(rename = "type")]
    pub sender_type: Option<String>,
    pub html_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Installation {
    pub id: i64,
    pub node_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_valid_signature() {
        let secret = "test-secret";
        let body = b"test body";

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let result = mac.finalize();
        let expected_sig = format!("sha256={}", hex::encode(result.into_bytes()));

        assert!(verify_github_signature(secret, body, &expected_sig));
    }

    #[test]
    fn test_verify_invalid_signature() {
        assert!(!verify_github_signature("secret", b"body", "sha256=invalid"));
        assert!(!verify_github_signature("secret", b"body", "wrong-prefix"));
        assert!(!verify_github_signature(
            "wrong-secret",
            b"body",
            "sha256=abc123"
        ));
    }
}
