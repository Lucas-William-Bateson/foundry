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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PushEvent {
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub after: String,
    pub repository: Repository,
    pub pusher: Pusher,
    pub head_commit: Option<HeadCommit>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HeadCommit {
    pub message: String,
    pub author: CommitAuthor,
    pub url: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CommitAuthor {
    pub name: String,
    pub username: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Repository {
    pub id: i64,
    pub name: String,
    pub full_name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub owner: Owner,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Owner {
    pub login: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Pusher {
    pub name: String,
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
