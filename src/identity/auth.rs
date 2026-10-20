use axum::http::{header, HeaderMap};
use chrono::{Duration, Utc};
use ring::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::num::NonZeroU32;

const ITERATIONS: u32 = 310_000;

#[derive(Clone, Debug, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub email: String,
    pub bio: String,
    pub avatar_key: Option<String>,
    pub email_verified_at: Option<String>,
    pub contributor_verified_at: Option<String>,
    pub is_admin: i64,
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("secure random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub fn password_hash(password: &str) -> String {
    let mut salt = [0u8; 16];
    SystemRandom::new()
        .fill(&mut salt)
        .expect("secure random source");
    let mut out = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(ITERATIONS).unwrap(),
        &salt,
        password.as_bytes(),
        &mut out,
    );
    format!("{ITERATIONS}:{}:{}", bytes_hex(&salt), bytes_hex(&out))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split(':').collect();
    if parts.len() != 3 {
        return false;
    }
    let Some(iterations) = parts[0].parse::<u32>().ok().and_then(NonZeroU32::new) else {
        return false;
    };
    let (Some(salt), Some(hash)) = (parse_hex(parts[1]), parse_hex(parts[2])) else {
        return false;
    };
    pbkdf2::verify(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &hash,
    )
    .is_ok()
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn cookie(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_owned())
}

pub async fn session(pool: &SqlitePool, headers: &HeaderMap) -> Option<(User, String)> {
    let token = cookie(headers, "ot_session")?;
    if token.len() != 64 {
        return None;
    }
    sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>, Option<String>, String, i64, String)>(
        "SELECT u.id,u.username,u.email,u.bio,u.avatar_key,u.email_verified_at,u.contributor_verified_at,u.created_at,u.is_admin,s.csrf_token FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.token_hash=? AND s.expires_at>datetime('now')")
        .bind(token_hash(&token)).fetch_optional(pool).await.ok().flatten()
        .map(|(id,username,email,bio,avatar_key,email_verified_at,contributor_verified_at,_,is_admin,csrf)|
            (User { id, username, email, bio, avatar_key, email_verified_at, contributor_verified_at, is_admin }, csrf))
}

pub async fn create_session(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(String, String), sqlx::Error> {
    let token = random_token();
    let csrf = random_token();
    let expires = (Utc::now() + Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO sessions(token_hash,user_id,csrf_token,expires_at) VALUES (?,?,?,?)")
        .bind(token_hash(&token))
        .bind(user_id)
        .bind(&csrf)
        .bind(expires)
        .execute(pool)
        .await?;
    Ok((token, csrf))
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "ot_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn csrf_cookie(token: &str, secure: bool) -> String {
    format!(
        "ot_csrf={token}; Path=/; SameSite=Lax; Max-Age=2592000{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn check_csrf(headers: &HeaderMap, submitted: &str, expected: &str, base_url: &str) -> bool {
    if submitted.is_empty() || submitted != expected {
        return false;
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        origin == base_url.trim_end_matches('/')
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passwords_are_salted_and_verified() {
        let a = password_hash("a long test password");
        let b = password_hash("a long test password");
        assert_ne!(a, b);
        assert!(verify_password("a long test password", &a));
        assert!(!verify_password("other", &a));
    }
}
